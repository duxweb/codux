use crate::ai_runtime::tool_driver::{AIRuntimeMemoryInjectionDriver, runtime_tool_driver};
use serde_json::{Value, json};
use std::{env, fs, path::Path};

pub fn handle_args(args: &[String]) -> Result<bool, String> {
    let Some(command) = args.first().map(String::as_str) else {
        return Ok(false);
    };
    if command != "--codux-wrapper-helper" {
        return Ok(false);
    }
    let Some(subcommand) = args.get(1).map(String::as_str) else {
        return Err("wrapper helper missing subcommand".to_string());
    };
    match subcommand {
        "tool-memory-injection" => print_tool_memory_injection(),
        "json-string-key" => print_json_string_key(),
        "codex-effort" => print_codex_effort(),
        "toml-string" => print_toml_string(),
        "opencode-session-state" => print_opencode_session_state(),
        "hook-notification-type" => print_hook_notification_type(),
        "hook-session-id" => print_hook_session_id(),
        "hook-field" => print_hook_field(),
        "hook-first-field" => print_hook_first_field(),
        "hook-number-field" => print_hook_number_field(),
        "claude-memory-context" => print_claude_memory_context(),
        "ssh-list-profiles" => print_ssh_profiles(),
        "ssh-profile-shell" => print_ssh_profile_shell(),
        _ => return Err(format!("unknown wrapper helper subcommand: {subcommand}")),
    }?;
    Ok(true)
}

fn print_tool_memory_injection() -> Result<(), String> {
    let tool = env_value("TOOL_NAME").to_ascii_lowercase();
    if tool.is_empty() {
        return Ok(());
    }
    if let Some(strategy) = tool_memory_injection_strategy(&tool) {
        println!("{strategy}");
    }
    Ok(())
}

fn tool_memory_injection_strategy(tool: &str) -> Option<&'static str> {
    let driver = runtime_tool_driver(tool)?;
    match driver.memory_injection {
        AIRuntimeMemoryInjectionDriver::None => None,
        AIRuntimeMemoryInjectionDriver::CodexDeveloperInstructions => {
            Some("codexDeveloperInstructions")
        }
        AIRuntimeMemoryInjectionDriver::ClaudeAppendSystemPrompt => {
            Some("claudeAppendSystemPrompt")
        }
    }
}

fn print_json_string_key() -> Result<(), String> {
    let path = env_value("CONFIG_PATH");
    let key = env_value("CONFIG_KEY");
    if path.is_empty() || key.is_empty() {
        return Ok(());
    }
    if let Some(value) = read_json_file(&path)
        .and_then(|root| root.get(&key).and_then(Value::as_str).map(str::to_string))
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        println!("{value}");
    }
    Ok(())
}

fn print_codex_effort() -> Result<(), String> {
    let path = env_value("CONFIG_PATH");
    if path.is_empty() {
        return Ok(());
    }
    let allowed = ["none", "minimal", "low", "medium", "high", "xhigh"];
    if let Some(value) = read_json_file(&path)
        .and_then(|root| {
            root.get("codexEffort")
                .and_then(Value::as_str)
                .map(str::to_string)
        })
        .map(|value| value.trim().to_string())
        .filter(|value| allowed.contains(&value.as_str()))
    {
        println!("{value}");
    }
    Ok(())
}

