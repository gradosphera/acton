use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::env;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use tolk_resolver::file_db::{FileDb, FileInfo};
use tolk_resolver::project_index::ProjectIndex;
use tolk_resolver::{AstNodeSpanExt, Resolved, resolve};
use tolk_syntax::ast::NodeTraversalExt;
use tolk_syntax::{DotAccess, DotAccessField, TopLevel};
use tolk_ty::{InferenceResult, TyData, TyId, TypeDb, TypeInterner, infer};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum UnknownState {
    No,
    Partial,
    Exact,
}

#[derive(Debug)]
struct TypeRow {
    file: PathBuf,
    line: usize,
    col: usize,
    snippet: String,
    ty: String,
    unknown: UnknownState,
    kind: &'static str,
}

#[derive(Debug)]
struct ProjectModel {
    project_root: PathBuf,
    config_path: Option<PathBuf>,
    mappings: BTreeMap<String, String>,
}

#[derive(Default, Debug, Clone)]
struct RootStats {
    files: usize,
    declarations: usize,
    undefined_exact: usize,
    undefined_partial: usize,
    unresolved_dot_fields: usize,
    index_errors: usize,
}

#[derive(Debug)]
struct RootReport {
    root: PathBuf,
    rows: Vec<TypeRow>,
    dot_rows: Vec<DotAccessRow>,
    stats: RootStats,
}

#[derive(Debug)]
struct DotAccessRow {
    file: PathBuf,
    line: usize,
    col: usize,
    field: String,
    snippet: String,
}

fn main() -> Result<(), Box<dyn Error>> {
    let mut target_dir = None;
    let mut summary_only = false;

    for arg in env::args().skip(1) {
        if arg == "--summary-only" {
            summary_only = true;
            continue;
        }
        if target_dir.is_some() {
            eprintln!("Usage: cargo run -p tolk-ty --example type_dump -- [DIR] [--summary-only]");
            return Ok(());
        }
        target_dir = Some(PathBuf::from(arg));
    }

    let target_dir = target_dir.unwrap_or_else(|| PathBuf::from("acton-contracts"));
    let target_dir = dunce::canonicalize(&target_dir)?;
    if !target_dir.is_dir() {
        return Err(format!("Path is not a directory: {}", target_dir.display()).into());
    }

    let workspace_root =
        dunce::canonicalize(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."))?;
    let project_model = build_project_model(&target_dir, &workspace_root)?;
    let analysis_roots = collect_analysis_roots(&target_dir, &project_model)?;
    if analysis_roots.is_empty() {
        return Err(format!("No analysis roots found under {}", target_dir.display()).into());
    }

    let stdlib_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../tolkc/assets/tolk-stdlib");
    let stdlib_path = dunce::canonicalize(stdlib_path)?;

    let acton_lib = project_model
        .mappings
        .get("acton")
        .map(PathBuf::from)
        .filter(|p| p.exists());
    let file_db = FileDb::new(stdlib_path.clone(), acton_lib);

    let common_tolk = stdlib_path.join("common.tolk");
    if common_tolk.exists() {
        let _ = file_db.process(&common_tolk);
    }

    let mappings_opt = if project_model.mappings.is_empty() {
        None
    } else {
        Some(project_model.mappings.clone())
    };

    let mut root_reports = Vec::new();
    for root_file in analysis_roots {
        let mut index = ProjectIndex::builder(&file_db, root_file.clone())
            .with_stdlib(file_db.stdlib_path().to_owned())
            .with_mappings(&mappings_opt)
            .build()?;
        resolve(&file_db, &mut index);

        let mut interner = TypeInterner::new();
        let mut type_db = TypeDb::new(&mut interner, &file_db, &index);
        let mut rows = Vec::new();
        let mut dot_rows = Vec::new();
        let mut stats = RootStats {
            index_errors: index.errors().len(),
            ..RootStats::default()
        };

        let Some(root_file_id) = index.get_file_by_path(&root_file) else {
            continue;
        };
        let reachable = index.reachable_files(root_file_id);

        for file_id in reachable {
            let Some(file_info) = file_db.get_by_id(file_id) else {
                continue;
            };
            let include_in_report = is_target_tolk_file(&target_dir, file_info.path());
            if include_in_report {
                stats.files += 1;
            }
            analyze_file(
                &file_info,
                &mut type_db,
                &mut rows,
                &mut dot_rows,
                &mut stats,
                include_in_report,
            );
        }

        rows.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.col.cmp(&b.col))
                .then(a.kind.cmp(b.kind))
        });
        rows.dedup_by(|a, b| {
            a.file == b.file
                && a.line == b.line
                && a.col == b.col
                && a.kind == b.kind
                && a.snippet == b.snippet
                && a.ty == b.ty
                && a.unknown == b.unknown
        });
        dot_rows.sort_by(|a, b| {
            a.file
                .cmp(&b.file)
                .then(a.line.cmp(&b.line))
                .then(a.col.cmp(&b.col))
                .then(a.field.cmp(&b.field))
                .then(a.snippet.cmp(&b.snippet))
        });
        dot_rows.dedup_by(|a, b| {
            a.file == b.file
                && a.line == b.line
                && a.col == b.col
                && a.field == b.field
                && a.snippet == b.snippet
        });

        root_reports.push(RootReport {
            root: root_file,
            rows,
            dot_rows,
            stats,
        });
    }

    root_reports.sort_by(|a, b| a.root.cmp(&b.root));
    print_root_reports(&target_dir, &project_model, &root_reports, summary_only);
    Ok(())
}

