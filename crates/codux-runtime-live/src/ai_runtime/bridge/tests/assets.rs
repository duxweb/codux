use super::*;

#[test]
fn bridge_stages_runtime_assets_without_installing_hooks() {
    let dir = std::env::temp_dir().join(format!("codux-ai-bridge-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));

    bridge.stage_assets().unwrap();

    assert!(bridge.managed_hook_script().is_file());
    #[cfg(not(windows))]
    {
        fs::write(bridge.wrapper_bin_dir().join("kiro"), "stale").unwrap();
        bridge.stage_assets().unwrap();

        assert!(bridge.wrapper_bin_dir().join("codex").is_file());
        assert!(bridge.wrapper_bin_dir().join("kiro-cli").is_file());
        assert!(!bridge.wrapper_bin_dir().join("kiro").exists());
        assert!(bridge.wrapper_bin_dir().join("codewhale").is_file());
        assert!(bridge.wrapper_bin_dir().join("kimi").is_file());
        assert!(bridge.wrapper_bin_dir().join("kimi-code").is_file());
        assert!(bridge.wrapper_bin_dir().join("mimo").is_file());
    }
    #[cfg(windows)]
    {
        fs::write(bridge.wrapper_bin_dir().join("kiro.ps1"), "stale").unwrap();
        fs::write(bridge.wrapper_bin_dir().join("kiro.cmd"), "stale").unwrap();
        bridge.stage_assets().unwrap();

        assert!(bridge.wrapper_bin_dir().join("codex.ps1").is_file());
        assert!(bridge.wrapper_bin_dir().join("kiro-cli.ps1").is_file());
        assert!(
            bridge
                .wrapper_bin_dir()
                .parent()
                .unwrap()
                .join("codux-wrapper-helper.exe")
                .is_file()
        );
        assert!(!bridge.wrapper_bin_dir().join("kiro.ps1").exists());
        assert!(!bridge.wrapper_bin_dir().join("kiro.cmd").exists());
        assert!(bridge.wrapper_bin_dir().join("codewhale.ps1").is_file());
        assert!(bridge.wrapper_bin_dir().join("codux-ssh.ps1").is_file());
        assert!(bridge.wrapper_bin_dir().join("codux-db.ps1").is_file());
        assert!(bridge.wrapper_bin_dir().join("kimi.ps1").is_file());
        assert!(bridge.wrapper_bin_dir().join("kimi-code.ps1").is_file());
        assert!(bridge.wrapper_bin_dir().join("mimo.ps1").is_file());
        assert!(!bridge.wrapper_bin_dir().join("codex.cmd").exists());
    }
    assert!(
        dir.join("root")
            .join("scripts/wrappers/opencode-config/package.json")
            .is_file()
    );
    assert!(
        dir.join("root")
            .join("scripts/wrappers/opencode-config/xdg/mimocode/package.json")
            .is_file()
    );
    assert!(
        dir.join("root")
            .join("scripts/wrappers/opencode-config/xdg/mimocode/plugins/dmux-runtime.js")
            .is_file()
    );
    let codewhale_config = dir
        .join("root")
        .join("scripts/wrappers/managed-config/codewhale.toml");
    assert!(codewhale_config.is_file());
    let codewhale_config_text = fs::read_to_string(&codewhale_config).unwrap();
    assert!(codewhale_config_text.contains("[[hooks.hooks]]"));
    assert!(codewhale_config_text.contains("event = \"message_submit\""));
    assert!(codewhale_config_text.contains("event = \"turn_end\""));
    assert!(codewhale_config_text.contains("codewhale-turn-end"));
    assert!(!codewhale_config_text.contains(crate::runtime_paths::app_slug()));
    assert!(
        dir.join("root")
            .join("scripts/wrappers/managed-env/codewhale.env")
            .is_file()
    );
    assert!(
        dir.join("root")
            .join("scripts/wrappers/managed-env/codewhale.ps1")
            .is_file()
    );
    let launch_config: serde_json::Value = serde_json::from_slice(
        &fs::read(dir.join("root").join("scripts/wrappers/tool-drivers.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(launch_config["tools"][0]["id"].as_str(), Some("codex"));
    assert!(launch_config["tools"].as_array().unwrap().iter().any(
        |tool| tool["id"] == "claude" && tool["memoryInjection"] == "claudeAppendSystemPrompt"
    ));
    assert!(launch_config["tools"].as_array().unwrap().iter().any(
        |tool| tool["id"] == "opencode" && tool["memoryInjection"] == "opencodeSystemTransform"
    ));
    assert!(
        launch_config["tools"].as_array().unwrap().iter().any(
            |tool| tool["id"] == "mimo" && tool["memoryInjection"] == "opencodeSystemTransform"
        )
    );
    assert!(
        launch_config["tools"]
            .as_array()
            .unwrap()
            .iter()
            .any(|tool| tool["id"] == "kimi" && tool["memoryInjection"] == "none")
    );
    let codewhale_driver = launch_config["tools"]
        .as_array()
        .unwrap()
        .iter()
        .find(|tool| tool["id"] == "codewhale")
        .unwrap();
    assert_eq!(codewhale_driver["memoryInjection"].as_str(), Some("none"));
    assert_eq!(
        codewhale_driver["lifecycleConfig"]["envVar"].as_str(),
        Some("DEEPSEEK_MANAGED_CONFIG_PATH")
    );
    assert_eq!(
        codewhale_driver["lifecycleConfig"]["relativePath"].as_str(),
        Some("managed-config/codewhale.toml")
    );
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bridge_start_clears_stale_runtime_bindings_before_scan() {
    let dir = std::env::temp_dir().join(format!("codux-ai-bridge-bindings-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let binding_path = dir
        .join("temp")
        .join(crate::runtime_paths::AI_RUNTIME_BINDING_DIR_NAME)
        .join("old-term-codex.json");
    fs::write(
        &binding_path,
        r#"{"runtimeBindingId":"old-instance-codex","terminalId":"old-term","terminalInstanceId":"old-instance","tool":"codex","projectId":"project-1","projectName":"Project","projectPath":"/tmp/project","sessionTitle":"Old","launchStartedAt":1000.0,"updatedAt":1000.0}"#,
    )
    .unwrap();

    bridge.ensure_started().unwrap();
    std::thread::sleep(std::time::Duration::from_millis(50));

    assert!(!binding_path.exists());
    assert!(bridge.runtime_state_snapshot().sessions.is_empty());
    fs::remove_dir_all(dir).unwrap();
}
#[cfg(not(windows))]
#[test]
fn tool_wrapper_keeps_codux_ssh_available_to_ai_cli() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-ai-wrapper-path-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(
        &fake_codex,
        "#!/bin/sh\ncommand -v codux-ssh >/dev/null || exit 42\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let wrapper = bridge.wrapper_bin_dir().join("codex");
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let zsh_dot_dir = dir.join("zsh");
    fs::create_dir_all(&zsh_dot_dir).unwrap();
    fs::write(zsh_dot_dir.join(".zshenv"), "").unwrap();
    let output = Command::new(wrapper)
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("ZDOTDIR", zsh_dot_dir)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should expose codux-ssh to AI CLI, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    fs::remove_dir_all(dir).unwrap();
}
