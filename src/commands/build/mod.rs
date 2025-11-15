use crate::config::{ActonConfig, ContractConfig};
use crate::file_build_cache::FileBuildCache;
use anyhow::anyhow;
use owo_colors::OwoColorize;
use std::fs;
use std::time::Instant;
use tycho_types::boc::Boc;

pub fn build_cmd(contract_filter: Option<String>, clear_cache: bool) -> anyhow::Result<()> {
    if clear_cache {
        let mut file_cache = FileBuildCache::new(None)?;
        file_cache.clear()?;
        println!("  {} Cache cleared", "✓".green().bold());
    }

    let config = ActonConfig::load()?;

    let contracts = match config.contracts() {
        Some(contracts) => contracts,
        None => {
            println!(
                "No contracts found in Acton.toml. Run 'acton init' first or add contracts manually."
            );
            return Ok(());
        }
    };

    if contracts.is_empty() {
        println!("No contracts to build.");
        return Ok(());
    }

    let mut file_cache = FileBuildCache::new(None)?;
    let mut failure_count = 0;
    let total_start = Instant::now();

    let filtered_contracts: Vec<_> = if let Some(filter) = &contract_filter {
        contracts.iter().filter(|(key, _)| key == &filter).collect()
    } else {
        contracts.iter().collect()
    };

    if filtered_contracts.is_empty() {
        if let Some(filter) = contract_filter {
            return Err(anyhow!("Contract '{}' not found in Acton.toml", filter));
        }
        return Ok(());
    }

    let mut sorted_contracts = filtered_contracts;
    sorted_contracts.sort_by(|a, b| a.1.name.cmp(&b.1.name));

    for (_, contract_config) in sorted_contracts {
        let contract_path = &contract_config.root;

        let compile_start = Instant::now();

        if let Some(cached_result) = file_cache.get(contract_path, false, 2, "1.2".to_string()) {
            if contract_config
                .output
                .as_ref()
                .map(|path| !std::path::Path::new(path).exists())
                .unwrap_or(false)
            {
                if let Err(e) = save_boc_file(contract_config, &cached_result.code_boc64) {
                    eprintln!(
                        "Warning: Failed to save cached BoC file for {}: {}",
                        contract_config.name, e
                    );
                }
            }
            continue;
        }

        println!("   {} {}", "Compiling".green().bold(), contract_config.name);

        let compilation_result = tolkc::compile(std::path::Path::new(contract_path), false);
        let compile_time = compile_start.elapsed();

        match compilation_result {
            tolkc::CompilerResult::Success(result) => {
                if let Err(e) = file_cache.put(contract_path, &result, false, 2, "1.2".to_string())
                {
                    eprintln!(
                        "Warning: Failed to cache compilation result for {}: {}",
                        contract_config.name, e
                    );
                }

                if let Err(e) = save_boc_file(contract_config, &result.code_boc64) {
                    eprintln!(
                        "Warning: Failed to save BoC file for {}: {}",
                        contract_config.name, e
                    );
                }

                println!("    {} in {:?}", "Finished".green(), compile_time);
            }
            tolkc::CompilerResult::Error(error) => {
                eprintln!("{}", error.message);
                failure_count += 1;
            }
        }
    }

    let total_elapsed = total_start.elapsed();

    if failure_count == 0 {
        println!("    {} in {:?}", "Finished".green().bold(), total_elapsed,);
        Ok(())
    } else {
        Err(anyhow!(
            "Build failed with {} error{}",
            failure_count,
            if failure_count == 1 { "" } else { "s" }
        ))
    }
}

fn save_boc_file(contract_config: &ContractConfig, code_boc64: &str) -> anyhow::Result<()> {
    if let Some(output_path) = &contract_config.output {
        let code = Boc::decode_base64(code_boc64)?;
        fs::write(output_path, Boc::encode(code))?;
    }
    Ok(())
}
