use crate::ai_runtime::tool_driver::{AIRuntimeMemoryInjectionDriver, runtime_tool_driver};
use percent_encoding::{AsciiSet, CONTROLS, utf8_percent_encode};
use serde_json::{Value, json};
use sqlx::{Any, AnyConnection, Column, Connection, Row, TypeInfo, ValueRef, any::AnyRow, query};
use std::{env, fs, path::Path, time::Duration};

const DB_QUERY_TIMEOUT_SECONDS: u64 = 15;
const DB_QUERY_MAX_ROWS: usize = 100;
const DB_QUERY_MAX_CELL_CHARS: usize = 240;
const DB_URL_ENCODE_SET: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'%')
    .add(b'<')
    .add(b'>')
    .add(b'?')
    .add(b'`')
    .add(b'{')
    .add(b'}')
    .add(b'/')
    .add(b':')
    .add(b'@')
    .add(b'[')
    .add(b']')
    .add(b'\\')
    .add(b'^')
    .add(b'|');

pub fn handle_args(args: &[String]) -> Result<bool, String> {
    // ssh execs $SSH_ASKPASS (always the staged helper binary) with the prompt
    // as argv; only that binary answers, so a leaked askpass env var can never
    // hijack a desktop or agent launch.
    if args.first().map(String::as_str) != Some("--codux-wrapper-helper")
        && env_value("CODUX_WRAPPER_HELPER_ASKPASS") == "1"
        && invoked_as_wrapper_helper()
    {
        print_ssh_askpass(args)?;
        return Ok(true);
    }
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
        "scp-profile-shell" => print_scp_profile_shell(),
        "ssh-askpass" => print_ssh_askpass(&args[2..]),
        "db-list-profiles" => print_db_profiles(),
        "db-query" => print_db_query(),
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
        AIRuntimeMemoryInjectionDriver::AppendSystemPrompt => Some("appendSystemPrompt"),
        AIRuntimeMemoryInjectionDriver::OpenCodeSystemTransform => Some("opencodeSystemTransform"),
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
    if let Some(external) = root.get("externalSessionID").and_then(Value::as_str)
        && !external.is_empty()
    {
        println!("{external}");
    }
    if let Some(model) = root.get("model").and_then(Value::as_str)
        && !model.is_empty()
    {
        println!("{model}");
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

struct ResolvedSshProfile {
    host: String,
    username: String,
    port: u16,
    credential_kind: String,
    private_key_path: String,
    password: String,
    key_passphrase: String,
}

fn resolve_ssh_profile() -> Result<(String, ResolvedSshProfile), String> {
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
    let credential_kind = string_field(profile, "credentialKind");
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
    Ok((
        profile_id,
        ResolvedSshProfile {
            host,
            username,
            port: port_field(profile),
            credential_kind,
            private_key_path: string_field(profile, "privateKeyPath"),
            password,
            key_passphrase,
        },
    ))
}

// Accept a new host key without the first-connect yes/no prompt (still rejects a CHANGED key), and bound the dial — a headless caller has no TTY to answer either.
fn ssh_hardening_options() -> [String; 4] {
    [
        "-o".to_string(),
        "StrictHostKeyChecking=accept-new".to_string(),
        "-o".to_string(),
        "ConnectTimeout=15".to_string(),
    ]
}

fn ssh_password_auth_options() -> [String; 6] {
    [
        "-o".to_string(),
        // keyboard-interactive keeps PAM-only servers reachable; both prompt
        // kinds reach the expect flow (unix) or askpass (windows) the same way.
        "PreferredAuthentications=password,keyboard-interactive".to_string(),
        "-o".to_string(),
        "PubkeyAuthentication=no".to_string(),
        "-o".to_string(),
        "NumberOfPasswordPrompts=1".to_string(),
    ]
}

fn ssh_identities_only_options() -> [String; 2] {
    ["-o".to_string(), "IdentitiesOnly=yes".to_string()]
}

fn print_ssh_profile_shell() -> Result<(), String> {
    let (profile_id, profile) = resolve_ssh_profile()?;
    let mut ssh_args = vec![
        "ssh".to_string(),
        "-p".to_string(),
        profile.port.to_string(),
    ];
    ssh_args.extend(ssh_hardening_options());
    if profile.credential_kind == "password" {
        ssh_args.extend(ssh_password_auth_options());
    }
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
    if profile.credential_kind == "privateKey" && !profile.private_key_path.is_empty() {
        ssh_args.extend(ssh_identities_only_options());
        ssh_args.push("-i".to_string());
        ssh_args.push(expand_home(&profile.private_key_path));
    }
    ssh_args.push(format!("{}@{}", profile.username, profile.host));
    println!("ssh_password={}", shell_quote(&profile.password));
    println!(
        "ssh_key_passphrase={}",
        shell_quote(&profile.key_passphrase)
    );
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

fn print_scp_profile_shell() -> Result<(), String> {
    let (_profile_id, profile) = resolve_ssh_profile()?;
    // scp takes the port as -P (uppercase); the host is a path prefix, not a trailing arg.
    let mut scp_args = vec![
        "scp".to_string(),
        "-P".to_string(),
        profile.port.to_string(),
    ];
    scp_args.extend(ssh_hardening_options());
    if profile.credential_kind == "password" {
        scp_args.extend(ssh_password_auth_options());
    }
    if profile.credential_kind == "privateKey" && !profile.private_key_path.is_empty() {
        scp_args.extend(ssh_identities_only_options());
        scp_args.push("-i".to_string());
        scp_args.push(expand_home(&profile.private_key_path));
    }
    println!("ssh_password={}", shell_quote(&profile.password));
    println!(
        "ssh_key_passphrase={}",
        shell_quote(&profile.key_passphrase)
    );
    println!(
        "ssh_remote={}",
        shell_quote(&format!("{}@{}", profile.username, profile.host))
    );
    println!(
        "scp_args=({})",
        scp_args
            .iter()
            .map(|value| shell_quote(value))
            .collect::<Vec<_>>()
            .join(" ")
    );
    Ok(())
}

fn invoked_as_wrapper_helper() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|path| {
            path.file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
        })
        .is_some_and(|stem| is_wrapper_helper_exe_name(&stem))
}

fn is_wrapper_helper_exe_name(name: &str) -> bool {
    name.eq_ignore_ascii_case("codux-wrapper-helper")
}

fn print_ssh_askpass(args: &[String]) -> Result<(), String> {
    let prompt = args.join(" ");
    let password = env_value("CODUX_SSH_PASSWORD");
    let key_passphrase = env_value("CODUX_SSH_KEY_PASSPHRASE");
    let Some(response) = ssh_askpass_response(&prompt, &password, &key_passphrase) else {
        return Err("codux-ssh: no saved credential matches the SSH prompt".to_string());
    };
    println!("{response}");
    Ok(())
}

fn ssh_askpass_response<'a>(
    prompt: &str,
    password: &'a str,
    key_passphrase: &'a str,
) -> Option<&'a str> {
    let prompt = prompt.to_ascii_lowercase();
    if prompt.contains("passphrase") && !key_passphrase.is_empty() {
        return Some(key_passphrase);
    }
    if prompt.contains("password") && !password.is_empty() {
        return Some(password);
    }
    None
}

