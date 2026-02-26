use crate::integration::check::run_fix_test;
use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_negated_is_type_can_use_not_is_reports_negated_is() {
    run_simple_test(
        "negated_is_type_can_use_not_is",
        r#"
            fun main(a: int?) {
                val b = !(a is int);
                b;
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_check_negated_is_type_can_use_not_is_reports_with_nested_parens() {
    run_simple_test(
        "negated_is_type_can_use_not_is",
        r#"
            fun main(a: int?) {
                val b = !(((a)) is int);
                b;
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_check_negated_is_type_can_use_not_is_ignores_not_is() {
    run_simple_test(
        "negated_is_type_can_use_not_is",
        r#"
            fun main(a: int?) {
                val b = a !is int;
                b;
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_check_negated_is_type_can_use_not_is_ignores_non_is_negation() {
    run_simple_test(
        "negated_is_type_can_use_not_is",
        r#"
            fun main(flag: bool) {
                val b = !flag;
                b;
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_check_negated_is_type_can_use_not_is_ignores_negated_not_is() {
    run_simple_test(
        "negated_is_type_can_use_not_is",
        r#"
            fun main(a: int?) {
                val b = !(a !is int);
                b;
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_fix_negated_is_type_can_use_not_is() {
    run_fix_test(
        r#"
            fun main(a: int?) {
                val b = !(a is int);
                b;
            }
        "#,
        r#"
            fun main(a: int?) {
                val b = a !is int;
                b;
            }
        "#,
        function_name!(),
    );
}

#[test]
#[named]
fn test_fix_negated_is_type_can_use_not_is_with_nested_parens() {
    run_fix_test(
        r#"
            fun main(a: int?) {
                val b = !(((a)) is int);
                b;
            }
        "#,
        r#"
            fun main(a: int?) {
                val b = ((a)) !is int;
                b;
            }
        "#,
        function_name!(),
    );
}
