use super::*;

#[cfg(windows)]
pub(super) const PATH_SEPARATOR: char = ';';
#[cfg(not(windows))]
pub(super) const PATH_SEPARATOR: char = ':';

#[cfg(windows)]
pub(super) const FALLBACK_PATH: &str = "C:\\Windows\\System32;C:\\Windows;C:\\Windows\\System32\\Wbem;C:\\Windows\\System32\\WindowsPowerShell\\v1.0";
#[cfg(not(windows))]
pub(super) const FALLBACK_PATH: &str =
    "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin:/opt/homebrew/bin";

// Runtime screen only serves remote scrollback views, so cap it to mobile depth.
pub(super) const COMMON_PASSTHROUGH_ENV_KEYS: &[&str] = &[
    "LANG",
    "LC_ALL",
    "LC_CTYPE",
    "LC_MESSAGES",
    "LC_COLLATE",
    "LC_NUMERIC",
    "LC_TIME",
    "LC_MONETARY",
    "LC_MEASUREMENT",
    "LC_IDENTIFICATION",
    "LC_PAPER",
    "LC_NAME",
    "LC_ADDRESS",
    "LC_TELEPHONE",
    "LC_RESPONSETIME",
];
#[cfg(unix)]
pub(super) const UNIX_PASSTHROUGH_ENV_KEYS: &[&str] =
    &["TMPDIR", "SSH_AUTH_SOCK", "__CF_USER_TEXT_ENCODING"];
#[cfg(windows)]
pub(super) const WINDOWS_PASSTHROUGH_ENV_KEYS: &[&str] = &[
    // Core system paths. A missing SystemDrive/ProgramData makes Windows
    // components write literal `%SystemDrive%\...` dirs into the cwd.
    "SystemDrive",
    "SystemRoot",
    "WINDIR",
    "ProgramData",
    "ALLUSERSPROFILE",
    "PUBLIC",
    "DriverData",
    "COMSPEC",
    "PATHEXT",
    "OS",
    "TEMP",
    "TMP",
    "USERPROFILE",
    "HOMEDRIVE",
    "HOMEPATH",
    "APPDATA",
    "LOCALAPPDATA",
    "ProgramFiles",
    "ProgramFiles(x86)",
    "ProgramW6432",
    "CommonProgramFiles",
    "CommonProgramFiles(x86)",
    "CommonProgramW6432",
    "USERNAME",
    "USERDOMAIN",
    "COMPUTERNAME",
    "OneDrive",
    "PROCESSOR_ARCHITECTURE",
    "NUMBER_OF_PROCESSORS",
    "PSModulePath",
];

#[cfg(not(windows))]
pub(super) fn capture_shell_environment_uncached(
    shell: &str,
    cwd: &str,
    home: &str,
    user: &str,
) -> Option<HashMap<String, String>> {
    let begin_marker = "__CODUX_SHELL_ENV_BEGIN__";
    let end_marker = "__CODUX_SHELL_ENV_END__";
    // Emit the env block only if `cd` into the project cwd actually succeeds
    // (`&&`, not `;`). If the cwd's inode is momentarily gone (an external drive
    // blipped), `cd` fails, nothing is printed, parsing returns None, and the
    // caller declines to cache it — so the next launch retries instead of being
    // stuck with a broken, PATH-less env for the whole process lifetime.
    let command = format!(
        "cd {} && {{ printf '%s\\000' '{}'; command env -0; printf '%s\\000' '{}'; }}",
        shell_quote(cwd),
        begin_marker,
        end_marker
    );
    let mut capture = Command::new(shell);
    capture
        .args(["-l", "-i", "-c", &command])
        // Run the capture process itself from $HOME, never the (possibly dead)
        // project cwd, so a stale inode can't stop the capture from starting.
        .current_dir(home)
        .env_clear()
        .env("HOME", home)
        .env("USER", user)
        .env("LOGNAME", user)
        .env("SHELL", shell)
        .env("TERM", "xterm-256color")
        .env(
            "PATH",
            std::env::var("PATH").unwrap_or_else(|_| FALLBACK_PATH.to_string()),
        )
        .stdin(Stdio::null());
    for key in COMMON_PASSTHROUGH_ENV_KEYS
        .iter()
        .chain(UNIX_PASSTHROUGH_ENV_KEYS.iter())
    {
        if let Ok(value) = std::env::var(key) {
            if !value.is_empty() {
                capture.env(key, value);
            }
        }
    }
    let output = capture.output().ok()?;
    parse_captured_shell_environment(&output.stdout, begin_marker, end_marker)
}

