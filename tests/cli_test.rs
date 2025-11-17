use assert_cmd::cargo;
use fs_extra::dir::{CopyOptions, copy};
use include_dir::{Dir, include_dir};
use once_cell::sync::Lazy;
use predicates::str::contains;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[test]
fn test_help_works() {
    let mut cmd = cargo::cargo_bin_cmd!("acton");
    cmd.arg("test")
        .arg("--help")
        .assert()
        .success()
        .stdout(contains("Usage: acton test"));
}

#[test]
fn test_can_test_basic_project() {
    let setup = ProjectSetup::new("basic");
    let (stdout, _, status) = setup.run_tests(None);

    assert!(status.success());

    assert!(stdout.contains("> ./tests/counter_test.tolk (2 tests)"));
    assert!(stdout.contains("✓ should increase counter"));
    assert!(stdout.contains("✓ should reset counter"));
    assert!(stdout.contains("✓ 2 passed in 1 file"));
}

#[test]
fn test_can_test_basic_project_with_failing_contract_code() {
    let setup = ProjectSetup::new("basic").with_enabled_contract_slot(1);
    let (stdout, _, status) = setup.run_tests(None);

    println!("{}", stdout);
    assert!(!status.success());

    assert!(stdout.contains(" > ./tests/counter_test.tolk (2 tests)
  ✗ should increase counter <TIME>
    └─ Error: expect(actual).toHaveSuccessfulTx(expected)
        N/A -> deployer A
        └── IncreaseCounter 0.1 TON -> Counter B                                        gas=1513 exit_code=10 aborted
            └── Compute phase failed: Dictionary error

        Cannot find transaction from deployer A EQBvDB..FHByMJ to Counter B EQANZp..QQ5GsV
        with:
          exit_code=0
      └─ at ./tests/counter_test.tolk:<LINE>"));
    assert!(stdout.contains("  ✗ should reset counter <TIME>
    └─ Error: expect(actual).toHaveSuccessfulTx(expected)
        N/A -> deployer A
        └── IncreaseCounter 0.1 TON -> Counter B                                        gas=1513 exit_code=10 aborted
            └── Compute phase failed: Dictionary error

        Cannot find transaction from deployer A EQBvDB..FHByMJ to Counter B EQANZp..QQ5GsV
        with:
          exit_code=0
      └─ at ./tests/counter_test.tolk:<LINE>"));
}

#[test]
fn test_can_test_basic_project_with_failing_contract_code_with_backtrace_full() {
    let setup = ProjectSetup::new("basic").with_enabled_contract_slot(1);
    let (stdout, _, status) = setup.run_tests(Some("full"));

    println!("{}", stdout);
    assert!(!status.success());

    assert!(stdout.contains(" > ./tests/counter_test.tolk (2 tests)
  ✗ should increase counter <TIME>
    └─ Error: expect(actual).toHaveSuccessfulTx(expected)
        N/A -> deployer A
        └── IncreaseCounter 0.1 TON -> Counter B                                        gas=1513 exit_code=10 aborted
            ├── Compute phase failed: Dictionary error
            └── at contracts/counter.tolk:<LINE>
                   __throw   at contracts/counter.tolk:<LINE>


        Cannot find transaction from deployer A EQBvDB..FHByMJ to Counter B EQANZp..QQ5GsV
        with:
          exit_code=0
      └─ at ./tests/counter_test.tolk:<LINE>"));
    assert!(stdout.contains("  ✗ should reset counter <TIME>
    └─ Error: expect(actual).toHaveSuccessfulTx(expected)
        N/A -> deployer A
        └── IncreaseCounter 0.1 TON -> Counter B                                        gas=1513 exit_code=10 aborted
            ├── Compute phase failed: Dictionary error
            └── at contracts/counter.tolk:<LINE>
                   __throw   at contracts/counter.tolk:<LINE>


        Cannot find transaction from deployer A EQBvDB..FHByMJ to Counter B EQANZp..QQ5GsV
        with:
          exit_code=0
      └─ at ./tests/counter_test.tolk:<LINE>"));
}

#[test]
fn test_can_test_project_with_compilation_error() {
    let setup = ProjectSetup::new("with_compilation_error").with_enabled_contract_slot(1);
    let (stdout, stderr, status) = setup.run_tests(None);

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(!status.success());

    assert!(stderr.contains("<ROOT>/contracts/counter.tolk:<LINE>: error: field `body2` doesn't exist in type `InMessage`

    // in function `onInternalMessage`
   6 |     val msg = lazy AllowedMessage.fromSlice(in.body2);
     |                                                ^^^^^

Error: Build failed with 1 error"));
}

#[test]
fn test_can_test_project_with_gas_limit_failure() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(1);
    let (stdout, stderr, status) = setup.run_tests(None);

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(!status.success());

    assert!(stdout.contains(
        " > ./tests/counter_test.tolk (2 tests)
  ✗ should increase counter <TIME>
    └─ Gas limit exceeded: used 153499, limit 100"
    ));
}

#[test]
fn test_can_test_project_with_simple_expect_failure() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(2);
    let (stdout, stderr, status) = setup.run_tests(None);

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(!status.success());

    assert!(stdout.contains(
        " > ./tests/counter_test.tolk (2 tests)
  ✗ should increase counter <TIME>
    └─ Error: expect(actual).toEqual(expected)
        (
            1,
            2
        )
      └─ at ./tests/counter_test.tolk:<LINE>"
    ));
}

#[test]
fn test_can_test_project_with_exit_code_mismatch() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(3);
    let (stdout, stderr, status) = setup.run_tests(None);

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(!status.success());

    assert!(stdout.contains(
        "  ✗ should reset counter <TIME>
    └─ Expected exit_code=100, got=0"
    ));
}

#[test]
fn test_can_test_project_with_throw_in_test() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(4);
    let (stdout, stderr, status) = setup.run_tests(None);

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(!status.success());

    assert!(stdout.contains(
        "  ✗ should reset counter <TIME>
    └─ exit_code=9
      ├─ Re-run with --backtrace full to get more information
      └─ Phase: Compute phase"
    ));
}

#[test]
fn test_can_test_project_with_throw_in_test_and_backtrace_full() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(4);
    let (stdout, stderr, status) = setup.run_tests(Some("full"));

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(!status.success());

    assert!(stdout.contains(
        "  ✗ should reset counter <TIME>
    └─ exit_code=9
      ├─ at tests/counter_test.tolk:<LINE>
      │     __throw   at tests/counter_test.tolk:<LINE>
      └─ Phase: Compute phase"
    ));
}

#[test]
fn test_can_test_project_with_debug_output_in_contract() {
    let setup = ProjectSetup::new("basic")
        .with_enabled_contract_slot(2)
        .with_enabled_test_slot(5);
    let (stdout, stderr, status) = setup.run_tests(Some("full"));

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(status.success());

    assert!(stdout.contains(
        " > ./tests/counter_test.tolk (2 tests)
  ✓ should increase counter <TIME>
    └─ Test output:
       N/A -> deployer A
       └── IncreaseCounter 0.1 TON -> Counter B                                        gas=1508"
    ));
}

#[test]
fn test_can_test_project_with_stderr_output_in_test() {
    let setup = ProjectSetup::new("basic").with_enabled_test_slot(6);
    let (stdout, stderr, status) = setup.run_tests(Some("full"));

    println!("{}", stdout);
    println!("{}", stderr);
    assert!(status.success());

    assert!(stdout.contains(
        " > ./tests/counter_test.tolk (2 tests)
  ✓ should increase counter <TIME>
    └─ Test stderr:
       error output"
    ));
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

    fn with_enabled_contract_slot(mut self, slot: usize) -> Self {
        enable_slot(&self.project_path, "contracts/counter.tolk", slot);
        self
    }

    fn with_enabled_test_slot(mut self, slot: usize) -> Self {
        enable_slot(&self.project_path, "tests/counter_test.tolk", slot);
        self
    }

    fn run_tests(&self, backtrace: Option<&str>) -> (String, String, std::process::ExitStatus) {
        let (stdout, stderr, status) = run_all_tests_in(&self.project_path, backtrace);
        let (stdout, stderr) = process_test_output(&stdout, &stderr, &self.project_path);
        (stdout, stderr, status)
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

fn run_all_tests_in(
    tmp: &PathBuf,
    backtrace: Option<&str>,
) -> (String, String, std::process::ExitStatus) {
    let mut cmd = cargo::cargo_bin_cmd!("acton");
    let cmd = cmd
        .arg("test")
        .env("NO_COLOR", "1")
        .current_dir(tmp)
        .arg(".");

    if backtrace == Some("full") {
        cmd.arg("--backtrace");
        cmd.arg("full");
    }

    let output = cmd.output().unwrap();

    let stdout = strip_ansi(&String::from_utf8(output.stdout).unwrap());
    let stderr = strip_ansi(&String::from_utf8(output.stderr).unwrap());
    (stdout, stderr, output.status)
}

fn strip_ansi(s: &str) -> String {
    let bytes = strip_ansi_escapes::strip(s.as_bytes());
    String::from_utf8(bytes).unwrap()
}

fn process_test_output(stdout: &str, stderr: &str, project_path: &PathBuf) -> (String, String) {
    let stdout = sanitize_output(strip_ansi(stdout).as_str(), project_path);
    let stderr = sanitize_output(strip_ansi(stderr).as_str(), project_path);
    (stdout, stderr)
}

fn sanitize_output(s: &str, full_path: &PathBuf) -> String {
    static TIME_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"\d+ms").unwrap());
    static LINE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"(\.tolk):\d+:\d+").unwrap());

    let s = TIME_RE.replace_all(s, "<TIME>").to_string();
    let s = LINE_RE.replace_all(&s, "$1:<LINE>").to_string();
    let s = s
        .replace(&full_path.to_string_lossy().to_string(), "<ROOT>")
        .to_string();
    let s = s.replace("/private", "").to_string();

    s.lines()
        .map(|l| {
            let is_whitespace_only = l.chars().all(|ch| ch.is_whitespace());
            if is_whitespace_only {
                return "".to_string();
            }
            return l.to_string();
        })
        .collect::<Vec<String>>()
        .join("\n")
}
