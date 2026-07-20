//! Memory serving for the headless host. The controller routes a remote-hosted
//! project's memory reads here; the agent runs the shared `codux-memory` engine
//! against its own memory store (`<agent_data_dir>/memory.sqlite3`) so the
//! project's memory lives where its AI sessions run.
//!
//! This is the read path (`memory.read` → summary/manager/management/status).
//! Extraction (`memory.extract`, the LLM write path driven by a
//! controller-forwarded provider config) is the follow-up.

use codux_memory::{
    MemoryConfig, MemoryLaunchRequest, MemoryManagementRequest, MemoryProjectInfo,
    MemoryProjectRecord, MemoryService, MemorySessionSnapshot,
};
use codux_runtime_live::terminal_pty::TerminalPtyConfig;
use serde_json::{Value, json};
use std::path::Path;

use crate::projects::{AgentProjectStore, agent_data_dir};

fn service() -> MemoryService {
    MemoryService::new(agent_data_dir())
}

pub(crate) fn prepare_terminal_launch_context(
    config: &mut TerminalPtyConfig,
    data_dir: &Path,
    runtime_root: &Path,
) -> Result<(), String> {
    let project_id = config
        .root_project_id
        .as_deref()
        .or(config.project_id.as_deref())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("project");
    let workspace_id = config
        .worktree_id
        .as_deref()
        .or(config.project_id.as_deref())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(project_id);
    let workspace_path = config
        .cwd
        .as_deref()
        .or(config.root_project_path.as_deref())
        .unwrap_or_default();
    let project_name = config
        .project_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Codux");
    let mut settings = MemoryConfig::default();
    settings.memory.enabled = false;
    settings.memory.automatic_injection_enabled = false;
    let artifacts = MemoryService::new(data_dir.to_path_buf())
        .prepare_launch_artifacts(
            runtime_root,
            MemoryLaunchRequest {
                project_id: project_id.to_string(),
                workspace_id: Some(workspace_id.to_string()),
                project_name: project_name.to_string(),
                workspace_path: Some(workspace_path.to_string()),
                settings,
                extra_context: Some(
                    codux_runtime_core::agent_worktree::agent_worktree_ai_directive().to_string(),
                ),
            },
        )
        .ok_or_else(|| "Unable to prepare the agent launch context.".to_string())?;
    config.memory_workspace_root = Some(artifacts.workspace_root);
    config.memory_prompt_file = Some(artifacts.prompt_file);
    config.memory_index_file = Some(artifacts.index_file);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let path = std::env::temp_dir().join(format!(
            "codux-agent-launch-{label}-{}",
            uuid::Uuid::new_v4()
        ));
        fs::create_dir_all(&path).unwrap();
        path
    }

    fn terminal_config(worktree_id: &str, workspace_path: &str) -> TerminalPtyConfig {
        TerminalPtyConfig {
            root_project_id: Some("project-root".to_string()),
            project_id: Some(worktree_id.to_string()),
            worktree_id: Some(worktree_id.to_string()),
            project_name: Some("Project Root".to_string()),
            cwd: Some(workspace_path.to_string()),
            ..Default::default()
        }
    }

    #[test]
    fn terminal_launch_context_writes_closed_loop_directive_per_worktree() {
        let data_dir = temp_dir("data");
        let runtime_root = temp_dir("runtime");
        let mut first = terminal_config("worktree-a", "/repo/worktree-a");
        let mut second = terminal_config("worktree-b", "/repo/worktree-b");

        prepare_terminal_launch_context(&mut first, &data_dir, &runtime_root).unwrap();
        prepare_terminal_launch_context(&mut second, &data_dir, &runtime_root).unwrap();

        let first_root = first.memory_workspace_root.as_ref().unwrap();
        let second_root = second.memory_workspace_root.as_ref().unwrap();
        assert_ne!(first_root, second_root);
        assert!(first_root.ends_with("worktree-a"));
        assert!(second_root.ends_with("worktree-b"));

        let prompt = fs::read_to_string(first.memory_prompt_file.as_ref().unwrap()).unwrap();
        assert!(prompt.contains("codux-worktree create"));
        assert!(prompt.contains("waits for the child agent to complete"));
        assert!(prompt.contains("--detach"));
        assert!(prompt.contains("Project ID: project-root"));
        assert!(prompt.contains("Workspace: /repo/worktree-a"));
        assert!(first.memory_index_file.as_ref().unwrap().is_file());
        assert!(!data_dir.join("memory.sqlite3").exists());

        fs::remove_dir_all(data_dir).ok();
        fs::remove_dir_all(runtime_root).ok();
    }
}

