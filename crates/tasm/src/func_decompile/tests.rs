use super::{FuncDecompiler, FuncDecompilerOptions};
use std::path::PathBuf;
use tycho_types::boc::Boc;

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn decompile_fixture(name: &str) -> String {
    let boc = std::fs::read(fixture_path(name)).expect("Failed to read fixture");
    FuncDecompiler::new()
        .decompile_boc(&boc)
        .expect("Failed to decompile fixture")
}

#[test]
fn decompile_binary_boc_fixture() {
    let out = decompile_fixture("01_arith.boc");
    assert!(out.contains("#pragma version >=0.4.0;"));
    assert!(out.contains("recv_internal") || out.contains("decompiled_entry"));
}

#[test]
fn decompile_hex_boc_string() {
    let boc = std::fs::read(fixture_path("02_if_else.boc")).expect("read boc fixture");
    let hex = Boc::encode_hex(Boc::decode(&boc).expect("decode fixture"));
    let out = FuncDecompiler::new()
        .decompile_boc_hex(&hex)
        .expect("hex decompilation must succeed");
    assert!(out.contains("method_id"));
}

#[test]
fn decompile_base64_boc_string() {
    let boc = std::fs::read(fixture_path("03_while.boc")).expect("read boc fixture");
    let b64 = Boc::encode_base64(Boc::decode(&boc).expect("decode fixture"));
    let out = FuncDecompiler::new()
        .decompile_boc_string(&b64)
        .expect("base64 decompilation must succeed");
    assert!(out.contains("do {"));
    assert!(out.contains("until ("));
}

#[test]
fn decompile_if_else_to_structured_blocks() {
    let out = decompile_fixture("02_if_else.boc");
    assert!(out.contains("if ("));
    assert!(out.contains("} else {"));
    assert!(!out.contains("IFELSE missing then continuation"));
}

#[test]
fn decompile_repeat_to_structured_block() {
    let out = decompile_fixture("04_repeat.boc");
    assert!(out.contains("repeat ("));
    assert!(!out.contains("REPEAT missing continuation"));
}

#[test]
fn can_disable_raw_fallback() {
    let options = FuncDecompilerOptions {
        include_raw_tasm_fallback: false,
        max_raw_tasm_lines_per_method: 32,
    };
    let boc = std::fs::read(fixture_path("01_arith.boc")).expect("read fixture");
    let out = FuncDecompiler::with_options(options)
        .decompile_boc(&boc)
        .expect("decompile should succeed");
    assert!(!out.contains("low-level fallback (TASM)"));
}
