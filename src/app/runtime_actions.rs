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
                        if app.state.remote.status != remote.status
                            || app.state.remote.message != remote.message
                            || app.state.remote.devices != remote.devices
                            || app.state.remote.online_devices != remote.online_devices
                            || app.state.remote.pending_pairings != remote.pending_pairings
                            || app.state.remote.pairing.is_some() != remote.pairing.is_some()
                        {
                            app.state.remote = remote;
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
        self.sync_desktop_pet_window(false, cx);
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let mut ticks = 0_u64;
            let mut performance_ticks_until_refresh = 0_u64;
            loop {
                timer.timer(Duration::from_millis(200)).await;
                ticks = ticks.wrapping_add(1);
                performance_ticks_until_refresh = performance_ticks_until_refresh.saturating_sub(1);
                let include_slow_tick = ticks % 5 == 0;
                let include_project_activity_tick = ticks % 75 == 0;
                let include_runtime_refresh_tick = ticks % 150 == 0;

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
                        if result.pet_update_events > 0 {
                            app.sync_desktop_pet_window(false, cx);
                        }
                        if performance_changed {
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

    pub(super) fn refresh_runtime_activity_state(&mut self, poll_live_ai: bool) {
        self.state.runtime_activity = self.runtime_service.reload_runtime_activity();
        self.state.runtime_events = self.runtime_service.reload_runtime_events();
        let ssh_event = current_ssh_update_event();
        if ssh_event.revision > self.ssh_seen_revision {
            self.ssh_seen_revision = ssh_event.revision;
            self.state.ssh = self.runtime_service.reload_ssh(self.runtime.root.clone());
            self.normalize_selected_ssh_profile();
        }
        let ai_snapshot = if poll_live_ai {
            self.runtime_service
                .poll_ai_runtime_state()
                .unwrap_or_else(|_| self.runtime_service.ai_runtime_state_snapshot())
        } else {
            self.runtime_service.ai_runtime_state_snapshot()
        };
        self.state.ai_runtime_state = self
            .runtime_service
            .summarize_ai_runtime_state_snapshot(&ai_snapshot);
        self.normalize_selected_runtime_session();
    }

    pub(crate) fn start_ai_history_refresh(&mut self, show_progress: bool, cx: &mut Context<Self>) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to refresh".to_string();
            return;
        };

        if show_progress {
            self.ai_index_progress_generation = self.ai_index_progress_generation.wrapping_add(1);
            self.ai_index_progress_visible_until = app_now_seconds() + 3.0;
            self.state.ai_history.is_loading = true;
            self.state.ai_history.queued = true;
            self.state.ai_history.progress = Some(0.0);
            self.state.ai_history.detail = "queued".to_string();
            self.save_current_project_view_state();
            self.invalidate_task_column(cx);
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
                self.state.ai_history.is_loading = false;
                self.state.ai_history.queued = false;
                self.ai_history_active_index_count =
                    self.runtime_service.active_ai_history_index_count();
                self.state.ai_history.error = Some(error.clone());
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
        if result.project_events > 0 || result.ai_events > 0 || result.ai_activity_changed {
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
