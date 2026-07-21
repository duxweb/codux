use super::*;

pub(super) fn default_project_name(path: &str) -> String {
    Path::new(path)
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("Project")
        .to_string()
}

impl RemoteHostRuntime {
    pub(super) fn handle_project_add(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_project_name(path));
        match ProjectStore::new(self.support_dir.clone()).create_project(ProjectCreateRequest {
            name,
            path: path.to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            environment_variables: Default::default(),
            runtime_target: ProjectRuntimeTarget::Local,
        }) {
            Ok(baseline) => {
                crate::runtime_state::note_pet_project_membership_change(&self.support_dir);
                let project_id = baseline.selected_project_id.unwrap_or_default();
                self.reply(
                    envelope,
                    REMOTE_PROJECT_UPDATED,
                    json!({ "action": "add", "projectId": project_id }),
                );
                self.broadcast_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_project_edit(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Project path is required.");
            return;
        };
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .map(str::to_string)
            .unwrap_or_else(|| default_project_name(path));
        match ProjectStore::new(self.support_dir.clone()).update_project_from_request(
            ProjectUpdateRequest {
                project_id: project_id.to_string(),
                name,
                path: path.to_string(),
                badge_text: None,
                badge_symbol: None,
                badge_color_hex: None,
                environment_variables: Default::default(),
                runtime_target: ProjectRuntimeTarget::Local,
            },
        ) {
            Ok(_) => {
                crate::runtime_state::note_pet_project_membership_change(&self.support_dir);
                self.reply(
                    envelope,
                    REMOTE_PROJECT_UPDATED,
                    json!({ "action": "edit", "projectId": project_id }),
                );
                self.broadcast_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_project_remove(&self, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        match ProjectStore::new(self.support_dir.clone()).close_project(project_id) {
            Ok(_) => {
                crate::runtime_state::note_pet_project_membership_change(&self.support_dir);
                self.remove_project_state(project_id);
                self.reply(
                    envelope,
                    REMOTE_PROJECT_UPDATED,
                    json!({ "action": "remove", "projectId": project_id }),
                );
                self.broadcast_project_and_terminal_lists(envelope.device_id.as_deref());
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_project_select(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(project_id) = envelope.payload.get("projectId").and_then(Value::as_str) else {
            self.send_error(envelope, "Project id is required.");
            return;
        };
        crate::runtime_trace::runtime_trace(
            "remote",
            &format!(
                "project_select start device={} project={project_id}",
                envelope.device_id.as_deref().unwrap_or("")
            ),
        );
        let preferred_worktree_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let device_id = envelope.device_id.as_deref();
        match self.remote_project_scope_with_worktree(project_id, preferred_worktree_id) {
            Ok(scope) => {
                if let Err(error) = self.ensure_remote_project_terminal(&scope) {
                    crate::runtime_trace::runtime_trace(
                        "remote",
                        &format!(
                            "project_select error device={} project={project_id} error={error}",
                            envelope.device_id.as_deref().unwrap_or("")
                        ),
                    );
                    self.send_error(envelope, &error);
                    return;
                }
                self.set_remote_project_scope(device_id, &scope.project_id);
                self.reply(
                    envelope,
                    REMOTE_PROJECT_SELECTED,
                    json!({ "projectId": scope.project_id, "worktreeId": scope.worktree_id }),
                );
                self.send_project_and_terminal_snapshots(envelope.device_id.as_deref());
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "project_select ok device={} project={}",
                        envelope.device_id.as_deref().unwrap_or(""),
                        scope.project_id
                    ),
                );
            }
            Err(error) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "project_select error device={} project={project_id} error={error}",
                        envelope.device_id.as_deref().unwrap_or("")
                    ),
                );
                self.send_error(envelope, &error)
            }
        }
    }

    pub(super) fn remote_project_list_payload(&self, device_id: Option<&str>) -> Value {
        let store = ProjectStore::new(self.support_dir.clone());
        let baseline = store.list_snapshot();
        // Only advertise local projects to a controller. A project backed by
        // another runtime target lives there; this host can't
        // serve its terminal — it would spawn a wrong local shell because the
        // remote path doesn't exist here. Chained host→host forwarding isn't
        // supported, so hiding them keeps the controller from opening a project
        // it can never actually use, so derive the local set from full records.
        let local_ids: HashSet<String> = store
            .projects_snapshot()
            .into_iter()
            .filter(|project| project.runtime_target.is_local())
            .map(|project| project.id)
            .collect();
        let selected_project_id = self
            .remote_project_scope_id(device_id)
            .filter(|id| local_ids.contains(id))
            .or_else(|| {
                baseline
                    .selected_project_id
                    .filter(|id| local_ids.contains(id))
            })
            .or_else(|| {
                baseline
                    .projects
                    .iter()
                    .find(|project| local_ids.contains(&project.id))
                    .map(|project| project.id.clone())
            });
        runtime_project::project_list_payload_with_worktrees(
            baseline
                .projects
                .into_iter()
                .filter(|project| local_ids.contains(&project.id))
                .map(|project| runtime_project::ProjectListItem {
                    id: project.id,
                    name: project.name,
                    path: project.path,
                }),
            selected_project_id,
            None,
            store
                .snapshot()
                .worktrees
                .into_iter()
                .filter(|worktree| local_ids.contains(&worktree.project_id))
                .map(|worktree| runtime_project::ProjectWorktreeListItem {
                    id: worktree.id,
                    project_id: worktree.project_id,
                    name: worktree.name,
                    branch: worktree.branch,
                    path: worktree.path,
                    status: worktree.status,
                    is_default: worktree.is_default,
                    exists: true,
                }),
        )
    }
}
