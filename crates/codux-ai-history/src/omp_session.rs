use serde_json::Value;
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Component, Path, PathBuf},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmpSessionRole {
    User,
    Assistant,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OmpUsage {
    pub input_tokens: i64,
    pub output_tokens: i64,
    pub cache_read_tokens: i64,
    pub cache_write_tokens: i64,
    pub cost_usd: f64,
}

impl OmpUsage {
    pub fn cached_input_tokens(&self) -> i64 {
        self.cache_read_tokens + self.cache_write_tokens
    }

    pub fn total_tokens(&self) -> i64 {
        self.input_tokens + self.output_tokens
    }

    fn add(&mut self, other: &Self) {
        self.input_tokens += other.input_tokens;
        self.output_tokens += other.output_tokens;
        self.cache_read_tokens += other.cache_read_tokens;
        self.cache_write_tokens += other.cache_write_tokens;
        self.cost_usd += other.cost_usd;
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OmpSessionEvent {
    pub role: OmpSessionRole,
    pub timestamp: f64,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub usage: Option<OmpUsage>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct OmpSession {
    pub id: Option<String>,
    pub parent_session: Option<String>,
    pub cwd: Option<String>,
    pub title: Option<String>,
    pub created_at: Option<f64>,
    pub updated_at: f64,
    pub model: Option<String>,
    pub provider: Option<String>,
    pub usage: OmpUsage,
    pub events: Vec<OmpSessionEvent>,
}

pub fn parse_omp_session(path: &Path) -> Option<OmpSession> {
    let file = fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut session = OmpSession::default();
    let mut parsed_row = false;
    let mut title_slot_seen = false;
    let mut line = String::new();

    loop {
        line.clear();
        let bytes = reader.read_line(&mut line).ok()?;
        if bytes == 0 {
            break;
        }
        let Ok(row) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        parsed_row = true;
        let timestamp = omp_timestamp(&row).unwrap_or(0.0);
        session.updated_at = session.updated_at.max(timestamp);

        match row.get("type").and_then(Value::as_str) {
            Some("title") => {
                if row.get("v").and_then(Value::as_i64) == Some(1) {
                    session.title = string_field(&row, "title");
                    title_slot_seen = true;
                }
            }
            Some("session") => {
                session.id = string_field(&row, "id").or(session.id);
                session.parent_session =
                    string_field(&row, "parentSession").or(session.parent_session);
                session.cwd = string_field(&row, "cwd").or(session.cwd);
                if !title_slot_seen {
                    session.title = string_field(&row, "title").or(session.title);
                }
                if timestamp > 0.0 {
                    session.created_at = Some(
                        session
                            .created_at
                            .map(|created_at| created_at.min(timestamp))
                            .unwrap_or(timestamp),
                    );
                }
            }
            Some("message") => parse_message(&row, timestamp, &mut session),
            Some("title_change") => {
                if !title_slot_seen {
                    session.title = string_field(&row, "title");
                }
            }
            _ => {}
        }
    }

    parsed_row.then_some(session)
}

pub fn omp_agent_roots(home: &Path) -> Vec<PathBuf> {
    let config_name = std::env::var("PI_CONFIG_DIR")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| ".omp".to_string());
    let custom_agent_root = std::env::var_os("PI_CODING_AGENT_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    let xdg_data_root = std::env::var_os("XDG_DATA_HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from);
    omp_agent_roots_from(
        home,
        &config_name,
        custom_agent_root,
        xdg_data_root,
        omp_profile_from_env(),
    )
}

fn omp_agent_roots_from(
    home: &Path,
    config_name: &str,
    custom_agent_root: Option<PathBuf>,
    xdg_data_root: Option<PathBuf>,
    active_profile: Option<String>,
) -> Vec<PathBuf> {
    let config_root = omp_config_root(home, config_name);
    let xdg_root = xdg_data_root
        .filter(|_| matches!(std::env::consts::OS, "linux" | "macos"))
        .map(|root| root.join("omp"));
    let custom_agent_root = custom_agent_root.filter(|custom| {
        !active_profile.as_ref().is_some_and(|profile| {
            let config_profile = config_root.join("profiles").join(profile).join("agent");
            if local_paths_equal(custom, &config_profile) {
                return true;
            }
            xdg_root
                .as_ref()
                .map(|root| root.join("profiles").join(profile))
                .is_some_and(|xdg_profile| {
                    xdg_profile.is_dir() && local_paths_equal(custom, &xdg_profile)
                })
        })
    });
    let mut roots = Vec::new();
    if let Some(custom) = custom_agent_root {
        push_unique(&mut roots, custom);
    } else if xdg_root.as_ref().is_some_and(|root| root.is_dir()) {
        push_unique(&mut roots, xdg_root.clone().unwrap());
    } else {
        push_unique(&mut roots, config_root.join("agent"));
    }

    let mut profile_names = directory_names(&config_root.join("profiles"));
    if let Some(xdg_root) = xdg_root.as_ref() {
        profile_names.extend(directory_names(&xdg_root.join("profiles")));
    }
    profile_names.sort();
    profile_names.dedup();
    for profile_name in profile_names {
        let xdg_profile = xdg_root
            .as_ref()
            .map(|root| root.join("profiles").join(&profile_name));
        if let Some(xdg_profile) = xdg_profile.filter(|path| path.is_dir()) {
            push_unique(&mut roots, xdg_profile);
        } else {
            push_unique(
                &mut roots,
                config_root
                    .join("profiles")
                    .join(profile_name)
                    .join("agent"),
            );
        }
    }
    roots
}

fn omp_profile_from_env() -> Option<String> {
    let value = std::env::var("OMP_PROFILE")
        .ok()
        .or_else(|| std::env::var("PI_PROFILE").ok())?;
    let value = value.trim();
    (!value.is_empty() && value != "default").then(|| value.to_string())
}

fn local_paths_equal(left: &Path, right: &Path) -> bool {
    codux_runtime_core::path::optional_local_path_equals(
        Some(left.to_string_lossy().as_ref()),
        right.to_string_lossy().as_ref(),
    )
}

fn omp_config_root(home: &Path, config_name: &str) -> PathBuf {
    let relative = Path::new(config_name)
        .components()
        .filter_map(|component| match component {
            Component::Prefix(_) | Component::RootDir => None,
            _ => Some(component.as_os_str()),
        })
        .collect::<PathBuf>();
    let joined = home.join(relative);
    codux_runtime_core::path::normalize_path_syntax(&joined.to_string_lossy())
        .map(PathBuf::from)
        .unwrap_or(joined)
}

pub fn omp_session_dirs(home: &Path) -> Vec<PathBuf> {
    omp_agent_roots(home)
        .into_iter()
        .map(|root| root.join("sessions"))
        .collect()
}

pub fn omp_session_paths(home: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for sessions_dir in omp_session_dirs(home) {
        let Ok(project_dirs) = fs::read_dir(sessions_dir) else {
            continue;
        };
        for project_dir in project_dirs.flatten().map(|entry| entry.path()) {
            if !project_dir.is_dir() {
                continue;
            }
            let Ok(entries) = fs::read_dir(project_dir) else {
                continue;
            };
            files.extend(entries.flatten().map(|entry| entry.path()).filter(|path| {
                path.is_file()
                    && path.extension().and_then(|extension| extension.to_str()) == Some("jsonl")
            }));
        }
    }
    files.sort();
    files.dedup();
    files
}

fn parse_message(row: &Value, timestamp: f64, session: &mut OmpSession) {
    let message = row.get("message").unwrap_or(&Value::Null);
    let role = match message.get("role").and_then(Value::as_str) {
        Some("user") => OmpSessionRole::User,
        Some("assistant") => OmpSessionRole::Assistant,
        _ => return,
    };
    let model = string_field(message, "model");
    let provider = string_field(message, "provider");
    let usage = (role == OmpSessionRole::Assistant)
        .then(|| message.get("usage").and_then(parse_usage))
        .flatten();
    if let Some(usage) = usage.as_ref() {
        session.usage.add(usage);
    }
    if model.is_some() {
        session.model = model.clone();
    }
    if provider.is_some() {
        session.provider = provider.clone();
    }
    session.events.push(OmpSessionEvent {
        role,
        timestamp,
        model,
        provider,
        usage,
    });
}

fn parse_usage(value: &Value) -> Option<OmpUsage> {
    let usage = OmpUsage {
        input_tokens: number_i64(value.get("input")),
        output_tokens: number_i64(value.get("output")),
        cache_read_tokens: number_i64(value.get("cacheRead")),
        cache_write_tokens: number_i64(value.get("cacheWrite")),
        cost_usd: value
            .get("cost")
            .and_then(|cost| cost.get("total"))
            .and_then(number_f64)
            .unwrap_or(0.0),
    };
    (usage.total_tokens() > 0 || usage.cached_input_tokens() > 0 || usage.cost_usd > 0.0)
        .then_some(usage)
}

fn omp_timestamp(value: &Value) -> Option<f64> {
    value
        .get("timestamp")
        .and_then(Value::as_str)
        .and_then(parse_iso8601_seconds)
        .or_else(|| {
            value
                .get("updatedAt")
                .and_then(Value::as_str)
                .and_then(parse_iso8601_seconds)
        })
        .or_else(|| {
            value
                .get("message")
                .and_then(|message| message.get("timestamp"))
                .and_then(number_f64)
                .map(normalize_epoch_seconds)
        })
}

fn parse_iso8601_seconds(value: &str) -> Option<f64> {
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|date| date.timestamp_micros() as f64 / 1_000_000.0)
}

fn normalize_epoch_seconds(value: f64) -> f64 {
    if value > 10_000_000_000.0 {
        value / 1_000.0
    } else {
        value
    }
}

fn string_field(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

fn number_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|number| number as i64))
        })
        .unwrap_or(0)
        .max(0)
}

