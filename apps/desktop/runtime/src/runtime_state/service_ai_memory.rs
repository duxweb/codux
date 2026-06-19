impl RuntimeService {
    fn memory_extraction_output_locale(&self) -> String {
        let language = SettingsService::new(self.support_dir.clone())
            .summary()
            .language;
        crate::settings::locale_from_language_setting(&language)
    }

    pub fn reload_project_ai_history(&self, project_path: &str) -> AIHistorySummary {
        load_ai_history(&self.support_dir, project_path)
    }

    pub fn reload_global_ai_history(&self) -> AIGlobalHistorySummary {
        load_global_ai_history(&self.support_dir)
    }

    pub fn reload_project_ai_session_detail(
        &self,
        project_path: &str,
        session_id: &str,
    ) -> AISessionDetail {
        let mut args = serde_json::Map::new();
        args.insert("sessionId".to_string(), session_id.into());
        if let Some(result) = self.remote_ai_session(project_path, "detail", args) {
            return result
                .ok()
                .and_then(|value| serde_json::from_value(value).ok())
                .unwrap_or_else(|| AISessionDetail {
                    id: session_id.to_string(),
                    error: Some("Unable to load remote session detail.".to_string()),
                    ..Default::default()
                });
        }
        AIHistoryService::new(self.support_dir.clone())
            .project_session_detail(project_path, session_id)
            .unwrap_or_else(|error| AISessionDetail {
                id: session_id.to_string(),
                error: Some(error),
                ..Default::default()
            })
    }

    pub fn fork_ai_session(
        &self,
        request: AISessionForkRequest,
    ) -> Result<AISessionForkResult, String> {
        let mut args = serde_json::Map::new();
        args.insert("projectId".to_string(), request.project_id.clone().into());
        args.insert("projectName".to_string(), request.project_name.clone().into());
        args.insert("sessionId".to_string(), request.session_id.clone().into());
        if let Ok(target) = serde_json::to_value(request.target_tool) {
            args.insert("targetTool".to_string(), target);
        }
        if let Some(result) = self.remote_ai_session(&request.project_path, "fork", args) {
            return result.and_then(|value| {
                serde_json::from_value(value).map_err(|error| error.to_string())
            });
        }
        AIHistoryService::new(self.support_dir.clone()).fork_project_session(request)
    }

    pub fn rename_ai_session(
        &self,
        project_path: &str,
        session_id: &str,
        title: &str,
    ) -> Result<AIHistorySummary, String> {
        let mut args = serde_json::Map::new();
        args.insert("sessionId".to_string(), session_id.into());
        args.insert("title".to_string(), title.into());
        if let Some(result) = self.remote_ai_session(project_path, "rename", args) {
            return result.and_then(|value| {
                serde_json::from_value(value).map_err(|error| error.to_string())
            });
        }
        AIHistoryService::new(self.support_dir.clone()).rename_project_session(
            project_path,
            session_id,
            title,
        )
    }

    pub fn remove_ai_session(
        &self,
        project_path: &str,
        session_id: &str,
    ) -> Result<AIHistorySummary, String> {
        let mut args = serde_json::Map::new();
        args.insert("sessionId".to_string(), session_id.into());
        if let Some(result) = self.remote_ai_session(project_path, "remove", args) {
            return result.and_then(|value| {
                serde_json::from_value(value).map_err(|error| error.to_string())
            });
        }
        AIHistoryService::new(self.support_dir.clone())
            .remove_project_session(project_path, session_id)
    }

    pub fn reload_memory(&self, project_id: Option<&str>) -> MemorySummary {
        if let Some(pid) = project_id {
            if let Some(result) = self.remote_memory_read(pid, "summary", Default::default()) {
                return result
                    .ok()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
            }
        }
        load_memory(&self.support_dir, project_id)
    }

    pub fn prepare_memory_launch_artifacts(
        &self,
        project_id: &str,
        project_name: &str,
        workspace_path: &str,
    ) -> Option<crate::memory::MemoryLaunchArtifacts> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let ssh_context =
            render_ssh_launch_context_from_support_dir(self.support_dir.clone(), None);
        MemoryService::new(self.support_dir.clone()).prepare_launch_artifacts(
            &crate::runtime_paths::runtime_root_dir(),
            crate::memory::MemoryLaunchRequest {
                project_id: project_id.to_string(),
                project_name: project_name.to_string(),
                workspace_path: Some(workspace_path.to_string()),
                settings: crate::memory::memory_config(&settings),
                extra_context: ssh_context,
            },
        )
    }

    pub fn enqueue_completed_session_memory(
        &self,
        session: &crate::ai_runtime::AISessionSnapshot,
    ) -> Result<MemoryEnqueueResult, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let projects = self.memory_project_workspaces();
        MemoryService::new(self.support_dir.clone()).enqueue_completed_session_if_ready(
            &crate::memory::memory_settings(&settings.memory),
            &projects,
            &crate::memory::memory_session(session),
        )
    }

    pub fn memory_extraction_status(&self) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone()).extraction_status_snapshot()
    }

    /// Trigger memory extraction for a remote-hosted project on its host. The
    /// desktop forwards its selected AI provider config (incl. API key) over the
    /// iroh-encrypted transport; the host runs the engine against its own AI
    /// sessions and does not persist the provider. Returns the host's refreshed
    /// extraction status. Errs if the project is not remote-hosted.
    pub fn extract_remote_project_memory(
        &self,
        project_id: &str,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let (device_id, _path) = self
            .remote_project_for_id(project_id)
            .ok_or_else(|| "Project is not remote-hosted.".to_string())?;
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let config = serde_json::to_value(crate::memory::memory_config(&settings))
            .map_err(|error| error.to_string())?;
        let output_locale = self.memory_extraction_output_locale();
        let controller = self.remote_controllers.controller_for(&device_id)?;
        let result = controller.memory_extract(config, &output_locale)?;
        serde_json::from_value(result).map_err(|error| error.to_string())
    }

    pub fn automatic_memory_extraction_available(&self) -> bool {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        crate::memory::extraction::select_memory_provider(
            &crate::memory::memory_config(&settings),
            None,
        )
        .is_some()
    }

    pub fn cancel_memory_extraction_queue(&self) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone()).cancel_extraction_queue()
    }

    pub fn recover_interrupted_memory_extraction_queue(
        &self,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone()).recover_interrupted_extraction_tasks()
    }

    pub fn clear_memory_extraction_failures(
        &self,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone()).clear_extraction_failures()
    }

    pub fn clear_failed_memory_extraction(
        &self,
        task_id: &str,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone()).clear_extraction_task(task_id, &["failed"])
    }

    pub fn clear_pending_memory_extraction(
        &self,
        task_id: &str,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone())
            .clear_extraction_task(task_id, &["queued", "pending"])
    }

    pub fn retry_failed_memory_extraction(
        &self,
        task_id: &str,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        MemoryService::new(self.support_dir.clone()).retry_failed_extraction_task(task_id)
    }

    pub fn enqueue_automatic_memory_extraction_candidates(
        &self,
    ) -> Result<MemoryExtractionEnqueueResult, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let memory_service = MemoryService::new(self.support_dir.clone());
        let status = memory_service.extraction_status_snapshot()?;
        if status.pending_count > 0 || status.running_count > 0 {
            return Ok(MemoryExtractionEnqueueResult {
                checked_count: 0,
                enqueued_count: 0,
                status,
            });
        }

        let projects = self.memory_project_workspaces();
        self.refresh_global_ai_history_index();
        let _ = self.ai_runtime.poll_runtime_state();
        let runtime_state = self.ai_runtime.runtime_state_snapshot();
        let history_sessions = indexed_sessions_since_at(self.ai_usage_database_path(), None)
            .map_err(|error| error.to_string())?;
        memory_service.enqueue_automatic_extraction_candidates(
            &crate::memory::memory_settings(&settings.memory),
            &projects,
            &crate::memory::memory_sessions(&runtime_state.sessions),
            &history_sessions,
        )
    }

    pub async fn process_memory_sessions_now(
        &self,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let output_locale = self.memory_extraction_output_locale();
        let projects = self.memory_project_workspaces();
        self.refresh_global_ai_history_index();
        let _ = self.ai_runtime.poll_runtime_state();
        let runtime_state = self.ai_runtime.runtime_state_snapshot();
        let history_sessions = indexed_sessions_since_at(self.ai_usage_database_path(), None)
            .map_err(|error| error.to_string())?;
        MemoryService::new(self.support_dir.clone())
            .process_memory_sessions_now(
                &crate::memory::memory_config(&settings),
                &projects,
                &crate::memory::memory_sessions(&runtime_state.sessions),
                &history_sessions,
                &output_locale,
            )
            .await
    }

    pub async fn process_next_memory_extraction_task(
        &self,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let output_locale = self.memory_extraction_output_locale();
        let projects = self.memory_project_workspaces();
        MemoryService::new(self.support_dir.clone())
            .process_next_memory_extraction_task(
                &crate::memory::memory_config(&settings),
                &projects,
                &output_locale,
            )
            .await
    }

    pub async fn process_memory_extraction_queue(
        &self,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let output_locale = self.memory_extraction_output_locale();
        let projects = self.memory_project_workspaces();
        MemoryService::new(self.support_dir.clone())
            .process_memory_extraction_queue(
                &crate::memory::memory_config(&settings),
                &projects,
                &output_locale,
            )
            .await
    }

    pub async fn process_memory_extraction_queue_limited(
        &self,
        limit: usize,
    ) -> Result<MemoryExtractionStatusSnapshot, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let output_locale = self.memory_extraction_output_locale();
        let projects = self.memory_project_workspaces();
        MemoryService::new(self.support_dir.clone())
            .process_memory_extraction_queue_limited(
                &crate::memory::memory_config(&settings),
                &projects,
                &output_locale,
                limit,
            )
            .await
    }

    fn memory_project_infos(&self) -> Vec<crate::memory::MemoryProjectInfo> {
        ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .map(|project| crate::memory::MemoryProjectInfo {
                id: project.id,
                name: project.name,
                path: project.path,
            })
            .collect()
    }

    fn memory_project_workspaces(&self) -> Vec<crate::memory::MemoryProjectRecord> {
        crate::memory::memory_project_records(
            &ProjectStore::new(self.support_dir.clone()).project_workspaces_snapshot(),
        )
    }

    fn refresh_global_ai_history_index(&self) {
        let root_projects = ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .map(|project| AIHistoryProjectRequest {
                id: project.id,
                name: project.name,
                path: project.path,
            })
            .collect();
        let _ = index_global_history_fresh_at(
            root_projects,
            crate::ai_usage_store::AIUsageStore::at_path(self.ai_usage_database_path()),
        );
    }

    pub fn reload_notifications(&self) -> NotificationSummary {
        load_notifications(&self.support_dir)
    }

    pub fn toggle_notification_channel(
        &self,
        channel_id: &str,
    ) -> Result<NotificationSummary, String> {
        NotificationService::new(self.support_dir.clone()).toggle_channel(channel_id)
    }

    pub fn set_notification_channel_enabled(
        &self,
        channel_id: &str,
        enabled: bool,
    ) -> Result<NotificationSummary, String> {
        NotificationService::new(self.support_dir.clone()).set_channel_enabled(channel_id, enabled)
    }

    pub fn update_notification_channel_string(
        &self,
        channel_id: &str,
        key: &str,
        value: &str,
    ) -> Result<NotificationSummary, String> {
        NotificationService::new(self.support_dir.clone())
            .update_channel_string(channel_id, key, value)
    }

    pub fn test_notification_channel(
        &self,
        channel_id: &str,
    ) -> Result<NotificationDispatchResult, String> {
        NotificationService::new(self.support_dir.clone()).test_channel(channel_id)
    }

    pub fn reload_memory_manager(
        &self,
        projects: &[ProjectInfo],
        scope: &str,
        project_id: Option<&str>,
        tab: &str,
    ) -> MemoryManagerSnapshot {
        load_memory_manager(&self.support_dir, projects, scope, project_id, tab)
    }

    pub fn memory_management_snapshot(
        &self,
        request: MemoryManagementRequest,
    ) -> Result<MemoryManagementSnapshot, String> {
        if let Some(pid) = request.project_id.clone() {
            let mut args = serde_json::Map::new();
            args.insert("scope".to_string(), request.scope.clone().into());
            if let Some(tier) = &request.tier {
                args.insert("tier".to_string(), tier.clone().into());
            }
            if let Some(status) = &request.status {
                args.insert("status".to_string(), status.clone().into());
            }
            if let Some(limit) = request.limit {
                args.insert("limit".to_string(), limit.into());
            }
            if let Some(result) = self.remote_memory_read(&pid, "management", args) {
                return result.and_then(|value| {
                    serde_json::from_value(value).map_err(|error| error.to_string())
                });
            }
        }
        MemoryService::new(self.support_dir.clone()).management_snapshot(request)
    }

    pub fn memory_manager_snapshot(
        &self,
        projects: &[ProjectInfo],
        request: MemoryManagerSnapshotRequest,
    ) -> MemoryManagerSnapshot {
        if let Some(pid) = request.project_id.clone() {
            let mut args = serde_json::Map::new();
            args.insert("scope".to_string(), request.scope.clone().into());
            args.insert("tab".to_string(), request.tab.clone().into());
            if let Some(limit) = request.limit {
                args.insert("limit".to_string(), limit.into());
            }
            if let Some(result) = self.remote_memory_read(&pid, "manager", args) {
                return result
                    .ok()
                    .and_then(|value| serde_json::from_value(value).ok())
                    .unwrap_or_default();
            }
        }
        MemoryService::new(self.support_dir.clone())
            .manager_snapshot_for_request(&crate::memory::memory_project_infos(projects), request)
    }

    pub fn archive_memory_entry(
        &self,
        project_id: Option<&str>,
        entry_id: &str,
    ) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone())
            .set_entry_status(project_id, entry_id, "archived")
    }

    pub fn restore_memory_entry(
        &self,
        project_id: Option<&str>,
        entry_id: &str,
    ) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone())
            .set_entry_status(project_id, entry_id, "active")
    }

    pub fn delete_memory_entry(
        &self,
        project_id: Option<&str>,
        entry_id: &str,
    ) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone()).delete_entry(project_id, entry_id)
    }

    pub fn delete_memory_summary(
        &self,
        project_id: Option<&str>,
        summary_id: &str,
    ) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone()).delete_summary(project_id, summary_id)
    }

    pub fn delete_memory_project_profile(&self, project_id: &str) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone()).delete_project_profile(project_id)
    }

    pub fn delete_memory_project(&self, project_id: &str) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone()).delete_project_memory(project_id)
    }

    pub fn migrate_memory_project(
        &self,
        request: MemoryProjectMigrationRequest,
    ) -> Result<MemorySummary, String> {
        MemoryService::new(self.support_dir.clone()).migrate_project_memory(request)
    }

    pub fn update_memory_summary(
        &self,
        request: MemorySummaryUpdateRequest,
    ) -> Result<MemorySummaryRow, String> {
        MemoryService::new(self.support_dir.clone()).update_summary(request)
    }

    pub fn refresh_memory_project_profile_local(
        &self,
        project_id: &str,
    ) -> Result<MemoryProjectProfile, String> {
        let project = self
            .memory_project_infos()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        MemoryService::new(self.support_dir.clone())
            .project_profile_for_launch(&project.id, &project.name, &project.path)
            .ok_or_else(|| "Unable to generate project profile.".to_string())
    }

    pub async fn force_refresh_memory_project_profile_with_llm(
        &self,
        project_id: &str,
    ) -> Result<MemoryProjectProfileRefreshResult, String> {
        let settings = SettingsService::new(self.support_dir.clone()).ai_settings();
        let project = self
            .memory_project_infos()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        MemoryService::new(self.support_dir.clone())
            .force_refresh_project_profile_with_llm_detailed(
                &crate::memory::memory_config(&settings),
                &project,
            )
            .await
            .ok_or_else(|| "Unable to refresh project profile.".to_string())
    }
}
