use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_parameter_mutation_requires_mutable_assignment() {
    run_simple_test(
        "parameter_mutation_requires_mutable",
        r#"
            fun main(value: int) {
                value = 10;
                value;
            }
        "#,
        function_name!(),
    );
}

#[test]
#[named]
fn test_check_parameter_mutation_requires_mutable_set_assignment() {
    run_simple_test(
        "parameter_mutation_requires_mutable",
        r#"
            fun main(value: int) {
                value += 10;
                value;
            }
        "#,
        function_name!(),
    );
}

#[test]
#[named]
fn test_check_parameter_mutation_requires_mutable_mutate_argument() {
    run_simple_test(
        "parameter_mutation_requires_mutable",
        r#"
            fun bump(mutate x: int) {
                x += 1;
            }

            fun main(value: int) {
                bump(mutate value);
            }
        "#,
        function_name!(),
    );
}

#[test]
#[named]
fn test_check_parameter_mutation_requires_mutable_mutable_parameter_is_ignored() {
    run_simple_test(
        "parameter_mutation_requires_mutable",
        r#"
            fun assign(mutate value: int) {
                value = 10;
                value;
            }

            fun main() {
                var x = 1;
                assign(mutate x);
                x;
            }
        "#,
        function_name!(),
    );
}
