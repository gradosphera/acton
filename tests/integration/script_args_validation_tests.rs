use crate::support::TestOutputExt;
use crate::support::project::ProjectBuilder;

use tycho_types::boc::Boc;
use tycho_types::cell::CellBuilder;

#[test]
fn test_script_args_accept_supported_literals() {
    let project = ProjectBuilder::new("script-args-supported-literals")
        .script_file(
            "supported",
            r#"
            import "../../lib/io"

            fun main(a: int, b: int?, t: tuple, x: [int, int], s: slice, c: cell, sl: slice) {
                println1("a: {}", a);
                println1("b: {}", b);
                println("ok");
            }
        "#,
        )
        .build();

    let mut cell_builder = CellBuilder::new();
    cell_builder.store_uint(123, 32).ok();
    let cell = cell_builder.build().ok().unwrap_or_default();
    let cell_hex = Boc::encode_hex(cell.clone());
    let slice_hex = Boc::encode_hex(cell);

    project
        .acton()
        .script("scripts/supported.tolk")
        .arg("42")
        .arg("null")
        .arg("(NaN 10)")
        .arg("[1 2]")
        .arg("\"hello\"")
        .arg(&format!("C{{{cell_hex}}}"))
        .arg(&format!("CS{{{slice_hex}}}"))
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/script_args_validation/test_script_args_accept_supported_literals.stdout.txt",
        );
}

#[test]
fn test_script_args_accept_supported_literals_with_clear_cache() {
    let project = ProjectBuilder::new("script-args-clear-cache")
        .script_file(
            "sum",
            r#"
            import "../../lib/io"

            fun main(a: int, b: int) {
                println(a + b);
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/sum.tolk")
        .arg("7")
        .arg("8")
        .clear_cache()
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/script_args_validation/test_script_args_accept_supported_literals_with_clear_cache.stdout.txt",
        );
}

#[test]
fn test_script_args_reject_builder_literal() {
    let project = ProjectBuilder::new("script-args-builder")
        .script_file(
            "builder",
            r#"
            import "../../lib/io"

            fun main(c: cell) {
                println(c);
            }
        "#,
        )
        .build();

    let mut builder = CellBuilder::new();
    builder.store_uint(999, 32).ok();
    let cell = builder.build().ok().unwrap_or_default();
    let cell_hex = Boc::encode_hex(cell);

    project
        .acton()
        .script("scripts/builder.tolk")
        .arg(&format!("BC{{{cell_hex}}}"))
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/script_args_validation/test_script_args_reject_builder_literal.stderr.txt",
        );
}

#[test]
fn test_script_args_reject_malformed_literal() {
    let project = ProjectBuilder::new("script-args-malformed")
        .script_file(
            "malformed",
            r#"
            import "../../lib/io"

            fun main(a: int) {
                println1("a: {}", a);
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/malformed.tolk")
        .arg("[ 10")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/script_args_validation/test_script_args_reject_malformed_literal.stderr.txt",
        );
}

#[test]
fn test_script_args_reject_missing_required_argument() {
    let project = ProjectBuilder::new("script-args-missing-required")
        .script_file(
            "missing",
            r#"
            import "../../lib/io"

            fun main(a: int) {
                println1("a: {}", a);
            }
        "#,
        )
        .build();

    project
        .acton()
        .script("scripts/missing.tolk")
        .run()
        .failure()
        .assert_stderr_snapshot_matches(
            "integration/snapshots/script_args_validation/test_script_args_reject_missing_required_argument.stderr.txt",
        );
}
