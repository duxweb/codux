use crate::runtime_paths::{
    app_display_name, app_support_dir, live_log_path, runtime_log_path, runtime_log_preview_path,
    runtime_temp_dir,
};
use crate::runtime_trace::rotated_log_paths;
use crate::settings::AppSettings;
use crate::update::UpdateService;
pub use crate::update::UpdateStatus;
use base64::{Engine as _, engine::general_purpose};
use chrono::Utc;
use minisign_verify::{PublicKey, Signature};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufReader, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

const TAURI_UPDATER_PUBLIC_KEY: &str = "RWTIDGGsK4geAihw4QK08H+tw5BUDYrQDww6GRCVQKWtH6RvOVe/huaA";
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppAboutMetadata {
    pub name: String,
    pub version: String,
    pub identifier: String,
    pub description: String,
    pub target_os: String,
    pub target_arch: String,
    pub build_profile: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExportRequest {
    pub destination_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsExportResult {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallResult {
    pub installed: bool,
    pub restart_to_install: bool,
    pub version: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
    pub message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInstallProgressEvent {
    pub phase: String,
    pub version: Option<String>,
    pub downloaded_bytes: u64,
    pub total_bytes: Option<u64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppDiagnosticsSnapshot {
    pub settings: Value,
    pub projects: Value,
    pub ai_state: Value,
    pub performance: Value,
    pub ssh: Value,
}

pub fn about_metadata(
    version: impl Into<String>,
    identifier: impl Into<String>,
) -> AppAboutMetadata {
    AppAboutMetadata {
        name: app_display_name().to_string(),
        version: version.into(),
        identifier: identifier.into(),
        description: env!("CARGO_PKG_DESCRIPTION").to_string(),
        target_os: std::env::consts::OS.to_string(),
        target_arch: std::env::consts::ARCH.to_string(),
        build_profile: if cfg!(debug_assertions) {
            "debug".to_string()
        } else {
            "release".to_string()
        },
    }
}

pub fn update_status(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
) -> UpdateStatus {
    UpdateService::status_from_settings(settings, repo_root, current_version)
}

pub fn export_diagnostics(
    request: DiagnosticsExportRequest,
    about: AppAboutMetadata,
    update: UpdateStatus,
    snapshot: AppDiagnosticsSnapshot,
) -> Result<DiagnosticsExportResult, String> {
    let destination = normalize_destination(&request.destination_path)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let report = json!({
        "generatedAt": Utc::now().to_rfc3339(),
        "app": about,
        "update": update,
        "paths": {
            "appSupport": app_support_dir().display().to_string(),
            "runtimeTemp": runtime_temp_dir().display().to_string(),
            "runtimeLog": runtime_log_path().display().to_string(),
            "liveLog": live_log_path().display().to_string()
        },
        "settings": redact_settings(snapshot.settings),
        "projects": snapshot.projects,
        "aiState": snapshot.ai_state,
        "performance": snapshot.performance,
        "ssh": redact_ssh(snapshot.ssh),
        "environment": {
            "os": std::env::consts::OS,
            "arch": std::env::consts::ARCH,
            "debug": cfg!(debug_assertions)
        }
    });
    let data = serde_json::to_vec_pretty(&report).map_err(|error| error.to_string())?;
    fs::write(&destination, &data).map_err(|error| error.to_string())?;
    Ok(DiagnosticsExportResult {
        path: destination.display().to_string(),
        bytes: data.len() as u64,
    })
}

pub fn write_runtime_log_preview() -> Result<PathBuf, String> {
    let path = runtime_log_path();
    if !path.exists() {
        open_or_create_text_file(
            &path,
            &format!(
                "{} runtime log\nThe runtime has not written log entries yet.\n",
                app_display_name()
            ),
        )?;
    }
    let preview_path = runtime_log_preview_path();
    write_runtime_log_preview_file(&path, &preview_path)?;
    Ok(preview_path)
}

pub fn ensure_live_log() -> Result<PathBuf, String> {
    let path = live_log_path();
    open_or_create_text_file(
        &path,
        &format!(
            "{} live log\nAI hook and polling activity is handled by the Rust runtime.\n",
            app_display_name()
        ),
    )?;
    Ok(path)
}

pub fn install_update(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
) -> Result<UpdateInstallResult, String> {
    install_update_with_progress(settings, repo_root, current_version, |_| {})
}

pub fn install_update_with_progress(
    settings: &AppSettings,
    repo_root: PathBuf,
    current_version: &str,
    mut on_progress: impl FnMut(UpdateInstallProgressEvent) + Send,
) -> Result<UpdateInstallResult, String> {
    let status = update_status(settings, repo_root, current_version);
    if !status.available {
        return Ok(UpdateInstallResult {
            installed: false,
            restart_to_install: false,
            version: status.latest_version,
            downloaded_bytes: 0,
            total_bytes: None,
            message: status.message,
        });
    }

    let download_url = status
        .download_url
        .as_deref()
        .ok_or_else(|| "Update is available, but no download URL was provided.".to_string())?;
    on_progress(UpdateInstallProgressEvent {
        phase: "downloading".to_string(),
        version: status.latest_version.clone(),
        downloaded_bytes: 0,
        total_bytes: None,
    });
    let destination = download_update(download_url, status.latest_version.as_deref(), |event| {
        on_progress(UpdateInstallProgressEvent {
            version: status.latest_version.clone(),
            ..event
        });
    })?;
    verify_download_checksum(&destination.path, status.download_checksum.as_deref())?;
    verify_download_signature(&destination.path, status.download_signature.as_deref())?;
    let pending_install = prepare_update_install(&destination.path)?;
    on_progress(UpdateInstallProgressEvent {
        phase: "finished".to_string(),
        version: status.latest_version.clone(),
        downloaded_bytes: destination.bytes,
        total_bytes: Some(destination.bytes),
    });
    Ok(UpdateInstallResult {
        installed: pending_install.installed,
        restart_to_install: pending_install.restart_to_install,
        version: status.latest_version,
        downloaded_bytes: destination.bytes,
        total_bytes: Some(destination.bytes),
        message: pending_install.message,
    })
}

pub fn request_restart() -> Result<(), String> {
    if let Some(helper_path) = pending_update_installer_path()? {
        spawn_update_installer_helper(&helper_path)?;
        std::process::exit(0);
    }
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    Command::new(exe)
        .args(args)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
}

fn spawn_update_installer_helper(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "windows")]
    {
        let mut command = Command::new("cmd");
        command.arg("/C").arg("start").arg("").arg(path);
        apply_no_window(&mut command);
        return command
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string());
    }
    #[cfg(not(target_os = "windows"))]
    {
        Command::new("sh")
            .arg(path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

pub fn open_runtime_log() -> Result<(), String> {
    let path = write_runtime_log_preview()?;
    open_path(&path)
}

pub fn open_live_log() -> Result<(), String> {
    let path = ensure_live_log()?;
    open_path(&path)
}

pub fn open_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL cannot be empty.".to_string());
    }
    let parsed = Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => open_target(parsed.as_str()),
        "file" => {
            let path = parsed
                .to_file_path()
                .map_err(|_| "Invalid file URL.".to_string())?;
            open_path(&path)
        }
        _ => Err("Only http, https, and file URLs can be opened.".to_string()),
    }
}

pub fn open_url_with_http_proxy(
    url: &str,
    proxy_host: &str,
    proxy_port: u16,
) -> Result<(), String> {
    let url = url.trim();
    if url.is_empty() {
        return Err("URL cannot be empty.".to_string());
    }
    let parsed = Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))?;
    match parsed.scheme() {
        "http" | "https" => open_target_with_http_proxy(parsed.as_str(), proxy_host, proxy_port),
        _ => Err("Only http and https URLs can be opened with a proxy.".to_string()),
    }
}

