use crate::support::TestOutputExt;
use crate::support::project::{Project, ProjectBuilder};
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

const MUTATION_CONTRACT: &str = r"
fun onInternalMessage(in: InMessage) {
    assert (in.valueCoins > 0) throw 5;
}

fun onBouncedMessage(_: InMessageBounced) {}

get fun addOne(x: int): int {
    return x + 1;
}
";

const PASSING_TEST: &str = r#"
import "../../lib/testing/expect"

get fun `test-always-pass`() {
    expect(1).toEqual(1);
}
"#;

const DEPENDENT_MUTATION_CONTRACT: &str = r#"
import "../gen/dependency_code.tolk"

fun onInternalMessage(in: InMessage) {
    assert (in.valueCoins > 0) throw 5;
    val code = dependencyCompiledCode();
}

fun onBouncedMessage(_: InMessageBounced) {}
"#;

const BROKEN_DEPENDENCY_MUTATION_CONTRACT: &str = r"
fun onInternalMessage(in: InMessage) {
    THIS IS A SYNTAX ERROR
}

fun onBouncedMessage(_: InMessageBounced) {}
";

const COMPILE_ERROR_MUTATION_CONTRACT: &str = r"
get fun mustFail(): int {
    throw 5;
}

get fun addOne(x: int): int {
    return x + 1;
}

fun onInternalMessage(_: InMessage) {}
fun onBouncedMessage(_: InMessageBounced) {}
";

const NO_MUTATION_POINTS_CONTRACT: &str = r"
fun onInternalMessage(_: InMessage) {}
fun onBouncedMessage(_: InMessageBounced) {}
";

const MUTATION_CONTRACT_ARITHMETIC_CHANGED: &str = r"
fun onInternalMessage(in: InMessage) {
    assert (in.valueCoins > 0) throw 5;
}

fun onBouncedMessage(_: InMessageBounced) {}

get fun addOne(x: int): int {
    return x + 2;
}
";

fn mutation_project(name: &str) -> Project {
    ProjectBuilder::new(name)
        .contract("simple", MUTATION_CONTRACT)
        .test_file("mutation", PASSING_TEST)
        .build()
}

fn git(project_root: &Path, args: &[&str]) -> Output {
    Command::new("git")
        .args(args)
        .current_dir(project_root)
        .output()
        .unwrap_or_else(|err| panic!("failed to run git {:?}: {err}", args))
}

fn git_ok(project_root: &Path, args: &[&str], context: &str) {
    let output = git(project_root, args);
    assert!(
        output.status.success(),
        "{context} failed:\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn init_git_repo(project_root: &Path) {
    git_ok(project_root, &["init", "-q"], "git init");
    git_ok(
        project_root,
        &["branch", "-M", "main"],
        "git branch -M main",
    );
    git_ok(
        project_root,
        &["config", "user.email", "acton-tests@example.com"],
        "git config user.email",
    );
    git_ok(
        project_root,
        &["config", "user.name", "Acton Tests"],
        "git config user.name",
    );
}

fn commit_all(project_root: &Path, message: &str) {
    git_ok(project_root, &["add", "."], "git add");
    git_ok(project_root, &["commit", "-qm", message], "git commit");
}

fn checkout_new_branch(project_root: &Path, branch: &str) {
    git_ok(
        project_root,
        &["checkout", "-qb", branch],
        "git checkout -b",
    );
}

fn set_upstream(project_root: &Path, target: &str) {
    git_ok(
        project_root,
        &["branch", "--set-upstream-to", target],
        "git branch --set-upstream-to",
    );
}

fn write_simple_contract(project: &Project, source: &str) {
    fs::write(project.path().join("contracts/simple.tolk"), source)
        .expect("failed to update simple contract");
}

#[test]
fn mutate_requires_mutate_contract() {
    mutation_project("j-mutate-requires-contract")
        .acton()
        .test()
        .arg("--mutate")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_requires_mutate_contract.stderr.txt",
        );
}

#[test]
fn mutate_fails_for_unknown_contract() {
    mutation_project("j-mutate-unknown-contract")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("missing")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_fails_for_unknown_contract.stderr.txt",
        );
}

#[test]
fn mutate_reports_summary() {
    mutation_project("j-mutate-summary")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_reports_summary.stdout.txt",
        );
}

#[test]
fn mutate_disable_rule_filters_mutants() {
    mutation_project("j-mutate-disable-rule")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--disable-rule")
        .arg("remove_assert")
        .arg("--disable-rule")
        .arg("flip_plus")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_disable_rule_filters_mutants.stdout.txt",
        );
}

#[test]
fn mutate_diff_ref_requires_ref() {
    mutation_project("j-mutate-diff-ref-requires-ref")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("ref")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_ref_requires_ref.stderr.txt",
        );
}

