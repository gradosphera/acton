use crate::AnalysisResult;
use crate::backend::profiling::ProfilingContext;
use crate::backend::utils::FileInfoExt;
use acton_config::config::ActonConfig;
use dashmap::DashMap;
use globset::{Glob, GlobSetBuilder};
use lsp_types::Url;
use owo_colors::OwoColorize;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;
use tolk_linter::Checker;
use tolk_resolver::{FileDb, FileId, ProjectIndex, ProjectIndexBuilder, SymbolId, resolve};
use tolk_ty::{InferenceResult, TypeDb, TypeInterner, infer};
use walkdir::WalkDir;

pub mod analysis;
pub mod analysis_engine;
pub mod code_actions;
pub mod diagnostics;
pub mod document_sync;
pub mod goto_definition;
pub mod inlay_hints;
pub mod references;
pub mod resolution;
pub mod semantic_tokens;
pub mod symbols;

#[derive(Debug)]
pub struct RootAnalysisResult {
    pub index: Arc<ProjectIndex>,
    pub all_body_types: HashMap<FileId, Arc<HashMap<SymbolId, InferenceResult>>>,
}

pub struct TolkAnalyzer {
    pub file_db: Arc<FileDb>,
    pub documents: DashMap<Url, String>,
    pub roots: FxHashMap<PathBuf, RootAnalysisResult>,
    pub file_urls: DashMap<FileId, Url>,
    #[cfg(feature = "profiling")]
    pub profiling: Arc<ProfilingContext>,
}

impl TolkAnalyzer {
    pub fn empty() -> Self {
        Self {
            file_db: Arc::new(FileDb::new(Default::default(), Default::default())),
            documents: Default::default(),
            roots: Default::default(),
            file_urls: Default::default(),
        }
    }

    pub fn start(root_dir: PathBuf) -> anyhow::Result<TolkAnalyzer> {
        let acton_config = ActonConfig::load()?;

        let files = find_files(&root_dir)?;

        let stdlib = find_stdlib(&root_dir)?;
        let acton_stdlib = find_acton_stdlib(&root_dir)?;
        let common_tolk = stdlib.join("common.tolk");

        let file_db = Arc::new(FileDb::new(stdlib.clone(), Some(acton_stdlib)));

        // We need stdlib for all targets so preprocess it before all.
        if common_tolk.exists() {
            file_db.process(&common_tolk)?;
        }

        let mut file_urls = DashMap::new();
        let mut roots = vec![];
        for file in files {
            let Ok(file_info) = file_db.process(&file) else {
                continue;
            };

            if file_info.index().is_root_file() {
                roots.push(file);
            }

            if let Some(url) = file_info.url() {
                file_urls.insert(file_info.id(), url);
            }
        }

        let mut all_roots_body_types: HashMap<FileId, Arc<HashMap<SymbolId, InferenceResult>>> =
            HashMap::new();

        let mut analyzed_roots = FxHashMap::default();
        for root_path in roots {
            let mut index = ProjectIndexBuilder::new(file_db.clone(), root_path.clone())
                .with_stdlib(stdlib.to_owned())
                .with_mappings(&acton_config.mappings)
                .build()?;
            resolve(&file_db, &mut index);

            let mut type_db = TypeDb::new(file_db.clone(), &index);

            let mut root_body_types = HashMap::new();

            let root_file_id = index
                .get_file_by_path(&root_path)
                .ok_or_else(|| anyhow::anyhow!("Root file id not found"))?;
            let reachable = index.reachable_files(root_file_id);

            for &file_id in &reachable {
                if let Some(cached) = all_roots_body_types.get(&file_id) {
                    root_body_types.insert(file_id, cached.clone());
                    continue;
                }

                let file_info = file_db.get_by_id(file_id).expect("file not found");

                let mut body_types = HashMap::new();
                for decl in file_info.source().top_levels() {
                    let Some(index_decl) = file_info.find_declaration(&decl) else {
                        continue;
                    };
                    let res = infer(&mut type_db, file_id, index_decl.id, &decl);
                    body_types.insert(index_decl.id, res);
                }

                let body_types = Arc::new(body_types);
                root_body_types.insert(file_id, body_types.clone());
                all_roots_body_types.insert(file_id, body_types);
            }

            analyzed_roots.insert(
                root_path,
                RootAnalysisResult {
                    index: Arc::new(index),
                    all_body_types: root_body_types,
                },
            );
        }

        #[cfg(feature = "profiling")]
        let profiling = Arc::new(crate::ProfilingContext::new());

        #[cfg(feature = "profiling")]
        {
            let profiling = profiling.clone();
            tokio::spawn(async move {
                let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(5));
                loop {
                    interval.tick().await;
                    profiling.log_stats();
                }
            });
        }

        Ok(TolkAnalyzer {
            file_db,
            documents: DashMap::default(),
            roots: analyzed_roots,
            file_urls,
            #[cfg(feature = "profiling")]
            profiling,
        })
    }

    pub fn get_file_url(&self, file_info: &tolk_resolver::file_db::FileInfo) -> Option<Url> {
        use crate::backend::utils::FileInfoExt;
        let url = self
            .file_urls
            .entry(file_info.id())
            .or_insert_with(|| file_info.url().expect("Failed to get URL for file"));
        Some(url.clone())
    }
}

fn find_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    const EXCLUDED_DIRS: &[&str] = &[
        ".git",
        ".github",
        ".idea",
        ".acton",
        "node_modules",
        "target",
        "tolk-stdlib",
    ];

    let mut exclude_builder = GlobSetBuilder::new();
    for p in [
        // ... for future ignoring via flags
    ] {
        exclude_builder.add(Glob::new(p)?);
    }
    let excludes = exclude_builder.build()?;

    let it = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name();
            if EXCLUDED_DIRS.iter().any(|d| name == OsStr::new(d)) {
                // fast path
                return false;
            }

            let p = entry.path();
            let rel = p.strip_prefix(root).unwrap_or(p);
            !excludes.is_match(rel)
        });

    let mut out: Vec<PathBuf> = Vec::with_capacity(32);

    for entry in it {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                log::warn!("walk dir error: {err}");
                continue;
            }
        };

        if entry.file_type().is_file() {
            let path = entry.path();

            if let Some(ext) = path.extension() {
                if ext != "tolk" {
                    continue;
                }
            } else {
                continue;
            }

            let rel = path.strip_prefix(root).unwrap_or(path);
            if excludes.is_match(rel) {
                continue;
            }

            out.push(path.to_path_buf());
        }
    }

    out.sort_unstable();
    Ok(out)
}

fn find_stdlib(root: &Path) -> anyhow::Result<PathBuf> {
    let path_to_stdlib = root.join(".acton/tolk-stdlib");
    if !path_to_stdlib.exists() {
        anyhow::bail!(
            "cannot find Tolk stdlib in .acton/, did you run {}?",
            "acton init".yellow()
        );
    }

    Ok(dunce::canonicalize(path_to_stdlib)?)
}

fn find_acton_stdlib(root: &Path) -> anyhow::Result<PathBuf> {
    let path_to_acton = root.join(".acton");
    if !path_to_acton.exists() {
        anyhow::bail!(
            "cannot find Acton in .acton/, did you run {}?",
            "acton init".yellow()
        );
    }

    Ok(dunce::canonicalize(path_to_acton)?)
}
