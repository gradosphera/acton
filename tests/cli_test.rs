use fs_extra::dir::{CopyOptions, copy};
use include_dir::{Dir, include_dir};
use snapbox::IntoData;
use snapbox::cmd::OutputAssert;
use snapbox::filter::Filter;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

macro_rules! regex {
    ($re:literal $(,)?) => {{
        static RE: std::sync::OnceLock<regex::Regex> = std::sync::OnceLock::new();
        RE.get_or_init(|| regex::Regex::new($re).unwrap())
    }};
}

#[test]
fn test_acton_help() {
    snapbox::cmd::Command::acton_ui()
        .arg("--help")
        .assert()
        .success()
        .stdout_eq(snapbox::file!["testsuite/acton/stdout.txt"])
        .stderr_eq(snapbox::str![""]);
}

#[test]
fn test_acton_build_help() {
    snapbox::cmd::Command::acton_ui()
        .arg("build")
        .arg("--help")
        .assert()
        .success()
        .stdout_eq(snapbox::file!["testsuite/build/stdout.txt"])
        .stderr_eq(snapbox::str![""]);
}

static MIN_LITERAL_REDACTIONS: &[(&str, &str)] = &[
    ("[EXE]", std::env::consts::EXE_SUFFIX),
    ("[BROKEN_PIPE]", "Broken pipe (os error 32)"),
    ("[BROKEN_PIPE]", "The pipe is being closed. (os error 232)"),
    // Unix message for an entity was not found
    ("[NOT_FOUND]", "No such file or directory (os error 2)"),
    // Windows message for an entity was not found
    (
        "[NOT_FOUND]",
        "The system cannot find the file specified. (os error 2)",
    ),
    (
        "[NOT_FOUND]",
        "The system cannot find the path specified. (os error 3)",
    ),
    ("[NOT_FOUND]", "Access is denied. (os error 5)"),
    ("[NOT_FOUND]", "program not found"),
    // Unix message for exit status
    ("[EXIT_STATUS]", "exit status"),
    // Windows message for exit status
    ("[EXIT_STATUS]", "exit code"),
];

pub fn assert_ui() -> snapbox::Assert {
    let mut subs = snapbox::Redactions::new();
    subs.extend(MIN_LITERAL_REDACTIONS.into_iter().cloned())
        .unwrap();
    add_regex_redactions(&mut subs);

    snapbox::Assert::new()
        .action_env(snapbox::assert::DEFAULT_ACTION_ENV)
        .redact_with(subs)
}

fn add_regex_redactions(subs: &mut snapbox::Redactions) {
    subs.insert("[TIME]", regex!(r"(\d+\.)?\d+ms")).unwrap();
    subs.insert("[LINE]", regex!(r"(\.tolk):\d+:\d+")).unwrap();
}

pub fn acton_exe() -> PathBuf {
    snapbox::cmd::cargo_bin!("acton").to_path_buf()
}

pub trait ActonCommandExt {
    fn acton_ui() -> Self;
}

impl ActonCommandExt for snapbox::cmd::Command {
    fn acton_ui() -> Self {
        Self::new(acton_exe()).with_assert(assert_ui())
    }
}

#[test]
fn test_can_test_basic_project() {
    let setup = ProjectSetup::new("basic");
    let output = setup.run_tests(None);

    let assert = output.success();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file!["projects/basic/outs/test_can_test_basic_project.stdout.txt"],
    );
}

#[test]
fn test_can_test_basic_project_with_failing_contract_code() {
    let setup = ProjectSetup::new("basic").with_enabled_contract_slot(1);
    let output = setup.run_tests(None);

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file![
            "projects/basic/outs/test_can_test_basic_project_with_failing_contract_code.stdout.txt"
        ],
    );
}

#[test]
fn test_can_test_basic_project_with_failing_contract_code_with_backtrace_full() {
    let setup = ProjectSetup::new("basic").with_enabled_contract_slot(1);
    let output = setup.run_tests(Some("full"));

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file!["projects/basic/outs/test_can_test_basic_project_with_failing_contract_code_with_backtrace_full.stdout.txt"],
    );
}

#[test]
fn test_can_test_project_with_compilation_error() {
    let setup = ProjectSetup::new("with_compilation_error").with_enabled_contract_slot(1);
    let output = setup.run_tests(None);

    let assert = output.failure();

    let stderr = assert.get_output().stderr.clone();
    let content = normalize_content(setup, stderr);

    assertion().eq(
        content,
        snapbox::file!["projects/with_compilation_error/outs/test_can_test_project_with_compilation_error.stderr.txt"],
    );
}

#[test]
fn test_can_test_project_with_gas_limit_failure() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(1);
    let output = setup.run_tests(None);

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file![
            "projects/basic/outs/test_can_test_project_with_gas_limit_failure.stdout.txt"
        ],
    );
}

#[test]
fn test_can_test_project_with_simple_expect_failure() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(2);
    let output = setup.run_tests(None);

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file![
            "projects/basic/outs/test_can_test_project_with_simple_expect_failure.stdout.txt"
        ],
    );
}

