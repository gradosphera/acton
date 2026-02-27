use crate::AnalysisResult;
use crate::backend::Backend;
use crate::backend::utils::FileInfoExt;
use crate::languages::tolk::analysis::RootAnalysis;
use crate::languages::tolk::diagnostics::convert_single_diagnostic;
use acton_config::config::ActonConfig;
use anyhow::anyhow;
use lsp_types::MessageType;
use rustc_hash::FxHashSet;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tolk_linter::Checker;
use tolk_resolver::ProjectIndex;
use tolk_resolver::file_index::FileId;
use tolk_resolver::symbol_resolver::resolve;
use tolk_ty::{TypeDb, TypeInterner, infer};
use tower_lsp::lsp_types::Url;
use tree_sitter::Tree;

impl Backend {
    pub async fn initialize_workspace_analysis(&self) -> anyhow::Result<()> {
        let config = ActonConfig::load()?;
        let roots = self.collect_workspace_roots(&config);
        if roots.is_empty() {
            log::info!("LS workspace analysis init skipped: no roots found");
            return Ok(());
        }

        log::info!("LS workspace analysis init: {} root(s)", roots.len());
        if log::log_enabled!(log::Level::Debug) {
            let root_paths = roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>();
            log::debug!("LS workspace roots: {root_paths:?}");
        }

        let mut files_to_republish = FxHashSet::default();
        self.reanalyze_roots(&config, &roots, &mut files_to_republish);
        self.rebuild_analysis_cache();
        self.publish_diagnostics_for_files(files_to_republish).await;

        Ok(())
    }

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
                Ok(_) => {}
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

        let canonical_path = self.file_db.canonicalize(&path).unwrap_or(path.clone());

