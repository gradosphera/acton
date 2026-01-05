use crate::support::project::ProjectBuilder;
use crate::support::assertions::TestOutputExt;
use std::fs;

#[test]
fn test_build_with_custom_config_path() {
    let project = ProjectBuilder::new("custom_config_build")
        .contract("main", "fun onInternalMessage(in: InMessage) {}")
        .build();

    // Create custom config
    let custom_toml = r#"[package]
name = "custom-project"
description = "Custom description"
version = "0.2.0"

[contracts.custom_contract]
name = "CustomContract"
src = "contracts/main.tolk"
depends = []
"#;
    fs::write(project.path().join("Custom.toml"), custom_toml).unwrap();

    // Build with default config (Acton.toml)
    project.acton()
        .build()
        .clear_cache()
        .run()
        .success()
        .assert_contains("Compiling main");

    // Build with custom config (Custom.toml)
    project.acton()
        .config_path("Custom.toml")
        .build()
        .clear_cache()
        .run()
        .success()
        .assert_contains("Compiling CustomContract");
}

#[test]
fn test_run_with_custom_config_path() {
    let script_code = r#"
        import "../../lib/io"
        fun main() {
            println("Hello from default");
        }
    "#;
    let custom_script_code = r#"
        import "../../lib/io"
        fun main() {
            println("Hello from custom");
        }
    "#;

    let project = ProjectBuilder::new("custom_config_run")
        .script_file("hello", script_code)
        .script_config("say-hello", "acton script scripts/hello.tolk")
        .build();

    // Create custom config with different script
    let custom_toml = r#"[package]
name = "custom-project"
version = "0.1.0"
description = "A test project"

[scripts]
say-hello = "acton script scripts/custom_hello.tolk"
"#;
    fs::write(project.path().join("Custom.toml"), custom_toml).unwrap();
    fs::write(project.path().join("scripts/custom_hello.tolk"), custom_script_code).unwrap();

    // Run with default config
    project.acton()
        .run_script_cmd("say-hello")
        .run()
        .success()
        .assert_contains("Hello from default");

    // Run with custom config
    project.acton()
        .config_path("Custom.toml")
        .run_script_cmd("say-hello")
        .run()
        .success()
        .assert_contains("Hello from custom");
}

#[test]
fn test_error_when_custom_config_missing() {
    let project = ProjectBuilder::new("missing_custom_config")
        .build();

    project.acton()
        .config_path("NonExistent.toml")
        .build()
        .run()
        .failure()
        .assert_contains("NonExistent.toml not found");
}

#[test]
fn test_wallet_with_custom_config_path() {
    let project = ProjectBuilder::new("custom_config_wallet")
        .build();

    // Create a custom config in a subdirectory
    fs::create_dir_all(project.path().join("subdir")).unwrap();
    let custom_toml = r#"[package]
name = "custom-project"
version = "0.1.0"
description = "A test project"
"#;
    let custom_config_path = project.path().join("subdir/Custom.toml");
    fs::write(&custom_config_path, custom_toml).unwrap();

    // Create a wallet using custom config path
    // This should create wallets.toml in subdir/
    project.acton()
        .config_path("subdir/Custom.toml")
        .arg("wallet")
        .arg("new")
        .arg("--name")
        .arg("custom-wallet")
        .arg("--version")
        .arg("v5r1")
        .arg("--local")
        .arg("--secure")
        .arg("false")
        .run()
        .success();

    let wallets_toml_path = project.path().join("subdir").join("wallets.toml");
    println!("Checking for wallets.toml at: {:?}", wallets_toml_path);
    assert!(wallets_toml_path.exists(), "wallets.toml should be created in subdir/, but not found at {:?}", wallets_toml_path);
    assert!(!project.path().join("wallets.toml").exists(), "wallets.toml should NOT be created in root");

    // List wallets using custom config path
    project.acton()
        .config_path("subdir/Custom.toml")
        .arg("wallet")
        .arg("list")
        .run()
        .success()
        .assert_contains("custom-wallet");
}
