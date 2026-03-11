use crate::backend::Backend;
use crate::backend::utils::{get_point, offsets_to_lsp_range};
use crate::languages::fift::psi::FiftReference;
use lsp_types::{GotoDefinitionParams, GotoDefinitionResponse, Location, Range};
use tower_lsp::jsonrpc::Result as LspResult;
use tree_sitter::{Node, Point};

impl Backend {
    pub async fn handle_fift_goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> LspResult<Option<GotoDefinitionResponse>> {
        crate::profile!(self, "fift-goto_definition");
        let now = std::time::Instant::now();

        let uri = params.text_document_position_params.text_document.uri;
        log::info!("Request: fift goto_definition for {}", uri);

        let Some(snapshot) = self.registry.find_fift_file(&uri) else {
            return Ok(None);
        };

        let source = snapshot.text.as_ref();
        let source_file = snapshot.source_file.as_ref();
        let point = get_point(source, params.text_document_position_params.position);
        let Some(range) = find_definition_range(source_file, source, point) else {
            return Ok(None);
        };

        log::info!("Response: fift goto_definition took {:?}", now.elapsed());
        Ok(Some(GotoDefinitionResponse::Scalar(Location::new(
            uri, range,
        ))))
    }
}

fn find_definition_range(
    source_file: &fift_syntax::SourceFile,
    source: &str,
    point: Point,
) -> Option<Range> {
    let node = node_at_position(source_file.root_node(), point)?;
    let reference = FiftReference::new(node, source_file)?;
    let definition = reference.resolve()?;
    let name_node = definition.child_by_field_name("name").unwrap_or(definition);

    Some(offsets_to_lsp_range(
        name_node.start_byte(),
        name_node.end_byte(),
        source,
    ))
}

fn node_at_position(root: Node<'_>, point: Point) -> Option<Node<'_>> {
    root.descendant_for_point_range(point, point)
}