fn print_db_profiles() -> Result<(), String> {
    let path = env_value("CODUX_DB_PROFILES_FILE");
    let project_id = env_value("CODUX_DB_PROJECT_ID");
    let root = read_json_file(&path)
        .ok_or_else(|| "codux-db: failed to read database profiles".to_string())?;
    let profiles = db_profiles_array(&root)
        .ok_or_else(|| "codux-db: invalid database profile file".to_string())?;
    let public_profiles = profiles
        .iter()
        .filter_map(|profile| public_db_profile(profile, &project_id))
        .collect::<Vec<_>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&json!({ "profiles": public_profiles }))
            .map_err(|error| error.to_string())?
    );
    Ok(())
}

fn print_db_query() -> Result<(), String> {
    let profile = selected_db_profile()?;
    let statement = db_statement()?;
    let output_json = env_value("CODUX_DB_OUTPUT_JSON") == "true";
    let result = run_db_query(&profile, &statement)?;
    if output_json {
        println!(
            "{}",
            serde_json::to_string_pretty(&result).map_err(|error| error.to_string())?
        );
    } else {
        print_db_query_table(&result);
    }
    Ok(())
}

#[cfg(windows)]
fn ssh_control_path(_profile_id: &str) -> Option<String> {
    None
}

#[cfg(not(windows))]
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

