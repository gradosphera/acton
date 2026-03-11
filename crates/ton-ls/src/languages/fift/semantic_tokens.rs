use crate::backend::Backend;
use crate::backend::utils::offsets_to_lsp_range;
use crate::languages::fift::psi::FiftReference;
use crate::languages::fift::traverse::PreorderTraverse;
use crate::languages::semantic_tokens::{
    SemanticTokensBuilder as CommonSemanticTokensBuilder, semantic_tokens_result_id,
};
use crate::languages::tolk::semantic_tokens::TokenType;
use lsp_types::{
    SemanticToken, SemanticTokens, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensResult::Tokens,
};
use tower_lsp::jsonrpc::Result as LspResult;
use tower_lsp::lsp_types::Url;
use tree_sitter::Node;

impl Backend {
    pub async fn handle_fift_semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> LspResult<Option<SemanticTokensResult>> {
        crate::profile!(self, "fift-semantic_tokens");
        let now = std::time::Instant::now();
        let uri = params.text_document.uri;
        log::info!("Request: fift semantic_tokens_full for {}", uri);

        let Some(data) = self.fift_semantic_tokens(&uri) else {
            return Ok(None);
        };

        log::info!(
            "Response: fift semantic_tokens_full took {:?}",
            now.elapsed()
        );
        Ok(Some(Tokens(SemanticTokens {
            result_id: Some(semantic_tokens_result_id()),
            data,
        })))
    }

    fn fift_semantic_tokens(&self, uri: &Url) -> Option<Vec<SemanticToken>> {
        let snapshot = self.registry.find_fift_file(uri)?;
        Some(collect_function_tokens(
            snapshot.source_file.as_ref(),
            snapshot.text.as_ref(),
        ))
    }
}

fn collect_function_tokens(
    source_file: &fift_syntax::SourceFile,
    source: &str,
) -> Vec<SemanticToken> {
    let mut builder = CommonSemanticTokensBuilder::new();
    for node in PreorderTraverse::new(source_file.root_node().walk()) {
        if !node.is_named() {
            continue;
        }

        if is_function_definition(node.kind())
            && let Some(name_node) = node.child_by_field_name("name")
        {
            push_function_token(&mut builder, name_node, source);
        }

        if node.kind() == "identifier" {
            let Some(parent) = node.parent() else {
                continue;
            };

            if !is_definition_name(parent, node)
                && FiftReference::new(node, source_file)
                    .and_then(|reference| reference.resolve())
                    .is_some()
            {
                push_function_token(&mut builder, node, source);
            }
        }
    }

    builder.build()
}

fn is_function_definition(kind: &str) -> bool {
    matches!(
        kind,
        "proc_definition"
            | "proc_inline_definition"
            | "proc_ref_definition"
            | "method_definition"
            | "proc_declaration"
            | "method_declaration"
            | "declaration"
    )
}

fn is_definition_name(parent: Node<'_>, node: Node<'_>) -> bool {
    if parent.child_by_field_name("name") != Some(node) {
        return false;
    }

    is_function_definition(parent.kind())
}

fn push_function_token(builder: &mut CommonSemanticTokensBuilder, node: Node<'_>, source: &str) {
    let range = offsets_to_lsp_range(node.start_byte(), node.end_byte(), source);
    builder.add_token_at_range(range, TokenType::Function as u32, 0);
}
