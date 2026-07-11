use super::*;

impl RemoteHostRuntime {
    pub(super) fn send_project_and_terminal_snapshots(&self, device_id: Option<&str>) {
        let Some(device_id) = device_id.filter(|value| !value.trim().is_empty()) else {
            return;
        };
        self.send_project_list(Some(device_id));
        self.send_terminal_list(Some(device_id));
    }

    pub(super) fn broadcast_project_and_terminal_lists(&self, source_device_id: Option<&str>) {
        self.broadcast_project_list(source_device_id);
        self.broadcast_terminal_list(source_device_id);
    }

    pub(super) fn send_project_list(&self, device_id: Option<&str>) {
        let version =
            self.resource_subscriptions
                .current_version(REMOTE_RESOURCE_PROJECTS, None, None);
        let payload = with_resource_version(self.remote_project_list_payload(device_id), version);
        self.send(REMOTE_PROJECT_LIST, device_id, None, payload);
    }

    pub(super) fn reply_project_list(&self, envelope: &RemoteEnvelope) {
        let version =
            self.resource_subscriptions
                .current_version(REMOTE_RESOURCE_PROJECTS, None, None);
        let payload = with_resource_version(
            self.remote_project_list_payload(envelope.device_id.as_deref()),
            version,
        );
        self.reply(envelope, REMOTE_PROJECT_LIST, payload);
    }

    pub(super) fn send_terminal_list(&self, device_id: Option<&str>) {
        let version =
            self.resource_subscriptions
                .current_version(REMOTE_RESOURCE_TERMINALS, None, None);
        let payload =
            with_resource_version(json!({ "terminals": self.remote_terminals() }), version);
        self.send(REMOTE_TERMINAL_LIST, device_id, None, payload);
    }

    pub(super) fn reply_terminal_list(&self, envelope: &RemoteEnvelope) {
        let version =
            self.resource_subscriptions
                .current_version(REMOTE_RESOURCE_TERMINALS, None, None);
        let payload =
            with_resource_version(json!({ "terminals": self.remote_terminals() }), version);
        self.reply(envelope, REMOTE_TERMINAL_LIST, payload);
    }

    pub(super) fn broadcast_project_list(&self, source_device_id: Option<&str>) {
        // Bump the version once so every recipient observes the same change.
        let version =
            self.resource_subscriptions
                .next_version(REMOTE_RESOURCE_PROJECTS, None, None);
        let mut device_ids =
            self.resource_subscriptions
                .devices_for(REMOTE_RESOURCE_PROJECTS, None, None);
        if let Some(source_device_id) = source_device_id.filter(|value| !value.trim().is_empty()) {
            device_ids.insert(source_device_id.to_string());
        }
        if device_ids.is_empty() {
            return;
        }
        for device_id in device_ids {
            let payload =
                with_resource_version(self.remote_project_list_payload(Some(&device_id)), version);
            self.send(REMOTE_PROJECT_LIST, Some(&device_id), None, payload);
        }
    }

    pub(super) fn broadcast_terminal_list(&self, source_device_id: Option<&str>) {
        let version =
            self.resource_subscriptions
                .next_version(REMOTE_RESOURCE_TERMINALS, None, None);
        let mut device_ids = self
            .resource_subscriptions
            .devices_for_resource(REMOTE_RESOURCE_TERMINALS);
        if let Some(source_device_id) = source_device_id.filter(|value| !value.trim().is_empty()) {
            device_ids.insert(source_device_id.to_string());
        }
        if device_ids.is_empty() {
            if source_device_id.is_some() {
                self.send_terminal_list(source_device_id);
            }
            return;
        }
        let payload =
            with_resource_version(json!({ "terminals": self.remote_terminals() }), version);
        for device_id in device_ids {
            self.send(
                REMOTE_TERMINAL_LIST,
                Some(&device_id),
                None,
                payload.clone(),
            );
        }
    }

    pub(super) fn reply_worktree_summary(
        &self,
        envelope: &RemoteEnvelope,
        kind: &str,
        project_id: &str,
        project_path: &str,
    ) {
        let summary = WorktreeService::new(self.support_dir.clone())
            .summary(Some(project_id), Some(project_path));
        self.reply_resource_payload(
            envelope,
            kind,
            REMOTE_RESOURCE_WORKTREES,
            Some(project_id),
            None,
            remote_worktree_summary_payload(project_id, summary),
        );
    }

