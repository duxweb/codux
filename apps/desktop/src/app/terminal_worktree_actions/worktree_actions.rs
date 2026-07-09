use super::*;

const WORKTREE_TERMINAL_CLOSE_TIMEOUT: Duration = Duration::from_secs(10);

struct WorktreeTerminalCleanup {
    terminal_manager: Arc<TerminalManager>,
    local_terminal_ids: Vec<String>,
    remote_targets: Vec<crate::terminal::RemoteTerminalCloseTarget>,
}

impl WorktreeTerminalCleanup {
    fn close(self) -> Result<bool, String> {
        for target in self.remote_targets {
            target.close()?;
        }
        let mut closed_local = false;
        for terminal_id in self.local_terminal_ids {
            closed_local |= self
                .terminal_manager
                .kill_and_wait_if_present(&terminal_id, WORKTREE_TERMINAL_CLOSE_TIMEOUT)
                .map_err(|error| error.to_string())?;
        }
        Ok(closed_local)
    }
}

struct DetachedWorktreeTerminals {
    lifecycle_removed: bool,
}

impl CoduxApp {
    pub(in crate::app) fn open_worktree_creator_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to create worktree".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        let locale = locale_from_language_setting(&self.state.settings.language);
        let default_base_branch = self.default_worktree_base_branch();
        let default_name = default_worktree_name();
        let parent_main_window = cx.entity().downgrade();

        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::WorktreeCreator,
                title: SharedString::from(translate(
                    &locale,
                    "worktree.create.title",
                    "New Worktree",
                )),
                size: size(px(420.0), px(300.0)),
                min_size: size(px(360.0), px(260.0)),
                already_open_message: "worktree creator already opened",
                opened_message: "worktree creator opened",
                failed_prefix: "failed to open worktree creator",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::WorktreeCreator;
                app.status_message = "worktree creator ready".to_string();
                app.worktree_creator_project_id = Some(project.id.clone());
                app.worktree_creator_project_name = project.name.clone();
                app.worktree_creator_project_path = project.path.clone();
                app.worktree_creator_base_branch = default_base_branch;
                app.worktree_creator_name = default_name;
                app.parent_main_window = Some(parent_main_window);
                app
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_status_bar(cx);
    }

    fn default_worktree_base_branch(&self) -> String {
        self.state
            .git
            .branches
            .iter()
            .find(|branch| branch.is_current)
            .or_else(|| self.state.git.branches.first())
            .map(|branch| branch.name.clone())
            .filter(|branch| !branch.trim().is_empty())
            .or_else(|| {
                super::ai_runtime_status::selected_worktree_info(&self.state)
                    .map(|worktree| worktree.branch)
            })
            .filter(|branch| !branch.trim().is_empty())
            .unwrap_or_else(|| self.state.git.branch.clone())
    }

    pub(in crate::app) fn submit_worktree_creator(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.worktree_creator_submitting {
            return;
        }
        let Some(project_id) = self.worktree_creator_project_id.clone() else {
            self.worktree_creator_error = Some("No selected project.".to_string());
            cx.notify();
            return;
        };
        let project_path = self.worktree_creator_project_path.clone();
        let branch_name = self.worktree_creator_name.trim().to_string();
        let base_branch = self.worktree_creator_base_branch.trim().to_string();
        if branch_name.is_empty() {
            self.worktree_creator_error =
                Some(self.text("worktree.branch.empty", "Branch name cannot be empty."));
            cx.notify();
            return;
        }
        if base_branch.is_empty() {
            self.worktree_creator_error = Some(self.text(
                "worktree.merge.base_missing",
                "This worktree has no base branch.",
            ));
            cx.notify();
            return;
        }

        self.worktree_creator_submitting = true;
        self.worktree_creator_error = None;
        if let Some(parent) = self.parent_main_window.clone() {
            let _ = parent.update(cx, |app, cx| {
                app.task_column_refreshing = true;
                app.invalidate_task_column(cx);
            });
        }
        cx.notify();

        let service = self.runtime_service.clone();
        let parent_service = self.runtime_service.clone();
        let parent_main_window = self.parent_main_window.clone();
        let title = self.text("worktree.create.title", "New Worktree");
        let button_label = self.text("common.ok", "OK");
        cx.spawn(async move |_: gpui::WeakEntity<Self>, _cx| {
            let worker_branch_name = branch_name.clone();
            let worker_project_id = project_id.clone();
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.create_worktree_from_request(
                    codux_runtime::worktree::WorktreeCreateRequest {
                        project_id: worker_project_id,
                        project_path: worker_project_path,
                        base_branch: Some(base_branch),
                        branch_name: worker_branch_name.clone(),
                        task_title: Some(worker_branch_name),
                    },
                )
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            match result {
                Ok(snapshot) => {
                    publish_child_window_update(ChildWindowUpdateKind::Worktree);
                    if let Some(parent) = parent_main_window.clone() {
                        let _ = parent.update(_cx, |app, cx| {
                            app.apply_worktree_creator_snapshot(snapshot, cx);
                        });
                    }
                }
                Err(error) => {
                    if let Some(parent) = parent_main_window.clone() {
                        let _ = parent.update(_cx, |app, cx| {
                            app.task_column_refreshing = false;
                            app.invalidate_task_column(cx);
                        });
                    }
                    let message = error.clone();
                    let alert_service = parent_service.clone();
                    let alert_title = title.clone();
                    let alert_button = button_label.clone();
                    let _ = codux_runtime::async_runtime::spawn_blocking(move || {
                        alert_service.localized_alert_dialog(LocalizedAlertDialogRequest {
                            title: alert_title,
                            message,
                            button_label: alert_button,
                        })
                    })
                    .await;
                }
            }
        })
        .detach();
        window.remove_window();
    }

    fn apply_worktree_creator_snapshot(
        &mut self,
        snapshot: WorktreeSnapshot,
        cx: &mut Context<Self>,
    ) {
        self.task_column_refreshing = false;
        if self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| project.id == snapshot.project_id)
        {
            self.sync_terminal_state_for_project_switch();

            self.state.worktrees = WorktreeSummary {
                available: true,
                selected_worktree_id: Some(snapshot.selected_worktree_id.clone()),
                worktrees: snapshot
                    .worktrees
                    .into_iter()
                    .map(|worktree| WorktreeInfo {
                        id: worktree.id,
                        project_id: worktree.project_id,
                        name: worktree.name,
                        branch: worktree.branch,
                        path: worktree.path.clone(),
                        status: worktree.status,
                        is_default: worktree.is_default,
                        exists: Path::new(&worktree.path).exists(),
                        git_summary: worktree.git_summary,
                    })
                    .collect(),
                tasks: snapshot
                    .tasks
                    .into_iter()
                    .map(|task| WorktreeTaskInfo {
                        worktree_id: task.worktree_id,
                        title: task.title,
                        base_branch: task.base_branch,
                        status: task.status,
                    })
                    .collect(),
                active_git: self.state.worktrees.active_git.clone(),
                error: snapshot.error,
            };
            if self
                .state
                .worktrees
                .worktrees
                .iter()
                .any(|worktree| worktree.id == snapshot.selected_worktree_id)
            {
                self.state.worktrees.selected_worktree_id = Some(snapshot.selected_worktree_id);
            }
            self.reset_current_worktree_ui_state(cx);
            self.file_editor_states.clear();
            self.file_editor_state_lru.clear();
            self.file_editor_loading_states.clear();
            self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
            let generation = self.project_switch_generation;
            self.apply_terminal_layout_from_summary(
                TerminalLayoutSummary::default(),
                TerminalRuntimeSummary::default(),
                cx,
            );
            self.persist_current_terminal_layout();
            self.spawn_worktree_sidebar_load(generation, cx);
            self.invalidate_task_column(cx);
            self.invalidate_worktree_context(cx);
            self.refresh_git_panel_state_async(cx);
        } else {
            self.invalidate_task_column(cx);
        }
    }

    pub(in crate::app) fn open_worktree_folder(&mut self, path: String, cx: &mut Context<Self>) {
        let path = path.trim().to_string();
        if path.is_empty() {
            self.status_message = "no worktree folder to open".to_string();
            self.invalidate_status_bar(cx);
            return;
        }

        match self.runtime_service.project_reveal_in_file_manager(&path) {
            Ok(()) => self.status_message = format!("opened worktree folder: {path}"),
            Err(error) => {
                let title = "Open Worktree Folder Failed".to_string();
                self.status_message = title.clone();
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn remove_worktree_by_id(
        &mut self,
        worktree_id: String,
        remove_branch: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to remove worktree".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        let Some(worktree) = self
            .state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
        else {
            self.status_message = "worktree not found".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        if worktree.is_default {
            self.status_message = self.text(
                "worktree.default.remove_denied",
                "The default worktree cannot be removed.",
            );
            self.invalidate_status_bar(cx);
            return;
        }

        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let cleanup = self.terminal_cleanup_for_worktree(worktree_id.as_str());
        let service = self.runtime_service.clone();
        self.status_message = self.text("worktree.remove.running", "Removing worktree.");
        self.invalidate_worktree_context(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            // Deleting the worktree directory can take a while; keep it off
            // the UI thread (issue #57).
            let result = codux_runtime::async_runtime::spawn_blocking({
                let project_id = project_id.clone();
                let project_path = project_path.clone();
                let worktree_id = worktree_id.clone();
                move || {
                    let closed_local_terminal = cleanup.close()?;
                    if closed_local_terminal {
                        service.broadcast_remote_terminal_list();
                    }
                    let summary = service.remove_worktree(
                        &project_id,
                        &project_path,
                        &worktree_id,
                        remove_branch,
                    )?;
                    let storage_key = worktree_terminal_storage_key(&WorktreeScopeKey {
                        project_id: project_id.clone(),
                        worktree_id: worktree_id.clone(),
                    });
                    if let Err(error) = service.delete_terminal_layout(&storage_key) {
                        codux_runtime::runtime_trace::runtime_trace(
                            "worktree-remove",
                            &format!(
                                "delete_terminal_layout_failed key={storage_key} error={error}"
                            ),
                        );
                    }
                    Ok(summary)
                }
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(summary) => {
                        let previous_selected_worktree_id =
                            app.state.worktrees.selected_worktree_id.clone();
                        let selected_matches =
                            app.state.selected_project.as_ref().is_some_and(|project| {
                                project.id == project_id && project.path == project_path
                            });
                        if selected_matches {
                            app.state.worktrees = summary;
                            let detached = app
                                .detach_managed_terminals_for_worktree(&project_id, &worktree_id);
                            if detached.lifecycle_removed {
                                app.sync_project_lifecycle_state(cx);
                                app.invalidate_task_column(cx);
                            }
                            let next_selected_worktree_id =
                                app.state.worktrees.selected_worktree_id.clone();
                            if previous_selected_worktree_id.as_deref()
                                == Some(worktree_id.as_str())
                                && let Some(next_worktree_id) = next_selected_worktree_id
                                && next_worktree_id != worktree_id
                            {
                                app.load_selected_worktree_context(next_worktree_id, None, cx);
                            }
                        }
                        app.status_message =
                            app.text("worktree.remove.success", "Removed worktree.");
                    }
                    Err(error) => {
                        let title = app.text("worktree.remove.title", "Remove Worktree");
                        app.status_message = title.clone();
                        app.show_system_error_alert(title, error, cx);
                    }
                }
                app.invalidate_worktree_context(cx);
            });
        })
        .detach();
    }

    fn load_selected_worktree_context(
        &mut self,
        worktree_id: String,
        status_message: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            return;
        };
        self.terminal_layout_loading = true;
        self.selected_ai_session_id = None;
        self.state.ai_history = AIHistorySummary {
            is_loading: true,
            detail: "loading".to_string(),
            ..AIHistorySummary::default()
        };
        self.state.refresh_ai_history_stats();
        self.state.ai_session_detail = None;
        self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
        let generation = self.project_switch_generation;
        if let Some(status_message) = status_message {
            self.status_message = status_message;
        }
        let Some(key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        let storage_key = worktree_terminal_storage_key(&key);
        let terminal_layout = self
            .runtime_service
            .reload_terminal_layout(Some(&storage_key));
        self.runtime_trace(
            "terminal-layout",
            &format!(
                "select_sync_layout key={} bottom_ratio={} top={} legacy_tabs={}",
                storage_key,
                terminal_layout.bottom_ratio,
                terminal_layout.top_panes.len(),
                terminal_layout.tabs.len()
            ),
        );
        self.state.terminal_layout = terminal_layout;
        self.state.terminal_runtime = TerminalRuntimeSummary::default();
        let selected_worktree = self
            .state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .cloned();
        self.trace_workspace_state("select_scope", &key.worktree_id, "loading runtime state");
        self.spawn_worktree_switch_load(
            project,
            worktree_id,
            selected_worktree,
            key,
            generation,
            cx,
        );
    }

    fn terminal_cleanup_for_worktree(&self, worktree_id: &str) -> WorktreeTerminalCleanup {
        let terminal_ids = self.managed_terminal_ids_for_worktree(worktree_id);
        let local_session_ids = self
            .terminal_manager
            .list()
            .into_iter()
            .map(|session| session.id)
            .collect::<HashSet<_>>();
        let mut local_terminal_ids = Vec::new();
        let mut remote_targets = Vec::new();
        for terminal_id in terminal_ids {
            if local_session_ids.contains(&terminal_id) {
                local_terminal_ids.push(terminal_id.clone());
            }
            if let Some(target) = self.remote_close_target_for_terminal(&terminal_id) {
                remote_targets.push(target);
            }
        }
        WorktreeTerminalCleanup {
            terminal_manager: self.terminal_manager.clone(),
            local_terminal_ids,
            remote_targets,
        }
    }

    fn detach_managed_terminals_for_worktree(
        &mut self,
        project_id: &str,
        worktree_id: &str,
    ) -> DetachedWorktreeTerminals {
        let scope_key = WorktreeScopeKey {
            project_id: project_id.to_string(),
            worktree_id: worktree_id.to_string(),
        };
        let terminal_ids = self.managed_terminal_ids_for_worktree(worktree_id);
        let mut lifecycle_removed = false;
        for terminal_id in terminal_ids {
            let cleanup = self.detach_terminal_session_if_present(&terminal_id);
            lifecycle_removed |= cleanup.lifecycle_removed;
            self.terminal_attach_in_flight.remove(&terminal_id);
        }
        self.terminals.retain_mut(|tab| {
            tab.panes.retain(|slot| {
                terminal_slot_id_for_owner(tab.terminal_id.as_deref(), slot, worktree_id).is_none()
            });
            !tab.panes.is_empty()
        });
        self.collapsed_terminal_panes.retain(|slot| {
            slot.terminal_id
                .as_deref()
                .is_none_or(|terminal_id| !terminal_id_belongs_to_owner(terminal_id, worktree_id))
        });
        self.active_terminal_runtime_ids.remove(&scope_key);
        self.terminal_layout_cache.remove(&scope_key);
        self.file_panel_cache.remove(&scope_key);
        DetachedWorktreeTerminals { lifecycle_removed }
    }

    fn managed_terminal_ids_for_worktree(&self, worktree_id: &str) -> Vec<String> {
        let mut terminal_ids = HashSet::new();
        for tab in &self.terminals {
            for slot in &tab.panes {
                if let Some(terminal_id) =
                    terminal_slot_id_for_owner(tab.terminal_id.as_deref(), slot, worktree_id)
                {
                    terminal_ids.insert(terminal_id.to_string());
                }
            }
        }
        for slot in &self.collapsed_terminal_panes {
            if let Some(terminal_id) = slot
                .terminal_id
                .as_deref()
                .filter(|terminal_id| terminal_id_belongs_to_owner(terminal_id, worktree_id))
            {
                terminal_ids.insert(terminal_id.to_string());
            }
        }
        for terminal_id in self.terminal_pane_registry.keys() {
            if terminal_id_belongs_to_owner(terminal_id, worktree_id) {
                terminal_ids.insert(terminal_id.clone());
            }
        }
        terminal_ids.into_iter().collect()
    }

    fn remote_close_target_for_terminal(
        &self,
        terminal_id: &str,
    ) -> Option<crate::terminal::RemoteTerminalCloseTarget> {
        self.terminal_pane_registry
            .get(terminal_id)
            .and_then(TerminalPane::remote_close_target)
            .or_else(|| {
                self.terminals.iter().find_map(|tab| {
                    let tab_terminal_id = tab.terminal_id.as_deref();
                    tab.panes.iter().find_map(|slot| {
                        let slot_terminal_id = slot.terminal_id.as_deref().or(tab_terminal_id)?;
                        if slot_terminal_id == terminal_id {
                            slot.pane.as_ref()?.remote_close_target()
                        } else {
                            None
                        }
                    })
                })
            })
            .or_else(|| {
                self.collapsed_terminal_panes.iter().find_map(|slot| {
                    if slot.terminal_id.as_deref() == Some(terminal_id) {
                        slot.pane.as_ref()?.remote_close_target()
                    } else {
                        None
                    }
                })
            })
    }

    pub(in crate::app) fn request_remove_worktree_by_id(
        &mut self,
        worktree_id: String,
        remove_branch: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(worktree) = self
            .state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .cloned()
        else {
            self.status_message = "worktree not found".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        if worktree.is_default {
            self.status_message = self.text(
                "worktree.default.remove_denied",
                "The default worktree cannot be removed.",
            );
            self.invalidate_status_bar(cx);
            return;
        }

        let title = self.text("worktree.remove.title", "Remove Worktree");
        let message = self
            .text(
                "worktree.remove.message_format",
                "Remove %@ from Codux and the Git worktree list? The branch will not be deleted.",
            )
            .replace("%@", &worktree_confirm_display_name(&worktree));
        let confirm_label = if remove_branch {
            self.text(
                "worktree.menu.remove_with_branch",
                "Remove and Delete Branch",
            )
        } else {
            self.text("common.remove", "Remove")
        };
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for worktree removal confirmation".to_string();
        self.invalidate_status_bar(cx);
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
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
                Ok(true) => app.remove_worktree_by_id(worktree_id, remove_branch, cx),
                Ok(false) => {
                    app.status_message = "worktree removal canceled".to_string();
                    app.invalidate_status_bar(cx);
                }
                Err(error) => {
                    let title = app.text("worktree.remove.title", "Remove Worktree");
                    app.status_message = title.clone();
                    app.show_system_error_alert(title, error, cx);
                    app.invalidate_status_bar(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::app) fn merge_worktree_by_id(
        &mut self,
        worktree_id: String,
        cx: &mut Context<Self>,
    ) {
        self.request_merge_worktree_by_id(worktree_id, cx);
    }

    pub(in crate::app) fn request_merge_worktree_by_id(
        &mut self,
        worktree_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to merge worktree".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        let Some(worktree) = self
            .state
            .worktrees
            .worktrees
            .iter()
            .find(|worktree| worktree.id == worktree_id)
            .cloned()
        else {
            self.status_message = "worktree not found".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
        if worktree.is_default {
            self.status_message = self.text(
                "worktree.default.merge_denied",
                "The default worktree cannot be merged into itself.",
            );
            self.invalidate_status_bar(cx);
            return;
        }

        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worktree_name = worktree_confirm_display_name(&worktree);
        let base_branch = self
            .state
            .worktrees
            .tasks
            .iter()
            .find(|task| task.worktree_id == worktree_id)
            .map(|task| task.base_branch.trim().to_string())
            .filter(|branch| !branch.is_empty())
            .or_else(|| {
                let branch = self.state.git.branch.trim();
                (!branch.is_empty()).then(|| branch.to_string())
            })
            .unwrap_or_else(|| self.text("git.branch.none", "No Branch"));
        let title = self.text("worktree.merge.title", "Merge Worktree");
        let message = self
            .text(
                "worktree.merge_to_mainline.message_format",
                "Merge %@ into %@.",
            )
            .replacen("%@", &worktree_name, 1)
            .replacen("%@", &base_branch, 1);
        let confirm_label = self.text("worktree.menu.merge", "Merge to Mainline");
        let cancel_label = self.text("common.cancel", "Cancel");
        let ok_label = self.text("common.ok", "OK");
        let success_label = self.text("worktree.merge.success", "Merged worktree.");
        let service = self.runtime_service.clone();
        let dialog_service = self.runtime_service.clone();
        self.status_message = "waiting for worktree merge confirmation".to_string();
        self.invalidate_status_bar(cx);
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
            let confirmed = codux_runtime::async_runtime::spawn_blocking({
                let service = dialog_service.clone();
                let title = title.clone();
                let message = message.clone();
                let confirm_label = confirm_label.clone();
                let cancel_label = cancel_label.clone();
                move || {
                    service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                        title,
                        message,
                        confirm_label,
                        cancel_label,
                    })
                }
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let should_merge = match confirmed {
                Ok(value) => value,
                Err(error) => {
                    let _ = this.update(cx, |app, cx| {
                        app.status_message = title.clone();
                        app.show_system_error_alert(title.clone(), error, cx);
                        app.invalidate_status_bar(cx);
                    });
                    return;
                }
            };
            if !should_merge {
                let _ = this.update(cx, |app, cx| {
                    app.status_message = "worktree merge canceled".to_string();
                    app.invalidate_status_bar(cx);
                });
                return;
            }

            let _ = this.update(cx, |app, cx| {
                app.status_message = app.text("worktree.merge.running", "Merging worktree.");
                app.runtime_trace(
                    "worktree",
                    &format!(
                        "merge start project={} worktree={}",
                        project_id, worktree_id
                    ),
                );
                app.invalidate_worktree_context(cx);
            });

            let result = codux_runtime::async_runtime::spawn_blocking({
                let service = service.clone();
                let project_id = project_id.clone();
                let project_path = project_path.clone();
                let worktree_id = worktree_id.clone();
                move || service.merge_worktree(&project_id, &project_path, &worktree_id)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let error = match result {
                Ok(summary) => {
                    publish_child_window_update(ChildWindowUpdateKind::Worktree);
                    let _ = this.update(cx, |app, cx| {
                        let selected_matches =
                            app.state.selected_project.as_ref().is_some_and(|project| {
                                project.id == project_id && project.path == project_path
                            });
                        if selected_matches {
                            app.state.worktrees = summary;
                        }
                        app.runtime_trace(
                            "worktree",
                            &format!("merge ok project={} worktree={}", project_id, worktree_id),
                        );
                        app.status_message = app.text("worktree.merge.success", "Merged worktree.");
                        app.refresh_git_panel_state_async(cx);
                        app.invalidate_worktree_context(cx);
                    });
                    let _ = codux_runtime::async_runtime::spawn_blocking({
                        let service = dialog_service.clone();
                        let title = title.clone();
                        let ok_label = ok_label.clone();
                        let message = success_label.clone();
                        move || {
                            service.localized_alert_dialog(LocalizedAlertDialogRequest {
                                title,
                                message,
                                button_label: ok_label,
                            })
                        }
                    })
                    .await;
                    return;
                }
                Err(error) => error,
            };

            let _ = codux_runtime::async_runtime::spawn_blocking({
                let service = dialog_service;
                let title = title.clone();
                let ok_label = ok_label.clone();
                let message = error.clone();
                move || {
                    service.localized_alert_dialog(LocalizedAlertDialogRequest {
                        title,
                        message,
                        button_label: ok_label,
                    })
                }
            })
            .await;
            let _ = this.update(cx, |app, cx| {
                app.runtime_trace(
                    "worktree",
                    &format!(
                        "merge failed project={} worktree={} error={}",
                        project_id, worktree_id, error
                    ),
                );
                app.status_message = title.clone();
                app.refresh_git_panel_state_async(cx);
                app.invalidate_worktree_context(cx);
            });
        })
        .detach();
    }

    pub(in crate::app) fn select_worktree(
        &mut self,
        worktree_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.selected_project.is_none() {
            self.status_message = "no selected project for worktree selection".to_string();
            self.invalidate_status_bar(cx);
            return;
        }
        if self.state.worktrees.selected_worktree_id.as_deref() == Some(worktree_id.as_str()) {
            return;
        }
        let previous_worktree_id = self
            .state
            .worktrees
            .selected_worktree_id
            .clone()
            .unwrap_or_default();
        self.trace_workspace_state(
            "select_begin",
            &previous_worktree_id,
            &format!("target_worktree={worktree_id}"),
        );
        self.remember_current_file_panel_state();
        self.remember_focused_terminal_for_current_scope(window, cx);
        self.sync_terminal_state_for_project_switch();
        self.state.worktrees.selected_worktree_id = Some(worktree_id.clone());
        self.load_selected_worktree_context(
            worktree_id.clone(),
            Some(format!("selected worktree: {worktree_id}")),
            cx,
        );
        self.invalidate_worktree_context(cx);
    }

    fn spawn_worktree_switch_load(
        &mut self,
        project: ProjectInfo,
        worktree_id: String,
        selected_worktree: Option<WorktreeInfo>,
        scope_key: WorktreeScopeKey,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let runtime_service = self.runtime_service.clone();
        self.trace_workspace_state(
            "load_spawn",
            &scope_key.worktree_id,
            &format!("target_worktree={worktree_id}"),
        );
        let include_cached = self.state.settings.statistics_mode == "includingCache";
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::spawn_blocking({
                let scope_key = scope_key.clone();
                move || {
                    runtime_service.select_worktree(&project.id, &worktree_id)?;
                    let request = ai_history_worktree_request(&project, selected_worktree.as_ref());
                    let ai_history = runtime_service
                        .indexed_project_ai_history_state(request)
                        .ok()
                        .map(|state| {
                            ai_history_summary_from_state_or_status(
                                &AIHistorySummary::default(),
                                &state,
                            )
                        })
                        .unwrap_or_else(|| AIHistorySummary {
                            is_loading: true,
                            detail: "loading".to_string(),
                            ..AIHistorySummary::default()
                        });
                    let remote_ai_current_sessions = runtime_service
                        .remote_ai_current_sessions(
                            &project.path,
                            &scope_key.worktree_id,
                            include_cached,
                        )
                        .and_then(Result::ok)
                        .unwrap_or_default();
                    let terminal_layout = runtime_service.reload_terminal_layout(Some(
                        &super::app_state::worktree_terminal_storage_key(&scope_key),
                    ));
                    Ok::<_, String>(WorktreeSwitchLoad {
                        project_id: project.id,
                        generation,
                        scope_key,
                        ai_history,
                        remote_ai_current_sessions,
                        terminal_layout,
                        terminal_runtime: TerminalRuntimeSummary::default(),
                    })
                }
            })
            .await
            .ok()
            .and_then(|result: Result<WorktreeSwitchLoad, String>| result.ok());
            let _ = this.update_in(cx, |app, window, cx| {
                if let Some(load) = load {
                    app.apply_worktree_switch_load(load, window, cx);
                } else {
                    if app.project_switch_generation == generation {
                        app.terminal_layout_loading = false;
                    }
                    app.runtime_trace(
                        "workspace-switch",
                        "load_failed_or_cancelled project=unknown worktree=unknown",
                    );
                    app.invalidate_worktree_context(cx);
                }
            });
        })
        .detach();
    }

    fn apply_worktree_switch_load(
        &mut self,
        load: WorktreeSwitchLoad,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(load.project_id.as_str())
            || current_worktree_scope_key(&self.state).as_ref() != Some(&load.scope_key)
            || self.project_switch_generation != load.generation
        {
            self.runtime_trace(
                "workspace-switch",
                &format!(
                    "load_stale project={} worktree={} generation={} current_generation={}",
                    load.project_id,
                    load.scope_key.worktree_id,
                    load.generation,
                    self.project_switch_generation
                ),
            );
            return;
        }
        self.trace_workspace_state(
            "load_apply",
            &load.scope_key.worktree_id,
            &format!(
                "worktrees={} tasks={} loaded_top_panes={} legacy_tabs={}",
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len(),
                load.terminal_layout.top_panes.len(),
                load.terminal_layout.tabs.len()
            ),
        );
        self.state.ai_history = load.ai_history;
        self.state.remote_ai_current_sessions = load.remote_ai_current_sessions;
        self.state.refresh_ai_history_stats();
        self.normalize_selected_ai_session();
        self.restore_cached_file_panel_state();
        self.invalidate_file_panel(cx);
        self.spawn_worktree_sidebar_load(load.generation, cx);
        self.schedule_terminal_layout_restore(
            load.terminal_layout,
            load.terminal_runtime,
            load.generation,
            window,
            cx,
        );
        self.trace_workspace_state(
            "select_done",
            &load.scope_key.worktree_id,
            &format!(
                "terminals={} active_terminal_id={}",
                self.terminals.len(),
                self.active_terminal_id
            ),
        );
        self.invalidate_worktree_context(cx);
    }
}

fn terminal_slot_id_for_owner<'a>(
    tab_terminal_id: Option<&'a str>,
    slot: &'a TerminalPaneSlot,
    owner_id: &str,
) -> Option<&'a str> {
    slot.terminal_id
        .as_deref()
        .or(tab_terminal_id)
        .filter(|terminal_id| terminal_id_belongs_to_owner(terminal_id, owner_id))
}

fn worktree_confirm_display_name(worktree: &WorktreeInfo) -> String {
    let name = worktree.name.trim();
    if !name.is_empty() {
        return name.to_string();
    }
    let branch = worktree.branch.trim();
    if !branch.is_empty() {
        return branch.to_string();
    }
    worktree.path.clone()
}

fn default_worktree_name() -> String {
    chrono::Local::now().format("%Y%m%d-%H%M%S").to_string()
}
