use crate::ai_runtime::{AIRuntimeBridgeSnapshot, AIRuntimeStateSnapshot};
use crate::app_settings::AppSettings;
use crate::paths::{app_support_dir, runtime_temp_dir};
use crate::performance::PerformanceSnapshot;
use crate::project_store::ProjectListSnapshot;
use crate::ssh::SSHProfilesSnapshot;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    pub configured: bool,
    pub checking: bool,
    pub available: bool,
    pub automatic_install_supported: bool,
    pub signed_updater_configured: bool,
    pub manifest_endpoint_configured: bool,
    pub current_version: String,
    pub latest_version: Option<String>,
    pub download_url: Option<String>,
    pub channel: Option<String>,
    pub installation_mode: String,
    pub message: String,
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

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct UpdateManifest {
    #[serde(alias = "version")]
    latest_version: Option<String>,
    #[serde(default, alias = "downloadUrl", alias = "url")]
    download_url: Option<String>,
    #[serde(default)]
    notes: Option<String>,
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

pub fn about_metadata(app: &AppHandle) -> AppAboutMetadata {
    let package = app.package_info();
    let config = app.config();
    AppAboutMetadata {
        name: config
            .product_name
            .clone()
            .unwrap_or_else(|| package.name.clone()),
        version: config
            .version
            .clone()
            .unwrap_or_else(|| package.version.to_string()),
        identifier: config.identifier.clone(),
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

pub async fn update_status(app: &AppHandle, settings: &AppSettings) -> UpdateStatus {
    let about = about_metadata(app);
    let plugins = &app.config().plugins.0;
    let updater = plugins.get("updater");
    let endpoints = updater
        .and_then(|value| value.get("endpoints"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .count()
        })
        .unwrap_or(0);
    let pubkey = updater
        .and_then(|value| value.get("pubkey"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let endpoint = settings.update.endpoint.trim();
    let signed_updater_configured = settings.update.enabled && endpoints > 0 && pubkey.is_some();
    let endpoint_configured = settings.update.enabled && !endpoint.is_empty();

    if endpoint_configured {
        if signed_updater_configured {
            match check_signed_update(app, endpoint).await {
                Ok(status) => return status,
                Err(error) => {
                    return update_error(
                        about.version,
                        Some(settings.update.channel.clone())
                            .filter(|value| !value.trim().is_empty()),
                        signed_updater_configured,
                        endpoint_configured,
                        error,
                    );
                }
            }
        }
        return check_update_endpoint(
            about.version,
            Some(settings.update.channel.clone()).filter(|value| !value.trim().is_empty()),
            endpoint,
            signed_updater_configured,
            endpoint_configured,
        )
        .await;
    }

    UpdateStatus {
        configured: signed_updater_configured,
        checking: false,
        available: false,
        automatic_install_supported: false,
        signed_updater_configured,
        manifest_endpoint_configured: endpoint_configured,
        current_version: about.version,
        latest_version: None,
        download_url: None,
        channel: Some(settings.update.channel.clone())
            .filter(|value| !value.trim().is_empty())
            .or_else(|| {
                updater
                    .and_then(|value| value.get("channel"))
                    .and_then(Value::as_str)
                    .map(str::to_string)
            }),
        installation_mode: if signed_updater_configured {
            "signedConfig".to_string()
        } else if settings.update.enabled {
            "notConfigured".to_string()
        } else {
            "disabled".to_string()
        },
        message: if signed_updater_configured {
            "Signed updater configuration is present, but automatic installation is not enabled in this build.".to_string()
        } else if settings.update.enabled {
            "Update settings are enabled, but no update endpoint is configured. Add a manifest endpoint or configure the signed Tauri updater endpoint/public key for this build.".to_string()
        } else {
            "Update channel is not configured. Configure Tauri updater endpoints and signing public key before enabling update checks.".to_string()
        },
    }
}

pub async fn install_update(
    app: &AppHandle,
    settings: &AppSettings,
) -> Result<UpdateInstallResult, String> {
    if !settings.update.enabled {
        return Err("Update checks are disabled.".to_string());
    }
    let endpoint = settings.update.endpoint.trim();
    if endpoint.is_empty() {
        return Err("Update endpoint is not configured.".to_string());
    }
    let signed = signed_updater_configured(app, settings);
    if !signed {
        return Err("Automatic installation requires signed Tauri updater endpoints and a public key configured in tauri.conf.json.".to_string());
    }
    let endpoint_url = endpoint
        .parse::<Url>()
        .map_err(|error| format!("Invalid update endpoint: {error}"))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint_url])
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())?;
    let Some(update) = updater.check().await.map_err(|error| error.to_string())? else {
        return Ok(UpdateInstallResult {
            installed: false,
            version: None,
            downloaded_bytes: 0,
            total_bytes: None,
            message: "Current version is up to date.".to_string(),
        });
    };
    let version = update.version.clone();
    let downloaded = std::sync::Arc::new(std::sync::Mutex::new((0_u64, None)));
    let progress = std::sync::Arc::clone(&downloaded);
    update
        .download_and_install(
            move |chunk_length, content_length| {
                if let Ok(mut state) = progress.lock() {
                    state.0 = state.0.saturating_add(chunk_length as u64);
                    state.1 = content_length;
                }
            },
            || {},
        )
        .await
        .map_err(|error| error.to_string())?;
    let (downloaded_bytes, total_bytes) = downloaded.lock().map(|state| *state).unwrap_or_default();
    Ok(UpdateInstallResult {
        installed: true,
        version: Some(version),
        downloaded_bytes,
        total_bytes,
        message: "Update was downloaded and installed. Restart Codux to finish applying it."
            .to_string(),
    })
}

async fn check_signed_update(app: &AppHandle, endpoint: &str) -> Result<UpdateStatus, String> {
    let about = about_metadata(app);
    let endpoint_url = endpoint
        .parse::<Url>()
        .map_err(|error| format!("Invalid update endpoint: {error}"))?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint_url])
        .map_err(|error| error.to_string())?
        .build()
        .map_err(|error| error.to_string())?;
    let update = updater.check().await.map_err(|error| error.to_string())?;
    if let Some(update) = update {
        return Ok(UpdateStatus {
            configured: true,
            checking: false,
            available: true,
            automatic_install_supported: true,
            signed_updater_configured: true,
            manifest_endpoint_configured: true,
            current_version: about.version,
            latest_version: Some(update.version),
            download_url: Some(update.download_url.to_string()),
            channel: None,
            installation_mode: "automatic".to_string(),
            message: "A signed update is available and can be installed automatically.".to_string(),
        });
    }
    Ok(UpdateStatus {
        configured: true,
        checking: false,
        available: false,
        automatic_install_supported: true,
        signed_updater_configured: true,
        manifest_endpoint_configured: true,
        current_version: about.version.clone(),
        latest_version: Some(about.version.clone()),
        download_url: None,
        channel: None,
        installation_mode: "automatic".to_string(),
        message: format!("Current version {} is up to date.", about.version),
    })
}

