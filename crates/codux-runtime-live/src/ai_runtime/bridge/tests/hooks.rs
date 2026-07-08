use super::*;

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
    assert_eq!(binding["sessionOrigin"].as_str(), Some("restored"));
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
    let hook = crate::ai_runtime::frame::runtime_frame_to_hook(&value).expect("hook");
    assert_eq!(hook.kind, "turnCompleted");
    assert_eq!(hook.tool, "codewhale");
    assert_eq!(hook.ai_session_id.as_deref(), Some("cw-session-1"));
    assert_eq!(hook.total_tokens, Some(42));
    let metadata = hook.metadata.expect("metadata");
    assert_eq!(metadata.was_interrupted, Some(true));
    assert_eq!(metadata.has_completed_turn, Some(false));
    fs::remove_dir_all(dir).unwrap();
}
