use super::*;

impl RemoteHostRuntime {
    /// Serve the host's saved SSH profiles (lean, no secrets). The host owns the
    /// profiles, so it just sends its own list as the shared DTO.
    pub(super) fn handle_ssh_list(&self, envelope: &RemoteEnvelope) {
        self.reply_ssh_list(envelope);
    }

    /// Reply with the saved SSH profiles as secret-free summaries.
    fn reply_ssh_list(&self, envelope: &RemoteEnvelope) {
        self.reply(envelope, REMOTE_SSH_LIST_RESULT, self.ssh_list_payload());
    }

    fn ssh_list_payload(&self) -> Value {
        let service =
            crate::ssh::SSHService::new(self.support_dir.clone(), std::path::PathBuf::new());
        let profiles: Vec<codux_protocol::RemoteSshProfileSummary> = service
            .summary()
            .profiles
            .into_iter()
            .map(|profile| codux_protocol::RemoteSshProfileSummary {
                id: profile.id,
                name: profile.name,
                endpoint: profile.endpoint,
                credential: profile.credential_kind,
            })
            .collect();
        json!({ "profiles": profiles })
    }

    /// Add or update a saved SSH profile, then reply with the refreshed list.
    pub(super) fn handle_ssh_upsert(&self, envelope: &RemoteEnvelope) {
        let request: crate::ssh::SSHProfileUpsertRequest =
            match serde_json::from_value(envelope.payload.clone()) {
                Ok(request) => request,
                Err(error) => {
                    self.send_error(envelope, &format!("Invalid SSH profile: {error}"));
                    return;
                }
            };
        let store = crate::ssh::SSHStore::from_support_dir(self.support_dir.clone());
        match store.upsert(request) {
            Ok(_) => self.reply_ssh_list(envelope),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    /// Remove a saved SSH profile by id, then reply with the refreshed list.
    pub(super) fn handle_ssh_remove(&self, envelope: &RemoteEnvelope) {
        let id = envelope
            .payload
            .get("id")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        if id.trim().is_empty() {
            self.send_error(envelope, "SSH profile id is required.");
            return;
        }
        let store = crate::ssh::SSHStore::from_support_dir(self.support_dir.clone());
        match store.delete(id) {
            Ok(_) => self.reply_ssh_list(envelope),
            Err(error) => self.send_error(envelope, &error),
        }
    }
}
