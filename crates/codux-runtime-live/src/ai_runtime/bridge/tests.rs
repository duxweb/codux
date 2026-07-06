use super::*;
use std::fs;
use uuid::Uuid;

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
        assert!(!bridge.wrapper_bin_dir().join("kiro.ps1").exists());
        assert!(!bridge.wrapper_bin_dir().join("kiro.cmd").exists());
        assert!(bridge.wrapper_bin_dir().join("codewhale.ps1").is_file());
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
            .any(|tool| tool["id"] == "kimi" && tool["memoryInjection"] == "kimiAgentFile")
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

#[cfg(not(windows))]
#[test]
fn zsh_runtime_hook_keeps_wrapper_bin_first_after_user_startup_files() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-zsh-wrapper-path-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();
    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codex": "fullAccess",
            "codexModel": "gpt-5.9",
            "codexEffort": "medium"
        })
        .to_string(),
    )
    .unwrap();

    let home = dir.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join(".zshrc"),
        format!(
            "if [[ \"${{ZDOTDIR:-}}\" != \"{}\" ]]; then exit 61; fi\nexport PATH=\"{}:$PATH\"\n",
            home.display(),
            real_bin.display()
        ),
    )
    .unwrap();
    let output = Command::new("zsh")
        .args([
            "-l",
            "-i",
            "-c",
            "codex smoke; printf 'HISTFILE=%s\\n' \"${HISTFILE:-}\"",
        ])
        .env("HOME", &home)
        .env("USER", "codux")
        .env("LOGNAME", "codux")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display()),
        )
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env("DMUX_USER_ZDOTDIR", &home)
        .env("ZDOTDIR", bridge.zsh_hook_dir())
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env(
            "DMUX_ORIGINAL_PATH",
            format!("{}:/usr/bin:/bin", real_bin.display()),
        )
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "zsh should resolve codex, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line == "--dangerously-bypass-approvals-and-sandbox"),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line == "--model=gpt-5.9"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == "model_reasoning_effort=\"medium\""),
        "{stdout}"
    );
    let expected_histfile = format!("HISTFILE={}", home.join(".zsh_history").display());
    assert!(
        stdout.lines().any(|line| line == expected_histfile),
        "{stdout}"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn zsh_runtime_hook_restores_wrapper_bin_before_each_prompt() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-zsh-wrapper-precmd-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();
    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codex": "fullAccess",
            "codexModel": "gpt-5.9",
            "codexEffort": "medium"
        })
        .to_string(),
    )
    .unwrap();

    let home = dir.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join(".zshrc"),
        format!(
            "autoload -Uz add-zsh-hook\n\
             _user_prepend_real_bin() {{ export PATH=\"{}:${{PATH}}\"; }}\n\
             add-zsh-hook precmd _user_prepend_real_bin\n",
            real_bin.display()
        ),
    )
    .unwrap();

    let output = Command::new("zsh")
        .args(["-l", "-i", "-c", "precmd; codex smoke"])
        .env("HOME", &home)
        .env("USER", "codux")
        .env("LOGNAME", "codux")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display()),
        )
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env("DMUX_USER_ZDOTDIR", &home)
        .env("ZDOTDIR", bridge.zsh_hook_dir())
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env(
            "DMUX_ORIGINAL_PATH",
            format!("{}:/usr/bin:/bin", real_bin.display()),
        )
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "zsh should resolve codex, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout
            .lines()
            .any(|line| line == "--dangerously-bypass-approvals-and-sandbox"),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line == "--model=gpt-5.9"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == "model_reasoning_effort=\"medium\""),
        "{stdout}"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