#[cfg(not(windows))]
pub(super) fn parse_captured_shell_environment(
    output: &[u8],
    begin_marker: &str,
    end_marker: &str,
) -> Option<HashMap<String, String>> {
    let begin = find_bytes(output, begin_marker.as_bytes())? + begin_marker.len();
    let rest = &output[begin..];
    let end = find_bytes(rest, end_marker.as_bytes())?;
    let mut body = &rest[..end];
    while matches!(body.first(), Some(0 | b'\n' | b'\r')) {
        body = &body[1..];
    }

    let mut values = HashMap::new();
    for entry in body.split(|byte| *byte == 0) {
        if entry.is_empty() {
            continue;
        }
        let Some(eq) = entry.iter().position(|byte| *byte == b'=') else {
            continue;
        };
        if eq == 0 {
            continue;
        }
        let key = String::from_utf8_lossy(&entry[..eq]).to_string();
        let value = String::from_utf8_lossy(&entry[eq + 1..]).to_string();
        values.insert(key, value);
    }
    Some(values)
}

#[cfg(not(windows))]
pub(super) fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    if needle.is_empty() {
        return Some(0);
    }
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

#[cfg(windows)]
pub(super) fn normalize_terminal_path(path: &str) -> String {
    if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
        return format!(r"\\{rest}");
    }
    if let Some(rest) = path.strip_prefix(r"\\?\") {
        return rest.to_string();
    }
    path.to_string()
}

#[cfg(not(windows))]
pub(super) fn normalize_terminal_path(path: &str) -> String {
    path.to_string()
}

pub fn default_shell() -> String {
    if cfg!(target_os = "windows") {
        windows_default_shell()
    } else {
        std::env::var("SHELL")
            .ok()
            .filter(|shell| valid_shell_path(shell))
            .or_else(default_unix_login_shell)
            .unwrap_or_else(|| "/bin/zsh".to_string())
    }
}

#[cfg(not(target_os = "windows"))]
pub(super) fn normalize_terminal_shell(shell: &str) -> Option<String> {
    let trimmed = shell.trim();
    if trimmed.is_empty() || is_terminal_integration_shell(trimmed) {
        return None;
    }
    Some(trimmed.to_string())
}

#[cfg(target_os = "windows")]
pub(super) fn normalize_terminal_shell(shell: &str) -> Option<String> {
    let trimmed = shell.trim();
    (!trimmed.is_empty()).then(|| trimmed.to_string())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn default_unix_login_shell() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        if let Some(shell) = macos_login_shell(default_user().as_str()) {
            return Some(shell);
        }
    }
    passwd_login_shell(default_user().as_str())
}

#[cfg(target_os = "windows")]
pub(super) fn default_unix_login_shell() -> Option<String> {
    None
}

pub(super) fn valid_shell_path(shell: &str) -> bool {
    let trimmed = shell.trim();
    !trimmed.is_empty()
        && !matches!(trimmed, "/bin/sh" | "sh")
        && !is_terminal_integration_shell(trimmed)
        && Path::new(trimmed).is_file()
}

#[cfg(not(target_os = "windows"))]
pub(super) fn is_terminal_integration_shell(shell: &str) -> bool {
    let Some(name) = Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.to_ascii_lowercase())
    else {
        return false;
    };
    name.contains("(kiro-cli-term)") || name == "kiro-cli-term"
}

