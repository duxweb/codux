use super::*;

#[cfg(not(windows))]
#[test]
fn codex_wrapper_reenters_recreated_session_working_directory() {
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-wrapper-cwd-recovery-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let project = dir.join("project");
    let real_bin = dir.join("real-bin");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&real_bin).unwrap();
    let fake_codex = real_bin.join("codex");
    fs::write(&fake_codex, "#!/bin/sh\n/bin/pwd -P\n").unwrap();
    let mut permissions = fs::metadata(&fake_codex).unwrap().permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&fake_codex, permissions).unwrap();
    let search_path = format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display());

    let output = Command::new("zsh")
        .args([
            "-fc",
            "builtin cd -- \"$DMUX_SESSION_CWD\" || exit 1; /bin/rmdir -- \"$DMUX_SESSION_CWD\" || exit 2; /bin/mkdir -- \"$DMUX_SESSION_CWD\" || exit 3; exec \"$CODEX_WRAPPER\"",
        ])
        .env("PATH", &search_path)
        .env("DMUX_ORIGINAL_PATH", &search_path)
        .env("DMUX_SESSION_CWD", &project)
        .env("CODEX_WRAPPER", bridge.wrapper_bin_dir().join("codex"))
        .env_remove("DMUX_ACTIVE_AI_RESOLVED_PATH")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "wrapper should restore the child cwd, stderr={}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        fs::canonicalize(&project).unwrap().display().to_string()
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("restored working directory"),
        "stderr={}",
        String::from_utf8_lossy(&output.stderr)
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
    assert!(current_exe_can_act_as_wrapper_helper(Path::new(
        "codux.exe"
    )));
    assert!(current_exe_can_act_as_wrapper_helper(Path::new("Codux")));
    assert!(current_exe_can_act_as_wrapper_helper(Path::new(
        "Codux.exe"
    )));
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
    assert_eq!(binding["sessionOrigin"].as_str(), Some("restored"));
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