#[ignore = "zsh shim smoke test; depends on the local zsh's function-vs-PATH resolution after a mid-session PATH rewrite; run with: cargo test -p codux-runtime-live -- --ignored zsh_runtime_hook_shims_codex"]
fn zsh_runtime_hook_shims_codex_when_path_is_rewritten_after_prompt() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-zsh-wrapper-shim-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codex": "fullAccess",
            "codexModel": "gpt-5.9",
            "codexEffort": "high"
        })
        .to_string(),
    )
    .unwrap();

    let home = dir.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(home.join(".zshrc"), "").unwrap();

    let output = Command::new("zsh")
        .args([
            "-l",
            "-i",
            "-c",
            &format!(
                "precmd; export PATH=\"{}:$PATH\"; codex smoke",
                real_bin.display()
            ),
        ])
        .env("HOME", &home)
        .env("USER", "codux")
        .env("LOGNAME", "codux")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display()),
        )
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env("DMUX_USER_ZDOTDIR", &home)
        .env("ZDOTDIR", bridge.zsh_hook_dir())
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env(
            "DMUX_ORIGINAL_PATH",
            format!("{}:/usr/bin:/bin", real_bin.display()),
        )
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "zsh codex command should run through wrapper, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(
        args.lines()
            .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox"),
        "{args}"
    );
    assert!(args.lines().any(|arg| arg == "--model=gpt-5.9"), "{args}");
    assert!(
        args.lines()
            .any(|arg| arg == "model_reasoning_effort=\"high\""),
        "{args}"
    );
    assert!(args.lines().any(|arg| arg == "smoke"), "{args}");
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn zsh_runtime_hook_suppresses_nested_terminal_integrations_in_user_startup() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir =
        std::env::temp_dir().join(format!("codux-zsh-terminal-integration-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf 'REAL %s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let home = dir.join("home");
    let shell_dir = home.join("Library/Application Support/kiro-cli/shell");
    fs::create_dir_all(&shell_dir).unwrap();
    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codex": "fullAccess",
            "codexModel": "gpt-5.9",
            "codexEffort": "medium"
        })
        .to_string(),
    )
    .unwrap();
    fs::write(
        home.join(".zprofile"),
        r#"[[ -f "${HOME}/Library/Application Support/kiro-cli/shell/zprofile.pre.zsh" ]] && builtin source "${HOME}/Library/Application Support/kiro-cli/shell/zprofile.pre.zsh"
export PATH="${HOME}/.local/bin:${PATH}"
"#,
    )
    .unwrap();
    fs::write(
        shell_dir.join("zprofile.pre.zsh"),
        r#"if [[ -z "${PROCESS_LAUNCHED_BY_Q:-}" ]]; then
  echo "KIRO_EXEC_WOULD_HAVE_RUN"
  export ZDOTDIR="${HOME}"
fi
"#,
    )
    .unwrap();

    let output = Command::new("zsh")
        .args([
            "-l",
            "-i",
            "-c",
            "printf 'ZDOTDIR=%s\\n' \"$ZDOTDIR\"; command -v codex; codex smoke",
        ])
        .env_clear()
        .env("HOME", &home)
        .env("USER", "codux")
        .env("LOGNAME", "codux")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display()),
        )
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env("DMUX_USER_ZDOTDIR", &home)
        .env("ZDOTDIR", bridge.zsh_hook_dir())
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env(
            "DMUX_ORIGINAL_PATH",
            format!("{}:/usr/bin:/bin", real_bin.display()),
        )
        .env_remove("PROCESS_LAUNCHED_BY_Q")
        .env_remove("Q_TERM")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "zsh should load Codux hook without nested terminal integration, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.contains("KIRO_EXEC_WOULD_HAVE_RUN"), "{stdout}");
    assert!(
        stdout.contains(&format!("ZDOTDIR={}", bridge.zsh_hook_dir().display())),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line == "REAL --enable"),
        "{stdout}"
    );
    assert!(stdout.lines().any(|line| line == "REAL hooks"), "{stdout}");
    assert!(
        stdout
            .lines()
            .any(|line| line == "REAL --dangerously-bypass-approvals-and-sandbox"),
        "{stdout}"
    );
    assert!(
        stdout
            .lines()
            .any(|line| line == "REAL model_reasoning_effort=\"medium\""),
        "{stdout}"
    );
    assert!(
        stdout.lines().any(|line| line == "REAL --model=gpt-5.9"),
        "{stdout}"
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn zsh_runtime_hook_preserves_user_configured_histfile() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-zsh-histfile-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let home = dir.join("home");
    fs::create_dir_all(&home).unwrap();
    fs::write(
        home.join(".zshrc"),
        "export HISTFILE=\"$HOME/.custom_zsh_history\"\n",
    )
    .unwrap();
    let output = Command::new("zsh")
        .args(["-l", "-i", "-c", "printf 'HISTFILE=%s\\n' \"$HISTFILE\""])
        .env("HOME", &home)
        .env("USER", "codux")
        .env("LOGNAME", "codux")
        .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env("DMUX_USER_ZDOTDIR", &home)
        .env("ZDOTDIR", bridge.zsh_hook_dir())
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "zsh should preserve user HISTFILE, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("HISTFILE={}", home.join(".custom_zsh_history").display())
    );
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn codex_wrapper_applies_tool_permissions_and_memory_injection() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-codex-wrapper-perms-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codex": "fullAccess",
            "claudeCode": "default",
            "agy": "default",
            "opencode": "default",
            "kiro": "default",
            "codexModel": "gpt-5.1",
            "codexEffort": "high"
        })
        .to_string(),
    )
    .unwrap();
    let memory_root = dir.join("memory");
    let project_root = dir.join("project");
    fs::create_dir_all(&memory_root).unwrap();
    fs::create_dir_all(&project_root).unwrap();
    fs::write(
        memory_root.join("AGENTS.md"),
        "# Codux Environment Directive\nUse `codux-ssh list` first.\n",
    )
    .unwrap();

    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env("DMUX_AI_MEMORY_WORKSPACE_ROOT", &memory_root)
        .env("DMUX_PROJECT_PATH", &project_root)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake codex, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(args.lines().any(|arg| arg == "--enable"));
    assert!(args.lines().any(|arg| arg == "hooks"));
    assert!(
        args.lines()
            .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox")
    );
    assert!(args.lines().any(|arg| arg == "--model=gpt-5.1"));
    assert!(
        args.lines()
            .any(|arg| arg == "model_reasoning_effort=\"high\"")
    );
    assert!(args.lines().any(|arg| arg == "--add-dir"));
    assert!(
        args.lines()
            .any(|arg| arg == memory_root.display().to_string())
    );
    assert!(
        args.lines()
            .any(|arg| arg.starts_with("developer_instructions="))
    );
    assert!(args.contains("# Codux Environment Directive"));
    assert!(args.contains("codux-ssh list"));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn mimo_wrapper_uses_managed_xdg_config_for_system_transform() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-mimo-wrapper-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_mimo = real_bin.join("mimo");
    fs::write(
        &fake_mimo,
        r#"#!/bin/sh
printf 'XDG_CONFIG_HOME=%s
' "${XDG_CONFIG_HOME:-}"
printf 'OPENCODE_CONFIG_DIR=%s
' "${OPENCODE_CONFIG_DIR:-}"
printf 'PROMPT=%s
' "${DMUX_AI_MEMORY_PROMPT_FILE:-}"
printf 'TOOL=%s
' "${DMUX_ACTIVE_AI_TOOL:-}"
"#,
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_mimo).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_mimo, permissions).unwrap();

    let prompt_file = dir.join("memory-prompt.txt");
    fs::write(&prompt_file, "Codux directive").unwrap();
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("mimo"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_AI_MEMORY_PROMPT_FILE", &prompt_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake mimo, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(&format!(
        "XDG_CONFIG_HOME={}",
        dir.join("root")
            .join("scripts/wrappers/opencode-config/xdg")
            .display()
    )));
    assert!(stdout.contains(
        "OPENCODE_CONFIG_DIR=
"
    ));
    assert!(stdout.contains(&format!("PROMPT={}", prompt_file.display())));
    assert!(stdout.contains("TOOL=mimo"));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn codex_wrapper_reads_tool_permissions_on_each_launch() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!(
        "codux-codex-wrapper-hot-settings-{}",
        Uuid::new_v4()
    ));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());

    write_codex_test_permissions(&permissions_file, "low");
    let first = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();
    assert!(first.status.success());
    let first_args = String::from_utf8_lossy(&first.stdout);
    assert!(
        first_args
            .lines()
            .any(|arg| arg == "model_reasoning_effort=\"low\""),
        "{first_args}"
    );

    write_codex_test_permissions(&permissions_file, "none");
    let second = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();
    assert!(second.status.success());
    let second_args = String::from_utf8_lossy(&second.stdout);
    assert!(
        second_args
            .lines()
            .all(|arg| !arg.starts_with("model_reasoning_effort=")),
        "{second_args}"
    );

    write_codex_test_permissions(&permissions_file, "high");
    let third = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();
    assert!(third.status.success());
    let third_args = String::from_utf8_lossy(&third.stdout);
    assert!(
        third_args
            .lines()
            .any(|arg| arg == "model_reasoning_effort=\"high\""),
        "{third_args}"
    );

    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
