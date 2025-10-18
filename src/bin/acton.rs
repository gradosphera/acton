use clap::{Parser, Subcommand};
use emulator_rs::compiler::{Compiler, TolkCompilerResult};
use emulator_rs::exts::register_get_extensions;
use emulator_rs::get_executor::{
    GetExecutor, GetMethodArgs, GetMethodInternalParams, GetMethodResult,
};
use emulator_rs::{exit_codes, tolk_parser};
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::ops::Add;
use std::path::Path;
use std::process;
use std::sync::Arc;
use std::time::Instant;
use tonlib_core::TonAddress;
use tonlib_core::cell::{ArcCell, Cell, CellBuilder};
use tonlib_core::tlb_types::tlb::TLB;

const CRC16: crc::Crc<u16> = crc::Crc::<u16>::new(&crc::CRC_16_XMODEM);

#[derive(Parser)]
#[command(name = "acton")]
#[command(about = "TON blockchain development tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Test { file: String },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test { file } => {
            if !file.ends_with("_test.tolk") {
                eprintln!("File must end with __test.tolk");
                process::exit(1);
            }

            let content = match fs::read_to_string(&file) {
                Ok(content) => content,
                Err(err) => {
                    eprintln!("Error reading file '{}': {}", file, err);
                    process::exit(1);
                }
            };

            let tests = find_all_test(file.clone(), &content);

            let executable_code = content + "\n\nfun main() {}"; // append dummy main
            let tmp_test_filename = "test_".to_string().add(&*file);

            fs::write(&tmp_test_filename, executable_code).unwrap();

            let compiler = Compiler::new();
            let compilation_result = compiler.compile(Path::new(&tmp_test_filename));
            match compilation_result {
                Ok(TolkCompilerResult::Success(result)) => {
                    let code_cell = ArcCell::from_boc_b64(&*result.code_boc64).unwrap();
                    let data_cell = ArcCell::default();
                    run_all_tests(&file, tests, &code_cell, &data_cell);
                }
                Ok(TolkCompilerResult::Error(error)) => {
                    eprintln!("Cannot compile test file {}", error.message);
                    process::exit(1);
                }
                Err(error) => {
                    eprintln!("Cannot compile test file {}", error);
                    process::exit(1);
                }
            }
        }
    }
}

fn run_all_tests(
    file_path: &str,
    tests: Vec<TestDescriptor>,
    code_cell: &Arc<Cell>,
    data_cell: &Arc<Cell>,
) {
    let cwd = std::env::current_dir().unwrap_or_else(|_| Path::new(".").to_path_buf());

    println!(
        "\n{} {}\n",
        " TEST ".bold().on_cyan(),
        cwd.display().dimmed()
    );
    println!("{}", "─".repeat(50).dimmed());

    let relative_path = Path::new(file_path)
        .strip_prefix(cwd)
        .unwrap_or_else(|_| Path::new(file_path));
    println!(
        " {} {} {}",
        ">".dimmed(),
        relative_path.display().to_string(),
        format!("({} tests)", tests.len()).dimmed()
    );

    let total_start_time = Instant::now();
    let dest_address = contract_address(&code_cell);

    let mut passed = 0;
    let mut failed = 0;

    for (_i, test) in tests.iter().enumerate() {
        print!("  {} {} ", "○".dimmed(), test.name.dimmed());
        std::io::stdout().flush().unwrap();

        let start_time = Instant::now();
        let result = execute_test(test, &code_cell, &data_cell, &dest_address);
        let duration = start_time.elapsed();

        let exit_code = match &result {
            GetMethodResult::Success(result) => result.vm_exit_code,
            GetMethodResult::Error(_) => 999,
        };

        // Clear the current line before printing result
        print!("\r\x1b[K");

        let duration_ms = duration.as_millis();
        let (time_value, time_unit) = if duration_ms > 0 {
            (duration_ms.to_string(), "ms")
        } else {
            (duration.as_micros().to_string(), "μs")
        };

        if exit_code == 0 {
            println!(
                "  {} {} {}{}",
                "✓".green(),
                test.name,
                time_value.green(),
                time_unit.green().dimmed()
            );
            passed += 1;
        } else {
            println!(
                "  {} {} {}{}",
                "✗".red(),
                test.name,
                time_value.red(),
                time_unit.red().dimmed()
            );
            failed += 1;

            match &result {
                GetMethodResult::Success(result) => {
                    let exit_code = result.vm_exit_code as i64;
                    println!(
                        "    {} exit_code={}",
                        "└─".dimmed(),
                        exit_code.to_string().yellow()
                    );

                    if let Some(info) = exit_codes::get_exit_code_info(exit_code) {
                        println!("      {} {}", "├─".dimmed(), info.description.dimmed());
                        println!("      {} Phase: {}", "└─".dimmed(), info.phase.dimmed());
                    }
                }
                GetMethodResult::Error(error) => {
                    println!("    {} {}", "└─".dimmed(), error.error.yellow());
                }
            }
        }
    }

    let total_duration = total_start_time.elapsed();
    let total_duration_ms = total_duration.as_millis();

    println!("{}", "─".repeat(50).dimmed());

    if failed == 0 {
        println!(
            " {} {} {} {}{}",
            "✓".green().bold(),
            passed.to_string().green().bold(),
            "passed".green().bold(),
            total_duration_ms.to_string().green(),
            "ms".green().dimmed()
        );
    } else {
        println!(
            " {} {} {}, {} {} {} {} {}{}",
            "✓".green().bold(),
            passed.to_string().green().bold(),
            "passed".green().bold(),
            "✗".red().bold(),
            failed.to_string().red().bold(),
            "failed".red().bold(),
            "in".white().dimmed(),
            total_duration_ms.to_string().red(),
            "ms".red().dimmed()
        );
    }

    if failed > 0 {
        println!(
            "\n{}",
            "Some tests failed. Check the output above for details.".red()
        );
    }
}

