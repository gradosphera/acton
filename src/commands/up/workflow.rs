use acton_config::color::OwoColorize;
use anyhow::{Context, Result, bail};
use flate2::read::GzDecoder;
use semver::Version;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};
use tar::Archive;
use tempfile::TempDir;

use super::client::{Asset, Release, ReleaseClient};

pub(crate) const MAX_RELEASE_ARCHIVE_BYTES: u64 = 256 * 1024 * 1024;
pub(crate) const MAX_RELEASE_CHECKSUM_BYTES: u64 = 16 * 1024;
pub(crate) const MAX_EXTRACTED_ACTON_BYTES: u64 = 256 * 1024 * 1024;
const MAX_RELEASE_ARCHIVE_ENTRIES: usize = 128;

#[derive(serde::Serialize)]
pub(super) struct UpdateInfo {
    pub success: bool,
    pub current_version: String,
    pub latest_version: String,
    pub update_available: bool,
}

pub(crate) struct VerifiedReleaseArchive {
    pub(crate) path: PathBuf,
}

pub(crate) struct ExtractedActonBinary {
    pub(crate) path: PathBuf,
    _temp_dir: TempDir,
}

pub(super) fn check_update<C: ReleaseClient>(
    client: &C,
    current_version_str: &str,
    current_is_trunk: bool,
) -> Result<UpdateInfo> {
    let release = client.get_release(None, false)?;
    let latest_version = release.tag_name;

    let update_available = if current_is_trunk {
        // don't report anything for trunk release user
        false
    } else {
        let current_v = Version::parse(current_version_str).with_context(|| {
            format!("Current Acton version '{current_version_str}' is not valid semver")
        })?;
        let latest_v_str = latest_version.trim_start_matches('v');
        if let Ok(latest_v) = Version::parse(latest_v_str) {
            latest_v > current_v
        } else {
            false
        }
    };

    Ok(UpdateInfo {
        success: true,
        current_version: current_version_str.to_string(),
        latest_version,
        update_available,
    })
}

#[allow(clippy::too_many_arguments)]
pub(super) fn run_update<C: ReleaseClient>(
    client: &C,
    current_exe: &Path,
    current_version_str: &str,
    current_is_trunk: bool,
    version: Option<String>,
    trunk: bool,
    stable: bool,
    force: bool,
) -> Result<()> {
    let current_version = Version::parse(current_version_str);
    let use_trunk_release = version.is_none() && !stable && (trunk || current_is_trunk);

    let release = client.get_release(version.as_deref(), use_trunk_release)?;

    let should_install = if version.is_some() || use_trunk_release {
        // An explicit version always wins; otherwise stay on the active trunk channel.
        if version.is_none() && use_trunk_release {
            println!("  {} trunk release", "Installing".green().bold());
        } else {
            println!("  {} {}", "Installing".green().bold(), release.tag_name);
        }
        true
    } else if stable {
        let clean_tag = release.tag_name.trim_start_matches('v');
        if let Ok(target_version) = Version::parse(clean_tag) {
            if current_is_trunk {
                // when we are on a trunk build and user provide `--stable`, update to latest stable version
                println!(
                    "  {} stable version {} (current: trunk)",
                    "Installing".green().bold(),
                    target_version
                );
                true
            } else if force {
                print_forced_install_release(
                    &target_version,
                    current_version.as_ref().ok(),
                    current_version_str,
                    true,
                );
                true
            } else if let Ok(current_version) = &current_version
                && &target_version != current_version
            {
                // if we on stable release, install new stable version
                println!(
                    "  {} stable version {} (current: {})",
                    "Installing".green().bold(),
                    target_version,
                    current_version
                );
                true
            } else {
                println!(
                    "  {} Acton is already at the latest stable version ({})",
                    "Up to date".green().bold(),
                    current_version?
                );
                false
            }
        } else {
            println!(
                "    {} Latest release tag '{}' is not a valid semver. Skipping auto-update.",
                "Skipping".yellow().bold(),
                release.tag_name
            );
            return Ok(());
        }
    } else {
        let clean_tag = release.tag_name.trim_start_matches('v');
        if let Ok(target_version) = Version::parse(clean_tag) {
            if force {
                print_forced_install_release(
                    &target_version,
                    current_version.as_ref().ok(),
                    current_version_str,
                    false,
                );
                true
            } else {
                let current_version = current_version.with_context(|| {
                    format!("Current Acton version '{current_version_str}' is not valid semver")
                })?;
                if target_version > current_version {
                    println!(
                        "    {} version {} (current: {})",
                        "Updating".green().bold(),
                        target_version,
                        current_version
                    );
                    true
                } else if target_version == current_version {
                    // If versions match, we're up to date
                    println!(
                        "  {} Acton is up to date (version {})",
                        "Up to date".green().bold(),
                        current_version
                    );
                    return Ok(());
                } else {
                    println!(
                        "  {} Acton is up to date (version {})",
                        "Up to date".green().bold(),
                        current_version
                    );
                    return Ok(());
                }
            }
        } else {
            println!(
                "   {} Latest release tag '{}' is not a valid semver. Skipping auto-update.",
                "Skipping".yellow().bold(),
                release.tag_name
            );
            return Ok(());
        }
    };

    if !should_install {
        return Ok(());
    }

    let archive = download_verified_release_archive(client, &release)?;

    install_binary(&archive.path, current_exe, current_version_str)?;

    println!("     {} to {}", "Updated".green().bold(), release.tag_name);

    Ok(())
}

