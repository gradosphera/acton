use crate::backend::Backend;
use crate::backend::utils::{get_point, offsets_to_lsp_range};
use lsp_types::{Hover, HoverContents, HoverParams, MarkupContent, MarkupKind, Range};
use std::collections::HashMap;
use std::sync::OnceLock;
use tasm::spec::{FiftInstruction, GasConsumptionEntry, SpecInstruction, Specification};
use tower_lsp::jsonrpc::Result as LspResult;
use tree_sitter::{Node, Point};

static TASM_SPEC_INDEX: OnceLock<Option<TasmSpecIndex>> = OnceLock::new();

struct TasmSpecIndex {
    specification: Specification,
    instructions_by_name: HashMap<String, usize>,
    aliases_by_name: HashMap<String, usize>,
}

impl TasmSpecIndex {
    fn load() -> serde_json::Result<Self> {
        let specification = tasm::spec::load_tvm_specification()?;

        let mut instructions_by_name = HashMap::with_capacity(specification.instructions.len());
        for (index, instruction) in specification.instructions.iter().enumerate() {
            instructions_by_name.insert(normalize_instruction_name(&instruction.name), index);
        }

        let mut aliases_by_name = HashMap::with_capacity(specification.fift_instructions.len());
        for (index, alias) in specification.fift_instructions.iter().enumerate() {
            aliases_by_name.insert(normalize_instruction_name(&alias.name), index);
        }

        Ok(Self {
            specification,
            instructions_by_name,
            aliases_by_name,
        })
    }

    fn instruction(&self, name: &str) -> Option<&SpecInstruction> {
        let normalized = normalize_instruction_name(name);
        let index = self.instructions_by_name.get(normalized.as_str())?;
        self.specification.instructions.get(*index)
    }

    fn alias(&self, name: &str) -> Option<&FiftInstruction> {
        let normalized = normalize_instruction_name(name);
        let index = self.aliases_by_name.get(normalized.as_str())?;
        self.specification.fift_instructions.get(*index)
    }
}

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

        let Ok(source_file) = tasm_syntax::parse(&source) else {
            return Ok(None);
        };

        let point = get_point(&source, params.text_document_position_params.position);
        let Some(target) = find_instruction_hover_target(&source_file, &source, point) else {
            return Ok(None);
        };

        let Some(spec_index) = get_spec_index() else {
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

fn get_spec_index() -> Option<&'static TasmSpecIndex> {
    TASM_SPEC_INDEX
        .get_or_init(|| match TasmSpecIndex::load() {
            Ok(index) => Some(index),
            Err(error) => {
                log::error!("failed to load TASM specification for hover: {error}");
                None
            }
        })
        .as_ref()
}

fn normalize_instruction_name(name: &str) -> String {
    name.trim().to_ascii_uppercase()
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

fn build_hover_markdown(name: &str, spec: &TasmSpecIndex) -> Option<String> {
    if let Some(alias) = spec.alias(name) {
        let actual_instruction = spec.instruction(&alias.actual_name);
        return Some(format_alias_markdown(alias, actual_instruction));
    }

    let instruction = spec.instruction(name)?;
    Some(format_instruction_markdown(&instruction.name, instruction))
}

fn format_instruction_markdown(instruction_name: &str, instruction: &SpecInstruction) -> String {
    let stack_info = instruction
        .signature
        .as_ref()
        .and_then(|signature| signature.stack_string.as_ref())
        .filter(|stack| !stack.is_empty())
        .map(|stack| {
            format!(
                "- Stack (top is on the right): `{}`",
                stack.replace("->", "\u{2192}")
            )
        });

    let gas = format_gas_ranges(&instruction.description.gas);
    let operands = format_operands(&instruction.description.operands);

    let raw_short = instruction.description.short.trim();
    let raw_long = instruction.description.long.trim();
    let short = if raw_short.is_empty() {
        raw_long
    } else {
        raw_short
    };
    let details = if raw_long.is_empty() || short == raw_long {
        ""
    } else {
        raw_long
    };

    let mut lines = Vec::new();
    lines.push("```".to_string());
    if operands.is_empty() {
        lines.push(instruction_name.to_string());
    } else {
        lines.push(format!("{instruction_name} {operands}"));
    }
    lines.push("```".to_string());

    if let Some(stack_line) = stack_info {
        lines.push(stack_line);
    }

    lines.push(format!("- Gas: `{gas}`"));
    lines.push(format!("- Opcode: `{}`", instruction.layout.prefix_str));
    lines.push(String::new());

    if !short.is_empty() {
        lines.push(short.to_string());
        lines.push(String::new());
    }

    if !details.is_empty() {
        lines.push("**Details:**".to_string());
        lines.push(String::new());
        lines.push(details.to_string());
        lines.push(String::new());
    }

    lines.join("\n")
}

fn format_alias_markdown(
    alias: &FiftInstruction,
    actual_instruction: Option<&SpecInstruction>,
) -> String {
    let arguments = alias
        .arguments
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(" ");

    let alias_target = if arguments.is_empty() {
        alias.actual_name.clone()
    } else {
        format!("{} {}", alias.actual_name, arguments)
    };

    let mut lines = vec![
        "```".to_string(),
        format!("{} alias of {}", alias.name, alias_target),
        "```".to_string(),
        String::new(),
    ];

    if let Some(description) = alias.description.as_ref().filter(|text| !text.is_empty()) {
        lines.push(description.clone());
        lines.push(String::new());
    }

    if let Some(instruction) = actual_instruction {
        lines.push("---".to_string());
        lines.push(String::new());
        lines.push("Aliased instruction info:".to_string());
        lines.push(String::new());
        lines.push(format_instruction_markdown(&instruction.name, instruction));
    }

    lines.join("\n")
}

fn format_operands(operands: &[String]) -> String {
    operands
        .iter()
        .map(|operand| format!("[{operand}]"))
        .collect::<Vec<_>>()
        .join(" ")
}

fn format_gas_ranges(entries: &[GasConsumptionEntry]) -> String {
    if entries.is_empty() {
        return "N/A".to_string();
    }

    let formula = entries.iter().find(|entry| entry.formula.is_some());
    let non_formula_values: Vec<i64> = entries
        .iter()
        .filter(|entry| entry.formula.is_none())
        .map(|entry| entry.value)
        .collect();

    if non_formula_values.is_empty()
        && let Some(value) = formula.and_then(|entry| entry.formula.as_ref())
    {
        return value.clone();
    }

    let mut sorted_values = non_formula_values;
    sorted_values.sort_unstable();

    let mut result_parts: Vec<String> = Vec::new();
    let mut start_index = 0usize;

    for index in 0..sorted_values.len() {
        let is_last = index + 1 == sorted_values.len();
        let breaks_range = !is_last && sorted_values[index + 1] != sorted_values[index] + 1;
        if is_last || breaks_range {
            if start_index == index {
                result_parts.push(sorted_values[index].to_string());
            } else {
                result_parts.push(format!(
                    "{}-{}",
                    sorted_values[start_index], sorted_values[index]
                ));
            }
            start_index = index + 1;
        }
    }

    let base_gas = result_parts
        .into_iter()
        .filter(|part| part != "36")
        .collect::<Vec<_>>()
        .join(" | ");

    if let Some(value) = formula.and_then(|entry| entry.formula.as_ref()) {
        return format!("{base_gas} + {value}");
    }

    base_gas
}
