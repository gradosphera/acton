use crate::commands::common::error_fmt;
use acton_config::color::OwoColorize;
use acton_config::config::{
    ActonConfig, CheckOutputFormat, ContractConfig, LintLevel,
    project_root as configured_project_root,
};
use anyhow::anyhow;
use globset::{Glob, GlobSet, GlobSetBuilder};
use rayon::prelude::*;
use rustc_hash::FxHashSet;
use std::collections::{HashMap, HashSet};
use std::ffi::OsStr;
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::time::Instant;
use std::{fs, io};
use tolk_linter::diagnostic::{Annotation, Applicability, Diagnostic, Severity};
use tolk_linter::{Checker, Linter, Rule, Tolk};
use tolk_resolver::file_db::FileDb;
use tolk_resolver::file_index::{FileId, Span};
use tolk_resolver::project_index::ProjectIndex;
use tolk_resolver::symbol_resolver::resolve;
use tolk_ty::TypeDb;
use tolk_ty::TypeInterner;
use tolk_ty::infer;
use walkdir::WalkDir;

mod check_explain;
mod check_list;
mod compiler;
mod fix;
mod output;
mod pos;
mod render;

pub(super) struct LintExcludes {
    project_root: PathBuf,
    patterns: Vec<String>,
    excludes: GlobSet,
}

struct CheckRunOptions<'a> {
    fix: bool,
    is_plain_report: bool,
    project_root: &'a Path,
    acton_config: &'a ActonConfig,
    excludes: &'a LintExcludes,
    only_rules: Option<&'a HashSet<Rule>>,
}

struct RootCheckInput {
    root: PathBuf,
    lint_settings: HashMap<Rule, LintLevel>,
}

struct PreparedRootCheck {
    root: PathBuf,
    root_file_id: FileId,
    lint_settings: HashMap<Rule, LintLevel>,
    base_diagnostics: Vec<Diagnostic>,
}

impl LintExcludes {
    fn from_config(project_root: &Path, config: &ActonConfig) -> anyhow::Result<Self> {
        let patterns = config
            .lint
            .as_ref()
            .and_then(|lint| lint.exclude.clone())
            .unwrap_or_default();

        let mut exclude_builder = GlobSetBuilder::new();
        for pattern in &patterns {
            exclude_builder.add(Glob::new(pattern)?);
        }

        Ok(Self {
            project_root: project_root.to_path_buf(),
            patterns,
            excludes: exclude_builder.build()?,
        })
    }

    fn is_match(&self, path: &Path) -> bool {
        if self.patterns.is_empty() {
            return false;
        }

        let absolute = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.project_root.join(path)
        };
        let relative =
            pathdiff::diff_paths(&absolute, &self.project_root).unwrap_or_else(|| absolute.clone());

        self.excludes.is_match(&relative) || self.excludes.is_match(&absolute)
    }

    fn is_match_file_id(&self, file_db: &FileDb, file_id: FileId) -> bool {
        let Some(info) = file_db.get_by_id(file_id) else {
            return false;
        };
        self.is_match(info.path())
    }
}

fn diagnostics_summary(diagnostics: &[Diagnostic]) -> (usize, usize) {
    let mut error_count = 0;
    let mut warning_count = 0;

    for diagnostic in diagnostics {
        match diagnostic.severity {
            Severity::Warning => warning_count += 1,
            Severity::Error | Severity::Fatal => error_count += 1,
            Severity::Info | Severity::Help => {}
        }
    }

    (error_count, warning_count)
}

struct DiagnosticsStatus {
    error_count: usize,
    warning_count: usize,
    warning_limit_exceeded: bool,
}

impl DiagnosticsStatus {
    const fn is_success(&self) -> bool {
        self.error_count == 0 && !self.warning_limit_exceeded
    }
}

fn diagnostics_status(diagnostics: &[Diagnostic], max_warnings: usize) -> DiagnosticsStatus {
    let (error_count, warning_count) = diagnostics_summary(diagnostics);

    DiagnosticsStatus {
        error_count,
        warning_count,
        warning_limit_exceeded: warning_count > max_warnings,
    }
}

