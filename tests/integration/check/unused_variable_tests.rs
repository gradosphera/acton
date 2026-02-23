use crate::integration::check::run_fix_test;
use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_unused_variable() {
    run_simple_test(
        "unused_variable",
        r#"
            fun main() {
                val unused = 1;
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_fix_unused_variable() {
    run_fix_test(
        r#"
            fun main() {
                val unused = 1;
            }
        "#,
        r#"
            fun main() {
                val _unused = 1;
            }
        "#,
        function_name!(),
    );
}