fn print_forced_install_release(
    target_version: &Version,
    current_version: Option<&Version>,
    current_version_str: &str,
    stable: bool,
) {
    let action = if current_version.is_some_and(|current| current == target_version) {
        "Reinstalling".green().bold()
    } else {
        "Installing".green().bold()
    };
    let current_display =
        current_version.map_or_else(|| current_version_str.to_owned(), ToString::to_string);
    let subject = if stable { "stable version" } else { "version" };

    println!("  {action} {subject} {target_version} (current: {current_display})");
}

fn find_asset(release: &Release) -> Result<&Asset> {
    find_asset_for_target_triple(release, env!("TARGET_TRIPLE"))
}

pub(crate) fn download_verified_release_archive<C: ReleaseClient>(
    client: &C,
    release: &Release,
) -> Result<VerifiedReleaseArchive> {
    let asset = find_asset(release)?;
    let checksum_asset = find_checksum_asset(release, &asset.name)?;
    ensure_asset_size(asset, MAX_RELEASE_ARCHIVE_BYTES, "release archive")?;
    ensure_asset_size(
        checksum_asset,
        MAX_RELEASE_CHECKSUM_BYTES,
        "release checksum",
    )?;
    let tarball_path = client.download_asset(asset)?;
    let checksum_path = client.download_asset(checksum_asset)?;

    let verify_result = verify_sha256(&tarball_path, &checksum_path, &asset.name);
    let _ = fs::remove_file(&checksum_path);
    if let Err(err) = verify_result {
        let _ = fs::remove_file(&tarball_path);
        return Err(err);
    }

    Ok(VerifiedReleaseArchive { path: tarball_path })
}

pub(crate) fn find_asset_for_target_triple<'a>(
    release: &'a Release,
    target_triple: &str,
) -> Result<&'a Asset> {
    let expected_name = release_asset_name_for_target_triple(target_triple)?;

    release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(&expected_name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Release {} does not include an archive for target {}. Expected asset: {}",
                release.tag_name,
                target_triple,
                expected_name
            )
        })
}

pub(crate) fn release_asset_name_for_target_triple(target_triple: &str) -> Result<String> {
    if target_triple.trim().is_empty() {
        bail!("Target triple is empty");
    }
    if target_triple.contains("windows") {
        bail!("Acton release archives are not supported on Windows");
    }

    Ok(format!("acton-{target_triple}.tar.gz"))
}

fn ensure_asset_size(asset: &Asset, max_size: u64, label: &str) -> Result<()> {
    if asset.size > max_size {
        bail!(
            "Refusing to download {} {} because release metadata reports {} bytes, above the {} byte limit.",
            label,
            asset.name,
            asset.size,
            max_size
        );
    }

    Ok(())
}

fn find_checksum_asset<'a>(release: &'a Release, archive_name: &str) -> Result<&'a Asset> {
    let checksum_name = format!("{archive_name}.sha256");

    release
        .assets
        .iter()
        .find(|asset| asset.name.eq_ignore_ascii_case(&checksum_name))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Release {} is missing the checksum file {}. The release may be incomplete.",
                release.tag_name,
                checksum_name
            )
        })
}

fn verify_sha256(tarball_path: &Path, checksum_path: &Path, archive_name: &str) -> Result<()> {
    let expected = read_expected_sha256(checksum_path, archive_name)?;
    let actual = compute_sha256(tarball_path)?;

    if actual != expected {
        bail!(
            "Downloaded archive {archive_name} failed SHA256 verification. Expected {expected}, got {actual}."
        );
    }

    println!("    {} {archive_name}.sha256", "Verified".green().bold());

    Ok(())
}

