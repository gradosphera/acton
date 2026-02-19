use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use tolk_syntax::parse;
use tree_sitter::Node;

use super::GITHUB_SOURCE_BASE;

pub(super) fn generate_stdlib_docs(lib_dir: &Path, out_dir: &Path) -> Result<()> {
    if !lib_dir.exists() {
        anyhow::bail!("Directory 'lib' not found");
    }

    fs::create_dir_all(out_dir)?;

    let index_article = out_dir.join("index.mdx");
    fs::write(
        &index_article,
        r#"---
title: "Standard library"
description: "Learn about available functions/struct/constants available in Acton standard library"
icon: FileCode
---

Acton provides a collection of functions for writing scripts and tests in Tolk.
"#,
    )?;

    let mut files: Vec<_> = walkdir::WalkDir::new(lib_dir)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| {
            e.file_type().is_file() && e.path().extension().is_some_and(|ext| ext == "tolk")
        })
        .collect();

    files.sort_by_key(|e| e.path().to_string_lossy().to_string());

    struct FileDoc {
        path: PathBuf,
        file_stem: String,
        symbols: Vec<SymbolInfo>,
        file_header: Option<String>,
    }

    let mut docs = Vec::new();
    let mut symbol_map: HashMap<String, Vec<LinkTarget>> = HashMap::new();

    for entry in files {
        let path = entry.path();
        let path_string = path.to_string_lossy();
        if path_string.contains("tests") || path_string.ends_with(".test.tolk") {
            continue;
        }

        let content = fs::read_to_string(path)?;
        let relative_path = path.strip_prefix(lib_dir)?;
        let file_stem = relative_path
            .file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .to_string();

        let tree = parse(&content)?;
        let root_node = tree.root_node();

        let symbols = extract_symbols(root_node, &content);
        let symbols: Vec<_> = symbols.into_iter().filter(|s| !skip_symbol(s)).collect();
        let file_header = extract_file_header_doc(&content);

        if !symbols.is_empty() || file_header.is_some() {
            let target_rel_path = relative_path.with_extension("");
            for symbol in &symbols {
                let link_target = LinkTarget {
                    path: target_rel_path.clone(),
                    anchor: symbol.name.clone(),
                };
                insert_link_target(&mut symbol_map, symbol.name.clone(), link_target.clone());
                for alias in &symbol.link_aliases {
                    insert_link_target(&mut symbol_map, alias.clone(), link_target.clone());
                }
            }

            docs.push(FileDoc {
                path: path.to_path_buf(),
                file_stem,
                symbols,
                file_header,
            });
        }
    }

    let link_regex = Regex::new(r"\[([a-zA-Z0-9_.]+)]")?;

    for doc in docs {
        let relative_path = doc.path.strip_prefix(lib_dir)?;
        let current_file_stem_path = relative_path.with_extension("");

        let mut output_path = out_dir.to_path_buf();
        if let Some(parent) = relative_path.parent() {
            output_path.push(parent);
            fs::create_dir_all(&output_path)?;
        }
        output_path.push(format!("{}.mdx", doc.file_stem));

        let mut mdx_content = String::new();
        mdx_content.push_str("---\n");
        mdx_content.push_str(&format!("title: \"{}\"\n", doc.file_stem));
        mdx_content.push_str(&format!(
            "description: \"{}.tolk standard library file\"\n",
            doc.file_stem
        ));
        mdx_content.push_str("---\n\n");
        mdx_content.push_str("import { SourceCodeLink } from '@/components/SourceCodeLink';\n\n");

        if let Some(header) = &doc.file_header {
            mdx_content.push_str(header);
            mdx_content.push_str("\n\n");
        }

        if !doc.symbols.is_empty() {
            mdx_content.push_str("## Definitions\n\n");
        }

        for symbol in doc.symbols {
            mdx_content.push_str(&format!("## `{}`\n\n", symbol.name));

            let source_url = format!(
                "{GITHUB_SOURCE_BASE}/{}#L{}",
                doc.path.to_string_lossy(),
                symbol.start_line + 1
            );

            mdx_content.push_str("```tolk\n");
            mdx_content.push_str(&symbol.signature);
            mdx_content.push_str("\n```\n\n");

            if let Some(doc_text) = symbol.doc.as_ref() {
                if Some(doc_text) == doc.file_header.as_ref() {
                    // skip if the symbol doc is exactly the same as the file header
                } else {
                    let processed_doc =
                        link_regex.replace_all(doc_text, |caps: &regex::Captures<'_>| {
                            let name = &caps[1];
                            if let Some(link_target) =
                                resolve_link_target(&symbol_map, name, &current_file_stem_path)
                            {
                                let target_path = &link_target.path;
                                if target_path == &current_file_stem_path {
                                    format!(
                                        "[{}](#{})",
                                        name,
                                        normalize_symbol_link(&link_target.anchor)
                                    )
                                } else {
                                    let relative_link_path =
                                        pathdiff::diff_paths(target_path, &current_file_stem_path)
                                            .unwrap_or_else(|| target_path.clone());

                                    let link = relative_link_path.to_string_lossy().to_string();
                                    format!(
                                        "[{}]({}/#{})",
                                        name,
                                        link,
                                        normalize_symbol_link(&link_target.anchor)
                                    )
                                }
                            } else {
                                eprintln!("Warning: Symbol '{name}' not found in documentation");
                                name.to_string()
                            }
                        });
                    mdx_content.push_str(&processed_doc);
                    mdx_content.push_str("\n\n");
                }
            }

            mdx_content.push_str(&format!("<SourceCodeLink href=\"{source_url}\" />\n\n"));
        }
        fs::write(output_path, mdx_content)?;
    }

    Ok(())
}

