use clap::{Parser, Subcommand};
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
    #[command(about = "Execute tests")]
    Test {
        file: String,
        #[arg(short, long, help = "Filter tests by regex pattern")]
        filter: Option<String>,
    },
}

fn main() {
    let cli = Cli::parse();

    match cli.command {
        Commands::Test { file, filter } => {
            let result = test_cmd(&file, filter.as_deref());
            if let Err(err) = result {
                eprintln!("{} {}", "Error:".red(), err);
            }
        }
    }
}
