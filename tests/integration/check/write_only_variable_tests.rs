use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_write_only_variable() {
    run_simple_test(
        "write_only_variable",
        r#"
            fun main() {
                var counter = 0;
                counter = 1;
            }
        "#,
        function_name!(),
    )
}
