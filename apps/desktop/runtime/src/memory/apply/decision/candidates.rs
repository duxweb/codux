impl MemoryService {
    fn write_decision_candidates(
        &self,
        candidate: &MemoryCandidate,
    ) -> Result<Vec<StoredMemoryEntry>, String> {
        let mut entries = self.list_entries(
            candidate.scope.clone(),
            candidate.project_id.as_deref(),
            candidate.tool_id.as_deref(),
            &[MemoryTier::Core, MemoryTier::Working],
            MEMORY_WRITE_CANDIDATE_LIMIT,
        )?;
        entries.retain(|entry| {
            entry.module_key.as_deref() == candidate.module_key.as_deref()
                && entry.kind == candidate.kind
        });
        Ok(entries)
    }

    fn list_entries(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
        tiers: &[MemoryTier],
        limit: i64,
    ) -> Result<Vec<StoredMemoryEntry>, String> {
        if tiers.is_empty() || limit <= 0 {
            return Ok(Vec::new());
        }
        let tier_values = tiers.iter().map(MemoryTier::as_str).collect::<Vec<_>>();
        let placeholders = tier_values
            .iter()
            .map(|_| "?")
            .collect::<Vec<_>>()
            .join(",");
        let sql = format!(
            r#"
            SELECT {}
            FROM memory_entries
            WHERE scope = ?
              AND COALESCE(project_id, '') = COALESCE(?, '')
              AND (tool_id IS NULL OR tool_id = ?)
              AND tier IN ({})
              AND superseded_by IS NULL
              AND status = 'active'
            ORDER BY access_count DESC, updated_at DESC
            LIMIT ?;
            "#,
            stored_entry_select_columns(),
            placeholders
        );
        let conn = self.open_connection()?;
        let mut statement = conn.prepare(&sql).map_err(|error| error.to_string())?;
        let mut values = vec![
            SqlValue::Text(scope.as_str().to_string()),
            optional_text_value(project_id),
            optional_text_value(tool_id),
        ];
        values.extend(
            tier_values
                .iter()
                .map(|value| SqlValue::Text((*value).to_string())),
        );
        values.push(SqlValue::Integer(limit));
        let rows = statement
            .query_map(params_from_iter(values), stored_memory_entry_from_row)
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }
}
