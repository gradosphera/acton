use crate::support::TestOutputExt;
use crate::support::project::ProjectBuilder;
use std::fs;

const SIMPLE_CONTRACT: &str = r"
fun onInternalMessage(in: InMessage) {}
fun onBouncedMessage(_: InMessageBounced) {}
";

const PASSING_TEST: &str = r#"
import "../../lib/testing/expect"

get fun `test-manifest-path-works`() {
    expect(1).toEqual(1);
}
"#;

const BUILD_FUNCTION_TEST: &str = r#"
import "../../lib/build/build"
import "../../lib/testing/expect"

get fun `test-build-default-path-from-project-root`() {
    build("simple");
    expect(1).toEqual(1);
}

get fun `test-build-explicit-path-from-project-root`() {
    build("simple", "contracts/simple.tolk");
    expect(1).toEqual(1);
}
"#;

const SCRIPT_ROOT_TEST: &str = r#"
import "../../lib/io"

fun main() {
    println("script-root-ok");
}
"#;

const UNFORMATTED_CONTRACT: &str = r"
fun onInternalMessage(in:InMessage){
val x=1;
    val y = 2;
}
";

const FAILING_PATH_TEST: &str = r#"
import "../../lib/testing/expect"

get fun `test-failing-path-output`() {
    expect(1).toEqual(2);
}
"#;

const SOURCE_MAP_STUB: &str =
    r#"{"high_level":{"version":"1","globals":[],"locations":[]},"debug_marks":{}}"#;

const FORMATTED_SIMPLE_CONTRACT: &str =
    "fun onInternalMessage(in: InMessage) {}\nfun onBouncedMessage(_: InMessageBounced) {}\n";

#[test]
fn test_run_specific_test_file() {
    let project = ProjectBuilder::new("multi-file")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test1",
            r#"
            import "../../lib/testing/expect"

            get fun `test-in-file-1`() {
                expect(1).toEqual(1);
            }
        "#,
        )
        .test_file(
            "test2",
            r#"
            import "../../lib/testing/expect"

            get fun `test-in-file-2`() {
                expect(2).toEqual(2);
            }
        "#,
        )
        .build();

    // Run only test1.tolk
    project
        .acton()
        .test()
        .path("tests/test1.test.tolk")
        .run()
        .success()
        .assert_passed(1)
        .assert_contains("in-file-1")
        .assert_not_contains("in-file-2");
}

#[test]
fn test_filter_by_name() {
    ProjectBuilder::new("filtered")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            r#"
            import "../../lib/testing/expect"

            get fun `test-unit-1`() {
                expect(1).toEqual(1);
            }

            get fun `test-unit-2`() {
                expect(2).toEqual(2);
            }

            get fun `test-other`() {
                expect(3).toEqual(3);
            }
        "#,
        )
        .build()
        .acton()
        .test()
        .filter("test-unit-.*")
        .run()
        .success()
        .assert_passed(2)
        .assert_contains("unit-1")
        .assert_contains("unit-2")
        .assert_not_contains("other");
}

#[test]
fn test_filter_single_test() {
    ProjectBuilder::new("single-filter")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            r#"
            import "../../lib/testing/expect"

            get fun `test-alpha`() {
                expect(1).toEqual(1);
            }

            get fun `test-beta`() {
                expect(2).toEqual(2);
            }

            get fun `test-gamma`() {
                expect(3).toEqual(3);
            }
        "#,
        )
        .build()
        .acton()
        .test()
        .filter("test-beta")
        .run()
        .success()
        .assert_passed(1)
        .assert_contains("beta")
        .assert_not_contains("alpha")
        .assert_not_contains("gamma");
}

#[test]
fn test_combined_path_and_filter() {
    let project = ProjectBuilder::new("path-and-filter")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "unit_tests",
            r#"
            import "../../lib/testing/expect"

            get fun `test-unit-counter-test`() {
                expect(1).toEqual(1);
            }

            get fun `test-unit-wallet-test`() {
                expect(2).toEqual(2);
            }
        "#,
        )
        .test_file(
            "integration_tests",
            r#"
            import "../../lib/testing/expect"

            get fun `test-integration-counter-test`() {
                expect(3).toEqual(3);
            }
        "#,
        )
        .build();

    // Run only unit_tests.tolk with counter filter
    project
        .acton()
        .test()
        .path("tests/unit_tests.test.tolk")
        .filter(".*counter.*")
        .run()
        .success()
        .assert_passed(1)
        .assert_contains("unit-counter-test")
        .assert_not_contains("unit-wallet-test")
        .assert_not_contains("integration-counter-test");
}

