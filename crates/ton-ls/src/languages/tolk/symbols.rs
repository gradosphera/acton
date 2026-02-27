use crate::backend::Backend;
use crate::backend::utils::{FileInfoExt, offset_to_range};
use lsp_types::*;
use std::collections::HashSet;
use tower_lsp::jsonrpc::Result as LspResult;

impl Backend {
    pub async fn handle_symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> LspResult<Option<Vec<SymbolInformation>>> {
        crate::profile!(self, "workspace_symbol");
        let now = std::time::Instant::now();
        log::info!("Request: workspace/symbol query='{}'", params.query);

        let query = params.query.to_lowercase();
        if self.root_analyses.is_empty() {
            return Ok(None);
        }

        let mut symbols = Vec::new();
        let mut seen_symbols = HashSet::new();

        for root_analysis in self.root_analyses.iter() {
            let analysis = &root_analysis.value().analysis;
            for (fqn, ids) in analysis.project_index.global_symbols() {
                if !fqn.to_lowercase().contains(&query) {
                    continue;
                }

                for &id in ids {
                    if !seen_symbols.insert(id) {
                        continue;
                    }

                    if let Some(symbol) = analysis.project_index.resolve_symbol(id)
                        && let Some(file_info) = self.file_db.get_by_id(id.file_id)
                        && let Some(url) = file_info.url()
                    {
                        let range = offset_to_range(&file_info, symbol.name_span.start());
                        symbols.push(SymbolInformation {
                            name: symbol.fqn.to_string(),
                            kind: self.to_lsp_symbol_kind(&symbol.kind),
                            location: Location::new(url, range),
                            container_name: None,
                            tags: None,
                            #[allow(deprecated)]
                            deprecated: None,
                        });
                    }
                }
            }
        }

        log::info!(
            "Response: workspace/symbol took {:?}, found {} symbols",
            now.elapsed(),
            symbols.len()
        );
        Ok(Some(symbols))
    }

    pub fn to_lsp_symbol_kind(&self, kind: &tolk_resolver::file_index::SymbolKind) -> SymbolKind {
        match kind {
            tolk_resolver::file_index::SymbolKind::GlobalVariable => SymbolKind::VARIABLE,
            tolk_resolver::file_index::SymbolKind::Function { .. } => SymbolKind::FUNCTION,
            tolk_resolver::file_index::SymbolKind::Method { .. } => SymbolKind::METHOD,
            tolk_resolver::file_index::SymbolKind::GetMethod { .. } => SymbolKind::METHOD,
            tolk_resolver::file_index::SymbolKind::Struct { .. } => SymbolKind::STRUCT,
            tolk_resolver::file_index::SymbolKind::StructField => SymbolKind::FIELD,
            tolk_resolver::file_index::SymbolKind::Enum { .. } => SymbolKind::ENUM,
            tolk_resolver::file_index::SymbolKind::EnumMember => SymbolKind::ENUM_MEMBER,
            tolk_resolver::file_index::SymbolKind::Constant => SymbolKind::CONSTANT,
            tolk_resolver::file_index::SymbolKind::TypeAlias { .. } => SymbolKind::CLASS,
        }
    }
}
