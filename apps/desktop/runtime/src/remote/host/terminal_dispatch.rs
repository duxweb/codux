use super::*;
use codux_runtime_live::remote_terminal_dispatch;

impl RemoteHostRuntime {
    pub(super) fn handle_terminal_subscribe(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(device_id) = envelope.device_id.as_deref() else {
            return;
        };
        match RemoteTerminalSubscriptionTarget::from_payload(
            envelope.session_id.as_deref(),
            &envelope.payload,
        ) {
            Ok(RemoteTerminalSubscriptionTarget::Project { project_id }) => {
                self.register_project_terminal_subscription(&project_id, Some(device_id));
                self.send_terminal_list(Some(device_id));
            }
            Ok(RemoteTerminalSubscriptionTarget::Session { session_id }) => {
                self.register_terminal_viewer(&session_id, Some(device_id));
                self.send_terminal_viewport_state(&session_id, Some(device_id));
                if RemoteTerminalSubscriptionTarget::baseline_requested(&envelope.payload) {
                    self.spawn_terminal_baseline(&session_id, Some(device_id), envelope);
                }
            }
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_terminal_unsubscribe(&self, envelope: &RemoteEnvelope) {
        let Some(device_id) = envelope.device_id.as_deref() else {
            return;
        };
        match RemoteTerminalSubscriptionTarget::from_payload(
            envelope.session_id.as_deref(),
            &envelope.payload,
        ) {
            Ok(RemoteTerminalSubscriptionTarget::Project { project_id }) => {
                self.remove_project_terminal_subscription(&project_id, Some(device_id));
            }
            Ok(RemoteTerminalSubscriptionTarget::Session { session_id }) => {
                self.remove_terminal_viewer_for_session(&session_id, Some(device_id));
            }
            Err(_) => {}
        }
    }

    pub(super) fn handle_resource_subscribe(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let change = match RuntimeSubscriptionRouter::parse_subscribe_envelope(envelope) {
            Ok(change) => change,
            Err(error) => {
                self.send_error(envelope, &error);
                return;
            }
        };
        if let Err(error) = self.validate_resource_subscription(&change) {
            self.send_error(envelope, &error);
            return;
        }
        self.resource_subscriptions.subscribe_change(&change);
        match change.resource.as_str() {
            REMOTE_RESOURCE_PROJECTS => self.reply_project_list(envelope),
            REMOTE_RESOURCE_TERMINALS => {
                if change.project_id.is_some() {
                    self.reply_terminal_list(envelope);
                } else if let Some(session_id) = change.session_id.as_deref() {
                    self.activate_terminal_viewer(session_id);
                    self.send_terminal_viewport_state(session_id, envelope.device_id.as_deref());
                    if change.baseline {
                        self.spawn_terminal_baseline(
                            session_id,
                            envelope.device_id.as_deref(),
                            envelope,
                        );
                    }
                } else {
                    self.reply_terminal_list(envelope);
                }
            }
            REMOTE_RESOURCE_WORKTREES => self.handle_worktree_list(envelope),
            REMOTE_RESOURCE_GIT_STATUS => self.handle_git_status(envelope),
            REMOTE_RESOURCE_AI_STATS => self.handle_ai_stats(envelope),
            _ => unreachable!("validated resource subscription"),
        }
    }

    fn validate_resource_subscription(
        &self,
        change: &codux_runtime_core::subscription::RuntimeSubscriptionChange,
    ) -> Result<(), String> {
        let project_id = change.project_id.as_deref();
        let session_id = change.session_id.as_deref();
        match change.resource.as_str() {
            REMOTE_RESOURCE_PROJECTS => {
                if project_id.is_some() || session_id.is_some() {
                    return Err("Projects subscriptions must use global scope.".to_string());
                }
            }
            REMOTE_RESOURCE_TERMINALS => {
                if project_id.is_some() && session_id.is_some() {
                    return Err(
                        "Terminal subscriptions cannot combine project and session scope."
                            .to_string(),
                    );
                }
                if let Some(project_id) = project_id {
                    self.validate_subscription_project(project_id)?;
                }
                if let Some(session_id) = session_id
                    && !self
                        .terminals
                        .list()
                        .iter()
                        .any(|terminal| terminal.id == session_id && terminal.is_running)
                {
                    return Err("Terminal session not found.".to_string());
                }
            }
            REMOTE_RESOURCE_WORKTREES | REMOTE_RESOURCE_GIT_STATUS | REMOTE_RESOURCE_AI_STATS => {
                if session_id.is_some() {
                    return Err("Resource subscription does not support session scope.".to_string());
                }
                let project_id = project_id
                    .ok_or_else(|| "Project id is required for this resource.".to_string())?;
                self.validate_subscription_project(project_id)?;
            }
            _ => return Err("Unsupported resource subscription.".to_string()),
        }
        Ok(())
    }

    fn validate_subscription_project(&self, project_id: &str) -> Result<(), String> {
        let exists = ProjectStore::new(self.support_dir.clone())
            .projects_snapshot()
            .into_iter()
            .any(|project| {
                project.id == project_id
                    && project
                        .host_device_id
                        .as_deref()
                        .map(str::trim)
                        .unwrap_or_default()
                        .is_empty()
            });
        if exists {
            Ok(())
        } else {
            Err("Project not found.".to_string())
        }
    }

    pub(super) fn handle_resource_unsubscribe(&self, envelope: &RemoteEnvelope) {
        let Ok(change) = self.resource_subscriptions.unsubscribe_envelope(envelope) else {
            return;
        };
        match change.resource.as_str() {
            REMOTE_RESOURCE_TERMINALS => {
                if let Some(session_id) = change.session_id.as_deref() {
                    self.clear_terminal_viewer_state(session_id, &change.device_id);
                }
            }
            REMOTE_RESOURCE_AI_STATS => {
                self.remove_ai_stats_watcher(change.project_id.as_deref(), &change.device_id)
            }
            _ => {}
        }
    }

    pub(super) fn handle_terminal_create(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let runtime = Arc::clone(self);
        let emit = move |event| {
            runtime.handle_terminal_event(event);
        };
        // A controller passes its stable `terminalId` so we key the session by it
        // and `attach_or_create` RE-ATTACHES to the still-running shell on a later
        // open (persistent remote terminals) instead of spawning a new one.
        let requested_terminal_id = envelope
            .payload
            .get("terminalId")
            .and_then(Value::as_str)
            .filter(|value| !value.trim().is_empty());
        let plan =
            match self.remote_terminal_plan_from_envelope(envelope, requested_terminal_id, false) {
                Ok(plan) => plan,
                Err(error) => {
                    self.send_error(envelope, &error);
                    return;
                }
            };
        self.set_remote_project_scope(envelope.device_id.as_deref(), &plan.scope.project_id);
        let lifecycle = prepare_terminal_create_lifecycle(
            self.terminals.as_ref(),
            &plan.config,
            envelope.device_id.as_deref(),
            |session_id, device_id| {
                self.resource_subscriptions.subscribe(
                    REMOTE_RESOURCE_TERMINALS,
                    None,
                    Some(session_id),
                    device_id,
                );
            },
        );
        let event_key = plan
            .config
            .terminal_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .map(|terminal_id| format!("remote-terminal:{terminal_id}"));
        let create_result = if let Some(event_key) = event_key {
            self.terminals.create_with_event_key(
                plan.config,
                event_key,
                Arc::new(move |event| {
                    emit(event);
                    true
                }),
            )
        } else {
            self.terminals.create(plan.config, emit)
        };
        match create_result {
            Ok(session_id) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "terminal_create session={session_id} reattaching={} device={}",
                        lifecycle.reattaching,
                        envelope.device_id.as_deref().unwrap_or("none")
                    ),
                );
                self.persist_remote_terminal_layout(
                    &plan.scope.layout_key,
                    &session_id,
                    &plan.title,
                );
                self.publish_remote_terminal_layout_changed();
                self.mark_terminal_event_subscription(&session_id);
                finish_terminal_create_viewer_lifecycle(
                    &session_id,
                    envelope.device_id.as_deref(),
                    |session_id, device_id| {
                        self.register_terminal_viewer(session_id, Some(device_id))
                    },
                );
                self.reply_with_session(
                    envelope,
                    Some(&session_id),
                    REMOTE_TERMINAL_CREATED,
                    self.remote_terminal_payload(&session_id)
                        .unwrap_or_else(|| json!({ "id": session_id })),
                );
                self.broadcast_terminal_list(envelope.device_id.as_deref());
                self.send_terminal_viewport_state(&session_id, envelope.device_id.as_deref());
                if lifecycle.reattaching {
                    self.send_terminal_buffer(
                        &session_id,
                        envelope.device_id.as_deref(),
                        0,
                        TerminalBaselineOptions {
                            max_chars: REMOTE_TERMINAL_BUFFER_MAX_CHARS,
                            chunk_chars: None,
                            request_id: None,
                            tail: true,
                            viewport: None,
                        },
                    );
                }
            }
            Err(error) => {
                rollback_terminal_create_viewer_lifecycle(
                    &lifecycle,
                    envelope.device_id.as_deref(),
                    |session_id, device_id| {
                        self.remove_terminal_viewer_for_session(session_id, Some(device_id));
                    },
                );
                self.send_error(envelope, &error.to_string());
            }
        }
    }

    pub(super) fn handle_terminal_buffer(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let offset = envelope
            .payload
            .get("offset")
            .and_then(Value::as_u64)
            .unwrap_or(0) as usize;
        let options = self.terminal_baseline_options(envelope, false);
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            crate::runtime_trace::runtime_trace(
                "remote",
                &format!("terminal_buffer start_failed session={session_id} error={error}"),
            );
            self.send_error(envelope, &error);
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        self.send_terminal_buffer(session_id, envelope.device_id.as_deref(), offset, options);
    }

    /// Friendly name of a connected device, looked up by device_id in the
    /// paired-device cache (for the desktop "handed off" placeholder). None when
    /// unknown / unnamed.
    pub(super) fn device_name_for(&self, device_id: &str) -> Option<String> {
        if device_id.trim().is_empty() {
            return None;
        }
        let raw = self.service().raw_settings();
        let settings = super::remote_settings_from_raw(&raw);
        settings
            .cached_devices
            .into_iter()
            .find(|device| device.id == device_id)
            .map(|device| device.name)
            .filter(|name| !name.trim().is_empty())
    }

    pub(super) fn handle_terminal_input(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            self.send_error(envelope, "Terminal session is required.");
            return;
        };
        let Some(data) = envelope.payload.get("data").and_then(Value::as_str) else {
            self.send_error(envelope, "Terminal input is required.");
            return;
        };
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let owner = self.remote_viewport_owner(envelope);
        let is_owner = match self.terminals.viewport_state(session_id) {
            Ok(state) if state.owner == owner => true,
            Ok(state) if state.owner == crate::terminal_pty::terminal_viewport_local_owner() => {
                self.terminals
                    .claim_viewport_auto(session_id, &owner)
                    .is_ok_and(|state| state.owner == owner)
            }
            Ok(_) => false,
            Err(_) => true,
        };
        if !is_owner {
            if let Some(input_id) = envelope.payload.get("inputId").and_then(Value::as_str) {
                self.send_terminal_data(
                    REMOTE_TERMINAL_INPUT_ACK,
                    envelope.device_id.as_deref(),
                    Some(session_id),
                    json!({ "inputId": input_id, "ok": false, "accepted": false }),
                );
            }
            return;
        }
        self.terminals.touch_viewport_lease(session_id, &owner);
        if let Some(input_id) = envelope.payload.get("inputId").and_then(Value::as_str) {
            self.send_terminal_data(
                REMOTE_TERMINAL_INPUT_ACK,
                envelope.device_id.as_deref(),
                Some(session_id),
                json!({ "inputId": input_id, "ok": true, "accepted": true }),
            );
        }
        if let Err(error) = self.ensure_remote_terminal_started(session_id, envelope) {
            self.send_error(envelope, &error);
            return;
        }
        if let Err(error) = self.terminals.write(session_id, data.as_bytes()) {
            self.send_error(envelope, &error.to_string());
        }
    }

    pub(super) fn handle_terminal_resize(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let Some(cols) = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| *value > 0)
        else {
            self.send_error(envelope, "terminal.resize requires positive cols.");
            return;
        };
        let Some(rows) = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .and_then(|value| u16::try_from(value).ok())
            .filter(|value| *value > 0)
        else {
            self.send_error(envelope, "terminal.resize requires positive rows.");
            return;
        };
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let owner = self.remote_viewport_owner(envelope);
        let _ = self.terminals.claim_viewport_auto(session_id, &owner);
        self.resize_terminal_viewport_from_envelope(session_id, envelope, cols, rows);
    }

    pub(super) fn handle_terminal_viewport_claim(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let owner = self.remote_viewport_owner(envelope);
        // Renew keeps the current phone lease alive without changing ownership;
        // auto and force are handled by the shared lease state machine below.
        let intent = remote_terminal_dispatch::terminal_viewport_claim_intent(&envelope.payload);
        if intent == remote_terminal_dispatch::TerminalViewportClaimIntent::Renew {
            self.terminals.touch_viewport_lease(session_id, &owner);
            // Echo the authoritative owner back ONLY when the phone has actually
            // lost the lease (desktop/another device took over): it needs to learn
            // the new owner and flip to the placeholder. If it still owns, the
            // keepalive is a silent lease renewal -- echoing its own ownership back
            // every 8s would just trigger a redundant repaint on an idle terminal.
            if let Ok(state) = self.terminals.viewport_state(session_id)
                && state.owner != owner
            {
                self.send_terminal_viewport_state_payload(
                    session_id,
                    envelope.device_id.as_deref(),
                    envelope.request_id.as_deref(),
                    &state,
                );
            }
            return;
        }
        let state = match intent {
            remote_terminal_dispatch::TerminalViewportClaimIntent::Auto => {
                self.terminals.claim_viewport_auto(session_id, &owner)
            }
            remote_terminal_dispatch::TerminalViewportClaimIntent::Force => {
                self.terminals.claim_viewport(session_id, &owner)
            }
            remote_terminal_dispatch::TerminalViewportClaimIntent::Renew => unreachable!(),
        };
        if let Ok(state) = state {
            let label = envelope
                .device_id
                .as_deref()
                .and_then(|id| self.device_name_for(id));
            self.terminals
                .set_viewport_owner_label(session_id, &owner, label);
            self.send_terminal_viewport_state_payload(
                session_id,
                envelope.device_id.as_deref(),
                envelope.request_id.as_deref(),
                &state,
            );
        }
    }

    pub(super) fn handle_terminal_viewport_resize(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let Some(cols) = envelope
            .payload
            .get("cols")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
        else {
            return;
        };
        let Some(rows) = envelope
            .payload
            .get("rows")
            .and_then(Value::as_u64)
            .map(|value| value as u16)
        else {
            return;
        };
        if self
            .ensure_remote_terminal_started(session_id, envelope)
            .is_err()
        {
            return;
        }
        self.register_terminal_viewer(session_id, envelope.device_id.as_deref());
        let owner = self.remote_viewport_owner(envelope);
        let _ = self.terminals.claim_viewport_auto(session_id, &owner);
        self.resize_terminal_viewport_from_envelope(session_id, envelope, cols, rows);
    }

    pub(super) fn resize_terminal_viewport_from_envelope(
        self: &Arc<Self>,
        session_id: &str,
        envelope: &RemoteEnvelope,
        cols: u16,
        rows: u16,
    ) {
        let owner = self.remote_viewport_owner(envelope);
        let label = envelope
            .device_id
            .as_deref()
            .and_then(|id| self.device_name_for(id));
        self.terminals
            .set_viewport_owner_label(session_id, &owner, label);
        match self
            .terminals
            .resize_viewport(session_id, &owner, cols, rows)
        {
            Ok(Some(state)) => {
                self.send_terminal_viewport_state_payload(
                    session_id,
                    envelope.device_id.as_deref(),
                    envelope.request_id.as_deref(),
                    &state,
                );
            }
            Ok(None) => {
                if let Ok(state) = self.terminals.viewport_state(session_id) {
                    self.send_terminal_viewport_state_payload(
                        session_id,
                        envelope.device_id.as_deref(),
                        envelope.request_id.as_deref(),
                        &state,
                    );
                }
            }
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    pub(super) fn remote_viewport_owner(&self, envelope: &RemoteEnvelope) -> String {
        envelope
            .device_id
            .as_deref()
            .map(terminal_viewport_remote_owner)
            .unwrap_or_else(|| "remote".to_string())
    }

    pub(super) fn handle_terminal_close(&self, envelope: &RemoteEnvelope) {
        let Some(session_id) = envelope.session_id.as_deref() else {
            return;
        };
        let requesting_device = envelope
            .device_id
            .as_deref()
            .filter(|device_id| !device_id.trim().is_empty());
        let restore_requesting_viewer = requesting_device
            .is_some_and(|device_id| self.terminal_output_viewers(session_id).contains(device_id));
        let terminal_exit_will_broadcast = self
            .terminal_event_subscriptions
            .lock()
            .map(|subscriptions| subscriptions.contains(session_id))
            .unwrap_or(false);
        if let Some(device_id) = requesting_device {
            self.remove_terminal_viewer_for_session(session_id, Some(device_id));
        }
        match self
            .terminals
            .kill_and_wait_if_present(session_id, Duration::from_secs(10))
        {
            Ok(_) => {}
            Err(error) => {
                if restore_requesting_viewer && let Some(device_id) = requesting_device {
                    self.resource_subscriptions.subscribe(
                        REMOTE_RESOURCE_TERMINALS,
                        None,
                        Some(session_id),
                        device_id,
                    );
                    self.terminals.restore_remote_screen_scrollback(session_id);
                }
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!("terminal close failed session={session_id} error={error}"),
                );
                self.send_error(envelope, &error.to_string());
                self.send_terminal_list(envelope.device_id.as_deref());
                return;
            }
        }
        let layout_removed = self.remove_remote_terminal_from_layout(session_id);
        self.clear_terminal_output_seq(session_id);
        if layout_removed {
            self.publish_remote_terminal_layout_changed();
        }
        self.reply_with_session(
            envelope,
            Some(session_id),
            REMOTE_TERMINAL_CLOSED,
            json!({ "id": session_id }),
        );
        if !terminal_exit_will_broadcast {
            self.broadcast_terminal_list(None);
        }
    }

    pub(super) fn write_terminal_upload_file(
        &self,
        session_id: &str,
        name: &str,
        bytes: &[u8],
    ) -> Result<PathBuf, String> {
        let directory = remote_terminal_upload_directory(session_id);
        fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
        let path = unique_remote_upload_path(&directory, name);
        fs::write(&path, bytes).map_err(|error| error.to_string())?;
        Ok(path)
    }

    pub(super) fn finish_terminal_upload(
        &self,
        device_id: Option<&str>,
        session_id: &str,
        path: PathBuf,
        kind: &str,
    ) {
        let text = format!("{} ", terminal_upload_path_input(&path));
        let _ = self.terminals.write(session_id, text.as_bytes());
        self.send_terminal_data(
            REMOTE_TERMINAL_UPLOADED,
            device_id,
            Some(session_id),
            json!({
                "path": path.to_string_lossy().to_string(),
                "name": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
                "kind": kind,
                "mode": "path",
                "tool": Value::Null,
                "inserted": true,
            }),
        );
    }
}