#[test]
fn mutate_diff_ref_without_mode_is_rejected() {
    mutation_project("j-mutate-diff-ref-without-mode")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff-ref")
        .arg("HEAD")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_ref_without_mode_is_rejected.stderr.txt",
        );
}

#[test]
fn mutate_diff_worktree_rejects_ref() {
    mutation_project("j-mutate-diff-worktree-rejects-ref")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("worktree")
        .arg("--mutation-diff-ref")
        .arg("HEAD")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_worktree_rejects_ref.stderr.txt",
        );
}

#[test]
fn mutate_diff_worktree_filters_mutants() {
    let project = mutation_project("j-mutate-diff-worktree");
    init_git_repo(project.path());
    commit_all(project.path(), "initial");
    write_simple_contract(&project, MUTATION_CONTRACT_ARITHMETIC_CHANGED);

    project
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("worktree")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_worktree_filters_mutants.stdout.txt",
        );
}

#[test]
fn mutate_diff_branch_requires_upstream_or_ref() {
    let project = mutation_project("j-mutate-diff-branch-missing-upstream");
    init_git_repo(project.path());
    commit_all(project.path(), "initial");
    checkout_new_branch(project.path(), "feature/no-upstream");
    write_simple_contract(&project, MUTATION_CONTRACT_ARITHMETIC_CHANGED);

    project
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("branch")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_branch_requires_upstream_or_ref.stderr.txt",
        );
}

#[test]
fn mutate_diff_ref_filters_mutants() {
    let project = mutation_project("j-mutate-diff-ref");
    init_git_repo(project.path());
    commit_all(project.path(), "initial");
    write_simple_contract(&project, MUTATION_CONTRACT_ARITHMETIC_CHANGED);
    commit_all(project.path(), "change arithmetic");

    project
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("ref")
        .arg("--mutation-diff-ref")
        .arg("HEAD~1")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_ref_filters_mutants.stdout.txt",
        );
}

#[test]
fn mutate_diff_branch_filters_mutants() {
    let project = mutation_project("j-mutate-diff-branch");
    init_git_repo(project.path());
    commit_all(project.path(), "initial");
    checkout_new_branch(project.path(), "feature/mutation-diff");
    set_upstream(project.path(), "main");
    write_simple_contract(&project, MUTATION_CONTRACT_ARITHMETIC_CHANGED);
    commit_all(project.path(), "change arithmetic");

    project
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("branch")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_branch_filters_mutants.stdout.txt",
        );
}

#[test]
fn mutate_diff_branch_with_explicit_ref_filters_mutants() {
    let project = mutation_project("j-mutate-diff-branch-explicit-ref");
    init_git_repo(project.path());
    commit_all(project.path(), "initial");
    checkout_new_branch(project.path(), "feature/mutation-diff-explicit-ref");
    write_simple_contract(&project, MUTATION_CONTRACT_ARITHMETIC_CHANGED);
    commit_all(project.path(), "change arithmetic");

    project
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-diff")
        .arg("branch")
        .arg("--mutation-diff-ref")
        .arg("main")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_diff_branch_with_explicit_ref_filters_mutants.stdout.txt",
        );
}

#[test]
fn mutate_levels_filter_mutants_from_cli() {
    mutation_project("j-mutate-levels-cli")
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .arg("--mutation-levels")
        .arg("critical,major")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_levels_filter_mutants_from_cli.stdout.txt",
        );
}

#[test]
fn mutate_uses_mutation_diff_from_config() {
    let project = ProjectBuilder::new("j-mutate-config-diff-worktree")
        .without_acton_toml()
        .contract("simple", MUTATION_CONTRACT)
        .test_file("mutation", PASSING_TEST)
        .raw_file(
            "Acton.toml",
            r#"[package]
name = "j-mutate-config-diff-worktree"
description = "A test project"
version = "0.1.0"

[contracts.simple]
name = "simple"
src = "contracts/simple.tolk"

[test.mutation]
diff = "worktree"
"#,
        )
        .build();
    init_git_repo(project.path());
    commit_all(project.path(), "initial");
    write_simple_contract(&project, MUTATION_CONTRACT_ARITHMETIC_CHANGED);

    project
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_uses_mutation_diff_from_config.stdout.txt",
        );
}

#[test]
fn mutate_uses_disable_rules_from_config() {
    ProjectBuilder::new("j-mutate-config-disable-rules")
        .without_acton_toml()
        .contract("simple", MUTATION_CONTRACT)
        .test_file("mutation", PASSING_TEST)
        .raw_file(
            "Acton.toml",
            r#"[package]
name = "j-mutate-config-disable-rules"
description = "A test project"
version = "0.1.0"

[contracts.simple]
name = "simple"
src = "contracts/simple.tolk"

[test.mutation]
disable-rules = ["remove_assert", "flip_plus", "flip_gt_ge"]
"#,
        )
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_uses_disable_rules_from_config.stdout.txt",
        );
}