fn open_or_create_text_file(path: &Path, initial_content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if !path.exists() {
        fs::write(path, initial_content).map_err(|error| error.to_string())?;
    }
    Ok(())
}

fn write_runtime_log_preview_file(source: &Path, destination: &Path) -> Result<(), String> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let lines = tail_runtime_log_family_lines(source, 1200, 256 * 1024)?;
    let mut output = String::new();
    output.push_str(&format!("{} runtime log preview\n\n", app_display_name()));
    for line in lines {
        output.push_str(&line);
        output.push('\n');
    }
    fs::write(destination, output).map_err(|error| error.to_string())
}

fn tail_runtime_log_family_lines(
    source: &Path,
    max_lines: usize,
    max_bytes: usize,
) -> Result<VecDeque<String>, String> {
    let mut lines = VecDeque::with_capacity(max_lines);
    for path in rotated_log_paths(source).into_iter().rev() {
        let Ok(file_lines) = tail_runtime_log_lines(&path, max_lines, max_bytes) else {
            continue;
        };
        append_runtime_log_lines(&mut lines, file_lines, max_lines);
    }
    append_runtime_log_lines(
        &mut lines,
        tail_runtime_log_lines(source, max_lines, max_bytes)?,
        max_lines,
    );
    Ok(lines)
}

