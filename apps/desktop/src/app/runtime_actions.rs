use super::*;

impl CoduxApp {
    pub(crate) fn start_settings_remote_snapshot_loop(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Settings {
            return;
        }
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            loop {
                timer.timer(Duration::from_millis(500)).await;
                let service = match this.update(cx, |app, _cx| {
                    (app.window_mode == AppWindowMode::Settings)
                        .then(|| app.runtime_service.clone())
                }) {
                    Ok(Some(service)) => service,
                    Ok(None) => return,
                    Err(_) => return,
                };
                let remote = match codux_runtime::async_runtime::spawn_blocking(move || {
                    service.reload_remote()
                })
                .await
                {
                    Ok(remote) => remote,
                    Err(_) => return,
                };
                if this
                    .update(cx, |app, cx| {
                        if app.window_mode != AppWindowMode::Settings {
                            return;
                        }
                        if app.state.remote != remote {
                            // The pairing sheet's own 1s poll watches
                            // state.remote.pairing to notice a completed pairing
                            // and close the sheet. This 500ms snapshot loop can
                            // null that pairing first (auto-confirm clears it in
                            // the runtime snapshot), which makes the poll exit
                            // early and strands the sheet on its placeholder
                            // spinner. Close it here when the pairing we were
                            // showing disappears.
                            let had_pairing = app.state.remote.pairing.is_some();
                            app.state.remote = remote;
                            if had_pairing
                                && app.state.remote.pairing.is_none()
                                && app.remote_pairing_sheet_open
                            {
                                app.remote_pairing_sheet_open = false;
                                app.remote_pairing_creating = false;
                            }
                            app.normalize_selected_remote_device();
                            if app.remote_reconnecting && app.state.remote.status != "connecting" {
                                app.remote_reconnecting = false;
                            }
                            app.invalidate_remote_panel(cx);
                        }
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    }

    pub(crate) fn start_runtime_event_loop(&mut self, cx: &mut Context<Self>) {
        if self.window_mode != AppWindowMode::Main {
            return;
        }
        self.start_runtime_ready_initialization(cx);
        self.sync_desktop_pet_window(false, cx);
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut ticks = 0_u64;
            let mut performance_ticks_until_refresh = 0_u64;
            let mut last_runtime_queue_busy = false;
            loop {
                timer.timer(Duration::from_millis(200)).await;
                ticks = ticks.wrapping_add(1);
                performance_ticks_until_refresh = performance_ticks_until_refresh.saturating_sub(1);
                let include_slow_tick = ticks % 5 == 0;
                let include_project_activity_tick = ticks % 75 == 0;
                let include_runtime_refresh_tick = ticks % 150 == 0;
                let runtime_queue_status = codux_runtime::async_runtime::blocking_queue_status();
                let runtime_queue_busy =
                    runtime_queue_status.queued > 0 || runtime_queue_status.running > 0;
                let runtime_queue_busy_changed = runtime_queue_busy != last_runtime_queue_busy;
                last_runtime_queue_busy = runtime_queue_busy;

                if this
                    .update(cx, |app, cx| {
                        if app.window_mode != AppWindowMode::Main {
                            return;
                        }
                        let performance_changed = app.apply_pending_performance_refresh();
                        if performance_ticks_until_refresh == 0 && app.state.settings.developer_hud
                        {
                            performance_ticks_until_refresh =
                                app.performance_refresh_interval_seconds().saturating_mul(5);
                            app.spawn_performance_refresh(cx);
                        }
                        if include_runtime_refresh_tick {
                            app.spawn_runtime_scheduled_refresh(cx);
                        }
                        let today_level_changed = app.refresh_today_level_after_day_change();
                        let result = if include_slow_tick {
                            app.apply_runtime_activity_tick(
                                true,
                                true,
                                include_project_activity_tick,
                                cx,
                            )
                        } else {
                            app.apply_ai_runtime_activity_tick(cx)
                        };
                        if include_slow_tick {
                            app.enqueue_automatic_memory_extraction_async(cx);
                            app.refresh_remote_link_states(cx);
                            app.reconcile_remote_terminal_bindings(cx);
                            app.detect_project_drive_recovery(cx);
                        }
                        if result.pet_update_events > 0 {
                            app.sync_desktop_pet_window(false, cx);
                        }
                        if performance_changed || runtime_queue_busy_changed {
                            app.invalidate_status_bar(cx);
                        }
                        if today_level_changed {
                            app.invalidate_ui(
                                cx,
                                [
                                    UiRegion::ProjectColumn,
                                    UiRegion::TaskColumn,
                                    UiRegion::WorkspaceChrome,
                                    UiRegion::StatusBar,
                                ],
                            );
                        }
                        if result.changed {
                            app.invalidate_for_runtime_activity(&result, cx);
                            cx.refresh_windows();
                        }
                    })
                    .is_err()
                {
                    return;
                }
            }
        })
        .detach();
    }

    fn start_runtime_ready_initialization(&mut self, cx: &mut Context<Self>) {
        if self.runtime_ready {
            return;
        }
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                let started_at = std::time::Instant::now();
                runtime_service
                    .runtime_trace_frontend("startup", "runtime background initialization start");
                let tool_permissions = runtime_service.sync_tool_permissions();
                let ai_runtime_status = match runtime_service.start_ai_runtime_event_processing() {
                    Ok(_) => {
                        let snapshot = runtime_service.ai_runtime_state_snapshot();
                        let summary =
                            runtime_service.summarize_ai_runtime_state_snapshot(&snapshot);
                        Ok((tool_permissions, snapshot, summary))
                    }
                    Err(error) => Err((tool_permissions, error)),
                };
                runtime_service.runtime_trace_frontend(
                    "startup",
                    &format!(
                        "runtime background initialization finish elapsed_ms={}",
                        started_at.elapsed().as_millis()
                    ),
                );
                ai_runtime_status
            })
            .await;