#[test]
fn mutate_uses_mutation_levels_from_config() {
    ProjectBuilder::new("j-mutate-config-mutation-levels")
        .without_acton_toml()
        .contract("simple", MUTATION_CONTRACT)
        .test_file("mutation", PASSING_TEST)
        .raw_file(
            "Acton.toml",
            r#"[package]
name = "j-mutate-config-mutation-levels"
description = "A test project"
version = "0.1.0"

[contracts.simple]
name = "simple"
src = "contracts/simple.tolk"

[test.mutation]
mutation-levels = ["critical"]
"#,
        )
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("simple")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_uses_mutation_levels_from_config.stdout.txt",
        );
}

#[test]
fn mutate_contract_with_dependencies() {
    ProjectBuilder::new("j-mutate-contract-with-dependencies")
        .contract("dependency", MUTATION_CONTRACT)
        .contract_with_deps("main", DEPENDENT_MUTATION_CONTRACT, vec!["dependency"])
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_contract_with_dependencies.stdout.txt",
        );
}

#[test]
fn mutate_contract_with_library_ref_dependency() {
    ProjectBuilder::new("j-mutate-contract-with-library-ref-dependency")
        .contract("dependency", MUTATION_CONTRACT)
        .contract_with_detailed_deps(
            "main",
            DEPENDENT_MUTATION_CONTRACT,
            vec![("dependency", Some("library_ref"), None, None)],
        )
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_contract_with_library_ref_dependency.stdout.txt",
        );
}

#[test]
fn mutate_contract_with_dependencies_and_clear_cache() {
    ProjectBuilder::new("j-mutate-contract-with-dependencies-clear-cache")
        .contract("dependency", MUTATION_CONTRACT)
        .contract_with_deps("main", DEPENDENT_MUTATION_CONTRACT, vec!["dependency"])
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .clear_cache()
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_contract_with_dependencies_and_clear_cache.stdout.txt",
        );
}

#[test]
fn mutate_reports_dependency_build_failure() {
    ProjectBuilder::new("j-mutate-dependency-build-failure")
        .contract("dependency", BROKEN_DEPENDENCY_MUTATION_CONTRACT)
        .contract_with_deps("main", DEPENDENT_MUTATION_CONTRACT, vec!["dependency"])
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_reports_dependency_build_failure.stderr.txt",
        );
}

#[test]
fn mutate_compile_errors_are_excluded_from_score() {
    ProjectBuilder::new("j-mutate-compile-errors-excluded-from-score")
        .contract("main", COMPILE_ERROR_MUTATION_CONTRACT)
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_compile_errors_are_excluded_from_score.stdout.txt",
        );
}

#[test]
fn mutate_reports_no_mutation_points() {
    ProjectBuilder::new("j-mutate-no-mutation-points")
        .contract("main", NO_MUTATION_POINTS_CONTRACT)
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_mutate/mutate_reports_no_mutation_points.stdout.txt",
        );
}

#[test]
#[ignore = "benchmark scenario for local perf tracking"]
fn mutate_benchmark_large_mutant_set() {
    use std::fmt::Write as _;
    use std::time::Instant;

    let mut asserts = String::new();
    for _ in 0..120 {
        writeln!(&mut asserts, "    assert (in.valueCoins > 0) throw 5;")
            .expect("write benchmark contract");
    }

    let contract = format!(
        r"
fun onInternalMessage(in: InMessage) {{
{asserts}
}}

fun onBouncedMessage(_: InMessageBounced) {{}}
"
    );

    let start = Instant::now();
    let output = ProjectBuilder::new("j-mutate-benchmark-large-mutant-set")
        .contract("main", &contract)
        .test_file("mutation", PASSING_TEST)
        .build()
        .acton()
        .test()
        .arg("--mutate")
        .arg("--mutate-contract")
        .arg("main")
        .run()
        .success();
    let elapsed = start.elapsed();

    let stdout = output.get_normalized_stdout();
    let mutant_count = stdout
        .lines()
        .find_map(|line| line.trim().strip_prefix("Mutants:"))
        .and_then(|value| value.trim().parse::<usize>().ok())
        .expect("mutation output must contain mutant count");

    assert!(
        mutant_count >= 100,
        "expected at least 100 mutants in benchmark scenario, got {mutant_count}\n{stdout}"
    );

    if let Some(max_ms) = std::env::var("MUTATION_BENCH_MAX_MS")
        .ok()
        .and_then(|value| value.parse::<u128>().ok())
    {
        assert!(
            elapsed.as_millis() <= max_ms,
            "mutation benchmark regression: elapsed={}ms exceeds MUTATION_BENCH_MAX_MS={}ms",
            elapsed.as_millis(),
            max_ms
        );
    }

    eprintln!("mutation benchmark: {mutant_count} mutants processed in {elapsed:?}");
}
