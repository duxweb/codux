//! `codux update` — check GitHub Releases for a newer build, then download,
//! verify, atomically replace this binary, and restart the host if it was up.

use dialoguer::Confirm;
use dialoguer::theme::ColorfulTheme;
use serde::Deserialize;
use std::time::Duration;

use crate::{cmd_service, cmd_start, runstate};

const LATEST_RELEASE_API: &str = "https://api.github.com/repos/duxweb/codux/releases/latest";
const RELEASES_API: &str = "https://api.github.com/repos/duxweb/codux/releases";
const USER_AGENT: &str = "codux-agent-updater";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    #[serde(default)]
    prerelease: bool,
    #[serde(default)]
    assets: Vec<Asset>,
}

#[derive(Deserialize)]
struct Asset {
    name: String,
    browser_download_url: String,
}

pub fn run(current: &str, beta: bool) -> Result<(), String> {
    let client = reqwest::blocking::Client::builder()
        .user_agent(USER_AGENT)
        .timeout(Duration::from_secs(30))
        .build()
        .map_err(|error| error.to_string())?;

    println!("Checking for {}updates…", if beta { "beta " } else { "" });
    let release = fetch_release(&client, beta)?;

    let latest = release.tag_name.trim_start_matches('v');
    if !is_newer(latest, current) {
        println!("Already up to date (v{current}).");
        return Ok(());
    }
    println!("A newer version is available: v{latest} (current v{current}).");

    let asset = pick_asset(&release.assets, latest)?;
    let proceed = Confirm::with_theme(&ColorfulTheme::default())
        .with_prompt(format!("Download and install {} now?", asset.name))
        .default(true)
        .interact()
        .map_err(|error| error.to_string())?;
    if !proceed {
        println!("Update cancelled.");
        return Ok(());
    }

    let was_running = runstate::is_running();
    println!("Downloading {}…", asset.name);
    let bytes = client
        .get(&asset.browser_download_url)
        .send()
        .map_err(|error| format!("failed to reach GitHub: {error}"))?
        .error_for_status()
        .map_err(|error| format!("download failed: {error}"))?
        .bytes()
        .map_err(|error| error.to_string())?;
    if bytes.is_empty() {
        return Err("downloaded an empty file".to_string());
    }

    replace_self(&bytes)?;
    println!("Installed v{latest}.");

    if was_running {
        println!("Restarting the host…");
        let _ = cmd_service::stop();
        std::thread::sleep(Duration::from_millis(400));
        cmd_start::run(true)?;
    }
    Ok(())
}

fn fetch_release(client: &reqwest::blocking::Client, beta: bool) -> Result<Release, String> {
    if !beta {
        return client
            .get(LATEST_RELEASE_API)
            .send()
            .map_err(|error| format!("failed to reach GitHub: {error}"))?
            .error_for_status()
            .map_err(|error| format!("release lookup failed: {error}"))?
            .json()
            .map_err(|error| format!("invalid release payload: {error}"));
    }

    let releases = client
        .get(RELEASES_API)
        .send()
        .map_err(|error| format!("failed to reach GitHub: {error}"))?
        .error_for_status()
        .map_err(|error| format!("release lookup failed: {error}"))?
        .json::<Vec<Release>>()
        .map_err(|error| format!("invalid release payload: {error}"))?;
    select_beta_release(releases)
}

fn select_beta_release(releases: Vec<Release>) -> Result<Release, String> {
    releases
        .into_iter()
        .find(|release| release.prerelease)
        .ok_or_else(|| "no beta release found".to_string())
}

/// Choose the release asset for this OS + architecture.
fn pick_asset<'a>(assets: &'a [Asset], version: &str) -> Result<&'a Asset, String> {
    let os = match std::env::consts::OS {
        "macos" => "macos",
        "linux" => "linux",
        "windows" => "windows",
        other => other,
    };
    let arch = std::env::consts::ARCH; // x86_64 / aarch64
    let extension = if os == "windows" { ".exe" } else { "" };
    let expected = format!("codux-agent-{version}-{os}-{arch}{extension}");
    let legacy = format!("codux-{os}-{arch}{extension}");
    assets
        .iter()
        .find(|asset| asset.name == expected)
        .or_else(|| assets.iter().find(|asset| asset.name == legacy))
        .ok_or_else(|| {
            let available = assets
                .iter()
                .map(|asset| asset.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            format!("no release asset for {expected} (available: {available})")
        })
}

