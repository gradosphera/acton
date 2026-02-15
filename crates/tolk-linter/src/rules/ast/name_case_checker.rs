use crate::rules::diagnostic::{
    Annotation, Applicability, Diagnostic, DiagnosticTag, Edit, Fix, Severity,
};
use crate::rules::violation::Violation;
use crate::rules::violation::ViolationMetadata;
use crate::{Checker, FixAvailability};
use heck::{ToLowerCamelCase, ToShoutySnakeCase, ToUpperCamelCase};
use tolk_macros::ViolationMetadata;
use tolk_resolver::Symbol;
use tolk_resolver::file_index::FileId;
use tolk_resolver::resolve_index::LocalDefKind;

/// ### What it does
/// Checks identifier naming style and suggests consistent casing.
///
/// ### Why is this bad?
/// Inconsistent naming makes code harder to read and maintain.
/// This rule enforces:
/// - `camelCase` for variables, functions, methods, and struct fields
/// - `PascalCase` for structs, enums, enum members, and type aliases
/// - `SCREAMING_SNAKE_CASE` for constants
///
/// ### Example
/// ```tolk
/// struct low_struct {
///     TheBad: int
/// }
///
/// const iAmConst_variable = 1
///
/// fun BadFunctionName() {}
/// ```
///
/// Use instead:
/// ```tolk
/// struct LowStruct {
///     theBad: int
/// }
///
/// const I_AM_CONST_VARIABLE = 1
///
/// fun badFunctionName() {}
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.1")]
pub struct NameCaseChecker;

impl Violation for NameCaseChecker {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Always;

    fn message(&self) -> String {
        "name should be in the expected case".to_owned()
    }
}

enum CaseRules {
    Camel,
    Pascal,
    ScreamingSnake,
}

fn check_case(symbol: &Symbol, checker: &mut Checker, symbol_def_file_id: FileId, case: CaseRules) {
    if symbol.name.starts_with("_") {
        // internal names
        return;
    }

    let (correct_case, case_name) = match case {
        CaseRules::Camel => (symbol.name.to_lower_camel_case(), "camelCase"),
        CaseRules::Pascal => (symbol.name.to_upper_camel_case(), "PascalCase"),
        CaseRules::ScreamingSnake => (symbol.name.to_shouty_snake_case(), "SCREAMING_SNAKE_CASE"),
    };

    if symbol.name.as_bytes() == correct_case.as_bytes() {
        return;
    }

    let mut edits = vec![
        // definition itself
        Edit {
            span: symbol.name_span,
            replacement: correct_case.clone(),
            file_id: symbol_def_file_id,
        },
    ];

    for (&file_id, index) in &checker.type_db.project_index.resolved_uses {
        for usage in index.global_usages_of(symbol.id) {
            edits.push(Edit {
                span: usage.span,
                replacement: correct_case.clone(),
                file_id,
            });
        }
    }

    // Extra usages resolved only during type inference (e.g. struct literal field keys).
    for (&file_id, file_body_types) in checker.body_types {
        for inference in file_body_types.values() {
            for usage in inference.global_usages_of(symbol.id) {
                edits.push(Edit {
                    span: usage.span,
                    replacement: correct_case.clone(),
                    file_id,
                });
            }
        }
    }

    let diagnostic = Diagnostic {
        file_id: symbol_def_file_id,
        severity: Severity::Warning,
        name: NameCaseChecker::rule().name(),
        code: NameCaseChecker::code().map(|c| c.to_string()),
        message: NameCaseChecker.message(),
        annotations: vec![Annotation {
            span: symbol.name_span,
            message: Some(format!("not {case_name}: `{}`", symbol.name)),
            is_primary: true,
            tags: vec![DiagnosticTag::Unnecessary],
        }],
        fixes: vec![Fix {
            message: format!("rename to {case_name}: {}", correct_case),
            edits,
            applicability: Applicability::Auto,
        }],
        help: None,
    };
    checker.emit_diagnostic(NameCaseChecker::rule(), diagnostic);
}