#[test]
fn test_filter_with_no_matches() {
    ProjectBuilder::new("no-match")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test",
            r#"
            import "../../lib/testing/expect"

            get fun `test-alpha`() {
                expect(1).toEqual(1);
            }
        "#,
        )
        .build()
        .acton()
        .test()
        .filter("non-existent-test")
        .run()
        .failure()
        .assert_passed(0);
}

#[test]
fn test_fail_fast() {
    let project = ProjectBuilder::new("fail-fast")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file(
            "test1",
            r#"
            import "../../lib/testing/expect"

            get fun `test-first-pass`() {
                expect(1).toEqual(1);
            }

            get fun `test-second-fail`() {
                expect(1).toEqual(2);
            }

            get fun `test-third-pass`() {
                expect(1).toEqual(1);
            }
        "#,
        )
        .test_file(
            "test2",
            r#"
            import "../../lib/testing/expect"

            get fun `test-fourth-pass`() {
                expect(1).toEqual(1);
            }
        "#,
        )
        .build();

    // Without fail-fast: should fail but run all tests
    project
        .acton()
        .test()
        .run()
        .failure() // exit code 1 because of failure
        .assert_passed(3) // first, third, fourth
        .assert_failed(1) // second
        .assert_contains("first-pass")
        .assert_contains("second-fail")
        .assert_contains("third-pass")
        .assert_contains("fourth-pass")
        .assert_snapshot_matches("integration/snapshots/flags/test_without_fail_fast.stdout.txt");

    // With fail-fast: should stop after second test
    project
        .acton()
        .test()
        .fail_fast()
        .run()
        .failure()
        .assert_passed(1) // only first
        .assert_failed(1) // second
        .assert_contains("first-pass")
        .assert_contains("second-fail")
        .assert_not_contains("third-pass")
        .assert_not_contains("fourth-pass")
        .assert_snapshot_matches("integration/snapshots/flags/test_with_fail_fast.stdout.txt");
}

#[test]
fn test_manifest_path_allows_running_outside_project_root() {
    let project = ProjectBuilder::new("manifest-path-outside")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let project_parent = project
        .path()
        .parent()
        .expect("Project should have a parent directory");
    let manifest_path = project.path().join("Acton.toml");
    let manifest_path = manifest_path.to_string_lossy().to_string();

    project
        .acton()
        .check()
        .current_dir(project_parent)
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_allows_running_outside_project_root_without_manifest.stderr.txt",
        );

    project
        .acton()
        .arg("--manifest-path")
        .arg(&manifest_path)
        .check()
        .current_dir(project_parent)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_allows_running_outside_project_root_with_manifest.stdout.txt",
        );
}

#[test]
fn test_manifest_path_accepts_project_directory() {
    let project = ProjectBuilder::new("manifest-path-directory")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let project_parent = project
        .path()
        .parent()
        .expect("Project should have a parent directory");
    let manifest_dir = project.path().to_string_lossy().to_string();

    project
        .acton()
        .arg("--manifest-path")
        .arg(&manifest_dir)
        .check()
        .current_dir(project_parent)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_accepts_project_directory.stdout.txt",
        );
}

#[test]
fn test_manifest_path_accepts_relative_path_from_parent() {
    let project = ProjectBuilder::new("manifest-path-relative")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let project_parent = project
        .path()
        .parent()
        .expect("Project should have a parent directory");
    let project_dir_name = project
        .path()
        .file_name()
        .expect("Project directory should have a name")
        .to_string_lossy()
        .to_string();
    let relative_manifest_path = format!("{project_dir_name}/Acton.toml");

    project
        .acton()
        .arg("--manifest-path")
        .arg(&relative_manifest_path)
        .check()
        .current_dir(project_parent)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_accepts_relative_path_from_parent.stdout.txt",
        );
}