fn normalize_symbol_link(link: &str) -> String {
    link.replace('\\', "/")
        .replace('.', "")
        .to_ascii_lowercase()
}

fn skip_symbol(s: &SymbolInfo) -> bool {
    s.name == "ffi"
        || s.name == "impl"
        || s.name == "expect_impl"
        || s.name == "impl_msg"
        || s.name.starts_with("ffi.")
        || s.name.starts_with("impl.")
        || s.name.starts_with("expect_impl.")
        || s.name.starts_with("impl_msg.")
        || s.name.starts_with("never.")
        || s.name.contains("__")
}

enum SymbolKind {
    Function,
    Struct,
    Enum,
    TypeAlias,
    Constant,
}

struct SymbolInfo {
    #[allow(dead_code)]
    kind: SymbolKind,
    name: String,
    signature: String,
    doc: Option<String>,
    start_line: usize,
    link_aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LinkTarget {
    path: PathBuf,
    anchor: String,
}

fn extract_symbols(root: Node<'_>, source: &str) -> Vec<SymbolInfo> {
    let mut symbols = Vec::new();
    let mut cursor = root.walk();

    for child in root.children(&mut cursor) {
        let kind = child.kind();
        if kind == "function_declaration"
            || kind == "method_declaration"
            || kind == "get_method_declaration"
        {
            if let Some(func) = parse_function(child, source) {
                symbols.push(func);
            }
        } else if kind == "type_alias_declaration"
            && let Some(type_alias) = parse_type_alias(child, source)
        {
            symbols.push(type_alias);
        } else if kind == "struct_declaration"
            && let Some(s) = parse_struct(child, source)
        {
            symbols.push(s);
        } else if kind == "enum_declaration"
            && let Some(e) = parse_enum(child, source)
        {
            symbols.push(e);
        } else if kind == "constant_declaration"
            && let Some(c) = parse_constant(child, source)
        {
            symbols.push(c);
        }
    }

    symbols
}

fn parse_type_alias(node: Node<'_>, source: &str) -> Option<SymbolInfo> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();
    let signature = node.utf8_text(source.as_bytes()).ok()?;

    let doc = extract_doc_comment(node, source);
    let start_line = node.start_position().row;

    Some(SymbolInfo {
        kind: SymbolKind::TypeAlias,
        name,
        signature: signature.to_owned(),
        doc,
        start_line,
        link_aliases: Vec::new(),
    })
}

fn parse_struct(node: Node<'_>, source: &str) -> Option<SymbolInfo> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();

    let signature = node.utf8_text(source.as_bytes()).ok()?;
    let link_aliases = parse_struct_field_aliases(node, source, &name);

    let doc = extract_doc_comment(node, source);
    let start_line = node.start_position().row;

    Some(SymbolInfo {
        kind: SymbolKind::Struct,
        name,
        signature: signature.to_owned(),
        doc,
        start_line,
        link_aliases,
    })
}

fn parse_enum(node: Node<'_>, source: &str) -> Option<SymbolInfo> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();
    let signature = node.utf8_text(source.as_bytes()).ok()?;
    let link_aliases = parse_enum_member_aliases(node, source, &name);

    let doc = extract_doc_comment(node, source);
    let start_line = node.start_position().row;

    Some(SymbolInfo {
        kind: SymbolKind::Enum,
        name,
        signature: signature.to_owned(),
        doc,
        start_line,
        link_aliases,
    })
}

fn parse_constant(node: Node<'_>, source: &str) -> Option<SymbolInfo> {
    let name_node = node.child_by_field_name("name")?;
    let name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();

    let full_text = node.utf8_text(source.as_bytes()).ok()?;
    let doc = extract_doc_comment(node, source);
    let start_line = node.start_position().row;

    Some(SymbolInfo {
        kind: SymbolKind::Constant,
        name,
        signature: full_text.to_string(),
        doc,
        start_line,
        link_aliases: Vec::new(),
    })
}

fn parse_function(node: Node<'_>, source: &str) -> Option<SymbolInfo> {
    let kind = node.kind();

    let name_node = node.child_by_field_name("name")?;
    let mut name = name_node.utf8_text(source.as_bytes()).ok()?.to_string();

    if kind == "method_declaration"
        && let Some(receiver_node) = node.child_by_field_name("receiver")
        && let Some(type_node) = receiver_node.child_by_field_name("receiver_type")
    {
        let type_name = type_node.utf8_text(source.as_bytes()).ok()?;
        name = format!("{type_name}.{name}");
    }

    let full_text = node.utf8_text(source.as_bytes()).ok()?;
    let link_aliases = parse_parameter_aliases(node, source, &name);

    let signature = if let Some(body) = node.child_by_field_name("body") {
        let cut_idx = body.start_byte() - node.start_byte();
        full_text[..cut_idx].trim().to_string()
    } else {
        full_text.to_string()
    };

    let doc = extract_doc_comment(node, source);
    let start_line = node.start_position().row;

    Some(SymbolInfo {
        kind: SymbolKind::Function,
        link_aliases,
        name,
        signature,
        doc,
        start_line,
    })
}

fn parse_parameter_aliases(node: Node<'_>, source: &str, owner_name: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    let Some(parameters) = node.child_by_field_name("parameters") else {
        return aliases;
    };