fn read_expected_sha256(checksum_path: &Path, archive_name: &str) -> Result<String> {
    let contents = fs::read_to_string(checksum_path).with_context(|| {
        format!("Failed to read the downloaded checksum file for {archive_name}")
    })?;

    let line = contents
        .lines()
        .map(str::trim)
        .find(|line| !line.is_empty())
        .ok_or_else(|| {
            anyhow::anyhow!("The downloaded checksum file for {archive_name} is empty")
        })?;

    let mut parts = line.split_whitespace();
    let checksum = parts.next().ok_or_else(|| {
        anyhow::anyhow!("The downloaded checksum file for {archive_name} has an invalid format")
    })?;

    if checksum.len() != 64 || !checksum.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("The downloaded checksum file for {archive_name} contains an invalid SHA256 digest");
    }

    if let Some(reported_name) = parts.next() {
        let reported_name = reported_name.trim_start_matches('*');
        if reported_name != archive_name {
            bail!(
                "The downloaded checksum file references '{reported_name}', but the archive is '{archive_name}'"
            );
        }
    }

    Ok(checksum.to_ascii_lowercase())
}

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file =
        File::open(path).with_context(|| format!("Failed to open file {}", path.display()))?;
    let mut hasher = Sha256::new();
    let mut buf = [0_u8; 8192];

    loop {
        let read = file.read(&mut buf)?;
        if read == 0 {
            break;
        }
        hasher.update(&buf[..read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}

pub(crate) fn extract_acton_binary(tarball_path: &Path) -> Result<ExtractedActonBinary> {
    let tar_gz = File::open(tarball_path).with_context(|| {
        format!(
            "Failed to open the downloaded release archive {}",
            tarball_path.display()
        )
    })?;
    let tar = GzDecoder::new(tar_gz);
    let mut archive = Archive::new(tar);

    let temp_dir = tempfile::tempdir()
        .context("Failed to prepare a temporary directory for extracting the new Acton binary")?;
    let new_bin_path = temp_dir.path().join("acton");
    let mut found = false;
    let mut entries_seen = 0usize;

    let entries = archive
        .entries()
        .context("Failed to read the downloaded release archive. The archive may be corrupted.")?;
    for entry in entries {
        entries_seen += 1;
        if entries_seen > MAX_RELEASE_ARCHIVE_ENTRIES {
            bail!(
                "The downloaded release archive contains more than {MAX_RELEASE_ARCHIVE_ENTRIES} entries"
            );
        }

        let mut entry = entry.context(
            "Failed to read an entry from the downloaded release archive. The archive may be corrupted.",
        )?;
        if !entry.header().entry_type().is_file() {
            continue;
        }

        let is_acton_binary = {
            let entry_path = entry
                .path()
                .context("Failed to read a file path from the downloaded release archive")?;
            entry_path.file_name().and_then(|name| name.to_str()) == Some("acton")
        };
        if !is_acton_binary {
            continue;
        }

        let size = entry
            .header()
            .size()
            .context("Failed to read the Acton binary size from the release archive")?;
        if size > MAX_EXTRACTED_ACTON_BYTES {
            bail!(
                "The Acton binary in the downloaded release archive is {} bytes, above the {} byte limit.",
                size,
                MAX_EXTRACTED_ACTON_BYTES
            );
        }

        entry.unpack(&new_bin_path).context(
            "Failed to extract the Acton binary from the downloaded release archive. The archive may be corrupted.",
        )?;
        found = true;
        break;
    }

    if !found {
        bail!("The downloaded release archive does not contain an `acton` binary");
    }

    Ok(ExtractedActonBinary {
        path: new_bin_path,
        _temp_dir: temp_dir,
    })
}

fn install_binary(tarball_path: &Path, current_exe: &Path, current_version: &str) -> Result<()> {
    let extracted = extract_acton_binary(tarball_path)?;

    let bin_dir = current_exe.parent().ok_or_else(|| {
        anyhow::anyhow!("Could not determine the directory of the current Acton binary")
    })?;

    let backup_name = format!("acton-{current_version}");
    let backup_path = bin_dir.join(&backup_name);

    // 1. Create backup by copying current binary
    fs::copy(current_exe, &backup_path).with_context(|| {
        format!(
            "Failed to create a backup of the current Acton binary at {}",
            backup_path.display()
        )
    })?;

    // 2. Prepare new binary in a temporary file in the same directory to ensure atomic rename
    let temp_file = tempfile::NamedTempFile::new_in(bin_dir)
        .context("Failed to create a temporary file for the new Acton binary")?;

    fs::copy(&extracted.path, temp_file.path())
        .context("Failed to copy the new Acton binary into the temporary file")?;

    // 3. Set permissions on the temporary file
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(temp_file.path())?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(temp_file.path(), perms)?;
    }

    // 4. Atomically replace the current binary
    temp_file.persist(current_exe).context(
        "Failed to replace the current Acton binary. Try re-running with sufficient permissions.",
    )?;

    let _ = fs::remove_file(tarball_path);

    Ok(())
}
