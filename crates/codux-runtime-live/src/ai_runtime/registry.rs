use crate::ai_runtime::screen_signal::screen_text_from_cells;
use codux_terminal_core::HeadlessTerminalScreen;
use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, Weak},
};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AIRuntimeTerminalState {
    pub terminal_id: String,
    pub project_id: String,
    pub slot_id: String,
    pub title: String,
    pub cwd: String,
    pub tool: Option<String>,
    pub is_active: bool,
    pub session_key: Option<String>,
    pub terminal_instance_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct AIRuntimeTerminalBinding {
    pub terminal_id: String,
    pub root_project_id: Option<String>,
    pub worktree_id: Option<String>,
    pub project_id: String,
    pub slot_id: String,
    pub title: String,
    pub cwd: String,
    pub tool: Option<String>,
    pub is_active: bool,
    pub session_key: Option<String>,
    pub terminal_instance_id: Option<String>,
}

#[derive(Default)]
pub struct AIRuntimeRegistry {
    terminals: Mutex<HashMap<String, AIRuntimeTerminalBinding>>,
    // Cycle-safe: a Weak to each terminal's rendered screen, so the supervisor
    // can scrape the screen for the universal "waiting for approval" signal
    // (`screen_signal`) without keeping the session alive.
    screens: Mutex<HashMap<String, Weak<parking_lot::Mutex<HeadlessTerminalScreen>>>>,
    // Each terminal's shell PID for process-tree tool detection (side map keeps the binding struct untouched).
    shell_pids: Mutex<HashMap<String, u32>>,
}

impl AIRuntimeRegistry {
    pub fn shared() -> Arc<Self> {
        Arc::new(Self::default())
    }

    pub fn upsert(&self, binding: AIRuntimeTerminalBinding) {
        if let Ok(mut terminals) = self.terminals.lock() {
            terminals.insert(binding.terminal_id.clone(), binding);
        }
    }

    /// Register a terminal's rendered screen (held weakly) so the runtime can
    /// scrape it for the screen-based `needsInput` signal.
    pub fn register_screen(
        &self,
        terminal_id: &str,
        screen: Weak<parking_lot::Mutex<HeadlessTerminalScreen>>,
    ) {
        if let Ok(mut screens) = self.screens.lock() {
            screens.insert(terminal_id.to_string(), screen);
        }
    }

    /// Scrape the terminal's rendered screen into plain visible text. Empty
    /// when the screen is gone; callers own tool-specific pattern matching.
    pub fn screen_text(&self, terminal_id: &str) -> Option<String> {
        let weak = match self.screens.lock() {
            Ok(screens) => screens.get(terminal_id).cloned(),
            Err(_) => None,
        };
        let screen = weak.and_then(|weak| weak.upgrade())?;
        // Skip the keyframe string (cells-only); wait for the worker reply
        // outside the lock, mirroring `TerminalPtySession::screen_snapshot`.
        let request = screen.lock().snapshot_request(false);
        let snapshot = request.snapshot();
        Some(screen_text_from_cells(&snapshot))
    }

    /// Record a terminal's shell PID for hook-free tool discovery.
    pub fn register_shell_pid(&self, terminal_id: &str, shell_pid: u32) {
        if let Ok(mut pids) = self.shell_pids.lock() {
            pids.insert(terminal_id.to_string(), shell_pid);
        }
    }

    /// `(terminal_id, shell_pid)` pairs for the process-tree tool detector.
    pub fn shell_pids_snapshot(&self) -> Vec<(String, u32)> {
        self.shell_pids
            .lock()
            .map(|pids| pids.iter().map(|(id, pid)| (id.clone(), *pid)).collect())
            .unwrap_or_default()
    }

    pub fn remove(&self, terminal_id: &str) {
        if let Ok(mut terminals) = self.terminals.lock() {
            terminals.remove(terminal_id);
        }
        if let Ok(mut screens) = self.screens.lock() {
            screens.remove(terminal_id);
        }
        if let Ok(mut pids) = self.shell_pids.lock() {
            pids.remove(terminal_id);
        }
    }

    pub fn snapshot(&self) -> Vec<AIRuntimeTerminalState> {
        let Ok(terminals) = self.terminals.lock() else {
            return Vec::new();
        };
        terminals
            .values()
            .map(|binding| AIRuntimeTerminalState {
                terminal_id: binding.terminal_id.clone(),
                project_id: binding.project_id.clone(),
                slot_id: binding.slot_id.clone(),
                title: binding.title.clone(),
                cwd: binding.cwd.clone(),
                tool: binding.tool.clone(),
                is_active: binding.is_active,
                session_key: binding.session_key.clone(),
                terminal_instance_id: binding.terminal_instance_id.clone(),
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_snapshots_terminal_bindings() {
        let registry = AIRuntimeRegistry::default();
        registry.upsert(AIRuntimeTerminalBinding {
            terminal_id: "term-1".to_string(),
            root_project_id: Some("project-1".to_string()),
            worktree_id: Some("project-1".to_string()),
            project_id: "project-1".to_string(),
            slot_id: "slot-1".to_string(),
            title: "Codex".to_string(),
            cwd: "/tmp/project".to_string(),
            tool: Some("codex".to_string()),
            is_active: true,
            session_key: Some("session-1".to_string()),
            terminal_instance_id: Some("instance-1".to_string()),
        });

        let snapshot = registry.snapshot();

        assert_eq!(snapshot.len(), 1);
        assert_eq!(snapshot[0].terminal_id, "term-1");
        assert_eq!(snapshot[0].tool.as_deref(), Some("codex"));
        registry.remove("term-1");
        assert!(registry.snapshot().is_empty());
    }
}
