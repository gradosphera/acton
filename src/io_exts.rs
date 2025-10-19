use crate::context::Context;
use emulator::executor::Executor;
use emulator::get_executor::GetExecutor;
use emulator::tuple::stack::{Tuple, TupleItem};
use emulator::{extension, pop_args, register_ext_methods};

extension!(println in (Context) with (s: TupleItem, type_name: String) using println_impl);
fn println_impl(ctx: &mut Context, _stack: &mut Tuple, s: TupleItem, type_name: String) {
    let typed_tuple = if let TupleItem::Tuple(tuple) = &s {
        TupleItem::TypedTuple {
            abi: ctx.abi.find_type(&type_name),
            items: tuple.clone(),
            type_name,
        }
    } else {
        s
    };
    let formatted = format!("{}", typed_tuple);
    let formatted = if formatted.starts_with("\"") {
        &formatted[1..formatted.len() - 1]
    } else {
        formatted.as_str()
    };

    if ctx.capture_test_output {
        ctx.stdout_buffer.push_str(formatted);
        ctx.stdout_buffer.push_str("\n");
    } else {
        println!("{}", formatted);
    }
}

extension!(eprintln in (Context) with (s: String) using eprintln_impl);
fn eprintln_impl(ctx: &mut Context, _stack: &mut Tuple, s: String) {
    let formatted = format!("{}", s);
    let formatted = if formatted.starts_with("\"") {
        &formatted[1..formatted.len() - 1]
    } else {
        formatted.as_str()
    };

    if ctx.capture_test_output {
        ctx.stderr_buffer.push_str(&formatted);
        ctx.stderr_buffer.push_str("\n");
    } else {
        eprintln!("{}", s);
    }
}

pub fn register_extensions(executor: &mut Executor, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        1 => println,
        2 => eprintln,
    });
}

pub fn register_get_extensions(executor: &mut GetExecutor, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        1 => println,
        2 => eprintln,
    });
}
