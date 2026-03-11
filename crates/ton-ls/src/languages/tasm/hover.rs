use crate::backend::Backend;
use crate::backend::utils::{get_point, offsets_to_lsp_range};
use crate::languages::engine::cache::ParsedSnapshot;
use crate::languages::instruction_docs::{build_hover_markdown, get_instruction_docs_index};
use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Range};
use tower_lsp::jsonrpc::Result as LspResult;
use tree_sitter::{Node, Point};

struct HoverTarget {
    name: String,
    range: Range,
}

impl Backend {
    pub async fn handle_tasm_hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        crate::profile!(self, "tasm-hover");
        let now = std::time::Instant::now();

        let uri = params.text_document_position_params.text_document.uri;
        log::info!("Request: tasm hover for {}", uri);

        let Some(snapshot) = self.registry.find_tasm_file(&uri) else {
            return Ok(None);
        };

        let Some(target) = find_hover_target_for_snapshot(
            &snapshot,
            params.text_document_position_params.position,
        ) else {
            return Ok(None);
        };

        let Some(spec_index) = get_instruction_docs_index() else {
            return Ok(None);
        };

        let Some(markdown) = build_hover_markdown(&target.name, spec_index) else {
            return Ok(None);
        };

        log::info!("Response: hover took {:?}", now.elapsed());
        Ok(Some(Hover {
            contents: HoverContents::Markup(MarkupContent {
                kind: MarkupKind::Markdown,
                value: markdown,
            }),
            range: Some(target.range),
        }))
    }
}

fn find_hover_target_for_snapshot(
    snapshot: &ParsedSnapshot<tasm_syntax::SourceFile>,
    position: lsp_types::Position,
) -> Option<HoverTarget> {
    let source = snapshot.text.as_ref();
    let source_file = snapshot.source_file.as_ref();
    let point = get_point(source, position);
    find_instruction_hover_target(source_file, source, point)
}

fn find_instruction_hover_target(
    source_file: &tasm_syntax::SourceFile,
    source: &str,
    point: Point,
) -> Option<HoverTarget> {
    let root = source_file.root_node();
    let node = node_at_position(root, point)?;

    let name = node.utf8_text(source.as_bytes()).ok()?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let range = offsets_to_lsp_range(node.start_byte(), node.end_byte(), source);

    Some(HoverTarget { name, range })
}

fn node_at_position(root: Node<'_>, point: Point) -> Option<Node<'_>> {
    root.descendant_for_point_range(point, point)
}