fn number_f64(value: &Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|number| number.parse().ok()))
        .filter(|number| number.is_finite() && *number >= 0.0)
}

fn directory_names(directory: &Path) -> Vec<std::ffi::OsString> {
    let Ok(entries) = fs::read_dir(directory) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|entry| entry.path().is_dir())
        .map(|entry| entry.file_name())
        .collect()
}

fn push_unique(paths: &mut Vec<PathBuf>, path: PathBuf) {
    if !paths.contains(&path) {
        paths.push(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_incremental_usage_and_ignores_malformed_tail() {
        let dir = std::env::temp_dir().join(format!("codux-omp-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"session\",\"version\":3,\"id\":\"session-1\",\"timestamp\":\"2026-07-19T01:00:00Z\",\"cwd\":\"/tmp/project\",\"title\":\"OMP test\"}\n",
                "{\"type\":\"message\",\"timestamp\":\"2026-07-19T01:00:01Z\",\"message\":{\"role\":\"user\"}}\n",
                "{\"type\":\"message\",\"timestamp\":\"2026-07-19T01:00:02Z\",\"message\":{\"role\":\"assistant\",\"provider\":\"anthropic\",\"model\":\"claude-sonnet-4-5\",\"usage\":{\"input\":3,\"output\":191,\"cacheRead\":5,\"cacheWrite\":1684,\"cost\":{\"total\":0.009189}}}}\n",
                "{\"type\":\"message\",\"timestamp\":\"2026-07-19T01:00:03Z\",\"message\":{\"role\":\"assistant\",\"provider\":\"anthropic\",\"model\":\"claude-sonnet-4-5\",\"usage\":{\"input\":7,\"output\":11,\"cacheRead\":13,\"cacheWrite\":17,\"cost\":{\"total\":0.01}}}}\n",
                "{\"type\":\"message\""
            ),
        )
        .unwrap();

        let session = parse_omp_session(&path).unwrap();

        assert_eq!(session.id.as_deref(), Some("session-1"));
        assert_eq!(session.cwd.as_deref(), Some("/tmp/project"));
        assert_eq!(session.model.as_deref(), Some("claude-sonnet-4-5"));
        assert_eq!(session.provider.as_deref(), Some("anthropic"));
        assert_eq!(session.usage.input_tokens, 10);
        assert_eq!(session.usage.output_tokens, 202);
        assert_eq!(session.usage.cached_input_tokens(), 1_719);
        assert!((session.usage.cost_usd - 0.019189).abs() < 0.000_000_1);
        assert_eq!(session.events.len(), 3);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn current_title_slot_overrides_stale_header_title() {
        let dir = std::env::temp_dir().join(format!("codux-omp-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("session.jsonl");
        fs::write(
            &path,
            concat!(
                "{\"type\":\"title\",\"v\":1,\"title\":\"Current title\",\"updatedAt\":\"2026-07-19T01:02:00Z\",\"pad\":\"\"}\n",
                "{\"type\":\"session\",\"version\":3,\"id\":\"session-1\",\"timestamp\":\"2026-07-19T01:00:00Z\",\"cwd\":\"/tmp/project\",\"title\":\"Stale title\"}\n"
            ),
        )
        .unwrap();

        let session = parse_omp_session(&path).unwrap();

        assert_eq!(session.title.as_deref(), Some("Current title"));
        assert_eq!(
            session.updated_at,
            parse_iso8601_seconds("2026-07-19T01:02:00Z").unwrap()
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn session_discovery_does_not_recurse_into_artifacts() {
        let home = std::env::temp_dir().join(format!("codux-omp-home-{}", uuid::Uuid::new_v4()));
        let sessions = home.join(".omp/agent/sessions/--tmp--project--");
        fs::create_dir_all(sessions.join("session-1")).unwrap();
        fs::write(sessions.join("session.jsonl"), "{}\n").unwrap();
        fs::write(sessions.join("session-1/subagent.jsonl"), "{}\n").unwrap();

        let paths = omp_session_paths(&home);

        assert_eq!(paths, vec![sessions.join("session.jsonl")]);
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn session_discovery_includes_named_profiles() {
        let home = std::env::temp_dir().join(format!("codux-omp-home-{}", uuid::Uuid::new_v4()));
        let session = home.join(".omp/profiles/work/agent/sessions/-tmp-project/session.jsonl");
        fs::create_dir_all(session.parent().unwrap()).unwrap();
        fs::write(&session, "{}\n").unwrap();

        let paths = omp_session_paths(&home);

        assert!(paths.contains(&session));
        fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn config_root_matches_home_relative_omp_semantics() {
        let home = Path::new("/home/tester");

        assert_eq!(omp_config_root(home, "/custom/../omp"), home.join("omp"));
        assert_eq!(
            omp_config_root(home, ".config/omp"),
            home.join(".config/omp")
        );
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn xdg_data_root_replaces_default_agent_root() {
        let root = std::env::temp_dir().join(format!("codux-omp-roots-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let xdg_data = root.join("data");
        fs::create_dir_all(home.join(".omp/agent/sessions")).unwrap();
        fs::create_dir_all(xdg_data.join("omp/sessions")).unwrap();

        let roots = omp_agent_roots_from(&home, ".omp", None, Some(xdg_data.clone()), None);

        assert_eq!(roots, vec![xdg_data.join("omp")]);
        fs::remove_dir_all(root).unwrap();
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    #[test]
    fn each_profile_uses_its_existing_xdg_location() {
        let root = std::env::temp_dir().join(format!("codux-omp-roots-{}", uuid::Uuid::new_v4()));
        let home = root.join("home");
        let xdg_data = root.join("data");
        fs::create_dir_all(home.join(".omp/profiles/local/agent/sessions")).unwrap();
        fs::create_dir_all(home.join(".omp/profiles/work/agent/sessions")).unwrap();
        fs::create_dir_all(xdg_data.join("omp/profiles/work/sessions")).unwrap();

        let roots = omp_agent_roots_from(&home, ".omp", None, Some(xdg_data.clone()), None);

        assert!(roots.contains(&xdg_data.join("omp")));
        assert!(roots.contains(&home.join(".omp/profiles/local/agent")));
        assert!(roots.contains(&xdg_data.join("omp/profiles/work")));
        assert!(!roots.contains(&home.join(".omp/profiles/work/agent")));
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn profile_derived_agent_dir_does_not_replace_default_root() {
        let home = std::env::temp_dir().join(format!("codux-omp-roots-{}", uuid::Uuid::new_v4()));
        let profile_agent = home.join(".omp/profiles/work/agent");
        fs::create_dir_all(profile_agent.join("sessions")).unwrap();

        let roots = omp_agent_roots_from(
            &home,
            ".omp",
            Some(profile_agent.clone()),
            None,
            Some("work".to_string()),
        );

        assert!(roots.contains(&home.join(".omp/agent")));
        assert!(roots.contains(&profile_agent));
        fs::remove_dir_all(home).unwrap();
    }
}
