use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use clap::{Args, ValueEnum};
use quick_junit::{NonSuccessKind, Report, TestCaseStatus};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Instant;
use tempfile::tempdir;

const SCHEMA_NAME: &str = "acton.template-ci.v1";
const SCHEMA_VERSION: u8 = 1;
const BUILD_ARTIFACTS_DIR: &str = "build";
const TEST_RESULTS_DIR: &str = "test-results";
const JUNIT_REPORT_NAME: &str = "junit-results.xml";

#[derive(Args)]
pub(crate) struct TemplateStatsArgs {
    #[arg(long, value_enum, default_value_t = TemplateKind::Jetton)]
    pub(crate) template: TemplateKind,
    #[arg(long, value_name = "PATH", default_value = "acton")]
    pub(crate) acton_bin: PathBuf,
    #[arg(long, value_name = "PATH", default_value = "template-stats.jsonl")]
    pub(crate) output: PathBuf,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, ValueEnum)]
pub(crate) enum TemplateKind {
    Empty,
    Counter,
    Jetton,
}

impl TemplateKind {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Empty => "empty",
            Self::Counter => "counter",
            Self::Jetton => "jetton",
        }
    }
}

#[derive(Debug, Serialize)]
struct StatsRecord {
    schema: &'static str,
    schema_version: u8,
    template: &'static str,
    phase: &'static str,
    status: &'static str,
    acton_version: String,
    project_name: String,
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    duration_ms: u128,
    exit_code: Option<i32>,
    command: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    error: Option<String>,
    ci: CiMetadata,
    details: serde_json::Value,
}

#[derive(Debug, Default, Serialize)]
struct CiMetadata {
    #[serde(skip_serializing_if = "Option::is_none")]
    repository: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    ref_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    git_ref: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    sha: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    workflow: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    job: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    event_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    actor: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    run_attempt: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runner_os: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    runner_arch: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    target_name: Option<String>,
}

impl CiMetadata {
    fn collect() -> Self {
        Self {
            repository: env_var("GITHUB_REPOSITORY"),
            ref_name: env_var("GITHUB_REF_NAME"),
            git_ref: env_var("GITHUB_REF"),
            sha: env_var("GITHUB_SHA"),
            workflow: env_var("GITHUB_WORKFLOW"),
            job: env_var("GITHUB_JOB"),
            event_name: env_var("GITHUB_EVENT_NAME"),
            actor: env_var("GITHUB_ACTOR"),
            run_id: env_var("GITHUB_RUN_ID"),
            run_attempt: env_var("GITHUB_RUN_ATTEMPT"),
            runner_os: env_var("RUNNER_OS"),
            runner_arch: env_var("RUNNER_ARCH"),
            target_name: env_var("TARGET_NAME"),
        }
    }
}

#[derive(Debug)]
struct CommandOutcome {
    started_at: DateTime<Utc>,
    finished_at: DateTime<Utc>,
    duration_ms: u128,
    exit_code: Option<i32>,
    error: Option<String>,
}

impl CommandOutcome {
    fn success(&self) -> bool {
        self.error.is_none() && self.exit_code == Some(0)
    }

    fn status(&self) -> &'static str {
        if self.success() { "success" } else { "failure" }
    }
}

#[derive(Debug, Deserialize)]
struct BuildArtifactFile {
    hash: String,
}

#[derive(Debug, Serialize)]
struct BuildArtifactSummary {
    contract_id: String,
    hash: String,
}

#[derive(Debug, Serialize)]
struct BuildPhaseDetails {
    artifact_dir: &'static str,
    contract_count: usize,
    contracts: Vec<BuildArtifactSummary>,
}

#[derive(Debug, Serialize)]
struct TestCaseSummary {
    suite_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suite_file_path: Option<String>,
    test_name: String,
    status: &'static str,
    duration_ms: u128,
    #[serde(skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct TestSuiteSummary {
    suite_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    suite_file_path: Option<String>,
    case_count: usize,
    passed: usize,
    failed: usize,
    errors: usize,
    skipped: usize,
    duration_ms: u128,
}

#[derive(Debug, Serialize)]
struct TestPhaseDetails {
    junit_path: String,
    report_found: bool,
    suite_count: usize,
    case_count: usize,
    passed: usize,
    failed: usize,
    errors: usize,
    skipped: usize,
    duration_ms: u128,
    suites: Vec<TestSuiteSummary>,
    cases: Vec<TestCaseSummary>,
}

