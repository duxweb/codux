impl SettingsService {
    pub fn toggle_update_enabled(&self) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let update = update_mut(&mut raw)?;
        let current = update
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        update.insert("enabled".to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn cycle_update_channel(&self) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let update = update_mut(&mut raw)?;
        let current = update
            .get("channel")
            .and_then(Value::as_str)
            .unwrap_or("stable");
        let next = match current {
            "stable" => "beta",
            "beta" => "stable",
            _ => "stable",
        };
        update.insert("channel".to_string(), Value::String(next.to_string()));
        sync_update_endpoint_for_channel(update, next);
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn set_update_channel(&self, channel: &str) -> Result<SettingsSummary, String> {
        let channel = if channel.trim() == "beta" {
            "beta"
        } else {
            "stable"
        };
        let mut raw = self.raw_settings();
        let update = update_mut(&mut raw)?;
        update.insert("channel".to_string(), Value::String(channel.to_string()));
        sync_update_endpoint_for_channel(update, channel);
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn cycle_git_refresh(&self) -> Result<SettingsSummary, String> {
        let next = next_interval(&self.summary().git_refresh, &[30, 60, 120, 300, 600], 60);
        self.update_string("gitRefresh", next.to_string())
    }

    pub fn set_git_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        let seconds = numeric_string(seconds, 60, 1, 86_400).to_string();
        self.update_string("gitRefresh", seconds)
    }

    pub fn cycle_ai_refresh(&self) -> Result<SettingsSummary, String> {
        let next = next_interval(&self.summary().ai_refresh, &[60, 120, 180, 300, 600], 180);
        self.update_string("aiRefresh", next.to_string())
    }

    pub fn set_ai_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        let seconds = numeric_string(seconds, 180, 1, 86_400).to_string();
        self.update_string("aiRefresh", seconds)
    }

    pub fn cycle_ai_background_refresh(&self) -> Result<SettingsSummary, String> {
        let next = next_interval(
            &self.summary().ai_background_refresh,
            &[300, 600, 900, 1800],
            600,
        );
        self.update_string("aiBackgroundRefresh", next.to_string())
    }

    pub fn set_ai_background_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        let seconds = numeric_string(seconds, 600, 1, 86_400).to_string();
        self.update_string("aiBackgroundRefresh", seconds)
    }

    pub fn cycle_developer_refresh(&self) -> Result<SettingsSummary, String> {
        let next = next_interval(&self.summary().developer_refresh, &[1, 2, 3, 5, 10], 3);
        self.update_string("developerRefresh", next.to_string())
    }

    pub fn set_developer_refresh(&self, seconds: &str) -> Result<SettingsSummary, String> {
        let seconds = numeric_string(seconds, 3, 1, 86_400).to_string();
        self.update_string("developerRefresh", seconds)
    }
}

fn sync_update_endpoint_for_channel(update: &mut serde_json::Map<String, Value>, channel: &str) {
    let endpoint = update
        .get("endpoint")
        .and_then(Value::as_str)
        .unwrap_or("")
        .trim();
    if endpoint.is_empty() || is_managed_update_endpoint(endpoint) {
        update.insert(
            "endpoint".to_string(),
            Value::String(update_endpoint_for_channel(channel).to_string()),
        );
    }
}

fn update_endpoint_for_channel(channel: &str) -> &'static str {
    match channel {
        "beta" => "https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json",
        _ => "https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json",
    }
}

fn is_managed_update_endpoint(endpoint: &str) -> bool {
    matches!(
        endpoint,
        "https://github.com/duxweb/codux/releases/latest/download/latest.json"
            | "https://github.com/duxweb/codux/releases/download/beta/latest.json"
            | "https://raw.githubusercontent.com/duxweb/codux/main/updates/stable/latest.json"
            | "https://raw.githubusercontent.com/duxweb/codux/main/updates/beta/latest.json"
    )
}
