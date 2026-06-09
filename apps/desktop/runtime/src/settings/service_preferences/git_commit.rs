impl SettingsService {
    pub fn set_git_commit_tone(&self, tone: &str) -> Result<SettingsSummary, String> {
        self.update_ai_string("gitCommitMessageTone", sanitize_git_commit_tone(tone))
    }

    pub fn set_git_commit_language(&self, language: &str) -> Result<SettingsSummary, String> {
        self.update_ai_string(
            "gitCommitMessageLanguage",
            sanitize_git_commit_language(language),
        )
    }

    pub fn set_ai_global_prompt(&self, prompt: &str) -> Result<SettingsSummary, String> {
        self.update_ai_string("globalPrompt", prompt.chars().take(20_000).collect())
    }

    pub fn set_git_commit_style_rules(&self, rules: &str) -> Result<SettingsSummary, String> {
        self.update_ai_string(
            "gitCommitMessageStyleRules",
            rules.chars().take(4_000).collect(),
        )
    }

    pub fn toggle_ai_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        let provider_id = provider_id.trim();
        if provider_id.is_empty() {
            return Err("AI provider id is empty.".to_string());
        }
        let mut raw = self.raw_settings();
        let providers = ai_providers_mut(&mut raw)?;
        let Some(provider) = providers.iter_mut().find(|provider| {
            provider
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id == provider_id)
                .unwrap_or(false)
        }) else {
            return Err("AI provider not found.".to_string());
        };
        let Some(provider) = provider.as_object_mut() else {
            return Err("AI provider record is invalid.".to_string());
        };
        let current = provider
            .get("isEnabled")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        provider.insert("isEnabled".to_string(), Value::Bool(!current));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn set_git_commit_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        let provider_id = provider_id.trim();
        if provider_id.is_empty() {
            return Err("AI provider id is empty.".to_string());
        }
        if provider_id == "automatic" || provider_id == "off" {
            return self.update_ai_string("gitCommitMessageProviderId", provider_id.to_string());
        }
        let mut raw = self.raw_settings();
        let providers = ai_providers_mut(&mut raw)?;
        let found = providers.iter().any(|provider| {
            provider
                .get("id")
                .and_then(Value::as_str)
                .map(|id| id == provider_id)
                .unwrap_or(false)
        });
        if !found {
            return Err("AI provider not found.".to_string());
        }
        let ai = ai_mut(&mut raw)?;
        ai.insert(
            "gitCommitMessageProviderId".to_string(),
            Value::String(provider_id.to_string()),
        );
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn cycle_git_commit_tone(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().git_commit_tone;
        let next = match current.as_str() {
            "conventional" => "concise",
            "concise" => "sentence",
            "sentence" => "changelog",
            "changelog" => "conventional",
            _ => "conventional",
        };
        self.update_ai_string("gitCommitMessageTone", next.to_string())
    }

    pub fn cycle_git_commit_language(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().git_commit_language;
        let next = match current.as_str() {
            "application" => "english",
            "english" => "simplifiedChinese",
            "simplifiedChinese" => "traditionalChinese",
            "traditionalChinese" => "japanese",
            "japanese" => "korean",
            "korean" => "french",
            "french" => "german",
            "german" => "spanish",
            "spanish" => "portugueseBrazil",
            "portugueseBrazil" => "russian",
            "russian" => "application",
            _ => "application",
        };
        self.update_ai_string("gitCommitMessageLanguage", next.to_string())
    }
}
