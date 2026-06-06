use crate::config::ConfigStore;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{fs, path::PathBuf};

#[derive(Clone, Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolPermissionsSummary {
    pub available: bool,
    pub path: String,
    pub codex: String,
    pub claude_code: String,
    pub gemini: String,
    pub opencode: String,
    pub kiro: String,
    pub codewhale: String,
    pub codex_model: String,
    pub claude_code_model: String,
    pub gemini_model: String,
    pub opencode_model: String,
    pub kiro_model: String,
    pub codewhale_model: String,
    pub codex_effort: String,
    pub full_access_count: usize,
    pub error: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AIRuntimeToolSettings {
    #[serde(default = "default_permission_mode")]
    codex: String,
    #[serde(default = "default_permission_mode")]
    claude_code: String,
    #[serde(default = "default_permission_mode")]
    gemini: String,
    #[serde(default = "default_permission_mode")]
    opencode: String,
    #[serde(default = "default_permission_mode")]
    kiro: String,
    #[serde(default = "default_permission_mode")]
    codewhale: String,
    #[serde(default)]
    codex_model: String,
    #[serde(default)]
    claude_code_model: String,
    #[serde(default)]
    gemini_model: String,
    #[serde(default)]
    opencode_model: String,
    #[serde(default)]
    kiro_model: String,
    #[serde(default)]
    codewhale_model: String,
    #[serde(default = "default_codex_effort")]
    codex_effort: String,
}

impl Default for AIRuntimeToolSettings {
    fn default() -> Self {
        Self {
            codex: default_permission_mode(),
            claude_code: default_permission_mode(),
            gemini: default_permission_mode(),
            opencode: default_permission_mode(),
            kiro: default_permission_mode(),
            codewhale: default_permission_mode(),
            codex_model: String::new(),
            claude_code_model: String::new(),
            gemini_model: String::new(),
            opencode_model: String::new(),
            kiro_model: String::new(),
            codewhale_model: String::new(),
            codex_effort: default_codex_effort(),
        }
    }
}

pub struct ToolPermissionsService {
    settings_path: PathBuf,
    output_path: PathBuf,
}

impl ToolPermissionsService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            settings_path: crate::config::settings_file_path(support_dir),
            output_path: runtime_temp_dir().join("tool-permissions.json"),
        }
    }

    pub fn summary(&self) -> ToolPermissionsSummary {
        match self.load_settings() {
            Ok(settings) => summary_from_settings(settings, &self.output_path, None),
            Err(error) => ToolPermissionsSummary {
                path: self.output_path.display().to_string(),
                error: Some(error),
                ..Default::default()
            },
        }
    }

    pub fn sync(&self) -> ToolPermissionsSummary {
        match self.load_settings() {
            Ok(settings) => {
                let result = self.write_settings(&settings).err();
                summary_from_settings(settings, &self.output_path, result)
            }
            Err(error) => ToolPermissionsSummary {
                path: self.output_path.display().to_string(),
                error: Some(error),
                ..Default::default()
            },
        }
    }

    fn load_settings(&self) -> Result<AIRuntimeToolSettings, String> {
        let settings = ConfigStore::for_file(self.settings_path.clone())
            .get_path(&["ai", "runtimeTools"])
            .unwrap_or(Value::Object(Default::default()));
        let settings = serde_json::from_value::<AIRuntimeToolSettings>(settings)
            .map_err(|error| error.to_string())?;
        Ok(sanitize_runtime_tool_settings(settings))
    }

    fn write_settings(&self, settings: &AIRuntimeToolSettings) -> Result<(), String> {
        if let Some(parent) = self.output_path.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let content = serde_json::to_string(settings).map_err(|error| error.to_string())?;
        fs::write(&self.output_path, content).map_err(|error| error.to_string())
    }
}

