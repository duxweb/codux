use super::*;

impl RemoteHostRuntime {
    pub(super) fn handle_git_status(&self, envelope: &RemoteEnvelope) {
        // Status may fall back to the first project (legacy id-less pulls);
        // mutations and reads never guess.
        let project = self.git_project_from_envelope(envelope).or_else(|| {
            ProjectStore::new(self.support_dir.clone())
                .projects_snapshot()
                .into_iter()
                .next()
                .map(|project| (project.id, project.path))
        });
        let Some((project_id, project_path)) = project else {
            self.send_error(envelope, "Unable to load Git status.");
            return;
        };
        let summary = crate::git::GitService::status(&project_path);
        self.reply_resource_payload(
            envelope,
            REMOTE_GIT_STATUS,
            REMOTE_RESOURCE_GIT_STATUS,
            Some(&project_id),
            None,
            remote_git_status_payload(project_id.clone(), project_path, summary),
        );
    }

    /// Generic git mutation (`git.invoke`) → GitService, then reply with
    /// refreshed status (the controller maps it back into a GitSummary).
    pub(super) fn handle_git_invoke(&self, envelope: &RemoteEnvelope) {
        let Some((project_id, project_path)) = self.git_project_from_envelope(envelope) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let op = envelope
            .payload
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = envelope.payload.get("args").cloned().unwrap_or(Value::Null);
        match crate::git::wire::invoke(project_path.as_str(), op, &args) {
            Ok(_) => {
                let summary = crate::git::GitService::status(project_path.as_str());
                self.reply_and_broadcast_resource_change(
                    envelope,
                    REMOTE_GIT_STATUS,
                    REMOTE_RESOURCE_GIT_STATUS,
                    Some(&project_id),
                    None,
                    remote_git_status_payload(project_id.clone(), project_path, summary),
                );
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    fn git_project_from_envelope(&self, envelope: &RemoteEnvelope) -> Option<(String, String)> {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let project_path = envelope
            .payload
            .get("projectPath")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .trim();
        let project_store = ProjectStore::new(self.support_dir.clone());
        let projects = project_store.projects_snapshot();
        if !project_id.is_empty()
            && let Some(project) = projects.iter().find(|project| project.id == project_id)
        {
            return Some((project.id.clone(), project.path.clone()));
        }
        if !project_path.is_empty()
            && let Some(project) = projects.iter().find(|project| project.path == project_path)
        {
            return Some((project.id.clone(), project.path.clone()));
        }
        if !project_path.is_empty() {
            return Some((project_id.to_string(), project_path.to_string()));
        }
        None
    }

    /// Generic git read (`git.read`) → `{op, result}`.
    pub(super) fn handle_git_read(&self, envelope: &RemoteEnvelope) {
        let Some((project_id, project_path)) = self.git_project_from_envelope(envelope) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let op = envelope
            .payload
            .get("op")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let args = envelope.payload.get("args").cloned().unwrap_or(Value::Null);
        // `stored_state` is a full status payload (needs the project envelope),
        // so it stays host-side; every other read op shares the engine table.
        let result: Result<Value, String> = if op == "stored_state" {
            Ok(remote_git_status_payload(
                project_id,
                project_path.clone(),
                crate::git::GitService::status(&project_path),
            ))
        } else {
            crate::git::wire::read(&project_path, op, &args)
        };
        match result {
            Ok(result) => self.reply(
                envelope,
                REMOTE_GIT_READ,
                json!({ "op": op, "result": result }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }
}

pub(crate) fn remote_git_status_payload(
    project_id: String,
    project_path: String,
    summary: crate::git::GitSummary,
) -> Value {
    runtime_git::git_status_payload(
        project_id,
        project_path,
        crate::git::wire::wire_status_summary(summary),
    )
}
