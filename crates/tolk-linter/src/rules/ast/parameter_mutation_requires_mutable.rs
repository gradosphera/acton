use crate::rules::diagnostic::{Annotation, Applicability, Diagnostic, Edit, Fix};
use crate::rules::violation::Violation;
use crate::{Checker, FixAvailability};
use tolk_analysis::UseFlags;
use tolk_macros::ViolationMetadata;
use tolk_resolver::AstNodeSpanExt;
use tolk_resolver::file_index::{FileId, Span};
use tolk_resolver::resolve_index::{LocalDef, LocalDefKind};
use tolk_syntax::{LambdaParameter, Parameter, TryFromNode};

/// ### What it does
/// Reports mutation of parameters that are not declared as `mutate`.
///
/// ### Why is this bad?
/// Reassigning or mutating an immutable parameter is unclear and usually indicates
/// one of two intentions: either the parameter should be mutable, or a local mutable
/// copy should be introduced. Without `mutate`, writes to the parameter do not mutate
/// the original value at the call site.
///
/// ### Example
/// ```tolk
/// fun main(value: int) {
///     value = 10;
/// }
/// ```
///
/// Use one of:
/// ```tolk
/// fun main(mutate value: int) {
///     value = 10;
/// }
/// ```
///
/// ```tolk
/// fun main(value: int) {
///     var valueCopy = value;
///     valueCopy = 10;
/// }
/// ```
#[derive(ViolationMetadata)]
#[violation_metadata(stable_since = "v0.0.1")]
pub struct ParameterMutationRequiresMutable;

impl Violation for ParameterMutationRequiresMutable {
    const FIX_AVAILABILITY: FixAvailability = FixAvailability::Sometimes;

    fn message(&self) -> String {
        "parameter is mutated but is not declared `mutate`".to_string()
    }
}

pub fn check_file(checker: &mut Checker, file_id: FileId) -> Option<()> {
    let file = checker.file_db.get_by_id(file_id)?;
    let source = file.source().source.as_ref();
    let root = file.source().tree.root_node();
    let resolved_index = checker.resolve_index_for(file_id)?;
    let use_facts = checker.use_facts(file_id)?;

    for local in &resolved_index.locals {
        if !matches!(
            local.kind,
            LocalDefKind::Param {
                is_mutable: false,
                is_self: false,
                in_asm_or_builtin: false,
                ..
            }
        ) {
            continue;
        }

        let Some(facts) = use_facts.per_local.get(&local.id) else {
            continue;
        };
        if !facts.flags.contains(UseFlags::WRITE) {
            continue;
        }

        let write_span = facts.first_write_span.unwrap_or(local.def_span);

        let mut fixes = vec![];
        if let Some(mutate_insert_span) = find_mutate_insert_span(root, local.def_span) {
            fixes.push(Fix {
                message: "declare parameter as `mutate`, if you want to mutate the value at the call site"
                    .to_string(),
                edits: vec![Edit {
                    span: mutate_insert_span,
                    replacement: "mutate ".to_string(),
                    file_id,
                }],
                applicability: Applicability::Manual,
            });
        }

        if let Some(copy_fix) =
            build_mutable_copy_fix(source, file.line_offsets(), local, write_span)
        {
            fixes.push(copy_fix);
        }

        let diagnostic = Diagnostic::warning_for(file_id, ParameterMutationRequiresMutable)
            .with_annotations(vec![
                Annotation {
                    span: local.def_span,
                    message: Some("parameter is declared immutable".to_string()),
                    is_primary: false,
                    tags: vec![],
                },
                Annotation {
                    span: write_span,
                    message: Some("parameter is mutated here".to_string()),
                    is_primary: true,
                    tags: vec![],
                },
            ])
            .with_help(
                "without `mutate`, writes to this parameter do not mutate the value at the call site",
            )
            .with_fixes(fixes);
        checker.emit_diagnostic(diagnostic);
    }

    Some(())
}

fn find_mutate_insert_span(root: tree_sitter::Node<'_>, def_span: Span) -> Option<Span> {
    let mut current = root.descendant_for_byte_range(def_span.start(), def_span.end());
    while let Some(node) = current {
        if let Ok(param) = Parameter::try_from_node(node) {
            if param.mutate() {
                return None;
            }
            let offset = param.span().start() as u32;
            return Some(Span {
                start: offset,
                end: offset,
            });
        }
        if let Ok(param) = LambdaParameter::try_from_node(node) {
            if param.mutate() {
                return None;
            }
            let offset = param.span().start() as u32;
            return Some(Span {
                start: offset,
                end: offset,
            });
        }
        current = node.parent();
    }
    None
}

fn build_mutable_copy_fix(
    source: &str,
    line_offsets: &[usize],
    local: &LocalDef,
    first_write_span: Span,
) -> Option<Fix> {
    let param_name = local.name.as_ref();
    if param_name.is_empty() {
        return None;
    }

    let copy_name = make_unique_copy_name(param_name);
    let (line_start, indent) =
        line_start_and_indent(source, line_offsets, first_write_span.start())?;
    let (insert_offset, insert_text) = if line_start > 0
        && source
            .as_bytes()
            .get(line_start - 1)
            .copied()
            .is_some_and(|b| b == b'\n')
    {
        (
            (line_start - 1) as u32,
            format!("\n{indent}var {copy_name} = {param_name};"),
        )
    } else {
        (
            line_start as u32,
            format!("{indent}var {copy_name} = {param_name};\n"),
        )
    };

    let declaration_insert = Edit {
        span: Span {
            start: insert_offset,
            end: insert_offset,
        },
        replacement: insert_text,
        file_id: local.id.file_id,
    };

    let replace_first_write = Edit {
        span: first_write_span,
        replacement: copy_name,
        file_id: local.id.file_id,
    };

    Some(Fix {
        message: "introduce a mutable local copy, if you want to mutate the value locally"
            .to_string(),
        edits: vec![declaration_insert, replace_first_write],
        applicability: Applicability::Manual,
    })
}

fn make_unique_copy_name(base_name: &str) -> String {
    format!("{base_name}Copy")
}

fn line_start_and_indent<'a>(
    source: &'a str,
    line_offsets: &[usize],
    offset: usize,
) -> Option<(usize, &'a str)> {
    let line_idx = match line_offsets.binary_search(&offset) {
        Ok(line) => line,
        Err(0) => 0,
        Err(next_line) => next_line - 1,
    };
    let line_start = *line_offsets.get(line_idx)?;
    let tail = source.get(line_start..)?;
    let indent_width = tail
        .as_bytes()
        .iter()
        .take_while(|b| **b == b' ' || **b == b'\t')
        .count();
    let indent = source.get(line_start..line_start + indent_width)?;
    Some((line_start, indent))
}
