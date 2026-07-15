use super::*;
use codux_memory::{
    MemoryConfig, MemoryManagementRequest, MemoryProjectInfo, MemoryProjectRecord, MemoryService,
};

impl RemoteHostRuntime {
    pub(super) fn handle_memory_read(&self, envelope: &RemoteEnvelope) {
        match memory_read_payload(&self.support_dir, &envelope.payload) {
            Ok(payload) => self.reply(envelope, REMOTE_MEMORY_RESULT, payload),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_memory_extract(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let runtime = Arc::clone(self);
        let envelope = envelope.clone();
        crate::async_runtime::spawn(async move {
            match memory_extract_payload(&runtime.support_dir, &envelope.payload).await {
                Ok(payload) => runtime.reply(&envelope, REMOTE_MEMORY_RESULT, payload),
                Err(error) => runtime.send_error(&envelope, &error),
            }
        });
    }
}

fn memory_service(support_dir: &Path) -> MemoryService {
    MemoryService::new(support_dir.to_path_buf())
}

fn memory_projects(support_dir: &Path) -> Vec<MemoryProjectInfo> {
    ProjectStore::new(support_dir.to_path_buf())
        .project_summaries()
        .into_iter()
        .map(|project| MemoryProjectInfo {
            id: project.id,
            name: project.name,
            path: project.path,
        })
        .collect()
}

fn memory_project_records(support_dir: &Path) -> Vec<MemoryProjectRecord> {
    crate::memory::memory_project_records(
        &ProjectStore::new(support_dir.to_path_buf()).project_workspaces_snapshot(),
    )
}

fn host_project_id(support_dir: &Path, payload: &Value) -> Option<String> {
    if let Some(path) = payload
        .get("projectPath")
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        && let Some(project) = ProjectStore::new(support_dir.to_path_buf())
            .projects_snapshot()
            .into_iter()
            .find(|project| {
                crate::path::local_paths_equal(Path::new(&project.path), Path::new(path))
            })
    {
        return Some(project.id);
    }
    payload
        .get("projectId")
        .and_then(Value::as_str)
        .map(str::to_string)
}

fn memory_read_payload(support_dir: &Path, payload: &Value) -> Result<Value, String> {
    let op = payload
        .get("op")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let project_id = host_project_id(support_dir, payload);
    let service = memory_service(support_dir);
    let result = match op {
        "summary" => serde_json::to_value(service.summary(project_id.as_deref()))
            .map_err(|error| error.to_string())?,
        "status" => service
            .extraction_status_snapshot()
            .and_then(|status| serde_json::to_value(status).map_err(|error| error.to_string()))?,
        "management" => {
            let mut request = serde_json::from_value::<MemoryManagementRequest>(payload.clone())
                .map_err(|error| error.to_string())?;
            request.project_id = project_id;
            service.management_snapshot(request).and_then(|snapshot| {
                serde_json::to_value(snapshot).map_err(|error| error.to_string())
            })?
        }
        "manager" => serde_json::to_value(
            service.manager_snapshot(
                &memory_projects(support_dir),
                payload
                    .get("scope")
                    .and_then(Value::as_str)
                    .unwrap_or("project"),
                project_id.as_deref(),
                payload
                    .get("tab")
                    .and_then(Value::as_str)
                    .unwrap_or("active"),
                payload.get("limit").and_then(Value::as_i64).unwrap_or(500),
            ),
        )
        .map_err(|error| error.to_string())?,
        _ => return Err(format!("Unsupported memory read operation: {op}")),
    };
    Ok(json!({ "op": op, "result": result }))
}

async fn memory_extract_payload(support_dir: &Path, payload: &Value) -> Result<Value, String> {
    let config = payload
        .get("config")
        .cloned()
        .map(serde_json::from_value::<MemoryConfig>)
        .transpose()
        .map_err(|error| error.to_string())?
        .unwrap_or_default();
    let projects = memory_project_records(support_dir);
    let history_sessions = codux_ai_history::normalized::indexed_sessions_since_at(
        support_dir.join("ai-usage.sqlite3"),
        None,
    )
    .map_err(|error| error.to_string())?;
    let service = memory_service(support_dir);
    service.enqueue_automatic_extraction_candidates(
        &config.memory,
        &projects,
        &[],
        &history_sessions,
    )?;
    let result = service
        .process_memory_extraction_queue(
            &config,
            &projects,
            payload
                .get("outputLocale")
                .and_then(Value::as_str)
                .unwrap_or_default(),
        )
        .await
        .and_then(|status| serde_json::to_value(status).map_err(|error| error.to_string()))?;
    Ok(json!({ "op": "extract", "result": result }))
}
