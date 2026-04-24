#![cfg(test)]

use std::fs;
use std::path::{Path, PathBuf};
use tolk_resolver::file_db::FileInfo;
use tolk_resolver::project_index::ProjectIndex;
use tolk_resolver::{AstNodeSpanExt, Span, resolve};
use tolk_syntax::ast::{self, Expr};
use tolk_ty::{InferenceResult, TyId, TypeDb, TypeInterner, infer};

#[derive(Debug, Clone)]
struct ExpectTypeCall {
    call_span: Span,
    expr_span: Span,
    expr_text: String,
    expected: String,
}

#[derive(Debug, Clone)]
struct ExpectTypeFailure {
    file: PathBuf,
    line: usize,
    col: usize,
    expr_text: String,
    expected: String,
    actual: String,
    problem: String,
}

#[derive(Debug, Default, Clone, Copy)]
struct FileStats {
    files_checked: usize,
    expectations_checked: usize,
}

fn collect_tolk_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return files;
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            files.extend(collect_tolk_files(&path));
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "tolk") {
            files.push(path);
        }
    }

    files.sort();
    files
}

fn find_expect_type_calls(decl: ast::TopLevel<'_>, source: &str) -> Vec<ExpectTypeCall> {
    fn walk(node: ast::Node<'_>, source: &str, out: &mut Vec<ExpectTypeCall>) {
        if node.kind() == "function_call" {
            let call = ast::Call(node);
            let callee_name = call
                .callee_identifier()
                .and_then(|callee| callee.utf8_text(source.as_bytes()).ok())
                .map(|name| name.trim_matches('`'))
                .unwrap_or_default();

            if callee_name == "__expect_type" {
                let args = call.arguments().collect::<Vec<_>>();
                if args.len() >= 2
                    && let Some(expr) = args[0].expr()
                    && let Some(arg2) = args[1].expr()
                    && let Expr::StringLit(expected) = arg2
                {
                    out.push(ExpectTypeCall {
                        call_span: call.span(),
                        expr_span: expr.span(),
                        expr_text: expr.text(source).to_string(),
                        expected: expected.content(source).to_string(),
                    });
                }
            }
        }

        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            walk(child, source, out);
        }
    }

    let mut calls = Vec::new();
    walk(decl.syntax(), source, &mut calls);
    calls
}

fn offset_to_line_col(file_info: &FileInfo, offset: usize) -> (usize, usize) {
    let line_offsets = file_info.line_offsets();
    let line = line_offsets
        .binary_search(&offset)
        .unwrap_or_else(|idx| idx.saturating_sub(1));
    let line_start = line_offsets.get(line).copied().unwrap_or(0);
    let source: &str = file_info.source().source.as_ref();
    let col = source[line_start..offset].chars().count();
    (line + 1, col + 1)
}

fn classify_problem(actual: &str) -> &'static str {
    if actual == "no-inferred-type" {
        "missing-expression-type"
    } else if actual == "undefined" {
        "exact-undefined"
    } else if actual.contains("undefined") {
        "partial-undefined"
    } else {
        "type-mismatch"
    }
}

fn find_type_for_expectation(result: &InferenceResult, span: Span) -> Option<TyId> {
    if let Some(ty) = result.type_of(span) {
        return Some(ty);
    }

    // Fallback 1: find the tightest inferred span that fully covers the expression span.
    let mut best_covering: Option<(usize, TyId)> = None;
    for (candidate_span, ty) in &result.expression_types {
        if candidate_span.start() <= span.start() && span.end() <= candidate_span.end() {
            let len = candidate_span.len();
            if best_covering.is_none_or(|(best_len, _)| len < best_len) {
                best_covering = Some((len, *ty));
            }
        }
    }
    if let Some((_, ty)) = best_covering {
        return Some(ty);
    }

    // Fallback 2: find the widest inferred span fully inside the expression span.
    let mut best_inside: Option<(usize, TyId)> = None;
    for (candidate_span, ty) in &result.expression_types {
        if span.start() <= candidate_span.start() && candidate_span.end() <= span.end() {
            let len = candidate_span.len();
            if best_inside.is_none_or(|(best_len, _)| len > best_len) {
                best_inside = Some((len, *ty));
            }
        }
    }
    best_inside.map(|(_, ty)| ty)
}

fn short_expr(expr: &str) -> String {
    let single_line = expr.replace(['\n', '\t'], " ");
    let compact = single_line.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.len() <= 80 {
        compact
    } else {
        format!("{}...", &compact[..77])
    }
}

fn markdown_escape(cell: &str) -> String {
    cell.replace('|', "\\|")
}

