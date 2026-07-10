use super::*;

impl RemoteHostRuntime {
    pub(super) fn handle_ai_stats(&self, envelope: &RemoteEnvelope) {
        let project_id = envelope
            .payload
            .get("projectId")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let project_store = ProjectStore::new(self.support_dir.clone());
        let project = project_store
            .projects_snapshot()
            .into_iter()
            .find(|project| project.id == project_id);
        let Some(project) = project else {
            self.send_error(envelope, "Project not found for AI stats.");
            return;
        };
        let current_session_scope_id = envelope
            .payload
            .get("worktreeId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&project.id)
            .to_string();
        let request = AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        };
        match self.ai_history.project_state(request) {
            Ok(state) => {
                // Register the requesting device as a watcher of this project so
                // we re-push fresh stats when the live AI runtime changes (and,
                // for a cold-on-request index, once the refresh completes).
                if let Some(device_id) = envelope
                    .device_id
                    .as_deref()
                    .filter(|value| !value.trim().is_empty())
                {
                    self.register_ai_stats_watcher(
                        &state.project_id,
                        device_id,
                        &current_session_scope_id,
                    );
                }
                match self.remote_ai_stats_payload(
                    project.id,
                    project.name,
                    state,
                    &current_session_scope_id,
                ) {
                    Ok(payload) => {
                        let payload_project_id = payload
                            .get("projectId")
                            .and_then(Value::as_str)
                            .map(str::to_string);
                        self.broadcast_resource_payload(
                            REMOTE_AI_STATS,
                            REMOTE_RESOURCE_AI_STATS,
                            envelope.device_id.as_deref(),
                            payload_project_id.as_deref(),
                            None,
                            payload,
                        );
                    }
                    Err(error) => self.send_error(envelope, &error),
                }
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Record that `device_id` is watching `project_id`'s `ai.stats` (a device
    /// watches at most one project, so drop its entries under any other project).
    pub(super) fn register_ai_stats_watcher(
        &self,
        project_id: &str,
        device_id: &str,
        scope_id: &str,
    ) {
        let Ok(mut watchers) = self.ai_stats_watchers.lock() else {
            return;
        };
        for (id, devices) in watchers.iter_mut() {
            if id != project_id {
                devices.remove(device_id);
            }
        }
        watchers.retain(|_, devices| !devices.is_empty());
        watchers
            .entry(project_id.to_string())
            .or_default()
            .insert(device_id.to_string(), scope_id.to_string());
    }

    /// Drop a disconnected device from every project's watcher set.
    pub(super) fn clear_ai_stats_watcher_device(&self, device_id: &str) {
        if let Ok(mut watchers) = self.ai_stats_watchers.lock() {
            for devices in watchers.values_mut() {
                devices.remove(device_id);
            }
            watchers.retain(|_, devices| !devices.is_empty());
        }
    }

    /// Re-push fresh `ai.stats` to every watcher. Called when the live AI runtime
    /// state changes so remote views tick like the desktop's local view.
    pub fn push_ai_stats_to_watchers(&self) {
        let snapshot = match self.ai_stats_watchers.lock() {
            Ok(watchers) => watchers.clone(),
            Err(_) => return,
        };
        for project_id in snapshot.keys() {
            self.push_ai_stats_for_project(project_id, &snapshot);
        }
    }

    /// Push freshly-indexed `ai.stats` to watchers of a project once its cold
    /// index refresh completes. No-op until the state is ready.
    pub fn flush_pending_ai_stats(&self, state: &AIHistoryProjectState) {
        if state.is_loading || state.queued {
            return;
        }
        let snapshot = match self.ai_stats_watchers.lock() {
            Ok(watchers) => watchers.clone(),
            Err(_) => return,
        };
        self.push_ai_stats_for_project(&state.project_id, &snapshot);
    }

    /// Build and send `ai.stats` to each device watching `project_id`, using each
    /// device's stored runtime session scope.
    pub(super) fn push_ai_stats_for_project(
        &self,
        project_id: &str,
        watchers: &HashMap<String, HashMap<String, String>>,
    ) {
        let Some(devices) = watchers
            .get(project_id)
            .filter(|devices| !devices.is_empty())
        else {
            return;
        };
        let project_store = ProjectStore::new(self.support_dir.clone());
        let Some(project) = project_store
            .projects_snapshot()
            .into_iter()
            .find(|project| project.id == project_id)
        else {
            return;
        };
        let request = AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        };
        let Ok(state) = self.ai_history.project_state(request) else {
            return;
        };
        // One version per push for this project; every watcher shares it. Keyed
        // by project so a stale stats frame for a previously-selected project
        // can't clobber the current one after a fast project switch.
        let version = self.resource_subscriptions.next_version(
            REMOTE_RESOURCE_AI_STATS,
            Some(project_id),
            None,
        );
        for (device_id, scope_id) in devices {
            let payload = match self.remote_ai_stats_payload(
                project.id.clone(),
                project.name.clone(),
                state.clone(),
                scope_id,
            ) {
                Ok(payload) => payload,
                Err(_) => continue,
            };
            let payload = with_resource_version(payload, version);
            self.send(REMOTE_AI_STATS, Some(device_id), None, payload);
        }
    }

    pub(super) fn remote_ai_stats_payload(
        &self,
        project_id: String,
        project_name: String,
        state: AIHistoryProjectState,
        current_session_scope_id: &str,
    ) -> Result<Value, String> {
        let current_sessions = self
            .ai_current_sessions
            .as_ref()
            .map(|provider| provider.current_sessions(current_session_scope_id))
            .unwrap_or_default();
        runtime_ai_stats::ai_stats_payload_from_state(
            project_id,
            project_name,
            state,
            current_sessions,
        )
    }

    /// Serve the full `AIHistoryProjectState` for a desktop controller, indexed
    /// from the path the controller sends (it owns the project record).
    pub(super) fn handle_ai_state(&self, envelope: &RemoteEnvelope) {
        let request = AIHistoryProjectRequest {
            id: envelope
                .payload
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            name: envelope
                .payload
                .get("projectName")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
            path: envelope
                .payload
                .get("projectPath")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string(),
        };
        match self.ai_history.project_state(request) {
            Ok(state) => match serde_json::to_value(state) {
                Ok(payload) => self.send(
                    REMOTE_AI_STATE,
                    envelope.device_id.as_deref(),
                    None,
                    payload,
                ),
                Err(error) => self.send_error(envelope, &error.to_string()),
            },
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Serve `ai.session` for a remote controller. Same channel + DTO the agent
    /// uses; the host owns the AI history, so it sends the lean session list.
    pub(super) fn handle_ai_session(&self, envelope: &RemoteEnvelope) {
        let payload = &envelope.payload;
        let op = payload.get("op").and_then(Value::as_str).unwrap_or("");
        let project_path = payload
            .get("projectPath")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .or_else(|| {
                let project_id = payload
                    .get("projectId")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let store = ProjectStore::new(self.support_dir.clone());
                store
                    .projects_snapshot()
                    .into_iter()
                    .find(|project| project.id == project_id)
                    .or_else(|| store.projects_snapshot().into_iter().next())
                    .map(|project| project.path)
            })
            .unwrap_or_default();
        let service = codux_ai_sessions::AIHistoryService::new(self.support_dir.clone());
        let result = codux_ai_sessions::session_op_result(&service, &project_path, payload);
        self.send(
            REMOTE_AI_SESSION_RESULT,
            envelope.device_id.as_deref(),
            None,
            json!({ "op": op, "result": result }),
        );
    }
}