#[test]
fn test_manifest_path_missing_file_returns_clear_error() {
    let project = ProjectBuilder::new("manifest-path-missing")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let project_parent = project
        .path()
        .parent()
        .expect("Project should have a parent directory");

    project
        .acton()
        .arg("--manifest-path")
        .arg("missing/Acton.toml")
        .check()
        .current_dir(project_parent)
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_missing_file_returns_clear_error.stderr.txt",
        );
}

#[test]
fn test_manifest_path_build_works_from_nested_directory() {
    let project = ProjectBuilder::new("manifest-path-build-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    let output = project
        .acton()
        .arg("--manifest-path")
        .arg("../Acton.toml")
        .build()
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_build_works_from_nested_directory.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "build/simple.json",
            "integration/snapshots/flags/test_manifest_path_build_works_from_nested_directory.build_simple_json.txt",
        );
}

#[test]
fn test_manifest_path_check_works_from_nested_directory() {
    let project = ProjectBuilder::new("manifest-path-check-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--manifest-path")
        .arg("../Acton.toml")
        .check()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_check_works_from_nested_directory.stdout.txt",
        );
}

#[test]
fn test_manifest_path_test_works_from_nested_directory() {
    let project = ProjectBuilder::new("manifest-path-test-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file("manifest_path", PASSING_TEST)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--manifest-path")
        .arg("../Acton.toml")
        .test()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_path_test_works_from_nested_directory.stdout.txt",
        );
}

#[test]
fn test_manifest_auto_detect_build_works_from_nested_directory() {
    let project = ProjectBuilder::new("manifest-auto-build-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    let output = project
        .acton()
        .build()
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_auto_detect_build_works_from_nested_directory.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "build/simple.json",
            "integration/snapshots/flags/test_manifest_auto_detect_build_works_from_nested_directory.build_simple_json.txt",
        );
}

#[test]
fn test_manifest_auto_detect_check_works_from_nested_directory() {
    let project = ProjectBuilder::new("manifest-auto-check-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .check()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_auto_detect_check_works_from_nested_directory.stdout.txt",
        );
}

#[test]
fn test_manifest_auto_detect_test_works_from_nested_directory() {
    let project = ProjectBuilder::new("manifest-auto-test-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file("manifest_path", PASSING_TEST)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .test()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_manifest_auto_detect_test_works_from_nested_directory.stdout.txt",
        );
}

#[test]
fn test_manifest_auto_detect_stops_at_git_boundary() {
    let project = ProjectBuilder::new("manifest-auto-git-boundary")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let subrepo_dir = project.path().join("subrepo");
    let nested_dir = subrepo_dir.join("nested");
    fs::create_dir_all(subrepo_dir.join(".git")).expect("Failed to create .git boundary");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .check()
        .current_dir(&nested_dir)
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/flags/test_manifest_auto_detect_stops_at_git_boundary.stderr.txt",
        );
}

#[test]
fn test_root_flag_build_works_from_nested_directory() {
    let project = ProjectBuilder::new("root-build-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .build()
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_build_works_from_nested_directory.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "build/simple.json",
            "integration/snapshots/flags/test_root_flag_build_works_from_nested_directory.build_simple_json.txt",
        );
}

#[test]
fn test_root_flag_check_works_from_nested_directory() {
    let project = ProjectBuilder::new("root-check-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .check()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_check_works_from_nested_directory.stdout.txt",
        );
}

#[test]
fn test_root_flag_test_works_from_nested_directory() {
    let project = ProjectBuilder::new("root-test-from-nested")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file("manifest_path", PASSING_TEST)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .test()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_test_works_from_nested_directory.stdout.txt",
        );
}

#[test]
fn test_root_flag_build_out_dir_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-build-out-dir")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .build()
        .with_out_dir("dist/build")
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_build_out_dir_resolves_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "dist/build/simple.json",
            "integration/snapshots/flags/test_root_flag_build_out_dir_resolves_from_project_root.build_simple_json.txt",
        );
}

#[test]
fn test_root_flag_build_output_fift_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-build-output-fift")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .build()
        .with_output_fift("dist/fift")
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_build_output_fift_resolves_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "dist/fift/simple.fif",
            "integration/snapshots/flags/test_root_flag_build_output_fift_resolves_from_project_root.simple_fif.txt",
        );
}

