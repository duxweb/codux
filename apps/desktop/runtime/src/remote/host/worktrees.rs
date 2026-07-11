use super::*;

impl RemoteHostRuntime {
    pub(super) fn handle_worktree_list(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        self.reply_worktree_summary(envelope, REMOTE_WORKTREE_LIST, &project_id, &project_path);
    }

    pub(super) fn handle_worktree_select(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_id) = envelope.payload.get("worktreeId").and_then(Value::as_str) else {
            self.send_error(envelope, "Worktree id is required.");
            return;
        };
        let service = WorktreeService::new(self.support_dir.clone());
        let mut summary = service.summary(Some(&project_id), Some(&project_path));
        let exists = summary
            .worktrees
            .iter()
            .any(|worktree| worktree.id == worktree_id);
        if !exists {
            self.send_error(envelope, "Worktree not found.");
            return;
        }
        summary.selected_worktree_id = Some(worktree_id.to_string());
        self.set_remote_project_scope(envelope.device_id.as_deref(), &project_id);
        if let Ok(scope) = self.remote_project_scope_for_envelope(envelope, Some(&project_id))
            && let Err(error) = self.ensure_remote_project_terminal(&scope)
        {
            self.send_error(envelope, &error);
            return;
        }
        self.reply(
            envelope,
            REMOTE_WORKTREE_UPDATED,
            remote_worktree_summary_payload(&project_id, summary),
        );
        self.send_project_and_terminal_snapshots(envelope.device_id.as_deref());
    }

    pub(super) fn handle_worktree_create(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(branch_name) = envelope.payload.get("branchName").and_then(Value::as_str) else {
            self.send_error(envelope, "Branch name is required.");
            return;
        };
        let request = WorktreeCreateRequest {
            project_id: project_id.clone(),
            project_path: project_path.clone(),
            base_branch: envelope
                .payload
                .get("baseBranch")
                .and_then(Value::as_str)
                .map(str::to_string),
            branch_name: branch_name.to_string(),
            task_title: envelope
                .payload
                .get("taskTitle")
                .and_then(Value::as_str)
                .map(str::to_string),
        };
        match WorktreeService::new(self.support_dir.clone()).create_from_request(request) {
            Ok(baseline) => {
                let git = crate::git::GitService::status(&project_path);
                self.reply_and_broadcast_resource_change(
                    envelope,
                    REMOTE_WORKTREE_UPDATED,
                    REMOTE_RESOURCE_WORKTREES,
                    Some(&project_id),
                    None,
                    remote_worktree_update_payload(project_id.clone(), baseline, git),
                );
                self.send_project_and_terminal_snapshots(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_worktree_merge(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_path) = envelope.payload.get("worktreePath").and_then(Value::as_str)
        else {
            self.send_error(envelope, "Worktree path is required.");
            return;
        };
        let request = WorktreeMergeRequest {
            project_id: project_id.clone(),
            project_path: project_path.clone(),
            worktree_path: worktree_path.to_string(),
            base_branch: envelope
                .payload
                .get("baseBranch")
                .and_then(Value::as_str)
                .map(str::to_string),
            remove_branch: envelope
                .payload
                .get("removeBranch")
                .and_then(Value::as_bool),
        };
        match WorktreeService::new(self.support_dir.clone()).merge_from_request(request) {
            Ok(baseline) => {
                let git = crate::git::GitService::status(&project_path);
                self.reply_and_broadcast_resource_change(
                    envelope,
                    REMOTE_WORKTREE_UPDATED,
                    REMOTE_RESOURCE_WORKTREES,
                    Some(&project_id),
                    None,
                    remote_worktree_update_payload(project_id.clone(), baseline, git),
                );
                self.send_project_and_terminal_snapshots(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_worktree_remove(&self, envelope: &RemoteEnvelope) {
        let Ok((project_id, project_path)) = self.worktree_request_scope(envelope) else {
            self.send_error(envelope, "Project id and path are required.");
            return;
        };
        let Some(worktree_path) = envelope.payload.get("worktreePath").and_then(Value::as_str)
        else {
            self.send_error(envelope, "Worktree path is required.");
            return;
        };
        let request = WorktreeRemoveRequest {
            project_id: project_id.clone(),
            project_path: project_path.clone(),
            worktree_path: worktree_path.to_string(),
            remove_branch: envelope
                .payload
                .get("removeBranch")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        };
        match WorktreeService::new(self.support_dir.clone()).remove_from_request(request) {
            Ok(baseline) => {
                let git = crate::git::GitService::status(&project_path);
                self.reply_and_broadcast_resource_change(
                    envelope,
                    REMOTE_WORKTREE_UPDATED,
                    REMOTE_RESOURCE_WORKTREES,
                    Some(&project_id),
                    None,
                    remote_worktree_update_payload(project_id.clone(), baseline, git),
                );
                self.send_project_and_terminal_snapshots(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }
}

pub(super) fn with_resource_version(mut payload: Value, version: u64) -> Value {
    if let Some(object) = payload.as_object_mut() {
        object.insert("version".to_string(), json!(version));
    }
    payload
}
pub(super) fn remote_worktree_summary_payload(
    project_id: &str,
    summary: crate::worktree::WorktreeSummary,
) -> Value {
    let base_branches = remote_worktree_base_branches(&summary.active_git);
    let default_base_branch = remote_default_worktree_base_branch(&summary.active_git);
    runtime_worktree::worktree_summary_payload(runtime_worktree::WorktreeSummaryPayload {
        project_id: project_id.to_string(),
        selected_worktree_id: summary.selected_worktree_id,
        worktrees: serde_json::to_value(summary.worktrees).unwrap_or_else(|_| json!([])),
        tasks: serde_json::to_value(summary.tasks).unwrap_or_else(|_| json!([])),
        available: summary.available,
        base_branches,
        default_base_branch,
        error: summary.error,
    })
}

pub(super) fn remote_worktree_update_payload(
    project_id: String,
    baseline: crate::worktree::WorktreeSnapshot,
    git: crate::git::GitSummary,
) -> Value {
    runtime_worktree::worktree_update_payload(
        project_id,
        baseline.selected_worktree_id,
        serde_json::to_value(baseline.worktrees).unwrap_or_else(|_| json!([])),
        serde_json::to_value(baseline.tasks).unwrap_or_else(|_| json!([])),
        remote_worktree_base_branches(&git),
        remote_default_worktree_base_branch(&git),
        baseline.error,
    )
}
pub(super) fn remote_worktree_base_branches(git: &crate::git::GitSummary) -> Vec<String> {
    runtime_worktree::worktree_base_branches(
        &git.branch,
        &crate::git::wire::wire_branches(&git.branches),
    )
}

pub(super) fn remote_default_worktree_base_branch(git: &crate::git::GitSummary) -> String {
    runtime_worktree::default_worktree_base_branch(
        &git.branch,
        &crate::git::wire::wire_branches(&git.branches),
    )
}
