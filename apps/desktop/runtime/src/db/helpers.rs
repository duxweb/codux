use super::types::{DBConnectionProfile, DBProfileUpsertRequest};
use crate::{config::ConfigDocumentStore, runtime_paths::app_support_dir};
use chrono::Utc;
use std::path::{Path, PathBuf};
use uuid::Uuid;

pub fn db_profiles_file_path() -> PathBuf {
    db_profiles_file_path_in(app_support_dir())
}

pub fn db_profiles_file_path_in(support_dir: PathBuf) -> PathBuf {
    support_dir.join("db_profiles.json")
}

pub fn render_db_launch_context(
    project_id: Option<&str>,
    codux_db_command: Option<String>,
) -> Option<String> {
    render_db_launch_context_from_support_dir(app_support_dir(), project_id, codux_db_command)
}

pub fn render_db_launch_context_from_support_dir(
    support_dir: PathBuf,
    project_id: Option<&str>,
    codux_db_command: Option<String>,
) -> Option<String> {
    let mut profiles = sanitize_profiles(load_profiles(&db_profiles_file_path_in(support_dir))?);
    render_db_launch_context_for_profiles(&mut profiles, project_id, codux_db_command)
}

pub(super) fn render_db_launch_context_for_profiles(
    profiles: &mut Vec<DBConnectionProfile>,
    project_id: Option<&str>,
    codux_db_command: Option<String>,
) -> Option<String> {
    let project_id = project_id.and_then(normalized)?;
    profiles.retain(|profile| profile.project_id == project_id);
    if profiles.is_empty() {
        return None;
    }
    let codux_db_command = codux_db_command
        .and_then(|value| normalized(&value))
        .unwrap_or_else(|| "codux-db".to_string());
    let lines = ["Codux saved database connections for the current root project are available through terminal commands.".to_string(),
        format!(
            "Always run `{codux_db_command} list` at the time of use to discover the current database profiles as redacted JSON."
        ),
        format!(
            "When a matching saved database profile exists, run `{codux_db_command} <profile-id> -- '<statement>'`."
        ),
        "Do not grep the repository or inspect Codux config files to discover saved database connections; use the wrapper list command.".to_string(),
        "Do not ask for, print, infer, or hardcode saved database usernames or passwords.".to_string(),
        "When selecting non-basic column types such as timestamps, dates, UUIDs, decimals, JSON, enums, arrays, or MySQL tinyint/boolean values, cast them to text so the portable database wrapper can decode the result: use `column::text` on Postgres and `CAST(column AS CHAR)` on MySQL.".to_string()];
    Some(lines.join("\n"))
}

pub(super) fn load_profiles(path: &Path) -> Option<Vec<DBConnectionProfile>> {
    ConfigDocumentStore::for_file(path.to_path_buf()).snapshot_as()
}

pub(super) fn sanitize_profiles(profiles: Vec<DBConnectionProfile>) -> Vec<DBConnectionProfile> {
    profiles
        .into_iter()
        .filter_map(|profile| {
            sanitize_request(DBProfileUpsertRequest {
                id: Some(profile.id),
                project_id: profile.project_id,
                name: profile.name,
                engine: profile.engine,
                host: Some(profile.host),
                port: Some(profile.port),
                database: profile.database,
                username: Some(profile.username),
                password: profile.password,
                ssl_mode: Some(profile.ssl_mode),
                read_only: profile.read_only,
            })
            .ok()
        })
        .collect()
}

pub(super) fn sanitize_request(
    request: DBProfileUpsertRequest,
) -> Result<DBConnectionProfile, String> {
    let project_id = normalized(&request.project_id)
        .ok_or_else(|| "Database profile must be attached to a root project.".to_string())?;
    let engine = normalized(&request.engine)
        .unwrap_or_else(|| "postgres".to_string())
        .to_ascii_lowercase();
    let engine = match engine.as_str() {
        "postgres" | "postgresql" | "pg" => "postgres",
        "mysql" | "mariadb" => "mysql",
        "sqlite" | "sqlite3" => "sqlite",
        _ => return Err("Database engine must be postgres, mysql, or sqlite.".to_string()),
    }
    .to_string();
    let database = normalized(&request.database)
        .ok_or_else(|| "Database name/path cannot be empty.".to_string())?;
    let host = request
        .host
        .as_deref()
        .and_then(normalized)
        .unwrap_or_else(|| {
            if engine == "sqlite" {
                String::new()
            } else {
                "localhost".to_string()
            }
        });
    let username = request
        .username
        .as_deref()
        .and_then(normalized)
        .unwrap_or_default();
    if engine != "sqlite" && username.is_empty() {
        return Err("Database username cannot be empty.".to_string());
    }
    let port = request.port.unwrap_or_else(|| default_port(&engine));
    let ssl_mode = request
        .ssl_mode
        .as_deref()
        .and_then(normalized)
        .unwrap_or_else(|| "prefer".to_string());

    Ok(DBConnectionProfile {
        id: request
            .id
            .and_then(|value| normalized(&value))
            .unwrap_or_else(|| Uuid::new_v4().to_string()),
        project_id,
        name: request.name.trim().to_string(),
        engine,
        host,
        port: port.clamp(1, 65535),
        database,
        username,
        password: request.password.and_then(|value| normalized(&value)),
        ssl_mode,
        read_only: request.read_only,
        updated_at: Utc::now().timestamp(),
    })
}

pub(super) fn default_port(engine: &str) -> u16 {
    match engine {
        "mysql" => 3306,
        "postgres" => 5432,
        _ => 1,
    }
}

pub(super) fn display_name(profile: &DBConnectionProfile) -> String {
    if profile.name.trim().is_empty() {
        if profile.engine == "sqlite" {
            profile.database.clone()
        } else {
            format!("{} · {}", profile.engine, profile.database)
        }
    } else {
        profile.name.clone()
    }
}

pub(super) fn endpoint(profile: &DBConnectionProfile) -> String {
    if profile.engine == "sqlite" {
        return profile.database.clone();
    }
    format!("{}:{} / {}", profile.host, profile.port, profile.database)
}

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}