fn summary_from_settings(
    settings: AIRuntimeToolSettings,
    output_path: &std::path::Path,
    error: Option<String>,
) -> ToolPermissionsSummary {
    let full_access_count = [
        &settings.codex,
        &settings.claude_code,
        &settings.gemini,
        &settings.opencode,
        &settings.kiro,
        &settings.codewhale,
    ]
    .into_iter()
    .filter(|mode| mode.as_str() == "fullAccess")
    .count();
    ToolPermissionsSummary {
        available: error.is_none(),
        path: output_path.display().to_string(),
        codex: settings.codex,
        claude_code: settings.claude_code,
        gemini: settings.gemini,
        opencode: settings.opencode,
        kiro: settings.kiro,
        codewhale: settings.codewhale,
        codex_model: settings.codex_model,
        claude_code_model: settings.claude_code_model,
        gemini_model: settings.gemini_model,
        opencode_model: settings.opencode_model,
        kiro_model: settings.kiro_model,
        codewhale_model: settings.codewhale_model,
        codex_effort: settings.codex_effort,
        full_access_count,
        error,
    }
}

fn sanitize_runtime_tool_settings(mut settings: AIRuntimeToolSettings) -> AIRuntimeToolSettings {
    settings.codex = sanitize_permission_mode(&settings.codex);
    settings.claude_code = sanitize_permission_mode(&settings.claude_code);
    settings.gemini = sanitize_permission_mode(&settings.gemini);
    settings.opencode = sanitize_permission_mode(&settings.opencode);
    settings.kiro = sanitize_permission_mode(&settings.kiro);
    settings.codewhale = sanitize_permission_mode(&settings.codewhale);
    settings.codex_model = sanitize_model(&settings.codex_model);
    settings.claude_code_model = sanitize_model(&settings.claude_code_model);
    settings.gemini_model = sanitize_model(&settings.gemini_model);
    settings.opencode_model = sanitize_model(&settings.opencode_model);
    settings.kiro_model = sanitize_model(&settings.kiro_model);
    settings.codewhale_model = sanitize_model(&settings.codewhale_model);
    settings.codex_effort = match settings.codex_effort.trim() {
        "none" => "none".to_string(),
        "minimal" => "minimal".to_string(),
        "low" => "low".to_string(),
        "medium" => "medium".to_string(),
        "high" => "high".to_string(),
        "xhigh" => "xhigh".to_string(),
        _ => default_codex_effort(),
    };
    settings
}

fn sanitize_permission_mode(value: &str) -> String {
    match value.trim() {
        "fullAccess" => "fullAccess".to_string(),
        _ => default_permission_mode(),
    }
}

fn sanitize_model(value: &str) -> String {
    value.trim().chars().take(160).collect()
}

fn default_permission_mode() -> String {
    "default".to_string()
}

fn default_codex_effort() -> String {
    "medium".to_string()
}

fn runtime_temp_dir() -> PathBuf {
    crate::runtime_paths::runtime_temp_dir()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use uuid::Uuid;

    fn temp_support_dir() -> PathBuf {
        let dir = std::env::temp_dir().join(format!("codux-gpui-tools-test-{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sync_writes_sanitized_tool_permission_file() {
        let support_dir = temp_support_dir();
        fs::write(
            support_dir.join("settings.json"),
            serde_json::to_string_pretty(&json!({
                "ai": {
                    "runtimeTools": {
                        "codex": "fullAccess",
                        "claudeCode": "bad",
                        "gemini": "fullAccess",
                        "opencode": "default",
                        "kiro": "fullAccess",
                        "codewhale": "fullAccess",
                        "codexModel": " gpt-5.5 ",
                        "codewhaleModel": " deepseek-chat ",
                        "codexEffort": "xhigh"
                    }
                }
            }))
            .unwrap(),
        )
        .unwrap();
        let output_path = support_dir.join("tmp/tool-permissions.json");
        let service = ToolPermissionsService {
            settings_path: support_dir.join("settings.json"),
            output_path: output_path.clone(),
        };

        let summary = service.sync();
        let written =
            serde_json::from_str::<Value>(&fs::read_to_string(output_path).unwrap()).unwrap();

        assert!(summary.available);
        assert_eq!(summary.full_access_count, 4);
        assert_eq!(summary.claude_code, "default");
        assert_eq!(summary.codex_model, "gpt-5.5");
        assert_eq!(summary.codewhale_model, "deepseek-chat");
        assert_eq!(written["codex"], "fullAccess");
        assert_eq!(written["codewhale"], "fullAccess");
        assert_eq!(written["codewhaleModel"], "deepseek-chat");
        assert_eq!(written["claudeCode"], "default");
        assert_eq!(written["codexEffort"], "xhigh");

        fs::remove_dir_all(support_dir).unwrap();
    }
}
