use crate::paths::app_support_dir;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
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
        let command = format!(
            "codux-ssh {}; printf '\\n[SSH session ended]\\n'; exec \"$SHELL\" -l",
            shell_quote(&profile_id)
        );
        let log_command = format!("codux-ssh {}", shell_quote(&profile_id));
        Ok(SSHLaunchCommand {
            command,
            log_command,
        })
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

pub fn ssh_profiles_file_path() -> PathBuf {
    app_support_dir().join("ssh_profiles.json")
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
        password: request.password.and_then(|value| normalized(&value)),
        key_passphrase: request.key_passphrase.and_then(|value| normalized(&value)),
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

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}
