use super::RemoteService;
use super::types::RemoteSettings;
use crate::config::ConfigStore;
use serde_json::{Map, Value};

impl RemoteService {
    pub(super) fn raw_settings(&self) -> Map<String, Value> {
        ConfigStore::for_file(self.settings_path.clone()).snapshot()
    }

    pub(super) fn save_raw_settings(&self, settings: &Map<String, Value>) -> Result<(), String> {
        ConfigStore::for_file(self.settings_path.clone()).save_snapshot(settings)
    }
}

pub(crate) fn remote_settings_mut(
    raw: &mut Map<String, Value>,
) -> Result<&mut Map<String, Value>, String> {
    raw.entry("remote".to_string())
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .ok_or_else(|| "Remote settings are invalid.".to_string())
}

pub(crate) fn remote_settings_from_raw(raw: &Map<String, Value>) -> RemoteSettings {
    raw.get("remote")
        .cloned()
        .and_then(|remote| serde_json::from_value::<RemoteSettings>(remote).ok())
        .unwrap_or_default()
}