#[test]
fn test_root_flag_build_gen_dir_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-build-gen-dir")
        .contract("child", SIMPLE_CONTRACT)
        .contract_with_deps("main", SIMPLE_CONTRACT, vec!["child"])
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .build()
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_build_gen_dir_resolves_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "gen/child_code.tolk",
            "integration/snapshots/flags/test_root_flag_build_gen_dir_resolves_from_project_root.child_code_tolk.txt",
        );
}

#[test]
fn test_root_flag_check_target_path_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-check-target-path")
        .contract("simple", SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .check()
        .arg("contracts/simple.tolk")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_check_target_path_resolves_from_project_root.stdout.txt",
        );
}

#[test]
fn test_root_flag_test_path_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-test-path")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file("manifest_path", PASSING_TEST)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .test()
        .path("tests/manifest_path.test.tolk")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_test_path_resolves_from_project_root.stdout.txt",
        );
}

#[test]
fn test_root_flag_test_build_function_resolves_paths_from_project_root() {
    let project = ProjectBuilder::new("root-test-build-function-paths")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file("build_function", BUILD_FUNCTION_TEST)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .test()
        .path("tests/build_function.test.tolk")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_test_build_function_resolves_paths_from_project_root.stdout.txt",
        );
}

#[test]
fn test_root_flag_script_path_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-script-path")
        .script_file("hello", SCRIPT_ROOT_TEST)
        .build();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested script directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .script("scripts/hello.tolk")
        .current_dir(&nested_dir)
        .run()
        .success();

    output.assert_snapshot_matches(
        "integration/snapshots/flags/test_root_flag_script_path_resolves_from_project_root.stdout.txt",
    );

    assert!(
        project.path().join(".acton/tolk-stdlib/common.tolk").exists(),
        "stdlib should be initialized in project root"
    );
    assert!(
        !nested_dir.join(".acton").exists(),
        "nested dir should not get its own .acton"
    );
}

#[test]
fn test_root_flag_compile_paths_resolve_from_project_root() {
    let project = ProjectBuilder::new("root-compile-paths")
        .contract("simple", SIMPLE_CONTRACT)
        .build();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested compile directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .compile("contracts/simple.tolk")
        .with_source_map("dist/simple.source_map.json")
        .with_fift_output("dist/simple.fif")
        .with_boc_output("dist/simple.boc")
        .arg("--abi")
        .arg("dist/simple.abi.json")
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_compile_paths_resolve_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "dist/simple.fif",
            "integration/snapshots/flags/test_root_flag_compile_paths_resolve_from_project_root.simple_fif.txt",
        )
        .assert_file_snapshot_matches(
            "dist/simple.abi.json",
            "integration/snapshots/flags/test_root_flag_compile_paths_resolve_from_project_root.simple_abi.json.txt",
        );

    assert!(
        project.path().join("dist/simple.boc").exists(),
        "BoC output should be created under project root"
    );
    assert!(
        project.path().join("dist/simple.source_map.json").exists(),
        "source map output should be created under project root"
    );
}

#[test]
fn test_root_flag_disasm_paths_resolve_from_project_root() {
    let project = ProjectBuilder::new("root-disasm-paths")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();
    project.acton().build().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested disasm directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .disasm_file("simple.boc")
        .with_output("dist/simple.tasm")
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_disasm_paths_resolve_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "dist/simple.tasm",
            "integration/snapshots/flags/test_root_flag_disasm_paths_resolve_from_project_root.simple_tasm.txt",
        );
}

#[test]
fn test_root_flag_disasm_source_map_path_resolves_from_project_root() {
    let project = ProjectBuilder::new("root-disasm-source-map-path")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();
    project.acton().build().run().success();

    fs::create_dir_all(project.path().join("dist")).expect("Failed to create dist directory");
    fs::write(project.path().join("dist/simple.map.json"), SOURCE_MAP_STUB)
        .expect("Failed to write source map fixture");

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested disasm directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .disasm_file("simple.boc")
        .arg("--source-map")
        .arg("dist/simple.map.json")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_disasm_source_map_path_resolves_from_project_root.stdout.txt",
        );
}

#[test]
fn test_root_flag_fmt_paths_resolve_from_project_root() {
    let project = ProjectBuilder::new("root-fmt-paths")
        .contract("simple", UNFORMATTED_CONTRACT)
        .build();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested fmt directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .fmt()
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_fmt_paths_resolve_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "contracts/simple.tolk",
            "integration/snapshots/flags/test_root_flag_fmt_paths_resolve_from_project_root.simple_contract.txt",
        );
}

