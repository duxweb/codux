use super::*;
use crate::app::app_events::{
    ChildWindowUpdateKind, publish_child_window_update, publish_memory_update,
};

const MAX_AUTOMATIC_MEMORY_PROCESS_TASKS: usize = 10;

impl CoduxApp {
    pub(super) fn schedule_ai_index_progress_expiry(
        &self,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_secs(3)).await;
            let _ = this.update(cx, |app, cx| {
                if app.ai_index_progress_generation == generation {
                    app.invalidate_memory_panel(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn show_memory_progress_for_at_least(
        &mut self,
        seconds: f64,
        cx: &mut Context<Self>,
    ) {
        self.memory_progress_generation = self.memory_progress_generation.wrapping_add(1);
        self.memory_progress_visible_until = app_now_seconds() + seconds;
        let generation = self.memory_progress_generation;
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_secs_f64(seconds)).await;
            let _ = this.update(cx, |app, cx| {
                if app.memory_progress_generation == generation {
                    app.invalidate_memory_panel(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn start_memory_extraction_status_refresh(&mut self, cx: &mut Context<Self>) {
        if self.memory_extraction_status_refreshing {
            return;
        }
        self.memory_extraction_status_refreshing = true;
        let service = self.runtime_service.clone();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(300)).await;
                let result = codux_runtime::async_runtime::spawn_blocking({
                    let service = service.clone();
                    move || service.memory_extraction_status()
                })
                .await
                .map_err(|error| error.to_string())
                .and_then(|result| result);

                let should_continue = match result {
                    Ok(status) => {
                        let pending = status.pending_count.max(0);
                        let running = status.running_count.max(0);
                        let is_running = running > 0;
                        let _ = this.update(cx, |app, cx| {
                            let extraction = &mut app.state.memory_manager.extraction;
                            let changed = extraction.queued != pending
                                || extraction.running != running
                                || extraction.last_error != status.last_error
                                || app.memory_processing != is_running;
                            extraction.queued = pending;
                            extraction.running = running;
                            extraction.last_error = status.last_error.clone();
                            app.memory_processing = is_running;
                            app.memory_extraction_status_refreshing = is_running;
                            if changed {
                                app.invalidate_status_bar(cx);
                                app.invalidate_memory_panel(cx);
                            }
                        });
                        is_running
                    }
                    Err(error) => {
                        let _ = this.update(cx, |app, cx| {
                            app.memory_processing = false;
                            app.memory_extraction_status_refreshing = false;
                            app.state.memory_manager.extraction.last_error = Some(error);
                            app.invalidate_status_bar(cx);
                            app.invalidate_memory_panel(cx);
                        });
                        false
                    }
                };

                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }

    pub(super) fn process_queued_memory_extraction_async(&mut self, cx: &mut Context<Self>) {
        if self.memory_processing {
            return;
        }
        let pending = self.state.memory_manager.extraction.queued.max(0);
        let running = self.state.memory_manager.extraction.running.max(0);
        if pending == 0 && running == 0 {
            return;
        }

        self.memory_processing = true;
        self.state.memory_manager.extraction.last_error = None;
        self.runtime_trace(
            "memory",
            &format!("auto_process start pending={pending} running={running}"),
        );
        publish_memory_update();
        publish_child_window_update(ChildWindowUpdateKind::Memory);
        self.start_memory_extraction_status_refresh(cx);
        self.invalidate_status_bar(cx);
        self.invalidate_memory_panel(cx);

        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let process_result = codux_runtime::async_runtime::spawn(async move {
                service
                    .process_memory_extraction_queue_limited(MAX_AUTOMATIC_MEMORY_PROCESS_TASKS)
                    .await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.apply_memory_processing_result(process_result, cx);
            });
        })
        .detach();
    }

    pub(super) fn enqueue_automatic_memory_extraction_async(&mut self, cx: &mut Context<Self>) {
        if self.memory_processing {
            return;
        }
        if !self.runtime_service.automatic_memory_extraction_available() {
            return;
        }

        let pending = self.state.memory_manager.extraction.queued.max(0);
        let running = self.state.memory_manager.extraction.running.max(0);
        if pending > 0 || running > 0 {
            self.process_queued_memory_extraction_async(cx);
            return;
        }

        let scheduler_key = "memory_auto_extract";
        let interval = self.memory_automatic_extraction_interval_seconds();
        let policy = ScheduledWorkPolicy::new(interval, interval);
        if !self.begin_scheduled_work(scheduler_key, policy) {
            return;
        }

        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.enqueue_automatic_memory_extraction_candidates()
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.finish_scheduled_work(scheduler_key);
                match result {
                    Ok(enqueue_result) => {
                        app.state.memory_manager.extraction.queued =
                            enqueue_result.status.pending_count.max(0);
                        app.state.memory_manager.extraction.running =
                            enqueue_result.status.running_count.max(0);
                        app.state.memory_manager.extraction.last_error =
                            enqueue_result.status.last_error.clone();
                        if enqueue_result.checked_count > 0 || enqueue_result.enqueued_count > 0 {
                            app.runtime_trace(
                                "memory",
                                &format!(
                                    "auto_enqueue checked={} enqueued={} pending={} running={}",
                                    enqueue_result.checked_count,
                                    enqueue_result.enqueued_count,
                                    enqueue_result.status.pending_count,
                                    enqueue_result.status.running_count
                                ),
                            );
                            app.reload_memory_manager_snapshot();
                            publish_memory_update();
                            publish_child_window_update(ChildWindowUpdateKind::Memory);
                            app.invalidate_status_bar(cx);
                            app.invalidate_memory_panel(cx);
                        }
                        app.process_queued_memory_extraction_async(cx);
                    }
                    Err(error) => {
                        app.state.memory_manager.extraction.last_error = Some(error.clone());
                        app.runtime_trace("memory", &format!("auto_enqueue failed error={error}"));
                        app.invalidate_status_bar(cx);
                        app.invalidate_memory_panel(cx);
                    }
                }
            });
        })
        .detach();
    }

    fn memory_automatic_extraction_interval_seconds(&self) -> f64 {
        self.state
            .settings
            .memory_extraction_idle_delay_seconds
            .parse::<f64>()
            .unwrap_or(120.0)
            .max(1.0)
    }

    pub(super) fn selected_ai_session(&self) -> Option<&AISessionSummary> {
        self.selected_ai_session_id.as_deref().and_then(|id| {
            self.state
                .ai_history
                .sessions
                .iter()
                .find(|session| session.id == id)
        })
    }

    pub(super) fn normalize_selected_ai_session(&mut self) {
        let selected_still_exists = self
            .selected_ai_session_id
            .as_deref()
            .map(|id| {
                self.state
                    .ai_history
                    .sessions
                    .iter()
                    .any(|session| session.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_ai_session_id = None;
            self.state.ai_session_detail = None;
        }
        if self
            .ai_session_delete_confirm_id
            .as_deref()
            .map(|id| {
                !self
                    .state
                    .ai_history
                    .sessions
                    .iter()
                    .any(|session| session.id == id)
            })
            .unwrap_or(false)
        {
            self.ai_session_delete_confirm_id = None;
        }
    }

    pub(super) fn reload_selected_ai_session_detail(&mut self) {
        if self.state.selected_project.is_none() {
            self.state.ai_session_detail = None;
            return;
        }
        let Some(session_id) = self.selected_ai_session_id.as_deref() else {
            self.state.ai_session_detail = None;
            return;
        };
        let Some(project_path) = self.selected_worktree_path() else {
            self.state.ai_session_detail = None;
            return;
        };
        self.state.ai_session_detail = Some(
            self.runtime_service
                .reload_project_ai_session_detail(&project_path, session_id),
        );
    }

    fn remove_ai_session_confirmed(&mut self, session_id: String, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for AI session removal".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        let worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let project_request = ai_history_worktree_request(project, worktree.as_ref());
        if !self
            .state
            .ai_history
            .sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            self.status_message = "no AI session to remove".to_string();
            self.invalidate_memory_panel(cx);
            return;
        }
        match self
            .runtime_service
            .remove_indexed_ai_session(project_request, session_id.clone())
        {
            Ok(state) => {
                if let Some(summary) = ai_history_summary_from_project_state(&state) {
                    self.state.ai_history = summary;
                    self.state.refresh_ai_history_stats();
                }
                self.refresh_ai_global_history_summary();
                self.selected_ai_session_id = None;
                self.ai_session_delete_confirm_id = None;
                self.normalize_selected_ai_session();
                self.reload_selected_ai_session_detail();
                self.status_message = "selected AI session removed from index".to_string();
            }
            Err(error) => {
                self.ai_session_delete_confirm_id = None;
                self.status_message = format!("failed to remove AI session: {error}");
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn request_remove_ai_session(
        &mut self,
        session_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((session_id, session_title)) = self
            .state
            .ai_history
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .map(|session| (session.id.clone(), session.title.clone()))
        else {
            self.status_message = "AI session is no longer available".to_string();
            self.normalize_selected_ai_session();
            self.invalidate_memory_panel(cx);
            return;
        };
        self.selected_ai_session_id = Some(session_id.clone());
        self.ai_session_delete_confirm_id = Some(session_id.clone());
        self.reload_selected_ai_session_detail();
        self.status_message = "waiting for AI session removal confirmation".to_string();
        self.invalidate_memory_panel(cx);
        let title = self.text("ai.sessions.delete_title", "Delete Session");
        let message = self
            .text("ai.sessions.delete_confirm_format", "Delete %@?")
            .replace("%@", &session_title);
        let confirm_label = self.text("common.delete", "Delete");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let remove_session_id = session_id.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(true) => app.remove_ai_session_confirmed(remove_session_id, cx),
                Ok(false) => app.cancel_remove_ai_session(cx),
                Err(error) => {
                    app.ai_session_delete_confirm_id = None;
                    app.status_message =
                        format!("failed to show AI session removal confirmation: {error}");
                    app.invalidate_memory_panel(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn cancel_remove_ai_session(&mut self, cx: &mut Context<Self>) {
        self.ai_session_delete_confirm_id = None;
        self.status_message = "AI session removal cancelled".to_string();
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn refresh_ai_global_history_summary(&mut self) {
        let projects = ai_history_project_requests(&self.state.projects);
        match self
            .runtime_service
            .indexed_global_ai_history_summary(projects)
        {
            Ok(snapshot) => {
                self.state.ai_global_history =
                    normalized_global_ai_history_snapshot_to_summary(snapshot);
            }
            Err(error) => {
                self.state.ai_global_history = self.runtime_service.reload_global_ai_history();
                self.state.ai_global_history.error = Some(error);
            }
        }
    }

    pub(super) fn restore_selected_ai_session(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.selected_project.is_none() {
            self.status_message = "no selected project for AI session restore".to_string();
            self.invalidate_memory_panel(cx);
            return;
        }
        let Some(session) = self.selected_ai_session().cloned() else {
            self.status_message = "no AI session to restore".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
        let command = ai_session_restore_command(&session);
        self.restore_ai_session_in_main_split(session.title.clone(), command, window, cx);
    }

    pub(super) fn fork_ai_session_to_tool(
        &mut self,
        session_id: String,
        target_tool: AISessionForkTarget,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project for AI session".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        if !self
            .state
            .ai_history
            .sessions
            .iter()
            .any(|session| session.id == session_id)
        {
            self.status_message = "no AI session to continue".to_string();
            self.invalidate_memory_panel(cx);
            return;
        }

        self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
        self.status_message = format!("preparing {} session handoff", target_tool.display_name());
        self.invalidate_memory_panel(cx);

        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let target = target_tool;
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.fork_ai_session(AISessionForkRequest {
                    project_id: project.id,
                    project_name: project.name,
                    project_path: project.path,
                    session_id,
                    target_tool: target,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(result) => {
                    let command = ai_session_fork_command(target_tool, &result.prompt_path);
                    app.status_message =
                        format!("prepared {} session handoff", target_tool.display_name());
                    app.restore_ai_session_in_main_split_without_focus(result.title, command, cx);
                }
                Err(error) => {
                    app.status_message = format!("failed to prepare session handoff: {error}");
                    app.invalidate_memory_panel(cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn reload_memory(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.reload_memory_manager_snapshot_async(cx);
    }

    pub(super) fn process_memory_sessions_now(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.memory_processing {
            self.status_message = "memory processing is already running".to_string();
            self.invalidate_memory_panel(cx);
            return;
        }

        self.memory_processing = true;
        self.state.memory_manager.extraction.running =
            self.state.memory_manager.extraction.running.max(1);
        self.state.memory_manager.extraction.last_error = None;
        self.show_memory_progress_for_at_least(3.0, cx);
        self.status_message = "memory processing started".to_string();
        self.runtime_trace("memory", "manual_process start");
        publish_memory_update();
        publish_child_window_update(ChildWindowUpdateKind::Memory);
        self.invalidate_status_bar(cx);
        let service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let process_result = codux_runtime::async_runtime::spawn(async move {
                service.process_memory_sessions_now().await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);
            let _ = this.update(cx, |app, cx| {
                app.apply_memory_processing_result(process_result, cx);
            });
        })
        .detach();
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn apply_memory_processing_result(
        &mut self,
        result: Result<MemoryExtractionStatusSnapshot, String>,
        cx: &mut Context<Self>,
    ) {
        self.memory_processing = false;
        match result {
            Ok(status) => {
                self.show_memory_progress_for_at_least(3.0, cx);
                self.start_ai_history_refresh(false, cx);
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.normalize_selected_memory_summary();
                publish_memory_update();
                publish_child_window_update(ChildWindowUpdateKind::Memory);
                self.invalidate_status_bar(cx);
                self.status_message = format!(
                    "memory indexed · checked {} · enqueued {} · pending {}",
                    status.checked_count, status.enqueued_count, status.pending_count
                );
                self.runtime_trace(
                    "memory",
                    &format!(
                        "manual_process ok checked={} enqueued={} pending={} running={} last_error={}",
                        status.checked_count,
                        status.enqueued_count,
                        status.pending_count,
                        status.running_count,
                        status.last_error.as_deref().unwrap_or("none")
                    ),
                );
            }
            Err(error) => {
                self.runtime_trace("memory", &format!("manual_process failed error={error}"));
                self.state.memory_manager.extraction.running = 0;
                self.reload_memory_manager_snapshot();
                publish_memory_update();
                publish_child_window_update(ChildWindowUpdateKind::Memory);
                self.invalidate_status_bar(cx);
                self.status_message = format!("failed to process memory: {error}");
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn cancel_memory_extraction_queue(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.cancel_memory_extraction_queue() {
            Ok(status) => {
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.normalize_selected_memory_summary();
                self.status_message = format!(
                    "memory queue cancelled · pending {} · running {}",
                    status.pending_count, status.running_count
                );
                self.runtime_trace(
                    "memory",
                    &format!(
                        "cancel_queue ok pending={} running={} last_error={}",
                        status.pending_count,
                        status.running_count,
                        status.last_error.as_deref().unwrap_or("none")
                    ),
                );
            }
            Err(error) => {
                self.runtime_trace("memory", &format!("cancel_queue failed error={error}"));
                self.status_message = format!("failed to cancel memory queue: {error}");
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn retry_failed_memory_extraction(
        &mut self,
        task_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .retry_failed_memory_extraction(&task_id)
        {
            Ok(status) => {
                self.state.memory_manager.extraction.queued = status.pending_count.max(0);
                self.state.memory_manager.extraction.running = status.running_count.max(0);
                self.state.memory_manager.extraction.last_error = status.last_error.clone();
                self.reload_memory_manager_snapshot();
                publish_memory_update();
                publish_child_window_update(ChildWindowUpdateKind::Memory);
                self.start_memory_extraction_status_refresh(cx);
                self.process_queued_memory_extraction_async(cx);
            }
            Err(error) => {
                self.state.memory_manager.extraction.last_error = Some(error.clone());
                self.runtime_trace("memory", &format!("retry_failed failed error={error}"));
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn clear_memory_extraction_failures(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.clear_memory_extraction_failures() {
            Ok(status) => {
                self.state.memory_manager.extraction.queued = status.pending_count.max(0);
                self.state.memory_manager.extraction.running = status.running_count.max(0);
                self.state.memory_manager.extraction.last_error = status.last_error.clone();
                self.reload_memory_manager_snapshot();
                publish_memory_update();
                publish_child_window_update(ChildWindowUpdateKind::Memory);
                self.runtime_trace(
                    "memory",
                    &format!(
                        "clear_failures ok pending={} running={} last_error={}",
                        status.pending_count,
                        status.running_count,
                        status.last_error.as_deref().unwrap_or("none")
                    ),
                );
            }
            Err(error) => {
                self.state.memory_manager.extraction.last_error = Some(error.clone());
                self.runtime_trace("memory", &format!("clear_failures failed error={error}"));
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn clear_failed_memory_extraction(
        &mut self,
        task_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .clear_failed_memory_extraction(&task_id)
        {
            Ok(status) => {
                self.state.memory_manager.extraction.queued = status.pending_count.max(0);
                self.state.memory_manager.extraction.running = status.running_count.max(0);
                self.state.memory_manager.extraction.last_error = status.last_error.clone();
                self.reload_memory_manager_snapshot();
                publish_memory_update();
                publish_child_window_update(ChildWindowUpdateKind::Memory);
            }
            Err(error) => {
                self.state.memory_manager.extraction.last_error = Some(error.clone());
                self.runtime_trace("memory", &format!("clear_failed_task failed error={error}"));
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn clear_pending_memory_extraction(
        &mut self,
        task_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .clear_pending_memory_extraction(&task_id)
        {
            Ok(status) => {
                self.state.memory_manager.extraction.queued = status.pending_count.max(0);
                self.state.memory_manager.extraction.running = status.running_count.max(0);
                self.state.memory_manager.extraction.last_error = status.last_error.clone();
                self.reload_memory_manager_snapshot();
                publish_memory_update();
                publish_child_window_update(ChildWindowUpdateKind::Memory);
            }
            Err(error) => {
                self.state.memory_manager.extraction.last_error = Some(error.clone());
                self.runtime_trace(
                    "memory",
                    &format!("clear_pending_task failed error={error}"),
                );
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn select_memory_entry(
        &mut self,
        entry_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self
            .state
            .memory
            .recent_entries
            .iter()
            .chain(self.state.memory_manager.entries.iter())
            .find(|entry| entry.id == entry_id)
        else {
            self.status_message = "memory entry is no longer available".to_string();
            self.normalize_selected_memory_entry();
            self.invalidate_memory_panel(cx);
            return;
        };
        self.selected_memory_entry_id = Some(entry.id.clone());
        self.status_message = format!("selected memory: {}", entry.content);
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn archive_selected_memory_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_selected_memory_status("archived", cx);
    }

    pub(super) fn delete_selected_memory_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_id) = self.selected_memory_entry_id.clone().or_else(|| {
            self.state
                .memory_manager
                .entries
                .first()
                .map(|entry| entry.id.clone())
        }) else {
            self.status_message = "no memory entry selected".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        let project_id = self.memory_manager_project_id.as_deref();
        match self
            .runtime_service
            .delete_memory_entry(project_id, &entry_id)
        {
            Ok(memory) => {
                self.state.memory = memory;
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.status_message = "memory entry deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete memory: {error}"),
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn delete_selected_memory_summary(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(summary_id) = self.selected_memory_summary_id.clone().or_else(|| {
            self.state
                .memory_manager
                .summaries
                .first()
                .map(|summary| summary.id.clone())
        }) else {
            self.status_message = "no memory summary selected".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        let project_id = self.memory_manager_project_id.as_deref();
        match self
            .runtime_service
            .delete_memory_summary(project_id, &summary_id)
        {
            Ok(memory) => {
                self.state.memory = memory;
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_summary();
                self.status_message = "memory summary deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete memory summary: {error}"),
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn update_memory_summary_content(
        &mut self,
        summary_id: String,
        content: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let content = content.trim().to_string();
        if content.is_empty() {
            self.status_message = "memory summary content cannot be empty".to_string();
            self.invalidate_memory_panel(cx);
            return;
        }
        match self
            .runtime_service
            .update_memory_summary(MemorySummaryUpdateRequest {
                summary_id: summary_id.clone(),
                content,
                max_versions: self
                    .state
                    .settings
                    .memory_max_summary_versions
                    .parse::<i32>()
                    .ok(),
            }) {
            Ok(_) => {
                self.selected_memory_summary_id = Some(summary_id);
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_summary();
                self.status_message = "memory summary updated".to_string();
            }
            Err(error) => self.status_message = format!("failed to update memory summary: {error}"),
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn delete_selected_memory_project_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self.memory_manager_project_id.clone() else {
            self.status_message = "no selected project profile".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        match self
            .runtime_service
            .delete_memory_project_profile(&project_id)
        {
            Ok(memory) => {
                self.state.memory = memory;
                self.reload_memory_manager_snapshot();
                self.status_message = "memory project profile deleted".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to delete project profile: {error}")
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn delete_selected_memory_project(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self.memory_manager_project_id.clone() else {
            self.status_message = "no selected project memory".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        match self.runtime_service.delete_memory_project(&project_id) {
            Ok(memory) => {
                self.state.memory = memory;
                self.selected_memory_entry_id = None;
                self.selected_memory_summary_id = None;
                self.reload_memory_manager_snapshot();
                self.status_message = "project memory deleted".to_string();
            }
            Err(error) => self.status_message = format!("failed to delete project memory: {error}"),
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn migrate_selected_memory_project_to(
        &mut self,
        to_project_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(from_project_id) = self.memory_manager_project_id.clone() else {
            self.status_message = "no selected project memory to migrate".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        match self
            .runtime_service
            .migrate_memory_project(MemoryProjectMigrationRequest {
                from_project_id: from_project_id.clone(),
                to_project_id: to_project_id.clone(),
                overwrite: false,
            }) {
            Ok(memory) => {
                self.state.memory = memory;
                self.selected_memory_entry_id = None;
                self.selected_memory_summary_id = None;
                self.memory_manager_scope = "project".to_string();
                self.memory_manager_project_id = Some(to_project_id.clone());
                self.reload_memory_manager_snapshot();
                self.status_message =
                    format!("project memory migrated from {from_project_id} to {to_project_id}");
            }
            Err(error) => {
                self.status_message = format!("failed to migrate project memory: {error}")
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn refresh_selected_memory_project_profile(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self.memory_manager_project_id.clone() else {
            self.status_message = "no selected project profile".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        let service = self.runtime_service.clone();
        self.memory_project_profile_refreshing = true;
        self.status_message = "memory project profile refresh started".to_string();
        self.runtime_trace(
            "memory",
            &format!("project_profile_refresh start project={project_id}"),
        );
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn(async move {
                service
                    .force_refresh_memory_project_profile_with_llm(&project_id)
                    .await
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_memory_project_profile_refresh_result(result, cx);
            });
        })
        .detach();
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn apply_memory_project_profile_refresh_result(
        &mut self,
        result: Result<MemoryProjectProfileRefreshResult, String>,
        cx: &mut Context<Self>,
    ) {
        self.memory_project_profile_refreshing = false;
        match result {
            Ok(result) => {
                self.state.memory = self.runtime_service.reload_memory(
                    self.state
                        .selected_project
                        .as_ref()
                        .map(|project| project.id.as_str()),
                );
                self.reload_memory_manager_snapshot();
                self.status_message = if result.used_llm {
                    self.runtime_trace(
                        "memory",
                        &format!(
                            "project_profile_refresh ok source=llm chars={}",
                            result.profile.content.chars().count()
                        ),
                    );
                    format!(
                        "memory project profile refreshed with LLM · {} chars",
                        result.profile.content.chars().count()
                    )
                } else {
                    let fallback_reason = result.fallback_reason.clone().unwrap_or_else(|| {
                        "Project profile was generated from local repository scan.".to_string()
                    });
                    self.runtime_trace(
                        "memory",
                        &format!(
                            "project_profile_refresh local_fallback chars={} reason={fallback_reason}",
                            result.profile.content.chars().count()
                        ),
                    );
                    self.show_memory_alert(
                        "memory.manager.project_profile.local_fallback.title",
                        "Project profile used local scan",
                        fallback_reason.clone(),
                        cx,
                    );
                    format!(
                        "memory project profile refreshed locally · {} chars{}",
                        result.profile.content.chars().count(),
                        result
                            .fallback_reason
                            .as_ref()
                            .map(|reason| format!(" · {reason}"))
                            .unwrap_or_default()
                    )
                };
            }
            Err(error) => {
                self.runtime_trace(
                    "memory",
                    &format!("project_profile_refresh failed error={error}"),
                );
                self.show_memory_alert(
                    "memory.manager.project_profile.refresh_failed",
                    "Project profile refresh failed",
                    error.clone(),
                    cx,
                );
                self.status_message = format!("failed to refresh project profile: {error}")
            }
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn set_memory_manager_tab(&mut self, tab: MemoryManagerTab, cx: &mut Context<Self>) {
        self.memory_manager_tab = tab;
        self.status_message = format!("memory manager tab: {}", tab.as_str());
        self.reload_memory_manager_snapshot_async(cx);
    }

    pub(super) fn select_memory_manager_queue(&mut self, cx: &mut Context<Self>) {
        self.memory_manager_scope = "user".to_string();
        self.memory_manager_project_id = None;
        self.memory_manager_tab = MemoryManagerTab::Queue;
        self.selected_memory_entry_id = None;
        self.selected_memory_summary_id = None;
        self.status_message = "memory manager tab: queue".to_string();
        self.reload_memory_manager_snapshot_async(cx);
    }

    pub(super) fn select_memory_manager_failed_records(&mut self, cx: &mut Context<Self>) {
        self.memory_manager_scope = "user".to_string();
        self.memory_manager_project_id = None;
        self.memory_manager_tab = MemoryManagerTab::Failed;
        self.selected_memory_entry_id = None;
        self.selected_memory_summary_id = None;
        self.status_message = "memory manager tab: failed".to_string();
        self.reload_memory_manager_snapshot_async(cx);
    }

    pub(super) fn select_memory_manager_target(
        &mut self,
        scope: String,
        project_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        self.memory_manager_scope = if scope == "user" {
            "user".to_string()
        } else {
            "project".to_string()
        };
        self.memory_manager_project_id = if self.memory_manager_scope == "project" {
            project_id
        } else {
            None
        };
        self.selected_memory_entry_id = None;
        self.selected_memory_summary_id = None;
        if self.memory_manager_tab == MemoryManagerTab::Queue
            || self.memory_manager_tab == MemoryManagerTab::Failed
        {
            self.memory_manager_tab = MemoryManagerTab::Summary;
        }
        self.reload_memory_manager_snapshot_async(cx);
    }

    fn show_memory_alert(
        &self,
        title_key: &'static str,
        title_fallback: &'static str,
        message: String,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title = translate(&locale, title_key, title_fallback);
        let button_label = translate(&locale, "common.ok", "OK");
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
            let dialog_error = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_alert_dialog(LocalizedAlertDialogRequest {
                    title,
                    message,
                    button_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result)
            .err();

            if let Some(dialog_error) = dialog_error {
                let _ = this.update(cx, |app, cx| {
                    app.status_message = format!("failed to show memory alert: {dialog_error}");
                    app.invalidate_memory_panel(cx);
                });
            }
        })
        .detach();
    }

    pub(super) fn update_selected_memory_status(&mut self, status: &str, cx: &mut Context<Self>) {
        let Some(entry_id) = self.selected_memory_entry_id.clone().or_else(|| {
            self.state
                .memory
                .recent_entries
                .first()
                .map(|entry| entry.id.clone())
        }) else {
            self.status_message = "no memory entry selected".to_string();
            self.invalidate_memory_panel(cx);
            return;
        };
        let project_id = self.memory_manager_project_id.as_deref();
        let result = if status == "archived" {
            self.runtime_service
                .archive_memory_entry(project_id, &entry_id)
        } else {
            self.runtime_service
                .restore_memory_entry(project_id, &entry_id)
        };
        match result {
            Ok(memory) => {
                self.state.memory = memory;
                self.selected_memory_entry_id = Some(entry_id);
                self.reload_memory_manager_snapshot();
                self.normalize_selected_memory_entry();
                self.status_message = format!("memory entry set to {status}");
            }
            Err(error) => self.status_message = format!("failed to update memory: {error}"),
        }
        self.invalidate_memory_panel(cx);
    }

    pub(super) fn normalize_selected_memory_entry(&mut self) {
        let selected_still_exists = self
            .selected_memory_entry_id
            .as_deref()
            .map(|id| {
                self.state
                    .memory
                    .recent_entries
                    .iter()
                    .any(|entry| entry.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_memory_entry_id = self
                .state
                .memory_manager
                .entries
                .first()
                .map(|entry| entry.id.clone());
        }
    }

    pub(super) fn normalize_selected_memory_summary(&mut self) {
        let selected_still_exists = self
            .selected_memory_summary_id
            .as_deref()
            .map(|id| {
                self.state
                    .memory_manager
                    .summaries
                    .iter()
                    .any(|summary| summary.id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_memory_summary_id = self
                .state
                .memory_manager
                .summaries
                .first()
                .map(|summary| summary.id.clone());
        }
    }

    pub(super) fn reload_memory_manager_snapshot(&mut self) {
        self.state.memory_manager = self.runtime_service.reload_memory_manager(
            &self.state.projects,
            &self.memory_manager_scope,
            self.memory_manager_project_id.as_deref(),
            self.memory_manager_tab.as_str(),
        );
        self.normalize_memory_status_seen_failures();
    }

    pub(super) fn reload_memory_manager_snapshot_async(&mut self, cx: &mut Context<Self>) {
        self.memory_manager_refresh_generation =
            self.memory_manager_refresh_generation.wrapping_add(1);
        let generation = self.memory_manager_refresh_generation;
        self.memory_manager_refreshing = true;
        let service = self.runtime_service.clone();
        let projects = self.state.projects.clone();
        let scope = self.memory_manager_scope.clone();
        let project_id = self.memory_manager_project_id.clone();
        let tab = self.memory_manager_tab.as_str().to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let snapshot = codux_runtime::async_runtime::spawn_blocking(move || {
                service.reload_memory_manager(&projects, &scope, project_id.as_deref(), &tab)
            })
            .await
            .map_err(|error| error.to_string());

            let _ = this.update(cx, |app, cx| {
                if app.memory_manager_refresh_generation != generation {
                    return;
                }
                app.memory_manager_refreshing = false;
                match snapshot {
                    Ok(snapshot) => {
                        app.state.memory_manager = snapshot;
                        app.normalize_memory_status_seen_failures();
                        app.normalize_selected_memory_entry();
                        app.normalize_selected_memory_summary();
                        let is_running = app.state.memory_manager.extraction.running > 0;
                        app.memory_processing = is_running;
                        if is_running {
                            app.start_memory_extraction_status_refresh(cx);
                        }
                        app.status_message = "memory summary reloaded".to_string();
                    }
                    Err(error) => {
                        app.status_message = format!("failed to reload memory manager: {error}");
                    }
                }
                app.invalidate_memory_panel(cx);
            });
        })
        .detach();
        self.invalidate_memory_panel(cx);
    }

    fn normalize_memory_status_seen_failures(&mut self) {
        let failed = self.state.memory_manager.extraction.failed.max(0);
        if self.memory_status_seen_failed_count > failed {
            self.memory_status_seen_failed_count = failed;
        }
    }

    pub(super) fn normalize_selected_runtime_session(&mut self) {
        let selected_still_exists = self
            .selected_runtime_terminal_id
            .as_deref()
            .map(|id| {
                self.state
                    .runtime_events
                    .sessions
                    .iter()
                    .any(|session| session.terminal_id == id)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_runtime_terminal_id = self
                .state
                .runtime_events
                .sessions
                .first()
                .map(|session| session.terminal_id.clone());
        }
    }
}