fn analyze_file(
    file_info: &FileInfo,
    type_db: &mut TypeDb<'_>,
    rows: &mut Vec<TypeRow>,
    dot_rows: &mut Vec<DotAccessRow>,
    stats: &mut RootStats,
    include_in_report: bool,
) {
    let source_file = file_info.source();
    let source_text: &str = source_file.source.as_ref();

    for decl in source_file.top_levels() {
        let Some(symbol) = file_info.find_declaration(&decl) else {
            continue;
        };
        let result = infer(type_db, file_info.id(), symbol.id, &decl);
        if !include_in_report {
            continue;
        }
        stats.declarations += 1;

        collect_unresolved_dot_access_fields(
            file_info,
            type_db,
            source_text,
            decl,
            &result,
            dot_rows,
            stats,
        );

        let mut expr_types = result
            .expression_types
            .iter()
            .map(|(span, ty)| (*span, *ty))
            .collect::<Vec<_>>();
        expr_types.sort_by_key(|(span, _)| span.start);

        for (span, ty_id) in expr_types {
            let (line, col) = offset_to_line_col(file_info, span.start());
            let ty = type_db.intrn.display(ty_id).to_string();
            let unknown = unknown_state(type_db.intrn, ty_id);
            if unknown == UnknownState::No {
                continue;
            }
            let row = TypeRow {
                file: file_info.path().clone(),
                line,
                col,
                snippet: snippet_for_span(source_text, span),
                ty,
                unknown,
                kind: "expr",
            };
            if is_internal_declaration_placeholder_row(&row) {
                continue;
            }

            match row.unknown {
                UnknownState::Exact => stats.undefined_exact += 1,
                UnknownState::Partial => stats.undefined_partial += 1,
                UnknownState::No => {}
            }
            rows.push(row);
        }
    }
}

fn collect_unresolved_dot_access_fields(
    file_info: &FileInfo,
    type_db: &TypeDb<'_>,
    source_text: &str,
    decl: TopLevel<'_>,
    inference: &InferenceResult,
    dot_rows: &mut Vec<DotAccessRow>,
    stats: &mut RootStats,
) {
    for node in decl.syntax().traverse() {
        if node.kind() != "dot_access" {
            continue;
        }
        let dot = DotAccess(node);
        let Some(DotAccessField::Ident(field_ident)) = dot.field() else {
            continue;
        };

        let field_span = field_ident.span();
        let inferred_resolved = has_resolved_usage_for_span(inference, field_span);
        if inferred_resolved {
            continue;
        }

        let index_resolved = type_db
            .project_index
            .find_use(file_info.id(), field_span.start())
            .is_some_and(|usage| !matches!(usage.resolved, Resolved::Unresolved));
        if index_resolved {
            continue;
        }

        let (line, col) = offset_to_line_col(file_info, field_span.start());
        dot_rows.push(DotAccessRow {
            file: file_info.path().clone(),
            line,
            col,
            field: snippet_for_span(source_text, field_span),
            snippet: snippet_for_span(source_text, dot.span()),
        });
        stats.unresolved_dot_fields += 1;
    }
}

fn has_resolved_usage_for_span(inference: &InferenceResult, span: tolk_resolver::Span) -> bool {
    if inference
        .resolve(span)
        .is_some_and(|usage| !matches!(usage.resolved, Resolved::Unresolved))
    {
        return true;
    }

    inference.resolved_refs.iter().any(|usage| {
        usage.span.start <= span.start
            && usage.span.end >= span.end
            && !matches!(usage.resolved, Resolved::Unresolved)
    })
}

fn is_target_tolk_file(target_dir: &Path, file_path: &Path) -> bool {
    file_path.starts_with(target_dir)
        && file_path.extension().and_then(|ext| ext.to_str()) == Some("tolk")
}