fn append_runtime_log_lines(
    lines: &mut VecDeque<String>,
    new_lines: impl IntoIterator<Item = String>,
    max_lines: usize,
) {
    for line in new_lines {
        lines.push_back(line);
        if lines.len() > max_lines {
            let _ = lines.pop_front();
        }
    }
}

fn tail_runtime_log_lines(
    source: &Path,
    max_lines: usize,
    max_bytes: usize,
) -> Result<VecDeque<String>, String> {
    let file = fs::File::open(source).map_err(|error| error.to_string())?;
    let file_len = file.metadata().map_err(|error| error.to_string())?.len() as usize;
    let read_len = file_len.min(max_bytes);
    let mut reader = BufReader::new(file);
    let mut buffer = Vec::with_capacity(read_len);
    if read_len > 0 {
        reader
            .seek(SeekFrom::End(-(read_len as i64)))
            .map_err(|error| error.to_string())?;
        reader
            .read_to_end(&mut buffer)
            .map_err(|error| error.to_string())?;
    }

    let tail = String::from_utf8_lossy(&buffer);
    let mut lines = VecDeque::with_capacity(max_lines);
    append_runtime_log_lines(
        &mut lines,
        tail.lines()
            .filter(|line| !line.trim().is_empty())
            .map(str::to_string),
        max_lines,
    );
    Ok(lines)
}

fn normalize_destination(path: &str) -> Result<PathBuf, String> {
    let trimmed = path.trim();
    if trimmed.is_empty() {
        return Err("Diagnostics destination cannot be empty.".to_string());
    }
    let mut destination = PathBuf::from(trimmed);
    if destination.extension().is_none() {
        destination.set_extension("json");
    }
    Ok(destination)
}

fn open_path(path: &Path) -> Result<(), String> {
    open_target(&path.display().to_string())
}

struct UpdateDownloadDestination {
    path: PathBuf,
    bytes: u64,
}

struct PendingUpdateInstall {
    installed: bool,
    restart_to_install: bool,
    message: String,
}

fn download_update(
    url: &str,
    version: Option<&str>,
    mut on_progress: impl FnMut(UpdateInstallProgressEvent) + Send,
) -> Result<UpdateDownloadDestination, String> {
    let destination = update_download_path(url, version)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let result = crate::async_runtime::block_on(async {
        let mut response = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(600))
            .build()
            .map_err(|error| error.to_string())?
            .get(url)
            .send()
            .await
            .and_then(|response| response.error_for_status())
            .map_err(|error| error.to_string())?;
        let total = response.content_length();
        let temp_path = destination.with_extension(format!(
            "{}download",
            destination
                .extension()
                .and_then(|extension| extension.to_str())
                .map(|extension| format!("{extension}."))
                .unwrap_or_default()
        ));
        let mut file = fs::File::create(&temp_path).map_err(|error| error.to_string())?;
        let mut downloaded = 0_u64;
        while let Some(chunk) = response.chunk().await.map_err(|error| error.to_string())? {
            file.write_all(&chunk).map_err(|error| error.to_string())?;
            downloaded += chunk.len() as u64;
            on_progress(UpdateInstallProgressEvent {
                phase: "downloading".to_string(),
                version: None,
                downloaded_bytes: downloaded,
                total_bytes: total,
            });
        }
        file.flush().map_err(|error| error.to_string())?;
        fs::rename(&temp_path, &destination).map_err(|error| error.to_string())?;
        Ok::<u64, String>(downloaded)
    });
    result.map(|bytes| UpdateDownloadDestination {
        path: destination,
        bytes,
    })
}

