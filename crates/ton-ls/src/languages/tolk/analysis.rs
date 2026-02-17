use std::collections::HashMap;
use std::sync::Arc;
use tolk_resolver::file_index::{FileId, SymbolId};
use tolk_resolver::project_index::ProjectIndex;
use tolk_ty::InferenceResult;

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub project_index: Arc<ProjectIndex>,
    pub all_body_types: HashMap<FileId, HashMap<SymbolId, InferenceResult>>,
    pub diagnostics: Vec<tolk_linter::diagnostic::Diagnostic>,
}
