use serde::Serialize;
use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
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

    pub fn remove(&self, terminal_id: &str) {
        if let Ok(mut terminals) = self.terminals.lock() {
            terminals.remove(terminal_id);
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
