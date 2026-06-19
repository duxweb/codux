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

    /// Reset a task to pending without counting an attempt -- used when the
    /// failure was a transient SQLite lock, which is not the task's fault and
    /// must not become a permanent failed record.
    pub fn requeue_extraction_task(&self, task_id: &str) -> Result<(), String> {
        self.update_task_status(task_id, "pending", None, false)
    }

    pub fn resolve_extraction_task_transcript(
        &self,
        projects: &[MemoryProjectRecord],
        task: &MemoryExtractionTask,
    ) -> Result<String, String> {
        let project = memory_project_context_for_task(projects, task)
            .ok_or_else(|| "Project not found for memory extraction.".to_string())?;
        resolve_transcript_for_task(task, &project)
    }

    pub async fn process_next_memory_extraction_task(
        &self,
        settings: &MemoryConfig,
        projects: &[MemoryProjectRecord],
        output_locale: &str,
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
            .process_extraction_task(settings, projects, task.clone(), output_locale)
            .await;
        match result {
            Ok(()) => self.mark_extraction_task_done(&task.id)?,
            Err(error) => {
                // A transient SQLite lock must not become a permanent failed
                // record -- requeue so the next pass retries it. (busy_timeout +
                // WAL should usually keep the lock from surfacing at all.)
                if is_retryable_memory_lock_error(&error) {
                    self.requeue_extraction_task(&task.id)?;
                } else {
                    self.mark_extraction_task_failed(&task.id, &error)?;
                }
                return Err(error);
            }
        }
        self.extraction_status_snapshot()
    }

    pub async fn process_memory_extraction_queue(
        &self,
        settings: &MemoryConfig,
        projects: &[MemoryProjectRecord],
        output_locale: &str,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        loop {
            match self
                .process_next_memory_extraction_task(settings, projects, output_locale)
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
        settings: &MemoryConfig,
        projects: &[MemoryProjectRecord],
        output_locale: &str,
        limit: usize,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let limit = limit.max(1);
        let mut processed = 0_usize;
        loop {
            match self
                .process_next_memory_extraction_task(settings, projects, output_locale)
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
        settings: &MemoryConfig,
        projects: &[MemoryProjectRecord],
        task: MemoryExtractionTask,
        output_locale: &str,
    ) -> Result<(), String> {
        let project = memory_project_context_for_task(projects, &task)
            .ok_or_else(|| "Project not found for memory extraction.".to_string())?;
        let provider = select_memory_provider(settings, Some(&task.tool))
            .cloned()
            .ok_or_else(|| "No available AI provider is configured.".to_string())?;
        let (response_text, response) = {
            let transcript =
                resolve_transcript_for_task_with_settings(&task, &project, &settings.memory)?;
            let (user_summary, user_memories, project_memories) =
                self.extraction_prompt_context(&settings.memory, &task.project_id, &transcript)?;
            let prompt = make_extraction_prompt(
                &transcript,
                user_summary.as_ref(),
                &user_memories,
                &project_memories,
                &project.project_name,
                output_locale,
                &settings.memory,
            );
            complete_and_decode_memory_extraction_with_retry(
                &provider,
                &prompt,
                Some(extraction_system_prompt()),
            )
            .await
            .map_err(|error| format!("{} failed: {}", provider_summary(&provider), error))?
        };
        let _ = response_text;
        let project_info = MemoryProjectInfo {
            id: project.project_id.clone(),
            name: project.project_name.clone(),
            path: project.workspace_path.clone(),
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
        response: crate::extraction::MemoryExtractionResponse,
        task: &MemoryExtractionTask,
        settings: &MemoryConfig,
        project: &MemoryProjectInfo,
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

async fn complete_memory_extraction_with_retry(
    provider: &crate::MemoryProvider,
    prompt: &str,
    system_prompt: Option<&str>,
) -> Result<String, String> {
    let options = llm::LLMProviderCompletionOptions {
        max_tokens: 4096,
        temperature: 0.1,
        preserve_formatting: true,
        json_response: true,
        json_schema: Some(llm::LLMJsonSchema {
            name: "codux_memory_extraction".to_string(),
            description: Some("Codux durable memory extraction result.".to_string()),
            schema: memory_extraction_json_schema(),
        }),
        timeout_seconds: 120,
    };
    let mut last_error = None;
    for attempt in 0..3 {
        match llm::complete_with_provider_options(provider, prompt, system_prompt, options.clone())
            .await
        {
            Ok(response) => return Ok(response),
            Err(error) if is_transient_memory_provider_error(&error) && attempt < 2 => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(300 * (attempt as u64 + 1))).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| "Memory extraction provider retry failed.".to_string()))
}

async fn complete_and_decode_memory_extraction_with_retry(
    provider: &crate::MemoryProvider,
    prompt: &str,
    system_prompt: Option<&str>,
) -> Result<(String, crate::extraction::MemoryExtractionResponse), String> {
    let mut last_error = None;
    for attempt in 0..3 {
        let response_text = complete_memory_extraction_with_retry(provider, prompt, system_prompt).await?;
        match decode_extraction_response_detailed(&response_text) {
            Ok(response) => return Ok((response_text, response)),
            Err(error) if attempt < 2 => {
                last_error = Some(error);
                tokio::time::sleep(Duration::from_millis(300 * (attempt as u64 + 1))).await;
            }
            Err(error) => return Err(error),
        }
    }
    Err(last_error.unwrap_or_else(|| {
        "Memory extraction provider returned malformed memory JSON.".to_string()
    }))
}

fn is_transient_memory_provider_error(error: &str) -> bool {
    let message = error.to_lowercase();
    [
        "empty response",
        "error decoding response body",
        "timeout",
        "timed out",
        "eof",
        "connection reset",
        "connection closed",
        // Connect/DNS-level reqwest failures (e.g. "Reqwest error: error sending
        // request for url ..."): the request never reached the provider, which
        // is a transient transport blip, not a permanent task failure.
        "error sending request",
        "reqwest error",
        "dns error",
        "connect",
        "network",
        "temporarily unavailable",
        "too many requests",
        "rate limit",
        "429",
        "502",
        "503",
        "504",
    ]
    .iter()
    .any(|needle| message.contains(needle))
}

/// A transient SQLite lock (`database is locked` / `SQLITE_BUSY`) that should
/// requeue the task rather than record a permanent failure.
fn is_retryable_memory_lock_error(error: &str) -> bool {
    let message = error.to_lowercase();
    message.contains("database is locked")
        || message.contains("database table is locked")
        || message.contains("sqlite_busy")
}

#[cfg(test)]
mod process_tests {
    use super::{is_retryable_memory_lock_error, is_transient_memory_provider_error};

    #[test]
    fn classifies_only_transport_like_memory_provider_errors_as_transient() {
        assert!(is_transient_memory_provider_error(
            "The AI provider returned an empty response."
        ));
        assert!(is_transient_memory_provider_error(
            "error decoding response body for url"
        ));
        assert!(is_transient_memory_provider_error(
            "request failed with status 503"
        ));
        // Connect/DNS-level reqwest blips must be retryable (the deepseek case).
        assert!(is_transient_memory_provider_error(
            "ds request failed for model deepseek-v4-flash: Web call failed ... Cause: Reqwest error: error sending request for url (https://api.deepseek.com/v1/chat/completions)"
        ));
        assert!(is_transient_memory_provider_error("dns error: failed to lookup address"));
        assert!(is_transient_memory_provider_error("tcp connect error: connection refused"));
        assert!(!is_transient_memory_provider_error("invalid api key"));
        assert!(!is_transient_memory_provider_error(
            "Memory extraction provider returned malformed memory JSON."
        ));
    }

    #[test]
    fn classifies_sqlite_lock_errors_as_retryable() {
        assert!(is_retryable_memory_lock_error("database is locked"));
        assert!(is_retryable_memory_lock_error(
            "failed to apply extraction: database is locked"
        ));
        assert!(!is_retryable_memory_lock_error("invalid api key"));
        assert!(!is_retryable_memory_lock_error("no such table"));
    }
}
