use crate::rules::diagnostic::{Annotation, Applicability, Diagnostic, Edit, Fix, Severity};
use crate::rules::violation::{Violation, ViolationMetadata};
use crate::{Checker, FixAvailability};
use tolk_macros::ViolationMetadata;
use tolk_resolver::AstNodeSpanExt;
use tolk_resolver::file_index::{FileId, SymbolId};
use tolk_resolver::resolve_index::Resolved;
use tolk_syntax::AstNode;
use tolk_syntax::ast::expressions::{Bin, Call, Expr};
use tolk_ty::InferenceResult;

/// ### What it does
/// Warns when `send` mode is passed as numeric literal instead of `SEND_MODE_*` constants.
///
/// ### Why is this bad?
/// Numeric send modes are hard to read and easy to misuse.
/// Named constants make intent explicit and reduce mistakes.
///
/// ### Example
/// ```tolk
/// outMsg.send(3);
/// ```
///
/// Use instead:
/// ```tolk
/// outMsg.send(SEND_MODE_PAY_FEES_SEPARATELY + SEND_MODE_IGNORE_ERRORS);
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.1")]
pub struct SendModeLiteral;

impl Violation for SendModeLiteral {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    fn message(&self) -> String {
        "send mode should use SEND_MODE_* constants".to_string()
    }
}

pub fn check_call(
    checker: &mut Checker,
    file_id: FileId,
    call: &Call,
    current_inference: Option<&InferenceResult>,
) -> Option<()> {
    if !is_send_mode_call(checker, file_id, call, current_inference) {
        return None;
    }

    let mode_arg = call.arguments().last()?;
    let mode_expr = mode_arg.expr()?;

    let file = checker.file_db.get_by_id(file_id)?;
    let source = file.source().source.as_ref();

    let replacement = rewrite_mode_expr(&mode_expr, source);
    if !replacement.has_number_literal {
        return None;
    }

    let mut fixes = vec![];
    let mut help = "replace numeric mode literals with `SEND_MODE_*` constants".to_string();

    if replacement.fully_mapped {
        help = "use named `SEND_MODE_*` constants instead of numeric mode literals".to_string();
        fixes.push(Fix {
            message: "replace with SEND_MODE_* constants".to_string(),
            edits: vec![Edit {
                span: mode_expr.span(),
                replacement: replacement.text,
                file_id,
            }],
            applicability: Applicability::Auto,
        });
    }

    let diagnostic = Diagnostic {
        file_id,
        severity: Severity::Warning,
        name: SendModeLiteral::rule().name(),
        code: SendModeLiteral::code().map(|c| c.to_string()),
        message: SendModeLiteral.message(),
        annotations: vec![Annotation {
            span: mode_expr.span(),
            message: Some("numeric send mode literal is used here".to_string()),
            is_primary: true,
            tags: vec![],
        }],
        fixes,
        help: Some(help),
    };
    checker.emit_diagnostic(SendModeLiteral::rule(), diagnostic);

    None
}

fn is_send_mode_call(
    checker: &Checker,
    file_id: FileId,
    call: &Call,
    current_inference: Option<&InferenceResult>,
) -> bool {
    let Some(symbol_id) = resolve_call_symbol(checker, file_id, call, current_inference) else {
        return false;
    };
    let Some(symbol) = checker.type_db.project_index.resolve_symbol(symbol_id) else {
        return false;
    };

    // low-level sendRawMessage(msg, mode)
    if symbol.name.as_ref() == "sendRawMessage" {
        return true;
    }

    if symbol.name.as_ref() != "send" {
        return false;
    }

    // message.send(mode) or net.send(..., mode)
    checker.file_db.is_stdlib_file(symbol_id.file_id)
        || checker.file_db.is_acton_file(symbol_id.file_id)
}

fn resolve_call_symbol(
    checker: &Checker,
    file_id: FileId,
    call: &Call,
    current_inference: Option<&InferenceResult>,
) -> Option<SymbolId> {
    let callee_ident = call.callee_identifier()?;
    let resolve_index = checker.resolve_index_for(file_id);

    if let Some(resolve_index) = resolve_index
        && let Some(name_use) = resolve_index.find_use(callee_ident.start_byte())
        && let Resolved::Global(symbol_id) = name_use.resolved
    {
        return Some(symbol_id);
    }

    if let Some(current_inference) = current_inference
        && let Some(name_use) = current_inference.resolve(callee_ident.span())
        && let Resolved::Global(symbol_id) = name_use.resolved
    {
        return Some(symbol_id);
    }

    None
}