pub(crate) fn run(args: TemplateStatsArgs) -> Result<()> {
    initialize_output_file(&args.output)?;

    let acton_bin = resolve_program_path(&args.acton_bin)?;
    let acton_version = read_acton_version(&acton_bin)?;
    let ci = CiMetadata::collect();
    let template = args.template.as_str();
    let project_name = format!("{template}-template-ci");
    let temp_dir =
        tempdir().context("failed to create a temporary directory for template stats")?;
    let project_dir = temp_dir.path().join(&project_name);

    let scaffold_command = vec![
        "new".to_owned(),
        project_name.clone(),
        "--template".to_owned(),
        template.to_owned(),
        "--name".to_owned(),
        format!("{template}-ci"),
        "--description".to_owned(),
        format!("{template} template smoke test"),
        "--license".to_owned(),
        "MIT".to_owned(),
    ];
    let scaffold_outcome = run_command(&acton_bin, temp_dir.path(), &scaffold_command);
    append_record(
        &args.output,
        StatsRecord {
            schema: SCHEMA_NAME,
            schema_version: SCHEMA_VERSION,
            template,
            phase: "scaffold",
            status: scaffold_outcome.status(),
            acton_version: acton_version.clone(),
            project_name: project_name.clone(),
            started_at: scaffold_outcome.started_at,
            finished_at: scaffold_outcome.finished_at,
            duration_ms: scaffold_outcome.duration_ms,
            exit_code: scaffold_outcome.exit_code,
            command: command_for_record(&acton_bin, &scaffold_command),
            error: scaffold_outcome.error.clone(),
            ci: CiMetadata::collect(),
            details: serde_json::json!({
                "project_name": project_name,
                "template_path": project_name,
            }),
        },
    )?;
    ensure_phase_success("scaffold", &scaffold_outcome)?;

    let build_command = vec!["build".to_owned(), "--color".to_owned(), "never".to_owned()];
    let build_outcome = run_command(&acton_bin, &project_dir, &build_command);
    let (build_details, build_details_error) = collect_build_details(&project_dir);
    append_record(
        &args.output,
        StatsRecord {
            schema: SCHEMA_NAME,
            schema_version: SCHEMA_VERSION,
            template,
            phase: "build",
            status: build_outcome.status(),
            acton_version: acton_version.clone(),
            project_name: project_name.clone(),
            started_at: build_outcome.started_at,
            finished_at: build_outcome.finished_at,
            duration_ms: build_outcome.duration_ms,
            exit_code: build_outcome.exit_code,
            command: command_for_record(&acton_bin, &build_command),
            error: combine_errors(build_outcome.error.clone(), build_details_error.clone()),
            ci: CiMetadata::collect(),
            details: serde_json::to_value(build_details)
                .context("failed to serialize build phase details")?,
        },
    )?;
    ensure_no_details_error("build", build_details_error)?;
    ensure_phase_success("build", &build_outcome)?;

    let test_command = vec![
        "test".to_owned(),
        "--color".to_owned(),
        "never".to_owned(),
        "--reporter".to_owned(),
        "junit".to_owned(),
        "--junit-path".to_owned(),
        TEST_RESULTS_DIR.to_owned(),
        "--junit-merge".to_owned(),
    ];
    let test_outcome = run_command(&acton_bin, &project_dir, &test_command);
    let (test_details, test_details_error) = collect_test_details(&project_dir);
    append_record(
        &args.output,
        StatsRecord {
            schema: SCHEMA_NAME,
            schema_version: SCHEMA_VERSION,
            template,
            phase: "test",
            status: test_outcome.status(),
            acton_version,
            project_name,
            started_at: test_outcome.started_at,
            finished_at: test_outcome.finished_at,
            duration_ms: test_outcome.duration_ms,
            exit_code: test_outcome.exit_code,
            command: command_for_record(&acton_bin, &test_command),
            error: combine_errors(test_outcome.error.clone(), test_details_error.clone()),
            ci,
            details: serde_json::to_value(test_details)
                .context("failed to serialize test phase details")?,
        },
    )?;
    ensure_no_details_error("test", test_details_error)?;
    ensure_phase_success("test", &test_outcome)?;

    println!("Wrote template stats to `{}`", args.output.display());
    Ok(())
}

fn resolve_program_path(path: &Path) -> Result<PathBuf> {
    if path.is_absolute() || path.components().count() > 1 {
        return fs::canonicalize(path)
            .with_context(|| format!("failed to resolve `{}`", path.display()));
    }

    Ok(path.to_path_buf())
}

fn initialize_output_file(path: &Path) -> Result<()> {
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create `{}`", parent.display()))?;
    }

    fs::write(path, b"").with_context(|| format!("failed to initialize `{}`", path.display()))
}

fn read_acton_version(acton_bin: &Path) -> Result<String> {
    let output = Command::new(acton_bin)
        .arg("--version")
        .output()
        .with_context(|| format!("failed to run `{}` --version", acton_bin.display()))?;

    if !output.status.success() {
        bail!(
            "`{}` --version failed with status {}",
            acton_bin.display(),
            output.status
        );
    }

    let version = String::from_utf8(output.stdout)
        .context("acton --version output is not valid UTF-8")?
        .trim()
        .to_owned();

    if version.is_empty() {
        bail!(
            "`{}` --version returned an empty string",
            acton_bin.display()
        );
    }

    Ok(version)
}

