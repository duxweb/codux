impl AIUsageStore {
    pub(crate) fn project_snapshot(
        &self,
        conn: &Connection,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistorySnapshot> {
        let links = self.project_session_links(conn, &project.path)?;
        let buckets = self.project_usage_buckets(conn, &project.path)?;
        Ok(build_snapshot_from_rows(project, links, buckets))
    }

    pub(crate) fn indexed_project_snapshot(
        &self,
        conn: &Connection,
        project: AIHistoryProjectRequest,
    ) -> Result<Option<AIHistorySnapshot>> {
        let indexed_at = conn
            .query_row(
                r#"
                SELECT indexed_at
                FROM ai_history_project_index_state
                WHERE project_path = ?1
                LIMIT 1;
                "#,
                params![project.path],
                |row| row.get::<_, f64>(0),
            )
            .optional()?;
        let Some(indexed_at) = indexed_at else {
            return Ok(None);
        };
        let mut snapshot = self.project_snapshot(conn, project)?;
        snapshot.indexed_at = indexed_at;
        Ok(Some(snapshot))
    }

    pub(crate) fn rename_project_session(
        &self,
        conn: &Connection,
        project_path: &str,
        session_id: &str,
        title: &str,
    ) -> Result<bool> {
        let title = title.trim();
        if title.is_empty() {
            return Ok(false);
        }
        let links = self.project_session_links(conn, project_path)?;
        let matched = matching_session_keys(&links, session_id);
        if matched.is_empty() {
            return Ok(false);
        }
        let tx = conn.unchecked_transaction()?;
        for (source, session_key) in &matched {
            tx.execute(
                r#"
                UPDATE ai_history_file_session_link
                SET session_title = ?1
                WHERE project_path = ?2 AND source = ?3 AND session_key = ?4;
                "#,
                params![title, project_path, source, session_key],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub(crate) fn remove_project_session(
        &self,
        conn: &Connection,
        project_path: &str,
        session_id: &str,
    ) -> Result<bool> {
        let links = self.project_session_links(conn, project_path)?;
        let matched = matching_session_keys(&links, session_id);
        if matched.is_empty() {
            return Ok(false);
        }
        let tx = conn.unchecked_transaction()?;
        for (source, session_key) in &matched {
            tx.execute(
                r#"
                DELETE FROM ai_history_file_usage_bucket
                WHERE project_path = ?1 AND source = ?2 AND session_key = ?3;
                "#,
                params![project_path, source, session_key],
            )?;
            tx.execute(
                r#"
                DELETE FROM ai_history_file_session_link
                WHERE project_path = ?1 AND source = ?2 AND session_key = ?3;
                "#,
                params![project_path, source, session_key],
            )?;
        }
        tx.commit()?;
        Ok(true)
    }

    pub(crate) fn save_project_index_state(
        &self,
        conn: &Connection,
        snapshot: &AIHistorySnapshot,
        project_path: &str,
    ) -> Result<()> {
        conn.execute(
            r#"
            INSERT INTO ai_history_project_index_state (project_path, project_id, project_name, indexed_at)
            VALUES (?1, ?2, ?3, ?4)
            ON CONFLICT(project_path) DO UPDATE SET
                project_id = excluded.project_id,
                project_name = excluded.project_name,
                indexed_at = excluded.indexed_at;
            "#,
            params![
                project_path,
                snapshot.project_id,
                snapshot.project_name,
                snapshot.indexed_at
            ],
        )?;
        Ok(())
    }
}
