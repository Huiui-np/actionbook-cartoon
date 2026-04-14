use std::io::Cursor;
use std::path::{Path, PathBuf};

use serde_json::json;

use crate::action_result::ActionResult;
use crate::config;

pub const COMMAND_NAME_PATH: &str = "extension path";
pub const COMMAND_NAME_INSTALL: &str = "extension install";
pub const COMMAND_NAME_UNINSTALL: &str = "extension uninstall";

const GITHUB_REPO: &str = "actionbook/actionbook";

fn extension_dir() -> PathBuf {
    config::actionbook_home().join("extension")
}

fn read_version(dir: &Path) -> Option<String> {
    let bytes = std::fs::read(dir.join("manifest.json")).ok()?;
    let v: serde_json::Value = serde_json::from_slice(&bytes).ok()?;
    v["version"].as_str().map(String::from)
}

pub fn execute_path() -> ActionResult {
    let dir = extension_dir();
    let installed = dir.join("manifest.json").exists();
    let version = if installed { read_version(&dir) } else { None };
    ActionResult::ok(json!({
        "path": dir.to_string_lossy(),
        "installed": installed,
        "version": version,
    }))
}

pub async fn execute_install(force: bool) -> ActionResult {
    let dir = extension_dir();

    if dir.exists() && !force {
        return ActionResult::fatal(
            "ALREADY_INSTALLED",
            format!(
                "extension already installed at '{}'; use --force to overwrite",
                dir.display()
            ),
        );
    }

    // Test seam: copy from local source directory
    if let Ok(src) = std::env::var("ACTIONBOOK_EXTENSION_TEST_SOURCE_DIR") {
        return copy_from_dir(Path::new(&src), &dir);
    }

    // Production: download from GitHub Releases
    download_from_github(&dir).await
}

fn copy_from_dir(src: &Path, dst: &Path) -> ActionResult {
    if let Err(e) = std::fs::remove_dir_all(dst)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to remove existing install: {e}"),
        );
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to create install directory: {e}"),
        );
    }
    if let Err(e) = copy_dir_all(src, dst) {
        return ActionResult::fatal("IO_ERROR", format!("failed to copy extension files: {e}"));
    }
    let version = read_version(dst).unwrap_or_default();
    ActionResult::ok(json!({
        "path": dst.to_string_lossy(),
        "version": version,
    }))
}

fn copy_dir_all(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let file_type = entry.file_type()?;
        let dest = dst.join(entry.file_name());
        if file_type.is_dir() {
            std::fs::create_dir_all(&dest)?;
            copy_dir_all(&entry.path(), &dest)?;
        } else {
            std::fs::copy(entry.path(), dest)?;
        }
    }
    Ok(())
}

async fn download_from_github(dst: &Path) -> ActionResult {
    let client = match reqwest::Client::builder()
        .user_agent(format!("actionbook-cli/{}", crate::BUILD_VERSION))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ActionResult::fatal(
                "INTERNAL_ERROR",
                format!("failed to build HTTP client: {e}"),
            );
        }
    };

    // Query the latest release for the extension asset
    let api_url = format!("https://api.github.com/repos/{GITHUB_REPO}/releases/latest");
    let release: serde_json::Value = match client
        .get(&api_url)
        .header("Accept", "application/vnd.github+json")
        .send()
        .await
    {
        Ok(r) => match r.json().await {
            Ok(v) => v,
            Err(e) => {
                return ActionResult::fatal(
                    "DOWNLOAD_ERROR",
                    format!("failed to parse GitHub release response: {e}"),
                );
            }
        },
        Err(e) => {
            return ActionResult::fatal(
                "DOWNLOAD_ERROR",
                format!("failed to fetch latest release from GitHub: {e}"),
            );
        }
    };

    // Find the extension zip asset
    let download_url = match release["assets"].as_array().and_then(|assets| {
        assets.iter().find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.starts_with("actionbook-extension-") && n.ends_with(".zip"))
                .unwrap_or(false)
        })
    }) {
        Some(asset) => match asset["browser_download_url"].as_str() {
            Some(u) => u.to_string(),
            None => {
                return ActionResult::fatal(
                    "DOWNLOAD_ERROR",
                    "extension asset has no download URL",
                );
            }
        },
        None => {
            return ActionResult::fatal(
                "NOT_AVAILABLE",
                "extension zip not found in the latest GitHub release; \
                 install manually by extracting to ~/.actionbook/extension/",
            );
        }
    };

    // Download the zip bytes
    let zip_bytes = match client.get(&download_url).send().await {
        Ok(r) => match r.bytes().await {
            Ok(b) => b,
            Err(e) => {
                return ActionResult::fatal(
                    "DOWNLOAD_ERROR",
                    format!("failed to read extension zip: {e}"),
                );
            }
        },
        Err(e) => {
            return ActionResult::fatal(
                "DOWNLOAD_ERROR",
                format!("failed to download extension: {e}"),
            );
        }
    };

    // Remove existing install and recreate directory
    if let Err(e) = std::fs::remove_dir_all(dst)
        && e.kind() != std::io::ErrorKind::NotFound
    {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to remove existing install: {e}"),
        );
    }
    if let Err(e) = std::fs::create_dir_all(dst) {
        return ActionResult::fatal(
            "IO_ERROR",
            format!("failed to create install directory: {e}"),
        );
    }

    // Extract zip
    let cursor = Cursor::new(zip_bytes);
    let mut archive = match zip::ZipArchive::new(cursor) {
        Ok(a) => a,
        Err(e) => {
            return ActionResult::fatal(
                "EXTRACT_ERROR",
                format!("failed to open extension zip: {e}"),
            );
        }
    };

    for i in 0..archive.len() {
        let mut file = match archive.by_index(i) {
            Ok(f) => f,
            Err(e) => {
                return ActionResult::fatal(
                    "EXTRACT_ERROR",
                    format!("failed to read zip entry {i}: {e}"),
                );
            }
        };

        // Sanitize path: skip entries with absolute or traversal paths
        let out_path = match file.enclosed_name() {
            Some(p) => dst.join(p),
            None => continue,
        };

        if file.is_dir() {
            if let Err(e) = std::fs::create_dir_all(&out_path) {
                return ActionResult::fatal(
                    "EXTRACT_ERROR",
                    format!("failed to create directory '{}': {e}", out_path.display()),
                );
            }
        } else {
            if let Some(parent) = out_path.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    return ActionResult::fatal(
                        "EXTRACT_ERROR",
                        format!("failed to create parent directory: {e}"),
                    );
                }
            }
            let mut out_file = match std::fs::File::create(&out_path) {
                Ok(f) => f,
                Err(e) => {
                    return ActionResult::fatal(
                        "EXTRACT_ERROR",
                        format!("failed to create '{}': {e}", out_path.display()),
                    );
                }
            };
            if let Err(e) = std::io::copy(&mut file, &mut out_file) {
                return ActionResult::fatal(
                    "EXTRACT_ERROR",
                    format!("failed to extract '{}': {e}", out_path.display()),
                );
            }
        }
    }

    let version = read_version(dst).unwrap_or_default();
    ActionResult::ok(json!({
        "path": dst.to_string_lossy(),
        "version": version,
    }))
}

pub fn execute_uninstall() -> ActionResult {
    let dir = extension_dir();
    if !dir.exists() {
        return ActionResult::fatal("NOT_INSTALLED", "extension is not installed");
    }
    if let Err(e) = std::fs::remove_dir_all(&dir) {
        return ActionResult::fatal("IO_ERROR", format!("failed to remove extension: {e}"));
    }
    ActionResult::ok(json!({ "uninstalled": true }))
}
