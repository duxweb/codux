use super::*;

pub(super) const DOTENV_KEYS: &[&str] = &[
    "GEMINI_API_KEY",
    "GEMINI_MODEL",
    "GOOGLE_API_KEY",
    "GOOGLE_GEMINI_BASE_URL",
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_BASE_URL",
    "OPENAI_API_KEY",
    "OPENAI_BASE_URL",
    "CODEX_HOME",
    "OPENCODE_API_KEY",
    "OPENCODE_BASE_URL",
    "CODEWHALE_PROVIDER",
    "DEEPSEEK_API_KEY",
    "DEEPSEEK_BASE_URL",
    "DEEPSEEK_MODEL",
    "DEEPSEEK_AUTH_TOKEN",
    "HTTPS_PROXY",
    "HTTP_PROXY",
    "ALL_PROXY",
    "NO_PROXY",
];

pub fn terminal_environment(
    shell: &str,
    cwd: Option<&str>,
    session_id: &str,
    config: &TerminalPtyConfig,
    context: Option<&TerminalLaunchContext>,
) -> HashMap<String, String> {
    let home = crate::runtime_paths::home_dir();
    let home_text = home.display().to_string();
    let user = default_user();
    let session_cwd = cwd
        .map(str::to_string)
        .or_else(|| context.map(|context| context.project_path.display().to_string()))
        .unwrap_or_else(|| home_text.clone());
    let project_path = context
        .map(|context| context.project_path.display().to_string())
        .unwrap_or_else(|| session_cwd.clone());
    let project_name = config
        .project_name
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.map(|context| context.project_name.clone()))
        .or_else(|| project_path_name(&project_path))
        .unwrap_or_else(|| "Codux".to_string());
    let project_id = config
        .project_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.map(|context| context.project_id.clone()))
        .unwrap_or_else(|| project_name.clone());
    let terminal_id = config
        .terminal_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.and_then(|context| context.terminal_id.clone()))
        .unwrap_or_else(|| session_id.to_string());
    let slot_id = config
        .slot_id
        .clone()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| context.and_then(|context| context.slot_id.clone()))
        .unwrap_or_default();
    let session_key = config
        .session_key
        .clone()
        .or_else(|| context.and_then(|context| context.session_key.clone()))
        .unwrap_or_default();
    let session_title = config
        .title
        .clone()
        .or_else(|| context.and_then(|context| context.session_title.clone()))
        .unwrap_or_else(|| "Terminal".to_string());
    let mut values = captured_shell_environment(shell, &session_cwd, &home_text, &user);
    values.insert("HOME".to_string(), home_text.clone());
    values.insert("USER".to_string(), user.clone());
    values.insert("LOGNAME".to_string(), user.clone());
    values.insert("SHELL".to_string(), shell.to_string());
    values.insert("PWD".to_string(), session_cwd.clone());
    append_passthrough_env(&mut values);

    for (key, value) in configured_dotenv(&home_text) {
        values.entry(key).or_insert(value);
    }
    for (key, value) in configured_codex_env(&home_text) {
        values.entry(key).or_insert(value);
    }

    let shell_path = values.get("PATH").cloned();
    let process_path = std::env::var("PATH").ok();
    let mut path = merged_executable_path(
        shell,
        &home_text,
        &user,
        shell_path.as_deref().or(process_path.as_deref()),
        shell_path.is_none(),
    );
    let runtime_root = config
        .runtime_root
        .as_ref()
        .or_else(|| context.map(|context| &context.runtime_root));
    if let Some(runtime_root) = runtime_root {
        let wrapper_bin = runtime_root
            .join("scripts/wrappers/bin")
            .display()
            .to_string();
        path = prepend_path_component(&wrapper_bin, &path);
        values.insert("DMUX_WRAPPER_BIN".to_string(), wrapper_bin);
        if is_zsh_shell(shell) && zsh_runtime_hook_ready(runtime_root) {
            let user_zdotdir = values
                .get("ZDOTDIR")
                .cloned()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| home_text.clone());
            values.insert("DMUX_USER_ZDOTDIR".to_string(), user_zdotdir);
            values.insert(
                "ZDOTDIR".to_string(),
                runtime_root
                    .join("scripts/shell-hooks/zsh")
                    .display()
                    .to_string(),
            );
            values.insert(
                "DMUX_ZSH_HOOK_SCRIPT".to_string(),
                runtime_root
                    .join("scripts/shell-hooks/dmux-ai-hook.zsh")
                    .display()
                    .to_string(),
            );
        }
        if is_powershell_shell(shell) && powershell_runtime_hook_ready(runtime_root) {
            values.insert(
                "DMUX_PS_HOOK_SCRIPT".to_string(),
                runtime_root
                    .join("scripts/shell-hooks/dmux-ai-hook.ps1")
                    .display()
                    .to_string(),
            );
        }
    }
    if let Some(support_dir) = config
        .support_dir
        .as_ref()
        .or_else(|| context.map(|context| &context.support_dir))
    {
        values.insert(
            "DMUX_APP_SUPPORT_ROOT".to_string(),
            support_dir.display().to_string(),
        );
        values.insert(
            "CODUX_SSH_PROFILES_FILE".to_string(),
            support_dir.join("ssh_profiles.json").display().to_string(),
        );
        values.insert(
            "CODUX_DB_PROFILES_FILE".to_string(),
            support_dir.join("db_profiles.json").display().to_string(),
        );
    }
    let root_project_id = context
        .map(|context| context.root_project_id.clone())
        .or_else(|| config.project_id.clone())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| project_id.clone());
    values.insert("CODUX_DB_PROJECT_ID".to_string(), root_project_id);
    if let Some(path) = config
        .tool_permissions_file
        .as_ref()
        .or_else(|| context.and_then(|context| context.tool_permissions_file.as_ref()))
    {
        values.insert(
            "DMUX_TOOL_PERMISSION_SETTINGS_FILE".to_string(),
            path.display().to_string(),
        );
    }
    if let Some(path) = config
        .memory_workspace_root
        .as_ref()
        .or_else(|| context.and_then(|context| context.memory_workspace_root.as_ref()))
    {
        values.insert(
            "DMUX_AI_MEMORY_WORKSPACE_ROOT".to_string(),
            path.display().to_string(),
        );
    }
    if let Some(path) = config
        .memory_prompt_file
        .as_ref()
        .or_else(|| context.and_then(|context| context.memory_prompt_file.as_ref()))
    {
        values.insert(
            "DMUX_AI_MEMORY_PROMPT_FILE".to_string(),
            path.display().to_string(),
        );
    }
    if let Some(path) = config
        .memory_index_file
        .as_ref()
        .or_else(|| context.and_then(|context| context.memory_index_file.as_ref()))
    {
        values.insert(
            "DMUX_AI_MEMORY_INDEX_FILE".to_string(),
            path.display().to_string(),
        );
    }
    values.insert("PATH".to_string(), path.clone());
    values.insert("DMUX_ORIGINAL_PATH".to_string(), path);
    values.insert("TERM".to_string(), "xterm-256color".to_string());
    values.insert("COLORTERM".to_string(), "truecolor".to_string());
    values.insert("CODEX_COLOR".to_string(), "1".to_string());
    values.insert("CODUX_GPUI".to_string(), "1".to_string());
    // Default Claude Code to its classic renderer (conversation stays in the
    // terminal's native scrollback) instead of the fullscreen alternate-screen
    // TUI. The alt screen has no scrollback and does not reflow, which is what
    // makes the desktop<->mobile viewport handoff fragile (blank top rows,
    // torn keyframes). Only sets a default -- a user who exports the var
    // themselves (e.g. "0" to keep the fullscreen TUI) is respected.
    values
        .entry("CLAUDE_CODE_DISABLE_ALTERNATE_SCREEN".to_string())
        .or_insert_with(|| "1".to_string());
    values.insert(
        "LANG".to_string(),
        values.get("LANG").cloned().unwrap_or_else(default_lang),
    );
    let lang = values.get("LANG").cloned().unwrap_or_else(default_lang);
    values.entry("LC_CTYPE".to_string()).or_insert(lang);
    values.insert("DMUX_PROJECT_ID".to_string(), project_id.clone());
    values.insert("DMUX_PROJECT_NAME".to_string(), project_name.clone());
    values.insert("DMUX_PROJECT_PATH".to_string(), project_path.clone());
    values.insert("CODUX_PROJECT_ID".to_string(), project_id);
    values.insert("CODUX_PROJECT_NAME".to_string(), project_name);
    values.insert("CODUX_PROJECT_PATH".to_string(), project_path);
    values.insert("CODUX_TERMINAL_ID".to_string(), terminal_id.clone());
    values.insert("CODUX_SLOT_ID".to_string(), slot_id.clone());
    values.insert("DMUX_SESSION_ID".to_string(), terminal_id.clone());
    values.insert("DMUX_TERMINAL_ID".to_string(), terminal_id);
    values.insert("DMUX_SLOT_ID".to_string(), slot_id);
    values.insert("DMUX_SESSION_KEY".to_string(), session_key);
    values.insert("DMUX_SESSION_TITLE".to_string(), session_title);
    values.insert("DMUX_SESSION_CWD".to_string(), session_cwd);
    values.insert(
        "DMUX_SESSION_INSTANCE_ID".to_string(),
        config
            .session_instance_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.session_instance_id.clone()))
            .unwrap_or_else(|| Uuid::new_v4().to_string().to_lowercase()),
    );
    values.insert(
        "DMUX_RUNTIME_OWNER".to_string(),
        crate::runtime_paths::app_slug().to_string(),
    );
    values.insert(
        "DMUX_RUNTIME_EVENT_DIR".to_string(),
        crate::runtime_paths::runtime_event_dir()
            .display()
            .to_string(),
    );
    values.insert(
        "DMUX_LOG_FILE".to_string(),
        crate::runtime_paths::live_log_path().display().to_string(),
    );
    values.insert(
        "DMUX_CLAUDE_SESSION_MAP_DIR".to_string(),
        crate::runtime_paths::claude_session_map_dir()
            .display()
            .to_string(),
    );
    values.insert(
        "DMUX_OPENCODE_SESSION_MAP_DIR".to_string(),
        crate::runtime_paths::opencode_session_map_dir()
            .display()
            .to_string(),
    );
    values.insert(
        "DMUX_AI_RUNTIME_BINDING_DIR".to_string(),
        crate::runtime_paths::ai_runtime_binding_dir()
            .display()
            .to_string(),
    );

    if let Some(overrides) = &config.env {
        for (key, value) in overrides {
            values.insert(key.clone(), value.clone());
        }
    }
    ensure_utf8_locale(&mut values);
    values
}

