use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_pure_function_call_unused() {
    run_simple_test(
        "pure_function_call_unused",
        r#"
            @pure
            fun add(a: int, b: int): int {
                return a + b;
            }

            fun main() {
                add(1, 2);
            }
        "#,
        function_name!(),
    )
}
