use super::*;

#[test]
fn terminal_environment_forces_utf8_locale() {
    let config = TerminalPtyConfig {
        env: Some(HashMap::from([
            ("LANG".to_string(), "C".to_string()),
            ("LC_ALL".to_string(), "C".to_string()),
            ("LC_CTYPE".to_string(), "POSIX".to_string()),
        ])),
        ..Default::default()
    };

    let env = terminal_environment("/bin/zsh", None, "term-1", &config, None);

    assert_eq!(env.get("LANG").map(String::as_str), Some("en_US.UTF-8"));
    assert_eq!(env.get("LC_ALL").map(String::as_str), Some("en_US.UTF-8"));
    assert_eq!(env.get("LC_CTYPE").map(String::as_str), Some("en_US.UTF-8"));
}

#[test]
fn terminal_environment_does_not_set_term_program() {
    let config = TerminalPtyConfig::default();

    let env = terminal_environment("/bin/zsh", None, "term-1", &config, None);

    assert!(!env.contains_key("TERM_PROGRAM"));
    assert!(!env.contains_key("TERM_PROGRAM_VERSION"));
}

#[test]
fn terminal_environment_preserves_real_term_program() {
    let config = TerminalPtyConfig {
        env: Some(HashMap::from([
            ("TERM_PROGRAM".to_string(), "Ghostty".to_string()),
            ("TERM_PROGRAM_VERSION".to_string(), "1.2.3".to_string()),
        ])),
        ..Default::default()
    };

    let env = terminal_environment("/bin/zsh", None, "term-1", &config, None);

    assert_eq!(env.get("TERM_PROGRAM").map(String::as_str), Some("Ghostty"));
    assert_eq!(
        env.get("TERM_PROGRAM_VERSION").map(String::as_str),
        Some("1.2.3")
    );
}

