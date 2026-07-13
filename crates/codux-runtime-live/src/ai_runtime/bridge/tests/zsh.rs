use super::*;

#[cfg(not(windows))]
#[test]
fn zsh_hook_reenters_recreated_session_working_directory() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-zsh-cwd-recovery-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let project = dir.join("project");
    fs::create_dir_all(&project).unwrap();

    let output = Command::new("zsh")
        .args([
            "-fc",
            "source \"$DMUX_ZSH_HOOK_SCRIPT\"; builtin cd -- \"$DMUX_SESSION_CWD\" || exit 1; /bin/rmdir -- \"$DMUX_SESSION_CWD\" || exit 2; /bin/mkdir -- \"$DMUX_SESSION_CWD\" || exit 3; [[ . -ef \"$DMUX_SESSION_CWD\" ]] && exit 4; _dmux_ai_preexec 'git status'; [[ . -ef \"$DMUX_SESSION_CWD\" ]] || exit 5; builtin pwd -P",
        ])
        .env("DMUX_SESSION_CWD", &project)
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env_remove("DMUX_AI_HOOK_INSTALLED")
        .output()
        .unwrap();

    assert!(
        output.status.success(),
        "hook should restore the shell cwd, stderr={}",
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
fn zsh_hook_preserves_a_valid_user_working_directory() {
    use std::process::Command;

    let dir = std::env::temp_dir().join(format!("codux-zsh-cwd-preserve-{}", Uuid::new_v4()));
    let bridge = AIRuntimeBridge::with_paths(dir.join("root"), dir.join("temp"), dir.join("home"));
    bridge.stage_assets().unwrap();
    let project = dir.join("project");
    let other = dir.join("other");
    fs::create_dir_all(&project).unwrap();
    fs::create_dir_all(&other).unwrap();

    let output = Command::new("zsh")
        .args([
            "-fc",
            "source \"$DMUX_ZSH_HOOK_SCRIPT\"; builtin cd -- \"$OTHER_CWD\" || exit 1; _dmux_ai_preexec 'git status'; builtin pwd -P",
        ])
        .env("DMUX_SESSION_CWD", &project)
        .env("DMUX_ZSH_HOOK_SCRIPT", bridge.zsh_hook_script())
        .env("DMUX_WRAPPER_BIN", bridge.wrapper_bin_dir())
        .env("OTHER_CWD", &other)
        .env_remove("DMUX_AI_HOOK_INSTALLED")
        .output()
        .unwrap();

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        fs::canonicalize(&other).unwrap().display().to_string()
    );
    assert!(!String::from_utf8_lossy(&output.stderr).contains("restored working directory"));
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
            // Skip /etc/z* so machine-level shell config can't leak into the
            // hook chain under test; the ZDOTDIR chain still runs in full.
            "--no-globalrcs",
            "-l",
            "-i",
            "-c",
            "codex smoke; printf 'HISTFILE=%s\\n' \"${HISTFILE:-}\"",
        ])
        .env_clear()
        .env("HOME", &home)
        .env("USER", "codux")
        .env("LOGNAME", "codux")
        .env(
            "PATH",
            format!("{}:/usr/bin:/bin:/usr/sbin:/sbin", real_bin.display()),
        )
        // The runner's shell may export HISTFILE; the assertion below needs
        // the hook chain to derive it from DMUX_USER_ZDOTDIR alone.
        .env_remove("HISTFILE")
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
        .args([
            "--no-globalrcs",
            "-l",
            "-i",
            "-c",
            "for fn in $precmd_functions; do $fn; done; codex smoke",
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