pub fn check_name_cases(checker: &mut Checker) -> Option<()> {
    // First check local declarations
    for file_id in checker.type_db.project_index.sorted_files() {
        let Some(file_info) = checker.file_db.get_by_id(file_id) else {
            continue;
        };
        if !file_info.is_workspace_file() {
            continue;
        }

        let Some(resolve_index) = checker.resolve_index_for(file_id) else {
            continue;
        };

        for local_def in resolve_index.locals.iter() {
            let name = local_def.name.clone();
            if name.starts_with("_") {
                // don't check explicitly unused symbols
                // we also skip something like `_foo_bar` but I think it's ok
                continue;
            }

            let (correct_case, case_name) = match local_def.kind {
                LocalDefKind::TypeParameter => (name.to_upper_camel_case(), "PascalCase"),
                _ => (name.to_lower_camel_case(), "camelCase"),
            };

            if correct_case.as_bytes() == name.as_bytes() {
                continue;
            }

            let usages = resolve_index.local_usages_of(local_def.id);
            let mut edits = vec![
                // definition itself
                Edit {
                    span: local_def.def_span,
                    replacement: correct_case.clone(),
                    file_id,
                },
            ];

            for usage in usages {
                edits.push(Edit {
                    span: usage.span,
                    replacement: correct_case.clone(),
                    file_id,
                });
            }

            let diagnostic = Diagnostic {
                file_id,
                severity: Severity::Warning,
                name: NameCaseChecker::rule().name(),
                code: NameCaseChecker::code().map(|c| c.to_string()),
                message: NameCaseChecker.message(),
                annotations: vec![Annotation {
                    span: local_def.def_span,
                    message: Some(format!("not {case_name}: {name}",)),
                    is_primary: true,
                    tags: vec![DiagnosticTag::Unnecessary],
                }],
                fixes: vec![Fix {
                    message: format!("rename to {case_name}: {correct_case}"),
                    edits,
                    applicability: Applicability::Auto,
                }],
                help: None,
            };
            checker.emit_diagnostic(NameCaseChecker::rule(), diagnostic);
        }
    }

    // And then global ones
    let globals = checker.type_db.project_index.global_symbols();

    // sort global symbols for stability
    let mut symbol_ids = globals.values().flatten().copied().collect::<Vec<_>>();
    symbol_ids.sort_unstable_by_key(|id| (id.file_id, id.local_id));

    for symbol_id in symbol_ids {
        let Some(file_info) = checker.file_db.get_by_id(symbol_id.file_id) else {
            continue;
        };
        if !file_info.is_workspace_file() {
            continue;
        }

        let Some(symbol) = checker.type_db.project_index.resolve_symbol(symbol_id) else {
            continue;
        };

        match &symbol.kind {
            tolk_resolver::SymbolKind::GetMethod { .. } => {
                // Since the get method name defines the method ID and there are names from TEPs in snake case (e.g. `get_wallet_info`),
                // we cannot warn about the get method names
                continue;
            }
            tolk_resolver::SymbolKind::GlobalVariable
            | tolk_resolver::SymbolKind::Function { .. }
            | tolk_resolver::SymbolKind::StructField
            | tolk_resolver::SymbolKind::Method { .. } => {
                check_case(symbol, checker, file_info.id(), CaseRules::Camel)
            }
            tolk_resolver::SymbolKind::Struct { .. }
            | tolk_resolver::SymbolKind::Enum { .. }
            | tolk_resolver::SymbolKind::EnumMember
            | tolk_resolver::SymbolKind::TypeAlias { .. } => {
                check_case(symbol, checker, file_info.id(), CaseRules::Pascal)
            }
            tolk_resolver::SymbolKind::Constant => {
                check_case(symbol, checker, file_info.id(), CaseRules::ScreamingSnake)
            }
        }
    }

    Some(())
}