fn update_download_path(url: &str, version: Option<&str>) -> Result<PathBuf, String> {
    let parsed = Url::parse(url).map_err(|error| format!("Invalid URL: {error}"))?;
    let file_name = parsed
        .path_segments()
        .and_then(|segments| segments.filter(|segment| !segment.is_empty()).next_back())
        .map(percent_encoding::percent_decode_str)
        .map(|decoded| decoded.decode_utf8_lossy().to_string())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| {
            let version = version.unwrap_or("latest");
            if cfg!(target_os = "macos") {
                format!("Codux-{version}.dmg")
            } else if cfg!(target_os = "windows") {
                format!("Codux-{version}.zip")
            } else {
                format!("Codux-{version}.tar.gz")
            }
        });
    Ok(runtime_temp_dir().join("updates").join(file_name))
}

fn verify_download_checksum(path: &Path, expected: Option<&str>) -> Result<(), String> {
    let Some(expected) = expected.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    let actual = sha256_hex(&bytes);
    if actual.eq_ignore_ascii_case(expected) {
        Ok(())
    } else {
        Err(format!(
            "Downloaded update checksum mismatch. expected={expected} actual={actual}"
        ))
    }
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let mut hasher = Sha256::new();
    hasher.update(bytes);
    format!("{:x}", hasher.finalize())
}

fn verify_download_signature(path: &Path, signature: Option<&str>) -> Result<(), String> {
    let Some(signature) = signature.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let public_key = PublicKey::from_base64(TAURI_UPDATER_PUBLIC_KEY)
        .map_err(|error| format!("Invalid updater public key: {error}"))?;
    let signature = decode_updater_signature(signature)?;
    let bytes = fs::read(path).map_err(|error| error.to_string())?;
    public_key
        .verify(&bytes, &signature, true)
        .map_err(|error| format!("Downloaded update signature verification failed: {error}"))
}

fn decode_updater_signature(signature: &str) -> Result<Signature, String> {
    match Signature::decode(signature) {
        Ok(signature) => Ok(signature),
        Err(raw_error) => {
            let decoded = general_purpose::STANDARD
                .decode(signature)
                .map_err(|_| format!("Invalid updater signature: {raw_error}"))?;
            let decoded = std::str::from_utf8(&decoded)
                .map_err(|_| format!("Invalid updater signature: {raw_error}"))?;
            Signature::decode(decoded)
                .map_err(|error| format!("Invalid updater signature: {error}"))
        }
    }
}

fn prepare_update_install(artifact_path: &Path) -> Result<PendingUpdateInstall, String> {
    #[cfg(target_os = "macos")]
    {
        return prepare_macos_update_install(artifact_path);
    }
    #[cfg(target_os = "windows")]
    {
        return prepare_windows_update_install(artifact_path);
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        open_path(artifact_path)?;
        Ok(PendingUpdateInstall {
            installed: false,
            restart_to_install: false,
            message: format!(
                "Update downloaded to {}. Complete the installer to update Codux.",
                artifact_path.display()
            ),
        })
    }
}

#[cfg(target_os = "windows")]
fn prepare_windows_update_install(artifact_path: &Path) -> Result<PendingUpdateInstall, String> {
    if artifact_path.extension().and_then(|ext| ext.to_str()) != Some("exe") {
        open_path(artifact_path)?;
        return Ok(PendingUpdateInstall {
            installed: false,
            restart_to_install: false,
            message: format!(
                "Update downloaded to {}. Complete the installer to update Codux.",
                artifact_path.display()
            ),
        });
    }
    let helper_path = runtime_temp_dir()
        .join("updates")
        .join("install-update.cmd");
    fs::create_dir_all(helper_path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|error| error.to_string())?;
    let script = windows_update_install_script(artifact_path, std::process::id())?;
    fs::write(&helper_path, script).map_err(|error| error.to_string())?;
    Ok(PendingUpdateInstall {
        installed: false,
        restart_to_install: true,
        message: "Update downloaded. Restart Codux to apply it.".to_string(),
    })
}