#[test]
fn terminal_environment_injects_codux_runtime_context() {
    let temp = std::env::temp_dir().join(format!("codux-terminal-runtime-root-{}", Uuid::new_v4()));
    let runtime_root = temp.join("runtime-root");
    fs::create_dir_all(runtime_root.join("scripts/shell-hooks/zsh")).unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/zsh/.zshenv"),
        "# test\n",
    )
    .unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/zsh/.zprofile"),
        "# test\n",
    )
    .unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/zsh/.zshrc"),
        "# test\n",
    )
    .unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/dmux-ai-hook.zsh"),
        "# test\n",
    )
    .unwrap();
    let context = TerminalLaunchContext {
        root_project_id: "project-1".to_string(),
        project_id: "project-1".to_string(),
        project_name: "Codux".to_string(),
        project_path: PathBuf::from("/workspace/codux"),
        support_dir: PathBuf::from("/support/Codux"),
        runtime_root: runtime_root.clone(),
        terminal_id: Some("gpui-term-1".to_string()),
        slot_id: Some("gpui-pane-1-1".to_string()),
        session_key: Some("gpui:project-1:gpui-term-1:gpui-pane-1-1".to_string()),
        session_title: Some("终端 1".to_string()),
        session_cwd: Some(PathBuf::from("/workspace/codux")),
        session_instance_id: Some("session-instance-1".to_string()),
        tool_permissions_file: Some(PathBuf::from("/tmp/codux/tool-permissions.json")),
        memory_workspace_root: Some(PathBuf::from("/tmp/codux/memory-workspaces/project-1")),
        memory_prompt_file: Some(PathBuf::from(
            "/tmp/codux/memory-workspaces/project-1/memory-prompt.txt",
        )),
        memory_index_file: Some(PathBuf::from(
            "/tmp/codux/memory-workspaces/project-1/MEMORY.md",
        )),
        runtime_target: Default::default(),
    };
    let env = terminal_environment(
        "/bin/zsh",
        Some("/workspace/codux"),
        "gpui-term-1",
        &context.to_config(),
        Some(&context),
    );
    let path = env.get("PATH").expect("PATH should be set");
    assert!(path.starts_with(runtime_root.join("scripts/wrappers/bin").to_str().unwrap()));
    assert_eq!(
        env.get("DMUX_PROJECT_PATH").map(String::as_str),
        Some("/workspace/codux")
    );
    // Claude Code defaults to its classic (scrollback) renderer for a clean
    // desktop<->mobile handoff, unless the user set the var themselves.
    assert_eq!(
        env.get("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN")
            .map(String::as_str),
        Some("1")
    );
    assert_eq!(
        env.get("CODUX_TERMINAL_ID").map(String::as_str),
        Some("gpui-term-1")
    );
    assert_eq!(
        env.get("DMUX_SESSION_INSTANCE_ID").map(String::as_str),
        Some("session-instance-1")
    );
    assert_eq!(
        env.get("DMUX_AI_MEMORY_INDEX_FILE").map(String::as_str),
        Some("/tmp/codux/memory-workspaces/project-1/MEMORY.md")
    );
    assert_eq!(
        env.get("DMUX_WRAPPER_BIN").map(String::as_str),
        Some(runtime_root.join("scripts/wrappers/bin").to_str().unwrap())
    );
    assert_eq!(
        env.get("DMUX_APP_SUPPORT_ROOT").map(String::as_str),
        Some("/support/Codux")
    );
    assert_eq!(
        env.get("CODUX_SSH_PROFILES_FILE").map(String::as_str),
        Some("/support/Codux/ssh_profiles.json")
    );
    assert_eq!(
        env.get("CODUX_DB_PROFILES_FILE").map(String::as_str),
        Some("/support/Codux/db_profiles.json")
    );
    assert_eq!(
        env.get("CODUX_DB_PROJECT_ID").map(String::as_str),
        Some("project-1")
    );
    assert_eq!(
        env.get("DMUX_USER_ZDOTDIR").map(String::as_str),
        env.get("HOME").map(String::as_str)
    );
    assert_eq!(
        env.get("ZDOTDIR").map(String::as_str),
        Some(
            runtime_root
                .join("scripts/shell-hooks/zsh")
                .to_str()
                .unwrap()
        )
    );
    assert_eq!(
        env.get("DMUX_ZSH_HOOK_SCRIPT").map(String::as_str),
        Some(
            runtime_root
                .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                .to_str()
                .unwrap()
        )
    );
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn terminal_environment_treats_named_zsh_wrapper_as_zsh() {
    let temp = std::env::temp_dir().join(format!(
        "codux-terminal-runtime-root-named-zsh-{}",
        Uuid::new_v4()
    ));
    let runtime_root = temp.join("runtime-root");
    fs::create_dir_all(runtime_root.join("scripts/shell-hooks/zsh")).unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/zsh/.zshenv"),
        "# test\n",
    )
    .unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/zsh/.zprofile"),
        "# test\n",
    )
    .unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/zsh/.zshrc"),
        "# test\n",
    )
    .unwrap();
    fs::write(
        runtime_root.join("scripts/shell-hooks/dmux-ai-hook.zsh"),
        "# test\n",
    )
    .unwrap();
    let context = TerminalLaunchContext {
        root_project_id: "project-1".to_string(),
        project_id: "project-1".to_string(),
        project_name: "Codux".to_string(),
        project_path: PathBuf::from("/workspace/codux"),
        support_dir: PathBuf::from("/support/Codux"),
        runtime_root: runtime_root.clone(),
        terminal_id: Some("gpui-term-1".to_string()),
        slot_id: None,
        session_key: None,
        session_title: None,
        session_cwd: Some(PathBuf::from("/workspace/codux")),
        session_instance_id: None,
        tool_permissions_file: None,
        memory_workspace_root: None,
        memory_prompt_file: None,
        memory_index_file: None,
        runtime_target: Default::default(),
    };

    let env = terminal_environment(
        "/Users/example/.local/bin/zsh (kiro-cli-term)",
        Some("/workspace/codux"),
        "gpui-term-1",
        &context.to_config(),
        Some(&context),
    );

    assert_eq!(
        env.get("ZDOTDIR").map(String::as_str),
        Some(
            runtime_root
                .join("scripts/shell-hooks/zsh")
                .to_str()
                .unwrap()
        )
    );
    assert_eq!(
        env.get("DMUX_ZSH_HOOK_SCRIPT").map(String::as_str),
        Some(
            runtime_root
                .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                .to_str()
                .unwrap()
        )
    );
    let _ = fs::remove_dir_all(temp);
}

#[cfg(not(target_os = "windows"))]
#[test]
fn terminal_shell_normalization_rejects_nested_integration_shells() {
    assert_eq!(
        normalize_terminal_shell("/Users/example/.local/bin/zsh (kiro-cli-term)"),
        None
    );
    assert_eq!(
        normalize_terminal_shell("/Users/example/.local/bin/kiro-cli-term"),
        None
    );
    assert_eq!(
        normalize_terminal_shell("/bin/zsh"),
        Some("/bin/zsh".to_string())
    );
}

