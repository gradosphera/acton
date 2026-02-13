use crate::support::TestOutputExt;
use crate::support::project::ProjectBuilder;
use function_name::named;

#[test]
#[named]
fn test_check_deprecated_function_use() {
    let project = ProjectBuilder::new("check-deprecated")
        .contract(
            "main",
            r#"
                @deprecated
                fun foo() {}

                fun main() {
                    foo();
                }
            "#,
        )
        .build();

    project.acton().init().run().success();

    project
        .acton()
        .check()
        .run()
        .success()
        .assert_stderr_snapshot_matches(&format!(
            "integration/snapshots/check/{}.txt",
            function_name!()
        ));
}

#[test]
#[named]
fn test_check_deprecated_function_use_with_message() {
    let project = ProjectBuilder::new("check-deprecated")
        .contract(
            "main",
            r#"
                @deprecated("use bar instead")
                fun foo() {}

                fun main() {
                    foo();
                }
            "#,
        )
        .build();

    project.acton().init().run().success();

    project
        .acton()
        .check()
        .run()
        .success()
        .assert_stderr_snapshot_matches(&format!(
            "integration/snapshots/check/{}.txt",
            function_name!()
        ));
}