#[cfg(target_os = "windows")]
fn windows_update_install_script(artifact_path: &Path, parent_pid: u32) -> Result<String, String> {
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let install_dir = current_exe
        .parent()
        .ok_or_else(|| "Unable to resolve current application directory.".to_string())?;
    Ok(format!(
        r#"@echo off
setlocal
set "ARTIFACT={artifact}"
set "PARENT_PID={parent_pid}"
set "INSTALL_DIR={install_dir}"
set "INSTALL_EXE={current_exe}"
for /l %%i in (1,1,150) do (
  tasklist /fi "PID eq %PARENT_PID%" | find "%PARENT_PID%" >nul
  if errorlevel 1 goto install
  powershell -NoProfile -Command "Start-Sleep -Milliseconds 200"
)
goto cleanup
:install
powershell -NoProfile -Command "Start-Process -FilePath $env:ARTIFACT -ArgumentList @('/S',('/D=' + $env:INSTALL_DIR)) -Wait"
if exist "%INSTALL_EXE%" start "" "%INSTALL_EXE%"
:cleanup
del "%~f0"
"#,
        artifact = artifact_path.display(),
        parent_pid = parent_pid,
        install_dir = install_dir.display(),
        current_exe = current_exe.display(),
    ))
}

#[cfg(target_os = "macos")]
fn prepare_macos_update_install(artifact_path: &Path) -> Result<PendingUpdateInstall, String> {
    if !is_macos_update_archive(artifact_path) {
        open_path(artifact_path)?;
        return Ok(PendingUpdateInstall {
            installed: false,
            restart_to_install: false,
            message: format!(
                "Update downloaded to {}. Complete the installer to update Codux.",
                artifact_path.display()
            ),
        });
    }
    let current_exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let Some(current_app) = current_macos_app_bundle(&current_exe) else {
        open_path(artifact_path)?;
        return Ok(PendingUpdateInstall {
            installed: false,
            restart_to_install: false,
            message: format!(
                "Update downloaded to {}. Complete the installer to update Codux.",
                artifact_path.display()
            ),
        });
    };
    let staging_dir = runtime_temp_dir().join("updates").join("install-staging");
    let helper_path = runtime_temp_dir().join("updates").join("install-update.sh");
    fs::create_dir_all(&staging_dir).map_err(|error| error.to_string())?;
    fs::create_dir_all(helper_path.parent().unwrap_or_else(|| Path::new(".")))
        .map_err(|error| error.to_string())?;
    let script = macos_update_install_script(
        artifact_path,
        &staging_dir,
        &current_app,
        std::process::id(),
    );
    fs::write(&helper_path, script).map_err(|error| error.to_string())?;
    set_executable(&helper_path)?;
    Ok(PendingUpdateInstall {
        installed: false,
        restart_to_install: true,
        message: "Update downloaded. Restart Codux to apply it.".to_string(),
    })
}

#[cfg(target_os = "macos")]
fn is_macos_update_archive(path: &Path) -> bool {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    file_name.ends_with(".app.tar.gz") || file_name.ends_with(".app.zip")
}

#[cfg(target_os = "macos")]
fn current_macos_app_bundle(exe: &Path) -> Option<PathBuf> {
    let mut current = exe;
    while let Some(parent) = current.parent() {
        if parent.file_name().and_then(|name| name.to_str()) == Some("Contents") {
            return Some(parent.parent()?.to_path_buf());
        }
        current = parent;
    }
    None
}

#[cfg(target_os = "macos")]
fn macos_update_install_script(
    artifact_path: &Path,
    staging_dir: &Path,
    current_app: &Path,
    parent_pid: u32,
) -> String {
    let app_name = current_app
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Codux.app");
    let reopen_path = current_app.display().to_string();
    format!(
        r#"#!/bin/sh
set -eu
ARTIFACT={artifact}
STAGING={staging}
CURRENT_APP={current_app}
APP_NAME={app_name}
PARENT_PID={parent_pid}
trap 'rm -f "$0"' EXIT
rm -rf "$STAGING"
mkdir -p "$STAGING"
case "$ARTIFACT" in
  *.app.tar.gz)
    /usr/bin/tar -xzf "$ARTIFACT" -C "$STAGING"
    ;;
  *.app.zip)
    /usr/bin/ditto -x -k "$ARTIFACT" "$STAGING"
    ;;
  *)
    /usr/bin/open "$ARTIFACT"
    exit 1
    ;;
esac
NEW_APP="$STAGING/$APP_NAME"
if [ ! -d "$NEW_APP" ]; then
  NEW_APP="$(/usr/bin/find "$STAGING" -maxdepth 2 -name '*.app' -type d | /usr/bin/head -n 1)"
fi
if [ -z "${{NEW_APP:-}}" ] || [ ! -d "$NEW_APP" ]; then
  /usr/bin/open "$ARTIFACT"
  exit 1
