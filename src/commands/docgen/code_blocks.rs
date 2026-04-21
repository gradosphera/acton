use anyhow::{Context, Result, anyhow};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

const REPO_DOCS_ROOT: &str = "docs/content/docs";
const WRAPPED_SNIPPET_NAME: &str = "__acton_docgen_snippet__";
const WRAPPED_ANNOTATION_NAME: &str = "__acton_docgen_annotation__";
const GENERATED_DOC_SUBDIRS: &[&str] = &[
    "commands",
    "standard_library",
    "tolk_standard_library",
    "linting/rules",
];

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeBlockInfo {
    language: Option<String>,
    tags: BTreeSet<String>,
}

impl CodeBlockInfo {
    fn has_tag(&self, tag: &str) -> bool {
        self.tags.contains(tag)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeBlock {
    info: CodeBlockInfo,
    code: String,
    start_line: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ValidationContext {
    treat_unlabeled_blocks_as_tolk: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct CodeBlockFailure {
    line: usize,
    message: String,
}

pub(super) fn report_tolk_code_block_issues(output_paths: &super::DocgenOutputPaths) -> Result<()> {
    let mut failures = Vec::new();

    failures.extend(validate_doc_dir(
        &output_paths.command_docs_out_dir,
        ValidationContext {
            treat_unlabeled_blocks_as_tolk: false,
        },
    )?);
    failures.extend(validate_doc_dir(
        &output_paths.stdlib_out_dir,
        ValidationContext {
            // Generated stdlib pages inherit many unlabeled fences from Tolk doc comments.
            treat_unlabeled_blocks_as_tolk: true,
        },
    )?);
    failures.extend(validate_doc_dir(
        &output_paths.linter_out_dir,
        ValidationContext {
            treat_unlabeled_blocks_as_tolk: false,
        },
    )?);
    failures.extend(validate_handwritten_docs()?);

    emit_warning_report(&failures);
    Ok(())
}

fn emit_warning_report(failures: &[(PathBuf, usize, CodeBlockFailure)]) {
    if failures.is_empty() {
        return;
    }

    const MAX_PRINTED_FAILURES: usize = 50;

    eprintln!(
        "Warning: found {} Tolk code blocks in documentation that do not pass parse/format \
checks yet.",
        failures.len()
    );
    eprintln!(
        "These checks are currently reported without failing `acton docgen` so the existing docs \
can be cleaned up incrementally."
    );
    eprintln!(
        "Use fence tags such as `parse_fail`, `no_fmt`, or `ignore` for intentional exceptions."
    );

    for (path, line, failure) in failures.iter().take(MAX_PRINTED_FAILURES) {
        eprintln!("- {}:{}: {}", path.display(), line, failure.message);
    }

    if failures.len() > MAX_PRINTED_FAILURES {
        eprintln!(
            "... and {} more documentation code block issue(s).",
            failures.len() - MAX_PRINTED_FAILURES
        );
    }
}

fn validate_handwritten_docs() -> Result<Vec<(PathBuf, usize, CodeBlockFailure)>> {
    let root = Path::new(REPO_DOCS_ROOT);
    if !root.exists() {
        return Ok(Vec::new());
    }

    let mut paths = Vec::new();
    for entry in WalkDir::new(root)
        .into_iter()
        .filter_entry(|entry| !should_skip_generated_docs_subtree(entry.path(), root))
        .filter_map(std::result::Result::ok)
    {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "mdx") {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();

    let mut failures = Vec::new();
    for path in paths {
        failures.extend(validate_doc_file(
            &path,
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        )?);
    }

    Ok(failures)
}

fn should_skip_generated_docs_subtree(path: &Path, docs_root: &Path) -> bool {
    let Ok(relative) = path.strip_prefix(docs_root) else {
        return false;
    };

    GENERATED_DOC_SUBDIRS
        .iter()
        .any(|generated| relative.starts_with(Path::new(generated)))
}

fn validate_doc_dir(
    dir: &Path,
    context: ValidationContext,
) -> Result<Vec<(PathBuf, usize, CodeBlockFailure)>> {
    let mut failures = Vec::new();
    if !dir.exists() {
        return Ok(failures);
    }

    let mut paths = Vec::new();
    for entry in WalkDir::new(dir).into_iter().filter_map(std::result::Result::ok) {
        if entry.file_type().is_file() && entry.path().extension().is_some_and(|ext| ext == "mdx") {
            paths.push(entry.path().to_path_buf());
        }
    }
    paths.sort();

    for path in paths {
        failures.extend(validate_doc_file(&path, context)?);
    }

    Ok(failures)
}

fn validate_doc_file(
    path: &Path,
    context: ValidationContext,
) -> Result<Vec<(PathBuf, usize, CodeBlockFailure)>> {
    let source = fs::read_to_string(path)
        .with_context(|| format!("Failed to read documentation file {}", path.display()))?;
    let failures = validate_doc_source(&source, context);

    Ok(failures
        .into_iter()
        .map(|failure| (path.to_path_buf(), failure.line, failure))
        .collect())
}

fn validate_doc_source(source: &str, context: ValidationContext) -> Vec<CodeBlockFailure> {
    let mut failures = Vec::new();

    for block in extract_code_blocks(source) {
        if !should_validate_code_block(&block.info, context) {
            continue;
        }
        if block.info.has_tag("ignore") || block.info.has_tag("skip") {
            continue;
        }

        let normalized_source = normalize_code_block_source(&block.code);
        if normalized_source.is_empty() {
            continue;
        }
        if should_skip_placeholder_block(&normalized_source) {
            continue;
        }

        if let Err(message) = validate_tolk_code_block(&block.info, &normalized_source) {
            failures.push(CodeBlockFailure {
                line: block.start_line,
                message,
            });
        }
    }

    failures
}

fn should_validate_code_block(info: &CodeBlockInfo, context: ValidationContext) -> bool {
    match info.language.as_deref() {
        Some("tolk") => true,
        Some(_) => false,
        None => context.treat_unlabeled_blocks_as_tolk,
    }
}

fn validate_tolk_code_block(info: &CodeBlockInfo, source: &str) -> Result<(), String> {
    let expects_parse_fail = info.has_tag("parse_fail");
    let skip_format_check = info.has_tag("no_fmt")
        || info.has_tag("no_format")
        || info.has_tag("unformatted");

    match analyze_tolk_code_block(source) {
        Ok(_formatted) if expects_parse_fail => {
            Err("expected this block to fail parsing, but it parsed successfully".to_owned())
        }
        Ok(formatted) => {
            if skip_format_check {
                return Ok(());
            }

            if normalize_for_comparison(&formatted) != normalize_for_comparison(source) {
                return Err(format!(
                    "block is not formatted. Expected:\n```tolk\n{formatted}\n```"
                ));
            }

            Ok(())
        }
        Err(_parse_error) if expects_parse_fail => Ok(()),
        Err(parse_error) => Err(parse_error),
    }
}

fn analyze_tolk_code_block(source: &str) -> Result<String, String> {
    if let Some(formatted) = try_format_function_signature_fragment(source)? {
        return Ok(formatted);
    }

    if let Some(formatted) = try_format_annotation_fragment(source)? {
        return Ok(formatted);
    }

    if parse_without_errors(source).is_ok() {
        return tolkfmt::format_source(source, tolkfmt::FormatOptions::default())
            .map(|formatted| normalize_code_block_source(&formatted))
            .map_err(|error| format!("failed to format block: {error}"));
    }

    let wrapped = wrap_tolk_snippet(source);
    if parse_without_errors(&wrapped).is_ok() {
        let formatted_wrapped = tolkfmt::format_source(&wrapped, tolkfmt::FormatOptions::default())
            .map_err(|error| format!("failed to format block: {error}"))?;
        let formatted = unwrap_tolk_snippet(&formatted_wrapped)
            .map_err(|error| format!("failed to unwrap formatted snippet: {error}"))?;
        return Ok(normalize_code_block_source(&formatted));
    }

    let parse_as_file_error = parse_without_errors(source).unwrap_err();
    let parse_as_snippet_error = parse_without_errors(&wrapped).unwrap_err();
    Err(format!(
        "failed to parse block as a Tolk file ({parse_as_file_error}) or as a Tolk snippet \
inside a function body ({parse_as_snippet_error})"
    ))
}

fn try_format_function_signature_fragment(source: &str) -> Result<Option<String>, String> {
    let trimmed = source.trim();
    if !(trimmed.starts_with("fun ") || trimmed.starts_with("get fun ")) {
        return Ok(None);
    }
    // Signature fragments can still contain `{}` inside parameter defaults like
    // `params: SearchParams = {}`, so only treat asm declarations as non-body fragments here.
    if trimmed.contains(" asm ") {
        return Ok(None);
    }

    let wrapped = format!("{trimmed} {{}}");
    if parse_without_errors(&wrapped).is_err() {
        return Ok(None);
    }

    let formatted = tolkfmt::format_source(&wrapped, tolkfmt::FormatOptions::default())
        .map_err(|error| format!("failed to format block: {error}"))?;
    let formatted = formatted.trim_end_matches('\n');
    let Some(signature) = formatted.strip_suffix(" {}") else {
        return Err(format!(
            "failed to unwrap formatted function signature fragment `{formatted}`"
        ));
    };
    Ok(Some(normalize_code_block_source(signature)))
}

fn try_format_annotation_fragment(source: &str) -> Result<Option<String>, String> {
    if !looks_like_annotation_fragment(source) {
        return Ok(None);
    }

    let wrapped = format!("{source}\nfun {WRAPPED_ANNOTATION_NAME}() {{}}\n");
    if parse_without_errors(&wrapped).is_err() {
        return Ok(None);
    }

    let formatted = tolkfmt::format_source(&wrapped, tolkfmt::FormatOptions::default())
        .map_err(|error| format!("failed to format block: {error}"))?;
    let formatted = formatted.trim_end_matches('\n');
    let trailer = format!("fun {WRAPPED_ANNOTATION_NAME}() {{}}");
    let Some(prefix) = formatted.strip_suffix(&trailer) else {
        return Err(format!(
            "failed to unwrap formatted annotation fragment `{formatted}`"
        ));
    };

    Ok(Some(trim_blank_lines(prefix)))
}

fn looks_like_annotation_fragment(source: &str) -> bool {
    let mut saw_annotation = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            continue;
        }

        if !trimmed.starts_with('@') {
            return false;
        }

        saw_annotation = true;
    }

    saw_annotation
}

fn parse_without_errors(source: &str) -> Result<(), String> {
    let parsed =
        tolk_syntax::parse(source).map_err(|error| format!("failed to initialize parser: {error}"))?;
    if !parsed.has_errors() {
        return Ok(());
    }

    let mut errors = parsed.errors().into_iter();
    let Some(first_error) = errors.next() else {
        return Err("unknown syntax error".to_owned());
    };

    Err(format!(
        "{} at {}:{}",
        first_error.message,
        first_error.span.start.row + 1,
        first_error.span.start.column + 1
    ))
}

fn wrap_tolk_snippet(source: &str) -> String {
    let mut wrapped = format!("fun {WRAPPED_SNIPPET_NAME}() {{\n");
    for line in source.lines() {
        if line.is_empty() {
            wrapped.push('\n');
        } else {
            wrapped.push_str("    ");
            wrapped.push_str(line);
            wrapped.push('\n');
        }
    }
    wrapped.push_str("}\n");
    wrapped
}

fn unwrap_tolk_snippet(formatted_wrapped: &str) -> Result<String> {
    let lines: Vec<_> = formatted_wrapped.lines().collect();
    if lines.is_empty() {
        return Ok(String::new());
    }

    let expected_header = format!("fun {WRAPPED_SNIPPET_NAME}() {{");
    if lines.first().copied() != Some(expected_header.as_str()) {
        return Err(anyhow!(
            "unexpected formatted wrapper header `{}`",
            lines.first().copied().unwrap_or_default()
        ));
    }
    if lines.last().copied() != Some("}") {
        return Err(anyhow!(
            "unexpected formatted wrapper footer `{}`",
            lines.last().copied().unwrap_or_default()
        ));
    }

    let mut body = Vec::new();
    for line in &lines[1..lines.len().saturating_sub(1)] {
        if line.is_empty() {
            body.push(String::new());
            continue;
        }

        let Some(stripped) = line.strip_prefix("    ") else {
            return Err(anyhow!("unexpected indentation in formatted wrapper body: `{line}`"));
        };
        body.push(stripped.to_owned());
    }

    Ok(body.join("\n"))
}

fn normalize_code_block_source(source: &str) -> String {
    let dedented = dedent_block(source);
    trim_blank_lines(
        &dedented
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn normalize_for_comparison(source: &str) -> String {
    trim_blank_lines(
        &source
            .lines()
            .map(str::trim_end)
            .collect::<Vec<_>>()
            .join("\n"),
    )
}

fn trim_blank_lines(source: &str) -> String {
    source
        .trim_matches('\n')
        .lines()
        .collect::<Vec<_>>()
        .join("\n")
}

fn should_skip_placeholder_block(source: &str) -> bool {
    source.contains("...")
        || (source.trim_start().starts_with('@') && source.contains('<') && source.contains('>'))
}

fn dedent_block(source: &str) -> String {
    let lines: Vec<_> = source.lines().collect();
    let min_indent = lines
        .iter()
        .filter(|line| !line.trim().is_empty())
        .map(|line| line.len() - line.trim_start().len())
        .min()
        .unwrap_or(0);

    lines
        .into_iter()
        .map(|line| {
            if line.trim().is_empty() {
                String::new()
            } else {
                line[min_indent.min(line.len())..].to_owned()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn extract_code_blocks(source: &str) -> Vec<CodeBlock> {
    #[derive(Debug)]
    struct OpenBlock {
        fence_len: usize,
        info: CodeBlockInfo,
        start_line: usize,
        lines: Vec<String>,
    }

    let mut blocks = Vec::new();
    let mut open_block: Option<OpenBlock> = None;

    for (index, line) in source.lines().enumerate() {
        let line_number = index + 1;
        let trimmed = line.trim_start();

        if let Some(current) = &mut open_block {
            if trimmed
                .chars()
                .take(current.fence_len)
                .all(|ch| ch == '`')
                && trimmed.starts_with(&"`".repeat(current.fence_len))
            {
                let finished = open_block.take().expect("open block should exist");
                blocks.push(CodeBlock {
                    info: finished.info,
                    code: finished.lines.join("\n"),
                    start_line: finished.start_line,
                });
            } else {
                current.lines.push(line.to_owned());
            }
            continue;
        }

        let fence_len = trimmed.chars().take_while(|&ch| ch == '`').count();
        if fence_len >= 3 {
            let rest = &trimmed[fence_len..];
            open_block = Some(OpenBlock {
                fence_len,
                info: parse_code_block_info(rest),
                start_line: line_number + 1,
                lines: Vec::new(),
            });
        }
    }

    blocks
}

fn parse_code_block_info(info: &str) -> CodeBlockInfo {
    let mut language = None;
    let mut tags = BTreeSet::new();

    let trimmed = info.trim();
    if trimmed.is_empty() {
        return CodeBlockInfo { language, tags };
    }

    let mut tokens = trimmed.split_whitespace();
    if let Some(first_token) = tokens.next() {
        parse_info_token(first_token, &mut language, &mut tags);
    }

    for token in tokens {
        parse_info_token(token, &mut language, &mut tags);
    }

    CodeBlockInfo { language, tags }
}

fn parse_info_token(
    token: &str,
    language: &mut Option<String>,
    tags: &mut BTreeSet<String>,
) {
    if token.is_empty() || token.contains('=') || token.contains('"') || token.contains('\'') {
        return;
    }

    for (index, part) in token.split(',').filter(|part| !part.is_empty()).enumerate() {
        let normalized = normalize_info_part(part);
        if normalized.is_empty() {
            continue;
        }

        if index == 0 && language.is_none() && !looks_like_tag(&normalized) {
            *language = Some(normalized);
        } else {
            tags.insert(normalized);
        }
    }
}

fn normalize_info_part(part: &str) -> String {
    part.trim().to_ascii_lowercase().replace('-', "_")
}

fn looks_like_tag(part: &str) -> bool {
    matches!(
        part,
        "ignore"
            | "skip"
            | "parse_fail"
            | "compile_fail"
            | "no_compile"
            | "no_fmt"
            | "no_format"
            | "unformatted"
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ValidationContext, analyze_tolk_code_block, extract_code_blocks, parse_code_block_info,
        validate_doc_source,
    };

    #[test]
    fn parses_language_tags_and_ignores_attrs() {
        let info = parse_code_block_info(r#"tolk,parse_fail,no-fmt title="example.tolk""#);

        assert_eq!(info.language.as_deref(), Some("tolk"));
        assert!(info.tags.contains("parse_fail"));
        assert!(info.tags.contains("no_fmt"));
    }

    #[test]
    fn extracts_fenced_blocks_with_content_start_line() {
        let source = "before\n```tolk\nval x = 1;\n```\nafter\n";
        let blocks = extract_code_blocks(source);

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].start_line, 3);
        assert_eq!(blocks[0].code, "val x = 1;");
    }

    #[test]
    fn validates_statement_snippets_by_wrapping_them_in_a_function() {
        let formatted = analyze_tolk_code_block("val answer=41 + 1;").expect("snippet should work");

        assert_eq!(formatted, "val answer = 41 + 1;");
    }

    #[test]
    fn validates_function_signature_fragments() {
        let formatted = analyze_tolk_code_block("fun build(name: string, path: string = \"\"): cell")
            .expect("signature fragment should work");

        assert_eq!(formatted, "fun build(name: string, path: string = \"\"): cell");
    }

    #[test]
    fn validates_annotation_fragments() {
        let formatted = analyze_tolk_code_block("@test.skip")
            .expect("annotation fragment should work");

        assert_eq!(formatted, "@test.skip");
    }

    #[test]
    fn reports_unformatted_blocks_by_default() {
        let failures = validate_doc_source(
            "```tolk\nval answer=41 + 1;\n```\n",
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        );

        assert_eq!(failures.len(), 1);
        assert!(failures[0].message.contains("block is not formatted"));
    }

    #[test]
    fn allows_explicitly_unformatted_blocks() {
        let failures = validate_doc_source(
            "```tolk,no_fmt\nval answer=41 + 1;\n```\n",
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        );

        assert!(failures.is_empty());
    }

    #[test]
    fn allows_expected_parse_fail_blocks() {
        let failures = validate_doc_source(
            "```tolk,parse_fail\nfun broken( {\n```\n",
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        );

        assert!(failures.is_empty());
    }

    #[test]
    fn treats_unlabeled_blocks_as_tolk_only_when_requested() {
        let source = "```\nval answer=41 + 1;\n```\n";

        let handwritten_failures = validate_doc_source(
            source,
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        );
        let stdlib_failures = validate_doc_source(
            source,
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: true,
            },
        );

        assert!(handwritten_failures.is_empty());
        assert_eq!(stdlib_failures.len(), 1);
    }

    #[test]
    fn dedents_mdx_blocks_before_validation() {
        let failures = validate_doc_source(
            "        ```tolk\n        val answer = 41 + 1;\n        ```\n",
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        );

        assert!(failures.is_empty());
    }

    #[test]
    fn skips_placeholder_blocks_with_ellipsis() {
        let failures = validate_doc_source(
            "```tolk\ncreateMessage({ ... }).send(SEND_MODE_REGULAR);\n```\n",
            ValidationContext {
                treat_unlabeled_blocks_as_tolk: false,
            },
        );

        assert!(failures.is_empty());
    }

    #[test]
    fn supports_quadruple_backtick_fences() {
        let blocks = extract_code_blocks("````tolk\nval answer = 42;\n````\n");

        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].info.language.as_deref(), Some("tolk"));
    }
}
