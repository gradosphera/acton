use crate::config::{ActonConfig, ContractConfig};
use crate::file_build_cache::FileBuildCache;
use anyhow::anyhow;
use owo_colors::OwoColorize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Instant;
use tycho_types::boc::Boc;

mod dep_graph;

pub fn build_cmd(
    contract_filter: Option<String>,
    clear_cache: bool,
    graph_output: Option<String>,
) -> anyhow::Result<()> {
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

    if let Some(filter) = &contract_filter {
        if contracts.iter().find(|(key, _)| key == &filter).is_none() {
            return Err(anyhow!("Contract '{}' not found in Acton.toml", filter));
        }
    }

    let flatten_contracts = contracts.iter().collect::<Vec<_>>();
    let compilation_order = dep_graph::build_dependency_graph(&flatten_contracts)?;

    let filtered_compilation_order = if let Some(filter) = &contract_filter {
        dep_graph::filter_compilation_order_for_contract(filter, &compilation_order, contracts)?
    } else {
        compilation_order
    };

    if let Some(graph_path) = &graph_output {
        let output_path = if graph_path.is_empty() {
            "deps.svg"
        } else {
            graph_path
        };
        dep_graph::generate_dependency_graph_svg(
            &filtered_compilation_order,
            contracts,
            output_path,
        )?;
    }

    let mut compiled_contracts: HashMap<String, String> = HashMap::new();

    for contract_key in filtered_compilation_order {
        let contract_config = contracts.get(&contract_key).unwrap();
        let contract_path = &contract_config.root;

        generate_dependency_files(&contract_key, &contract_config, &compiled_contracts)?;

        let cached_result = file_cache.get(contract_path, false, 2, "1.2".to_string());

        let code_boc64 = if let Some(cached_result) = cached_result {
            Some(cached_result.code_boc64)
        } else {
            let compile_start = Instant::now();
            println!("   {} {}", "Compiling".green().bold(), contract_config.name);

            let compilation_result = tolkc::compile(Path::new(contract_path), false);
            let compile_time = compile_start.elapsed();

            match compilation_result {
                tolkc::CompilerResult::Success(result) => {
                    if let Err(e) =
                        file_cache.put(contract_path, &result, false, 2, "1.2".to_string())
                    {
                        eprintln!(
                            "Warning: Failed to cache compilation result for {}: {}",
                            contract_config.name, e
                        );
                    }

                    println!("    {} in {:?}", "Finished".green(), compile_time);

                    Some(result.code_boc64)
                }
                tolkc::CompilerResult::Error(error) => {
                    eprintln!("{}", error.message);
                    failure_count += 1;
                    None
                }
            }
        };

        let Some(code_boc64) = &code_boc64 else {
            continue;
        };

        compiled_contracts.insert(contract_key.clone(), code_boc64.clone());

        if let Err(e) = save_boc_file(contract_config, &code_boc64) {
            eprintln!(
                "Warning: Failed to save cached BoC file for {}: {}",
                contract_config.name, e
            );
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

pub(crate) fn generate_dependency_files(
    key: &String,
    config: &ContractConfig,
    compiled_contracts: &HashMap<String, String>, // contract_key -> boc_base64
) -> anyhow::Result<()> {
    let gen_dir = Path::new("gen");
    if !gen_dir.exists() {
        fs::create_dir_all(gen_dir)?;
    }

    let Some(depends) = &config.depends else {
        return Ok(());
    };
    if depends.is_empty() {
        return Ok(());
    }

    for dep in depends {
        let gen_file_path = gen_dir.join(format!("{}_code.tolk", dep));
        let mut content = String::new();
        let boc_base64 = compiled_contracts.get(dep).ok_or_else(|| {
            anyhow!(
                "[INTERNAL ERROR] Dependency '{}' must be compiled before '{}'",
                dep,
                key
            )
        })?;

        let func_name = format!("{}CompiledCode", dep.replace("-", "_"));

        content.push_str(&format!(
            "fun {}(): cell asm \"\"\"\n    \"{}\" base64>B B>boc PUSHREF\n\"\"\"\n",
            func_name, boc_base64
        ));
        fs::write(&gen_file_path, content)?;
    }

    Ok(())
}