    let mut cursor = parameters.walk();
    for child in parameters.children(&mut cursor) {
        if child.kind() != "parameter_declaration" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(param_name) = name_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        let param_name = param_name.trim_matches('`');
        if param_name.is_empty() || param_name == "self" {
            continue;
        }
        aliases.push(param_name.to_string());
        aliases.push(format!("{owner_name}.{param_name}"));
    }

    aliases
}

fn parse_struct_field_aliases(node: Node<'_>, source: &str, owner_name: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    let Some(body) = node.child_by_field_name("body") else {
        return aliases;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "struct_field_declaration" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(field_name) = name_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        let field_name = field_name.trim_matches('`');
        if field_name.is_empty() {
            continue;
        }
        aliases.push(format!("{owner_name}.{field_name}"));
    }

    aliases
}

fn parse_enum_member_aliases(node: Node<'_>, source: &str, owner_name: &str) -> Vec<String> {
    let mut aliases = Vec::new();

    let Some(body) = node.child_by_field_name("body") else {
        return aliases;
    };

    let mut cursor = body.walk();
    for child in body.children(&mut cursor) {
        if child.kind() != "enum_member_declaration" {
            continue;
        }
        let Some(name_node) = child.child_by_field_name("name") else {
            continue;
        };
        let Ok(member_name) = name_node.utf8_text(source.as_bytes()) else {
            continue;
        };
        let member_name = member_name.trim_matches('`');
        if member_name.is_empty() {
            continue;
        }
        aliases.push(format!("{owner_name}.{member_name}"));
    }

    aliases
}

fn insert_link_target(
    symbol_map: &mut HashMap<String, Vec<LinkTarget>>,
    name: String,
    target: LinkTarget,
) {
    let entry = symbol_map.entry(name).or_default();
    if !entry.iter().any(|existing| existing == &target) {
        entry.push(target);
    }
}

fn resolve_link_target<'a>(
    symbol_map: &'a HashMap<String, Vec<LinkTarget>>,
    name: &str,
    current_file_stem_path: &Path,
) -> Option<&'a LinkTarget> {
    let targets = symbol_map.get(name)?;
    if targets.len() == 1 {
        return targets.first();
    }

    targets
        .iter()
        .find(|target| target.path == current_file_stem_path)
}

fn extract_doc_comment(node: Node<'_>, source: &str) -> Option<String> {
    let start_byte = node.start_byte();
    let prefix = &source[..start_byte];

    let mut lines = Vec::new();

    for line in prefix.lines().rev() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            if !lines.is_empty() {
                break;
            }
        } else if trimmed.starts_with("///") {
            let content = trimmed.trim_start_matches("///");
            let content = if let Some(stripped) = content.strip_prefix(' ') {
                stripped.to_string()
            } else {
                content.to_string()
            };

            lines.push(content);
        } else {
            break;
        }
    }

    if lines.is_empty() {
        None
    } else {
        lines.reverse();
        Some(lines.join("\n"))
    }
}

fn extract_file_header_doc(source: &str) -> Option<String> {
    let mut lines = Vec::new();
    let mut has_content = false;

    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("///") {
            let content = trimmed.trim_start_matches("///");
            let content = if let Some(stripped) = content.strip_prefix(' ') {
                stripped
            } else {
                content
            };
            lines.push(content);
            has_content = true;
        } else if trimmed.is_empty() {
            break;
        } else {
            // Found a non-comment, non-empty line. Stop.
            break;
        }
    }

    // Trim trailing empty lines from the buffer
    while let Some(last) = lines.last() {
        if last.is_empty() {
            lines.pop();
        } else {
            break;
        }
    }

    if has_content && !lines.is_empty() {
        Some(lines.join("\n"))
    } else {
        None
    }
}