fn print_root_reports(
    target_dir: &Path,
    project_model: &ProjectModel,
    root_reports: &[RootReport],
    summary_only: bool,
) {
    let totals = root_reports
        .iter()
        .fold(RootStats::default(), |mut acc, report| {
            acc.files += report.stats.files;
            acc.declarations += report.stats.declarations;
            acc.undefined_exact += report.stats.undefined_exact;
            acc.undefined_partial += report.stats.undefined_partial;
            acc.unresolved_dot_fields += report.stats.unresolved_dot_fields;
            acc.index_errors += report.stats.index_errors;
            acc
        });

    println!("Target directory: {}", target_dir.display());
    println!("Project root: {}", project_model.project_root.display());
    match &project_model.config_path {
        Some(path) => println!("Config file: {}", path.display()),
        None => println!("Config file: none"),
    }
    println!(
        "Mappings loaded: {}",
        if project_model.mappings.is_empty() {
            "none".to_string()
        } else {
            project_model
                .mappings
                .iter()
                .map(|(key, value)| {
                    let normalized = normalize_mapping_key(key);
                    let state = if Path::new(value).exists() {
                        "ok"
                    } else {
                        "missing"
                    };
                    format!("{normalized}={value} [{state}]")
                })
                .collect::<Vec<_>>()
                .join(", ")
        }
    );
    println!(
        "Roots: {}, files: {}, declarations: {}",
        root_reports.len(),
        totals.files,
        totals.declarations
    );
    println!(
        "Undefined types: exact={}, partial={}",
        totals.undefined_exact, totals.undefined_partial
    );
    println!(
        "Unresolved dot-access fields: {}",
        totals.unresolved_dot_fields
    );
    println!("Index errors (sum): {}", totals.index_errors);

    let mut merged_rows = root_reports
        .iter()
        .flat_map(|report| report.rows.iter())
        .filter(|row| row.unknown != UnknownState::No)
        .collect::<Vec<_>>();
    merged_rows.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.col.cmp(&b.col))
            .then(a.kind.cmp(b.kind))
            .then(a.snippet.cmp(&b.snippet))
            .then(a.ty.cmp(&b.ty))
    });
    merged_rows.dedup_by(|a, b| {
        a.file == b.file
            && a.line == b.line
            && a.col == b.col
            && a.kind == b.kind
            && a.snippet == b.snippet
            && a.ty == b.ty
            && a.unknown == b.unknown
    });

    let unique_exact = merged_rows
        .iter()
        .filter(|row| row.unknown == UnknownState::Exact)
        .count();
    let unique_partial = merged_rows
        .iter()
        .filter(|row| row.unknown == UnknownState::Partial)
        .count();

    let mut merged_dot_rows = root_reports
        .iter()
        .flat_map(|report| report.dot_rows.iter())
        .collect::<Vec<_>>();
    merged_dot_rows.sort_by(|a, b| {
        a.file
            .cmp(&b.file)
            .then(a.line.cmp(&b.line))
            .then(a.col.cmp(&b.col))
            .then(a.field.cmp(&b.field))
            .then(a.snippet.cmp(&b.snippet))
    });
    merged_dot_rows.dedup_by(|a, b| {
        a.file == b.file
            && a.line == b.line
            && a.col == b.col
            && a.field == b.field
            && a.snippet == b.snippet
    });

    println!(
        "Undefined types (unique): exact={}, partial={}, total={}",
        unique_exact,
        unique_partial,
        merged_rows.len()
    );
    println!(
        "Unresolved dot-access fields (unique): {}",
        merged_dot_rows.len()
    );

    println!("\n== Undefined Types (Merged) ==");
    if merged_rows.is_empty() {
        println!("none");
    } else if !summary_only {
        for row in merged_rows {
            let tag = match row.unknown {
                UnknownState::Exact => "exact-undefined",
                UnknownState::Partial => "partial-undefined",
                UnknownState::No => continue,
            };
            println!(
                "{}:{}:{} [{}] {} => {} ({})",
                display_relative(target_dir, &row.file),
                row.line,
                row.col,
                row.kind,
                row.snippet,
                row.ty,
                tag
            );
        }
    }

    println!("\n== Unresolved DotAccess Fields (Merged) ==");
    if merged_dot_rows.is_empty() {
        println!("none");
    } else if !summary_only {
        for row in merged_dot_rows {
            println!(
                "{}:{}:{} [dot-field] {} -> {}",
                display_relative(target_dir, &row.file),
                row.line,
                row.col,
                row.snippet,
                row.field
            );
        }
    }
}

