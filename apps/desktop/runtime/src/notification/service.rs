use crate::config::ConfigStore;

pub struct NotificationService {
    settings_path: PathBuf,
}

impl NotificationService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            settings_path: crate::config::settings_file_path(support_dir),
        }
    }

    pub fn summary(&self) -> NotificationSummary {
        summary_from_raw(&self.raw_settings())
    }

    pub fn toggle_channel(&self, channel_id: &str) -> Result<NotificationSummary, String> {
        let channel_id = channel_id.trim();
        if channel_id.is_empty() {
            return Err("Notification channel id is empty.".to_string());
        }
        let mut raw = self.raw_settings();
        let channels = notification_channels_mut(&mut raw)?;
        let channel = channels
            .entry(channel_id.to_string())
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| "Notification channel settings are invalid.".to_string())?;
        let current = channel
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        channel.insert("enabled".to_string(), Value::Bool(!current));
        channel
            .entry("endpoint".to_string())
            .or_insert_with(|| Value::String(String::new()));
        channel
            .entry("token".to_string())
            .or_insert_with(|| Value::String(String::new()));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn set_channel_enabled(
        &self,
        channel_id: &str,
        enabled: bool,
    ) -> Result<NotificationSummary, String> {
        let mut raw = self.raw_settings();
        let channel = notification_channel_mut(&mut raw, channel_id)?;
        channel.insert("enabled".to_string(), Value::Bool(enabled));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn update_channel_string(
        &self,
        channel_id: &str,
        key: &str,
        value: &str,
    ) -> Result<NotificationSummary, String> {
        let key = match key {
            "endpoint" => "endpoint",
            "token" => "token",
            _ => return Err("Unsupported notification channel field.".to_string()),
        };
        let mut raw = self.raw_settings();
        let channel = notification_channel_mut(&mut raw, channel_id)?;
        channel.insert(
            key.to_string(),
            Value::String(value.trim().chars().take(1_000).collect()),
        );
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn test_channel(&self, channel_id: &str) -> Result<NotificationDispatchResult, String> {
        let channel_id = channel_id.trim();
        let channel = self
            .summary()
            .channels
            .into_iter()
            .find(|channel| channel.id == channel_id)
            .ok_or_else(|| "Notification channel not found.".to_string())?;
        if channel.endpoint.trim().is_empty() {
            return Err("Notification endpoint is empty.".to_string());
        }
        let label = channel.label.clone();
        Ok(dispatch_notification_channels_blocking(
            NotificationDispatchRequest {
                channels: vec![NotificationChannelConfig {
                    id: channel.id,
                    endpoint: channel.endpoint,
                    token: channel.token,
                }],
                title: "Test".to_string(),
                body: format!("Test succeeded: {label}"),
                group: "codux-test".to_string(),
            },
        ))
    }

    fn raw_settings(&self) -> Map<String, Value> {
        ConfigStore::for_file(self.settings_path.clone()).snapshot()
    }

    fn save_raw_settings(&self, settings: &Map<String, Value>) -> Result<(), String> {
        ConfigStore::for_file(self.settings_path.clone()).save_snapshot(settings)
    }
}