pub(super) struct DesktopTerminalCtx<'a> {
    pub(super) host: Arc<RemoteHostRuntime>,
    pub(super) envelope: &'a RemoteEnvelope,
}

impl RemoteTerminalDispatch for DesktopTerminalCtx<'_> {
    fn terminal_manager(&self) -> &TerminalManager {
        self.host.terminals.as_ref()
    }

    fn reply_terminal(
        &self,
        device_id: Option<&str>,
        session_id: Option<&str>,
        request_id: Option<&str>,
        kind: &str,
        payload: Value,
    ) {
        self.host
            .send_transport_with_request_id(kind, device_id, session_id, request_id, payload);
    }

    fn handle_terminal_list_msg(&self, _msg: &TerminalMessage) {
        self.host.reply_terminal_list(self.envelope);
    }

    fn handle_terminal_subscribe_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_subscribe(self.envelope);
    }

    fn handle_terminal_unsubscribe_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_unsubscribe(self.envelope);
    }

    fn handle_terminal_create_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_create(self.envelope);
    }

    fn handle_terminal_buffer_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_buffer(self.envelope);
    }

    fn handle_terminal_output_ack_msg(&self, msg: &TerminalMessage) {
        let Some(session_id) = msg.session_id else {
            return;
        };
        let output_seq = msg.payload.get("outputSeq").and_then(Value::as_i64);
        self.host
            .record_terminal_output_ack(session_id, msg.device_id, output_seq);
        let owner = self.viewport_owner_for(msg.device_id);
        self.terminal_manager()
            .touch_viewport_lease(session_id, &owner);
    }

    fn handle_terminal_input_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_input(self.envelope);
    }

    fn handle_terminal_resize_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_resize(self.envelope);
    }

    fn handle_terminal_close_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_close(self.envelope);
    }

    fn handle_terminal_viewport_claim_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_viewport_claim(self.envelope);
    }

    fn handle_terminal_viewport_resize_msg(&self, _msg: &TerminalMessage) {
        self.host.handle_terminal_viewport_resize(self.envelope);
    }
}

pub(crate) fn remote_terminal_upload_directory(session_id: &str) -> PathBuf {
    runtime_upload::terminal_upload_directory(session_id)
}

pub(crate) fn sanitized_remote_upload_name(value: &str) -> String {
    runtime_upload::sanitized_upload_name(value)
}

pub(crate) fn terminal_upload_path_input(path: &Path) -> String {
    runtime_upload::terminal_upload_path_input(path)
}

pub(crate) fn unique_remote_upload_path(directory: &Path, file_name: &str) -> PathBuf {
    runtime_upload::unique_upload_path(directory, file_name)
}
