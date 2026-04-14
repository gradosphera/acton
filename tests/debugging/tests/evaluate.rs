use crate::debugging::support::debug::DebugBuilder;
use anyhow::bail;
use std::fs;
use tempfile::tempdir;
use tolkc::{Compiler, CompilerResult};
use ton::block_tlb::StateInit;
use ton::ton_core::cell::TonCell;
use ton::ton_core::traits::tlb::TLB;
use tvmffi::stack::{Tuple, TupleItem};
use tycho_types::models::{StdAddr, StdAddrFormat};

fn compile_contract_address(code: &str) -> anyhow::Result<String> {
    let temp_dir = tempdir()?;
    let path = temp_dir.path().join("main.tolk");
    fs::write(&path, code)?;

    let compiled = match Compiler::new(2).compile(&path, true) {
        CompilerResult::Success(result) => result,
        CompilerResult::Error(error) => bail!("Cannot compile test script: {}", error.message),
    };
    let code_cell = TonCell::from_boc_base64(&compiled.code_boc64)?;
    let address = StateInit::new(code_cell, TonCell::empty().clone()).derive_address(0)?;
    let (address, _) = StdAddr::from_str_ext(&address.to_string(), StdAddrFormat::any())?;
    Ok(address.to_string())
}

#[test]
fn test_evaluate_zero_arg_function_call() -> anyhow::Result<()> {
    let code = r"
fun helper(): int {
    return 42;
}

fun main() {
    val value = 1;
    return value;
}
";

    let session = DebugBuilder::new("debug-evaluate-zero-arg")
        .code(code)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        let value = executor.evaluate("helper()")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_one_arg_function_call() -> anyhow::Result<()> {
    let code = r"
fun helper(x: int): int {
    return x + 1;
}

fun main(value: int) {
    return value;
}
";

    let session = DebugBuilder::new("debug-evaluate-one-arg")
        .code(code)
        .accept_int(41)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        let value = executor.evaluate("helper(value)")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_literal_argument_function_call() -> anyhow::Result<()> {
    let code = r"
fun helper(x: int): int {
    return x + 1;
}

fun main() {
    return 0;
}
";

    let session = DebugBuilder::new("debug-evaluate-literal-arg")
        .code(code)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        let value = executor.evaluate("helper(41)")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_multi_arg_function_call() -> anyhow::Result<()> {
    let code = r"
fun helper(a: int, b: int): int {
    return a + b;
}

fun main(left: int, right: int) {
    return left + right;
}
";

    let session = DebugBuilder::new("debug-evaluate-multi-arg")
        .code(code)
        .stack(Tuple(vec![
            TupleItem::Int(20.into()),
            TupleItem::Int(22.into()),
        ]))
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        let value = executor.evaluate("helper(left, right)")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_mixed_local_and_literal_arguments() -> anyhow::Result<()> {
    let code = r"
fun helper(a: int, b: int): int {
    return a + b;
}

fun main(value: int) {
    return value;
}
";

    let session = DebugBuilder::new("debug-evaluate-mixed-args")
        .code(code)
        .accept_int(41)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        let value = executor.evaluate("helper(value, 1)")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_field_access_argument_function_call() -> anyhow::Result<()> {
    let code = r"
struct BoxedInt {
    value: int,
}

fun helper(x: int): int {
    return x + 1;
}

fun main() {
    val boxed = BoxedInt { value: 41 };
    return boxed.value;
}
";

    let session = DebugBuilder::new("debug-evaluate-field-arg")
        .code(code)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        executor.step_over()?;
        let value = executor.evaluate("helper(boxed.value)")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_field_access_argument_on_lazy_struct_function_call() -> anyhow::Result<()> {
    let code = r"
struct LazyBoxedInt {
    value: uint32,
    other: uint32,
}

fun helper(x: uint32): uint32 {
    return x + 1;
}

fun main() {
    val packed = LazyBoxedInt { value: 41, other: 999 }.toCell().beginParse();
    val boxed = lazy LazyBoxedInt.fromSlice(packed);
    val current = boxed.value;
    return current;
}
";

    let session = DebugBuilder::new("debug-evaluate-lazy-field-arg")
        .code(code)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        executor.step_over_times(3)?;
        let value = executor.evaluate("helper(boxed.value)")?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_multiline_function_call_with_trailing_comma() -> anyhow::Result<()> {
    let code = r"
struct BoxedInt {
    value: int,
}

fun calcDeployedJettonWallet(a: int, b: int, c: int): int {
    return a + b + c;
}

fun main() {
    val boxed = BoxedInt { value: 14 };
    return boxed.value;
}
";

    let session = DebugBuilder::new("debug-evaluate-multiline-call")
        .code(code)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        executor.step_over()?;
        let value = executor.evaluate(
            "calcDeployedJettonWallet(
                boxed.value,
                boxed.value,
                14,
            )",
        )?;
        assert_eq!(value, "42");
        Ok(())
    })?;

    Ok(())
}

#[test]
fn test_evaluate_contract_get_address_from_c7() -> anyhow::Result<()> {
    let code = r"
fun main() {
    return 0;
}
";
    let expected_address = compile_contract_address(code)?;

    let session = DebugBuilder::new("debug-evaluate-contract-get-address")
        .code(code)
        .build();
    let mut client = session.start();

    let _result = client.execute(|executor| {
        let value = executor.evaluate("contract.getAddress()")?;
        assert_eq!(value, expected_address);
        Ok(())
    })?;

    Ok(())
}