    /// Push the current worktree list to subscribed devices after a
    /// desktop-initiated worktree mutation (create/remove/merge), so mobile
    /// reconciles its view instead of showing a stale list. A no-op when no
    /// device is subscribed. Mirrors the `worktree.list` request reply.
    pub fn broadcast_worktree_list_change(&self, project_id: &str, project_path: &str) {
        if project_id.trim().is_empty() {
            return;
        }
        let summary = WorktreeService::new(self.support_dir.clone())
            .summary(Some(project_id), Some(project_path));
        self.broadcast_worktree_update(
            REMOTE_WORKTREE_LIST,
            None,
            project_id,
            remote_worktree_summary_payload(project_id, summary),
        );
    }

    pub(super) fn broadcast_worktree_update(
        &self,
        kind: &str,
        source_device_id: Option<&str>,
        project_id: &str,
        payload: Value,
    ) {
        self.broadcast_resource_payload(
            kind,
            REMOTE_RESOURCE_WORKTREES,
            source_device_id,
            Some(project_id),
            None,
            payload,
        );
    }

    pub(super) fn broadcast_resource_payload(
        &self,
        kind: &str,
        resource: &str,
        source_device_id: Option<&str>,
        project_id: Option<&str>,
        session_id: Option<&str>,
        mut payload: Value,
    ) {
        // Stamp the monotonic version for this resource key ONCE, before fan-out,
        // so every subscriber (and any later snapshot reply) orders this change
        // consistently and a client that missed a push reconciles by version
        // instead of sticking on stale state. (Unified state-sync, design step 2.)
        let version = self
            .resource_subscriptions
            .next_version(resource, project_id, session_id);
        payload = with_resource_version(payload, version);
        let mut device_ids = self
            .resource_subscriptions
            .devices_for(resource, project_id, session_id);
        if let Some(source_device_id) = source_device_id.filter(|value| !value.trim().is_empty()) {
            device_ids.insert(source_device_id.to_string());
        }
        if device_ids.is_empty() {
            return;
        }
        for device_id in device_ids {
            self.send(kind, Some(&device_id), session_id, payload.clone());
        }
    }

    pub(super) fn reply_resource_payload(
        &self,
        envelope: &RemoteEnvelope,
        kind: &str,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        let version = self
            .resource_subscriptions
            .current_version(resource, project_id, session_id);
        self.reply(envelope, kind, with_resource_version(payload, version));
    }

    pub(super) fn reply_and_broadcast_resource_change(
        &self,
        envelope: &RemoteEnvelope,
        kind: &str,
        resource: &str,
        project_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        let version = self
            .resource_subscriptions
            .next_version(resource, project_id, session_id);
        let payload = with_resource_version(payload, version);
        self.reply(envelope, kind, payload.clone());
        let source_device_id = envelope.device_id.as_deref();
        for device_id in self
            .resource_subscriptions
            .devices_for(resource, project_id, session_id)
        {
            if Some(device_id.as_str()) != source_device_id {
                self.send(kind, Some(&device_id), session_id, payload.clone());
            }
        }
    }

    pub(super) fn send(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        self.send_transport(kind, device_id, session_id, payload);
    }

    pub(super) fn send_plain(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) -> bool {
        let envelope = super::types::RemoteOutgoingEnvelope {
            kind: kind.to_string(),
            device_id: device_id.map(str::to_string),
            session_id: session_id.map(str::to_string),
            request_id: None,
            seq: None,
            payload,
        };
        let Ok(data) = serde_json::to_vec(&envelope) else {
            return false;
        };
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        let Some(transport) = transport else {
            return false;
        };
        transport.send(data, device_id)
    }

    pub(super) fn send_terminal_data(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: Value,
    ) {
        self.send_transport(kind, device_id, session_id, payload);
    }