pub(super) fn ensure_utf8_locale(values: &mut HashMap<String, String>) {
    let fallback = default_lang();
    if !is_utf8_locale(values.get("LANG").map(String::as_str)) {
        values.insert("LANG".to_string(), fallback.clone());
    }

    for key in ["LC_ALL", "LC_CTYPE"] {
        if values
            .get(key)
            .is_some_and(|value| !is_utf8_locale(Some(value.as_str())))
        {
            values.insert(key.to_string(), fallback.clone());
        }
    }

    values.entry("LC_CTYPE".to_string()).or_insert(fallback);
}

pub(super) fn is_utf8_locale(value: Option<&str>) -> bool {
    value
        .map(|value| {
            let normalized = value.trim().to_ascii_lowercase().replace('-', "");
            normalized.contains("utf8")
        })
        .unwrap_or(false)
}

pub(super) fn append_passthrough_env(values: &mut HashMap<String, String>) {
    append_env_keys(values, COMMON_PASSTHROUGH_ENV_KEYS);
    #[cfg(unix)]
    append_env_keys(values, UNIX_PASSTHROUGH_ENV_KEYS);
    #[cfg(windows)]
    append_env_keys(values, WINDOWS_PASSTHROUGH_ENV_KEYS);
}

pub(super) fn append_env_keys(values: &mut HashMap<String, String>, keys: &[&str]) {
    for key in keys {
        if let Ok(value) = std::env::var(key)
            && !value.is_empty()
        {
            values.entry((*key).to_string()).or_insert(value);
        }
    }
}

