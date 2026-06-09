impl SettingsService {
    pub fn add_ai_provider(&self, kind: &str) -> Result<SettingsSummary, String> {
        let kind = sanitize_provider_kind(kind);
        let defaults = provider_defaults(&kind);
        let mut raw = self.raw_settings();
        let providers = ai_providers_mut(&mut raw)?;
        let priority = providers.len() as i64;
        let mut provider = Map::new();
        provider.insert(
            "id".to_string(),
            Value::String(format!(
                "api-{kind}-{}",
                current_unix_seconds().saturating_mul(1000) + priority
            )),
        );
        provider.insert("kind".to_string(), Value::String(kind));
        provider.insert(
            "displayName".to_string(),
            Value::String(defaults.0.to_string()),
        );
        provider.insert("isEnabled".to_string(), Value::Bool(true));
        provider.insert("model".to_string(), Value::String(defaults.1.to_string()));
        provider.insert("baseUrl".to_string(), Value::String(defaults.2.to_string()));
        provider.insert("apiKey".to_string(), Value::String(String::new()));
        provider.insert("useForMemoryExtraction".to_string(), Value::Bool(true));
        provider.insert("priority".to_string(), Value::Number(priority.into()));
        providers.push(Value::Object(provider));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn remove_ai_provider(&self, provider_id: &str) -> Result<SettingsSummary, String> {
        let provider_id = provider_id.trim();
        if provider_id.is_empty() {
            return Err("AI provider id is empty.".to_string());
        }
        let mut raw = self.raw_settings();
        {
            let providers = ai_providers_mut(&mut raw)?;
            let before = providers.len();
            providers.retain(|provider| {
                provider
                    .get("id")
                    .and_then(Value::as_str)
                    .map(|id| id != provider_id)
                    .unwrap_or(true)
            });
            if providers.len() == before {
                return Err("AI provider not found.".to_string());
            }
        }
        let ai = ai_mut(&mut raw)?;
        if ai
            .get("gitCommitMessageProviderId")
            .and_then(Value::as_str)
            .map(|id| id == provider_id)
            .unwrap_or(false)
        {
            ai.insert(
                "gitCommitMessageProviderId".to_string(),
                Value::String("automatic".to_string()),
            );
        }
        if let Some(memory) = ai.get_mut("memory").and_then(Value::as_object_mut) {
            if memory
                .get("defaultExtractorProviderId")
                .and_then(Value::as_str)
                .map(|id| id == provider_id)
                .unwrap_or(false)
            {
                memory.insert(
                    "defaultExtractorProviderId".to_string(),
                    Value::String("automatic".to_string()),
                );
            }
        }
        if let Some(pet) = ai.get_mut("pet").and_then(Value::as_object_mut) {
            if pet
                .get("speechProviderId")
                .and_then(Value::as_str)
                .map(|id| id == provider_id)
                .unwrap_or(false)
            {
                pet.insert(
                    "speechProviderId".to_string(),
                    Value::String("automatic".to_string()),
                );
            }
        }
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn update_ai_provider_string(
        &self,
        provider_id: &str,
        key: &str,
        value: &str,
    ) -> Result<SettingsSummary, String> {
        let key = match key {
            "kind" => "kind",
            "displayName" => "displayName",
            "model" => "model",
            "baseUrl" => "baseUrl",
            "apiKey" => "apiKey",
            _ => return Err("Unsupported AI provider field.".to_string()),
        };
        let mut raw = self.raw_settings();
        let provider = ai_provider_mut(&mut raw, provider_id)?;
        let value = if key == "kind" {
            let kind = sanitize_provider_kind(value);
            let defaults = provider_defaults(&kind);
            provider.insert(
                "displayName".to_string(),
                Value::String(defaults.0.to_string()),
            );
            provider.insert("model".to_string(), Value::String(defaults.1.to_string()));
            provider.insert("baseUrl".to_string(), Value::String(defaults.2.to_string()));
            kind
        } else {
            value.trim().chars().take(512).collect()
        };
        provider.insert(key.to_string(), Value::String(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn set_ai_provider_bool(
        &self,
        provider_id: &str,
        key: &str,
        value: bool,
    ) -> Result<SettingsSummary, String> {
        let key = match key {
            "isEnabled" => "isEnabled",
            "useForMemoryExtraction" => "useForMemoryExtraction",
            _ => return Err("Unsupported AI provider field.".to_string()),
        };
        let mut raw = self.raw_settings();
        let provider = ai_provider_mut(&mut raw, provider_id)?;
        provider.insert(key.to_string(), Value::Bool(value));
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn test_ai_provider(&self, provider_id: &str) -> Result<LLMProviderTestResult, String> {
        let provider = self.ai_provider(provider_id)?;
        block_on_llm(llm::test_provider(provider))
    }
}