fn unknown_state(interner: &TypeInterner, ty_id: TyId) -> UnknownState {
    if matches!(interner.data(ty_id), TyData::Undefined) {
        return UnknownState::Exact;
    }
    let mut visited = HashSet::new();
    if type_contains_unknown(interner, ty_id, &mut visited) {
        UnknownState::Partial
    } else {
        UnknownState::No
    }
}

fn is_internal_declaration_placeholder_row(row: &TypeRow) -> bool {
    if row.kind != "expr" {
        return false;
    }
    if row.unknown == UnknownState::No {
        return false;
    }
    let snippet = row.snippet.trim_start();
    (snippet.starts_with("val ") || snippet.starts_with("var ")) && !snippet.contains('=')
}

fn type_contains_unknown(
    interner: &TypeInterner,
    ty_id: TyId,
    visited: &mut HashSet<TyId>,
) -> bool {
    if !visited.insert(ty_id) {
        return false;
    }

    match interner.data(ty_id) {
        TyData::Undefined => true,
        TyData::Struct { args, .. } => args.as_ref().is_some_and(|args| {
            args.iter()
                .any(|ty| type_contains_unknown(interner, *ty, visited))
        }),
        TyData::TypeAlias { inner_ty, args, .. } => {
            type_contains_unknown(interner, *inner_ty, visited)
                || args.as_ref().is_some_and(|args| {
                    args.iter()
                        .any(|ty| type_contains_unknown(interner, *ty, visited))
                })
        }
        TyData::Tensor(items) | TyData::Tuple(items) | TyData::Union(items) => items
            .iter()
            .any(|ty| type_contains_unknown(interner, *ty, visited)),
        TyData::Array(item_ty) => type_contains_unknown(interner, *item_ty, visited),
        TyData::Func { params, return_ty } => {
            params
                .iter()
                .any(|ty| type_contains_unknown(interner, *ty, visited))
                || type_contains_unknown(interner, *return_ty, visited)
        }
        TyData::TypeParameter { default_type, .. } => {
            default_type.is_some_and(|ty| type_contains_unknown(interner, ty, visited))
        }
        TyData::GenericTypeWithTs { inner_ty, types } => {
            type_contains_unknown(interner, *inner_ty, visited)
                || types
                    .iter()
                    .any(|ty| type_contains_unknown(interner, *ty, visited))
        }
        TyData::MapKV { key, value } => {
            type_contains_unknown(interner, *key, visited)
                || type_contains_unknown(interner, *value, visited)
        }
        _ => false,
    }
}

fn collect_analysis_roots(
    target_dir: &Path,
    project_model: &ProjectModel,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut roots = BTreeSet::new();

    if let Some(config_path) = &project_model.config_path {
        for contract_root in load_contract_root_files(config_path, &project_model.project_root)? {
            if contract_root.starts_with(target_dir) {
                roots.insert(contract_root);
            }
        }
    }

    let mut all_tolk_files = collect_tolk_files(target_dir)?;
    all_tolk_files.sort_unstable();

    for file in &all_tolk_files {
        let name = file
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        if name.ends_with(".test.tolk") {
            roots.insert(file.clone());
        }
    }

    if roots.is_empty() {
        roots.extend(all_tolk_files);
    }

    Ok(roots.into_iter().collect())
}

fn load_contract_root_files(
    config_path: &Path,
    project_root: &Path,
) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let content = fs::read_to_string(config_path)?;
    let mut in_contract_section = false;
    let mut roots = Vec::new();

    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }

        if line.starts_with('[') && line.ends_with(']') {
            in_contract_section = line.starts_with("[contracts.");
            continue;
        }
        if !in_contract_section {
            continue;
        }

        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        if left.trim() != "src" {
            continue;
        }

        let raw_value = right.trim();
        let value = parse_toml_string(raw_value).unwrap_or_else(|| raw_value.to_string());
        let path = if Path::new(&value).is_absolute() {
            PathBuf::from(value)
        } else {
            project_root.join(value)
        };
        if path.exists() {
            roots.push(dunce::canonicalize(path)?);
        }
    }

    Ok(roots)
}

fn collect_tolk_files(root: &Path) -> Result<Vec<PathBuf>, Box<dyn Error>> {
    let mut out = Vec::new();
    collect_tolk_files_rec(root, &mut out, true)?;
    Ok(out)
}

