use rustc_hash::FxHashSet;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use tolk_resolver::file_index::{FileId, SymbolId};
use tolk_resolver::project_index::ProjectIndex;
use tolk_ty::InferenceResult;
use tolk_ty::TypeInterner;

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub project_index: Arc<ProjectIndex>,
    pub type_interner: Arc<TypeInterner>,
    pub all_body_types: HashMap<FileId, HashMap<SymbolId, InferenceResult>>,
    pub diagnostics: Vec<tolk_linter::diagnostic::Diagnostic>,
}

#[derive(Debug, Clone)]
pub struct RootAnalysis {
    pub root_path: PathBuf,
    pub root_file_id: FileId,
    pub scope_files: FxHashSet<FileId>,
    pub analysis: Arc<AnalysisResult>,
}