            let _ = this.update(cx, |app, cx| {
                app.runtime_ready = true;
                match result {
                    Ok(Ok((tool_permissions, snapshot, summary))) => {
                        app.state.tool_permissions = tool_permissions;
                        app.state.ai_runtime_state = summary;
                        let _ = app.sync_pane_agent_lifecycle();
                        app.state.refresh_ai_history_stats();
                        app.status_message = format!(
                            "runtime ready · {} session{}",
                            snapshot.sessions.len(),
                            if snapshot.sessions.len() == 1 {
                                ""
                            } else {
                                "s"
                            }
                        );
                    }
                    Ok(Err((tool_permissions, error))) => {
                        app.state.tool_permissions = tool_permissions;
                        app.status_message =
                            format!("runtime ready with AI runtime error: {error}");
                    }
                    Err(error) => {
                        app.status_message =
                            format!("runtime ready with initialization join error: {error}");
                    }
                }
                app.invalidate_ui(
                    cx,
                    [
                        UiRegion::StatusBar,
                        UiRegion::WorkspaceAssistant,
                        UiRegion::AIStatsSidebar,
                    ],
                );
                app.refresh_window_runtime_data(cx);
                app.maybe_prompt_github_star(cx);
            });
        })
        .detach();
    }

    pub(crate) fn start_ai_history_refresh(&mut self, show_progress: bool, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to refresh".to_string();
            return;
        };

        if show_progress {
            self.ai_history_refreshing = true;
            self.ai_index_progress_generation = self.ai_index_progress_generation.wrapping_add(1);
            self.ai_index_progress_visible_until = app_now_seconds() + 3.0;
            self.state.ai_history.is_loading = true;
            self.state.ai_history.queued = true;
            self.state.ai_history.progress = Some(0.0);
            self.state.ai_history.detail = "queued".to_string();
            self.state.refresh_ai_history_stats();
            self.invalidate_ui(
                cx,
                [
                    UiRegion::WorkspaceAssistant,
                    UiRegion::AIStatsSidebar,
                    UiRegion::StatusBar,
                ],
            );
            self.schedule_ai_index_progress_expiry(self.ai_index_progress_generation, cx);
        }

        let worktree = super::ai_runtime_status::selected_worktree_info(&self.state);
        let request = ai_history_worktree_request(&project, worktree.as_ref());
        match self
            .runtime_service
            .refresh_indexed_project_ai_history(request)
        {
            Ok(()) => {
                self.ai_history_active_index_count =
                    self.runtime_service.active_ai_history_index_count();
                self.status_message = format!("AI history indexing queued for {}", project.name);
            }
            Err(error) => {
                self.ai_history_refreshing = false;
                self.state.ai_history.is_loading = false;
                self.state.ai_history.queued = false;
                self.ai_history_active_index_count =
                    self.runtime_service.active_ai_history_index_count();
                self.state.ai_history.error = Some(error.clone());
                self.state.refresh_ai_history_stats();
                self.status_message = format!("failed to queue AI history indexing: {error}");
            }
        }
    }
}

impl CoduxApp {
    pub(in crate::app) fn invalidate_for_runtime_activity(
        &mut self,
        result: &RuntimeActivityTickResult,
        cx: &mut Context<Self>,
    ) {
        if result.project_events > 0 || result.ai_activity_changed {
            self.invalidate_ui(
                cx,
                [
                    UiRegion::ProjectColumn,
                    UiRegion::TaskColumn,
                    UiRegion::StatusBar,
                ],
            );
        }
        if result.file_events > 0 {
            self.invalidate_ui(cx, [UiRegion::FileSidebar, UiRegion::WorkspaceBody]);
        }
        if result.ai_history_events > 0 {
            self.invalidate_ui(
                cx,
                [
                    UiRegion::TaskColumn,
                    UiRegion::WorkspaceChrome,
                    UiRegion::WorkspaceAssistant,
                    UiRegion::StatusBar,
                ],
            );
            if self.workspace_view == WorkspaceView::Stats {
                self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
            }
        }
        if result.ai_runtime_events > 0 && self.workspace_view == WorkspaceView::Terminal {
            self.invalidate_ui_region(cx, UiRegion::WorkspaceBody);
        }
        if result.memory_events > 0 {
            self.invalidate_ui(cx, [UiRegion::WorkspaceAssistant, UiRegion::StatusBar]);
        }
        if result.pet_events > 0 || result.pet_update_events > 0 {
            self.invalidate_ui(cx, [UiRegion::WorkspaceChrome, UiRegion::StatusBar]);
        }
        if result.dock_badge_count.is_some() {
            self.invalidate_status_bar(cx);
        }
    }
}