fi
wait_count=0
while /bin/kill -0 "$PARENT_PID" 2>/dev/null; do
  if [ "$wait_count" -ge 150 ]; then
    /usr/bin/open "$ARTIFACT"
    exit 1
  fi
  wait_count=$((wait_count + 1))
  /bin/sleep 0.2
done
BACKUP="$CURRENT_APP.previous"
rm -rf "$BACKUP"
if [ -d "$CURRENT_APP" ]; then
  mv "$CURRENT_APP" "$BACKUP"
fi
if ! /usr/bin/ditto "$NEW_APP" "$CURRENT_APP"; then
  rm -rf "$CURRENT_APP"
  if [ -d "$BACKUP" ]; then
    mv "$BACKUP" "$CURRENT_APP"
  fi
  /usr/bin/open "$ARTIFACT"
  exit 1
fi
rm -rf "$BACKUP"
/usr/bin/open {reopen}
"#,
        artifact = shell_quote(&artifact_path.display().to_string()),
        staging = shell_quote(&staging_dir.display().to_string()),
        current_app = shell_quote(&current_app.display().to_string()),
        app_name = shell_quote(app_name),
        parent_pid = parent_pid,
        reopen = shell_quote(&reopen_path),
    )
}

#[cfg(target_os = "macos")]
fn set_executable(path: &Path) -> Result<(), String> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = fs::metadata(path)
        .map_err(|error| error.to_string())?
        .permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).map_err(|error| error.to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn set_executable(_path: &Path) -> Result<(), String> {
    Ok(())
}

#[cfg(target_os = "macos")]
fn pending_update_installer_path() -> Result<Option<PathBuf>, String> {
    let path = runtime_temp_dir().join("updates").join("install-update.sh");
    if path.is_file() {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

#[cfg(target_os = "windows")]
fn pending_update_installer_path() -> Result<Option<PathBuf>, String> {
    let path = runtime_temp_dir()
        .join("updates")
        .join("install-update.cmd");
    if path.is_file() {
        Ok(Some(path))
    } else {
        Ok(None)
    }
}

#[cfg(not(any(target_os = "macos", target_os = "windows")))]
fn pending_update_installer_path() -> Result<Option<PathBuf>, String> {
    Ok(None)
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn open_target(target: &str) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return spawn_open_command("open", &[target]);
    }
    #[cfg(target_os = "windows")]
    {
        return spawn_open_command("cmd", &["/C", "start", "", target]);
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        return spawn_open_command("xdg-open", &[target]);
    }
}

fn open_target_with_http_proxy(
    target: &str,
    proxy_host: &str,
    proxy_port: u16,
) -> Result<(), String> {
    let proxy_arg = format!("--proxy-server=http://{proxy_host}:{proxy_port}");
    let proxy_bypass_arg = "--proxy-bypass-list=<-loopback>";
    let user_data_dir = runtime_temp_dir().join(format!("web-tunnel-browser-{proxy_port}"));
    let user_data_arg = format!("--user-data-dir={}", user_data_dir.to_string_lossy());
    let browser_args = [
        "--no-first-run",
        "--no-default-browser-check",
        "--disable-default-apps",
        "--disable-features=DialMediaRouteProvider",
        &proxy_arg,
        proxy_bypass_arg,
        &user_data_arg,
        target,
    ];
    #[cfg(target_os = "macos")]
    {
        let browsers = [
            "Google Chrome",
            "Chromium",
            "Microsoft Edge",
            "Brave Browser",
        ];
        for browser in browsers {
            if open_macos_app_with_args(browser, &browser_args).is_ok() {
                return Ok(());
            }
        }
        return Err("No Chromium-based browser found for Web Tunnel proxy mode.".to_string());
    }
    #[cfg(target_os = "windows")]
    {
        let browsers = ["chrome", "msedge", "chromium", "brave"];
        for browser in browsers {
            if spawn_open_command(browser, &browser_args).is_ok() {
                return Ok(());
            }
        }
        return Err("No Chromium-based browser found for Web Tunnel proxy mode.".to_string());
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let browsers = [
            "google-chrome",
            "chromium",
            "chromium-browser",
            "microsoft-edge",
            "brave-browser",
        ];
        for browser in browsers {
            if spawn_open_command(browser, &browser_args).is_ok() {
                return Ok(());
            }
        }
        Err("No Chromium-based browser found for Web Tunnel proxy mode.".to_string())
    }
}

#[cfg(target_os = "macos")]
fn open_macos_app_with_args(app_name: &str, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new("open");
    command.args(["-na", app_name, "--args"]);
    command.args(args);
    apply_no_window(&mut command);
    let status = command.status().map_err(|error| error.to_string())?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{app_name} is not available"))
    }
}

fn spawn_open_command(program: &str, args: &[&str]) -> Result<(), String> {
    let mut command = Command::new(program);
    command.args(args);
    apply_no_window(&mut command);
    command
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}

/// On Windows, launch the helper process without flashing a console window.
/// Opening a log/file goes through `cmd /C start ...`, and without this flag
/// `cmd.exe` briefly pops a console window before the associated app appears.
fn apply_no_window(command: &mut Command) {
    #[cfg(target_os = "windows")]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = command;
    }
}