fn print_toml_string() -> Result<(), String> {
    let value = env::var("VALUE").unwrap_or_default();
    println!(
        "{}",
        serde_json::to_string(&value).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn print_opencode_session_state() -> Result<(), String> {
    let path = env_value("OPENCODE_STATE_PATH");
    if path.is_empty() {
        return Ok(());
    }
    let Some(root) = read_json_file(&path) else {
        return Ok(());
    };
    if let Some(external) = root.get("externalSessionID").and_then(Value::as_str) {
        if !external.is_empty() {
            println!("{external}");
        }
    }
    if let Some(model) = root.get("model").and_then(Value::as_str) {
        if !model.is_empty() {
            println!("{model}");
        }
    }
    Ok(())
}

fn print_hook_notification_type() -> Result<(), String> {
    let Some(root) = hook_payload() else {
        return Ok(());
    };
    if let Some(value) = notification_type(&root) {
        println!("{value}");
    }
    Ok(())
}

fn print_hook_session_id() -> Result<(), String> {
    let Some(root) = hook_payload() else {
        return Ok(());
    };
    if let Some(value) = find_first_string_recursive(&root, &["session_id", "sessionId"]) {
        println!("{value}");
    }
    Ok(())
}

fn print_hook_field() -> Result<(), String> {
    let field = env_value("HOOK_FIELD_NAME");
    if field.is_empty() {
        return Ok(());
    }
    let Some(root) = hook_payload() else {
        return Ok(());
    };
    if let Some(value) = find_first_string_recursive(&root, &[field.as_str()]) {
        println!("{value}");
    }
    Ok(())
}

fn print_hook_first_field() -> Result<(), String> {
    let fields = env_list("HOOK_FIELD_NAMES");
    if fields.is_empty() {
        return Ok(());
    }
    let Some(root) = hook_payload() else {
        return Ok(());
    };
    let field_refs = fields.iter().map(String::as_str).collect::<Vec<_>>();
    if let Some(value) = find_first_string_recursive(&root, &field_refs) {
        println!("{value}");
    }
    Ok(())
}

fn print_hook_number_field() -> Result<(), String> {
    let fields = env_list("HOOK_FIELD_NAMES");
    if fields.is_empty() {
        return Ok(());
    }
    let Some(root) = hook_payload() else {
        return Ok(());
    };
    let field_refs = fields.iter().map(String::as_str).collect::<Vec<_>>();
    if let Some(value) = find_first_integer_recursive(&root, &field_refs) {
        println!("{value}");
    }
    Ok(())
}

fn print_claude_memory_context() -> Result<(), String> {
    let path = env_value("MEMORY_INDEX_FILE");
    if path.is_empty() {
        return Ok(());
    }
    let Ok(text) = fs::read_to_string(&path) else {
        return Ok(());
    };
    let text = text.trim();
    if text.is_empty() {
        return Ok(());
    }
    let prefix = format!(
        "Codux memory refresh: the conversation may have been compacted, or this is a new user turn. Re-apply relevant durable memory below. Prefer current user instructions and repository state over stale memory. Memory index file: {path}\n\n"
    );
    let suffix = "\n[Codux memory refresh truncated]";
    let mut payload = format!("{prefix}{text}");
    if payload.len() > 9500 {
        truncate_at_char_boundary(&mut payload, 9500usize.saturating_sub(suffix.len()));
        payload.push_str(suffix);
    }
    let event = env_value("CLAUDE_HOOK_EVENT_NAME");
    let output = json!({
        "hookSpecificOutput": {
            "hookEventName": if event.is_empty() { "UserPromptSubmit" } else { event.as_str() },
            "additionalContext": payload,
        },
        "suppressOutput": true,
    });
    println!(
        "{}",
        serde_json::to_string(&output).map_err(|error| error.to_string())?
    );
    Ok(())
}

fn print_ssh_profiles() -> Result<(), String> {
    let path = env_value("CODUX_SSH_PROFILES_FILE");
    let root = read_json_file(&path)
        .ok_or_else(|| "codux-ssh: failed to read SSH profiles".to_string())?;
    let profiles = ssh_profiles_array(&root)
        .ok_or_else(|| "codux-ssh: invalid SSH profile file".to_string())?;
    let public_profiles = profiles
        .iter()
        .filter_map(public_ssh_profile)
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "profiles": public_profiles }))
            .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn print_ssh_profile_shell() -> Result<(), String> {
    let profile_id = env_value("CODUX_SSH_PROFILE_ID").to_ascii_lowercase();
    let path = env_value("CODUX_SSH_PROFILES_FILE");
    let root = read_json_file(&path)
        .ok_or_else(|| "codux-ssh: failed to read SSH profiles".to_string())?;
    let profiles = ssh_profiles_array(&root)
        .ok_or_else(|| "codux-ssh: invalid SSH profile file".to_string())?;
    let profile = profiles
        .iter()
        .find(|profile| string_field(profile, "id").to_ascii_lowercase() == profile_id)
        .ok_or_else(|| "codux-ssh: SSH profile not found".to_string())?;
    let host = string_field(profile, "host");
    let username = string_field(profile, "username");
    if host.is_empty() || username.is_empty() {
        return Err("codux-ssh: SSH profile is missing host or username".to_string());
    }
    let port = port_field(profile);
    let credential_kind = string_field(profile, "credentialKind");
    let private_key_path = string_field(profile, "privateKeyPath");
    let password = if credential_kind == "password" {
        string_field(profile, "password")
    } else {
        String::new()
    };
    let key_passphrase = if credential_kind == "privateKey" {
        string_field(profile, "keyPassphrase")
    } else {
        String::new()
    };
    let mut ssh_args = vec!["ssh".to_string(), "-p".to_string(), port.to_string()];
    if let Some(control_path) = ssh_control_path(&profile_id) {
        ssh_args.extend([
            "-o".to_string(),
            "ControlMaster=auto".to_string(),
            "-o".to_string(),
            format!("ControlPath={control_path}"),
            "-o".to_string(),
            "ControlPersist=300".to_string(),
        ]);
    }
    if credential_kind == "privateKey" && !private_key_path.is_empty() {
        ssh_args.push("-i".to_string());
        ssh_args.push(expand_home(&private_key_path));
    }
    ssh_args.push(format!("{username}@{host}"));
    println!("ssh_password={}", shell_quote(&password));
    println!("ssh_key_passphrase={}", shell_quote(&key_passphrase));
    println!(
        "ssh_args=({})",
        ssh_args
            .iter()
            .map(|value| shell_quote(value))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Ok(())
}

fn ssh_control_path(profile_id: &str) -> Option<String> {
    let socket_name = format!("cxs-{:016x}", stable_hash64(profile_id.as_bytes()));
    #[cfg(target_os = "macos")]
    let base_dir = env::var("TMPDIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "/tmp".to_string());
    #[cfg(not(target_os = "macos"))]
    let base_dir = env::var("XDG_RUNTIME_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .or_else(|| {
            env::var("TMPDIR")
                .ok()
                .filter(|value| !value.trim().is_empty())
        })
        .unwrap_or_else(|| "/tmp".to_string());
    let dir = Path::new(&base_dir).join("codux-ssh");
    if fs::create_dir_all(&dir).is_err() {
        return None;
    }
    Some(dir.join(socket_name).display().to_string())
}

fn stable_hash64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    hash
}

