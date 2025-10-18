use clap::{Parser, Subcommand};
use emulator_rs::commands::test::test_cmd;

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
            let result = test_cmd(&file);
            if let Err(err) = result {
                eprintln!("{}", err);
            }
        }
    }
}
