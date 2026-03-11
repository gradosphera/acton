use crate::backend::Backend;
use crate::backend::utils::offsets_to_lsp_range;
use crate::languages::instruction_docs::{
    InstructionDocsIndex, get_instruction_docs_index, stack_effect_title,
};
use lsp_types::{CodeLens, CodeLensParams, Command};
use tasm_syntax::{Argument, AstNode, Code, Dictionary, Expr, TopLevel};
use tower_lsp::jsonrpc::Result as LspResult;

pub const STACK_EFFECT_CODE_LENS_COMMAND: &str = "tonls.tasm.stackEffect";

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

        let instruction_docs = get_instruction_docs_index();
        let mut lenses = Vec::new();
        for top_level in source_file.top_levels() {
            collect_top_level(top_level, &source, instruction_docs, &mut lenses);
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

fn collect_top_level(
    top_level: TopLevel<'_>,
    source: &str,
    instruction_docs: Option<&InstructionDocsIndex>,
    lenses: &mut Vec<CodeLens>,
) {
    match top_level {
        TopLevel::Instruction(node) => {
            push_instruction_code_lens(node, source, instruction_docs, lenses);
            for arg in node.args() {
                collect_argument(arg, source, instruction_docs, lenses);
            }
        }
        TopLevel::ExplicitRef(node) => {
            if let Some(code) = node.code() {
                collect_code(code, source, instruction_docs, lenses);
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
    instruction_docs: Option<&InstructionDocsIndex>,
    lenses: &mut Vec<CodeLens>,
) {
    if let Some(expr) = argument.expr() {
        collect_expr(expr, source, instruction_docs, lenses);
    }
}

fn collect_expr(
    expr: Expr<'_>,
    source: &str,
    instruction_docs: Option<&InstructionDocsIndex>,
    lenses: &mut Vec<CodeLens>,
) {
    match expr {
        Expr::Code(code) => collect_code(code, source, instruction_docs, lenses),
        Expr::Dictionary(dictionary) => {
            collect_dictionary(dictionary, source, instruction_docs, lenses)
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
    instruction_docs: Option<&InstructionDocsIndex>,
    lenses: &mut Vec<CodeLens>,
) {
    if let Some(instructions) = code.instructions() {
        for top_level in instructions.items() {
            collect_top_level(top_level, source, instruction_docs, lenses);
        }
    }
}

fn collect_dictionary(
    dictionary: Dictionary<'_>,
    source: &str,
    instruction_docs: Option<&InstructionDocsIndex>,
    lenses: &mut Vec<CodeLens>,
) {
    for entry in dictionary.entries() {
        if let Some(code) = entry.code() {
            collect_code(code, source, instruction_docs, lenses);
        }
    }
}

fn push_instruction_code_lens(
    instruction: tasm_syntax::Instruction<'_>,
    source: &str,
    instruction_docs: Option<&InstructionDocsIndex>,
    lenses: &mut Vec<CodeLens>,
) {
    let Some(name_node) = instruction.name() else {
        return;
    };

    let instruction_name = name_node.text(source).trim();
    if instruction_name.is_empty() {
        return;
    }

    let title = stack_effect_title(instruction_name, instruction_docs);
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
