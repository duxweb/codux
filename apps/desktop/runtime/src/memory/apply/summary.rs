use super::{helpers::*, types::StoredMemorySummary};
use crate::memory::extraction::MemoryScope;
use crate::memory::{MemoryService, now_seconds};
use rusqlite::{OptionalExtension, params};
use uuid::Uuid;

impl MemoryService {
    pub(in crate::memory) fn upsert_summary(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        tool_id: Option<&str>,
        content: &str,
        source_entry_ids: &[String],
        max_versions: i32,
    ) -> Result<StoredMemorySummary, String> {
        let content = content.trim();
        if content.is_empty() {
            return Err("summary content cannot be empty".to_string());
        }
        let source_ids = sorted_unique(source_entry_ids);
        let source_json = serde_json::to_string(&source_ids).map_err(|error| error.to_string())?;
        let now = now_seconds();
        let conn = self.open_connection()?;
        let existing = conn
            .query_row(
                r#"
                SELECT id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
                FROM memory_summaries
                WHERE scope = ?1
                  AND COALESCE(project_id, '') = COALESCE(?2, '')
                  AND COALESCE(tool_id, '') = COALESCE(?3, '')
                LIMIT 1;
                "#,
                params![scope.as_str(), project_id, tool_id],
                stored_memory_summary_from_row,
            )
            .optional()
            .map_err(|error| error.to_string())?;
        let token_estimate = estimate_tokens(content);
        if let Some(existing) = existing {
            let version = existing.version + 1;
            conn.execute(
                r#"
                UPDATE memory_summaries
                SET content = ?1, version = ?2, source_entry_ids = ?3, token_estimate = ?4, updated_at = ?5
                WHERE id = ?6;
                "#,
                params![content, version, source_json, token_estimate, now, existing.id],
            )
            .map_err(|error| error.to_string())?;
            self.insert_summary_version(&existing.id, version, content, &source_ids, now)?;
            self.trim_summary_versions(&existing.id, max_versions)?;
            return Ok(StoredMemorySummary {
                id: existing.id,
                scope,
                project_id: project_id.map(str::to_string),
                tool_id: tool_id.map(str::to_string),
                content: content.to_string(),
                version,
                source_entry_ids: source_ids,
                token_estimate,
                created_at: existing.created_at,
                updated_at: now,
            });
        }

        let summary = StoredMemorySummary {
            id: Uuid::new_v4().to_string(),
            scope,
            project_id: project_id.map(str::to_string),
            tool_id: tool_id.map(str::to_string),
            content: content.to_string(),
            version: 1,
            source_entry_ids: source_ids,
            token_estimate,
            created_at: now,
            updated_at: now,
        };
        let source_json =
            serde_json::to_string(&summary.source_entry_ids).map_err(|error| error.to_string())?;
        conn.execute(
            r#"
            INSERT INTO memory_summaries (
                id, scope, project_id, tool_id, content, version, source_entry_ids, token_estimate, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10);
            "#,
            params![
                summary.id,
                summary.scope.as_str(),
                summary.project_id,
                summary.tool_id,
                summary.content,
                summary.version,
                source_json,
                summary.token_estimate,
                summary.created_at,
                summary.updated_at
            ],
        )
        .map_err(|error| error.to_string())?;
        self.insert_summary_version(
            &summary.id,
            summary.version,
            &summary.content,
            &summary.source_entry_ids,
            now,
        )?;
        self.trim_summary_versions(&summary.id, max_versions)?;
        Ok(summary)
    }

    fn insert_summary_version(
        &self,
        summary_id: &str,
        version: i64,
        content: &str,
        source_ids: &[String],
        created_at: f64,
    ) -> Result<(), String> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            INSERT INTO memory_summary_versions (
                id, summary_id, version, content, source_entry_ids, created_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6);
            "#,
            params![
                Uuid::new_v4().to_string(),
                summary_id,
                version,
                content,
                serde_json::to_string(source_ids).map_err(|error| error.to_string())?,
                created_at
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn trim_summary_versions(&self, summary_id: &str, max_versions: i32) -> Result<(), String> {
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            DELETE FROM memory_summary_versions
            WHERE summary_id = ?1
              AND id NOT IN (
                SELECT id
                FROM memory_summary_versions
                WHERE summary_id = ?1
                ORDER BY version DESC
                LIMIT ?2
              );
            "#,
            params![summary_id, max_versions.max(1)],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn mark_entries_merged(
        &self,
        entry_ids: &[String],
        summary_id: &str,
    ) -> Result<(), String> {
        if entry_ids.is_empty() {
            return Ok(());
        }
        let now = now_seconds();
        let mut conn = self.open_connection()?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for id in entry_ids {
            tx.execute(
                r#"
                UPDATE memory_entries
                SET status = 'merged', merged_summary_id = ?1, merged_at = ?2, updated_at = ?2
                WHERE id = ?3 AND status = 'active' AND tier = 'working';
                "#,
                params![summary_id, now, id],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(())
    }

    pub(super) fn merge_stale_working_entries(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        max_active: i32,
        summary_id: &str,
    ) -> Result<(), String> {
        let ids = self.stale_working_entry_ids(scope, project_id, max_active)?;
        self.mark_entries_merged(&ids, summary_id)
    }

    pub(super) fn trim_working_entries(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        max_active: i32,
    ) -> Result<(), String> {
        let ids = self.stale_working_entry_ids(scope, project_id, max_active)?;
        self.archive_entries(&ids)
    }

    fn stale_working_entry_ids(
        &self,
        scope: MemoryScope,
        project_id: Option<&str>,
        max_active: i32,
    ) -> Result<Vec<String>, String> {
        let conn = self.open_connection()?;
        let mut statement = conn
            .prepare(
                r#"
                SELECT id
                FROM memory_entries
                WHERE scope = ?1
                  AND COALESCE(project_id, '') = COALESCE(?2, '')
                  AND tier = 'working'
                  AND status = 'active'
                ORDER BY updated_at DESC
                LIMIT -1 OFFSET ?3;
                "#,
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(
                params![scope.as_str(), project_id, i64::from(max_active.max(0))],
                |row| row.get(0),
            )
            .map_err(|error| error.to_string())?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|error| error.to_string())
    }

    pub(super) fn archive_entries(&self, entry_ids: &[String]) -> Result<(), String> {
        if entry_ids.is_empty() {
            return Ok(());
        }
        let now = now_seconds();
        let mut conn = self.open_connection()?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        for id in entry_ids {
            tx.execute(
                r#"
                UPDATE memory_entries
                SET tier = 'archive', status = 'archived', archived_at = ?1, updated_at = ?1
                WHERE id = ?2;
                "#,
                params![now, id],
            )
            .map_err(|error| error.to_string())?;
        }
        tx.commit().map_err(|error| error.to_string())?;
        Ok(())
    }
}