fn write_codex_test_permissions(path: &Path, effort: &str) {
    fs::write(
        path,
        serde_json::json!({
            "codex": "fullAccess",
            "codexModel": "gpt-5.7",
            "codexEffort": effort
        })
        .to_string(),
    )
    .unwrap();
}

#[cfg(not(windows))]
#[test]
fn codex_wrapper_applies_tool_permissions_when_helper_is_broken() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!(
        "codux-codex-wrapper-helper-broken-{}",
        Uuid::new_v4()
    ));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf '%s\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let wrapper_dir = bridge.wrapper_bin_dir().parent().unwrap().to_path_buf();
    let broken_helper = wrapper_dir.join("codux-wrapper-helper");
    fs::write(
        &broken_helper,
        "#!/bin/sh\necho 'error: Unrecognized option: codux-wrapper-helper' >&2\nexit 2\n",
    )
    .unwrap();
    let mut helper_permissions = fs::metadata(&broken_helper).unwrap().permissions();
    helper_permissions.set_mode(0o755);
    fs::set_permissions(&broken_helper, helper_permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codex": "fullAccess",
            "codexModel": "gpt-5.7",
            "codexEffort": "high"
        })
        .to_string(),
    )
    .unwrap();

    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake codex, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(
        args.lines()
            .any(|arg| arg == "--dangerously-bypass-approvals-and-sandbox"),
        "{args}"
    );
    assert!(args.lines().any(|arg| arg == "--model=gpt-5.7"), "{args}");
    assert!(
        args.lines()
            .any(|arg| arg == "model_reasoning_effort=\"high\""),
        "{args}"
    );
}

