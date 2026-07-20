use codux_runtime_core::agent_worktree::AgentWorktreeTerminalScope;
use std::collections::HashMap;

#[derive(Default)]
pub(super) struct CapabilityRegistry {
    by_capability: HashMap<String, CapabilityGrant>,
    by_terminal: HashMap<String, String>,
}

#[derive(Clone)]
pub(super) struct CapabilityGrant {
    pub capability: String,
    pub scope: AgentWorktreeTerminalScope,
    terminal_id: String,
}

impl CapabilityRegistry {
    pub fn grant(
        &mut self,
        terminal_id: String,
        scope: AgentWorktreeTerminalScope,
    ) -> CapabilityGrant {
        self.revoke_terminal(&terminal_id);
        let capability = uuid::Uuid::new_v4().to_string();
        let grant = CapabilityGrant {
            capability: capability.clone(),
            scope,
            terminal_id: terminal_id.clone(),
        };
        self.by_terminal.insert(terminal_id, capability.clone());
        self.by_capability.insert(capability, grant.clone());
        grant
    }

    pub fn resolve(&self, capability: &str) -> Option<CapabilityGrant> {
        self.by_capability.get(capability.trim()).cloned()
    }

    pub fn revoke_terminal(&mut self, terminal_id: &str) {
        if let Some(capability) = self.by_terminal.remove(terminal_id) {
            self.by_capability.remove(&capability);
        }
    }

    pub fn revoke_capability(&mut self, capability: &str) {
        let Some(grant) = self.by_capability.remove(capability) else {
            return;
        };
        if self
            .by_terminal
            .get(&grant.terminal_id)
            .is_some_and(|current| current == capability)
        {
            self.by_terminal.remove(&grant.terminal_id);
        }
    }

    #[cfg(test)]
    pub fn contains_terminal(&self, terminal_id: &str) -> bool {
        self.by_terminal.contains_key(terminal_id)
    }

    #[cfg(test)]
    pub fn contains_capability(&self, capability: &str) -> bool {
        self.by_capability.contains_key(capability)
    }

    #[cfg(test)]
    pub fn terminal_capability(&self, terminal_id: &str) -> Option<String> {
        self.by_terminal.get(terminal_id).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use codux_runtime_core::runtime_target::RuntimeTarget;

    fn scope() -> AgentWorktreeTerminalScope {
        AgentWorktreeTerminalScope {
            root_project_id: "project-1".to_string(),
            root_project_path: "/repo".to_string(),
            project_name: "Repo".to_string(),
            source_worktree_id: "worktree-1".to_string(),
            source_worktree_path: "/repo".to_string(),
            runtime_target: RuntimeTarget::Local,
        }
    }

    #[test]
    fn replacing_and_revoking_terminal_invalidates_old_capabilities() {
        let mut registry = CapabilityRegistry::default();
        let first = registry.grant("terminal-1".to_string(), scope());
        let second = registry.grant("terminal-1".to_string(), scope());
        assert!(registry.resolve(&first.capability).is_none());
        assert!(registry.resolve(&second.capability).is_some());

        registry.revoke_terminal("terminal-1");
        assert!(registry.resolve(&second.capability).is_none());
    }

    #[test]
    fn revoking_old_capability_does_not_revoke_replacement() {
        let mut registry = CapabilityRegistry::default();
        let first = registry.grant("terminal-1".to_string(), scope());
        let second = registry.grant("terminal-1".to_string(), scope());

        registry.revoke_capability(&first.capability);

        assert!(registry.resolve(&second.capability).is_some());
        assert!(registry.contains_terminal("terminal-1"));
    }
}