/// Atomically replace the running executable with `bytes`.
fn replace_self(bytes: &[u8]) -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let dir = exe.parent().ok_or("cannot resolve binary directory")?;
    let staged = dir.join(".codux.update.new");
    std::fs::write(&staged, bytes).map_err(|error| format!("failed to write update: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&staged)
            .map_err(|error| error.to_string())?
            .permissions();
        perms.set_mode(0o755);
        let _ = std::fs::set_permissions(&staged, perms);
        // Renaming over the running binary is allowed on unix.
        std::fs::rename(&staged, &exe)
            .map_err(|error| format!("failed to replace binary: {error}"))?;
    }

    #[cfg(windows)]
    {
        // A running .exe can't be overwritten, but it can be renamed aside.
        let old = dir.join(".codux.old.exe");
        let _ = std::fs::remove_file(&old);
        std::fs::rename(&exe, &old)
            .map_err(|error| format!("failed to move current binary: {error}"))?;
        std::fs::rename(&staged, &exe)
            .map_err(|error| format!("failed to install update: {error}"))?;
    }

    Ok(())
}

/// Numeric dotted-version comparison (`a` strictly newer than `b`).
fn is_newer(a: &str, b: &str) -> bool {
    let parse = |value: &str| -> Vec<u64> {
        value
            .split(['.', '-', '+'])
            .map(|part| part.parse::<u64>().unwrap_or(0))
            .collect()
    };
    let (left, right) = (parse(a), parse(b));
    for index in 0..left.len().max(right.len()) {
        let l = left.get(index).copied().unwrap_or(0);
        let r = right.get(index).copied().unwrap_or(0);
        if l != r {
            return l > r;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::{Asset, Release, is_newer, pick_asset, select_beta_release};

    #[test]
    fn newer_version_compares_numeric_parts() {
        assert!(is_newer("1.10.0", "1.9.9"));
        assert!(!is_newer("1.9.0", "1.9.0"));
        assert!(!is_newer("1.8.9", "1.9.0"));
    }

    #[test]
    fn pick_asset_ignores_desktop_assets() {
        let expected = current_agent_asset_name("1.9.1");
        let assets = vec![
            asset("codux-1.9.1-macos-aarch64.dmg"),
            asset("codux-1.9.1-windows-x86_64-setup.exe"),
            asset(&expected),
        ];

        assert_eq!(pick_asset(&assets, "1.9.1").unwrap().name, expected);
    }

    #[test]
    fn pick_asset_falls_back_to_legacy_agent_alias() {
        let legacy = current_legacy_agent_asset_name();
        let assets = vec![asset(&legacy)];

        assert_eq!(pick_asset(&assets, "1.9.1").unwrap().name, legacy);
    }

    #[test]
    fn select_beta_release_uses_first_prerelease() {
        let stable = release("v2.0.0", false);
        let beta = release("v2.1.0-beta.1", true);
        let older_beta = release("v2.0.0-beta.5", true);

        let selected = select_beta_release(vec![stable, beta, older_beta]).unwrap();

        assert_eq!(selected.tag_name, "v2.1.0-beta.1");
    }

    #[test]
    fn select_beta_release_requires_prerelease() {
        let error = match select_beta_release(vec![release("v2.0.0", false)]) {
            Ok(_) => panic!("expected missing beta release to fail"),
            Err(error) => error,
        };

        assert_eq!(error, "no beta release found");
    }

    fn current_agent_asset_name(version: &str) -> String {
        let os = match std::env::consts::OS {
            "macos" => "macos",
            "linux" => "linux",
            "windows" => "windows",
            other => other,
        };
        let extension = if os == "windows" { ".exe" } else { "" };
        format!(
            "codux-agent-{version}-{os}-{}{extension}",
            std::env::consts::ARCH
        )
    }

    fn current_legacy_agent_asset_name() -> String {
        let os = match std::env::consts::OS {
            "macos" => "macos",
            "linux" => "linux",
            "windows" => "windows",
            other => other,
        };
        let extension = if os == "windows" { ".exe" } else { "" };
        format!("codux-{os}-{}{extension}", std::env::consts::ARCH)
    }

    fn asset(name: &str) -> Asset {
        Asset {
            name: name.to_string(),
            browser_download_url: format!("https://example.com/{name}"),
        }
    }

    fn release(tag_name: &str, prerelease: bool) -> Release {
        Release {
            tag_name: tag_name.to_string(),
            prerelease,
            assets: Vec::new(),
        }
    }
}
