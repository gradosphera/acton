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

extension!(format1 in (Context) with (arg1: TupleItem, type1: String, fmt: String) using format1_impl);
fn format1_impl(ctx: &mut Context, stack: &mut Tuple, arg1: TupleItem, type1: String, fmt: String) {
    let args = vec![(type1, arg1)];
    let result = format_args(ctx, fmt, args);
    stack.push_string(&result)
}

extension!(format2 in (Context) with (arg2: TupleItem, type2: String, arg1: TupleItem, type1: String, fmt: String) using format2_impl);
fn format2_impl(
    ctx: &mut Context,
    stack: &mut Tuple,
    arg2: TupleItem,
    type2: String,
    arg1: TupleItem,
    type1: String,
    fmt: String,
) {
    let args = vec![(type1, arg1), (type2, arg2)];
    let result = format_args(ctx, fmt, args);
    stack.push_string(&result)
}

extension!(format3 in (Context) with (arg3: TupleItem, type3: String, arg2: TupleItem, type2: String, arg1: TupleItem, type1: String, fmt: String) using format3_impl);
fn format3_impl(
    ctx: &mut Context,
    stack: &mut Tuple,
    arg3: TupleItem,
    type3: String,
    arg2: TupleItem,
    type2: String,
    arg1: TupleItem,
    type1: String,
    fmt: String,
) {
    let args = vec![(type1, arg1), (type2, arg2), (type3, arg3)];
    let result = format_args(ctx, fmt, args);
    stack.push_string(&result)
}

extension!(format4 in (Context) with (arg4: TupleItem, type4: String, arg3: TupleItem, type3: String, arg2: TupleItem, type2: String, arg1: TupleItem, type1: String, fmt: String) using format4_impl);
fn format4_impl(
    ctx: &mut Context,
    stack: &mut Tuple,
    arg4: TupleItem,
    type4: String,
    arg3: TupleItem,
    type3: String,
    arg2: TupleItem,
    type2: String,
    arg1: TupleItem,
    type1: String,
    fmt: String,
) {
    let args = vec![(type1, arg1), (type2, arg2), (type3, arg3), (type4, arg4)];
    let result = format_args(ctx, fmt, args);
    stack.push_string(&result)
}

extension!(format5 in (Context) with (arg5: TupleItem, type5: String, arg4: TupleItem, type4: String, arg3: TupleItem, type3: String, arg2: TupleItem, type2: String, arg1: TupleItem, type1: String, fmt: String) using format5_impl);
fn format5_impl(
    ctx: &mut Context,
    stack: &mut Tuple,
    arg5: TupleItem,
    type5: String,
    arg4: TupleItem,
    type4: String,
    arg3: TupleItem,
    type3: String,
    arg2: TupleItem,
    type2: String,
    arg1: TupleItem,
    type1: String,
    fmt: String,
) {
    let args = vec![
        (type1, arg1),
        (type2, arg2),
        (type3, arg3),
        (type4, arg4),
        (type5, arg5),
    ];
    let result = format_args(ctx, fmt, args);
    stack.push_string(&result)
}

fn format_args(ctx: &mut Context, mut fmt: String, args: Vec<(String, TupleItem)>) -> String {
    for (type_name, arg) in args {
        let typed_arg = if let TupleItem::Tuple(tuple) = &arg {
            TupleItem::TypedTuple {
                abi: ctx.abi.find_type(&type_name),
                items: tuple.clone(),
                type_name,
            }
        } else {
            arg
        };

        let formatted_arg = format!("{}", typed_arg);
        let formatted_arg = if formatted_arg.starts_with("\"") {
            &formatted_arg[1..formatted_arg.len() - 1]
        } else {
            formatted_arg.as_str()
        };

        if let Some(pos) = fmt.find("{}") {
            fmt.replace_range(pos..pos + 2, formatted_arg);
        }
    }
    fmt
}

pub fn register_extensions(executor: &mut Executor, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        1 => println,
        2 => eprintln,
        200 => format1,
        201 => format2,
        202 => format3,
        203 => format4,
        204 => format5,
    });
}

pub fn register_get_extensions(executor: &mut GetExecutor, ctx: &mut Context) {
    register_ext_methods!(executor, ctx, {
        1 => println,
        2 => eprintln,
        200 => format1,
        201 => format2,
        202 => format3,
        203 => format4,
        204 => format5,
    });
}
