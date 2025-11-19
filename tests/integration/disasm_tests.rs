use crate::common::assertion;
use crate::support::snapshots::normalize_output;
use crate::support::{ProjectBuilder, TestOutputExt};
use std::fs;

const SIMPLE_CONTRACT: &str = r#"
fun onInternalMessage(in: InMessage) {}
fun onBouncedMessage(_: InMessageBounced) {}
"#;

#[test]
fn test_disasm_from_boc_file() {
    let project = ProjectBuilder::new("disasm-file")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    project
        .acton()
        .disasm_file("simple.boc")
        .run()
        .success()
        .assert_snapshot_matches("integration/snapshots/test_disasm_from_boc_file.stdout.txt");
}

#[test]
fn test_disasm_from_boc_file_with_output() {
    let project = ProjectBuilder::new("disasm-output")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    project
        .acton()
        .disasm_file("simple.boc")
        .with_output("output.tasm")
        .run()
        .success()
        .assert_contains("Disassembled code written to output.tasm");

    let output_file = project.path().join("output.tasm");
    assert!(output_file.exists(), "Output file should exist");
}

#[test]
fn test_disasm_from_hex_string() {
    let project = ProjectBuilder::new("disasm-hex")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    let boc_bytes = fs::read(project.path().join("simple.boc")).unwrap();
    let hex_string = hex::encode(boc_bytes);

    project
        .acton()
        .disasm_string(&hex_string)
        .run()
        .success()
        .assert_snapshot_matches("integration/snapshots/test_disasm_from_hex_string.stdout.txt");
}

#[test]
fn test_disasm_from_base64_string() {
    let project = ProjectBuilder::new("disasm-base64")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    let boc_bytes = fs::read(project.path().join("simple.boc")).unwrap();
    let base64_string =
        tycho_types::boc::Boc::encode_base64(tycho_types::boc::Boc::decode(boc_bytes).unwrap());

    project
        .acton()
        .disasm_string(&base64_string)
        .run()
        .success()
        .assert_snapshot_matches("integration/snapshots/test_disasm_from_base64_string.stdout.txt");
}

#[test]
fn test_disasm_file_not_found() {
    let project = ProjectBuilder::new("disasm-not-found").build();

    project
        .acton()
        .disasm_file("nonexistent.boc")
        .run()
        .failure()
        .assert_stderr_contains("No such file");
}

#[test]
fn test_disasm_invalid_boc_data() {
    let project = ProjectBuilder::new("disasm-invalid").build();

    fs::create_dir_all(project.path().join("data")).unwrap();
    fs::write(project.path().join("data/invalid.boc"), "invalid boc data").unwrap();

    project
        .acton()
        .disasm_file("data/invalid.boc")
        .run()
        .failure()
        .assert_stderr_contains("Failed to decode BOC");
}

#[test]
fn test_disasm_invalid_hex_string() {
    let project = ProjectBuilder::new("disasm-invalid-hex").build();

    project
        .acton()
        .disasm_string("not_valid_hex_or_base64")
        .run()
        .failure()
        .assert_stderr_contains("Failed to decode BOC");
}

#[test]
fn test_disasm_no_input_provided() {
    let project = ProjectBuilder::new("disasm-no-input").build();

    project
        .acton()
        .disasm()
        .run()
        .failure()
        .assert_stderr_contains("Either --string/-s or boc_file must be provided");
}

#[test]
fn test_disasm_built_contract() {
    let complex_contract = r#"
    fun onInternalMessage(in: InMessage) {}
    fun onBouncedMessage(_: InMessageBounced) {}
    "#;

    let project = ProjectBuilder::new("disasm-complex")
        .contract_with_output("complex", complex_contract, "complex.boc")
        .build();

    project.acton().build().run().success();

    project
        .acton()
        .disasm_file("complex.boc")
        .run()
        .success()
        .assert_snapshot_matches(
            "integration/snapshots/test_disasm_with_complex_contract.stdout.txt",
        );
}

#[test]
fn test_disasm_snapshot() {
    let project = ProjectBuilder::new("disasm-snapshot")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    project
        .acton()
        .disasm_file("simple.boc")
        .run()
        .success()
        .assert_snapshot_matches("integration/snapshots/test_disasm_snapshot.stdout.txt");
}

#[test]
fn test_disasm_output_file_created() {
    let project = ProjectBuilder::new("disasm-create")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    let output_file = project.path().join("result.tasm");
    assert!(!output_file.exists());

    project
        .acton()
        .disasm_file("simple.boc")
        .with_output("result.tasm")
        .run()
        .success();

    assert!(output_file.exists());

    let content = fs::read_to_string(&output_file).unwrap();
    assertion().eq(
        normalize_output(&content, project.path()),
        snapbox::file!("snapshots/test_disasm_output_file_created.tasm.gen"),
    );
}

#[test]
fn test_disasm_overwrite_existing_file() {
    let project = ProjectBuilder::new("disasm-overwrite")
        .contract_with_output("simple", SIMPLE_CONTRACT, "simple.boc")
        .build();

    project.acton().build().run().success();

    let output_file = project.path().join("output.tasm");
    fs::write(&output_file, "old content").unwrap();

    project
        .acton()
        .disasm_file("simple.boc")
        .with_output("output.tasm")
        .run()
        .success();

    let content = fs::read_to_string(&output_file).unwrap();
    assert_ne!(content, "old content");
    assertion().eq(
        normalize_output(&content, project.path()),
        snapbox::file!("snapshots/test_disasm_overwrite_existing_file.tasm.gen"),
    );
}
