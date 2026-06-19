use super::{
    MemoryProjectMigrationRequest, MemoryService, MemorySummary, MemorySummaryRow,
    MemorySummaryUpdateRequest, now_seconds,
};
use crate::extraction::MemoryScope;
use rusqlite::{OptionalExtension, params};

impl MemoryService {
    pub fn delete_project_memory(&self, project_id: &str) -> Result<MemorySummary, String> {
        let project_id = project_id.trim();
        if project_id.is_empty() {
            return Err("Project id is empty.".to_string());
        }
        self.ensure_queue_schema()?;
        let mut conn = self.open_connection()?;
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        delete_project_memory_in_tx(&tx, project_id)?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(self.summary(Some(project_id)))
    }

    pub fn migrate_project_memory(
        &self,
        request: MemoryProjectMigrationRequest,
    ) -> Result<MemorySummary, String> {
        let from_project_id = request.from_project_id.trim();
        let to_project_id = request.to_project_id.trim();
        if from_project_id.is_empty() || to_project_id.is_empty() {
            return Err("project id cannot be empty".to_string());
        }
        if from_project_id == to_project_id {
            return Err("source and target project are the same".to_string());
        }

        self.ensure_queue_schema()?;
        let mut conn = self.open_connection()?;
        if project_memory_total_count(&conn, from_project_id)? == 0 {
            return Err("source project memory is empty".to_string());
        }
        if project_memory_total_count(&conn, to_project_id)? > 0 && !request.overwrite {
            return Err("target project already has memory".to_string());
        }

        let now = now_seconds();
        let tx = conn.transaction().map_err(|error| error.to_string())?;
        if request.overwrite {
            delete_project_memory_in_tx(&tx, to_project_id)?;
        }
        tx.execute(
            r#"
            UPDATE memory_entries
            SET project_id = ?1, updated_at = ?2
            WHERE scope = 'project' AND project_id = ?3;
            "#,
            params![to_project_id, now, from_project_id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            r#"
            UPDATE memory_summaries
            SET project_id = ?1, updated_at = ?2
            WHERE scope = 'project' AND project_id = ?3;
            "#,
            params![to_project_id, now, from_project_id],
        )
        .map_err(|error| error.to_string())?;
        tx.execute(
            r#"
            UPDATE memory_project_profiles
            SET project_id = ?1, updated_at = ?2
            WHERE project_id = ?3;
            "#,
            params![to_project_id, now, from_project_id],
        )
        .map_err(|error| error.to_string())?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(self.summary(Some(to_project_id)))
    }

    pub fn update_summary(
        &self,
        request: MemorySummaryUpdateRequest,
    ) -> Result<MemorySummaryRow, String> {
        let content = request.content.trim();
        if content.is_empty() {
            return Err("summary content cannot be empty".to_string());
        }
        self.ensure_queue_schema()?;
        let existing = self.summary_row_by_id(&request.summary_id)?;
        let conn = self.open_connection()?;
        let updated = self.upsert_summary(
            &conn,
            MemoryScope::from_str(&existing.scope),
            existing.project_id.as_deref(),
            existing.tool_id.as_deref(),
            content,
            &existing.source_entry_ids,
            request.max_versions.unwrap_or(20).max(1),
        )?;
        Ok(MemorySummaryRow {
            id: updated.id,
            scope: updated.scope.as_str().to_string(),
            project_id: updated.project_id,
            tool_id: updated.tool_id,
            content: updated.content,
            version: updated.version,
            source_entry_ids: updated.source_entry_ids,
            token_estimate: updated.token_estimate,
            created_at: updated.created_at,
            updated_at: updated.updated_at,
        })
    }

    fn summary_row_by_id(&self, summary_id: &str) -> Result<MemorySummaryRow, String> {
        let summary_id = summary_id.trim();
        if summary_id.is_empty() {
            return Err("summary id cannot be empty".to_string());
        }
        let conn = self.open_connection()?;
        conn.query_row(
            r#"
            SELECT id, scope, project_id, tool_id, content, version, source_entry_ids,
                   token_estimate, created_at, updated_at
            FROM memory_summaries
            WHERE id = ?1
            LIMIT 1;
            "#,
            params![summary_id],
            |row| {
                let source_ids: Option<String> = row.get(6)?;
                Ok(MemorySummaryRow {
                    id: row.get(0)?,
                    scope: row.get(1)?,
                    project_id: row.get(2)?,
                    tool_id: row.get(3)?,
                    content: row.get(4)?,
                    version: row.get(5)?,
                    source_entry_ids: super::decode_string_array(source_ids.as_deref()),
                    token_estimate: row.get(7)?,
                    created_at: row.get(8)?,
                    updated_at: row.get(9)?,
                })
            },
        )
        .optional()
        .map_err(|error| error.to_string())?
        .ok_or_else(|| "summary not found".to_string())
    }
}

fn delete_project_memory_in_tx(
    tx: &rusqlite::Transaction<'_>,
    project_id: &str,
) -> Result<(), String> {
    tx.execute(
        "DELETE FROM memory_entries WHERE scope = 'project' AND project_id = ?1;",
        params![project_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        r#"
        DELETE FROM memory_summary_versions
        WHERE summary_id IN (
            SELECT id FROM memory_summaries
            WHERE scope = 'project' AND project_id = ?1
        );
        "#,
        params![project_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM memory_summaries WHERE scope = 'project' AND project_id = ?1;",
        params![project_id],
    )
    .map_err(|error| error.to_string())?;
    tx.execute(
        "DELETE FROM memory_project_profiles WHERE project_id = ?1;",
        params![project_id],
    )
    .map_err(|error| error.to_string())?;
    Ok(())
}

fn project_memory_total_count(
    conn: &rusqlite::Connection,
    project_id: &str,
) -> Result<i64, String> {
    let entries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_entries WHERE scope = 'project' AND project_id = ?1;",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let summaries: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_summaries WHERE scope = 'project' AND project_id = ?1;",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    let profiles: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM memory_project_profiles WHERE project_id = ?1;",
            params![project_id],
            |row| row.get(0),
        )
        .map_err(|error| error.to_string())?;
    Ok(entries + summaries + profiles)
}
