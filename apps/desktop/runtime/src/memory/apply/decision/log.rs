impl MemoryService {
    pub(super) fn record_memory_decision(&self, decision: MemoryDecisionLog) -> Result<(), String> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO memory_decision_logs (
                id, decision, entry_id, target_entry_id, reason, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6);
            "#,
            params![
                Uuid::new_v4().to_string(),
                decision.kind.as_str(),
                decision.entry_id,
                decision.target_entry_id,
                decision.reason,
                decision.created_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }
}