pub fn check_cmd(
    fix: bool,
    cli_output_format: Option<CheckOutputFormat>,
    output_file: Option<PathBuf>,
    enable_only: Option<Vec<String>>,
    explain: Option<String>,
    list_lint_rules: bool,
    target: Option<String>,
) -> anyhow::Result<()> {
    if list_lint_rules {
        return check_list::check_list_cmd();
    }
    if let Some(code) = explain {
        return check_explain::check_explain_cmd(&code);
    }

    let config = ActonConfig::load()?;
    let output_format = cli_output_format
        .or_else(|| {
            config
                .lint
                .as_ref()
                .and_then(|lint| lint.output_format.clone())
        })
        .unwrap_or(CheckOutputFormat::Plain);
    let is_plain_report = output_format == CheckOutputFormat::Plain;
    if is_plain_report && output_file.is_some() {
        anyhow::bail!("output_file cannot be used with plain output format")
    }

    let max_warnings = config
        .lint
        .as_ref()
        .map_or(usize::MAX, |lint| lint.max_warnings);

    let project_root = configured_project_root().to_path_buf();
    let excludes = LintExcludes::from_config(&project_root, &config)?;
    let only_rules = parse_rules_filter(enable_only)?;
    let run_options = CheckRunOptions {
        fix,
        is_plain_report,
        project_root: &project_root,
        acton_config: &config,
        excludes: &excludes,
        only_rules: only_rules.as_ref(),
    };

    let now = Instant::now();
    let files = find_files(&project_root)?;
    log::info!("found {} files in {:?}", files.len(), now.elapsed());

    let stdlib = find_stdlib(&project_root)?;
    let acton_stdlib = find_acton_stdlib(&project_root)?;
    let common_tolk = stdlib.join("common.tolk");

    let file_db = FileDb::new(stdlib, Some(acton_stdlib));

    // We need stdlib for all targets so preprocess it before all.
    if common_tolk.exists() {
        file_db.process(&common_tolk)?;
    }

    let mut all_diagnostics = Vec::new();

    if let Some(target) = target {
        if target.ends_with(".tolk") {
            let contract_diagnostics = check_test_file(Path::new(&target), &file_db, &run_options)?;
            all_diagnostics.extend(contract_diagnostics);
        } else {
            let contract = config
                .get_contract(&target)
                .ok_or_else(|| anyhow!(error_fmt::contract_not_found(&config, &target)))?;
            let contract_diagnostics = check_contract(&target, contract, &file_db, &run_options)?;
            all_diagnostics.extend(contract_diagnostics);
        }
    } else {
        let mut root_inputs = Vec::new();
        let mut seen_roots = HashSet::new();

        let contracts = config.contracts().cloned().unwrap_or_default();
        for (contract_id, contract) in contracts {
            if excludes.is_match(Path::new(&contract.src)) {
                continue;
            }
            if !contract.src.ends_with(".tolk") {
                continue;
            }

            if is_plain_report {
                println!("    {} {}", "Checking".green().bold(), contract.name);
            }

            let source_path = Path::new(&contract.src);
            let source_path = if source_path.is_absolute() {
                source_path.to_path_buf()
            } else {
                project_root.join(source_path)
            };
            let root = dunce::canonicalize(source_path)?;
            if !seen_roots.insert(root.clone()) {
                continue;
            }

            let lint_settings = Checker::build_settings(&config, Some(&contract_id));
            let lint_settings = apply_rules_filter(lint_settings, only_rules.as_ref());
            root_inputs.push(RootCheckInput {
                root,
                lint_settings,
            });
        }

        for file in files {
            let Some(name) = file.file_name() else {
                continue;
            };
            if name.to_string_lossy().ends_with(".test.tolk") && !excludes.is_match(&file) {
                let root = dunce::canonicalize(&file)?;
                if !seen_roots.insert(root.clone()) {
                    continue;
                }

                let current_dir = std::env::current_dir().unwrap_or_default();
                let relative_root =
                    pathdiff::diff_paths(&root, &current_dir).unwrap_or_else(|| root.clone());
                if is_plain_report {
                    println!(
                        "    {} {}",
                        "Checking".green().bold(),
                        relative_root.display()
                    );
                }

                let mut lint_settings = Checker::build_settings(&config, None);
                lint_settings.insert(Rule::ActonImportInContract, LintLevel::Allow);
                lint_settings.insert(Rule::RandomRequiresInitialization, LintLevel::Allow);
                lint_settings.insert(Rule::DivideBeforeMultiply, LintLevel::Allow);
                let lint_settings = apply_rules_filter(lint_settings, only_rules.as_ref());
                root_inputs.push(RootCheckInput {
                    root,
                    lint_settings,
                });
            }
        }

        let diagnostics = check_roots(
            root_inputs,
            &file_db,
            fix,
            is_plain_report,
            &config,
            &excludes,
        )?;
        all_diagnostics.extend(diagnostics);
    }

    // Deduplicate all diagnostic for JSON output to avoid duplicate errors in IDEs
    let all_diagnostics = all_diagnostics
        .into_iter()
        .collect::<HashSet<_>>()
        .into_iter()
        .collect::<Vec<_>>();
    let status = diagnostics_status(&all_diagnostics, max_warnings);

    let mut writer: Box<dyn Write> = match output_file {
        Some(path) => {
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }

            let file = fs::File::create(path)?;
            Box::new(BufWriter::new(file))
        }
        None => Box::new(BufWriter::new(io::stdout())),
    };

    match output_format {
        CheckOutputFormat::Plain => {
            show_plain_report(fix, max_warnings, &all_diagnostics, &file_db)?;
        }
        CheckOutputFormat::Json => {
            output::json::write_report(
                &mut writer,
                status.is_success(),
                &all_diagnostics,
                &file_db,
            )?;
        }
        CheckOutputFormat::Sarif => {
            output::sarif::write_report(&mut writer, &all_diagnostics, &file_db, &project_root)?;
        }
        CheckOutputFormat::Github => {
            output::github::write_report(&mut writer, &all_diagnostics, &file_db, &project_root)?;
        }
        CheckOutputFormat::Gitlab => {
            output::gitlab::write_report(&mut writer, &all_diagnostics, &file_db, &project_root)?;
        }
    }

    if output_format != CheckOutputFormat::Plain {
        writer.flush()?;

        if !status.is_success() {
            std::process::exit(1);
        }
    }

    Ok(())
}

