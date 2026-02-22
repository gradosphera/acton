use acton_config::color::OwoColorize;
use acton_config::config::{ActonConfig, project_root, resolve_path_from_project_root};
use anyhow::{Context, Result};
use globset::{Glob, GlobSetBuilder};
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use walkdir::WalkDir;

pub fn fmt_cmd(paths: Vec<String>, check: bool) -> Result<()> {
    let config = ActonConfig::load().ok();
    let fmt_settings = config.as_ref().and_then(|c| c.fmt.as_ref());

    let width = fmt_settings.and_then(|s| s.width).unwrap_or(100);
    let ignore_patterns = fmt_settings.and_then(|s| s.ignore.as_ref());

    let mut ignore_builder = GlobSetBuilder::new();
    ignore_builder.add(Glob::from_str("**/.git/**")?);
    ignore_builder.add(Glob::from_str("**/node_modules/**")?);
    ignore_builder.add(Glob::from_str("**/target/**")?);
    if let Some(ignores) = ignore_patterns {
        for pattern in ignores {
            ignore_builder.add(Glob::new(pattern)?);
        }
    }
    let ignore_set = ignore_builder.build()?;

    let mut files_to_format = Vec::new();

    let search_paths = if paths.is_empty() {
        vec![resolve_path_from_project_root(".")]
    } else {
        paths
            .into_iter()
            .map(resolve_path_from_project_root)
            .collect()
    };

    for search_root in search_paths {
        if search_root.is_file() {
            if search_root.extension().is_some_and(|ext| ext == "tolk") {
                files_to_format.push(search_root);
            }
        } else if search_root.is_dir() {
            let iter = WalkDir::new(&search_root)
                .into_iter()
                .filter_entry(|entry| {
                    if !entry.file_type().is_dir() {
                        return true;
                    }
                    let p = entry.path();
                    let rel = p.strip_prefix(&search_root).unwrap_or(p);
                    !ignore_set.is_match(rel)
                })
                .filter_map(std::result::Result::ok);

            for entry in iter {
                let path = entry.path();
                if !path.extension().is_some_and(|ext| ext == "tolk") || !path.is_file() {
                    continue;
                }

                let rel = path.strip_prefix(&search_root).unwrap_or(path);
                if !ignore_set.is_match(rel) {
                    files_to_format.push(path.to_path_buf());
                }
            }
        } else {
            anyhow::bail!("Path {} does not exist", search_root.display());
        }
    }

    if files_to_format.is_empty() {
        println!("{}", "No Tolk files found to format".yellow());
        return Ok(());
    }

    let mut unformatted_files = Vec::new();
    let mut formatted_count = 0;
    let mut error_count = 0;

    for file_path in files_to_format {
        let display_path = relative_display_path(&file_path);
        let content = fs::read_to_string(&file_path)
            .with_context(|| format!("Failed to read {}", display_path.display()))?;

        match tolkfmt::format_source(&content, width) {
            Ok(formatted) => {
                if content != formatted {
                    if check {
                        unformatted_files.push(file_path.clone());

                        let diff = TextDiff::from_lines(&content, &formatted);
                        println!("Diff in {}:", display_path.display().to_string().bold());

                        for hunk in diff.unified_diff().context_radius(3).iter_hunks() {
                            for change in hunk.iter_changes() {
                                let (sign, value) = match change.tag() {
                                    ChangeTag::Delete => {
                                        ("-".red().to_string(), change.value().red().to_string())
                                    }
                                    ChangeTag::Insert => (
                                        "+".green().to_string(),
                                        change.value().green().to_string(),
                                    ),
                                    ChangeTag::Equal => (
                                        " ".dimmed().to_string(),
                                        change.value().dimmed().to_string(),
                                    ),
                                };
                                print!("{sign}{value}");
                            }
                        }
                        println!();
                    } else {
                        fs::write(&file_path, formatted)
                            .with_context(|| format!("Failed to write {}", display_path.display()))?;
                        formatted_count += 1;
                        println!(
                            "{} {}",
                            "Formatted".green(),
                            display_path.display()
                        );
                    }
                }
            }
            Err(err) => {
                eprintln!("{} {}: {}", "Error".red(), display_path.display(), err);
                error_count += 1;
            }
        }
    }

    if check {
        if !unformatted_files.is_empty() {
            anyhow::bail!("Files are not formatted");
        } else if error_count > 0 {
            anyhow::bail!("Formatting check failed due to syntax errors in {error_count} files");
        }
        println!("{}", "All files are properly formatted".green());
    } else {
        if formatted_count > 0 {
            println!("\n{} {} files formatted", "Done:".green(), formatted_count);
        } else if error_count == 0 {
            println!("{}", "All files are already formatted".green());
        }

        if error_count > 0 {
            anyhow::bail!("Failed to format {error_count} files due to syntax errors");
        }
    }

    Ok(())
}

fn relative_display_path(path: &Path) -> PathBuf {
    pathdiff::diff_paths(path, project_root()).unwrap_or_else(|| path.to_path_buf())
}