fn collect_tolk_files_rec(
    dir: &Path,
    out: &mut Vec<PathBuf>,
    is_root: bool,
) -> Result<(), Box<dyn Error>> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let metadata = entry.metadata()?;
        if metadata.is_dir() {
            if !is_root
                && let Some(name) = path.file_name().and_then(|name| name.to_str())
                && (name.starts_with('.') || name == "target")
            {
                continue;
            }
            collect_tolk_files_rec(&path, out, false)?;
            continue;
        }
        if path.extension().and_then(|ext| ext.to_str()) == Some("tolk") {
            out.push(dunce::canonicalize(path)?);
        }
    }
    Ok(())
}

fn build_project_model(
    target_dir: &Path,
    workspace_root: &Path,
) -> Result<ProjectModel, Box<dyn Error>> {
    let (project_root, config_path) = match find_project_root_with_config(target_dir) {
        Some((root, cfg)) => (root, Some(cfg)),
        None => (target_dir.to_owned(), None),
    };

    let mut mappings = match &config_path {
        Some(cfg_path) => load_mappings(cfg_path, &project_root)?,
        None => BTreeMap::new(),
    };

    let default_acton_lib = workspace_root.join("lib");
    if !mappings.contains_key("acton") && default_acton_lib.exists() {
        mappings.insert(
            "acton".to_string(),
            default_acton_lib.to_string_lossy().into_owned(),
        );
    }

    Ok(ProjectModel {
        project_root,
        config_path,
        mappings,
    })
}

fn find_project_root_with_config(start_dir: &Path) -> Option<(PathBuf, PathBuf)> {
    for dir in start_dir.ancestors() {
        for config_name in ["Acton.toml", "acton.toml"] {
            let config_path = dir.join(config_name);
            if config_path.exists() {
                let project_root = dunce::canonicalize(dir).unwrap_or_else(|_| dir.to_owned());
                let config_path =
                    dunce::canonicalize(config_path).unwrap_or_else(|_| dir.join(config_name));
                return Some((project_root, config_path));
            }
        }
    }

    None
}

fn load_mappings(
    config_path: &Path,
    project_root: &Path,
) -> Result<BTreeMap<String, String>, Box<dyn Error>> {
    let content = fs::read_to_string(config_path)?;
    let mut mappings = BTreeMap::new();
    let mut in_import_mappings = false;

    for raw_line in content.lines() {
        let line = raw_line.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        if line.starts_with('[') && line.ends_with(']') {
            in_import_mappings = line == "[import-mappings]";
            continue;
        }
        if !in_import_mappings {
            continue;
        }

        let Some((left, right)) = line.split_once('=') else {
            continue;
        };
        let key = left.trim().trim_matches('"').to_string();
        let raw_value = right.trim();
        let value = parse_toml_string(raw_value).unwrap_or_else(|| raw_value.to_string());
        let mapped = if Path::new(&value).is_absolute() {
            PathBuf::from(value)
        } else {
            project_root.join(value)
        };
        let mapped = dunce::canonicalize(&mapped).unwrap_or(mapped);
        mappings.insert(key, mapped.to_string_lossy().into_owned());
    }

    Ok(mappings)
}

fn normalize_mapping_key(key: &str) -> String {
    if key.starts_with('@') {
        key.to_string()
    } else {
        format!("@{key}")
    }
}

fn parse_toml_string(s: &str) -> Option<String> {
    let s = s.trim();
    if s.len() >= 2
        && ((s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')))
    {
        return Some(s[1..s.len() - 1].to_string());
    }
    None
}

fn offset_to_line_col(file_info: &FileInfo, offset: usize) -> (usize, usize) {
    let line_offsets = file_info.line_offsets();
    let line = line_offsets
        .binary_search(&offset)
        .unwrap_or_else(|idx| idx.saturating_sub(1));
    let line_start = line_offsets.get(line).copied().unwrap_or(0);
    let source: &str = file_info.source().source.as_ref();
    let col = source[line_start..offset].chars().count();
    (line + 1, col + 1)
}

fn snippet_for_span(source: &str, span: tolk_resolver::Span) -> String {
    let text = source
        .get(span.start()..span.end())
        .unwrap_or("<invalid-span>");
    let mut normalized = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if normalized.is_empty() {
        normalized = "<empty>".to_string();
    }
    if normalized.len() > 120 {
        normalized.truncate(117);
        normalized.push_str("...");
    }
    normalized
}

fn display_relative(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).ok();
    let Some(rel) = rel else {
        return path.to_string_lossy().into_owned();
    };

    let root_name = root
        .file_name()
        .and_then(|n| n.to_str())
        .map_or_else(|| root.to_string_lossy().into_owned(), ToString::to_string);

    if rel.as_os_str().is_empty() {
        root_name
    } else {
        format!("{}/{}", root_name, rel.to_string_lossy())
    }
}
