impl RuntimeService {
    pub fn complete_llm(
        &self,
        request: LLMCompletionRequest,
    ) -> Result<LLMCompletionResponse, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        crate::async_runtime::block_on(llm::complete_with_settings(&settings, request))
    }

    pub fn generate_project_git_commit_message(
        &self,
        project_path: &str,
    ) -> Result<String, String> {
        let context = git::GitService::commit_message_context(project_path);
        if !context.is_repository {
            return Err(context
                .error
                .unwrap_or_else(|| "Selected project is not a Git repository.".to_string()));
        }
        if let Some(error) = context.error {
            return Err(error);
        }
        if context.diff.trim().is_empty() {
            return Err("No staged diff is available for commit message generation.".to_string());
        }

        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        if settings.git_commit_message_provider_id == "off" {
            return Err("Git commit AI provider is disabled.".to_string());
        }
        let prompt = git_commit_message_prompt(
            &context.diff,
            context.truncated,
            &settings.git_commit_message_tone,
            &settings.git_commit_message_language,
            &settings.git_commit_message_style_rules,
        );
        let response = crate::async_runtime::block_on(llm::complete_with_settings(
            &settings,
            LLMCompletionRequest {
                provider_id: Some(settings.git_commit_message_provider_id.clone()),
                prompt,
                system_prompt: Some(
                    "You write concise Git commit messages. Return only one commit subject line."
                        .to_string(),
                ),
                purpose: "gitCommitMessage".to_string(),
            },
        ))?;
        let message = response
            .text
            .lines()
            .next()
            .unwrap_or("")
            .trim()
            .trim_matches(|ch| matches!(ch, '"' | '\'' | '“' | '”' | '‘' | '’'))
            .chars()
            .take(120)
            .collect::<String>();
        if message.is_empty() {
            Err("AI provider returned an empty commit message.".to_string())
        } else {
            Ok(message)
        }
    }
}

fn git_commit_message_prompt(
    diff: &str,
    truncated: bool,
    tone: &str,
    language: &str,
    style_rules: &str,
) -> String {
    let language = match language {
        "zh-Hans" | "simplifiedChinese" => "Simplified Chinese",
        "zh-Hant" | "traditionalChinese" => "Traditional Chinese",
        "ja" | "japanese" => "Japanese",
        "ko" | "korean" => "Korean",
        "en" | "english" => "English",
        _ => "English",
    };
    let tone = match tone {
        "imperative" => "imperative mood",
        "conventional" => "Conventional Commits style when obvious",
        "concise" => "very concise",
        _ => "clear and concise",
    };
    let style_rules = style_rules.trim();
    let style_rules = if style_rules.is_empty() {
        String::new()
    } else {
        format!("\nProject style rules:\n{style_rules}\n")
    };
    format!(
        "Generate one Git commit subject line in {language}. Use {tone}. Keep it under 72 characters. Do not include markdown, bullets, quotes, explanation, or a trailing period.{style_rules}\nDiff{}:\n{}",
        if truncated { " (truncated)" } else { "" },
        diff
    )
}