#[cfg(not(windows))]
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
        if let Some(value) = object.get(*key).and_then(Value::as_str)
            && !value.is_empty()
        {
            return Some(value.to_string());
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
                    if let Some(value) = object.get(*key).and_then(Value::as_str)
                        && !value.is_empty()
                    {
                        return Some(value.to_string());
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
                    if let Some(number) = value.as_f64()
                        && number.is_finite()
                        && number.fract() == 0.0
                    {
                        return Some(number as i64);
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

fn ssh_profiles_array(root: &Value) -> Option<&[Value]> {
    if root.is_null() {
        return Some(&[]);
    }
    root.get("sshProfiles")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .or_else(|| root.as_array().map(Vec::as_slice))
}

fn db_profiles_array(root: &Value) -> Option<&[Value]> {
    if root.is_null() {
        return Some(&[]);
    }
    root.get("dbProfiles")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .or_else(|| root.as_array().map(Vec::as_slice))
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

fn public_db_profile(profile: &Value, project_id: &str) -> Option<Value> {
    let profile_project_id = string_field(profile, "projectId");
    if profile_project_id.is_empty() || profile_project_id != project_id {
        return None;
    }
    let profile_id = string_field(profile, "id");
    let engine = string_field(profile, "engine");
    let database = string_field(profile, "database");
    if profile_id.is_empty() || engine.is_empty() || database.is_empty() {
        return None;
    }
    let host = string_field(profile, "host");
    let port = db_port_field(profile);
    let name = {
        let value = string_field(profile, "name");
        if value.is_empty() {
            format!("{engine} · {database}")
        } else {
            value
        }
    };
    let endpoint = if engine == "sqlite" {
        database.clone()
    } else {
        format!("{host}:{port}/{database}")
    };
    Some(json!({
        "id": profile_id,
        "name": name,
        "engine": engine,
        "database": database,
        "endpoint": endpoint,
        "readOnly": profile.get("readOnly").and_then(Value::as_bool).unwrap_or(false),
    }))
}

fn selected_db_profile() -> Result<Value, String> {
    let profile_id = env_value("CODUX_DB_PROFILE_ID");
    let project_id = env_value("CODUX_DB_PROJECT_ID");
    if profile_id.trim().is_empty() {
        return Err("codux-db: missing database profile id".to_string());
    }
    if project_id.trim().is_empty() {
        return Err("codux-db: missing Codux project context".to_string());
    }
    let path = env_value("CODUX_DB_PROFILES_FILE");
    let root = read_json_file(&path)
        .ok_or_else(|| "codux-db: failed to read database profiles".to_string())?;
    let profiles = db_profiles_array(&root)
        .ok_or_else(|| "codux-db: invalid database profile file".to_string())?;
    profiles
        .iter()
        .find(|profile| {
            string_field(profile, "id") == profile_id
                && string_field(profile, "projectId") == project_id
        })
        .cloned()
        .ok_or_else(|| "codux-db: database profile not found for this project".to_string())
}

fn db_statement() -> Result<String, String> {
    let statement = env_value("CODUX_DB_STATEMENT");
    let statement = statement.trim();
    if statement.is_empty() {
        return Err("codux-db: missing SQL statement".to_string());
    }
    Ok(statement.to_string())
}

fn run_db_query(profile: &Value, statement: &str) -> Result<Value, String> {
    if !statement_allowed(profile, statement) {
        return Err("codux-db: read-only database profile only allows SELECT/SHOW/WITH/EXPLAIN/PRAGMA statements".to_string());
    }
    let url = db_connection_url(profile)?;
    let read_only = profile
        .get("readOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let engine = string_field(profile, "engine").to_ascii_lowercase();
    let future = async move {
        sqlx::any::install_default_drivers();
        let mut connection = AnyConnection::connect(&url)
            .await
            .map_err(|error| format!("codux-db: failed to connect: {error}"))?;
        apply_db_session_guard(&mut connection, &engine, read_only).await?;
        let rows = query::<Any>(statement)
            .fetch_all(&mut connection)
            .await
            .map_err(|error| format!("codux-db: query failed: {error}"))?;
        Ok::<Value, String>(db_rows_json(rows))
    };
    run_with_tokio_timeout(future)
}

fn run_with_tokio_timeout<F>(future: F) -> Result<Value, String>
where
    F: std::future::Future<Output = Result<Value, String>>,
{
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| format!("codux-db: failed to start query runtime: {error}"))?;
    runtime.block_on(async {
        tokio::time::timeout(Duration::from_secs(DB_QUERY_TIMEOUT_SECONDS), future)
            .await
            .map_err(|_| {
                format!("codux-db: query timed out after {DB_QUERY_TIMEOUT_SECONDS} seconds")
            })?
    })
}

async fn apply_db_session_guard(
    connection: &mut AnyConnection,
    engine: &str,
    read_only: bool,
) -> Result<(), String> {
    if !read_only {
        return Ok(());
    }
    match engine {
        "sqlite" | "sqlite3" => {
            query::<Any>("PRAGMA query_only = ON")
                .execute(&mut *connection)
                .await
                .map_err(|error| {
                    format!("codux-db: failed to enable sqlite read-only guard: {error}")
                })?;
        }
        "postgres" | "postgresql" | "pg" => {
            query::<Any>("SET default_transaction_read_only = on")
                .execute(&mut *connection)
                .await
                .map_err(|error| {
                    format!("codux-db: failed to enable postgres read-only guard: {error}")
                })?;
        }
        "mysql" | "mariadb" => {
            query::<Any>("SET SESSION TRANSACTION READ ONLY")
                .execute(&mut *connection)
                .await
                .map_err(|error| {
                    format!("codux-db: failed to enable mysql read-only guard: {error}")
                })?;
        }
        _ => {}
    }
    Ok(())
}

fn db_connection_url(profile: &Value) -> Result<String, String> {
    let engine = string_field(profile, "engine").to_ascii_lowercase();
    let database = string_field(profile, "database");
    if database.is_empty() {
        return Err("codux-db: database name/path cannot be empty".to_string());
    }
    if matches!(engine.as_str(), "sqlite" | "sqlite3") {
        return Ok(format!("sqlite://{}", encode_sqlite_path(&database)));
    }
    let host = string_field(profile, "host");
    let username = string_field(profile, "username");
    if host.is_empty() || username.is_empty() {
        return Err("codux-db: database host and username are required".to_string());
    }
    let password = string_field(profile, "password");
    let port = db_port_field(profile);
    let scheme = match engine.as_str() {
        "postgres" | "postgresql" | "pg" => "postgres",
        "mysql" | "mariadb" => "mysql",
        _ => return Err("codux-db: unsupported database engine".to_string()),
    };
    let user_info = if password.is_empty() {
        percent_encode(&username)
    } else {
        format!(
            "{}:{}",
            percent_encode(&username),
            percent_encode(&password)
        )
    };
    let ssl_mode = normalized_db_ssl_mode(&string_field(profile, "sslMode"), scheme);
    let ssl_key = if scheme == "postgres" {
        "sslmode"
    } else {
        "ssl-mode"
    };
    Ok(format!(
        "{scheme}://{user_info}@{}:{}/{}?{}={}",
        percent_encode_host(&host),
        port,
        percent_encode(&database),
        ssl_key,
        ssl_mode
    ))
}

fn statement_allowed(profile: &Value, statement: &str) -> bool {
    if !profile
        .get("readOnly")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        return true;
    }
    let first = first_sql_keyword(statement);
    matches!(
        first.as_deref(),
        Some("select" | "show" | "with" | "explain" | "pragma" | "describe" | "desc")
    ) && statement_has_only_readonly_keywords(statement)
}

fn first_sql_keyword(statement: &str) -> Option<String> {
    let mut text = statement.trim_start();
    loop {
        if let Some(rest) = text.strip_prefix("--") {
            if let Some(index) = rest.find('\n') {
                text = &rest[index + 1..];
                continue;
            }
            return None;
        }
        if let Some(rest) = text.strip_prefix("/*") {
            if let Some(index) = rest.find("*/") {
                text = &rest[index + 2..];
                continue;
            }
            return None;
        }
        break;
    }
    text.split(|ch: char| !ch.is_ascii_alphabetic())
        .find(|part| !part.is_empty())
        .map(|part| part.to_ascii_lowercase())
}

fn statement_has_only_readonly_keywords(statement: &str) -> bool {
    let mut characters = statement.char_indices().peekable();
    let mut word_start = None;
    while let Some((index, character)) = characters.next() {
        match character {
            '\'' | '"' => {
                if !readonly_word_allowed(statement, &mut word_start, index) {
                    return false;
                }
                skip_sql_quoted_string(&mut characters, character);
            }
            '-' if statement[index..].starts_with("--") => {
                if !readonly_word_allowed(statement, &mut word_start, index) {
                    return false;
                }
                skip_sql_line_comment(&mut characters);
            }
            '/' if statement[index..].starts_with("/*") => {
                if !readonly_word_allowed(statement, &mut word_start, index) {
                    return false;
                }
                skip_sql_block_comment(&mut characters);
            }
            character if character.is_ascii_alphabetic() => {
                word_start.get_or_insert(index);
            }
            _ => {
                if !readonly_word_allowed(statement, &mut word_start, index) {
                    return false;
                }
            }
        }
    }
    readonly_word_allowed(statement, &mut word_start, statement.len())
}

fn readonly_word_allowed(statement: &str, word_start: &mut Option<usize>, end: usize) -> bool {
    let Some(start) = word_start.take() else {
        return true;
    };
    let word = statement[start..end].to_ascii_lowercase();
    !matches!(
        word.as_str(),
        "insert"
            | "update"
            | "delete"
            | "replace"
            | "alter"
            | "drop"
            | "truncate"
            | "create"
            | "grant"
            | "revoke"
            | "merge"
            | "upsert"
            | "call"
            | "execute"
            | "exec"
            | "load"
            | "copy"
            | "set"
            | "begin"
            | "commit"
            | "rollback"
    )
}

fn skip_sql_quoted_string(
    characters: &mut std::iter::Peekable<std::str::CharIndices<'_>>,
    quote: char,
) {
    while let Some((_, character)) = characters.next() {
        if character != quote {
            continue;
        }
        if characters
            .peek()
            .is_some_and(|(_, next_character)| *next_character == quote)
        {
            let _ = characters.next();
            continue;
        }
        break;
    }
}

fn skip_sql_line_comment(characters: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    let _ = characters.next();
    for (_, character) in characters.by_ref() {
        if character == '\n' {
            break;
        }
    }
}

fn skip_sql_block_comment(characters: &mut std::iter::Peekable<std::str::CharIndices<'_>>) {
    let _ = characters.next();
    let mut previous = '\0';
    for (_, character) in characters.by_ref() {
        if previous == '*' && character == '/' {
            break;
        }
        previous = character;
    }
}

fn db_rows_json(rows: Vec<AnyRow>) -> Value {
    let total_rows = rows.len();
    let rows = rows
        .into_iter()
        .take(DB_QUERY_MAX_ROWS)
        .map(|row| {
            let columns = row
                .columns()
                .iter()
                .enumerate()
                .map(|(index, column)| {
                    (
                        column.name().to_string(),
                        db_cell_json(&row, index, column.type_info().name()),
                    )
                })
                .collect::<serde_json::Map<_, _>>();
            Value::Object(columns)
        })
        .collect::<Vec<_>>();
    json!({
        "ok": true,
        "rowCount": rows.len(),
        "totalRows": total_rows,
        "truncated": total_rows > DB_QUERY_MAX_ROWS,
        "rows": rows,
    })
}

fn db_cell_json(row: &AnyRow, index: usize, type_name: &str) -> Value {
    if row
        .try_get_raw(index)
        .map(|value| value.is_null())
        .unwrap_or(false)
    {
        return Value::Null;
    }
    match type_name {
        "BOOLEAN" => row
            .try_get::<bool, _>(index)
            .map(Value::Bool)
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "SMALLINT" => row
            .try_get::<i16, _>(index)
            .map(|value| json!(value))
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "INTEGER" => row
            .try_get::<i32, _>(index)
            .map(|value| json!(value))
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "BIGINT" => row
            .try_get::<i64, _>(index)
            .map(|value| json!(value))
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "REAL" => row
            .try_get::<f32, _>(index)
            .map(|value| json!(value))
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "DOUBLE" => row
            .try_get::<f64, _>(index)
            .map(|value| json!(value))
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "BLOB" => row
            .try_get::<Vec<u8>, _>(index)
            .map(|value| Value::String(format!("<{} blob bytes>", value.len())))
            .unwrap_or_else(|_| fallback_db_cell_json(row, index)),
        "NULL" => Value::Null,
        _ => fallback_db_cell_json(row, index),
    }
}

fn fallback_db_cell_json(row: &AnyRow, index: usize) -> Value {
    row.try_get::<String, _>(index)
        .map(|value| Value::String(truncate_cell(&value)))
        .unwrap_or_else(|_| Value::String("<decode error>".to_string()))
}

fn print_db_query_table(result: &Value) {
    let rows = result
        .get("rows")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() {
        println!("ok: 0 rows");
        return;
    }
    let columns = rows
        .first()
        .and_then(Value::as_object)
        .map(|object| object.keys().cloned().collect::<Vec<_>>())
        .unwrap_or_default();
    println!("{}", columns.join("\t"));
    for row in rows {
        let object = row.as_object();
        let cells = columns
            .iter()
            .map(|column| {
                object
                    .and_then(|object| object.get(column))
                    .map(db_cell_display)
                    .unwrap_or_default()
            })
            .collect::<Vec<_>>();
        println!("{}", cells.join("\t"));
    }
    if result
        .get("truncated")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        println!("[truncated at {DB_QUERY_MAX_ROWS} rows]");
    }
}

fn db_cell_display(value: &Value) -> String {
    match value {
        Value::Null => "NULL".to_string(),
        Value::String(value) => value.clone(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.to_string(),
        other => serde_json::to_string(other).unwrap_or_default(),
    }
}

fn truncate_cell(value: &str) -> String {
    let mut output = value
        .chars()
        .take(DB_QUERY_MAX_CELL_CHARS)
        .collect::<String>();
    if value.chars().count() > DB_QUERY_MAX_CELL_CHARS {
        output.push('…');
    }
    output
}

fn percent_encode(value: &str) -> String {
    utf8_percent_encode(value, DB_URL_ENCODE_SET).to_string()
}

fn percent_encode_host(value: &str) -> String {
    if value.contains(':') && !value.starts_with('[') {
        format!("[{value}]")
    } else {
        value.to_string()
    }
}

fn encode_sqlite_path(value: &str) -> String {
    if value == ":memory:" {
        return ":memory:".to_string();
    }
    let path = value.replace('\\', "/");
    if path.as_bytes().get(1) == Some(&b':') {
        format!("/{path}")
    } else {
        path
    }
}

fn normalized_db_ssl_mode(value: &str, scheme: &str) -> &'static str {
    match (scheme, value.trim().to_ascii_lowercase().as_str()) {
        ("postgres", "disable") => "disable",
        ("postgres", "allow") => "allow",
        ("postgres", "require") => "require",
        ("postgres", "verify-ca") => "verify-ca",
        ("postgres", "verify-full") => "verify-full",
        ("mysql", "disable" | "disabled") => "DISABLED",
        ("mysql", "require" | "required") => "REQUIRED",
        ("mysql", "verify-ca") => "VERIFY_CA",
        ("mysql", "verify-identity" | "verify-full") => "VERIFY_IDENTITY",
        ("mysql", _) => "PREFERRED",
        _ => "prefer",
    }
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

fn db_port_field(profile: &Value) -> u16 {
    let default = match string_field(profile, "engine")
        .to_ascii_lowercase()
        .as_str()
    {
        "mysql" | "mariadb" => 3306,
        "sqlite" | "sqlite3" => 1,
        _ => 5432,
    };
    let raw = profile
        .get("port")
        .and_then(|value| {
            value
                .as_u64()
                .or_else(|| value.as_str().and_then(|text| text.parse::<u64>().ok()))
        })
        .unwrap_or(default);
    raw.clamp(1, 65535) as u16
}

fn home_dir_env() -> String {
    let home = env_value("HOME");
    if !home.is_empty() {
        return home;
    }
    env_value("USERPROFILE")
}

fn expand_home(path: &str) -> String {
    if path == "~" {
        return home_dir_env();
    }
    if let Some(rest) = path.strip_prefix("~/").or_else(|| path.strip_prefix("~\\")) {
        let home = home_dir_env();
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
            Some("appendSystemPrompt")
        );
        assert_eq!(
            tool_memory_injection_strategy("opencode"),
            Some("opencodeSystemTransform")
        );
        assert_eq!(
            tool_memory_injection_strategy("omp"),
            Some("appendSystemPrompt")
        );
        assert_eq!(
            tool_memory_injection_strategy("mimo"),
            Some("opencodeSystemTransform")
        );
        assert_eq!(tool_memory_injection_strategy("codewhale"), None);
        assert_eq!(tool_memory_injection_strategy("kimi-code"), None);
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
    fn null_profile_files_are_empty_lists() {
        let root = Value::Null;
        assert_eq!(ssh_profiles_array(&root).map(<[Value]>::len), Some(0));
        assert_eq!(db_profiles_array(&root).map(<[Value]>::len), Some(0));
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

    #[test]
    fn ssh_askpass_selects_matching_saved_secret() {
        assert_eq!(
            ssh_askpass_response("root@example.com's password:", "secret", ""),
            Some("secret")
        );
        assert_eq!(
            ssh_askpass_response("Enter passphrase for key 'id_ed25519':", "", "phrase"),
            Some("phrase")
        );
        assert_eq!(
            ssh_askpass_response("Password:", "secret", "phrase"),
            Some("secret")
        );
        assert_eq!(ssh_askpass_response("Password:", "", ""), None);
    }

    #[test]
    fn askpass_mode_only_answers_from_the_helper_binary_name() {
        assert!(is_wrapper_helper_exe_name("codux-wrapper-helper"));
        assert!(is_wrapper_helper_exe_name("CODUX-WRAPPER-HELPER"));
        assert!(!is_wrapper_helper_exe_name("codux"));
        assert!(!is_wrapper_helper_exe_name("codux-agent"));
    }

    #[test]
    fn ssh_auth_options_pin_saved_credential_strategy() {
        let password_options = ssh_password_auth_options();
        assert!(
            password_options
                .contains(&"PreferredAuthentications=password,keyboard-interactive".to_string())
        );
        assert!(password_options.contains(&"PubkeyAuthentication=no".to_string()));
        assert!(password_options.contains(&"NumberOfPasswordPrompts=1".to_string()));

        let key_options = ssh_identities_only_options();
        assert!(key_options.contains(&"IdentitiesOnly=yes".to_string()));
    }

    #[test]
    fn public_db_profile_filters_project_and_redacts_secrets() {
        let profile = serde_json::json!({
            "id": "db-1",
            "projectId": "project-a",
            "name": "Production",
            "engine": "postgres",
            "host": "db.example.com",
            "port": 5432,
            "database": "app",
            "username": "app_user",
            "password": "secret-password",
            "readOnly": true
        });

        let public = public_db_profile(&profile, "project-a").expect("public profile");
        assert_eq!(public.get("id").and_then(Value::as_str), Some("db-1"));
        assert_eq!(
            public.get("endpoint").and_then(Value::as_str),
            Some("db.example.com:5432/app")
        );
        assert!(public.get("username").is_none());
        assert!(public.get("password").is_none());
        assert!(public_db_profile(&profile, "project-b").is_none());
    }

    #[test]
    fn readonly_db_statement_gate_blocks_writes() {
        let profile = serde_json::json!({ "readOnly": true });
        assert!(statement_allowed(&profile, " /* comment */ SELECT 1"));
        assert!(statement_allowed(
            &profile,
            "-- hi\nWITH x AS (SELECT 1) SELECT * FROM x"
        ));
        assert!(statement_allowed(
            &profile,
            "SELECT ';' AS literal; -- trailing comment"
        ));
        assert!(statement_allowed(&profile, "SHOW TABLES"));
        assert!(statement_allowed(&profile, "SELECT 1; SHOW TABLES"));
        assert!(!statement_allowed(&profile, "DELETE FROM users"));
        assert!(!statement_allowed(&profile, "INSERT INTO users VALUES (1)"));
        assert!(!statement_allowed(&profile, "SELECT 1; DELETE FROM users"));
        assert!(statement_allowed(&profile, "SELECT 'delete' AS literal"));
        assert!(statement_allowed(
            &profile,
            "-- delete is a comment\nSELECT 1"
        ));
    }

    #[test]
    fn sqlite_path_encoder_preserves_windows_drive_paths_as_absolute() {
        assert_eq!(
            encode_sqlite_path(r"F:\codux\app.sqlite3"),
            "/F:/codux/app.sqlite3"
        );
    }

    #[test]
    fn sqlite_db_query_executes_without_exposing_passwords() {
        let dir = std::env::temp_dir().join(format!("codux-db-query-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let database = dir.join("app.sqlite3");
        {
            let conn = rusqlite::Connection::open(&database).unwrap();
            conn.execute("CREATE TABLE users (id INTEGER, name TEXT)", [])
                .unwrap();
            conn.execute("INSERT INTO users VALUES (1, 'Ada')", [])
                .unwrap();
        }
        let profile = serde_json::json!({
            "id": "db-1",
            "projectId": "project-a",
            "engine": "sqlite",
            "database": database.display().to_string(),
            "readOnly": true,
            "password": "secret-password"
        });

        let result = run_db_query(&profile, "SELECT id, name FROM users").unwrap();
        assert_eq!(result.get("rowCount").and_then(Value::as_u64), Some(1));
        let output = serde_json::to_string(&result).unwrap();
        assert!(output.contains("Ada"));
        assert!(!output.contains("secret-password"));
        fs::remove_dir_all(dir).ok();
    }
}
