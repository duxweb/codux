impl MemoryService {
    pub fn summary(&self, project_id: Option<&str>) -> MemorySummary {
        self.summary_with_user_recall(project_id, true)
    }

    pub fn summary_with_user_recall(
        &self,
        project_id: Option<&str>,
        include_user_recall: bool,
    ) -> MemorySummary {
        if !self.database_path.is_file() {
            return MemorySummary {
                error: Some("memory.sqlite3 not found".to_string()),
                ..Default::default()
            };
        }

        let conn = match Connection::open(&self.database_path) {
            Ok(conn) => conn,
            Err(error) => {
                return MemorySummary {
                    error: Some(error.to_string()),
                    ..Default::default()
                };
            }
        };

        match self.summary_from_conn(&conn, project_id, include_user_recall) {
            Ok(summary) => summary,
            Err(error) => MemorySummary {
                available: true,
                error: Some(error),
                ..Default::default()
            },
        }
    }

    fn summary_from_conn(
        &self,
        conn: &Connection,
        project_id: Option<&str>,
        include_user_recall: bool,
    ) -> Result<MemorySummary, String> {
        // Scope the headline "active" count to the same project as core/working
        // so the injected numbers reconcile (previously this counted active rows
        // across every project, so 592 active never matched 165+58 core+working).
        let active_entries = count_entries(conn, None, project_id, Some("active"))?;
        let core_entries = count_entries(conn, Some("core"), project_id, Some("active"))?;
        let working_entries = count_entries(conn, Some("working"), project_id, Some("active"))?;
        let archived_entries = count_entries(conn, None, project_id, Some("archived"))?;
        let summaries = count_summaries(conn, project_id)?;
        let queued_extractions = count_queue(conn, &["queued", "pending", "running"])?;
        let failed_extractions = count_queue(conn, &["failed"])?;
        let project_profile_present = project_id
            .and_then(|id| {
                conn.query_row(
                    "SELECT 1 FROM memory_project_profiles WHERE project_id = ?1 LIMIT 1",
                    [id],
                    |_| Ok(true),
                )
                .optional()
                .ok()
                .flatten()
            })
            .unwrap_or(false);
        let recent_entries = load_recent_entries(conn, project_id, include_user_recall)?;

        Ok(MemorySummary {
            available: true,
            active_entries,
            core_entries,
            working_entries,
            archived_entries,
            summaries,
            queued_extractions,
            failed_extractions,
            project_profile_present,
            recent_entries,
            error: None,
        })
    }
}