fn parse_rules_filter(rule_codes: Option<Vec<String>>) -> anyhow::Result<Option<HashSet<Rule>>> {
    let Some(rule_codes) = rule_codes else {
        return Ok(None);
    };

    let mut rules = HashSet::new();
    for rule_code in rule_codes {
        let rule = parse_rule_selector(&rule_code)?;
        rules.insert(rule);
    }

    if rules.is_empty() {
        anyhow::bail!("--enable-only requires at least one rule code");
    }

    Ok(Some(rules))
}

fn parse_rule_selector(rule_code: &str) -> anyhow::Result<Rule> {
    let code = rule_code.trim();
    if code.is_empty() {
        anyhow::bail!("rule code cannot be empty");
    }

    if let Some(rule) = parse_exact_rule_code(code) {
        return Ok(rule);
    }

    anyhow::bail!("Unknown rule code: {rule_code}");
}

fn parse_exact_rule_code(code: &str) -> Option<Rule> {
    let selector = Tolk::from_str(code).ok()?;
    let mut rules = selector.rules();
    let rule = rules.next()?;
    if rules.next().is_some() {
        return None;
    }
    Some(rule)
}

fn apply_rules_filter(
    mut lint_settings: HashMap<Rule, LintLevel>,
    selected_rules: Option<&HashSet<Rule>>,
) -> HashMap<Rule, LintLevel> {
    let Some(selected_rules) = selected_rules else {
        return lint_settings;
    };

    for rule in Linter::Tolk.all_rules() {
        if selected_rules.contains(&rule) {
            if matches!(lint_settings.get(&rule), Some(LintLevel::Allow)) {
                lint_settings.remove(&rule);
            }
        } else {
            lint_settings.insert(rule, LintLevel::Allow);
        }
    }

    lint_settings
}