pub(super) fn captured_shell_environment(
    shell: &str,
    cwd: &str,
    home: &str,
    user: &str,
) -> HashMap<String, String> {
    #[cfg(windows)]
    {
        let _ = shell;
        let _ = cwd;
        let _ = home;
        let _ = user;
        HashMap::new()
    }

    #[cfg(not(windows))]
    {
        static CACHE: OnceLock<parking_lot::Mutex<HashMap<String, HashMap<String, String>>>> =
            OnceLock::new();
        let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
        let key = format!("{shell}|{cwd}|{home}|{user}");
        if let Some(value) = cache.lock().get(&key) {
            return value.clone();
        }
        match capture_shell_environment_uncached(shell, cwd, home, user) {
            // Only cache a healthy capture — a real login env always carries
            // PATH. A momentary failure (e.g. the project's external drive
            // blipped, so `cd <cwd>` failed and no env was emitted) must NOT be
            // cached for the whole process lifetime, or codex stays "command not
            // found" even after the drive is back. Leaving it uncached lets the
            // next launch retry and self-heal once the path returns.
            Some(value) if value.contains_key("PATH") => {
                cache.lock().insert(key, value.clone());
                value
            }
            Some(value) => value,
            None => HashMap::new(),
        }
    }
}

pub(super) fn configured_dotenv(home: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let allowed = DOTENV_KEYS
        .iter()
        .copied()
        .collect::<std::collections::HashSet<_>>();
    for path in dotenv_paths(home) {
        let Ok(text) = fs::read_to_string(path) else {
            continue;
        };
        for raw_line in text.lines() {
            let mut line = raw_line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            if let Some(value) = line.strip_prefix("export ") {
                line = value.trim();
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            if !allowed.contains(key) || values.contains_key(key) {
                continue;
            }
            values.insert(key.to_string(), unquote_env_value(value.trim()));
        }
    }
    values
}

pub(super) fn configured_codex_env(home: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let codex_home = Path::new(home).join(".codex");
    if let Ok(text) = fs::read_to_string(codex_home.join("auth.json")) {
        for (key, value) in codex_auth_env_from_text(&text) {
            values.entry(key).or_insert(value);
        }
    }
    if let Ok(text) = fs::read_to_string(codex_home.join("config.toml")) {
        for (key, value) in codex_config_env_from_text(&text) {
            values.entry(key).or_insert(value);
        }
    }
    values
}

pub(super) fn codex_auth_env_from_text(text: &str) -> HashMap<String, String> {
    let mut values = HashMap::new();
    let Ok(json) = serde_json::from_str::<serde_json::Value>(text) else {
        return values;
    };
    if let Some(api_key) = json
        .get("OPENAI_API_KEY")
        .and_then(|value| value.as_str())
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        values.insert("OPENAI_API_KEY".to_string(), api_key.to_string());
    }
    values
}

