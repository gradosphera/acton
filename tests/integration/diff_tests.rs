use crate::support::assertions::TestOutputExt;
use crate::support::project::ProjectBuilder;

#[test]
fn test_diff_for_numbers() {
    let project = ProjectBuilder::new("diff-numbers")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            get fun `test diff`() {
                expect(10).toEqual(20)
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_numbers.stdout.txt");
}

#[test]
fn test_diff_for_tensors() {
    let project = ProjectBuilder::new("diff-tensors")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            get fun `test diff`() {
                expect((10, 20)).toEqual((10, 30))
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_tensors.stdout.txt");
}

#[test]
fn test_diff_for_bools() {
    let project = ProjectBuilder::new("diff-bools")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            get fun `test diff`() {
                expect(true).toEqual(false)
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_bools.stdout.txt");
}

#[test]
fn test_diff_for_strings() {
    let project = ProjectBuilder::new("diff-strings")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            get fun `test diff`() {
                expect("hello").toEqual("world")
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_strings.stdout.txt");
}

#[test]
fn test_diff_for_nullables() {
    let project = ProjectBuilder::new("diff-nullables")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            get fun `test diff`() {
                expect(10).toEqual(null)
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_nullables.stdout.txt");
}

#[test]
fn test_diff_for_structs() {
    let project = ProjectBuilder::new("diff-structs")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            struct Point {
                x: int,
                y: int
            }

            get fun `test diff`() {
                expect(Point{x: 1, y: 2}).toEqual(Point{x: 1, y: 3})
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_structs.stdout.txt");
}

#[test]
fn test_diff_for_nested_structs() {
    let project = ProjectBuilder::new("diff-nested-structs")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            struct Line {
                start: Point
                end: Point
            }

            struct Point {
                x: int
                y: int
            }

            get fun `test diff`() {
                expect(Line { start: Point{ x: 1, y: 2 }, end: Point{ x: 1, y: 3 } }).toEqual(Line { start: Point{ x: 2, y: 2 }, end: Point{ x: 2, y: 3 } })
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_nested_structs.stdout.txt");
}

#[test]
fn test_diff_for_addresses() {
    let project = ProjectBuilder::new("diff-addresses")
        .contract("simple", "fun main() {}")
        .test_file(
            "simple",
            r#"
            import "../../lib/testing/expect"

            get fun `test diff`() {
                val addr1 = address("EQC2jeGorIAFh2LXwsDjHfRK-GSo9UzchdIEMh24A7T7AHot");
                val addr2 = address("EQD__________________________________________0vo");
                expect(addr1).toEqual(addr2);
            }
        "#,
        )
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_snapshot_matches("integration/snapshots/test_diff_for_addresses.stdout.txt");
}