fn signed_updater_configured(app: &AppHandle, settings: &AppSettings) -> bool {
    if !settings.update.enabled {
        return false;
    }
    let plugins = &app.config().plugins.0;
    let updater = plugins.get("updater");
    let endpoints = updater
        .and_then(|value| value.get("endpoints"))
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(Value::as_str)
                .filter(|item| !item.trim().is_empty())
                .count()
        })
        .unwrap_or(0);
    let pubkey = updater
        .and_then(|value| value.get("pubkey"))
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    endpoints > 0 && pubkey.is_some()
}

async fn check_update_endpoint(
    current_version: String,
    channel: Option<String>,
    endpoint: &str,
    signed_updater_configured: bool,
    manifest_endpoint_configured: bool,
) -> UpdateStatus {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(10))
        .build()
    {
        Ok(client) => client,
        Err(error) => {
            return update_error(
                current_version,
                channel,
                signed_updater_configured,
                manifest_endpoint_configured,
                error.to_string(),
            );
        }
    };
    let manifest = match client
        .get(endpoint)
        .header(reqwest::header::ACCEPT, "application/json")
        .send()
        .await
        .and_then(|response| response.error_for_status())
    {
        Ok(response) => match response.json::<UpdateManifest>().await {
            Ok(manifest) => manifest,
            Err(error) => {
                return update_error(
                    current_version,
                    channel,
                    signed_updater_configured,
                    manifest_endpoint_configured,
                    error.to_string(),
                );
            }
        },
        Err(error) => {
            return update_error(
                current_version,
                channel,
                signed_updater_configured,
                manifest_endpoint_configured,
                error.to_string(),
            );
        }
    };
    let latest = manifest
        .latest_version
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let available = latest
        .as_deref()
        .is_some_and(|version| version_is_newer(version, &current_version));
    let latest_text = latest.clone().unwrap_or_else(|| current_version.clone());
    let message = if available {
        format!(
            "A new version {latest_text} is available. Automatic installation requires signed Tauri updater packaging; open the download URL to update manually."
        )
    } else {
        format!("Current version {current_version} is up to date.")
    };
    UpdateStatus {
        configured: true,
        checking: false,
        available,
        automatic_install_supported: false,
        signed_updater_configured,
        manifest_endpoint_configured,
        current_version,
        latest_version: latest,
        download_url: manifest
            .download_url
            .or(manifest.notes)
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty()),
        channel,
        installation_mode: "manifest".to_string(),
        message,
    }
}

