use acton_config::color::OwoColorize;
use anyhow::Context;
use std::path::{Path, PathBuf};

pub async fn localnet_snapshot_create_cmd(
    name: &str,
    force: bool,
    port: u16,
    auth_token: Option<String>,
) -> anyhow::Result<()> {
    let name = normalize_snapshot_name(name)?;

    super::post_localnet_control(
        port,
        auth_token,
        "acton_snapshot",
        serde_json::json!({
            "name": &name,
            "force": force,
        }),
        "Create snapshot",
    )
    .await?;

    println!(
        "{} localnet snapshot {}",
        "Created".green().bold(),
        name.cyan(),
    );
    Ok(())
}

pub async fn localnet_snapshot_list_cmd(
    port: u16,
    auth_token: Option<String>,
) -> anyhow::Result<()> {
    let result = super::post_localnet_control(
        port,
        auth_token,
        "acton_listSnapshots",
        serde_json::json!({}),
        "List snapshots",
    )
    .await?;
    let snapshots = result
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|snapshot| {
            snapshot
                .get("name")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        })
        .collect::<Vec<_>>();
    if snapshots.is_empty() {
        println!("No localnet snapshots found");
        return Ok(());
    }

    println!("{}", "Localnet snapshots:".white().bold());
    for snapshot in snapshots {
        println!("  {snapshot}");
    }
    Ok(())
}

pub async fn localnet_snapshot_revert_cmd(
    name: &str,
    port: u16,
    auth_token: Option<String>,
) -> anyhow::Result<()> {
    let name = normalize_snapshot_name(name)?;

    super::post_localnet_control(
        port,
        auth_token,
        "acton_revert",
        serde_json::json!({ "name": &name }),
        "Revert snapshot",
    )
    .await?;

    println!(
        "{} localnet to snapshot {}",
        "Reverted".green().bold(),
        name.cyan(),
    );
    Ok(())
}

pub async fn localnet_snapshot_export_cmd(
    name: &str,
    out: PathBuf,
    force: bool,
    port: u16,
    auth_token: Option<String>,
) -> anyhow::Result<()> {
    let name = normalize_snapshot_name(name)?;
    let out = resolve_project_path(out);
    if out.exists() && !force {
        anyhow::bail!(
            "Output file {} already exists; pass {} to overwrite it",
            out.display().to_string().cyan(),
            "--force".yellow(),
        );
    }
    super::post_localnet_control(
        port,
        auth_token,
        "acton_exportSnapshot",
        serde_json::json!({
            "name": &name,
            "path": out.display().to_string(),
        }),
        "Export snapshot",
    )
    .await?;

    println!(
        "{} localnet snapshot {} to {}",
        "Exported".green().bold(),
        name.cyan(),
        display_project_path(&out).dimmed(),
    );
    Ok(())
}

pub async fn localnet_snapshot_import_cmd(
    path: PathBuf,
    name: Option<String>,
    force: bool,
    port: u16,
    auth_token: Option<String>,
) -> anyhow::Result<()> {
    let path = resolve_project_path(path);
    if !path.is_file() {
        anyhow::bail!(
            "Snapshot file {} does not exist",
            path.display().to_string().cyan()
        );
    }
    let name = match name {
        Some(name) => normalize_snapshot_name(&name)?,
        None => snapshot_name_from_path(&path)?,
    };
    super::post_localnet_control(
        port,
        auth_token,
        "acton_importSnapshot",
        serde_json::json!({
            "name": &name,
            "path": path.display().to_string(),
            "force": force,
        }),
        "Import snapshot",
    )
    .await?;

    println!(
        "{} localnet snapshot {} from {}",
        "Imported".green().bold(),
        name.cyan(),
        display_project_path(&path).dimmed(),
    );
    Ok(())
}

fn resolve_project_path(path: PathBuf) -> PathBuf {
    if path.is_absolute() {
        path
    } else {
        acton_config::config::project_root().join(path)
    }
}

fn normalize_snapshot_name(name: &str) -> anyhow::Result<String> {
    let name = name.trim();
    if name.is_empty() {
        anyhow::bail!("Localnet snapshot name cannot be empty");
    }
    if name == "." || name == ".." {
        anyhow::bail!("Localnet snapshot name cannot be {}", name.cyan());
    }
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
        anyhow::bail!(
            "Invalid localnet snapshot name {}; use only letters, numbers, '.', '_' and '-'",
            name.cyan(),
        );
    }
    Ok(name.to_owned())
}

fn snapshot_name_from_path(path: &Path) -> anyhow::Result<String> {
    let stem = path
        .file_stem()
        .and_then(|name| name.to_str())
        .with_context(|| {
            format!(
                "Cannot infer localnet snapshot name from file {}; pass {}",
                path.display().to_string().cyan(),
                "--name".yellow(),
            )
        })?;
    normalize_snapshot_name(stem)
}

fn display_project_path(path: &Path) -> String {
    path.strip_prefix(acton_config::config::project_root())
        .unwrap_or(path)
        .display()
        .to_string()
}
