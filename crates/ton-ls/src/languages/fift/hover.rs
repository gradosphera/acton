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
    pub async fn handle_fift_hover(&self, params: HoverParams) -> LspResult<Option<Hover>> {
        crate::profile!(self, "fift-hover");
        let now = std::time::Instant::now();

        let uri = params.text_document_position_params.text_document.uri;
        log::info!("Request: fift hover for {}", uri);

        let Some(snapshot) = self.registry.find_fift_file(&uri) else {
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

        log::info!("Response: fift hover took {:?}", now.elapsed());
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
    snapshot: &ParsedSnapshot<fift_syntax::SourceFile>,
    position: lsp_types::Position,
) -> Option<HoverTarget> {
    let source = snapshot.text.as_ref();
    let source_file = snapshot.source_file.as_ref();
    let point = get_point(source, position);
    find_hover_target(source_file, source, point)
}

fn find_hover_target(
    source_file: &fift_syntax::SourceFile,
    source: &str,
    point: Point,
) -> Option<HoverTarget> {
    let root = source_file.root_node();
    let node = node_at_position(root, point)?;

    let raw_name = node.utf8_text(source.as_bytes()).ok()?.trim();
    if raw_name.is_empty() {
        return None;
    }

    let name = adjusted_hover_name(node, source).unwrap_or_else(|| raw_name.to_string());

    let range = offsets_to_lsp_range(node.start_byte(), node.end_byte(), source);
    Some(HoverTarget { name, range })
}

fn node_at_position(root: Node<'_>, point: Point) -> Option<Node<'_>> {
    root.descendant_for_point_range(point, point)
}

fn adjusted_hover_name(node: Node<'_>, source: &str) -> Option<String> {
    let instruction = enclosing_instruction(node)?;
    let name_node = instruction.named_child(0)?;
    let instruction_name = name_node.utf8_text(source.as_bytes()).ok()?.trim();
    if instruction_name.is_empty() {
        return None;
    }

    let args = collect_inline_argument_nodes(instruction, source);
    Some(adjust_name(instruction_name, &args, source))
}

fn enclosing_instruction(mut node: Node<'_>) -> Option<Node<'_>> {
    loop {
        if node.kind() == "instruction" {
            return Some(node);
        }
        node = node.parent()?;
    }
}

fn collect_inline_argument_nodes<'tree>(
    instruction: Node<'tree>,
    source: &str,
) -> Vec<Node<'tree>> {
    let mut args_reversed = Vec::new();
    let mut next_start = instruction.start_byte();
    let mut sibling = instruction.prev_named_sibling();

    while let Some(current) = sibling {
        if current.kind() != "instruction" {
            break;
        }
        if contains_line_break(source, current.end_byte(), next_start) {
            break;
        }

        if let Some(argument_node) = current.named_child(0) {
            args_reversed.push(argument_node);
        }

        next_start = current.start_byte();
        sibling = current.prev_named_sibling();
    }

    args_reversed.reverse();
    args_reversed
}

fn contains_line_break(source: &str, start: usize, end: usize) -> bool {
    let Some(slice) = source.get(start..end) else {
        return true;
    };
    slice.bytes().any(|byte| matches!(byte, b'\n' | b'\r'))
}

fn adjust_name(name: &str, args: &[Node<'_>], source: &str) -> String {
    let name = name.trim().to_ascii_uppercase();

    if name == "PUSHINT" {
        if args.is_empty() {
            return "PUSHINT_4".to_string();
        }

        let arg = args
            .first()
            .and_then(|node| node.utf8_text(source.as_bytes()).ok())
            .map(str::trim)
            .and_then(|text| text.parse::<i64>().ok());

        let Some(arg) = arg else {
            return "PUSHINT_4".to_string();
        };

        if (0..=15).contains(&arg) {
            return "PUSHINT_4".to_string();
        }
        if (-128..=127).contains(&arg) {
            return "PUSHINT_8".to_string();
        }
        if (-32_768..=32_767).contains(&arg) {
            return "PUSHINT_16".to_string();
        }

        return "PUSHINT_LONG".to_string();
    }

    if name == "PUSH" {
        if args.len() == 1 && is_stack_register(args[0], source) {
            return "PUSH".to_string();
        }
        if args.len() == 2 {
            return "PUSH2".to_string();
        }
        if args.len() == 3 {
            return "PUSH3".to_string();
        }
        return name;
    }

    if name == "XCHG0" {
        return "XCHG_0I".to_string();
    }

    if name == "XCHG" {
        return "XCHG_IJ".to_string();
    }

    name
}

fn is_stack_register(node: Node<'_>, source: &str) -> bool {
    if node.kind() == "stack_ref" {
        return true;
    }

    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return false;
    };

    let text = text.trim();
    let Some(rest) = text.strip_prefix('s').or_else(|| text.strip_prefix('S')) else {
        return false;
    };

    let digits = rest.strip_prefix('-').unwrap_or(rest);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}