        let acton_config = match ActonConfig::load() {
            Ok(config) => config,
            Err(e) => {
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("Failed to load Acton config: {e}"),
                    )
                    .await;
                return;
            }
        };
        let known_roots = self.collect_analysis_roots(&acton_config, &canonical_path);

        let changed_file_id = self
            .file_db
            .get_by_path(&canonical_path)
            .or_else(|| self.file_db.get_by_path(&path))
            .map(|info| info.id());

        log::info!(
            "LS invalidation input: changed={}, file_id={changed_file_id:?}, known_roots={}",
            canonical_path.display(),
            known_roots.len()
        );

        if log::log_enabled!(log::Level::Debug) {
            let roots = known_roots
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>();
            log::debug!("LS known roots for {}: {roots:?}", canonical_path.display());
        }

        let mut roots_to_reanalyze =
            self.affected_roots_for_change(changed_file_id, &canonical_path, &known_roots);
        let affected_roots_count = roots_to_reanalyze.len();

        let mut new_roots = Vec::new();
        for root in known_roots {
            if !self.root_analyses.contains_key(&root) {
                new_roots.push(root.clone());
                roots_to_reanalyze.push(root);
            }
        }
        roots_to_reanalyze.sort_unstable();
        roots_to_reanalyze.dedup();

        log::info!(
            "LS invalidation decision: changed={}, affected_roots={}, new_roots={}, reanalyze_roots={}",
            canonical_path.display(),
            affected_roots_count,
            new_roots.len(),
            roots_to_reanalyze.len()
        );

        if log::log_enabled!(log::Level::Debug) {
            let roots = roots_to_reanalyze
                .iter()
                .map(|root| root.display().to_string())
                .collect::<Vec<_>>();
            log::debug!(
                "LS roots scheduled for reanalysis after change {}: {roots:?}",
                canonical_path.display()
            );
        }

        if roots_to_reanalyze.is_empty() {
            log::debug!(
                "LS invalidation skipped: no roots to reanalyze for {}",
                canonical_path.display()
            );
            return;
        }

        let mut files_to_republish = FxHashSet::default();
        self.reanalyze_roots(&acton_config, &roots_to_reanalyze, &mut files_to_republish);
        self.rebuild_analysis_cache();
        self.publish_diagnostics_for_files(files_to_republish).await;
    }

    fn affected_roots_for_change(
        &self,
        changed_file_id: Option<FileId>,
        changed_path: &Path,
        known_roots: &[PathBuf],
    ) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();

        if let Some(file_id) = changed_file_id
            && let Some(root_paths) = self.file_to_roots.get(&file_id)
        {
            let mut from_index = root_paths.iter().cloned().collect::<Vec<_>>();
            from_index.sort_unstable();
            for root in from_index {
                if seen.insert(root.clone()) {
                    roots.push(root);
                }
            }
        }

        if roots.is_empty() && known_roots.iter().any(|root| root == changed_path) {
            let root = changed_path.to_path_buf();
            if seen.insert(root.clone()) {
                roots.push(root);
            }
        }

        if roots.is_empty() {
            let root = changed_path.to_path_buf();
            if seen.insert(root.clone()) {
                roots.push(root);
            }
        }

        roots
    }

    fn reanalyze_roots(
        &self,
        acton_config: &ActonConfig,
        roots: &[PathBuf],
        files_to_republish: &mut FxHashSet<FileId>,
    ) {
        log::info!("LS reanalyzing {} root(s)", roots.len());
        for root in roots {
            let started = Instant::now();
            let mut previous_scope_size = 0usize;
            if let Some(existing) = self.root_analyses.get(root) {
                previous_scope_size = existing.scope_files.len();
                files_to_republish.extend(existing.scope_files.iter().copied());
            }

            match self.run_root_analysis(root.clone(), acton_config) {
                Ok(root_analysis) => {
                    let next_scope_size = root_analysis.scope_files.len();
                    let diagnostics_count = root_analysis.analysis.diagnostics.len();
                    files_to_republish.extend(root_analysis.scope_files.iter().copied());
                    self.root_analyses
                        .insert(root.clone(), Arc::new(root_analysis));
                    log::info!(
                        "LS root reanalyzed: root={}, prev_scope_files={}, new_scope_files={}, diagnostics={}, took={:?}",
                        root.display(),
                        previous_scope_size,
                        next_scope_size,
                        diagnostics_count,
                        started.elapsed()
                    );
                }
                Err(err) => {
                    log::error!(
                        "Failed to analyze root {} after {:?}: {err:#}",
                        root.display(),
                        started.elapsed()
                    );
                }
            }
        }
    }

    fn rebuild_analysis_cache(&self) {
        self.analysis.clear();
        self.file_to_roots.clear();

        let mut root_entries = self
            .root_analyses
            .iter()
            .map(|entry| (entry.key().clone(), entry.value().clone()))
            .collect::<Vec<_>>();
        let root_count = root_entries.len();
        root_entries.sort_by(|(left, _), (right, _)| left.cmp(right));
        let mut file_root_edges = 0usize;

        for (root_path, root_analysis) in root_entries {
            for &file_id in &root_analysis.scope_files {
                file_root_edges += 1;
                self.file_to_roots
                    .entry(file_id)
                    .and_modify(|roots| {
                        roots.insert(root_path.clone());
                    })
                    .or_insert_with(|| {
                        let mut roots = FxHashSet::default();
                        roots.insert(root_path.clone());
                        roots
                    });

                if let Some(file_info) = self.file_db.get_by_id(file_id)
                    && let Some(file_uri) = file_info.url()
                {
                    if file_id == root_analysis.root_file_id {
                        // For root file itself prefer its own root analysis.
                        self.analysis
                            .insert(file_uri, root_analysis.analysis.clone());
                    } else {
                        self.analysis
                            .entry(file_uri)
                            .or_insert_with(|| root_analysis.analysis.clone());
                    }
                }
            }
        }

        log::info!(
            "LS analysis cache rebuilt: roots={}, uri_analysis_entries={}, file_to_roots_entries={}, file_root_edges={}",
            root_count,
            self.analysis.len(),
            self.file_to_roots.len(),
            file_root_edges
        );
    }

    fn collect_diagnostics_for_file(
        &self,
        file_id: FileId,
    ) -> Vec<tolk_linter::diagnostic::Diagnostic> {
        let Some(root_paths) = self.file_to_roots.get(&file_id) else {
            return Vec::new();
        };

        let mut diagnostics = Vec::new();
        for root_path in root_paths.iter() {
            if let Some(root_analysis) = self.root_analyses.get(root_path) {
                diagnostics.extend(
                    root_analysis
                        .analysis
                        .diagnostics
                        .iter()
                        .filter(|diagnostic| diagnostic.file_id == file_id)
                        .cloned(),
                );
            }
        }

        diagnostics
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect()
    }

    async fn publish_diagnostics_for_files(&self, files: FxHashSet<FileId>) {
        let mut file_ids = files.into_iter().collect::<Vec<_>>();
        file_ids.sort_unstable();
        log::info!("LS publishing diagnostics for {} file(s)", file_ids.len());

        for file_id in file_ids {
            let Some(file_info) = self.file_db.get_by_id(file_id) else {
                continue;
            };
            let Some(uri) = file_info.url() else {
                continue;
            };

            let diagnostics = self.collect_diagnostics_for_file(file_id);
            let lsp_diagnostics = diagnostics
                .iter()
                .filter_map(|diagnostic| convert_single_diagnostic(diagnostic, &file_info))
                .collect::<Vec<_>>();

            log::debug!(
                "LS diagnostics publish: file_id={file_id:?}, path={}, diagnostics={}",
                file_info.path().display(),
                lsp_diagnostics.len()
            );

            self.client
                .publish_diagnostics(uri, lsp_diagnostics, None)
                .await;
        }
    }

    pub fn run_root_analysis(
        &self,
        root_path: PathBuf,
        acton_config: &ActonConfig,
    ) -> anyhow::Result<RootAnalysis> {
        crate::profile!(self, "run_analysis");
        let now = Instant::now();

        let stdlib_path = self.file_db.stdlib_path();
        let root_path = self.file_db.canonicalize(root_path)?;
        log::debug!("LS run_root_analysis start: root={}", root_path.display());
        let mut index = ProjectIndex::builder_for_roots(&self.file_db, vec![root_path.clone()])
            .with_stdlib(stdlib_path.to_owned())
            .with_mappings(&acton_config.mappings)
            .build()?;
        resolve(&self.file_db, &mut index);

        let resolving_time = now.elapsed();
        let now = Instant::now();

        let mut interner = TypeInterner::new();
        let mut type_db = TypeDb::new(&mut interner, &self.file_db, &index);

        let Some(root_file_id) = index.get_file_by_path(&root_path) else {
            return Err(anyhow!(
                "Root file {} is missing in project index",
                root_path.display()
            ));
        };
        let scope_files: FxHashSet<_> = index.reachable_files(root_file_id).into_iter().collect();

        let mut all_body_types = HashMap::new();
        let mut infer_files = scope_files.iter().copied().collect::<Vec<_>>();
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
            "LS root analysis stats: root={}, resolving={resolving_time:?}, type_inference={type_inference_time:?}, scope_files={}, bodies={}",
            root_path.display(),
            scope_files.len(),
            bodies
        );

        let now = Instant::now();
        let mut diagnostics = Vec::new();
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

        let diagnostics = diagnostics
            .into_iter()
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let linting_time = now.elapsed();
        log::info!(
            "LS root linting stats: root={}, linting={linting_time:?}, diagnostics={}",
            root_path.display(),
            diagnostics.len()
        );

        Ok(RootAnalysis {
            root_path,
            root_file_id,
            scope_files,
            analysis: Arc::new(AnalysisResult {
                project_index: Arc::new(index),
                type_interner: Arc::new(interner),
                all_body_types,
                diagnostics,
            }),
        })
    }

    fn collect_workspace_roots(&self, config: &ActonConfig) -> Vec<PathBuf> {
        let mut roots = Vec::new();
        let mut seen = HashSet::new();

        let mut add_root = |path: PathBuf| {
            if seen.insert(path.clone()) {
                roots.push(path);
            }
        };

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

    fn collect_analysis_roots(&self, config: &ActonConfig, initial_root: &Path) -> Vec<PathBuf> {
        let mut roots = self.collect_workspace_roots(config);
        if !roots.iter().any(|root| root == initial_root) {
            roots.push(initial_root.to_owned());
            roots.sort_unstable();
        }
        roots
    }
}
