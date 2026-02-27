use crate::AnalysisResult;
use crate::backend::Backend;
use crate::backend::utils::FileInfoExt;
use acton_config::config::ActonConfig;
use lsp_types::MessageType;
use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;
use tolk_linter::Checker;
use tolk_resolver::ProjectIndex;
use tolk_resolver::symbol_resolver::resolve;
use tolk_ty::{TypeDb, TypeInterner, infer};
use tower_lsp::lsp_types::Url;
use tree_sitter::Tree;

impl Backend {
    pub async fn analyze(&self, uri: Url) {
        self.analyze_incremental(uri, None).await;
    }

    pub async fn analyze_incremental(&self, uri: Url, old_tree: Option<Tree>) {
        crate::profile!(self, "analyze");
        let path = match uri.to_file_path() {
            Ok(p) => p,
            Err(_) => return,
        };

        let now = Instant::now();
        if let Some(content) = self.documents.get(&uri) {
            match self.file_db.process_content_incremental(
                path.clone(),
                &content,
                old_tree.as_ref(),
            ) {
                Ok(info) => Some(info),
                Err(e) => {
                    self.client
                        .log_message(
                            MessageType::ERROR,
                            format!("Failed to process content: {}", e),
                        )
                        .await;
                    return;
                }
            };
        }
        log::info!("Reparse took {:?}", now.elapsed());

        match self.run_analysis(path.clone()) {
            Ok(analysis) => {
                let arc_analysis = Arc::new(analysis);
                for &file_id in arc_analysis.all_body_types.keys() {
                    if let Some(info) = self.file_db.get_by_id(file_id)
                        && let Some(file_uri) = info.url()
                    {
                        self.analysis.insert(file_uri, arc_analysis.clone());
                    }
                }

                // Publish diagnostics to client
                let diagnostics_by_uri =
                    self.convert_linter_diagnostics_to_lsp(&arc_analysis.diagnostics);
                for (uri, diagnostics) in diagnostics_by_uri {
                    self.client
                        .publish_diagnostics(uri, diagnostics, None)
                        .await;
                }
            }
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Analysis failed for {}: {}", path.display(), e),
                    )
                    .await;
            }
        }
    }

    pub fn run_analysis(&self, root_path: PathBuf) -> anyhow::Result<AnalysisResult> {
        crate::profile!(self, "run_analysis");
        let now = Instant::now();

        let stdlib_path = self.file_db.stdlib_path();
        let root_path = self.file_db.canonicalize(root_path)?;

        let acton_config = ActonConfig::load()?;
        let analysis_roots = self.collect_analysis_roots(&acton_config, &root_path);

        let mut index = ProjectIndex::builder_for_roots(&self.file_db, analysis_roots.clone())
            .with_stdlib(stdlib_path.to_owned())
            .with_mappings(&acton_config.mappings)
            .build()?;
        resolve(&self.file_db, &mut index);

        let resolving_time = now.elapsed();
        let now = Instant::now();

        let mut interner = TypeInterner::new();
        let mut type_db = TypeDb::new(&mut interner, &self.file_db, &index);

        let mut root_scopes = HashMap::new();
        let mut files_to_infer = FxHashSet::default();
        for root in &analysis_roots {
            let Some(root_file_id) = index.get_file_by_path(root) else {
                continue;
            };

            let reachable: FxHashSet<_> = index.reachable_files(root_file_id).into_iter().collect();
            files_to_infer.extend(reachable.iter().copied());
            root_scopes.insert(root_file_id, reachable);
        }

        if root_scopes.is_empty()
            && let Some(root_file_id) = index.get_file_by_path(&root_path)
        {
            let reachable: FxHashSet<_> = index.reachable_files(root_file_id).into_iter().collect();
            files_to_infer.extend(reachable.iter().copied());
            root_scopes.insert(root_file_id, reachable);
        }

        let mut all_body_types = HashMap::new();
        let mut infer_files = files_to_infer.into_iter().collect::<Vec<_>>();
        infer_files.sort_unstable();
        for file_id in infer_files {
            let Some(file_info) = self.file_db.get_by_id(file_id) else {
                continue;
            };

            let mut body_types = HashMap::new();

            for decl in file_info.source().top_levels() {
                let Some(index_decl) = file_info.find_declaration(&decl) else {
                    continue;
                };

                let res = infer(&mut type_db, file_id, index_decl.id, &decl);
                body_types.insert(index_decl.id, res);
            }

            all_body_types.insert(file_id, body_types);
        }

        let type_inference_time = now.elapsed();

        let bodies = all_body_types.values().flat_map(|b| b.keys()).count();
        log::info!(
            "Analysing took: resolving {resolving_time:?}, type inference {type_inference_time:?}, bodies: {bodies}"
        );

        let now = Instant::now();
        let mut diagnostics = Vec::new();
        for scope_files in root_scopes.values() {
            let mut checker = Checker::new(&self.file_db, &mut type_db, &all_body_types)
                .with_scope_files(scope_files.clone());
            checker.run_once();

            let mut files_to_check = scope_files.iter().copied().collect::<Vec<_>>();
            files_to_check.sort_unstable();

            for file_id in files_to_check {
                let Some(file_info) = self.file_db.get_by_id(file_id) else {
                    continue;
                };
                if !file_info.is_workspace_file() {
                    // we don't want to check non-workspace files
                    continue;
                }
                checker.process_file(file_info.source(), file_id);
            }

            diagnostics.extend(checker.diagnostics);
        }

        let diagnostics = diagnostics
            .into_iter()
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let linting_time = now.elapsed();
        log::info!("Linting took {:?}", linting_time);

        Ok(AnalysisResult {
            project_index: Arc::new(index),
            type_interner: Arc::new(interner),
            all_body_types,
            diagnostics,
        })
    }

    fn collect_analysis_roots(&self, config: &ActonConfig, initial_root: &PathBuf) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let mut seen = std::collections::HashSet::new();

        let mut add_root = |path: PathBuf| {
            if seen.insert(path.clone()) {
                roots.push(path);
            }
        };

        add_root(initial_root.clone());

        for document in self.documents.iter() {
            let Ok(path) = document.key().to_file_path() else {
                continue;
            };
            let Ok(path) = self.file_db.canonicalize(path) else {
                continue;
            };
            add_root(path);
        }

        for contract in config.contracts().cloned().unwrap_or_default().values() {
            if !contract.src.ends_with(".tolk") {
                continue;
            }
            let Ok(path) = self.file_db.canonicalize(PathBuf::from(&contract.src)) else {
                continue;
            };
            add_root(path);
        }

        roots.sort_unstable();
        roots
    }
}