fn update_error(
    current_version: String,
    channel: Option<String>,
    signed_updater_configured: bool,
    manifest_endpoint_configured: bool,
    error: String,
) -> UpdateStatus {
    UpdateStatus {
        configured: true,
        checking: false,
        available: false,
        automatic_install_supported: false,
        signed_updater_configured,
        manifest_endpoint_configured,
        current_version,
        latest_version: None,
        download_url: None,
        channel,
        installation_mode: if manifest_endpoint_configured {
            "manifest".to_string()
        } else if signed_updater_configured {
            "signedConfig".to_string()
        } else {
            "notConfigured".to_string()
        },
        message: format!("Unable to check updates: {error}"),
    }
}

fn version_is_newer(latest: &str, current: &str) -> bool {
    let latest_parts = semantic_version_parts(latest);
    let current_parts = semantic_version_parts(current);
    latest_parts > current_parts
}

fn semantic_version_parts(value: &str) -> Vec<u64> {
    value
        .trim()
        .trim_start_matches('v')
        .split(|ch: char| !ch.is_ascii_digit())
        .filter(|part| !part.is_empty())
        .map(|part| part.parse::<u64>().unwrap_or(0))
        .collect()
}

pub fn export_diagnostics(
    app: &AppHandle,
    request: DiagnosticsExportRequest,
    settings: AppSettings,
    projects: ProjectListSnapshot,
    ai_runtime: AIRuntimeBridgeSnapshot,
    ai_state: AIRuntimeStateSnapshot,
    performance: PerformanceSnapshot,
    ssh: SSHProfilesSnapshot,
) -> Result<DiagnosticsExportResult, String> {
    let destination = normalize_destination(&request.destination_path)?;
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }

    let report = json!({
        "generatedAt": Utc::now().to_rfc3339(),
        "app": about_metadata(app),
        "update": update_status_snapshot(app, &settings),
        "paths": {
            "appSupport": app_support_dir().display().to_string(),
            "runtimeTemp": runtime_temp_dir().display().to_string(),
            "runtimeLog": runtime_log_path().display().to_string(),
            "liveLog": live_log_path().display().to_string()
        },
        "settings": redact_settings(settings),
        "projects": projects,
        "aiRuntime": ai_runtime,
        "aiState": ai_state,
        "performance": performance,
        "ssh": redact_ssh(ssh),
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

pub fn open_runtime_log() -> Result<(), String> {
    open_or_create_text_file(
        &runtime_log_path(),
        "Codux runtime log\nThe runtime has not written log entries yet.\n",
    )
}

pub fn open_live_log() -> Result<(), String> {
    open_or_create_text_file(
        &live_log_path(),
        "Codux live log\nAI hook and polling activity is handled by the Rust runtime.\n",
    )
}

pub fn open_url(url: &str) -> Result<(), String> {
    let url = url.trim();
    if !url.starts_with("https://") && !url.starts_with("http://") {
        return Err("Only http and https URLs can be opened.".to_string());
    }
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|error| error.to_string())
}

fn open_or_create_text_file(path: &Path, initial_content: &str) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    if !path.exists() {
        fs::write(path, initial_content).map_err(|error| error.to_string())?;
    }
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
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

fn redact_settings(mut settings: AppSettings) -> AppSettings {
    for channel in settings.notification_channels.values_mut() {
        if !channel.token.trim().is_empty() {
            channel.token = "******".to_string();
        }
    }
    settings
}

fn update_status_snapshot(app: &AppHandle, settings: &AppSettings) -> UpdateStatus {
    let about = about_metadata(app);
    let manifest_endpoint_configured =
        settings.update.enabled && !settings.update.endpoint.trim().is_empty();
    UpdateStatus {
        configured: manifest_endpoint_configured,
        checking: false,
        available: false,
        automatic_install_supported: false,
        signed_updater_configured: false,
        manifest_endpoint_configured,
        current_version: about.version,
        latest_version: None,
        download_url: None,
        channel: Some(settings.update.channel.clone()).filter(|value| !value.trim().is_empty()),
        installation_mode: if manifest_endpoint_configured {
            "manifest".to_string()
        } else if settings.update.enabled {
            "notConfigured".to_string()
        } else {
            "disabled".to_string()
        },
        message: if settings.update.enabled {
            "Diagnostics snapshot does not perform network update checks.".to_string()
        } else {
            "Update checks are disabled.".to_string()
        },
    }
}

fn redact_ssh(mut snapshot: SSHProfilesSnapshot) -> SSHProfilesSnapshot {
    for profile in &mut snapshot.profiles {
        if profile.password.is_some() {
            profile.password = Some("******".to_string());
        }
        if profile.key_passphrase.is_some() {
            profile.key_passphrase = Some("******".to_string());
        }
    }
    snapshot
}

fn runtime_log_path() -> PathBuf {
    app_support_dir().join("runtime.log")
}

fn live_log_path() -> PathBuf {
    runtime_temp_dir().join("live.log")
}
