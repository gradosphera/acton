use clap::{Parser, Subcommand};
use emulator_rs::commands::compile::compile_cmd;
use emulator_rs::commands::script::script_cmd;
use emulator_rs::commands::test::test_cmd;
use owo_colors::OwoColorize;

#[derive(Parser)]
#[command(name = "acton")]
#[command(about = "TON blockchain development tool")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    #[command(about = "Execute tests in file or directory")]
    Test {
        #[arg(help = "Test file or directory containing test files")]
        path: String,
        #[arg(short, long, help = "Filter tests by regex pattern")]
        filter: Option<String>,
    },
    #[command(about = "Execute a Tolk script file")]
    Script {
        #[arg(help = "Script file to execute")]
        path: String,
    },
    #[command(about = "Compile a Tolk file")]
    Compile {
        #[arg(help = "Tolk file to compile")]
        path: String,
        #[arg(long, help = "Output result as JSON")]
        json: bool,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test { path, filter } => {
            let result = test_cmd(&path, filter.as_deref());
            if let Err(err) = result {
                eprintln!("{} {}", "Error:".red(), err);
            }
        }
        Commands::Script { path } => {
            let result = script_cmd(&path);
            if let Err(err) = result {
                eprintln!("{} {}", "Error:".red(), err);
            }
        }
        Commands::Compile { path, json } => {
            let result = compile_cmd(&path, json);
            if let Err(err) = result {
                if json {
                    println!(
                        "{}",
                        serde_json::to_string_pretty(&serde_json::json!({
                            "success": false,
                            "error": err.to_string()
                        }))
                        .unwrap()
                    );
                } else {
                    eprintln!("{} {}", "Error:".red(), err);
                }
            }
        }
    }
}