const SEND_MODE_FLAGS: &[(u32, &str)] = &[
    (1, "SEND_MODE_PAY_FEES_SEPARATELY"),
    (2, "SEND_MODE_IGNORE_ERRORS"),
    (16, "SEND_MODE_BOUNCE_ON_ACTION_FAIL"),
    (32, "SEND_MODE_DESTROY"),
    (64, "SEND_MODE_CARRY_ALL_REMAINING_MESSAGE_VALUE"),
    (128, "SEND_MODE_CARRY_ALL_BALANCE"),
    (1024, "SEND_MODE_ESTIMATE_FEE_ONLY"),
];

struct RewrittenMode {
    text: String,
    has_number_literal: bool,
    fully_mapped: bool,
}

fn rewrite_mode_expr(expr: &Expr, source: &str) -> RewrittenMode {
    match expr {
        Expr::NumberLit(lit) => {
            let literal_text = lit.text(source);
            let mapped = parse_int_literal(literal_text).and_then(send_mode_value_to_constants);
            let fully_mapped = mapped.is_some();

            RewrittenMode {
                text: mapped.unwrap_or_else(|| literal_text.to_string()),
                has_number_literal: true,
                fully_mapped,
            }
        }
        Expr::Paren(paren) => {
            if let Some(inner) = paren.inner() {
                let rewritten = rewrite_mode_expr(&inner, source);
                RewrittenMode {
                    text: format!("({})", rewritten.text),
                    has_number_literal: rewritten.has_number_literal,
                    fully_mapped: rewritten.fully_mapped,
                }
            } else {
                RewrittenMode {
                    text: paren.text(source).to_string(),
                    has_number_literal: false,
                    fully_mapped: true,
                }
            }
        }
        Expr::Bin(bin) => rewrite_bin(bin, source),
        _ => RewrittenMode {
            text: expr.text(source).to_string(),
            has_number_literal: false,
            fully_mapped: true,
        },
    }
}

fn rewrite_bin(bin: &Bin, source: &str) -> RewrittenMode {
    let Some(left) = bin.left() else {
        return RewrittenMode {
            text: bin.text(source).to_string(),
            has_number_literal: false,
            fully_mapped: true,
        };
    };
    let Some(right) = bin.right() else {
        return RewrittenMode {
            text: bin.text(source).to_string(),
            has_number_literal: false,
            fully_mapped: true,
        };
    };

    let left_rewritten = rewrite_mode_expr(&left, source);
    let right_rewritten = rewrite_mode_expr(&right, source);
    let has_number_literal =
        left_rewritten.has_number_literal || right_rewritten.has_number_literal;

    if !has_number_literal {
        return RewrittenMode {
            text: bin.text(source).to_string(),
            has_number_literal: false,
            fully_mapped: true,
        };
    }

    if bin.operator_name(source) != "+" {
        return RewrittenMode {
            text: bin.text(source).to_string(),
            has_number_literal: true,
            fully_mapped: false,
        };
    }

    RewrittenMode {
        text: format!("{} + {}", left_rewritten.text, right_rewritten.text),
        has_number_literal: true,
        fully_mapped: left_rewritten.fully_mapped && right_rewritten.fully_mapped,
    }
}

fn parse_int_literal(raw: &str) -> Option<u32> {
    let normalized = raw.replace('_', "");

    if let Some(hex) = normalized
        .strip_prefix("0x")
        .or_else(|| normalized.strip_prefix("0X"))
    {
        return u32::from_str_radix(hex, 16).ok();
    }
    if let Some(binary) = normalized
        .strip_prefix("0b")
        .or_else(|| normalized.strip_prefix("0B"))
    {
        return u32::from_str_radix(binary, 2).ok();
    }

    normalized.parse::<u32>().ok()
}

fn send_mode_value_to_constants(value: u32) -> Option<String> {
    if value == 0 {
        return Some("SEND_MODE_REGULAR".to_string());
    }

    let mut remaining = value;
    let mut parts = Vec::with_capacity(4);

    for &(flag, name) in SEND_MODE_FLAGS {
        if remaining & flag != 0 {
            parts.push(name);
            remaining &= !flag;
        }
    }

    if remaining != 0 || parts.is_empty() {
        return None;
    }

    Some(parts.join(" + "))
}
