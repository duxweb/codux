impl MemoryService {
    pub(crate) fn extraction_prompt_context(
        &self,
        memory_settings: &MemorySettings,
        project_id: &str,
        query: &str,
    ) -> Result<
        (
            Option<PromptMemorySummary>,
            Vec<PromptMemoryEntry>,
            Vec<PromptMemoryEntry>,
        ),
        String,
    > {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        let user_summary = if memory_settings.allow_cross_project_user_recall {
            conn.query_row(
                    r#"
                SELECT content, version
                FROM memory_summaries
                WHERE scope = 'user'
                  AND project_id IS NULL
                  AND tool_id IS NULL
                LIMIT 1;
                "#,
                [],
                |row| {
                    Ok(PromptMemorySummary {
                        content: row.get(0)?,
                        version: row.get(1)?,
                    })
                },
            )
            .optional()
            .map_err(|error| error.to_string())?
        } else {
            None
        };
        let user_memories = if memory_settings.allow_cross_project_user_recall {
            prompt_entries(
                &conn,
                "user",
                None,
                i64::from(memory_settings.max_injected_user_working_memories.max(0)),
                query,
            )?
        } else {
            Vec::new()
        };
        let project_memories = prompt_entries(
            &conn,
            "project",
            Some(project_id),
            i64::from(memory_settings.max_injected_project_working_memories.max(0)),
            query,
        )?;
        Ok((user_summary, user_memories, project_memories))
    }

    fn update_task_status(
        &self,
        task_id: &str,
        status: &str,
        error: Option<&str>,
        increment_attempts: bool,
    ) -> Result<(), String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.execute(
            r#"
            UPDATE memory_extraction_queue
            SET status = ?1,
                attempts = attempts + ?2,
                error = ?3
            WHERE id = ?4;
            "#,
            params![
                status,
                if increment_attempts { 1_i64 } else { 0_i64 },
                error,
                task_id
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }
}
