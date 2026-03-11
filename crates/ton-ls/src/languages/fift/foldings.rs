use crate::backend::Backend;
use crate::languages::fift::traverse::PreorderTraverse;
use lsp_types::{FoldingRange, FoldingRangeKind, FoldingRangeParams};
use tower_lsp::jsonrpc::Result as LspResult;
use tree_sitter::Node;

impl Backend {
    pub async fn handle_fift_folding_range(
        &self,
        params: FoldingRangeParams,
    ) -> LspResult<Option<Vec<FoldingRange>>> {
        crate::profile!(self, "fift-folding_range");
        let now = std::time::Instant::now();

        let uri = params.text_document.uri;
        log::info!("Request: fift folding_range for {}", uri);

        let Some(snapshot) = self.registry.find_fift_file(&uri) else {
            return Ok(None);
        };

        let ranges = collect_ranges(snapshot.source_file.as_ref());

        log::info!(
            "Response: fift folding_range took {:?}, found {} ranges",
            now.elapsed(),
            ranges.len()
        );
        Ok(Some(ranges))
    }
}

fn collect_ranges(source_file: &fift_syntax::SourceFile) -> Vec<FoldingRange> {
    let mut result = Vec::new();

    for node in PreorderTraverse::new(source_file.root_node().walk()) {
        if !node.is_named() {
            continue;
        }
        if !is_foldable(node.kind()) {
            continue;
        }
        push_generic_folding(node, &mut result);
    }

    result.sort_by_key(|range| (range.start_line, range.end_line));
    result
}

fn is_foldable(kind: &str) -> bool {
    matches!(
        kind,
        "program"
            | "proc_definition"
            | "proc_inline_definition"
            | "proc_ref_definition"
            | "method_definition"
            | "block_instruction"
            | "instruction_block"
            | "if_statement"
            | "ifjmp_statement"
            | "while_statement"
            | "repeat_statement"
            | "until_statement"
    )
}

fn push_generic_folding(node: Node<'_>, result: &mut Vec<FoldingRange>) {
    let child_count = node.child_count();
    if child_count == 0 {
        return;
    }

    let Some(open_brace) = node.child(0) else {
        return;
    };
    let Some(close_brace) = node.child(child_count - 1) else {
        return;
    };

    let start_line = open_brace.end_position().row as u32;
    let end_line = close_brace.start_position().row as u32;

    if end_line <= start_line {
        return;
    }

    result.push(FoldingRange {
        start_line,
        start_character: None,
        end_line,
        end_character: None,
        kind: Some(FoldingRangeKind::Region),
        collapsed_text: None,
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn pairs(source: &str) -> anyhow::Result<Vec<(u32, u32)>> {
        let source_file = fift_syntax::parse(source)?;
        Ok(collect_ranges(&source_file)
            .into_iter()
            .map(|range| (range.start_line, range.end_line))
            .collect())
    }

    #[test]
    fn folds_program_and_proc_definition() -> anyhow::Result<()> {
        let source = r#"PROGRAM{
DECLPROC foo
foo PROC:<{
  1
  2
}>
END>c
"#;

        let fold_pairs = pairs(source)?;
        assert!(fold_pairs.contains(&(0, 6)));
        assert!(fold_pairs.contains(&(2, 5)));
        Ok(())
    }

    #[test]
    fn folds_nested_control_blocks() -> anyhow::Result<()> {
        let source = r#"PROGRAM{
DECLPROC foo
foo PROC:<{
  IF:<{
    1
  }>ELSE<{
    2
  }>
  IFJMP:<{
    3
  }>
  <{
    4
  }>
}>
END>c
"#;

        let fold_pairs = pairs(source)?;
        assert!(fold_pairs.iter().any(|(start, _)| *start == 3)); // IF
        assert!(fold_pairs.iter().any(|(start, _)| *start == 8)); // IFJMP
        assert!(fold_pairs.iter().any(|(start, _)| *start == 11)); // instruction_block
        Ok(())
    }
}