fn redact_settings(mut settings: Value) -> Value {
    redact_sensitive_json_fields(&mut settings);
    settings
}

fn redact_ssh(mut snapshot: Value) -> Value {
    redact_sensitive_json_fields(&mut snapshot);
    let Some(profiles) = snapshot.get_mut("profiles").and_then(Value::as_array_mut) else {
        return snapshot;
    };
    for profile in profiles {
        if let Some(password) = profile.get_mut("password")
            && !password.is_null()
        {
            *password = Value::String("******".to_string());
        }
        if let Some(passphrase) = profile.get_mut("keyPassphrase")
            && !passphrase.is_null()
        {
            *passphrase = Value::String("******".to_string());
        }
    }
    snapshot
}

fn redact_sensitive_json_fields(value: &mut Value) {
    match value {
        Value::Object(object) => {
            for (key, value) in object.iter_mut() {
                if is_sensitive_json_key(key)
                    && value.as_str().is_some_and(|text| !text.trim().is_empty())
                {
                    *value = Value::String("******".to_string());
                } else {
                    redact_sensitive_json_fields(value);
                }
            }
        }
        Value::Array(items) => {
            for item in items {
                redact_sensitive_json_fields(item);
            }
        }
        _ => {}
    }
}

fn is_sensitive_json_key(key: &str) -> bool {
    let normalized = key
        .chars()
        .filter(|character| *character != '_' && *character != '-')
        .flat_map(char::to_lowercase)
        .collect::<String>();
    matches!(
        normalized.as_str(),
        "apikey" | "token" | "password" | "keypassphrase" | "privatekeypath"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_destination_adds_json_extension() {
        let path = normalize_destination("/tmp/codux-diagnostics").unwrap();
        assert_eq!(
            path.extension().and_then(|value| value.to_str()),
            Some("json")
        );
    }

    #[test]
    fn file_urls_parse_to_local_paths() {
        let parsed = Url::parse("file:///tmp/codux-log.txt").unwrap();
        let path = parsed.to_file_path().unwrap();
        assert_eq!(path, PathBuf::from("/tmp/codux-log.txt"));
    }

    #[test]
    fn redacts_sensitive_tokens() {
        let value = redact_settings(json!({
            "notificationChannels": {
                "a": {"token": "secret"},
                "b": {"token": ""}
            },
            "ai": {
                "providers": [
                    {"apiKey": "secret", "api_key": "secret"}
                ]
            },
        }));
        assert_eq!(value["notificationChannels"]["a"]["token"], "******");
        assert_eq!(value["notificationChannels"]["b"]["token"], "");
        assert_eq!(value["ai"]["providers"][0]["apiKey"], "******");
        assert_eq!(value["ai"]["providers"][0]["api_key"], "******");
    }

    #[test]
    fn runtime_log_preview_includes_recent_rotated_log() {
        let directory =
            std::env::temp_dir().join(format!("codux-runtime-preview-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("runtime-rust.log");
        let destination = directory.join("runtime-rust-preview.log");
        fs::write(source.with_file_name("runtime-rust.log.1"), "previous\n").unwrap();
        fs::write(&source, "current\n").unwrap();

        write_runtime_log_preview_file(&source, &destination).unwrap();

        let preview = fs::read_to_string(&destination).unwrap();
        assert!(preview.contains("previous"));
        assert!(preview.contains("current"));
        assert!(preview.find("previous") < preview.find("current"));

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn runtime_log_preview_requires_current_log() {
        let directory =
            std::env::temp_dir().join(format!("codux-runtime-preview-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&directory).unwrap();
        let source = directory.join("runtime-rust.log");
        fs::write(source.with_file_name("runtime-rust.log.1"), "previous\n").unwrap();

        let result = tail_runtime_log_family_lines(&source, 1200, 256 * 1024);

        assert!(result.is_err());

        let _ = fs::remove_dir_all(directory);
    }

    #[test]
    fn update_signature_verification_skips_empty_signature() {
        let path = std::env::temp_dir().join(format!(
            "codux-update-signature-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, b"payload").unwrap();

        assert!(verify_download_signature(&path, None).is_ok());
        assert!(verify_download_signature(&path, Some("")).is_ok());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn update_signature_verification_rejects_invalid_signature() {
        let path = std::env::temp_dir().join(format!(
            "codux-update-signature-test-{}",
            uuid::Uuid::new_v4()
        ));
        fs::write(&path, b"payload").unwrap();

        assert!(verify_download_signature(&path, Some("invalid")).is_err());

        let _ = fs::remove_file(path);
    }

    #[test]
    fn update_signature_decoder_accepts_tauri_base64_signature() {
        let signature = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVUSURHR3NLNGdlQWd5dUtWdks5SEhBbUFUa3ZSL3RGUE9EZVBLUlp0TEN2UE5IWkFwekVyYXRaOE43Z0xIY0lmYzk2cXVMSXc1UHl0U214cktwTTh5bWxjeno0enY2a2dFPQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzgwNTc5MjU5CWZpbGU6Y29kdXgtMS41LjEtbWFjb3MtdW5pdmVyc2FsLWZvcm1hbC11cGRhdGVyLmFwcC50YXIuZ3oKUDBLdTN4Szh1eWgyNmNVMlIvZkE5R1M4aDIvVWZOaGM3SXljRkFSdG5WOWlpN3laSnl2V3c4N1ZYeDkvUUpma1VieHpSNVVtMElLbmtqM2pKNVhiQlE9PQo=";

        assert!(decode_updater_signature(signature).is_ok());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_update_helper_waits_for_parent_and_cleans_itself() {
        let script = macos_update_install_script(
            Path::new("/tmp/Codux.app.tar.gz"),
            Path::new("/tmp/codux staging"),
            Path::new("/Applications/Codux.app"),
            12345,
        );

        assert!(script.contains("PARENT_PID=12345"));
        assert!(script.contains("while /bin/kill -0 \"$PARENT_PID\""));
        assert!(script.contains("trap 'rm -f \"$0\"' EXIT"));
        assert!(script.contains("/usr/bin/tar -xzf \"$ARTIFACT\""));
        assert!(script.contains("/usr/bin/ditto \"$NEW_APP\" \"$CURRENT_APP\""));
        assert!(script.contains("/usr/bin/open '/Applications/Codux.app'"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn macos_update_archive_accepts_tauri_tarball() {
        assert!(is_macos_update_archive(Path::new(
            "/tmp/Codux-updater.app.tar.gz"
        )));
        assert!(is_macos_update_archive(Path::new("/tmp/Codux.app.zip")));
        assert!(!is_macos_update_archive(Path::new("/tmp/Codux.dmg")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn current_macos_app_bundle_resolves_bundle_from_executable_path() {
        let bundle =
            current_macos_app_bundle(Path::new("/Applications/Codux.app/Contents/MacOS/Codux"));

        assert_eq!(bundle, Some(PathBuf::from("/Applications/Codux.app")));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_update_helper_runs_nsis_silently_and_reopens_app() {
        let script =
            windows_update_install_script(Path::new(r"C:\Temp\Codux-setup.exe"), 12345).unwrap();

        assert!(script.contains("PARENT_PID=12345"));
        assert!(script.contains("set \"INSTALL_DIR="));
        assert!(script.contains("Start-Process -FilePath $env:ARTIFACT"));
        assert!(script.contains("('/D=' + $env:INSTALL_DIR)"));
        assert!(script.contains("if exist \"%INSTALL_EXE%\" start \"\" \"%INSTALL_EXE%\""));
        assert!(script.contains("del \"%~f0\""));
    }
}