fn show_plain_report(
    fix: bool,
    max_warnings: usize,
    all_diagnostics: &[Diagnostic],
    file_db: &FileDb,
) -> anyhow::Result<()> {
    if fix {
        fix::apply_fixes(file_db, all_diagnostics)?;
    }

    let mut shown_diagnostics = if fix {
        fix::filter_fixed_diagnostics(all_diagnostics)
    } else {
        Vec::from(all_diagnostics)
    };
    let status = diagnostics_status(&shown_diagnostics, max_warnings);

    if !shown_diagnostics.is_empty() {
        shown_diagnostics.sort();
        let first_code = shown_diagnostics
            .iter()
            .find(|d| d.code.is_some())
            .and_then(|d| d.code.clone());

        let mut printed_autofix_notice = false;
        if !fix {
            let count_to_autofix = shown_diagnostics
                .iter()
                .filter(|d| {
                    d.fixes
                        .iter()
                        .any(|f| f.applicability == Applicability::Auto)
                })
                .count();

            if count_to_autofix > 0 {
                let issue_word = if count_to_autofix == 1 {
                    "issue"
                } else {
                    "issues"
                };

                eprintln!();
                eprintln!(
                    "{count_to_autofix} {issue_word} can be fixed automatically, rerun with {} flag.",
                    "--fix".yellow()
                );
                printed_autofix_notice = true;
            }
        }

        if status.warning_limit_exceeded {
            if !printed_autofix_notice {
                eprintln!();
            }
            eprintln!(
                "Warning limit exceeded: {} {} (max-warnings = {}).",
                status.warning_count,
                if status.warning_count == 1 {
                    "warning"
                } else {
                    "warnings"
                },
                max_warnings
            );
        }

        if let Some(code) = first_code {
            eprintln!();
            eprintln!(
                "Use {} to get detailed explanation of a rule.",
                "acton check --explain <CODE>".yellow()
            );
            eprintln!("For example: acton check --explain {code}");
        }
    }

    if !status.is_success() {
        std::process::exit(1);
    }
    Ok(())
}

fn find_stdlib(project_root: &Path) -> anyhow::Result<PathBuf> {
    let path_to_stdlib = project_root.join(".acton/tolk-stdlib");
    if !path_to_stdlib.exists() {
        anyhow::bail!(
            "cannot find Tolk stdlib in .acton/, did you run {}?",
            "acton init".yellow()
        );
    }

    Ok(dunce::canonicalize(path_to_stdlib)?)
}

fn find_acton_stdlib(project_root: &Path) -> anyhow::Result<PathBuf> {
    let path_to_acton = project_root.join(".acton");
    if !path_to_acton.exists() {
        anyhow::bail!(
            "cannot find Acton in .acton/, did you run {}?",
            "acton init".yellow()
        );
    }

    Ok(dunce::canonicalize(path_to_acton)?)
}

fn check_contract(
    contract_id: &str,
    config: &ContractConfig,
    file_db: &FileDb,
    options: &CheckRunOptions<'_>,
) -> anyhow::Result<Vec<Diagnostic>> {
    if !config.src.ends_with(".tolk") {
        // skip contracts with .boc sources
        return Ok(vec![]);
    }

    if options.is_plain_report {
        println!("    {} {}", "Checking".green().bold(), config.name,);
    }

    let source_path = Path::new(&config.src);
    let source_path = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        options.project_root.join(source_path)
    };
    let root = dunce::canonicalize(source_path)?;
    let lint_settings = Checker::build_settings(options.acton_config, Some(contract_id));
    let lint_settings = apply_rules_filter(lint_settings, options.only_rules);

    check_roots(
        vec![RootCheckInput {
            root,
            lint_settings,
        }],
        file_db,
        options.fix,
        options.is_plain_report,
        options.acton_config,
        options.excludes,
    )
}

