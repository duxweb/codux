impl RuntimeService {
    pub fn prepare_ai_runtime_bridge(&self) -> Result<AIRuntimeBridgeSnapshot, String> {
        self.ai_runtime.prepare()?;
        Ok(self.ai_runtime.snapshot())
    }

    pub fn start_ai_runtime_event_processing(&self) -> Result<AIRuntimeBridgeSnapshot, String> {
        self.ai_runtime.start_event_processing_background()?;
        Ok(self.ai_runtime.snapshot())
    }

    pub fn ai_runtime_bridge_snapshot(&self) -> AIRuntimeBridgeSnapshot {
        self.ai_runtime.snapshot()
    }

    pub fn ai_runtime_state_snapshot(&self) -> AIRuntimeStateSnapshot {
        self.ai_runtime.runtime_state_snapshot()
    }

    pub fn summarize_ai_runtime_state_snapshot(
        &self,
        snapshot: &AIRuntimeStateSnapshot,
    ) -> AIRuntimeStateSummary {
        AIRuntimeStateService::new(&self.support_dir).summary_from_runtime_snapshot(snapshot)
    }
    pub fn poll_ai_runtime_state(&self) -> Result<AIRuntimeStateSnapshot, String> {
        self.ai_runtime.poll_runtime_state()?;
        Ok(self.ai_runtime.runtime_state_snapshot())
    }

    pub fn ai_runtime_dismiss_completion(&self, project_id: &str) -> bool {
        self.ai_runtime.dismiss_completion(project_id)
    }

    pub fn dismiss_ai_runtime_completion(&self, project_id: &str) -> AIRuntimeStateSnapshot {
        let _ = self.ai_runtime_dismiss_completion(project_id);
        self.ai_runtime.runtime_state_snapshot()
    }

    pub fn drain_ai_runtime_events(&self) -> Vec<AIRuntimeSupervisorEvent> {
        self.ai_runtime.drain_supervisor_events()
    }

    pub fn ai_runtime_terminal_statuses(&self) -> Vec<crate::ai_runtime::TerminalStatusEvent> {
        self.ai_runtime.terminal_statuses_snapshot()
    }

    pub fn drain_ai_runtime_events_and_enqueue_memory(&self) -> AIRuntimeDrainResult {
        let events = self.ai_runtime.drain_supervisor_events();
        // Mirror terminal status to connected controllers so a viewer of this
        // host renders the same loading/waiting/completed dots.
        for event in &events {
            if let AIRuntimeSupervisorEvent::TerminalStatus { status } = event
                && let Ok(payload) = serde_json::to_value(status) {
                    self.remote_host.broadcast_terminal_status(payload);
                }
        }
        let memory = events
            .iter()
            .filter_map(|event| match event {
                AIRuntimeSupervisorEvent::Completion { completion } => completion.session.as_ref(),
                _ => None,
            })
            .filter_map(|session| self.enqueue_completed_session_memory(session).ok())
            .collect::<Vec<_>>();
        AIRuntimeDrainResult { events, memory }
    }
}