fn run_command(program: &Path, cwd: &Path, args: &[String]) -> CommandOutcome {
    let started_at = Utc::now();
    let started = Instant::now();

    let mut command = Command::new(program);
    command.args(args).current_dir(cwd);
    command.stdin(Stdio::null());

    if env_var("ACTON_LOG_DIR").is_none() {
        command.env("ACTON_LOG_DIR", std::env::temp_dir());
    }

    let result = command.status();

    let finished_at = Utc::now();
    let duration_ms = started.elapsed().as_millis();

    match result {
        Ok(status) if status.success() => CommandOutcome {
            started_at,
            finished_at,
            duration_ms,
            exit_code: status.code(),
            error: None,
        },
        Ok(status) => CommandOutcome {
            started_at,
            finished_at,
            duration_ms,
            exit_code: status.code(),
            error: Some(format!("command exited with status {status}")),
        },
        Err(error) => CommandOutcome {
            started_at,
            finished_at,
            duration_ms,
            exit_code: None,
            error: Some(format!("failed to spawn command: {error}")),
        },
    }
}

fn collect_build_details(project_dir: &Path) -> (BuildPhaseDetails, Option<String>) {
    match do_collect_build_details(project_dir) {
        Ok(details) => (details, None),
        Err(error) => (
            BuildPhaseDetails {
                artifact_dir: BUILD_ARTIFACTS_DIR,
                contract_count: 0,
                contracts: Vec::new(),
            },
            Some(error.to_string()),
        ),
    }
}

fn do_collect_build_details(project_dir: &Path) -> Result<BuildPhaseDetails> {
    let build_dir = project_dir.join(BUILD_ARTIFACTS_DIR);
    if !build_dir.is_dir() {
        return Ok(BuildPhaseDetails {
            artifact_dir: BUILD_ARTIFACTS_DIR,
            contract_count: 0,
            contracts: Vec::new(),
        });
    }

    let mut contracts = Vec::new();
    let mut entries = fs::read_dir(&build_dir)
        .with_context(|| format!("failed to read `{}`", build_dir.display()))?
        .collect::<std::io::Result<Vec<_>>>()
        .with_context(|| format!("failed to enumerate `{}`", build_dir.display()))?;

    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }

        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read `{}`", path.display()))?;
        let artifact: BuildArtifactFile = serde_json::from_str(&contents)
            .with_context(|| format!("failed to parse `{}`", path.display()))?;
        let contract_id = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .context("build artifact file name is not valid UTF-8")?
            .to_owned();

        contracts.push(BuildArtifactSummary {
            contract_id,
            hash: artifact.hash,
        });
    }

    Ok(BuildPhaseDetails {
        artifact_dir: BUILD_ARTIFACTS_DIR,
        contract_count: contracts.len(),
        contracts,
    })
}

fn collect_test_details(project_dir: &Path) -> (TestPhaseDetails, Option<String>) {
    match do_collect_test_details(project_dir) {
        Ok(details) => (details, None),
        Err(error) => (
            TestPhaseDetails {
                junit_path: format!("{TEST_RESULTS_DIR}/{JUNIT_REPORT_NAME}"),
                report_found: false,
                suite_count: 0,
                case_count: 0,
                passed: 0,
                failed: 0,
                errors: 0,
                skipped: 0,
                duration_ms: 0,
                suites: Vec::new(),
                cases: Vec::new(),
            },
            Some(error.to_string()),
        ),
    }
}