#[cfg(target_os = "windows")]
pub(super) fn is_terminal_integration_shell(_shell: &str) -> bool {
    false
}

#[cfg(target_os = "macos")]
pub(super) fn macos_login_shell(user: &str) -> Option<String> {
    let output = Command::new("/usr/bin/dscl")
        .args([".", "-read", &format!("/Users/{user}"), "UserShell"])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    text.split_whitespace()
        .last()
        .map(str::trim)
        .filter(|shell| valid_shell_path(shell))
        .map(ToOwned::to_owned)
}

#[cfg(not(target_os = "macos"))]
pub(super) fn macos_login_shell(_user: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "windows"))]
pub(super) fn passwd_login_shell(user: &str) -> Option<String> {
    let passwd = fs::read_to_string("/etc/passwd").ok()?;
    passwd.lines().find_map(|line| {
        let mut fields = line.split(':');
        let name = fields.next()?;
        if name != user {
            return None;
        }
        let shell = fields.nth(5)?;
        valid_shell_path(shell).then(|| shell.to_string())
    })
}

#[cfg(target_os = "windows")]
pub(super) fn passwd_login_shell(_user: &str) -> Option<String> {
    None
}

#[cfg(target_os = "windows")]
pub(super) fn windows_default_shell() -> String {
    windows_shell_candidates()
        .into_iter()
        .find(|path| Path::new(path).exists())
        .unwrap_or_else(|| "powershell.exe".to_string())
}

#[cfg(not(target_os = "windows"))]
pub(super) fn windows_default_shell() -> String {
    String::new()
}

#[cfg(target_os = "windows")]
pub(super) fn windows_shell_candidates() -> Vec<String> {
    let mut candidates = Vec::new();
    if let Ok(program_files) = std::env::var("ProgramFiles") {
        candidates.push(
            Path::new(&program_files)
                .join("PowerShell")
                .join("7")
                .join("pwsh.exe")
                .display()
                .to_string(),
        );
    }
    if let Ok(system_root) = std::env::var("SystemRoot").or_else(|_| std::env::var("WINDIR")) {
        candidates.push(
            Path::new(&system_root)
                .join("System32")
                .join("WindowsPowerShell")
                .join("v1.0")
                .join("powershell.exe")
                .display()
                .to_string(),
        );
    }
    candidates.push("powershell.exe".to_string());
    candidates
}

pub(super) fn merged_executable_path(
    shell: &str,
    home: &str,
    user: &str,
    inherited_path: Option<&str>,
    include_login_shell_path: bool,
) -> String {
    let default_path = inherited_path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(FALLBACK_PATH);
    let login_shell_path = include_login_shell_path
        .then(|| resolved_login_shell_path(shell, home, user))
        .flatten();
    let user_tool_paths = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
        Path::new(home).join(".local/bin").display().to_string(),
        Path::new(home).join(".bun/bin").display().to_string(),
        Path::new(home).join(".cargo/bin").display().to_string(),
        Path::new(home).join(".opencode/bin").display().to_string(),
    ];
    let mut paths = Vec::new();
    if let Some(login_shell_path) = login_shell_path {
        push_path_components(&mut paths, &login_shell_path);
    }
    #[cfg(not(windows))]
    {
        for path in user_tool_paths {
            push_path(&mut paths, path);
        }
    }
    #[cfg(windows)]
    {
        let _ = user_tool_paths;
    }
    push_path_components(&mut paths, default_path);
    paths.join(&PATH_SEPARATOR.to_string())
}

pub(super) fn push_path_components(paths: &mut Vec<String>, value: &str) {
    for path in value.split(PATH_SEPARATOR) {
        push_path(paths, path);
    }
}

pub(super) fn push_path(paths: &mut Vec<String>, value: impl AsRef<str>) {
    let value = value.as_ref().trim();
    if value.is_empty() || paths.iter().any(|existing| existing == value) {
        return;
    }
    paths.push(value.to_string());
}

