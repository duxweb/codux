use crate::runtime_paths::app_support_dir;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::Mutex,
};

mod helpers;
mod test_command;
#[cfg(test)]
mod tests;
mod types;

use helpers::{
    credential_label, display_name, load_profiles, sanitize_profiles, sanitize_request,
    shell_quote, ssh_profiles_file_path_in, ssh_terminal_command,
};
pub use helpers::{
    render_ssh_launch_context, render_ssh_launch_context_from_support_dir, ssh_profiles_file_path,
};
use test_command::{
    profile_test_result, run_ssh_test_command, ssh_wrapper_path, write_test_profile_file,
};
pub use types::*;

pub struct SSHStore {
    profiles: Mutex<Vec<SSHConnectionProfile>>,
    state_file: PathBuf,
}

impl SSHStore {
    pub fn load_or_seed() -> Self {
        Self::from_support_dir(app_support_dir())
    }

    pub fn from_support_dir(support_dir: PathBuf) -> Self {
        let state_file = ssh_profiles_file_path_in(support_dir);
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
        output.map(profile_test_result)
    }

    fn save(&self) -> Result<(), String> {
        let profiles = self
            .profiles
            .lock()
            .map_err(|_| "SSH profile store lock poisoned.".to_string())?
            .clone();
        crate::config::ConfigDocumentStore::for_file(self.state_file.clone())
            .save_snapshot(&profiles)
    }
}

pub struct SSHService {
    profiles_path: PathBuf,
    wrapper_path: PathBuf,
}

impl SSHService {
    pub fn new(support_dir: PathBuf, runtime_assets: PathBuf) -> Self {
        Self {
            profiles_path: support_dir.join("ssh_profiles.json"),
            wrapper_path: ssh_wrapper_path(runtime_assets),
        }
    }

    pub fn summary(&self) -> SSHSummary {
        let wrapper_available = self.wrapper_path.is_file();
        let Some(profiles): Option<Vec<SSHConnectionProfile>> =
            crate::config::ConfigDocumentStore::for_file(self.profiles_path.clone()).snapshot_as()
        else {
            return SSHSummary {
                wrapper_available,
                error: Some("Unable to load ssh_profiles.json".to_string()),
                ..Default::default()
            };
        };
        let mut profiles = profiles
            .into_iter()
            .map(|profile| SSHProfileSummary {
                name: display_name(&profile),
                endpoint: format!("{}@{}:{}", profile.username, profile.host, profile.port),
                credential_kind: credential_label(&profile).to_string(),
                id: profile.id,
                updated_at: profile.updated_at,
            })
            .collect::<Vec<_>>();
        profiles.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
        SSHSummary {
            profiles,
            wrapper_available,
            error: None,
        }
    }
}
