impl MemoryService {
    pub fn enqueue_completed_session_if_ready(
        &self,
        memory_settings: &AIMemorySettings,
        projects: &[ProjectWorkspaceRecord],
        session: &AISessionSnapshot,
    ) -> Result<MemoryEnqueueResult, String> {
        let reason = if !memory_settings.enabled || !memory_settings.automatic_extraction_enabled {
            Some("disabled")
        } else if session.state != "idle" || !session.has_completed_turn || session.was_interrupted
        {
            Some("session-not-completed")
        } else if memory_settings.extraction_idle_delay_seconds > 0
            && now_seconds() - session.updated_at
                < f64::from(memory_settings.extraction_idle_delay_seconds)
        {
            Some("idle-delay")
        } else {
            None
        };
        if let Some(reason) = reason {
            return Ok(MemoryEnqueueResult {
                enqueued: false,
                reason: reason.to_string(),
                summary: self.summary(session.project_id.as_str().into()),
            });
        }

        let Some(project) = memory_project_context(projects, session) else {
            return Ok(MemoryEnqueueResult {
                enqueued: false,
                reason: "project-not-found".to_string(),
                summary: self.summary(session.project_id.as_str().into()),
            });
        };
        let Some(source) = resolve_transcript_source(session, &project) else {
            return Ok(MemoryEnqueueResult {
                enqueued: false,
                reason: "transcript-not-found".to_string(),
                summary: self.summary(Some(&project.project_id)),
            });
        };
        self.ensure_queue_schema()?;
        let enqueued = self.enqueue_extraction_if_needed(
            &project.project_id,
            &project.workspace_path,
            &session.tool,
            &session_identifier(session),
            &source.location,
            &source.fingerprint,
            false,
        )?;
        Ok(MemoryEnqueueResult {
            enqueued,
            reason: if enqueued {
                "enqueued"
            } else {
                "already-queued"
            }
            .to_string(),
            summary: self.summary(Some(&project.project_id)),
        })
    }

    pub fn extraction_status_snapshot(&self) -> Result<MemoryExtractionStatusSnapshot, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        let pending_count = queue_count(&conn, "pending")?;
        let running_count = queue_count(&conn, "running")?;
        let last_error = latest_failed_error(&conn)?;
        let status = if running_count > 0 {
            MemoryExtractionStatus::Processing
        } else if pending_count > 0 {
            MemoryExtractionStatus::Queued
        } else if last_error.is_some() {
            MemoryExtractionStatus::Failed
        } else {
            MemoryExtractionStatus::Idle
        };
        Ok(MemoryExtractionStatusSnapshot {
            status,
            pending_count,
            running_count,
            checked_count: 0,
            enqueued_count: 0,
            last_error,
            updated_at: now_seconds(),
        })
    }

    pub fn cancel_extraction_queue(&self) -> Result<MemoryExtractionStatusSnapshot, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE memory_extraction_queue SET status = 'failed', error = ?1 WHERE status IN ('pending', 'running');",
            params!["Memory indexing stopped by user."],
        )
        .map_err(|error| error.to_string())?;
        self.extraction_status_snapshot()
    }

    pub fn recover_interrupted_extraction_tasks(
        &self,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE memory_extraction_queue SET status = 'pending', error = NULL WHERE status = 'running';",
            [],
        )
        .map_err(|error| error.to_string())?;
        self.extraction_status_snapshot()
    }

    pub fn clear_extraction_failures(&self) -> Result<MemoryExtractionStatusSnapshot, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE memory_extraction_queue SET status = 'cleared' WHERE status = 'failed';",
            [],
        )
        .map_err(|error| error.to_string())?;
        self.extraction_status_snapshot()
    }

    pub fn clear_extraction_task(
        &self,
        task_id: &str,
        statuses: &[&str],
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let task_id = task_id.trim();
        if task_id.is_empty() {
            return Err("Memory extraction task id is empty.".to_string());
        }
        if statuses.is_empty() {
            return Err("Memory extraction task status list is empty.".to_string());
        }
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        let placeholders = std::iter::repeat("?")
            .take(statuses.len())
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "UPDATE memory_extraction_queue SET status = 'cleared' WHERE id = ? AND status IN ({placeholders});"
        );
        let mut values = Vec::with_capacity(statuses.len() + 1);
        values.push(task_id);
        values.extend(statuses.iter().copied());
        let changed = conn
            .execute(&sql, rusqlite::params_from_iter(values))
            .map_err(|error| error.to_string())?;
        if changed == 0 {
            return Err("Memory extraction task not found.".to_string());
        }
        self.extraction_status_snapshot()
    }

    pub fn failed_extraction_tasks(
        &self,
        project_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<MemoryExtractionTask>, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        db_failed_extraction_tasks(&conn, project_id, limit)
    }

    pub fn active_extraction_tasks(
        &self,
        project_id: Option<&str>,
        limit: i64,
    ) -> Result<Vec<MemoryExtractionTask>, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        db_active_extraction_tasks(&conn, project_id, limit)
    }

    pub(super) fn enqueue_extraction_if_needed(
        &self,
        project_id: &str,
        workspace_path: &str,
        tool: &str,
        session_id: &str,
        transcript_path: &str,
        source_fingerprint: &str,
        allow_retry_failed: bool,
    ) -> Result<bool, String> {
        let conn = self.open_or_create_connection()?;
        let existing: Option<String> = conn
            .query_row(
                "SELECT status FROM memory_extraction_queue WHERE source_fingerprint = ?1 LIMIT 1;",
                params![source_fingerprint],
                |row| row.get(0),
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(status) = existing {
            if allow_retry_failed && status == "failed" {
                conn.execute(
                    r#"
                    UPDATE memory_extraction_queue
                    SET project_id = ?1,
                        tool = ?2,
                        session_id = ?3,
                        transcript_path = ?4,
                        workspace_path = ?5,
                        status = 'pending',
                        error = NULL,
                        enqueued_at = ?6
                    WHERE source_fingerprint = ?7;
                    "#,
                    params![
                        project_id,
                        tool,
                        session_id,
                        transcript_path,
                        workspace_path,
                        now_seconds(),
                        source_fingerprint
                    ],
                )
                .map_err(|error| error.to_string())?;
                return Ok(true);
            }
            return Ok(false);
        }
        conn.execute(
            r#"
            INSERT INTO memory_extraction_queue (
                id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', 0, NULL, ?8);
            "#,
            params![
                uuid::Uuid::new_v4().to_string(),
                project_id,
                tool,
                session_id,
                transcript_path,
                workspace_path,
                source_fingerprint,
                now_seconds()
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(true)
    }
}
