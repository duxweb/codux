impl MemoryService {
    fn merge_candidate_into_entry(
        &self,
        conn: &Connection,
        entry_id: &str,
        candidate: MemoryCandidate,
    ) -> Result<StoredMemoryEntry, String> {
        let existing = conn
            .query_row(
                &format!(
                    r#"
                    SELECT {}
                    FROM memory_entries
                    WHERE id = ?1 AND status = 'active' AND superseded_by IS NULL
                    LIMIT 1;
                    "#,
                    stored_entry_select_columns()
                ),
                params![entry_id],
                stored_memory_entry_from_row,
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let Some(mut entry) = existing else {
            return self.upsert(conn, candidate);
        };
        let content = merge_memory_content(&entry.content, &candidate.content);
        let rationale =
            merge_optional_memory_text(entry.rationale.as_deref(), candidate.rationale.as_deref());
        let normalized_hash = sha256_hex(&normalized_memory_content(&content));
        let tier = preferred_tier(&entry.tier, &candidate.tier);
        let now = now_seconds();
        conn.execute(
            r#"
            UPDATE memory_entries
            SET tier = ?1, kind = ?2, content = ?3, rationale = ?4, source_tool = ?5,
                source_session_id = ?6, source_fingerprint = ?7, normalized_hash = ?8,
                status = 'active', merged_summary_id = NULL, merged_at = NULL, archived_at = NULL,
                updated_at = ?9
            WHERE id = ?10;
            "#,
            params![
                tier.as_str(),
                candidate.kind.as_str(),
                content,
                rationale,
                candidate.source_tool,
                candidate.source_session_id,
                candidate.source_fingerprint,
                normalized_hash,
                now,
                entry.id
            ],
        )
        .map_err(|error| error.to_string())?;
        entry.tier = tier;
        entry.kind = candidate.kind;
        entry.content = content;
        entry.rationale = rationale;
        entry.source_tool = candidate.source_tool;
        entry.source_session_id = candidate.source_session_id;
        entry.source_fingerprint = candidate.source_fingerprint;
        entry.normalized_hash = normalized_hash;
        entry.status = MemoryEntryStatus::Active;
        entry.merged_summary_id = None;
        entry.merged_at = None;
        entry.archived_at = None;
        entry.updated_at = now;
        Ok(entry)
    }

    fn supersede_entry(
        &self,
        conn: &Connection,
        old_entry_id: &str,
        new_entry_id: &str,
    ) -> Result<(), String> {
        if old_entry_id == new_entry_id {
            return Ok(());
        }
        let now = now_seconds();
        conn.execute(
            r#"
            UPDATE memory_entries
            SET superseded_by = ?1, status = 'archived', archived_at = ?2, updated_at = ?2
            WHERE id = ?3 AND status = 'active';
            "#,
            params![new_entry_id, now, old_entry_id],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn upsert(
        &self,
        conn: &Connection,
        candidate: MemoryCandidate,
    ) -> Result<StoredMemoryEntry, String> {
        let normalized_content = normalized_memory_content(&candidate.content);
        let normalized_hash = sha256_hex(&normalized_content);
        let existing = conn
            .query_row(
                &format!(
                    r#"
                    SELECT {}
                    FROM memory_entries
                    WHERE scope = ?1
                      AND COALESCE(project_id, '') = COALESCE(?2, '')
                      AND COALESCE(tool_id, '') = COALESCE(?3, '')
                      AND COALESCE(module_key, '') = COALESCE(?4, '')
                      AND normalized_hash = ?5
                    LIMIT 1;
                    "#,
                    stored_entry_select_columns()
                ),
                params![
                    candidate.scope.as_str(),
                    candidate.project_id.as_deref(),
                    candidate.tool_id.as_deref(),
                    candidate.module_key.as_deref(),
                    normalized_hash
                ],
                stored_memory_entry_from_row,
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let now = now_seconds();
        if let Some(mut entry) = existing {
            let tier = preferred_tier(&entry.tier, &candidate.tier);
            conn.execute(
                r#"
                UPDATE memory_entries
                SET tier = ?1, kind = ?2, content = ?3, rationale = ?4, source_tool = ?5,
                    source_session_id = ?6, source_fingerprint = ?7, module_key = ?8, status = 'active',
                    merged_summary_id = NULL, merged_at = NULL, archived_at = NULL, updated_at = ?9
                WHERE id = ?10;
                "#,
                params![
                    tier.as_str(),
                    candidate.kind.as_str(),
                    candidate.content,
                    candidate.rationale,
                    candidate.source_tool,
                    candidate.source_session_id,
                    candidate.source_fingerprint,
                    candidate.module_key,
                    now,
                    entry.id
                ],
            )
            .map_err(|error| error.to_string())?;
            entry.tier = tier;
            entry.kind = candidate.kind;
            entry.content = candidate.content;
            entry.module_key = candidate.module_key;
            entry.status = MemoryEntryStatus::Active;
            entry.updated_at = now;
            return Ok(entry);
        }

        let entry = StoredMemoryEntry {
            id: Uuid::new_v4().to_string(),
            scope: candidate.scope,
            project_id: candidate.project_id,
            tool_id: candidate.tool_id,
            module_key: candidate.module_key,
            tier: candidate.tier,
            kind: candidate.kind,
            content: candidate.content,
            rationale: candidate.rationale,
            source_tool: candidate.source_tool,
            source_session_id: candidate.source_session_id,
            source_fingerprint: candidate.source_fingerprint,
            normalized_hash,
            superseded_by: None,
            status: MemoryEntryStatus::Active,
            merged_summary_id: None,
            merged_at: None,
            archived_at: None,
            access_count: 0,
            last_accessed_at: None,
            created_at: now,
            updated_at: now,
            last_decision: None,
        };
        conn.execute(
            r#"
            INSERT INTO memory_entries (
                id, scope, project_id, tool_id, module_key, tier, kind, content, rationale, source_tool, source_session_id,
                source_fingerprint, normalized_hash, superseded_by, status, merged_summary_id, merged_at, archived_at,
                access_count, last_accessed_at, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16, ?17, ?18, ?19, ?20, ?21, ?22);
            "#,
            params![
                entry.id,
                entry.scope.as_str(),
                entry.project_id,
                entry.tool_id,
                entry.module_key,
                entry.tier.as_str(),
                entry.kind.as_str(),
                entry.content,
                entry.rationale,
                entry.source_tool,
                entry.source_session_id,
                entry.source_fingerprint,
                entry.normalized_hash,
                entry.superseded_by,
                entry.status.as_str(),
                entry.merged_summary_id,
                entry.merged_at,
                entry.archived_at,
                entry.access_count,
                entry.last_accessed_at,
                entry.created_at,
                entry.updated_at,
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(entry)
    }
}