pub(super) fn codex_config_env_from_text(text: &str) -> HashMap<String, String> {
    let mut root = HashMap::new();
    let mut providers: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut profiles: HashMap<String, HashMap<String, String>> = HashMap::new();
    let mut section = Vec::<String>::new();
    for raw_line in text.lines() {
        let line = strip_toml_comment(raw_line).trim().to_string();
        if line.is_empty() {
            continue;
        }
        if let Some(next_section) = parse_toml_section(&line) {
            section = next_section;
            continue;
        }
        let Some((key, value)) = parse_toml_string_assignment(&line) else {
            continue;
        };
        match section.as_slice() {
            [] => {
                root.insert(key, value);
            }
            [table, provider] if table == "model_providers" => {
                providers
                    .entry(provider.clone())
                    .or_default()
                    .insert(key, value);
            }
            [table, profile] if table == "profiles" => {
                profiles
                    .entry(profile.clone())
                    .or_default()
                    .insert(key, value);
            }
            _ => {}
        }
    }
    let active_profile = root.get("profile").and_then(|name| profiles.get(name));
    let active_provider = active_profile
        .and_then(|profile| profile.get("model_provider"))
        .or_else(|| root.get("model_provider"))
        .map(String::as_str)
        .unwrap_or("openai");
    let base_url = providers
        .get(active_provider)
        .and_then(|provider| provider.get("base_url"))
        .or_else(|| active_profile.and_then(|profile| profile.get("openai_base_url")))
        .or_else(|| root.get("openai_base_url"))
        .map(String::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let mut values = HashMap::new();
    if let Some(base_url) = base_url {
        values.insert("OPENAI_BASE_URL".to_string(), base_url.to_string());
    }
    values
}

pub(super) fn strip_toml_comment(line: &str) -> &str {
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut escaped = false;
    for (index, ch) in line.char_indices() {
        if escaped {
            escaped = false;
            continue;
        }
        match ch {
            '\\' if in_double_quote => escaped = true,
            '"' if !in_single_quote => in_double_quote = !in_double_quote,
            '\'' if !in_double_quote => in_single_quote = !in_single_quote,
            '#' if !in_single_quote && !in_double_quote => return &line[..index],
            _ => {}
        }
    }
    line
}

pub(super) fn parse_toml_section(line: &str) -> Option<Vec<String>> {
    let name = line.strip_prefix('[')?.strip_suffix(']')?.trim();
    if name.is_empty() {
        return None;
    }
    Some(
        name.split('.')
            .map(|part| unquote_env_value(part.trim()))
            .filter(|part| !part.is_empty())
            .collect(),
    )
}

pub(super) fn parse_toml_string_assignment(line: &str) -> Option<(String, String)> {
    let (key, raw_value) = line.split_once('=')?;
    let key = key.trim();
    if key.is_empty() {
        return None;
    }
    Some((key.to_string(), parse_toml_string(raw_value.trim())?))
}

pub(super) fn parse_toml_string(value: &str) -> Option<String> {
    if value.len() < 2 {
        return None;
    }
    let bytes = value.as_bytes();
    match (bytes[0], bytes[value.len() - 1]) {
        (b'"', b'"') => serde_json::from_str::<String>(value).ok(),
        (b'\'', b'\'') => Some(value[1..value.len() - 1].to_string()),
        _ => None,
    }
}

pub(super) fn dotenv_paths(home: &str) -> Vec<PathBuf> {
    [
        ".gemini/.env",
        ".claude/.env",
        ".codex/.env",
        ".opencode/.env",
        ".config/opencode/.env",
        ".codewhale/.env",
    ]
    .iter()
    .map(|path| Path::new(home).join(path))
    .collect()
}

pub(super) fn unquote_env_value(value: &str) -> String {
    if value.len() >= 2 {
        let bytes = value.as_bytes();
        if (bytes[0] == b'"' && bytes[value.len() - 1] == b'"')
            || (bytes[0] == b'\'' && bytes[value.len() - 1] == b'\'')
        {
            return value[1..value.len() - 1].to_string();
        }
    }
    value.to_string()
}
