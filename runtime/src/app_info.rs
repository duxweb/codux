use crate::runtime_paths::{
    app_display_name, app_support_dir, live_log_path, runtime_log_path, runtime_log_preview_path,
    runtime_temp_dir,
};
use crate::settings::AppSettings;
use crate::update::UpdateService;
pub use crate::update::UpdateStatus;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::VecDeque;
use std::fs;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::Command;
use url::Url;

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
    pub ai_runtime: Value,
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
        "aiRuntime": snapshot.ai_runtime,
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
    let status = update_status(settings, repo_root, current_version);
    if !status.available {
        return Ok(UpdateInstallResult {
            installed: false,
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
    open_url(download_url)?;
    Ok(UpdateInstallResult {
        installed: false,
        version: status.latest_version,
        downloaded_bytes: 0,
        total_bytes: None,
        message: "Update download URL opened. Automatic installation requires signed updater packaging."
            .to_string(),
    })
}

pub fn request_restart() -> Result<(), String> {
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let args = std::env::args_os().skip(1).collect::<Vec<_>>();
    Command::new(exe)
        .args(args)
        .spawn()
        .map_err(|error| error.to_string())?;
    Ok(())
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
        _ => Err("Only http and https URLs can be opened.".to_string()),
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
    let lines = tail_runtime_log_lines(source, 1200, 256 * 1024)?;
    let mut output = String::new();
    output.push_str(&format!("{} runtime log preview\n\n", app_display_name()));
    for line in lines {
        output.push_str(&line);
        output.push('\n');
    }
    fs::write(destination, output).map_err(|error| error.to_string())
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
    for line in tail.lines() {
        if line.trim().is_empty() {
            continue;
        }
        lines.push_back(line.to_string());
        if lines.len() > max_lines {
            let _ = lines.pop_front();
        }
    }
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

fn spawn_open_command(program: &str, args: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
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
}