#[test]
fn test_can_test_project_with_exit_code_mismatch() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(3);
    let output = setup.run_tests(None);

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file![
            "projects/basic/outs/test_can_test_project_with_exit_code_mismatch.stdout.txt"
        ],
    );
}

#[test]
fn test_can_test_project_with_throw_in_test() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(4);
    let output = setup.run_tests(None);

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file!["projects/basic/outs/test_can_test_project_with_throw_in_test.stdout.txt"],
    );
}

#[test]
fn test_can_test_project_with_throw_in_test_and_backtrace_full() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(4);
    let output = setup.run_tests(Some("full"));

    let assert = output.failure();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file!["projects/basic/outs/test_can_test_project_with_throw_in_test_and_backtrace_full.stdout.txt"],
    );
}

#[test]
fn test_can_test_project_with_debug_output_in_contract() {
    let setup = ProjectSetup::new("basic")
        .with_enabled_contract_slot(2)
        .with_enabled_test_slot(5);
    let output = setup.run_tests(Some("full"));

    let assert = output.success();

    let stdout = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stdout);

    assertion().eq(
        content,
        snapbox::file![
            "projects/basic/outs/test_can_test_project_with_debug_output_in_contract.stdout.txt"
        ],
    );
}

#[test]
fn test_can_test_project_with_stderr_output_in_test() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(6);
    let output = setup.run_tests(Some("full"));

    let assert = output.success();

    let stderr = assert.get_output().stdout.clone();
    let content = normalize_content(setup, stderr);

    assertion().eq(
        content,
        snapbox::file![
            "projects/basic/outs/test_can_test_project_with_stderr_output_in_test.stderr.txt"
        ],
    );
}

struct ProjectSetup {
    tmp_dir: TempDir,
    project_path: PathBuf,
}

impl ProjectSetup {
    fn new(project_name: &str) -> Self {
        let tmp = Self::copy_fixture_project(project_name);
        let project_path = tmp.path().join(project_name);
        patch_imports(&project_path);

        Self {
            tmp_dir: tmp,
            project_path,
        }
    }

    fn with_enabled_contract_slot(self, slot: usize) -> Self {
        enable_slot(&self.project_path, "contracts/counter.tolk", slot);
        self
    }

    fn with_enabled_test_slot(self, slot: usize) -> Self {
        enable_slot(&self.project_path, "tests/counter_test.tolk", slot);
        self
    }

    fn run_tests(&self, backtrace: Option<&str>) -> OutputAssert {
        let mut cmd = snapbox::cmd::Command::acton_ui()
            .arg("test")
            .env("NO_COLOR", "1")
            .current_dir(&self.project_path)
            .arg(".");

        cmd = if backtrace == Some("full") {
            cmd.arg("--backtrace").arg("full")
        } else {
            cmd
        };

        cmd.assert()
    }

    fn copy_fixture_project(name: &str) -> TempDir {
        static LIB_DIR: Dir = include_dir!("$CARGO_MANIFEST_DIR/lib");

        let tmp = TempDir::new().unwrap();
        fs::create_dir_all(tmp.path().join("lib")).unwrap();
        LIB_DIR.extract(tmp.path().join("lib")).unwrap();
        let fixture_dir = Path::new("tests/projects").join(name);

        let mut opts = CopyOptions::new();
        opts.copy_inside = true;

        copy(&fixture_dir, tmp.path(), &opts).unwrap();

        tmp
    }
}

fn enable_slot(full_path: &PathBuf, file: &str, index: usize) {
    let counter_path = full_path.join(file);
    let new_content = fs::read_to_string(&counter_path)
        .unwrap()
        .replace(format!("// SLOT_{index}: ").as_str(), "");
    fs::write(&counter_path, new_content).unwrap();
}

fn patch_imports(full_path: &PathBuf) {
    let counter_path = full_path.join("tests/counter_test.tolk");
    let new_content = fs::read_to_string(&counter_path)
        .unwrap()
        .replace("../../../../", "../../");
    fs::write(&counter_path, new_content).unwrap();
}

fn strip_ansi(s: &str) -> String {
    let bytes = strip_ansi_escapes::strip(s.as_bytes());
    String::from_utf8(bytes).unwrap()
}

fn assertion() -> snapbox::Assert {
    snapbox::Assert::new().action_env("SNAPSHOTS")
}

fn normalize_content(setup: ProjectSetup, stdout: Vec<u8>) -> String {
    let content = strip_ansi(String::from_utf8(stdout.clone()).unwrap().as_str()).into_data();
    let content = snapbox::filter::FilterPaths.filter(content.into_data());
    let content = snapbox::filter::FilterNewlines.filter(content);
    let content = content.render().expect("came in as a String");
    let assert1 = assert_ui();
    let mut redactions = assert1.redactions().clone();
    let tmp_dir = setup.tmp_dir.path().to_string_lossy().to_string();
    redactions.insert("[ROOT]", tmp_dir.clone()).unwrap();
    redactions
        .insert("[ROOT]", "/private".to_owned() + tmp_dir.as_str())
        .unwrap();
    let content = redactions.redact(&content);
    content
}