#[cfg(not(windows))]
#[test]
fn wrapper_helper_allowlist_includes_agent_binary() {
    assert!(current_exe_can_act_as_wrapper_helper(Path::new("codux")));
    assert!(current_exe_can_act_as_wrapper_helper(Path::new("Codux")));
    assert!(current_exe_can_act_as_wrapper_helper(Path::new(
        "Codux Dev"
    )));
    assert!(current_exe_can_act_as_wrapper_helper(Path::new(
        "codux-agent"
    )));
    assert!(!current_exe_can_act_as_wrapper_helper(Path::new(
        "codux-helper"
    )));
}

#[cfg(not(windows))]
#[test]
fn codex_wrapper_writes_resume_session_id_to_runtime_binding() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-codex-wrapper-resume-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let binding_dir = dir.join("bindings");
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .args(["resume", "019f0c1b-f835-7c33-a4f4-3e737d2fbf90"])
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", dir.join("project"))
        .env("DMUX_SESSION_TITLE", "Codex")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_AI_RUNTIME_BINDING_DIR", &binding_dir)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake codex, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(binding_dir.join("terminal-1-codex.json")).unwrap())
            .unwrap();
    assert_eq!(
        binding["externalSessionId"].as_str(),
        Some("019f0c1b-f835-7c33-a4f4-3e737d2fbf90")
    );
    assert!(binding["launchStartedAt"].as_f64().is_some());
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn wrapper_timestamp_handles_comma_decimal_locale() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-wrapper-comma-locale-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nexit 0\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let binding_dir = dir.join("bindings");
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("codex"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", dir.join("project"))
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_AI_RUNTIME_BINDING_DIR", &binding_dir)
        .env("EPOCHREALTIME", "1783071984,407")
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake codex, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(binding_dir.join("terminal-1-codex.json")).unwrap())
            .unwrap();
    assert_eq!(binding["launchStartedAt"].as_f64(), Some(1783071984.407));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn hook_event_names_use_millisecond_numeric_prefix() {
    use std::process::{Command, Stdio};

    let dir = std::env::temp_dir().join(format!("codux-hook-event-timestamp-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let events = dir.join("events");
    fs::create_dir_all(&events).unwrap();

    let output = Command::new(bridge.managed_hook_script())
        .args(["session-start", "codux", "claude"])
        .env("DMUX_RUNTIME_OWNER", "codux")
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", "/tmp/project")
        .env("DMUX_SESSION_TITLE", "Claude")
        .env("DMUX_RUNTIME_EVENT_DIR", &events)
        .env("DMUX_EXTERNAL_SESSION_ID", "claude-session-1")
        .env("EPOCHREALTIME", "1783071984,407")
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "hook failed stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let entries = fs::read_dir(&events)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    assert_eq!(entries.len(), 1);
    let name = entries[0].file_name().unwrap().to_str().unwrap();
    assert!(
        name.starts_with("1783071984407-"),
        "event filename should use millisecond timestamp, got {name}"
    );
    assert!(serde_json::from_slice::<serde_json::Value>(&fs::read(&entries[0]).unwrap()).is_ok());
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn claude_wrapper_applies_driver_memory_prompt_injection() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-claude-wrapper-memory-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_claude = real_bin.join("claude");
    fs::write(&fake_claude, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_claude).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_claude, permissions).unwrap();

    let prompt_file = dir.join("memory-prompt.txt");
    fs::write(&prompt_file, "Use Claude memory.").unwrap();

    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("claude"))
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_AI_MEMORY_PROMPT_FILE", &prompt_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake claude, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(args.lines().any(|arg| arg == "--append-system-prompt"));
    assert!(args.lines().any(|arg| arg == "Use Claude memory."));
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn bridge_prepare_strips_codux_hooks_and_keeps_user_hooks() {
    let dir = std::env::temp_dir().join(format!("codux-ai-bridge-file-{}", Uuid::new_v4()));
    let home = dir.join("home");
    let claude_settings = home.join(".claude").join("settings.json");
    fs::create_dir_all(claude_settings.parent().unwrap()).unwrap();
    fs::write(
        &claude_settings,
        serde_json::json!({
            "env": { "ANTHROPIC_AUTH_TOKEN": "PROXY_MANAGED" },
            "includeCoAuthoredBy": false,
            "hooks": {
                "UserPromptSubmit": [{
                    "matcher": "",
                    "hooks": [
                        {
                            "type": "command",
                            "command": "'/old/dmux-ai-state.sh' 'prompt-submit' 'codux' 'claude'"
                        },
                        { "type": "command", "command": "echo user-hook" }
                    ]
                }]
            }
        })
        .to_string(),
    )
    .unwrap();
    // The runtime is non-intrusive: prepare() STRIPS codux hooks, never installs.
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), home);

    bridge.prepare().unwrap();

    let settings: serde_json::Value =
        serde_json::from_slice(&fs::read(&claude_settings).unwrap()).unwrap();
    // The user's own settings and hook survive; only codux's entry is gone.
    assert_eq!(
        settings["env"]["ANTHROPIC_AUTH_TOKEN"].as_str(),
        Some("PROXY_MANAGED")
    );
    assert_eq!(settings["includeCoAuthoredBy"].as_bool(), Some(false));
    let serialized = settings.to_string();
    assert!(!serialized.contains("dmux-ai-state.sh"));
    assert!(serialized.contains("echo user-hook"));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn codewhale_wrapper_applies_configured_model_and_resume_session() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-codewhale-wrapper-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codewhale = real_bin.join("codewhale");
    fs::write(
        &fake_codewhale,
        "#!/bin/sh\nprintf 'external=%s\\n' \"$DMUX_EXTERNAL_SESSION_ID\"\nprintf 'model=%s\\n' \"$DMUX_ACTIVE_AI_MODEL\"\nprintf 'managed=%s\\n' \"$DEEPSEEK_MANAGED_CONFIG_PATH\"\nprintf '%s\\n' \"$@\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_codewhale).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codewhale, permissions).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\nprintf 'wrong-codex\\n'\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "codewhale": "fullAccess",
            "codewhaleModel": "deepseek-chat"
        })
        .to_string(),
    )
    .unwrap();

    let binding_dir = dir.join("bindings");
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("codewhale"))
        .args([
            "--config",
            "/tmp/user-codewhale.toml",
            "resume",
            "session-1",
        ])
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", dir.join("project"))
        .env("DMUX_SESSION_TITLE", "CodeWhale")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_AI_RUNTIME_BINDING_DIR", &binding_dir)
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env("DMUX_ACTIVE_AI_RESOLVED_PATH", real_bin.join("codex"))
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake codewhale, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(args.lines().any(|arg| arg == "external=session-1"));
    assert!(args.lines().any(|arg| arg == "model=deepseek-chat"));
    assert!(
        args.lines()
            .any(|arg| arg.ends_with("managed-config/codewhale.toml"))
    );
    assert!(args.lines().any(|arg| arg == "--yolo"));
    assert!(args.lines().any(|arg| arg == "--model"));
    assert!(args.lines().any(|arg| arg == "deepseek-chat"));
    assert!(args.lines().any(|arg| arg == "--config"));
    assert!(args.lines().any(|arg| arg == "/tmp/user-codewhale.toml"));
    assert!(args.lines().any(|arg| arg == "resume"));
    assert!(args.lines().any(|arg| arg == "session-1"));
    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(binding_dir.join("terminal-1-codewhale.json")).unwrap())
            .unwrap();
    assert_eq!(binding["externalSessionId"].as_str(), Some("session-1"));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn kimi_wrapper_applies_configured_model_and_memory_agent_file() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-kimi-wrapper-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_kimi = real_bin.join("kimi");
    fs::write(&fake_kimi, "#!/bin/sh\nprintf '%s\\n' \"$@\"\n").unwrap();
    let mut permissions = fs::metadata(&fake_kimi).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_kimi, permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "kimi": "fullAccess",
            "kimiModel": "kimi-k2"
        })
        .to_string(),
    )
    .unwrap();

    let prompt_file = dir.join("memory-prompt.txt");
    fs::write(&prompt_file, "Use Kimi memory.\nSecond line.").unwrap();

    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("kimi"))
        .arg("hello")
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env("DMUX_AI_MEMORY_PROMPT_FILE", &prompt_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake kimi, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(args.lines().any(|arg| arg == "--model"), "{args}");
    assert!(args.lines().any(|arg| arg == "kimi-k2"), "{args}");
    assert!(args.lines().any(|arg| arg == "--agent-file"), "{args}");
    let agent_path = args
        .lines()
        .skip_while(|arg| *arg != "--agent-file")
        .nth(1)
        .expect("agent file argument");
    let agent = fs::read_to_string(agent_path).unwrap();
    assert!(agent.contains("extend: default"));
    assert!(agent.contains("ROLE_ADDITIONAL: |"));
    assert!(agent.contains("Use Kimi memory."));
    assert!(agent.contains("Second line."));
    assert!(!args.lines().any(|arg| arg == "--approval-mode"), "{args}");
    assert!(!args.lines().any(|arg| arg == "yolo"), "{args}");
    assert!(args.lines().any(|arg| arg == "hello"), "{args}");
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn kiro_cli_wrapper_writes_runtime_binding_and_model_without_permission_args() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-kiro-wrapper-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();

    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&real_bin).unwrap();
    let fake_kiro = real_bin.join("kiro-cli");
    fs::write(
        &fake_kiro,
        "#!/bin/sh\nprintf 'external=%s\\n' \"$DMUX_EXTERNAL_SESSION_ID\"\nprintf 'model=%s\\n' \"$DMUX_ACTIVE_AI_MODEL\"\nprintf '%s\\n' \"$@\"\n",
    )
    .unwrap();
    let mut permissions = fs::metadata(&fake_kiro).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_kiro, permissions).unwrap();

    let permissions_file = dir.join("tool-permissions.json");
    fs::write(
        &permissions_file,
        serde_json::json!({
            "kiro": "fullAccess",
            "kiroModel": "auto"
        })
        .to_string(),
    )
    .unwrap();

    let binding_dir = dir.join("bindings");
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());
    let output = Command::new(bridge.wrapper_bin_dir().join("kiro-cli"))
        .args(["--resume-id", "session-1"])
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", dir.join("project"))
        .env("DMUX_SESSION_TITLE", "Kiro")
        .env("DMUX_RUNTIME_EVENT_DIR", dir.join("events"))
        .env("DMUX_AI_RUNTIME_BINDING_DIR", &binding_dir)
        .env("DMUX_TOOL_PERMISSION_SETTINGS_FILE", &permissions_file)
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should execute fake kiro-cli, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let args = String::from_utf8_lossy(&output.stdout);
    assert!(args.lines().any(|arg| arg == "external=session-1"));
    assert!(args.lines().any(|arg| arg == "model=auto"));
    assert!(!args.lines().any(|arg| arg == "--model"), "{args}");
    assert!(args.lines().any(|arg| arg == "--resume-id"), "{args}");
    assert!(args.lines().any(|arg| arg == "session-1"), "{args}");
    assert!(!args.lines().any(|arg| arg == "--approval-mode"), "{args}");
    assert!(!args.lines().any(|arg| arg == "yolo"), "{args}");

    let binding: serde_json::Value =
        serde_json::from_slice(&fs::read(binding_dir.join("terminal-1-kiro-cli.json")).unwrap())
            .unwrap();
    assert_eq!(binding["tool"].as_str(), Some("kiro-cli"));
    assert_eq!(binding["externalSessionId"].as_str(), Some("session-1"));
    assert_eq!(binding["model"].as_str(), Some("auto"));
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn codewhale_hook_writes_runtime_event() {
    use std::process::{Command, Stdio};

    let dir = std::env::temp_dir().join(format!("codux-codewhale-event-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let events = dir.join("events");
    fs::create_dir_all(&events).unwrap();

    let mut child = Command::new(bridge.managed_hook_script())
        .args(["codewhale-message-submit", "codewhale"])
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", "/tmp/project")
        .env("DMUX_SESSION_TITLE", "CodeWhale")
        .env("DMUX_RUNTIME_EVENT_DIR", &events)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(
                br#"{"event":"message_submit","session_id":"cw-session-1","workspace":"/tmp/project","text":"hello"}"#,
            )
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "hook failed stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut entries = fs::read_dir(&events)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries.len(), 1);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&entries[0]).unwrap()).unwrap();
    assert_eq!(value["kind"], "ai-hook");
    assert_eq!(value["payload"]["kind"], "promptSubmitted");
    assert_eq!(value["payload"]["tool"], "codewhale");
    assert_eq!(value["payload"]["aiSessionID"], "cw-session-1");
    assert_eq!(value["payload"]["metadata"]["source"], "user-input");
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn codewhale_hook_without_payload_does_not_block() {
    use std::process::{Command, Stdio};

    let dir = std::env::temp_dir().join(format!("codux-codewhale-empty-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let events = dir.join("events");
    fs::create_dir_all(&events).unwrap();

    let output = Command::new(bridge.managed_hook_script())
        .args(["codewhale-message-submit", "codewhale"])
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", "/tmp/project")
        .env("DMUX_SESSION_TITLE", "CodeWhale")
        .env("DMUX_RUNTIME_EVENT_DIR", &events)
        .stdin(Stdio::null())
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "hook failed stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut entries = fs::read_dir(&events)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries.len(), 1);
    let value: serde_json::Value = serde_json::from_slice(&fs::read(&entries[0]).unwrap()).unwrap();
    assert_eq!(value["payload"]["kind"], "promptSubmitted");
    assert_eq!(value["payload"]["tool"], "codewhale");
    assert_eq!(value["payload"]["aiSessionID"], serde_json::Value::Null);
    assert_eq!(value["payload"]["metadata"]["source"], "user-input");
    fs::remove_dir_all(dir).unwrap();
}

#[cfg(not(windows))]
#[test]
fn codewhale_turn_end_hook_writes_interrupted_lifecycle_event() {
    use std::process::{Command, Stdio};

    let dir = std::env::temp_dir().join(format!("codux-codewhale-turn-end-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let events = dir.join("events");
    fs::create_dir_all(&events).unwrap();

    let mut child = Command::new(bridge.managed_hook_script())
        .args(["codewhale-turn-end", "codewhale"])
        .env("DMUX_SESSION_ID", "terminal-1")
        .env("DMUX_SESSION_INSTANCE_ID", "instance-1")
        .env("DMUX_PROJECT_ID", "project-1")
        .env("DMUX_PROJECT_NAME", "Project")
        .env("DMUX_PROJECT_PATH", "/tmp/project")
        .env("DMUX_SESSION_TITLE", "CodeWhale")
        .env("DMUX_ACTIVE_AI_MODEL", "deepseek-chat")
        .env("DMUX_RUNTIME_EVENT_DIR", &events)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().unwrap();
        stdin
            .write_all(
                br#"{"event":"turn_end","session_id":"cw-session-1","status":"interrupted","totals":{"session_tokens":42}}"#,
            )
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "hook failed stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );

    let mut entries = fs::read_dir(&events)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .collect::<Vec<_>>();
    entries.sort();
    assert_eq!(entries.len(), 1);
    let value = fs::read(&entries[0]).unwrap();
    let envelope: serde_json::Value = serde_json::from_slice(&value).unwrap();
    assert_eq!(envelope["kind"], "ai-lifecycle-hook");
    let hook = super::super::frame::runtime_frame_to_hook(&value).expect("hook");
    assert_eq!(hook.kind, "turnCompleted");
    assert_eq!(hook.tool, "codewhale");
    assert_eq!(hook.ai_session_id.as_deref(), Some("cw-session-1"));
    assert_eq!(hook.total_tokens, Some(42));
    let metadata = hook.metadata.expect("metadata");
    assert_eq!(metadata.was_interrupted, Some(true));
    assert_eq!(metadata.has_completed_turn, Some(false));
    fs::remove_dir_all(dir).unwrap();
}
