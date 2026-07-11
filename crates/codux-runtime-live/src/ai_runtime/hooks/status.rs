use super::codewhale::codewhale_hook_config_status_in;
use crate::{
    ai_runtime::bridge::{AIRuntimeHookConfigStatus, AIRuntimeToolHookConfigStatus},
    ai_runtime::tool_driver::{AIRuntimeToolHookDriver, ai_runtime_tool_drivers},
};
use std::path::Path;

pub fn hook_config_status(wrapper_dir: &Path) -> AIRuntimeHookConfigStatus {
    hook_config_status_in(wrapper_dir)
}

pub fn hook_config_status_in(wrapper_dir: &Path) -> AIRuntimeHookConfigStatus {
    let mut codewhale = AIRuntimeToolHookConfigStatus::default();
    let opencode = opencode_hook_config_status(&wrapper_dir.join("opencode-config"));
    let mimo = opencode.clone();

    for driver in ai_runtime_tool_drivers() {
        let status = match driver.hook {
            AIRuntimeToolHookDriver::CodeWhaleToml => {
                let Some(config) = driver.lifecycle_config else {
                    continue;
                };
                codewhale_hook_config_status_in(
                    &wrapper_dir.join(config.relative_path),
                    driver.lifecycle_hooks,
                )
            }
            AIRuntimeToolHookDriver::OpenCodePlugin | AIRuntimeToolHookDriver::None => continue,
        };
        if driver.id == "codewhale" {
            codewhale = status;
        }
    }

    AIRuntimeHookConfigStatus {
        codex: AIRuntimeToolHookConfigStatus::default(),
        claude: AIRuntimeToolHookConfigStatus::default(),
        opencode,
        mimo,
        kiro: AIRuntimeToolHookConfigStatus::default(),
        codewhale,
        kimi: AIRuntimeToolHookConfigStatus::default(),
    }
}

pub fn opencode_hook_config_status(config_dir: &Path) -> AIRuntimeToolHookConfigStatus {
    let expected = ["package.json", "plugins/dmux-runtime.js"];
    let missing = expected
        .iter()
        .filter(|relative| !config_dir.join(relative).exists())
        .map(|relative| relative.to_string())
        .collect::<Vec<_>>();
    AIRuntimeToolHookConfigStatus {
        configured: missing.is_empty(),
        config_path: config_dir.display().to_string(),
        missing,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_paths::app_slug;
    use std::fs;
    use uuid::Uuid;

    #[test]
    fn opencode_hook_config_status_matches_embedded_runtime_assets() {
        let home = std::env::temp_dir().join(format!("codux-opencode-hooks-{}", Uuid::new_v4()));
        let config = home.join("opencode-config");
        fs::create_dir_all(config.join("plugins")).unwrap();
        fs::write(config.join("package.json"), "{}").unwrap();
        fs::write(config.join("plugins/dmux-runtime.js"), "export {};").unwrap();

        let status = opencode_hook_config_status(&config);

        assert!(status.configured);
        assert!(status.missing.is_empty());
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn hook_config_status_reports_missing_codewhale_hooks() {
        let root = std::env::temp_dir().join(format!("codux-codewhale-hooks-{}", Uuid::new_v4()));
        let wrapper_dir = root.join("wrappers");
        fs::create_dir_all(&wrapper_dir).unwrap();

        let status = hook_config_status_in(&wrapper_dir);

        assert!(!status.codewhale.configured);
        assert!(!status.codewhale.missing.is_empty());
        assert!(
            status
                .codewhale
                .config_path
                .ends_with("managed-config/codewhale.toml")
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn hook_config_status_accepts_staged_codewhale_lifecycle_config() {
        let root = std::env::temp_dir().join(format!("codux-codewhale-hooks-{}", Uuid::new_v4()));
        let wrapper_dir = root.join("wrappers");
        let config = wrapper_dir.join("managed-config").join("codewhale.toml");
        fs::create_dir_all(config.parent().unwrap()).unwrap();
        fs::write(
            &config,
            format!(
                r#"
[hooks]
enabled = true

[[hooks.hooks]]
name = "codux-codewhale-message-submit"
event = "message_submit"
command = "'/tmp/dmux-ai-state.sh' 'codewhale-message-submit' '{}' 'codewhale'"

[[hooks.hooks]]
name = "codux-codewhale-turn-end"
event = "turn_end"
command = "'/tmp/dmux-ai-state.sh' 'codewhale-turn-end' '{}' 'codewhale'"
"#,
                app_slug(),
                app_slug()
            ),
        )
        .unwrap();

        let status = hook_config_status_in(&wrapper_dir);

        assert!(status.codewhale.configured);
        assert!(status.codewhale.missing.is_empty());
        fs::remove_dir_all(root).unwrap();
    }
}
