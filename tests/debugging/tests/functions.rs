use crate::debugging::support::assertions::{DebugTestOutput, DebugTestOutputExt};
use crate::debugging::support::debug::DebugBuilder;

#[test]
fn test_inline_function_call_step_in() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    return my_sum(foo, foo);
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/inline/step_in.trace.txt",
    );

    Ok(())
}

#[test]
fn test_inline_function_call_step_over() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    val goo = my_sum(foo, foo);
    return foo + goo;
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_over()?;
        executor.step_over()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/inline/step_over.trace.txt",
    );

    Ok(())
}

#[test]
fn test_inline_function_call_step_out() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    val goo = my_sum(foo, foo);
    return foo + goo;
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_over()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_out()?;
        executor.step_over()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/inline/step_out.trace.txt",
    );

    Ok(())
}

#[test]
fn test_ref_inline_function_call_step_in() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

@inline_ref
fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    return my_sum(foo, foo);
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/ref_inline/step_in.trace.txt",
    );

    Ok(())
}

#[test]
fn test_ref_inline_function_call_step_over() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

@inline_ref
fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    val result = my_sum(foo, foo);
    return result + foo;
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_over()?;
        executor.step_over()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/ref_inline/step_over.trace.txt",
    );

    Ok(())
}

#[test]
fn test_ref_inline_function_call_step_out() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

@inline_ref
fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    val result = my_sum(foo, foo);
    return result + foo;
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_over()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_out()?;
        executor.step_over()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/ref_inline/step_out.trace.txt",
    );

    Ok(())
}

#[test]
fn test_noinline_function_call_step_in() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

@method_id(123)
fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    return my_sum(foo, foo);
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/no_inline/step_in.trace.txt",
    );

    Ok(())
}

#[test]
fn test_noinline_function_call_step_over() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

@method_id(123)
fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    val res = my_sum(foo, foo);
    return res + foo;
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_over()?;
        executor.step_over()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/no_inline/step_over.trace.txt",
    );

    Ok(())
}

#[test]
fn test_noinline_function_call_step_out() -> anyhow::Result<()> {
    let code = r#"
global foo: int;

@method_id(123)
fun my_sum(a: int, b: int): int {
    return a + b;
}

fun main() {
    foo = 300;
    val res = my_sum(foo, foo);
    return res + foo;
}
"#;

    let session = DebugBuilder::new("debug-callback").code(code).build();

    let mut client = session.start();

    let result = client.execute(|executor| {
        executor.step_over()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_in()?;
        executor.step_out()?;
        executor.step_over()?;
        Ok(())
    })?;

    let debug_output = DebugTestOutput::new(result);
    debug_output.assert_trace_snapshot_matches(
        "debugging/snapshots/function_call/no_inline/step_out.trace.txt",
    );

    Ok(())
}
