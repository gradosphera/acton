use crate::support::{ProjectBuilder, TestOutputExt};
use std::fs;

#[test]
fn test_script_simple_execution() {
    let project = ProjectBuilder::new("script-simple")
        .script_file(
            "hello",
            r#"
            import "../../lib/io"

            fun main() {
                println("Hello from script!");
            }
        "#,
        )
        .build();

    let output = project.acton().script("scripts/hello.tolk").run().code(0);

    output.assert_contains("Hello from script!");
}

#[test]
fn test_script_with_calculations() {
    let project = ProjectBuilder::new("script-calc")
        .script_file(
            "calc",
            r#"
            import "../../lib/io"

            fun main() {
                val result = 2 + 2 * 2;
                println("Result: ");
                println(result);
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/calc.tolk")
        .run()
        .code(0)
        .assert_contains("Result:")
        .assert_contains("6");
}

#[test]
fn test_script_file_not_found() {
    let project = ProjectBuilder::new("script-not-found").build();

    project
        .acton()
        .script("scripts/nonexistent.tolk")
        .run()
        .failure()
        .assert_stderr_contains("Cannot find file or directory");
}

#[test]
fn test_script_not_a_file() {
    let project = ProjectBuilder::new("script-dir").build();

    fs::create_dir_all(project.path().join("scripts")).unwrap();

    project
        .acton()
        .script("scripts")
        .run()
        .failure()
        .assert_stderr_contains("is not a file");
}

#[test]
fn test_script_wrong_extension() {
    let project = ProjectBuilder::new("script-wrong-ext").build();

    fs::create_dir_all(project.path().join("scripts")).unwrap();
    fs::write(project.path().join("scripts/test.txt"), "some content").unwrap();

    project
        .acton()
        .script("scripts/test.txt")
        .run()
        .failure()
        .assert_stderr_contains("must end with .tolk");
}

// ========================================
// Script Compilation Tests
// ========================================

#[test]
fn test_script_compilation_error() {
    let project = ProjectBuilder::new("script-compile-error")
        .script_file(
            "broken",
            r#"
            fun main() {
                val x = nonexistent_function();
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/broken.tolk")
        .run()
        .failure()
        .assert_stderr_contains("undefined symbol")
        .assert_stderr_snapshot_matches(
            "integration/snapshots/test_script_compilation_error.stderr.txt",
        );
}

#[test]
fn test_script_syntax_error() {
    let project = ProjectBuilder::new("script-syntax")
        .script_file(
            "syntax",
            r#"
            val x = {{{;
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/syntax.tolk")
        .run()
        .failure();
}

// ========================================
// Script with Libraries Tests
// ========================================

#[test]
fn test_script_with_multiple_operations() {
    let project = ProjectBuilder::new("script-multi")
        .script_file(
            "multi",
            r#"
            import "../../lib/io"

            fun main() {
                println("Step 1");
                val a = 10;
                println("Step 2");
                val b = 20;
                println("Step 3");
                val sum = a + b;
                println("Sum: ");
                println(sum);
            }
        "#,
        )
        .build();

    let output = project.acton().script("scripts/multi.tolk").run().code(0);

    output
        .assert_contains("Step 1")
        .assert_contains("Step 2")
        .assert_contains("Step 3")
        .assert_contains("Sum:")
        .assert_contains("30");
}

// ========================================
// Clear Cache Tests
// ========================================

#[test]
fn test_script_with_clear_cache() {
    let project = ProjectBuilder::new("script-cache")
        .script_file(
            "test",
            r#"
            import "../../lib/io"

            fun main() {
                println("Running with cache clear");
            }
        "#,
        )
        .build();

    project.acton().script("scripts/test.tolk").run().code(0);

    project
        .acton()
        .script("scripts/test.tolk")
        .clear_cache()
        .run()
        .code(0)
        .assert_contains("Cache cleared");
}

// ========================================
// Exit Code Tests
// ========================================

#[test]
fn test_script_custom_exit_code() {
    let project = ProjectBuilder::new("script-exit")
        .script_file(
            "exit_42",
            r#"
            import "../../lib/io"

            fun main() {
                println("Exiting with code 42");
                throw 42
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/exit_42.tolk")
        .run()
        .code(42);
}

#[test]
fn test_script_success_exit_code() {
    let project = ProjectBuilder::new("script-success")
        .script_file(
            "success",
            r#"
            import "../../lib/io"

            fun main() {
                println("Success!");
            }
        "#,
        )
        .build();

    project.acton().script("scripts/success.tolk").run().code(0);
}

// ========================================
// Snapshot Tests
// ========================================

#[test]
fn test_script_output_snapshot() {
    let project = ProjectBuilder::new("script-snapshot")
        .script_file(
            "output",
            r#"
            import "../../lib/io"

            fun main() {
                println("Line 1");
                println("Line 2");
                println("Line 3");
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/output.tolk")
        .run()
        .code(0)
        .assert_snapshot_matches("integration/snapshots/test_script_output_snapshot.stdout.txt");
}