fn execute_test(
    test: &TestDescriptor,
    code_cell: &Arc<Cell>,
    data_cell: &Arc<Cell>,
    dest_address: &TonAddress,
) -> GetMethodResult {
    // thread::sleep(Duration::from_secs(2));

    let params = GetMethodInternalParams {
        code: code_cell.to_boc_b64(false).unwrap().to_string(),
        data: data_cell.to_boc_b64(false).unwrap().to_string(),
        verbosity: 5,
        libs: "".to_string(),
        address: dest_address.to_string(),
        unixtime: 0,
        balance: "10".to_string(),
        rand_seed: "0000000000000000000000000000000000000000000000000000000000000000".to_string(),
        gas_limit: "0".to_string(),
        method_id: test.id,
        debug_enabled: true,
        extra_currencies: HashMap::new(),
        prev_blocks_info: None,
    };
    let mut get_executor = GetExecutor::new(params.clone());
    register_get_extensions(&mut get_executor);

    let result = get_executor.run_get_method(GetMethodArgs {
        stack: Default::default(),
        params,
    });
    result
}

fn contract_address(code: &Arc<Cell>) -> TonAddress {
    let state_init = CellBuilder::new()
        .store_bit(false)
        .unwrap()
        .store_bit(false)
        .unwrap()
        .store_ref_cell_optional(Some(&code))
        .unwrap()
        .store_ref_cell_optional(Some(&ArcCell::default()))
        .unwrap()
        .store_bit(false)
        .unwrap()
        .build()
        .unwrap();

    let dest_address = TonAddress::new(0, state_init.cell_hash());
    dest_address
}

#[derive(Debug)]
struct TestDescriptor {
    pub file: String,
    pub id: i32,
    pub name: String,
    pub annotations: Vec<String>,
}

fn find_all_test(file: String, content: &String) -> Vec<TestDescriptor> {
    let tree = tolk_parser::parse(&content);
    let root_node = tree.root_node();
    let mut cursor = root_node.walk();

    root_node
        .children(&mut cursor)
        .flat_map(|child| {
            if child.kind() == "get_method_declaration" {
                let name_node = child.child_by_field_name("name");
                let raw_name = name_node
                    .unwrap()
                    .utf8_text(content.as_bytes())
                    .unwrap()
                    .to_string();
                let name = raw_name
                    .strip_prefix("`")
                    .unwrap_or(&raw_name)
                    .strip_suffix("`")
                    .unwrap_or(&raw_name);

                if name.starts_with("test") {
                    let id = (CRC16.checksum(name.as_bytes()) & 0xff_ff) as i32 | 0x1_00_00;

                    return vec![TestDescriptor {
                        file: file.clone(),
                        id,
                        name: name.to_string(),
                        annotations: vec![],
                    }];
                }
            };

            vec![]
        })
        .collect()
}
