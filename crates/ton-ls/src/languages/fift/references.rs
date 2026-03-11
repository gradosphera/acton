use crate::backend::Backend;
use crate::backend::utils::{get_point, offsets_to_lsp_range};
use crate::languages::fift::psi::FiftReferent;
use lsp_types::{Location, Range, ReferenceParams};
use tower_lsp::jsonrpc::Result as LspResult;
use tree_sitter::{Node, Point};

impl Backend {
    pub async fn handle_fift_references(
        &self,
        params: ReferenceParams,
    ) -> LspResult<Option<Vec<Location>>> {
        crate::profile!(self, "fift-references");
        let now = std::time::Instant::now();

        let uri = params.text_document_position.text_document.uri;
        log::info!("Request: fift references for {}", uri);

        let Some(source) = self
            .documents
            .get(&uri)
            .map(|text| text.clone())
            .or_else(|| {
                uri.to_file_path()
                    .ok()
                    .and_then(|path| std::fs::read_to_string(path).ok())
            })
        else {
            return Ok(None);
        };

        let Ok(source_file) = fift_syntax::parse(&source) else {
            return Ok(None);
        };

        let point = get_point(&source, params.text_document_position.position);
        let Some(ranges) = find_reference_ranges(
            &source_file,
            &source,
            point,
            params.context.include_declaration,
        ) else {
            return Ok(None);
        };

        let locations = ranges
            .into_iter()
            .map(|range| Location::new(uri.clone(), range))
            .collect::<Vec<_>>();

        log::info!(
            "Response: fift references took {:?}, found {} references",
            now.elapsed(),
            locations.len()
        );
        Ok(Some(locations))
    }
}

fn find_reference_ranges(
    source_file: &fift_syntax::SourceFile,
    source: &str,
    point: Point,
    include_definition: bool,
) -> Option<Vec<Range>> {
    let node = node_at_position(source_file.root_node(), point)?;
    let referent = FiftReferent::new(node, source_file);
    referent.resolved()?;

    let ranges = referent
        .find_references(include_definition)
        .into_iter()
        .map(|node| {
            let target = node.child_by_field_name("name").unwrap_or(node);
            offsets_to_lsp_range(target.start_byte(), target.end_byte(), source)
        })
        .collect::<Vec<_>>();

    Some(ranges)
}

fn node_at_position(root: Node<'_>, point: Point) -> Option<Node<'_>> {
    root.descendant_for_point_range(point, point)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_reference_ranges() {
        let source = r#"PROGRAM{
DECLPROC entry
entry PROC:<{
  foo
  foo
}>
foo PROC:<{
  foo
}>
END>c
"#;

        let source_file = fift_syntax::parse(source).expect("failed to parse fixture");
        let offset = source.find("  foo").expect("reference must exist") + 2;
        let point = offset_to_point(source, offset);

        let without_def =
            find_reference_ranges(&source_file, source, point, false).expect("ranges must resolve");
        assert_eq!(without_def.len(), 3);

        let with_def =
            find_reference_ranges(&source_file, source, point, true).expect("ranges must resolve");
        assert_eq!(with_def.len(), 4);
    }

    #[test]
    fn returns_none_for_unresolved_symbol() {
        let source = r#"PROGRAM{
DECLPROC entry
entry PROC:<{
  missing
}>
foo PROC:<{ }>
END>c
"#;

        let source_file = fift_syntax::parse(source).expect("failed to parse fixture");
        let offset = source.find("missing").expect("reference must exist");
        let point = offset_to_point(source, offset);
        assert!(find_reference_ranges(&source_file, source, point, false).is_none());
    }

    fn offset_to_point(source: &str, byte_offset: usize) -> Point {
        let mut row = 0usize;
        let mut column = 0usize;

        for (index, byte) in source.bytes().enumerate() {
            if index >= byte_offset {
                break;
            }
            if byte == b'\n' {
                row += 1;
                column = 0;
            } else {
                column += 1;
            }
        }

        Point::new(row, column)
    }
}