fn write_failure_report(
    report_path: &Path,
    crate_dir: &Path,
    stats: FileStats,
    failures: &[ExpectTypeFailure],
) {
    let mut out = String::new();
    out.push_str("# Compiler `__expect_type` Failures\n\n");
    out.push_str(&format!(
        "- Files checked: `{}`\n- `__expect_type` checks: `{}`\n- Failures: `{}`\n\n",
        stats.files_checked,
        stats.expectations_checked,
        failures.len()
    ));

    if failures.is_empty() {
        out.push_str("No failures.\n");
        let _ = fs::write(report_path, out);
        return;
    }

    out.push_str("| File | Pos | Expr | Expected | Actual | Problem |\n");
    out.push_str("| --- | --- | --- | --- | --- | --- |\n");

    for failure in failures {
        let file = failure
            .file
            .strip_prefix(crate_dir)
            .unwrap_or(&failure.file)
            .display()
            .to_string();
        out.push_str(&format!(
            "| {} | {}:{} | `{}` | `{}` | `{}` | {} |\n",
            markdown_escape(&file),
            failure.line,
            failure.col,
            markdown_escape(&short_expr(&failure.expr_text)),
            markdown_escape(&failure.expected),
            markdown_escape(&failure.actual),
            failure.problem,
        ));
    }

    let _ = fs::write(report_path, out);
}

fn check_file(file: &Path, stdlib_path: &Path) -> Result<(usize, Vec<ExpectTypeFailure>), String> {
    let root_path = dunce::canonicalize(file)
        .map_err(|e| format!("failed to canonicalize {}: {e}", file.display()))?;
    let file_db = tolk_resolver::file_db::FileDb::new(stdlib_path.to_owned(), None);

    let common_tolk = stdlib_path.join("common.tolk");
    if common_tolk.exists() {
        let _ = file_db.process(&common_tolk);
    }

    let mut index = ProjectIndex::builder(&file_db, root_path.clone())
        .with_stdlib(file_db.stdlib_path().to_owned())
        .build()
        .map_err(|e| format!("failed to build index for {}: {e}", file.display()))?;

    let file_info = file_db
        .get_by_path(&root_path)
        .ok_or_else(|| format!("failed to load {}", file.display()))?;

    resolve(&file_db, &mut index);

    let mut interner = TypeInterner::new();
    let mut type_db = TypeDb::new(&mut interner, &file_db, &index);
    let source: &str = file_info.source().source.as_ref();

    let mut expectations_checked = 0usize;
    let mut failures = Vec::new();

    for decl in file_info.source().top_levels() {
        let Some(index_decl) = file_info.find_declaration(&decl) else {
            continue;
        };

        let inference = infer(&mut type_db, file_info.id(), index_decl.id, &decl);
        let expectations = find_expect_type_calls(decl, source);
        expectations_checked += expectations.len();

        for expectation in expectations {
            let actual = find_type_for_expectation(&inference, expectation.expr_span).map_or_else(
                || "no-inferred-type".to_string(),
                |ty| type_db.intrn.display(ty).to_string(),
            );

            if actual == expectation.expected {
                continue;
            }

            let (line, col) = offset_to_line_col(&file_info, expectation.call_span.start());
            failures.push(ExpectTypeFailure {
                file: root_path.clone(),
                line,
                col,
                expr_text: expectation.expr_text,
                expected: expectation.expected,
                actual: actual.clone(),
                problem: classify_problem(&actual).to_string(),
            });
        }
    }

    for error in index.errors() {
        failures.push(ExpectTypeFailure {
            file: root_path.clone(),
            line: 1,
            col: 1,
            expr_text: "<project-index>".to_string(),
            expected: "<no-errors>".to_string(),
            actual: error.clone(),
            problem: "project-index-error".to_string(),
        });
    }

    Ok((expectations_checked, failures))
}

#[test]
fn compiler_expect_types_match_inference() {
    let crate_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let tests_root = crate_dir.join("tests/compiler-tests/tests");
    let report_path = crate_dir.join("tests/compiler-tests/EXPECT_TYPE_FAILURES.md");
    let stdlib_path = dunce::canonicalize(crate_dir.join("../tolkc/assets/tolk-stdlib"))
        .expect("failed to resolve stdlib path");

    let mut files: Vec<PathBuf> = collect_tolk_files(&tests_root)
        .into_iter()
        .filter(|path| {
            fs::read_to_string(path)
                .map(|content| content.contains("__expect_type("))
                .unwrap_or(false)
        })
        .collect();
    files.sort();

    let mut stats = FileStats::default();
    let mut failures = Vec::new();

    for file in files {
        stats.files_checked += 1;
        match check_file(&file, &stdlib_path) {
            Ok((expectations, mut file_failures)) => {
                stats.expectations_checked += expectations;
                failures.append(&mut file_failures);
            }
            Err(message) => {
                failures.push(ExpectTypeFailure {
                    file: file.clone(),
                    line: 1,
                    col: 1,
                    expr_text: "<runner>".to_string(),
                    expected: "<ok>".to_string(),
                    actual: message,
                    problem: "runner-error".to_string(),
                });
            }
        }
    }

    write_failure_report(&report_path, &crate_dir, stats, &failures);

    if failures.is_empty() {
        return;
    }

    let summary = failures
        .iter()
        .take(15)
        .map(|failure| {
            format!(
                "{}:{}:{} expected `{}`, got `{}`",
                failure.file.display(),
                failure.line,
                failure.col,
                failure.expected,
                failure.actual
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

    panic!(
        "{} compiler `__expect_type` mismatches found.\nSee report: {}\n\n{}",
        failures.len(),
        report_path.display(),
        summary,
    );
}
