use crate::{
    config::ConfigStore, notification::NotificationChannelConfig, runtime_paths::app_support_dir,
};
use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

use super::{sanitize::sanitize_settings, types::AppSettings};

pub struct AppSettingsStore {
    settings: Mutex<AppSettings>,
    state_file: PathBuf,
}

impl AppSettingsStore {
    pub fn load_or_seed() -> Self {
        Self::from_settings_file(settings_file_path())
    }

    pub fn from_support_dir(support_dir: PathBuf) -> Self {
        Self::from_settings_file(crate::config::settings_file_path(support_dir))
    }

    pub fn from_settings_file(state_file: PathBuf) -> Self {
        let settings = load_settings(&state_file).unwrap_or_default();
        let store = Self {
            settings: Mutex::new(sanitize_settings(settings)),
            state_file,
        };
        let _ = store.save();
        store
    }

    pub fn snapshot(&self) -> AppSettings {
        self.settings
            .lock()
            .map(|settings| settings.clone())
            .unwrap_or_default()
    }

    pub fn reload_snapshot(&self) -> AppSettings {
        let next =
            sanitize_settings(load_settings(&self.state_file).unwrap_or_else(|| self.snapshot()));
        if let Ok(mut settings) = self.settings.lock() {
            *settings = next.clone();
        }
        next
    }

    pub fn replace(&self, next: AppSettings) -> Result<AppSettings, String> {
        let next = sanitize_settings(next);
        {
            let mut settings = self
                .settings
                .lock()
                .map_err(|_| "App settings lock poisoned.".to_string())?;
            *settings = next.clone();
        }
        self.save()?;
        Ok(next)
    }

    pub fn update(&self, apply: impl FnOnce(&mut AppSettings)) -> Result<AppSettings, String> {
        let next = {
            let mut settings = self
                .settings
                .lock()
                .map_err(|_| "App settings lock poisoned.".to_string())?;
            apply(&mut settings);
            let next = sanitize_settings((*settings).clone());
            *settings = next.clone();
            next
        };
        self.save()?;
        Ok(next)
    }

    pub fn configured_notification_channels(&self) -> Vec<NotificationChannelConfig> {
        self.snapshot()
            .notification_channels
            .into_iter()
            .filter_map(|(id, channel)| {
                let endpoint = channel.endpoint.trim().to_string();
                if !channel.enabled || endpoint.is_empty() {
                    return None;
                }
                Some(NotificationChannelConfig {
                    id,
                    endpoint,
                    token: channel.token.trim().to_string(),
                })
            })
            .collect()
    }

    fn save(&self) -> Result<(), String> {
        let settings = self.snapshot();
        let value = serde_json::to_value(settings).map_err(|error| error.to_string())?;
        let raw = value
            .as_object()
            .cloned()
            .ok_or_else(|| "App settings must be a JSON object.".to_string())?;
        ConfigStore::for_file(self.state_file.clone()).save_snapshot(&raw)
    }
}

fn load_settings(path: &Path) -> Option<AppSettings> {
    let raw = ConfigStore::for_file(path.to_path_buf()).snapshot();
    if raw.is_empty() {
        return None;
    }
    serde_json::from_value(serde_json::Value::Object(raw)).ok()
}

fn settings_file_path() -> PathBuf {
    crate::config::settings_file_path(app_support_dir())
}