#[test]
fn test_root_flag_wrapper_paths_resolve_from_project_root() {
    let project = ProjectBuilder::new("root-wrapper-paths")
        .contract("simple", SIMPLE_CONTRACT)
        .build();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested wrapper directory");

    let output = project
        .acton()
        .arg("--root")
        .arg("..")
        .wrapper("simple")
        .generate_test_stub()
        .current_dir(&nested_dir)
        .run()
        .success();

    output
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_wrapper_paths_resolve_from_project_root.stdout.txt",
        )
        .assert_file_snapshot_matches(
            "tests/wrappers/Simple.tolk",
            "integration/snapshots/flags/test_root_flag_wrapper_paths_resolve_from_project_root.wrapper.tolk.txt",
        )
        .assert_file_snapshot_matches(
            "tests/simple.test.tolk",
            "integration/snapshots/flags/test_root_flag_wrapper_paths_resolve_from_project_root.test.tolk.txt",
        );
}

#[test]
#[cfg_attr(not(unix), ignore)]
fn test_root_flag_run_executes_script_from_project_root() {
    let project = ProjectBuilder::new("root-run-path")
        .contract("simple", SIMPLE_CONTRACT)
        .script_config("show-contract", "cat contracts/simple.tolk")
        .build();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested run directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .run_script_cmd("show-contract")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_run_executes_script_from_project_root.stdout.txt",
        );
}

#[test]
fn test_root_flag_init_from_nested_directory_uses_project_root() {
    let project = ProjectBuilder::new("root-init-from-nested")
        .without_acton_toml()
        .contract("simple", SIMPLE_CONTRACT)
        .build();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested init directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .init()
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_init_from_nested_directory_uses_project_root.stdout.txt",
        );

    assert!(
        project.path().join("Acton.toml").exists(),
        "Acton.toml should be created in project root"
    );
    assert!(
        !nested_dir.join("Acton.toml").exists(),
        "Acton.toml should not be created in nested dir"
    );
}

#[test]
fn test_root_flag_logging_writes_debug_log_under_project_root() {
    let project = ProjectBuilder::new("root-logging-path")
        .contract("simple", FORMATTED_SIMPLE_CONTRACT)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested log directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .check()
        .arg("simple")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_logging_writes_debug_log_under_project_root.stdout.txt",
        );

    assert!(
        project.path().join(".acton/debug.log").exists(),
        "debug log should be created in project root"
    );
    assert!(
        !nested_dir.join(".acton/debug.log").exists(),
        "nested directory should not contain debug log"
    );
}

#[test]
fn test_root_flag_check_import_path_can_use_mappings_from_nested_directory() {
    let project = ProjectBuilder::new("root-check-mappings")
        .mapping("libs", "./libs")
        .file(
            "libs/math",
            r#"
            fun plusOne(value: int): int {
                return value + 1;
            }
            "#,
        )
        .contract(
            "main",
            r#"
            import "../libs/math.tolk";

            fun onInternalMessage(in: InMessage) {
                plusOne(1);
            }
            fun onBouncedMessage(_: InMessageBounced) {}
            "#,
        )
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested check directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .check()
        .arg("main")
        .current_dir(&nested_dir)
        .run()
        .success()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_check_import_path_can_use_mappings_from_nested_directory.stderr.txt",
        );
}

#[test]
fn test_root_flag_test_failure_paths_are_project_relative() {
    let project = ProjectBuilder::new("root-test-failure-path")
        .contract("simple", SIMPLE_CONTRACT)
        .test_file("failing_path", FAILING_PATH_TEST)
        .build();
    project.acton().init().run().success();

    let nested_dir = project.path().join("nested");
    fs::create_dir_all(&nested_dir).expect("Failed to create nested test directory");

    project
        .acton()
        .arg("--root")
        .arg("..")
        .test()
        .path("tests/failing_path.test.tolk")
        .with_backtrace("full")
        .current_dir(&nested_dir)
        .run()
        .failure()
        .assert_snapshot_matches(
            "integration/snapshots/flags/test_root_flag_test_failure_paths_are_project_relative.stdout.txt",
        );
}