pub(super) fn prepend_path_component(component: &str, path: &str) -> String {
    if component.trim().is_empty()
        || path
            .split(PATH_SEPARATOR)
            .any(|existing| existing.trim() == component.trim())
    {
        return path.to_string();
    }
    if path.trim().is_empty() {
        component.to_string()
    } else {
        format!("{component}{PATH_SEPARATOR}{path}")
    }
}

pub(super) fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

pub(super) fn resolved_login_shell_path(shell: &str, home: &str, user: &str) -> Option<String> {
    #[cfg(windows)]
    {
        let _ = shell;
        let _ = home;
        let _ = user;
        None
    }
    #[cfg(not(windows))]
    {
        static CACHE: OnceLock<parking_lot::Mutex<HashMap<String, Option<String>>>> =
            OnceLock::new();
        let cache = CACHE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()));
        let key = format!("{shell}|{home}|{user}");
        if let Some(value) = cache.lock().get(&key) {
            return value.clone();
        }
        let resolved = resolve_login_shell_path_uncached(shell, home, user);
        // Only cache a real path; caching None would strand a one-off failure for the whole process (same trap as the env-capture fix).
        if resolved.is_some() {
            cache.lock().insert(key, resolved.clone());
        }
        resolved
    }
}

#[cfg(not(windows))]
pub(super) fn resolve_login_shell_path_uncached(
    shell: &str,
    home: &str,
    user: &str,
) -> Option<String> {
    let begin_marker = "__CODUX_LOGIN_PATH_BEGIN__";
    let end_marker = "__CODUX_LOGIN_PATH_END__";
    let output = Command::new(shell)
        .args([
            "-lic",
            &format!("printf '{begin_marker}%s{end_marker}' \"$PATH\""),
        ])
        .env_clear()
        .env("HOME", home)
        .env("USER", user)
        .env("LOGNAME", user)
        .env("SHELL", shell)
        .env("PATH", FALLBACK_PATH)
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let begin = text.find(begin_marker)? + begin_marker.len();
    let end = text[begin..].find(end_marker)? + begin;
    let path = text[begin..end].trim();
    (!path.is_empty()).then(|| path.to_string())
}

pub(super) fn shell_name(shell: &str) -> Option<String> {
    Path::new(shell)
        .file_name()
        .and_then(|name| name.to_str())
        .map(|name| name.trim_start_matches('-').to_ascii_lowercase())
}

pub(super) fn is_zsh_shell(shell: &str) -> bool {
    let Some(name) = shell_name(shell) else {
        return false;
    };
    let Some(rest) = name.strip_prefix("zsh") else {
        return false;
    };
    rest.is_empty()
        || rest
            .chars()
            .next()
            .is_some_and(|ch| !ch.is_ascii_alphanumeric())
}

pub(super) fn zsh_runtime_hook_ready(runtime_root: &Path) -> bool {
    let hook_dir = runtime_root.join("scripts/shell-hooks/zsh");
    hook_dir.join(".zshenv").is_file()
        && hook_dir.join(".zprofile").is_file()
        && hook_dir.join(".zshrc").is_file()
        && runtime_root
            .join("scripts/shell-hooks/dmux-ai-hook.zsh")
            .is_file()
}

pub(super) fn project_path_name(path: &str) -> Option<String> {
    Path::new(path)
        .file_name()
        .and_then(|name| name.to_str())
        .filter(|name| !name.trim().is_empty())
        .map(str::to_string)
}

pub(super) fn default_user() -> String {
    std::env::var("USER")
        .or_else(|_| std::env::var("LOGNAME"))
        .or_else(|_| std::env::var("USERNAME"))
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "codux".to_string())
}

pub(super) fn default_lang() -> String {
    "en_US.UTF-8".to_string()
}

pub(super) fn now_seconds() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default()
}

pub(super) fn rfc3339_now() -> String {
    chrono::Utc::now().to_rfc3339()
}
