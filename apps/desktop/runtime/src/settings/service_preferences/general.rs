impl SettingsService {
    pub fn set_language(&self, language: &str) -> Result<SettingsSummary, String> {
        let value = match language.trim() {
            "zh-Hans" | "zh-CN" | "simplifiedChinese" => "simplifiedChinese",
            "zh-Hant" | "zh-TW" | "traditionalChinese" => "traditionalChinese",
            "en" | "english" => "english",
            "ja" | "japanese" => "japanese",
            "ko" | "korean" => "korean",
            "fr" | "french" => "french",
            "de" | "german" => "german",
            "es" | "spanish" => "spanish",
            "pt-BR" | "portugueseBrazil" => "portugueseBrazil",
            "ru" | "russian" => "russian",
            _ => "system",
        };
        self.update_string("language", value.to_string())
    }

    pub fn cycle_language(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().language;
        let next = match current.as_str() {
            "system" => "english",
            "english" => "simplifiedChinese",
            "simplifiedChinese" => "traditionalChinese",
            "traditionalChinese" => "japanese",
            "japanese" => "korean",
            "korean" => "french",
            "french" => "german",
            "german" => "spanish",
            "spanish" => "portugueseBrazil",
            "portugueseBrazil" => "russian",
            "russian" => "system",
            _ => "system",
        };
        self.update_string("language", next.to_string())
    }

    pub fn toggle_dock_badge(&self) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let current = raw
            .get("showsDockBadge")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        raw.insert("showsDockBadge".to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn toggle_developer_hud(&self) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let current = raw
            .get("developerHud")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        raw.insert("developerHud".to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn toggle_memory_enabled(&self) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let memory = ai_memory_mut(&mut raw)?;
        let current = memory
            .get("enabled")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        memory.insert("enabled".to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn set_sleep_mode(&self, mode: &str) -> Result<SettingsSummary, String> {
        self.update_string(
            "sleepMode",
            crate::power::normalize_sleep_mode(mode).to_string(),
        )
    }

    pub fn set_statistics_mode(&self, mode: &str) -> Result<SettingsSummary, String> {
        self.update_string("statisticsMode", sanitize_statistics_mode(mode))
    }

    pub fn set_file_open_default(&self, mode: &str) -> Result<SettingsSummary, String> {
        self.update_string("fileOpenDefault", sanitize_file_open_default(mode))
    }

    pub fn cycle_statistics_mode(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().statistics_mode;
        let next = match current.as_str() {
            "normalized" => "includingCache",
            "includingCache" => "normalized",
            _ => "normalized",
        };
        self.update_string("statisticsMode", next.to_string())
    }
}
