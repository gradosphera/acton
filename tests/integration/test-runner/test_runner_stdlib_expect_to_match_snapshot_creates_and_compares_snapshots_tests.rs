use crate::support::TestOutputExt;
use crate::support::project::ProjectBuilder;
use std::collections::BTreeMap;

const EXPECT_IMPORTS: &str = r#"
import "../../lib/testing/expect"
"#;

const SNAPSHOT_FILE: &str = "tests/__snapshots__/expect_snapshot.test.tolk.snap.json";

#[test]
fn expect_to_match_snapshot_creates_named_snapshot_file() {
    let source = format!(
        r#"{EXPECT_IMPORTS}

get fun `test-by-expect-snapshot-create`() {{
    expect("alpha").toMatchSnapshot("text");
}}
"#
    );

    let project = ProjectBuilder::new("by-stdlib-expect-snapshot-create")
        .test_file("expect_snapshot", &source)
        .build();

    project
        .acton()
        .test()
        .run()
        .success()
        .assert_passed(1)
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_stdlib_expect_to_match_snapshot_creates_and_compares_snapshots_tests/expect_to_match_snapshot_creates_named_snapshot_file.stdout.txt",
        );

    let stored = std::fs::read_to_string(project.path().join(SNAPSHOT_FILE))
        .expect("Snapshot file must be created");
    let parsed: BTreeMap<String, String> =
        serde_json::from_str(&stored).expect("Snapshot file must be valid JSON");
    assert_eq!(parsed.get("text"), Some(&"alpha".to_string()));
}

#[test]
fn expect_to_match_snapshot_reuses_existing_named_snapshot_in_same_test() {
    let source = format!(
        r#"{EXPECT_IMPORTS}

get fun `test-by-expect-snapshot-reuse-same-name`() {{
    expect(123).toMatchSnapshot("counter");
    expect(123).toMatchSnapshot("counter");
}}
"#
    );

    let project = ProjectBuilder::new("by-stdlib-expect-snapshot-reuse")
        .test_file("expect_snapshot", &source)
        .build();

    project
        .acton()
        .test()
        .run()
        .success()
        .assert_passed(1)
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_stdlib_expect_to_match_snapshot_creates_and_compares_snapshots_tests/expect_to_match_snapshot_reuses_existing_named_snapshot_in_same_test.stdout.txt",
        );

    let stored = std::fs::read_to_string(project.path().join(SNAPSHOT_FILE))
        .expect("Snapshot file must be created");
    let parsed: BTreeMap<String, String> =
        serde_json::from_str(&stored).expect("Snapshot file must be valid JSON");
    assert_eq!(parsed.len(), 1);
    assert_eq!(parsed.get("counter"), Some(&"123".to_string()));
}

#[test]
fn expect_to_match_snapshot_reports_mismatch_and_keeps_existing_value() {
    let source = format!(
        r#"{EXPECT_IMPORTS}

get fun `test-by-expect-snapshot-mismatch`() {{
    expect(124).toMatchSnapshot("counter");
}}
"#
    );

    let project = ProjectBuilder::new("by-stdlib-expect-snapshot-mismatch")
        .test_file("expect_snapshot", &source)
        .raw_file(SNAPSHOT_FILE, "{\n  \"counter\": \"123\"\n}\n")
        .build();

    project
        .acton()
        .test()
        .run()
        .failure()
        .assert_failed(1)
        .assert_contains("expect(actual).toMatchSnapshot(expected)")
        .assert_contains("Snapshot key: counter")
        .assert_contains("--- Expected ---")
        .assert_contains("--- Actual ---")
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_stdlib_expect_to_match_snapshot_creates_and_compares_snapshots_tests/expect_to_match_snapshot_reports_mismatch_and_keeps_existing_value.stdout.txt",
        );

    let stored = std::fs::read_to_string(project.path().join(SNAPSHOT_FILE))
        .expect("Snapshot file must still exist");
    let parsed: BTreeMap<String, String> =
        serde_json::from_str(&stored).expect("Snapshot file must be valid JSON");
    assert_eq!(parsed.get("counter"), Some(&"123".to_string()));
}

#[test]
fn expect_to_match_snapshot_without_name_uses_distinct_location_keys() {
    let source = format!(
        r#"{EXPECT_IMPORTS}

get fun `test-by-expect-snapshot-without-name`() {{
    expect(1).toMatchSnapshot();
    expect(2).toMatchSnapshot();
}}
"#
    );

    let project = ProjectBuilder::new("by-stdlib-expect-snapshot-no-name")
        .test_file("expect_snapshot", &source)
        .build();

    project
        .acton()
        .test()
        .run()
        .success()
        .assert_passed(1)
        .assert_snapshot_matches(
            "integration/snapshots/test-runner/test_runner_stdlib_expect_to_match_snapshot_creates_and_compares_snapshots_tests/expect_to_match_snapshot_without_name_uses_distinct_location_keys.stdout.txt",
        );

    let stored = std::fs::read_to_string(project.path().join(SNAPSHOT_FILE))
        .expect("Snapshot file must be created");
    let parsed: BTreeMap<String, String> =
        serde_json::from_str(&stored).expect("Snapshot file must be valid JSON");
    assert_eq!(parsed.len(), 2);
    assert!(parsed.values().any(|v| v == "1"));
    assert!(parsed.values().any(|v| v == "2"));
}
