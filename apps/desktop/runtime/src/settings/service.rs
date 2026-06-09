pub struct SettingsService {
    settings_path: PathBuf,
}

impl SettingsService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            settings_path: crate::config::settings_file_path(support_dir),
        }
    }

    pub fn summary(&self) -> SettingsSummary {
        summary_from_raw(&self.raw_settings())
    }

    fn update_string(&self, key: &str, value: String) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        raw.insert(key.to_string(), Value::String(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn update_ai_string(&self, key: &str, value: String) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let ai = ai_mut(&mut raw)?;
        ai.insert(key.to_string(), Value::String(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn toggle_pet_bool(&self, key: &str, default: bool) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let pet = pet_mut(&mut raw)?;
        let current = pet.get(key).and_then(Value::as_bool).unwrap_or(default);
        pet.insert(key.to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn update_pet_string(&self, key: &str, value: String) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let pet = pet_mut(&mut raw)?;
        pet.insert(key.to_string(), Value::String(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn update_ai_pet_string(&self, key: &str, value: String) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let pet = ai_pet_mut(&mut raw)?;
        pet.insert(key.to_string(), Value::String(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn update_runtime_tool_string(
        &self,
        key: &str,
        value: String,
    ) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let tools = ai_runtime_tools_mut(&mut raw)?;
        tools.insert(key.to_string(), Value::String(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn toggle_ai_pet_bool(&self, key: &str, default: bool) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let pet = ai_pet_mut(&mut raw)?;
        let current = pet.get(key).and_then(Value::as_bool).unwrap_or(default);
        pet.insert(key.to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    fn raw_settings(&self) -> Map<String, Value> {
        ConfigStore::for_file(self.settings_path.clone()).snapshot()
    }

    fn save_raw_settings(&self, settings: &Map<String, Value>) -> Result<(), String> {
        ConfigStore::for_file(self.settings_path.clone()).save_snapshot(settings)
    }

    pub fn ai_settings(&self) -> AISettings {
        let raw = self.raw_settings();
        raw.get("ai")
            .cloned()
            .and_then(|value| serde_json::from_value::<AISettings>(value).ok())
            .map(sanitize_ai_settings)
            .unwrap_or_default()
    }

    fn ai_provider(&self, provider_id: &str) -> Result<AIProviderSettings, String> {
        let provider_id = provider_id.trim();
        if provider_id.is_empty() {
            return Err("AI provider id is empty.".to_string());
        }
        self.ai_settings()
            .providers
            .into_iter()
            .find(|provider| provider.id == provider_id)
            .ok_or_else(|| "AI provider not found.".to_string())
    }
}
