use anyhow::Result;
use clap::Subcommand;

use crate::toolchain::{ToolchainEnvironment, load_project_toolchain_config, resolve_toolchain};

#[derive(Subcommand, Clone)]
pub enum ToolchainCommand {
    #[command(about = "Resolve the Acton toolchain selected for the current project")]
    Resolve,
}

pub fn toolchain_cmd(command: ToolchainCommand) -> Result<()> {
    match command {
        ToolchainCommand::Resolve => resolve_cmd(),
    }
}

fn resolve_cmd() -> Result<()> {
    let config = load_project_toolchain_config()?;
    let environment = ToolchainEnvironment::runtime()?;
    let report = resolve_toolchain(config.as_ref(), None, &environment)?;

    println!("{}", serde_json::to_string_pretty(&report)?);

    Ok(())
}
