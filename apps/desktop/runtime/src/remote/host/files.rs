use super::*;

pub(crate) fn remote_file_list(path: Option<&str>, purpose: Option<&str>) -> Value {
    runtime_file::file_list_payload(path, purpose)
}

pub(crate) fn remote_file_read(path: &str) -> Result<Value, String> {
    runtime_file::file_read_payload(path)
}

pub(crate) fn remote_file_write(path: &str, content: &str) -> Result<(), String> {
    runtime_file::file_write(path, content)
}

pub(crate) fn remote_file_rename(path: &str, new_path: &str) -> Result<(), String> {
    runtime_file::file_rename(path, new_path)
}

impl RemoteHostRuntime {
    pub(super) fn handle_file_read_blob(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let path = envelope
            .payload
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let runtime = Arc::clone(self);
        let envelope = envelope.clone();
        crate::async_runtime::spawn(async move {
            let result = match runtime_file::file_read_blob_bytes(&path) {
                Ok(bytes) => {
                    let transport = runtime
                        .transport
                        .lock()
                        .ok()
                        .and_then(|transport| transport.clone());
                    match transport {
                        Some(transport) => match transport.publish_blob(bytes).await {
                            Ok(ticket) => json!({ "ticket": ticket }),
                            Err(error) => json!({ "error": error }),
                        },
                        None => json!({ "error": "transport unavailable" }),
                    }
                }
                Err(error) => json!({ "error": error.to_string() }),
            };
            runtime.reply(&envelope, REMOTE_FILE_BLOB, result);
        });
    }

    pub(super) fn handle_file_write_blob(self: &Arc<Self>, envelope: &RemoteEnvelope) {
        let directory = envelope
            .payload
            .get("directory")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let ticket = envelope
            .payload
            .get("ticket")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string();
        let runtime = Arc::clone(self);
        let envelope = envelope.clone();
        crate::async_runtime::spawn(async move {
            let transport = runtime
                .transport
                .lock()
                .ok()
                .and_then(|transport| transport.clone());
            let result = match transport {
                Some(transport) => transport.fetch_blob(&ticket).await,
                None => Err("transport unavailable".to_string()),
            };
            match result.and_then(|bytes| runtime_file::file_write_bytes(&directory, &name, &bytes))
            {
                Ok(path) => runtime.reply(
                    &envelope,
                    REMOTE_FILE_BYTES_WRITTEN,
                    json!({ "path": path }),
                ),
                Err(error) => runtime.send_error(&envelope, &error),
            }
        });
    }

    pub(super) fn handle_file_read(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        match remote_file_read(path) {
            Ok(payload) => self.reply(envelope, REMOTE_FILE_READ, payload),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_file_write(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        let Some(content) = envelope.payload.get("content").and_then(Value::as_str) else {
            self.send_error(envelope, "File content is required.");
            return;
        };
        match remote_file_write(path, content) {
            Ok(()) => self.reply(envelope, REMOTE_FILE_WRITTEN, json!({ "path": path })),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_file_rename(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        let Some(new_path) = envelope.payload.get("newPath").and_then(Value::as_str) else {
            self.send_error(envelope, "New file path is required.");
            return;
        };
        match remote_file_rename(path, new_path) {
            Ok(()) => self.reply(
                envelope,
                REMOTE_FILE_RENAMED,
                json!({ "path": path, "newPath": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_file_delete(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "File path is required.");
            return;
        };
        match fs::remove_file(path).or_else(|_| fs::remove_dir_all(path)) {
            Ok(()) => self.reply(envelope, REMOTE_FILE_DELETED, json!({ "path": path })),
            Err(error) => self.send_error(envelope, &error.to_string()),
        }
    }

    pub(super) fn handle_file_create_directory(&self, envelope: &RemoteEnvelope) {
        let Some(path) = envelope.payload.get("path").and_then(Value::as_str) else {
            self.send_error(envelope, "Directory path is required.");
            return;
        };
        match runtime_file::file_make_directory(path) {
            Ok(()) => self.reply(
                envelope,
                REMOTE_FILE_DIRECTORY_CREATED,
                json!({ "path": path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_file_copy(&self, envelope: &RemoteEnvelope) {
        let path = envelope
            .payload
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = envelope
            .payload
            .get("targetDir")
            .and_then(Value::as_str)
            .unwrap_or_default();
        match runtime_file::file_copy(path, target) {
            Ok(new_path) => self.reply(envelope, REMOTE_FILE_COPIED, json!({ "path": new_path })),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_file_move(&self, envelope: &RemoteEnvelope) {
        let path = envelope
            .payload
            .get("path")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let target = envelope
            .payload
            .get("targetDir")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let overwrite = envelope
            .payload
            .get("overwrite")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        match runtime_file::file_move(path, target, overwrite) {
            Ok(new_path) => self.reply(envelope, REMOTE_FILE_MOVED, json!({ "path": new_path })),
            Err(error) => self.send_error(envelope, &error),
        }
    }

    pub(super) fn handle_file_write_bytes(&self, envelope: &RemoteEnvelope) {
        use base64::Engine;
        let directory = envelope
            .payload
            .get("directory")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let name = envelope
            .payload
            .get("name")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let bytes = envelope
            .payload
            .get("bytes")
            .and_then(Value::as_str)
            .and_then(|encoded| {
                base64::engine::general_purpose::STANDARD
                    .decode(encoded)
                    .ok()
            })
            .unwrap_or_default();
        match runtime_file::file_write_bytes(directory, name, &bytes) {
            Ok(new_path) => self.reply(
                envelope,
                REMOTE_FILE_BYTES_WRITTEN,
                json!({ "path": new_path }),
            ),
            Err(error) => self.send_error(envelope, &error),
        }
    }
}