#[test]
fn terminal_environment_does_not_override_zdotdir_when_runtime_zsh_hook_is_incomplete() {
    let temp = std::env::temp_dir().join(format!(
        "codux-terminal-runtime-root-missing-hook-{}",
        Uuid::new_v4()
    ));
    let runtime_root = temp.join("runtime-root");
    fs::create_dir_all(runtime_root.join("scripts/shell-hooks/zsh")).unwrap();
    let context = TerminalLaunchContext {
        root_project_id: "project-1".to_string(),
        project_id: "project-1".to_string(),
        project_name: "Codux".to_string(),
        project_path: PathBuf::from("/workspace/codux"),
        support_dir: PathBuf::from("/support/Codux"),
        runtime_root: runtime_root.clone(),
        terminal_id: Some("gpui-term-1".to_string()),
        slot_id: Some("gpui-pane-1-1".to_string()),
        session_key: Some("gpui:project-1:gpui-term-1:gpui-pane-1-1".to_string()),
        session_title: Some("Terminal 1".to_string()),
        session_cwd: Some(PathBuf::from("/workspace/codux")),
        session_instance_id: Some("session-instance-1".to_string()),
        tool_permissions_file: None,
        memory_workspace_root: None,
        memory_prompt_file: None,
        memory_index_file: None,
        runtime_target: Default::default(),
    };

    let env = terminal_environment(
        "/bin/zsh",
        Some("/workspace/codux"),
        "gpui-term-1",
        &context.to_config(),
        Some(&context),
    );

    assert_ne!(
        env.get("ZDOTDIR").map(String::as_str),
        Some(
            runtime_root
                .join("scripts/shell-hooks/zsh")
                .to_str()
                .unwrap()
        )
    );
    assert!(!env.contains_key("DMUX_ZSH_HOOK_SCRIPT"));
    let _ = fs::remove_dir_all(temp);
}

#[test]
fn terminal_environment_keeps_runtime_context_compact() {
    let context = TerminalLaunchContext {
        root_project_id: "project-1".to_string(),
        project_id: "project-1".to_string(),
        project_name: "Codux".to_string(),
        project_path: PathBuf::from("/workspace/codux"),
        support_dir: PathBuf::from("/support/Codux"),
        runtime_root: PathBuf::from("/runtime-assets"),
        terminal_id: Some("gpui-term-1".to_string()),
        slot_id: Some("gpui-pane-1-1".to_string()),
        session_key: Some("gpui:project-1:gpui-term-1:gpui-pane-1-1".to_string()),
        session_title: Some("Terminal 1".to_string()),
        session_cwd: Some(PathBuf::from("/workspace/codux")),
        session_instance_id: Some("session-instance-1".to_string()),
        tool_permissions_file: Some(PathBuf::from("/tmp/codux/tool-permissions.json")),
        memory_workspace_root: Some(PathBuf::from("/tmp/codux/memory-workspaces/project-1")),
        memory_prompt_file: Some(PathBuf::from(
            "/tmp/codux/memory-workspaces/project-1/memory-prompt.txt",
        )),
        memory_index_file: Some(PathBuf::from(
            "/tmp/codux/memory-workspaces/project-1/MEMORY.md",
        )),
        runtime_target: Default::default(),
    };

    let env = terminal_environment(
        "/bin/zsh",
        Some("/workspace/codux"),
        "gpui-term-1",
        &context.to_config(),
        Some(&context),
    );
    let total_bytes = env
        .iter()
        .map(|(key, value)| key.len() + value.len() + 2)
        .sum::<usize>();

    assert!(total_bytes < 16 * 1024);
}

#[cfg(not(windows))]
#[test]
fn parses_noisy_shell_environment_capture() {
    let mut output = Vec::new();
    output.extend_from_slice(b"startup noise");
    output.extend_from_slice(b"__BEGIN__\0PATH=/opt/bin:/usr/bin\0HISTFILE=/tmp/history\0");
    output.extend_from_slice(b"__END__\0more noise");

    let env = parse_captured_shell_environment(&output, "__BEGIN__", "__END__").unwrap();

    assert_eq!(
        env.get("PATH").map(String::as_str),
        Some("/opt/bin:/usr/bin")
    );
    assert_eq!(
        env.get("HISTFILE").map(String::as_str),
        Some("/tmp/history")
    );
}