    /// Fan-out helper for terminal output: sends a frame whose payload was
    /// already serialized once (see [`RemoteService::outgoing_transport_text_raw`]),
    /// so broadcasting one batch to N subscribers does not clone + re-serialize
    /// the payload per device. Only the small per-device `seq` wrapper differs.
    pub(super) fn send_terminal_output_raw(
        &self,
        device_id: Option<&str>,
        session_id: Option<&str>,
        payload: &serde_json::value::RawValue,
    ) -> bool {
        let text = {
            let Ok(mut send_seq) = self.send_seq_by_device.lock() else {
                return false;
            };
            self.service().outgoing_transport_text_raw(
                REMOTE_TERMINAL_OUTPUT,
                device_id,
                session_id,
                payload,
                &mut send_seq,
            )
        };
        let Some(text) = text else {
            return false;
        };
        let transport = self.transport.lock().ok().and_then(|value| value.clone());
        let Some(transport) = transport else {
            return false;
        };
        transport.send_terminal(text.into_bytes(), device_id)
    }

    pub(super) fn send_error(&self, envelope: &RemoteEnvelope, message: &str) {
        self.reply(envelope, REMOTE_ERROR, json!({ "message": message }));
    }

    pub(super) fn reply(&self, envelope: &RemoteEnvelope, kind: &str, payload: Value) {
        self.send_transport_with_request_id(
            kind,
            envelope.device_id.as_deref(),
            envelope.session_id.as_deref(),
            envelope.request_id.as_deref(),
            payload,
        );
    }

    pub(super) fn reply_with_session(
        &self,
        envelope: &RemoteEnvelope,
        session_id: Option<&str>,
        kind: &str,
        payload: Value,
    ) {
        self.send_transport_with_request_id(
            kind,
            envelope.device_id.as_deref(),
            session_id,
            envelope.request_id.as_deref(),
            payload,
        );
    }

    pub(super) fn outgoing_transport_text(
        &self,
        kind: &str,
        device_id: Option<&str>,
        session_id: Option<&str>,
        request_id: Option<&str>,
        payload: Value,
    ) -> Option<String> {
        let Ok(mut send_seq) = self.send_seq_by_device.lock() else {
            return None;
        };
        self.service().outgoing_transport_text(
            kind,
            device_id,
            session_id,
            request_id,
            payload,
            &mut send_seq,
        )
    }

    pub(super) fn update_device_online(&self, device_id: Option<&str>, online: bool) {
        let Some(device_id) = device_id else {
            return;
        };
        let mut status = self.snapshot();
        if !status
            .device_list
            .iter()
            .any(|device| device.id == device_id)
        {
            status = self.summary_from_settings_preserving_connection();
        }
        if let Some(device) = status
            .device_list
            .iter_mut()
            .find(|device| device.id == device_id)
        {
            device.online = Some(online);
            if online {
                device.last_seen = chrono::Utc::now().to_rfc3339();
            }
        }
        status.online_devices = status
            .device_list
            .iter()
            .filter(|device| device.online.unwrap_or(false))
            .count();
        self.update_snapshot(status);
    }

    pub(super) fn is_authorized_device_token(
        &self,
        device_id: Option<&str>,
        device_token: Option<&str>,
    ) -> bool {
        let Some(device_id) = device_id.map(str::trim).filter(|value| !value.is_empty()) else {
            return false;
        };
        let Some(device_token) = device_token
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return false;
        };
        self.authorization.is_authorized(device_id, device_token)
    }

    pub(super) fn update_snapshot(&self, summary: RemoteSummary) {
        if let Ok(mut current) = self.snapshot.lock() {
            *current = summary;
            self.push_event(RemoteHostEvent::Summary(Box::new(current.clone())));
        }
    }

    pub(super) fn push_event(&self, event: RemoteHostEvent) {
        if let Ok(mut events) = self.events.lock() {
            events.push_back(event);
            while events.len() > 128 {
                events.pop_front();
            }
        }
    }

    pub(super) fn summary_from_settings_preserving_connection(&self) -> RemoteSummary {
        let mut summary = self.service().summary();
        let current = self.snapshot();
        if summary.enabled && current.enabled && summary.relay == current.relay {
            summary.status = current.status;
            summary.message = current.message;
            summary.pairing = current.pairing;
            summary.pending_pairing_list = current.pending_pairing_list;
            summary.pending_pairings = summary.pending_pairing_list.len();
        }
        summary
    }

    pub(super) fn service(&self) -> RemoteService {
        RemoteService::new(self.support_dir.clone())
    }
}
