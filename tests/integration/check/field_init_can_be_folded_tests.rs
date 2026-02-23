use crate::integration::check::run_fix_test;
use crate::integration::check::run_simple_test;
use function_name::named;

#[test]
#[named]
fn test_check_field_init_can_be_folded() {
    run_simple_test(
        "field_init_can_be_folded",
        r#"
            struct Foo {
                bar: int,
            }

            fun fold(bar: int): Foo {
                return Foo {
                    bar: bar,
                };
            }
        "#,
        function_name!(),
    )
}

#[test]
#[named]
fn test_fix_field_init_can_be_folded() {
    run_fix_test(
        r#"
            struct Foo {
                bar: int,
            }

            fun fold(bar: int): Foo {
                return Foo {
                    bar: bar,
                };
            }
        "#,
        r#"
            struct Foo {
                bar: int,
            }

            fun fold(bar: int): Foo {
                return Foo {
                    bar,
                };
            }
        "#,
        function_name!(),
    );
}
