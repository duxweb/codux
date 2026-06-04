impl MemoryService {
    pub fn next_pending_extraction_task(&self) -> Result<Option<MemoryExtractionTask>, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.query_row(
            r#"
            SELECT id, project_id, tool, session_id, transcript_path, workspace_path, source_fingerprint, status, attempts, error, enqueued_at
            FROM memory_extraction_queue
            WHERE status = 'pending'
            ORDER BY enqueued_at ASC
            LIMIT 1;
            "#,
            [],
            memory_task_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
    }

    pub fn has_pending_extraction_task(&self) -> Result<bool, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        Ok(queue_count(&conn, "pending")? > 0)
    }

    pub fn mark_extraction_task_running(&self, task_id: &str) -> Result<(), String> {
        self.update_task_status(task_id, "running", None, true)
    }

    pub fn mark_extraction_task_done(&self, task_id: &str) -> Result<(), String> {
        self.update_task_status(task_id, "done", None, false)
    }

    pub fn mark_extraction_task_failed(&self, task_id: &str, error: &str) -> Result<(), String> {
        self.update_task_status(task_id, "failed", Some(error), false)
    }

    pub fn resolve_extraction_task_transcript(
        &self,
        projects: &[ProjectWorkspaceRecord],
        task: &MemoryExtractionTask,
    ) -> Result<String, String> {
        let project = memory_project_context_for_task(projects, task)
            .ok_or_else(|| "Project not found for memory extraction.".to_string())?;
        resolve_transcript_for_task(task, &project)
    }

    pub async fn process_next_memory_extraction_task(
        &self,
        settings: &AISettings,
        projects: &[ProjectWorkspaceRecord],
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        if !settings.memory.enabled {
            return self.extraction_status_snapshot();
        }
        let Some(task) = self.next_pending_extraction_task()? else {
            return self.extraction_status_snapshot();
        };
        ensure_memory_provider_available(settings)?;
        self.mark_extraction_task_running(&task.id)?;
        let result = self
            .process_extraction_task(settings, projects, task.clone())
            .await;
        match result {
            Ok(()) => self.mark_extraction_task_done(&task.id)?,
            Err(error) => {
                self.mark_extraction_task_failed(&task.id, &error)?;
                return Err(error);
            }
        }
        self.extraction_status_snapshot()
    }

    pub async fn process_memory_extraction_queue(
        &self,
        settings: &AISettings,
        projects: &[ProjectWorkspaceRecord],
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        loop {
            match self
                .process_next_memory_extraction_task(settings, projects)
                .await
            {
                Ok(status) if status.pending_count > 0 => continue,
                Ok(status) => return Ok(status),
                Err(error) if should_stop_memory_queue_after_error(&error) => return Err(error),
                Err(_) if self.has_pending_extraction_task()? => continue,
                Err(error) => return Err(error),
            }
        }
    }

    pub async fn process_memory_extraction_queue_limited(
        &self,
        settings: &AISettings,
        projects: &[ProjectWorkspaceRecord],
        limit: usize,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let limit = limit.max(1);
        let mut processed = 0_usize;
        loop {
            match self
                .process_next_memory_extraction_task(settings, projects)
                .await
            {
                Ok(status) => {
                    processed += 1;
                    if status.pending_count > 0 && processed < limit {
                        tokio::time::sleep(MEMORY_EXTRACTION_TASK_INTERVAL).await;
                        continue;
                    }
                    return Ok(status);
                }
                Err(error) if should_stop_memory_queue_after_error(&error) => return Err(error),
                Err(_) if self.has_pending_extraction_task()? && processed + 1 < limit => {
                    processed += 1;
                    tokio::time::sleep(MEMORY_EXTRACTION_TASK_INTERVAL).await;
                    continue;
                }
                Err(error) => return Err(error),
            }
        }
    }

    async fn process_extraction_task(
        &self,
        settings: &AISettings,
        projects: &[ProjectWorkspaceRecord],
        task: MemoryExtractionTask,
    ) -> Result<(), String> {
        let project = memory_project_context_for_task(projects, &task)
            .ok_or_else(|| "Project not found for memory extraction.".to_string())?;
        let provider = select_memory_provider(settings, Some(&task.tool))
            .cloned()
            .ok_or_else(|| "No available AI provider is configured.".to_string())?;
        let response_text = {
            let transcript = resolve_transcript_for_task(&task, &project)?;
            let (user_summary, user_memories, project_memories) =
                self.extraction_prompt_context(&settings.memory, &task.project_id, &transcript)?;
            let prompt = make_extraction_prompt(
                &transcript,
                user_summary.as_ref(),
                &user_memories,
                &project_memories,
                &project.project_name,
                &settings.memory,
            );
            llm::complete_with_provider_options(
                &provider,
                &prompt,
                Some(extraction_system_prompt()),
                llm::LLMProviderCompletionOptions {
                    max_tokens: 4096,
                    temperature: 0.1,
                    preserve_formatting: true,
                    json_response: true,
                    timeout_seconds: 120,
                },
            )
            .await
            .map_err(|error| format!("{} failed: {}", provider_summary(&provider), error))?
        };
        let response = decode_extraction_response(&response_text)?;
        let project_info = ProjectInfo {
            id: project.project_id.clone(),
            name: project.project_name.clone(),
            path: project.workspace_path.clone(),
            exists: true,
            badge: crate::project_store::badge_from_name(&project.project_name),
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
        };
        self.apply_extraction_response_with_profile_refresh(
            response,
            &task,
            settings,
            &project_info,
        )
        .await
    }

    pub(crate) async fn apply_extraction_response_with_profile_refresh(
        &self,
        response: crate::memory::extraction::MemoryExtractionResponse,
        task: &MemoryExtractionTask,
        settings: &AISettings,
        project: &ProjectInfo,
    ) -> Result<(), String> {
        let project_profile_refresh_recommended = response.project_profile_refresh_recommended;
        self.apply_extraction_response(response, task, &settings.memory)?;
        if project_profile_refresh_recommended {
            let _ = self
                .force_refresh_project_profile_with_llm_detailed(settings, &project)
                .await;
        }
        Ok(())
    }
}