fn hook_payload() -> Option<Value> {
    serde_json::from_str(&env_value("HOOK_PAYLOAD")).ok()
}

fn notification_type(root: &Value) -> Option<String> {
    first_string(root, &["notification_type"])
        .or_else(|| {
            first_string(
                root.get("notification")?,
                &["notification_type", "type", "kind", "reason"],
            )
        })
        .or_else(|| {
            first_string(
                root.get("data")?,
                &["notification_type", "type", "kind", "reason"],
            )
        })
}

fn first_string(value: &Value, keys: &[&str]) -> Option<String> {
    let object = value.as_object()?;
    for key in keys {
        if let Some(value) = object.get(*key).and_then(Value::as_str) {
            if !value.is_empty() {
                return Some(value.to_string());
            }
        }
    }
    None
}

fn find_first_string_recursive(root: &Value, keys: &[&str]) -> Option<String> {
    let mut stack = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(object) => {
                for key in keys {
                    if let Some(value) = object.get(*key).and_then(Value::as_str) {
                        if !value.is_empty() {
                            return Some(value.to_string());
                        }
                    }
                }
                stack.extend(object.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    None
}

fn find_first_integer_recursive(root: &Value, keys: &[&str]) -> Option<i64> {
    let mut stack = vec![root];
    while let Some(value) = stack.pop() {
        match value {
            Value::Object(object) => {
                for key in keys {
                    let Some(value) = object.get(*key) else {
                        continue;
                    };
                    if let Some(number) = value.as_i64() {
                        return Some(number);
                    }
                    if let Some(number) = value.as_f64() {
                        if number.is_finite() && number.fract() == 0.0 {
                            return Some(number as i64);
                        }
                    }
                }
                stack.extend(object.values());
            }
            Value::Array(items) => stack.extend(items),
            _ => {}
        }
    }
    None
}

fn read_json_file(path: &str) -> Option<Value> {
    let text = fs::read_to_string(path).ok()?;
    serde_json::from_str(&text).ok()
}

fn truncate_at_char_boundary(value: &mut String, max_len: usize) {
    if value.len() <= max_len {
        return;
    }
    let mut cut = max_len.min(value.len());
    while cut > 0 && !value.is_char_boundary(cut) {
        cut -= 1;
    }
    value.truncate(cut);
}

fn env_value(key: &str) -> String {
    env::var(key).unwrap_or_default()
}

fn env_list(key: &str) -> Vec<String> {
    env_value(key)
        .split_whitespace()
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect()
}

fn ssh_profiles_array(root: &Value) -> Option<&Vec<Value>> {
    root.get("sshProfiles")
        .and_then(Value::as_array)
        .or_else(|| root.as_array())
}

fn public_ssh_profile(profile: &Value) -> Option<Value> {
    let profile_id = string_field(profile, "id");
    let host = string_field(profile, "host");
    let username = string_field(profile, "username");
    if profile_id.is_empty() || host.is_empty() || username.is_empty() {
        return None;
    }
    let port = port_field(profile);
    let name = {
        let value = string_field(profile, "name");
        if value.is_empty() {
            format!("{username}@{host}")
        } else {
            value
        }
    };
    let credential = {
        let value = string_field(profile, "credentialKind");
        if value.is_empty() {
            "none".to_string()
        } else {
            value
        }
    };
    Some(json!({
        "id": profile_id,
        "name": name,
        "host": host,
        "port": port,
        "username": username,
        "endpoint": format!("{username}@{host}:{port}"),
        "credential": credential,
    }))
}

fn string_field(value: &Value, key: &str) -> String {
    value
        .get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim()
        .to_string()
}

fn port_field(profile: &Value) -> u16 {
    let raw = profile
        .get("port")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
        .unwrap_or(22);
    raw.clamp(1, 65535) as u16
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return env_value("HOME");
    }
    if let Some(rest) = path.strip_prefix("~/") {
        let home = env_value("HOME");
        if !home.is_empty() {
            return Path::new(&home).join(rest).display().to_string();
        }
    }
    path.to_string()
}

fn shell_quote(value: &str) -> String {
    if value.is_empty() {
        return "''".to_string();
    }
    if value.chars().all(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.' | '/' | ':' | '@' | '=')
    }) {
        return value.to_string();
    }
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn handle_args_ignores_non_helper_invocations() {
        assert!(!handle_args(&["--version".to_string()]).unwrap());
        assert!(!handle_args(&[]).unwrap());
    }

    #[test]
    fn handle_args_rejects_missing_or_unknown_subcommands() {
        assert!(handle_args(&["--codux-wrapper-helper".to_string()]).is_err());
        assert!(
            handle_args(&[
                "--codux-wrapper-helper".to_string(),
                "missing-command".to_string()
            ])
            .is_err()
        );
    }

    #[test]
    fn tool_memory_injection_strategy_uses_runtime_driver_registry() {
        assert_eq!(
            tool_memory_injection_strategy("codex"),
            Some("codexDeveloperInstructions")
        );
        assert_eq!(
            tool_memory_injection_strategy("claude-code"),
            Some("claudeAppendSystemPrompt")
        );
        assert_eq!(tool_memory_injection_strategy("codewhale"), None);
        assert_eq!(tool_memory_injection_strategy("unknown"), None);
    }

    #[test]
    fn truncate_at_char_boundary_keeps_valid_utf8() {
        let mut value = "prefix-中文🙂suffix".repeat(800);
        truncate_at_char_boundary(&mut value, 9467);
        assert!(value.len() <= 9467);
        assert!(std::str::from_utf8(value.as_bytes()).is_ok());
    }

    #[test]
    fn shell_quote_handles_spaces_and_single_quotes() {
        assert_eq!(shell_quote("simple/path"), "simple/path");
        assert_eq!(shell_quote("a b'id"), "'a b'\\''id'");
        assert_eq!(shell_quote(""), "''");
    }

    #[test]
    fn recursive_number_field_ignores_bool_values() {
        let value = serde_json::json!({
            "outer": {
                "total_tokens": true,
                "nested": {
                    "total_tokens": 42,
                },
            },
        });
        assert_eq!(
            find_first_integer_recursive(&value, &["total_tokens"]),
            Some(42)
        );
    }

    #[test]
    fn ssh_control_path_is_stable_and_safe() {
        let path = ssh_control_path("profile with spaces").expect("control path");
        assert!(path.contains("cxs-"));
        assert!(!path.contains(' '));
        assert!(!path.contains("%r"));
    }

    #[test]
    fn ssh_control_path_stays_below_macos_socket_limit_for_uuid_profiles() {
        let path = ssh_control_path("123e4567-e89b-12d3-a456-426614174000").expect("control path");
        assert!(
            path.len() < 104,
            "ControlPath must fit macOS sockaddr_un.sun_path: {path}"
        );
    }
}