fn check_test_file(
    file: &Path,
    file_db: &FileDb,
    options: &CheckRunOptions<'_>,
) -> anyhow::Result<Vec<Diagnostic>> {
    let root = dunce::canonicalize(file)?;
    let current_dir = std::env::current_dir().unwrap_or_default();
    let relative_root = pathdiff::diff_paths(&root, &current_dir).unwrap_or_else(|| root.clone());

    if options.is_plain_report {
        println!(
            "    {} {}",
            "Checking".green().bold(),
            relative_root.display()
        );
    }

    let mut lint_settings = Checker::build_settings(options.acton_config, None);
    // we can import any files in tests
    lint_settings.insert(Rule::ActonImportInContract, LintLevel::Allow);
    // random is not so important in tests
    lint_settings.insert(Rule::RandomRequiresInitialization, LintLevel::Allow);
    // division is not so important in tests
    lint_settings.insert(Rule::DivideBeforeMultiply, LintLevel::Allow);
    let lint_settings = apply_rules_filter(lint_settings, options.only_rules);

    check_roots(
        vec![RootCheckInput {
            root,
            lint_settings,
        }],
        file_db,
        options.fix,
        options.is_plain_report,
        options.acton_config,
        options.excludes,
    )
}

fn prepare_root_check(
    input: RootCheckInput,
    file_db: &FileDb,
    acton_config: &ActonConfig,
) -> anyhow::Result<PreparedRootCheck> {
    let file_info = file_db.process(&input.root)?;
    let file_source = file_info.source().source.clone();

    let mut base_diagnostics = vec![];

    let has_compiler_errors =
        compiler::check_with_compiler(&input.root, file_db, acton_config, &mut base_diagnostics)?;

    let parse_errors = file_info.source().errors();

    if has_compiler_errors {
        // don't possibly duplicate parsing errors if we have compiler errors
        for parse_error in parse_errors {
            let start_byte = pos::byte_offset_from_point(&parse_error.span.start, &file_source);
            let end_byte = pos::byte_offset_from_point(&parse_error.span.end, &file_source);

            let diagnostic = Diagnostic {
                file_id: file_info.id(),
                severity: Severity::Error,
                code: None,
                rule: Rule::CompilerError,
                name: "parse-error",
                message: parse_error.message.clone(),
                annotations: vec![Annotation {
                    span: Span {
                        start: start_byte as u32,
                        end: end_byte as u32,
                    },
                    message: None,
                    is_primary: true,
                    tags: vec![],
                }],
                fixes: vec![],
                help: None,
            };
            base_diagnostics.push(diagnostic);
        }
    }

    Ok(PreparedRootCheck {
        root: input.root,
        root_file_id: file_info.id(),
        lint_settings: input.lint_settings,
        base_diagnostics,
    })
}

