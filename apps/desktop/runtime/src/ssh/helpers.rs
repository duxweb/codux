use super::types::{SSHConnectionProfile, SSHProfileUpsertRequest};
use crate::{config::ConfigDocumentStore, runtime_paths::app_support_dir};
use chrono::Utc;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn ssh_profiles_file_path() -> PathBuf {
    ssh_profiles_file_path_in(app_support_dir())
}

pub fn ssh_profiles_file_path_in(support_dir: PathBuf) -> PathBuf {
    support_dir.join("ssh_profiles.json")
}

pub fn render_ssh_launch_context(codux_ssh_command: Option<String>) -> Option<String> {
    render_ssh_launch_context_from_support_dir(app_support_dir(), codux_ssh_command)
}

pub fn render_ssh_launch_context_from_support_dir(
    support_dir: PathBuf,
    codux_ssh_command: Option<String>,
) -> Option<String> {
    let mut profiles = sanitize_profiles(load_profiles(&ssh_profiles_file_path_in(support_dir))?);
    render_ssh_launch_context_for_profiles(&mut profiles, codux_ssh_command)
}

pub(super) fn render_ssh_launch_context_for_profiles(
    profiles: &mut Vec<SSHConnectionProfile>,
    codux_ssh_command: Option<String>,
) -> Option<String> {
    if profiles.is_empty() {
        return None;
    }
    let codux_ssh_command = codux_ssh_command
        .and_then(|value| normalized(&value))
        .unwrap_or_else(|| "codux-ssh".to_string());
    profiles.sort_by(|left, right| {
        display_name(left)
            .to_lowercase()
            .cmp(&display_name(right).to_lowercase())
    });
    let mut lines = vec![
        "Codux exposes saved SSH connections through terminal commands.".to_string(),
        format!(
            "Use `{codux_ssh_command} list` to read available saved SSH profiles as JSON; the shell wrapper path is only a command entry point, not a working directory."
        ),
        format!(
            "When a matching saved profile exists, use `{codux_ssh_command}` for that profile; do not look for or use `codux` or `dmux`, and do not use raw `ssh` unless no saved profile matches."
        ),
        format!(
            "For one-off remote command execution, use `{codux_ssh_command} <profile-id> -- '<remote-command>'`."
        ),
        format!(
            "For an interactive SSH session only when the user explicitly asks to connect/open SSH, use `{codux_ssh_command} <profile-id>`."
        ),
        "When the user asks to run a command on a saved SSH profile by name, host, or user, prefer the one-off remote command form. If no saved profile matches, ask the user to add a saved SSH profile or use the system ssh command with explicit host details.".to_string(),
        "Do not ask for, print, infer, or expose saved passwords, passphrases, or private key paths.".to_string(),
        "Available SSH profiles:".to_string(),
    ];
    lines.extend(profiles.iter().map(|profile| {
        format!(
            "- {}: id={}, endpoint={}@{}:{}, credential={}",
            display_name(profile),
            profile.id,
            profile.username,
            profile.host,
            profile.port,
            credential_label(profile)
        )
    }));
    Some(lines.join("\n"))
}

pub(super) fn load_profiles(path: &Path) -> Option<Vec<SSHConnectionProfile>> {
    ConfigDocumentStore::for_file(path.to_path_buf()).snapshot_as()
}

pub(super) fn sanitize_profiles(profiles: Vec<SSHConnectionProfile>) -> Vec<SSHConnectionProfile> {
    profiles
        .into_iter()
        .filter_map(|profile| {
            sanitize_request(SSHProfileUpsertRequest {
                id: Some(profile.id),
                name: profile.name,
                host: profile.host,
                port: profile.port,
                username: profile.username,
                credential_kind: profile.credential_kind,
                private_key_path: Some(profile.private_key_path),
                password: profile.password,
                key_passphrase: profile.key_passphrase,
            })
            .ok()
        })
        .collect()
}

pub(super) fn sanitize_request(
    request: SSHProfileUpsertRequest,
) -> Result<SSHConnectionProfile, String> {
    let host = request.host.trim().to_string();
    let username = request.username.trim().to_string();
    if host.is_empty() {
        return Err("Host cannot be empty.".to_string());
    }
    if username.is_empty() {
        return Err("Username cannot be empty.".to_string());
    }
    let credential_kind = match request.credential_kind.as_str() {
        "password" => "password",
        "privateKey" => "privateKey",
        _ => "none",
    }
    .to_string();
    let private_key_path = request
        .private_key_path
        .unwrap_or_default()
        .trim()
        .to_string();
    let password = request.password.and_then(|value| normalized(&value));
    let key_passphrase = request.key_passphrase.and_then(|value| normalized(&value));
    if credential_kind == "password" && password.is_none() {
        return Err("Password cannot be empty.".to_string());
    }
    if credential_kind == "privateKey" && private_key_path.is_empty() {
        return Err("Private key path cannot be empty.".to_string());
    }

    Ok(SSHConnectionProfile {
        id: request
            .id
            .and_then(|value| normalized(&value))
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        name: request.name.trim().to_string(),
        host,
        port: request.port.clamp(1, 65535),
        username,
        credential_kind,
        private_key_path,
        updated_at: Utc::now().timestamp(),
        password,
        key_passphrase,
    })
}

pub(super) fn display_name(profile: &SSHConnectionProfile) -> String {
    if profile.name.trim().is_empty() {
        format!("{}@{}", profile.username, profile.host)
    } else {
        profile.name.clone()
    }
}

pub(super) fn credential_label(profile: &SSHConnectionProfile) -> &'static str {
    match profile.credential_kind.as_str() {
        "password" => "password",
        "privateKey" => "privateKey",
        _ => "sshAgent",
    }
}

pub(super) fn ssh_terminal_command(profile_id: &str) -> String {
    platform_ssh_terminal_command(profile_id)
}

pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(windows)]
fn platform_ssh_terminal_command(profile_id: &str) -> String {
    format!(
        "codux-ssh {}; Write-Host ''; Write-Host '[SSH session ended]'",
        powershell_quote(profile_id)
    )
}

#[cfg(not(windows))]
fn platform_ssh_terminal_command(profile_id: &str) -> String {
    format!(
        "codux-ssh {}; printf '\\n[SSH session ended]\\n'; exec \"$SHELL\" -l",
        shell_quote(profile_id)
    )
}

#[cfg(windows)]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}
