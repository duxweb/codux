use crate::paths::app_support_dir;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHConnectionProfile {
    pub id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credential_kind: String,
    pub private_key_path: String,
    pub updated_at: i64,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub key_passphrase: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHProfileUpsertRequest {
    pub id: Option<String>,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub credential_kind: String,
    pub private_key_path: Option<String>,
    pub password: Option<String>,
    pub key_passphrase: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHProfilesSnapshot {
    pub profiles: Vec<SSHConnectionProfile>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHLaunchCommand {
    pub command: String,
    pub log_command: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SSHProfileTestResult {
    pub ok: bool,
    pub message: String,
}

pub struct SSHStore {
    profiles: Mutex<Vec<SSHConnectionProfile>>,
    state_file: PathBuf,
}

impl SSHStore {
    pub fn load_or_seed() -> Self {
        let state_file = ssh_profiles_file_path();
        let profiles = load_profiles(&state_file).unwrap_or_default();
        let store = Self {
            profiles: Mutex::new(sanitize_profiles(profiles)),
            state_file,
        };
        let _ = store.save();
        store
    }

    pub fn snapshot(&self) -> SSHProfilesSnapshot {
        let mut profiles = self
            .profiles
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        profiles.sort_by(|left, right| {
            display_name(left)
                .to_lowercase()
                .cmp(&display_name(right).to_lowercase())
        });
        SSHProfilesSnapshot { profiles }
    }

    pub fn upsert(&self, request: SSHProfileUpsertRequest) -> Result<SSHProfilesSnapshot, String> {
        let profile = sanitize_request(request)?;
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| "SSH profile store lock poisoned.".to_string())?;
        if let Some(index) = profiles.iter().position(|item| item.id == profile.id) {
            profiles[index] = profile;
        } else {
            profiles.push(profile);
        }
        drop(profiles);
        self.save()?;
        Ok(self.snapshot())
    }

    pub fn delete(&self, profile_id: String) -> Result<SSHProfilesSnapshot, String> {
        let mut profiles = self
            .profiles
            .lock()
            .map_err(|_| "SSH profile store lock poisoned.".to_string())?;
        profiles.retain(|profile| profile.id != profile_id);
        drop(profiles);
        self.save()?;
        Ok(self.snapshot())
    }

    pub fn launch_command(&self, profile_id: String) -> Result<SSHLaunchCommand, String> {
        let profiles = self
            .profiles
            .lock()
            .map_err(|_| "SSH profile store lock poisoned.".to_string())?;
        if !profiles.iter().any(|profile| profile.id == profile_id) {
            return Err("SSH connection not found.".to_string());
        }
        let command = ssh_terminal_command(&profile_id);
        let log_command = format!("codux-ssh {}", shell_quote(&profile_id));
        Ok(SSHLaunchCommand {
            command,
            log_command,
        })
    }

    pub fn test_profile(
        &self,
        request: SSHProfileUpsertRequest,
        wrapper_bin_dir: &Path,
    ) -> Result<SSHProfileTestResult, String> {
        let profile = sanitize_request(request)?;
        let wrapper = ssh_wrapper_path(wrapper_bin_dir);
        if !wrapper.exists() {
            return Err("codux-ssh wrapper is not ready.".to_string());
        }
        let profiles_file = write_test_profile_file(&profile)?;
        let output = run_ssh_test_command(&wrapper, &profile.id, &profiles_file);
        let _ = fs::remove_file(&profiles_file);
        let output = output?;
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        if output.status.success() && stdout.contains("codux-ssh-ok") {
            return Ok(SSHProfileTestResult {
                ok: true,
                message: "SSH connection test succeeded.".to_string(),
            });
        }
        let detail = stderr.trim();
        let message = if detail.is_empty() {
            format!(
                "SSH connection test failed with status {}.",
                output.status.code().unwrap_or(-1)
            )
        } else {
            detail.to_string()
        };
        Ok(SSHProfileTestResult { ok: false, message })
    }

    fn save(&self) -> Result<(), String> {
        let profiles = self
            .profiles
            .lock()
            .map_err(|_| "SSH profile store lock poisoned.".to_string())?
            .clone();
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let data = serde_json::to_vec_pretty(&profiles).map_err(|error| error.to_string())?;
        fs::write(&self.state_file, data).map_err(|error| error.to_string())
    }
}

fn write_test_profile_file(profile: &SSHConnectionProfile) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("codux-ssh-test-{}.json", Uuid::new_v4()));
    let data = serde_json::to_vec_pretty(&vec![profile]).map_err(|error| error.to_string())?;
    fs::write(&path, data).map_err(|error| error.to_string())?;
    Ok(path)
}

fn ssh_wrapper_path(wrapper_bin_dir: &Path) -> PathBuf {
    #[cfg(windows)]
    {
        wrapper_bin_dir.join("codux-ssh.cmd")
    }
    #[cfg(not(windows))]
    {
        wrapper_bin_dir.join("codux-ssh")
    }
}

fn run_ssh_test_command(
    wrapper: &Path,
    profile_id: &str,
    profiles_file: &Path,
) -> Result<Output, String> {
    let mut child = Command::new(wrapper)
        .arg(profile_id)
        .arg("--")
        .arg("echo codux-ssh-ok")
        .env("CODUX_SSH_PROFILES_FILE", profiles_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to run SSH test: {error}"))?;
    let stdout = read_child_pipe(child.stdout.take());
    let stderr = read_child_pipe(child.stderr.take());
    let start = Instant::now();

    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(Output {
                status,
                stdout: stdout.join().unwrap_or_default(),
                stderr: stderr.join().unwrap_or_default(),
            });
        }
        if start.elapsed() >= Duration::from_secs(12) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err("SSH connection test timed out.".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn read_child_pipe<T>(pipe: Option<T>) -> thread::JoinHandle<Vec<u8>>
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        let Some(mut pipe) = pipe else {
            return Vec::new();
        };
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}

pub fn ssh_profiles_file_path() -> PathBuf {
    app_support_dir().join("ssh_profiles.json")
}

pub fn render_ssh_launch_context(codux_ssh_command: Option<String>) -> Option<String> {
    let mut profiles = sanitize_profiles(load_profiles(&ssh_profiles_file_path())?);
    render_ssh_launch_context_for_profiles(&mut profiles, codux_ssh_command)
}

fn render_ssh_launch_context_for_profiles(
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

fn load_profiles(path: &Path) -> Option<Vec<SSHConnectionProfile>> {
    let data = fs::read(path).ok()?;
    if data.is_empty() {
        return None;
    }
    serde_json::from_slice(&data).ok()
}

fn sanitize_profiles(profiles: Vec<SSHConnectionProfile>) -> Vec<SSHConnectionProfile> {
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

fn sanitize_request(request: SSHProfileUpsertRequest) -> Result<SSHConnectionProfile, String> {
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

fn display_name(profile: &SSHConnectionProfile) -> String {
    if !profile.name.trim().is_empty() {
        profile.name.clone()
    } else {
        format!("{}@{}", profile.username, profile.host)
    }
}

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn credential_label(profile: &SSHConnectionProfile) -> &'static str {
    match profile.credential_kind.as_str() {
        "password" => "password",
        "privateKey" => "privateKey",
        _ => "sshAgent",
    }
}

#[cfg(windows)]
fn ssh_terminal_command(profile_id: &str) -> String {
    format!(
        "codux-ssh {}; Write-Host ''; Write-Host '[SSH session ended]'",
        powershell_quote(profile_id)
    )
}

#[cfg(not(windows))]
fn ssh_terminal_command(profile_id: &str) -> String {
    format!(
        "codux-ssh {}; printf '\\n[SSH session ended]\\n'; exec \"$SHELL\" -l",
        shell_quote(profile_id)
    )
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(windows)]
fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile_with_secret() -> SSHConnectionProfile {
        SSHConnectionProfile {
            id: "profile-1".to_string(),
            name: "Production".to_string(),
            host: "example.com".to_string(),
            port: 2222,
            username: "root".to_string(),
            credential_kind: "password".to_string(),
            private_key_path: "/Users/me/.ssh/id_ed25519".to_string(),
            updated_at: 1,
            password: Some("secret-password".to_string()),
            key_passphrase: Some("secret-passphrase".to_string()),
        }
    }

    #[test]
    fn password_profiles_require_password() {
        let result = sanitize_request(SSHProfileUpsertRequest {
            id: None,
            name: "Production".to_string(),
            host: "example.com".to_string(),
            port: 22,
            username: "root".to_string(),
            credential_kind: "password".to_string(),
            private_key_path: None,
            password: None,
            key_passphrase: None,
        });
        assert!(result.is_err());
    }

    #[test]
    fn launch_context_lists_profiles_without_secrets() {
        let mut profiles = vec![profile_with_secret()];
        let context = render_ssh_launch_context_for_profiles(&mut profiles, None).unwrap();
        assert!(context.contains("codux-ssh list"));
        assert!(context.contains("codux-ssh <profile-id>"));
        assert!(context.contains("codux-ssh <profile-id> -- '<remote-command>'"));
        assert!(context.contains("do not look for or use `codux` or `dmux`"));
        assert!(context.contains("Production"));
        assert!(context.contains("root@example.com:2222"));
        assert!(context.contains("profile-1"));
        assert!(!context.contains("secret-password"));
        assert!(!context.contains("secret-passphrase"));
        assert!(!context.contains("/Users/me/.ssh/id_ed25519"));
    }

    #[test]
    fn launch_context_can_include_absolute_wrapper_command() {
        let mut profiles = vec![profile_with_secret()];
        let context = render_ssh_launch_context_for_profiles(
            &mut profiles,
            Some("/tmp/codux/scripts/wrappers/bin/codux-ssh".to_string()),
        )
        .unwrap();
        assert!(context.contains("/tmp/codux/scripts/wrappers/bin/codux-ssh <profile-id>"));
    }

    #[test]
    fn launch_command_only_references_profile_id() {
        let profile = profile_with_secret();
        let store = SSHStore {
            profiles: Mutex::new(vec![profile]),
            state_file: PathBuf::from("/tmp/codux-ssh-test.json"),
        };
        let command = store.launch_command("profile-1".to_string()).unwrap();
        assert!(command.command.contains("codux-ssh"));
        assert!(command.command.contains("profile-1"));
        assert!(!command.command.contains("secret-password"));
    }
}
