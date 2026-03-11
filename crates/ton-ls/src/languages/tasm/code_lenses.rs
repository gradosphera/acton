use crate::backend::Backend;
use crate::backend::utils::offsets_to_lsp_range;
use lsp_types::{CodeLens, CodeLensParams, Command};
use std::collections::HashMap;
use std::sync::OnceLock;
use tasm_syntax::{Argument, AstNode, Code, Dictionary, Expr, TopLevel};
use tower_lsp::jsonrpc::Result as LspResult;

pub const STACK_EFFECT_CODE_LENS_COMMAND: &str = "tonls.tasm.stackEffect";

static STACK_EFFECT_INDEX: OnceLock<Option<HashMap<String, String>>> = OnceLock::new();

impl Backend {
    pub async fn handle_tasm_code_lens(
        &self,
        params: CodeLensParams,
    ) -> LspResult<Option<Vec<CodeLens>>> {
        crate::profile!(self, "tasm-code-lens");
        let now = std::time::Instant::now();

        let uri = params.text_document.uri;
        log::info!("Request: tasm code_lens for {}", uri);

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

        let stack_effect_index = get_stack_effect_index();
        let mut lenses = Vec::new();
        for top_level in source_file.top_levels() {
            collect_top_level(top_level, &source, stack_effect_index, &mut lenses);
        }

        lenses.sort_by_key(|lens| (lens.range.start.line, lens.range.start.character));

        log::info!(
            "Response: tasm code_lens took {:?}, found {} lenses",
            now.elapsed(),
            lenses.len()
        );
        Ok(Some(lenses))
    }
}

fn get_stack_effect_index() -> Option<&'static HashMap<String, String>> {
    STACK_EFFECT_INDEX
        .get_or_init(|| match load_stack_effect_index() {
            Ok(index) => Some(index),
            Err(error) => {
                log::error!("failed to load TASM specification for code lenses: {error}");
                None
            }
        })
        .as_ref()
}

fn load_stack_effect_index() -> serde_json::Result<HashMap<String, String>> {
    let specification = tasm::spec::load_tvm_specification()?;

    let mut instruction_stack_effects = HashMap::with_capacity(specification.instructions.len());
    let mut stack_effects = HashMap::with_capacity(
        specification.instructions.len() + specification.fift_instructions.len(),
    );

    for instruction in &specification.instructions {
        let Some(stack_string) = instruction
            .signature
            .as_ref()
            .and_then(|signature| signature.stack_string.as_ref())
            .map(|stack| stack.trim())
            .filter(|stack| !stack.is_empty())
        else {
            continue;
        };

        let normalized_name = instruction.name.clone();
        let formatted_stack = format_stack_effect(stack_string);
        instruction_stack_effects.insert(normalized_name.clone(), formatted_stack.clone());
        stack_effects.insert(normalized_name, formatted_stack);
    }

    for alias in &specification.fift_instructions {
        let alias_name = alias.name.clone();
        let actual_name = &alias.actual_name;
        if let Some(stack_effect) = instruction_stack_effects.get(actual_name) {
            stack_effects.insert(alias_name, stack_effect.clone());
        }
    }

    Ok(stack_effects)
}

fn format_stack_effect(effect: &str) -> String {
    effect.replace("->", "\u{2192}")
}

fn stack_effect_title(
    instruction_name: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
) -> String {
    stack_effect_index
        .and_then(|index| index.get(instruction_name))
        .cloned()
        .unwrap_or_else(|| "N/A".to_string())
        .replace(":Any", "")
        .replace(":", ": ")
}

fn collect_top_level(
    top_level: TopLevel<'_>,
    source: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
    lenses: &mut Vec<CodeLens>,
) {
    match top_level {
        TopLevel::Instruction(node) => {
            push_instruction_code_lens(node, source, stack_effect_index, lenses);
            for arg in node.args() {
                collect_argument(arg, source, stack_effect_index, lenses);
            }
        }
        TopLevel::ExplicitRef(node) => {
            if let Some(code) = node.code() {
                collect_code(code, source, stack_effect_index, lenses);
            }
        }
        TopLevel::EmbedSlice(_) => {}
        TopLevel::Exotic(_) => {}
        TopLevel::Unmapped(_) => {}
    }
}

fn collect_argument(
    argument: Argument<'_>,
    source: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
    lenses: &mut Vec<CodeLens>,
) {
    if let Some(expr) = argument.expr() {
        collect_expr(expr, source, stack_effect_index, lenses);
    }
}

fn collect_expr(
    expr: Expr<'_>,
    source: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
    lenses: &mut Vec<CodeLens>,
) {
    match expr {
        Expr::Code(code) => collect_code(code, source, stack_effect_index, lenses),
        Expr::Dictionary(dictionary) => {
            collect_dictionary(dictionary, source, stack_effect_index, lenses)
        }
        Expr::IntegerLit(_)
        | Expr::DataLiteral(_)
        | Expr::StackElement(_)
        | Expr::ControlRegister(_)
        | Expr::Unmapped(_) => {}
    }
}

fn collect_code(
    code: Code<'_>,
    source: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
    lenses: &mut Vec<CodeLens>,
) {
    if let Some(instructions) = code.instructions() {
        for top_level in instructions.items() {
            collect_top_level(top_level, source, stack_effect_index, lenses);
        }
    }
}

fn collect_dictionary(
    dictionary: Dictionary<'_>,
    source: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
    lenses: &mut Vec<CodeLens>,
) {
    for entry in dictionary.entries() {
        if let Some(code) = entry.code() {
            collect_code(code, source, stack_effect_index, lenses);
        }
    }
}

fn push_instruction_code_lens(
    instruction: tasm_syntax::Instruction<'_>,
    source: &str,
    stack_effect_index: Option<&HashMap<String, String>>,
    lenses: &mut Vec<CodeLens>,
) {
    let Some(name_node) = instruction.name() else {
        return;
    };

    let instruction_name = name_node.text(source).trim();
    if instruction_name.is_empty() {
        return;
    }

    let title = stack_effect_title(instruction_name, stack_effect_index);
    let range = offsets_to_lsp_range(
        name_node.syntax().start_byte(),
        name_node.syntax().end_byte(),
        source,
    );

    lenses.push(CodeLens {
        range,
        command: Some(Command {
            title,
            command: STACK_EFFECT_CODE_LENS_COMMAND.to_string(),
            arguments: None,
        }),
        data: None,
    });
}