fn do_collect_test_details(project_dir: &Path) -> Result<TestPhaseDetails> {
    let report_path = project_dir.join(TEST_RESULTS_DIR).join(JUNIT_REPORT_NAME);
    let relative_path = format!("{TEST_RESULTS_DIR}/{JUNIT_REPORT_NAME}");

    if !report_path.is_file() {
        return Ok(TestPhaseDetails {
            junit_path: relative_path,
            report_found: false,
            suite_count: 0,
            case_count: 0,
            passed: 0,
            failed: 0,
            errors: 0,
            skipped: 0,
            duration_ms: 0,
            suites: Vec::new(),
            cases: Vec::new(),
        });
    }

    let file = File::open(&report_path)
        .with_context(|| format!("failed to open `{}`", report_path.display()))?;
    let report = Report::deserialize(BufReader::new(file))
        .with_context(|| format!("failed to parse `{}`", report_path.display()))?;

    let mut suites = Vec::new();
    let mut cases = Vec::new();
    let mut passed = 0usize;
    let mut failed = 0usize;
    let mut errors = 0usize;
    let mut skipped = 0usize;
    let mut duration_ms = 0u128;

    for suite in report.test_suites {
        let suite_name = suite.name.to_string();
        let suite_file_path = suite
            .properties
            .iter()
            .find(|property| property.name.as_str() == "file.path")
            .map(|property| normalize_suite_path(project_dir, property.value.as_str()));

        let mut suite_passed = 0usize;
        let mut suite_failed = 0usize;
        let mut suite_errors = 0usize;
        let mut suite_skipped = 0usize;
        let mut suite_duration_ms = 0u128;

        for case in &suite.test_cases {
            let case_duration_ms = case.time.unwrap_or_default().as_millis();
            suite_duration_ms += case_duration_ms;
            duration_ms += case_duration_ms;

            let (status, message) = match &case.status {
                TestCaseStatus::Success { .. } => {
                    passed += 1;
                    suite_passed += 1;
                    ("passed", None)
                }
                TestCaseStatus::NonSuccess { kind, message, .. } => match kind {
                    NonSuccessKind::Failure => {
                        failed += 1;
                        suite_failed += 1;
                        (
                            "failed",
                            message.as_ref().map(|value| value.as_str().to_owned()),
                        )
                    }
                    NonSuccessKind::Error => {
                        errors += 1;
                        suite_errors += 1;
                        (
                            "error",
                            message.as_ref().map(|value| value.as_str().to_owned()),
                        )
                    }
                },
                TestCaseStatus::Skipped { message, .. } => {
                    skipped += 1;
                    suite_skipped += 1;
                    (
                        "skipped",
                        message.as_ref().map(|value| value.as_str().to_owned()),
                    )
                }
            };

            cases.push(TestCaseSummary {
                suite_name: suite_name.clone(),
                suite_file_path: suite_file_path.clone(),
                test_name: case.name.to_string(),
                status,
                duration_ms: case_duration_ms,
                message,
            });
        }

        suites.push(TestSuiteSummary {
            suite_name,
            suite_file_path,
            case_count: suite.test_cases.len(),
            passed: suite_passed,
            failed: suite_failed,
            errors: suite_errors,
            skipped: suite_skipped,
            duration_ms: suite_duration_ms,
        });
    }

    Ok(TestPhaseDetails {
        junit_path: relative_path,
        report_found: true,
        suite_count: suites.len(),
        case_count: report.tests,
        passed,
        failed,
        errors,
        skipped,
        duration_ms,
        suites,
        cases,
    })
}

fn append_record(path: &Path, record: StatsRecord) -> Result<()> {
    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open `{}` for append", path.display()))?;

    serde_json::to_writer(&mut file, &record)
        .with_context(|| format!("failed to write JSONL record to `{}`", path.display()))?;
    writeln!(file).with_context(|| format!("failed to finalize `{}`", path.display()))
}

fn command_for_record(program: &Path, args: &[String]) -> Vec<String> {
    let mut command = Vec::with_capacity(args.len() + 1);
    command.push(program.display().to_string());
    command.extend(args.iter().cloned());
    command
}

fn combine_errors(command_error: Option<String>, details_error: Option<String>) -> Option<String> {
    match (command_error, details_error) {
        (Some(command_error), Some(details_error)) => Some(format!(
            "{command_error}; failed to collect phase details: {details_error}"
        )),
        (Some(command_error), None) => Some(command_error),
        (None, Some(details_error)) => {
            Some(format!("failed to collect phase details: {details_error}"))
        }
        (None, None) => None,
    }
}

fn ensure_phase_success(phase: &str, outcome: &CommandOutcome) -> Result<()> {
    if outcome.success() {
        return Ok(());
    }

    let exit_code = outcome
        .exit_code
        .map(|code| code.to_string())
        .unwrap_or_else(|| "unknown".to_owned());

    bail!("template {phase} phase failed with exit code {exit_code}");
}

fn ensure_no_details_error(phase: &str, error: Option<String>) -> Result<()> {
    if let Some(error) = error {
        bail!("failed to collect {phase} phase details: {error}");
    }

    Ok(())
}

fn env_var(name: &str) -> Option<String> {
    std::env::var(name)
        .ok()
        .filter(|value| !value.trim().is_empty())
}

fn normalize_suite_path(project_dir: &Path, path: &str) -> String {
    let candidate = Path::new(path);
    if let (Ok(canonical_project_dir), Ok(canonical_candidate)) =
        (fs::canonicalize(project_dir), fs::canonicalize(candidate))
        && let Ok(relative) = canonical_candidate.strip_prefix(&canonical_project_dir)
    {
        return relative.display().to_string();
    }

    candidate
        .strip_prefix(project_dir)
        .unwrap_or(candidate)
        .display()
        .to_string()
}