/// The host's projects as workspace records (the agent has no root/worktree
/// split, so each project is its own root + workspace).
fn memory_records() -> Vec<MemoryProjectRecord> {
    AgentProjectStore::new()
        .list()
        .into_iter()
        .map(|project| MemoryProjectRecord {
            id: project.id.clone(),
            root_project_id: project.id,
            root_project_name: project.name,
            root_project_path: project.path.clone(),
            workspace_path: project.path,
            git_default_push_remote_name: None,
        })
        .collect()
}

/// Run a memory extraction pass on the host with the controller-forwarded
/// provider config. The config (incl. its provider's API key) is used for this
/// run only and never persisted. Returns `{op: "extract", result: <status>}`.
pub async fn memory_extract_payload(payload: &Value) -> Result<Value, String> {
    let config: MemoryConfig = payload
        .get("config")
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let output_locale = payload
        .get("outputLocale")
        .and_then(Value::as_str)
        .unwrap_or("");
    let projects = memory_records();
    // The host's indexed AI sessions are the extraction candidates; the agent
    // runs no live AI supervisor, so there are no runtime snapshots.
    let history_sessions = codux_ai_history::normalized::indexed_sessions_since_at(
        agent_data_dir().join("ai-usage.sqlite3"),
        None,
    )
    .map_err(|error| error.to_string())?;
    let runtime_sessions: Vec<MemorySessionSnapshot> = Vec::new();

    let service = service();
    service.enqueue_automatic_extraction_candidates(
        &config.memory,
        &projects,
        &runtime_sessions,
        &history_sessions,
    )?;
    let status = service
        .process_memory_extraction_queue(&config, &projects, output_locale)
        .await
        .and_then(|status| serde_json::to_value(status).map_err(|error| error.to_string()))?;
    Ok(json!({ "op": "extract", "result": status }))
}

/// The host's projects mapped into the engine's project shape (the manager view
/// labels rows per project).
fn memory_projects() -> Vec<MemoryProjectInfo> {
    AgentProjectStore::new()
        .list()
        .into_iter()
        .map(|project| MemoryProjectInfo {
            id: project.id,
            name: project.name,
            path: project.path,
        })
        .collect()
}

/// Resolve the host's own project id for a controller-supplied path. The
/// host's memory store is keyed by the host's project ids, but the controller
/// only knows its own ids; like `ai.state`, it sends the project *path* and the
/// host maps it to its local project (falling back to the supplied id).
fn host_project_id(payload: &Value) -> Option<String> {
    let project_path = payload.get("projectPath").and_then(Value::as_str);
    if let Some(path) = project_path.filter(|value| !value.is_empty())
        && let Some(project) = AgentProjectStore::new().list().into_iter().find(|project| {
            codux_runtime_core::path::local_paths_equal(
                std::path::Path::new(&project.path),
                std::path::Path::new(path),
            )
        })
    {
        return Some(project.id);
    }
    payload
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Serve a `memory.read` query. Returns `{op, result}` where `result` is the
/// operation's JSON snapshot.
pub fn memory_read_payload(payload: &Value) -> Result<Value, String> {
    let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
    let resolved_project_id = host_project_id(payload);
    let project_id = resolved_project_id.as_deref();
    let result = match op {
        "summary" => serde_json::to_value(service().summary(project_id))
            .map_err(|error| error.to_string())?,
        "status" => service()
            .extraction_status_snapshot()
            .and_then(|status| serde_json::to_value(status).map_err(|error| error.to_string()))?,
        "management" => {
            let mut request = serde_json::from_value::<MemoryManagementRequest>(payload.clone())
                .map_err(|error| error.to_string())?;
            request.project_id = project_id.map(str::to_string);
            service()
                .management_snapshot(request)
                .and_then(|snapshot| {
                    serde_json::to_value(snapshot).map_err(|error| error.to_string())
                })?
        }
        "manager" => {
            let scope = payload
                .get("scope")
                .and_then(Value::as_str)
                .unwrap_or("project");
            let tab = payload
                .get("tab")
                .and_then(Value::as_str)
                .unwrap_or("active");
            let limit = payload.get("limit").and_then(Value::as_i64).unwrap_or(500);
            serde_json::to_value(service().manager_snapshot(
                &memory_projects(),
                scope,
                project_id,
                tab,
                limit,
            ))
            .map_err(|error| error.to_string())?
        }
        _ => return Err(format!("Unsupported memory read operation: {op}")),
    };
    Ok(json!({ "op": op, "result": result }))
}
