use super::*;
use crate::remote::transport::RemoteTransport;
use crate::remote::types::RemoteOutgoingEnvelope;
use crate::terminal_layout::TerminalPaneSummary;
use async_trait::async_trait;
use codux_remote_transport::RemoteTransportKind;

pub(super) fn temp_support_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("{name}-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create temp support dir");
    dir
}

pub(super) fn buffer_options(
    max_chars: usize,
    request_id: Option<&str>,
    tail: bool,
) -> TerminalBaselineOptions {
    TerminalBaselineOptions {
        max_chars,
        chunk_chars: None,
        request_id: request_id.map(ToOwned::to_owned),
        tail,
        viewport: None,
    }
}

pub(super) fn viewport_buffer_options(
    max_chars: usize,
    request_id: Option<&str>,
    tail: bool,
    cols: u16,
    rows: u16,
) -> TerminalBaselineOptions {
    TerminalBaselineOptions {
        max_chars,
        chunk_chars: None,
        request_id: request_id.map(ToOwned::to_owned),
        tail,
        viewport: Some(BaselineViewport { cols, rows }),
    }
}

#[derive(Default)]
struct CapturingTransport {
    messages: Mutex<Vec<(Option<String>, Vec<u8>)>>,
}

impl CapturingTransport {
    pub(super) fn take_messages(&self) -> Vec<(Option<String>, Vec<u8>)> {
        self.messages
            .lock()
            .map(|mut messages| messages.drain(..).collect())
            .unwrap_or_default()
    }

    pub(super) fn wait_for_message<F>(&self, mut predicate: F) -> Option<(Option<String>, Vec<u8>)>
    where
        F: FnMut(&(Option<String>, Vec<u8>)) -> bool,
    {
        // Generous cap: slow replies (host-metrics sampling under full-suite
        // load) can exceed a few seconds; passing tests return early.
        for _ in 0..600 {
            for message in self.take_messages() {
                if predicate(&message) {
                    return Some(message);
                }
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        None
    }
}

#[async_trait]
impl RemoteTransport for CapturingTransport {
    fn kind(&self) -> RemoteTransportKind {
        RemoteTransportKind::Iroh
    }

    fn send(&self, data: Vec<u8>, device_id: Option<&str>) -> bool {
        if let Ok(mut messages) = self.messages.lock() {
            messages.push((device_id.map(str::to_string), data));
        }
        true
    }

    async fn shutdown(&self) {}
}

pub(super) fn write_paired_remote_settings(support_dir: &Path) {
    fs::write(
        support_dir.join("settings.json"),
        serde_json::to_string_pretty(&json!({
            "remote": {
                "isEnabled": true,
                "relayUrl": "http://relay.example",
                "hostID": "host-1",
                "cachedDevices": [
                    {
                        "id": "device-1",
                        "token": "device-token-1",
                        "hostId": "host-1",
                        "name": "Phone"
                    }
                ]
            }
        }))
        .expect("serialize settings"),
    )
    .expect("write settings");
}

pub(super) fn write_two_project_state(support_dir: &Path) -> (PathBuf, PathBuf) {
    let project_a = support_dir.join("project-a");
    let project_b = support_dir.join("project-b");
    fs::create_dir_all(&project_a).expect("create project a");
    fs::create_dir_all(&project_b).expect("create project b");
    fs::write(
        support_dir.join("state.json"),
        serde_json::to_string_pretty(&json!({
            "projects": [
                {"id": "project-a", "name": "Project A", "path": project_a.to_string_lossy()},
                {"id": "project-b", "name": "Project B", "path": project_b.to_string_lossy()}
            ],
            "worktrees": [
                {
                    "id": "worktree-b",
                    "projectId": "project-b",
                    "name": "Task B",
                    "branch": "task-b",
                    "path": project_b.to_string_lossy(),
                    "status": "active",
                    "isDefault": true,
                    "createdAt": 1,
                    "updatedAt": 1
                }
            ],
            "selectedProjectId": "project-a",
            "selectedWorktreeIdByProject": {
                "project-b": "worktree-b"
            }
        }))
        .expect("serialize state"),
    )
    .expect("write state");
    (project_a, project_b)
}

mod ai_stats;
mod memory;
mod projects;
mod resources;
mod terminal_buffer;
mod terminal_lifecycle;
mod terminal_viewport;
mod transport;