fn check_roots(
    roots: Vec<RootCheckInput>,
    file_db: &FileDb,
    fix: bool,
    is_plain_report: bool,
    acton_config: &ActonConfig,
    excludes: &LintExcludes,
) -> anyhow::Result<Vec<Diagnostic>> {
    if roots.is_empty() {
        return Ok(Vec::new());
    }

    let prepared_roots = roots
        .into_par_iter()
        .map(|root| prepare_root_check(root, file_db, acton_config))
        .collect::<anyhow::Result<Vec<_>>>()?;

    let now = Instant::now();
    let mappings = acton_config.mappings();
    let root_paths = prepared_roots.iter().map(|r| r.root.clone()).collect();
    let mut index = ProjectIndex::builder_for_roots(file_db, root_paths)
        .with_stdlib(file_db.stdlib_path().to_owned())
        .with_mappings(&mappings)
        .build()?;
    log::debug!("Build shared project index took {:?}", now.elapsed());
    log::debug!("Shared index files: {}", index.files().len());

    let now = Instant::now();
    resolve(file_db, &mut index);
    log::debug!("Resolve shared project took {:?}", now.elapsed());

    let mut root_scopes: HashMap<FileId, FxHashSet<FileId>> = HashMap::new();
    let mut files_to_infer: FxHashSet<FileId> = FxHashSet::default();
    for root in &prepared_roots {
        let reachable = index.reachable_files(root.root_file_id);
        let scope_files: FxHashSet<FileId> = reachable.into_iter().collect();
        files_to_infer.extend(scope_files.iter().copied());
        root_scopes.insert(root.root_file_id, scope_files);
    }

    let now = Instant::now();
    let mut interner = TypeInterner::new();
    let mut type_db = TypeDb::new(&mut interner, file_db, &index);
    let mut body_types = HashMap::new();
    let mut infer_files = files_to_infer.into_iter().collect::<Vec<_>>();
    infer_files.sort_unstable();
    for file_id in infer_files {
        let Some(file_to_infer) = file_db.get_by_id(file_id) else {
            continue;
        };
        let mut file_body_types = HashMap::new();

        for decl in file_to_infer.source().top_levels() {
            let Some(index_decl) = file_to_infer.find_declaration(&decl) else {
                continue;
            };

            let res = infer(&mut type_db, file_to_infer.id(), index_decl.id, &decl);
            file_body_types.insert(index_decl.id, res);
        }

        body_types.insert(file_id, file_body_types);
    }
    log::debug!("Infer shared types took {:?}", now.elapsed());

    let now = Instant::now();
    let mut all_diagnostics = Vec::new();
    let project_root = configured_project_root().to_path_buf();

    for root in prepared_roots {
        let scope_files = root_scopes
            .get(&root.root_file_id)
            .cloned()
            .unwrap_or_default();

        let mut checker = Checker::new(file_db, &type_db, &body_types)
            .with_settings(root.lint_settings)
            .with_project_root(project_root.clone())
            .with_scope_files(scope_files.clone());

        checker.run_once();

        let mut files_to_check = scope_files.iter().copied().collect::<Vec<_>>();
        files_to_check.sort_unstable();
        for file_id in files_to_check {
            let Some(info) = file_db.get_by_id(file_id) else {
                continue;
            };
            if !info.is_workspace_file() {
                continue;
            }
            if info.id() != root.root_file_id && excludes.is_match(info.path()) {
                continue;
            }

            checker.process_file(info.source(), info.id());
        }

        checker.apply_suppressions();

        #[cfg(feature = "profile_rules")]
        {
            checker.print_profiling_results();
        }

        let mut diagnostics = checker.diagnostics;
        diagnostics.extend(root.base_diagnostics);
        diagnostics.retain(|diagnostic| {
            diagnostic.rule == Rule::CompilerError
                || diagnostic.file_id == root.root_file_id
                || (scope_files.contains(&diagnostic.file_id)
                    && !excludes.is_match_file_id(file_db, diagnostic.file_id))
        });

        if is_plain_report {
            let diagnostics_to_show = if fix {
                fix::filter_fixed_diagnostics(&diagnostics)
            } else {
                diagnostics.clone()
            };
            let _ = render::emit_diagnostics(file_db, &diagnostics_to_show);
        }

        all_diagnostics.extend(diagnostics);
    }

    log::debug!("Run shared diagnostics in {:?}", now.elapsed());
    Ok(all_diagnostics)
}

fn find_files(root: &Path) -> anyhow::Result<Vec<PathBuf>> {
    const EXCLUDED_DIRS: &[&str] = &[
        ".git",
        ".github",
        ".idea",
        ".acton",
        "node_modules",
        "target",
        "tolk-stdlib",
        ".codex",
        ".claude",
    ];

    let mut exclude_builder = GlobSetBuilder::new();
    for p in [
        // ... for future ignoring via flags
    ] {
        exclude_builder.add(Glob::new(p)?);
    }
    let excludes = exclude_builder.build()?;

    let it = WalkDir::new(root)
        .follow_links(false)
        .into_iter()
        .filter_entry(|entry| {
            if !entry.file_type().is_dir() {
                return true;
            }
            let name = entry.file_name();
            if EXCLUDED_DIRS.iter().any(|d| name == OsStr::new(d)) {
                // fast path
                return false;
            }

            let p = entry.path();
            let rel = p.strip_prefix(root).unwrap_or(p);
            !excludes.is_match(rel)
        });

    let mut out: Vec<PathBuf> = Vec::with_capacity(32);

    for entry in it {
        let entry = match entry {
            Ok(e) => e,
            Err(err) => {
                log::warn!("walk dir error: {err}");
                continue;
            }
        };

        if entry.file_type().is_file() {
            let path = entry.path();

            if let Some(ext) = path.extension() {
                if ext != "tolk" {
                    continue;
                }
            } else {
                continue;
            }

            let rel = path.strip_prefix(root).unwrap_or(path);
            if excludes.is_match(rel) {
                continue;
            }

            out.push(path.to_path_buf());
        }
    }

    out.sort_unstable();
    Ok(out)
}
