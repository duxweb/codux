use super::*;
use crate::app::app_events::{ChildWindowUpdateKind, publish_child_window_update};
use crate::app::window_actions::{AuxiliaryWindowSlot, AuxiliaryWindowSpec};

impl CoduxApp {
    pub(super) fn open_worktree_creator_window(
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

    pub(super) fn submit_worktree_creator(&mut self, window: &mut Window, cx: &mut Context<Self>) {
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

    pub(super) fn open_worktree_folder(&mut self, path: String, cx: &mut Context<Self>) {
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

    pub(super) fn ensure_active_terminal_mounted(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some((tab_index, _)) = active_terminal_slot_indices(
            &self.terminals,
            &self.state.terminal_layout.active_terminal_id,
            self.active_terminal_id,
        ) else {
            return Ok(());
        };
        let config = self.terminal_config_from_settings();
        let terminal_manager = self.terminal_manager.clone();
        let base_pty_config = self.current_terminal_base_pty_config();
        let terminal_pane_registry = self.terminal_pane_registry.clone();
        let mut registrations = Vec::new();
        let tab = &mut self.terminals[tab_index];
        for slot in &mut tab.panes {
            if slot.pane.is_some() {
                continue;
            }
            let pty_config = terminal_pty_config_for_terminal_id(
                &base_pty_config,
                slot.terminal_id.as_deref(),
                &slot.title,
            );
            if let Some(pane) = slot
                .terminal_id
                .as_deref()
                .and_then(|terminal_id| terminal_pane_registry.get(terminal_id))
                .cloned()
            {
                refresh_terminal_pane_config(&pane, &config, cx);
                slot.pane = Some(pane);
                continue;
            }
            let pane = TerminalPane::spawn_with_pty_config(
                cx,
                terminal_manager.clone(),
                pty_config,
                config.clone(),
            )
            .map_err(|error| error.to_string())?;
            if let Some(terminal_id) = slot.terminal_id.clone() {
                registrations.push((terminal_id, pane.clone()));
            }
            slot.pane = Some(pane);
        }
        for (terminal_id, pane) in registrations {
            self.register_terminal_pane(Some(&terminal_id), &pane, cx);
        }
        Ok(())
    }

    pub(super) fn refresh_terminal_slot_snapshots(&mut self) {
        let terminal_manager = self.terminal_manager.clone();
        for tab in &mut self.terminals {
            let tab_terminal_id = tab.terminal_id.clone();
            for slot in &mut tab.panes {
                if let Some(pane) = &slot.pane {
                    let output = pane.output_snapshot();
                    slot.restored_output_bytes = output.bytes;
                    slot.restored_output_tail = output.tail;
                    continue;
                }
                let Some(terminal_id) = slot
                    .terminal_id
                    .clone()
                    .or_else(|| tab_terminal_id.clone())
                    .filter(|id| !id.trim().is_empty())
                else {
                    continue;
                };
                if let Ok(output) = terminal_manager.output_snapshot(&terminal_id) {
                    slot.restored_output_bytes = output.bytes;
                    slot.restored_output_tail = output.tail;
                }
            }
        }
    }

    pub(super) fn detach_inactive_terminal_views(&mut self) {
        self.refresh_terminal_slot_snapshots();
        for tab in &mut self.terminals {
            if tab.placement == TerminalTabPlacement::Top || tab.id == self.active_terminal_id {
                continue;
            }
            for slot in &mut tab.panes {
                slot.pane = None;
            }
        }
    }

    pub(super) fn main_terminal(&self) -> Option<&TerminalTab> {
        self.terminals
            .iter()
            .find(|tab| tab.placement == TerminalTabPlacement::Top)
            .or_else(|| self.active_terminal())
    }

    pub(super) fn main_terminal_mut(&mut self) -> Option<&mut TerminalTab> {
        let index = self
            .terminals
            .iter()
            .position(|tab| tab.placement == TerminalTabPlacement::Top)
            .or_else(|| {
                let active_id = self.active_terminal_id;
                self.terminals.iter().position(|tab| tab.id == active_id)
            })
            .or_else(|| (!self.terminals.is_empty()).then_some(0))?;
        self.terminals.get_mut(index)
    }

    pub(super) fn bottom_terminals(&self) -> impl Iterator<Item = &TerminalTab> {
        self.terminals
            .iter()
            .filter(|tab| tab.placement == TerminalTabPlacement::Bottom)
    }

    pub(super) fn active_bottom_terminal(&self) -> Option<&TerminalTab> {
        self.bottom_terminals()
            .find(|tab| tab.id == self.active_terminal_id)
            .or_else(|| self.bottom_terminals().next())
    }

    pub(super) fn normalize_active_bottom_terminal_id(&mut self) {
        let active_is_bottom = self.terminals.iter().any(|tab| {
            tab.placement == TerminalTabPlacement::Bottom && tab.id == self.active_terminal_id
        });
        if active_is_bottom {
            return;
        }
        let bottom_id = self
            .terminals
            .iter()
            .find(|tab| tab.placement == TerminalTabPlacement::Bottom)
            .map(|tab| tab.id);
        if let Some(bottom_id) = bottom_id {
            self.active_terminal_id = bottom_id;
        }
    }

    pub(super) fn terminal_slot_terminal_id(
        tab: &TerminalTab,
        _pane_index: usize,
        slot: &TerminalPaneSlot,
    ) -> Option<String> {
        slot.terminal_id
            .clone()
            .or_else(|| tab.terminal_id.clone())
            .filter(|id| !id.trim().is_empty())
    }

    pub(super) fn kill_terminal_session_if_present(
        &mut self,
        terminal_id: &str,
    ) -> Result<(), String> {
        self.remove_registered_terminal_pane(terminal_id);
        let exists = self
            .terminal_manager
            .list()
            .iter()
            .any(|session| session.id == terminal_id);
        if exists {
            self.terminal_manager
                .kill(terminal_id)
                .map_err(|error| error.to_string())
        } else {
            Ok(())
        }
    }

    pub(super) fn register_terminal_panes(&mut self, cx: &mut Context<Self>) {
        let registrations = self.terminal_pane_registrations();
        for (terminal_id, pane) in registrations {
            self.register_terminal_pane(Some(&terminal_id), &pane, cx);
        }
    }

    pub(super) fn register_terminal_panes_without_observers(&mut self) {
        let registrations = self.terminal_pane_registrations();
        for (terminal_id, pane) in registrations {
            self.terminal_pane_registry.insert(terminal_id, pane);
        }
    }

    fn terminal_pane_registrations(&self) -> Vec<(String, TerminalPane)> {
        let mut registrations = Vec::new();
        for tab in &self.terminals {
            let tab_terminal_id = tab.terminal_id.clone();
            for slot in &tab.panes {
                let Some(pane) = slot.pane.as_ref() else {
                    continue;
                };
                let Some(terminal_id) = slot
                    .terminal_id
                    .clone()
                    .or_else(|| tab_terminal_id.clone())
                    .filter(|id| !id.trim().is_empty())
                else {
                    continue;
                };
                registrations.push((terminal_id, pane.clone()));
            }
        }
        registrations
    }

    pub(super) fn remove_registered_terminal_pane(&mut self, terminal_id: &str) {
        self.terminal_pane_registry.remove(terminal_id);
    }

    pub(super) fn register_terminal_pane(
        &mut self,
        terminal_id: Option<&str>,
        pane: &TerminalPane,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal_id) = terminal_id.filter(|id| !id.trim().is_empty()) else {
            return;
        };
        let app = cx.entity().downgrade();
        let terminal_id = terminal_id.to_string();
        let observer_terminal_id = terminal_id.clone();
        pane.view.update(cx, |terminal, _| {
            terminal.set_focus_observer(move |_window, cx| {
                let terminal_id = observer_terminal_id.clone();
                let _ = app.update(cx, |app, cx| {
                    app.record_focused_terminal_runtime_id(&terminal_id, cx);
                });
            });
        });
        self.terminal_pane_registry
            .insert(terminal_id, pane.clone());
    }

    pub(super) fn record_focused_terminal_runtime_id(
        &mut self,
        terminal_id: &str,
        cx: &mut Context<Self>,
    ) {
        let terminal_id = terminal_id.trim();
        if terminal_id.is_empty() {
            return;
        }
        let Some(key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        let Some((tab_id, placement)) = self.terminals.iter().find_map(|tab| {
            let matches_tab = tab.terminal_id.as_deref() == Some(terminal_id);
            let matches_pane = tab
                .panes
                .iter()
                .any(|slot| slot.terminal_id.as_deref() == Some(terminal_id));
            (matches_tab || matches_pane).then_some((tab.id, tab.placement))
        }) else {
            self.runtime_trace(
                "terminal-focus",
                &format!("skip stale focus terminal_id={terminal_id}"),
            );
            return;
        };

        let runtime_changed = self.state.terminal_layout.active_terminal_id != terminal_id;
        let remembered_runtime_changed = self
            .active_terminal_runtime_ids
            .get(&key)
            .is_none_or(|active| active != terminal_id);
        let bottom_changed = placement == TerminalTabPlacement::Bottom
            && (self.active_terminal_id != tab_id
                || self
                    .active_bottom_terminal_ids
                    .get(&key)
                    .is_none_or(|active| active != terminal_id));

        if !runtime_changed && !remembered_runtime_changed && !bottom_changed {
            return;
        }

        self.state.terminal_layout.active_terminal_id = terminal_id.to_string();
        self.active_terminal_runtime_ids
            .insert(key.clone(), terminal_id.to_string());
        if placement == TerminalTabPlacement::Bottom {
            self.active_terminal_id = tab_id;
            self.active_bottom_terminal_ids
                .insert(key, terminal_id.to_string());
        }
        self.runtime_trace(
            "terminal-focus",
            &format!("focus_in terminal_id={terminal_id} tab={tab_id} placement={placement:?}"),
        );
        self.invalidate_terminal_workspace(cx);
    }

    pub(super) fn sync_terminal_state_after_layout_change(&mut self, _cx: &mut Context<Self>) {
        self.refresh_terminal_slot_snapshots();
        let owner_id = super::ai_runtime_status::terminal_layout_owner_id(&self.state);
        let storage_key =
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state);
        let runtime_snapshot = self.terminal_runtime_snapshot();
        let runtime = terminal_runtime_summary_from_inputs(
            &self.state.terminal_runtime,
            runtime_snapshot.0.clone(),
            runtime_snapshot.1.clone(),
        );
        self.sync_terminal_layout_snapshot("layout change", owner_id, storage_key, runtime);
    }

    pub(super) fn sync_terminal_state_for_project_switch(&mut self) {
        if self.terminal_layout_loading {
            self.runtime_trace(
                "terminal-layout",
                "skip project-switch layout sync while terminal layout is loading",
            );
            return;
        }
        let owner_id = super::ai_runtime_status::terminal_layout_owner_id(&self.state);
        let storage_key =
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state);
        let runtime = self.lightweight_terminal_runtime_summary();
        self.sync_terminal_layout_snapshot("project-switch", owner_id, storage_key, runtime);
    }

    fn sync_terminal_layout_snapshot(
        &mut self,
        reason: &str,
        owner_id: Option<String>,
        storage_key: Option<String>,
        runtime: TerminalRuntimeSummary,
    ) {
        let layout_snapshot = self.terminal_layout_snapshot();
        if layout_snapshot.tabs.is_empty() && layout_snapshot.top_panes.is_empty() {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip {reason} layout sync because snapshot is empty"),
            );
            return;
        }
        let layout = TerminalLayoutSummary {
            active_terminal_id: String::new(),
            top_panes: layout_snapshot.top_panes.clone(),
            tabs: layout_snapshot.tabs.clone(),
            top_ratios: layout_snapshot.top_ratios.clone(),
            bottom_ratio: layout_snapshot.bottom_ratio,
            error: None,
        };
        let (layout, runtime) = normalize_terminal_restore_state(
            owner_id.as_deref(),
            layout,
            runtime,
            &self.state.settings.language,
        );
        self.state.terminal_layout = layout;
        self.state.terminal_runtime = runtime;
        self.remember_active_bottom_terminal_for_current_scope();
        self.cache_current_terminal_layout_state();
        self.spawn_persist_terminal_layout_snapshot(storage_key, layout_snapshot);
    }

    pub(super) fn cache_current_terminal_layout_state(&mut self) {
        let Some(key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        self.terminal_layout_cache.insert(
            key,
            super::app_state::TerminalLayoutCacheEntry {
                layout: self.state.terminal_layout.clone(),
                runtime: self.state.terminal_runtime.clone(),
            },
        );
    }

    pub(super) fn cached_terminal_layout_state(
        &self,
        key: &WorktreeScopeKey,
    ) -> Option<(TerminalLayoutSummary, TerminalRuntimeSummary)> {
        self.terminal_layout_cache
            .get(key)
            .map(|entry| (entry.layout.clone(), entry.runtime.clone()))
    }

    fn lightweight_terminal_runtime_summary(&self) -> TerminalRuntimeSummary {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|duration| duration.as_secs_f64())
            .unwrap_or_default();
        let active_terminal_id = self.active_terminal_runtime_id();
        let base_pty_config = self.current_terminal_base_pty_config();
        let existing_by_key = self
            .state
            .terminal_runtime
            .sessions
            .iter()
            .map(|session| (session.terminal_id.clone(), session))
            .collect::<HashMap<_, _>>();
        let sessions = self
            .terminals
            .iter()
            .flat_map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .filter_map(|(pane_index, slot)| {
                        let project = self.state.selected_project.as_ref();
                        let terminal_id = Self::terminal_slot_terminal_id(tab, pane_index, slot)?;
                        let existing = existing_by_key.get(&terminal_id).copied();
                        let project_id = base_pty_config
                            .project_id
                            .clone()
                            .or_else(|| project.map(|project| project.id.clone()))
                            .or_else(|| existing.map(|session| session.project_id.clone()))
                            .unwrap_or_default();
                        let project_name = base_pty_config
                            .project_name
                            .clone()
                            .or_else(|| project.map(|project| project.name.clone()))
                            .or_else(|| existing.map(|session| session.project_name.clone()))
                            .unwrap_or_default();
                        let project_path = base_pty_config
                            .cwd
                            .clone()
                            .or_else(|| project.map(|project| project.path.clone()))
                            .or_else(|| existing.map(|session| session.project_path.clone()))
                            .unwrap_or_default();
                        let cwd = base_pty_config
                            .cwd
                            .clone()
                            .or_else(|| existing.map(|session| session.cwd.clone()))
                            .unwrap_or_else(|| project_path.clone());
                        Some(TerminalRuntimeSessionSummary {
                            terminal_id,
                            title: slot.title.clone(),
                            project_id,
                            project_name,
                            project_path,
                            cwd,
                            status: "running".to_string(),
                            is_running: true,
                            created_at: existing.map(|session| session.created_at).unwrap_or(now),
                            last_active_at: now,
                            has_buffer: existing.map(|session| session.has_buffer).unwrap_or(false),
                            buffer_characters: existing
                                .map(|session| session.buffer_characters)
                                .unwrap_or_default(),
                            input_bytes: existing
                                .map(|session| session.input_bytes)
                                .unwrap_or_default(),
                            last_input_at: existing.and_then(|session| session.last_input_at),
                            input_history: existing
                                .map(|session| session.input_history.clone())
                                .unwrap_or_default(),
                            output_bytes: existing
                                .map(|session| session.output_bytes)
                                .unwrap_or(slot.restored_output_bytes),
                            output_tail: existing
                                .map(|session| session.output_tail.clone())
                                .unwrap_or_else(|| slot.restored_output_tail.clone()),
                        })
                    })
            })
            .collect::<Vec<_>>();
        TerminalRuntimeSummary {
            path: self.state.terminal_runtime.path.clone(),
            active_terminal_id,
            open_count: sessions.len(),
            closed_count: 0,
            sessions,
            error: None,
        }
    }

    pub(super) fn terminal_runtime_snapshot(&self) -> (String, Vec<TerminalRuntimeSessionInput>) {
        let active_terminal_id = self.active_terminal_runtime_id();
        let base_pty_config = self.current_terminal_base_pty_config();
        let sessions = self
            .terminals
            .iter()
            .flat_map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .filter_map(|(pane_index, slot)| {
                        let project = self.state.selected_project.as_ref();
                        let terminal_id = Self::terminal_slot_terminal_id(tab, pane_index, slot)?;
                        let project_id = base_pty_config
                            .project_id
                            .clone()
                            .or_else(|| project.map(|project| project.id.clone()))
                            .unwrap_or_default();
                        let project_name = base_pty_config
                            .project_name
                            .clone()
                            .or_else(|| project.map(|project| project.name.clone()))
                            .unwrap_or_default();
                        let project_path = base_pty_config
                            .cwd
                            .clone()
                            .or_else(|| project.map(|project| project.path.clone()))
                            .unwrap_or_default();
                        let cwd = base_pty_config
                            .cwd
                            .clone()
                            .unwrap_or_else(|| project_path.clone());
                        let input = slot
                            .pane
                            .as_ref()
                            .map(|pane| pane.input_snapshot())
                            .or_else(|| self.terminal_manager.input_snapshot(&terminal_id).ok());
                        let output = slot
                            .pane
                            .as_ref()
                            .map(|pane| pane.output_snapshot())
                            .or_else(|| self.terminal_manager.output_snapshot(&terminal_id).ok());
                        let input_bytes =
                            input.as_ref().map(|input| input.bytes).unwrap_or_default();
                        let input_history = input
                            .map(|input| {
                                input
                                    .history
                                    .into_iter()
                                    .map(|entry| TerminalInputSummary {
                                        text: entry.text,
                                        bytes: entry.bytes,
                                        timestamp: entry.timestamp,
                                    })
                                    .collect()
                            })
                            .unwrap_or_default();
                        let (output_bytes, output_tail) = output
                            .filter(|output| !output.tail.is_empty())
                            .map(|output| (output.bytes, output.tail))
                            .unwrap_or_else(|| {
                                (
                                    slot.restored_output_bytes,
                                    slot.restored_output_tail.clone(),
                                )
                            });
                        Some(TerminalRuntimeSessionInput {
                            terminal_id,
                            title: slot.title.clone(),
                            project_id,
                            project_name,
                            project_path,
                            cwd,
                            input_bytes,
                            input_history,
                            output_bytes,
                            output_tail,
                        })
                    })
            })
            .collect();
        (active_terminal_id, sessions)
    }

    pub(super) fn terminal_layout_snapshot(&self) -> TerminalLayoutSnapshot {
        let tabs = self
            .terminals
            .iter()
            .filter(|tab| tab.placement == TerminalTabPlacement::Bottom)
            .map(terminal_tab_summary)
            .collect::<Vec<_>>();
        let top_panes = self
            .main_terminal()
            .map(|tab| {
                tab.panes
                    .iter()
                    .map(terminal_pane_summary)
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            top_panes.len(),
        );
        let bottom_ratio = clamp_terminal_bottom_ratio(self.state.terminal_layout.bottom_ratio);
        TerminalLayoutSnapshot {
            tabs,
            top_panes,
            top_ratios,
            bottom_ratio,
        }
    }

    pub(super) fn active_bottom_terminal_runtime_id(&self) -> Option<String> {
        self.terminals
            .iter()
            .find(|tab| {
                tab.placement == TerminalTabPlacement::Bottom && tab.id == self.active_terminal_id
            })
            .and_then(|tab| tab.terminal_id.clone())
    }

    pub(super) fn remember_focused_terminal_for_current_scope(
        &mut self,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        let Some(terminal_id) = self.focused_terminal_runtime_id(window, cx) else {
            self.runtime_trace(
                "terminal-focus",
                "skip switch remember because no terminal view is focused",
            );
            return;
        };
        self.runtime_trace(
            "terminal-focus",
            &format!("remember focused terminal id={terminal_id}"),
        );
        self.state.terminal_layout.active_terminal_id = terminal_id.clone();
        self.active_terminal_runtime_ids.insert(key, terminal_id);
    }

    pub(super) fn remembered_active_terminal_runtime_id(&self) -> Option<String> {
        let key = current_worktree_scope_key(&self.state)?;
        self.active_terminal_runtime_ids.get(&key).cloned()
    }

    pub(super) fn remember_active_bottom_terminal_for_current_scope(&mut self) {
        let Some(key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        let Some(terminal_id) = self.active_bottom_terminal_runtime_id() else {
            return;
        };
        self.active_bottom_terminal_ids.insert(key, terminal_id);
    }

    pub(super) fn remembered_active_bottom_terminal_id(&self) -> Option<String> {
        let key = current_worktree_scope_key(&self.state)?;
        self.active_bottom_terminal_ids.get(&key).cloned()
    }

    pub(in crate::app) fn update_terminal_bottom_ratio(
        &mut self,
        layout_key: String,
        bottom_ratio: f64,
        cx: &mut Context<Self>,
    ) {
        if super::ai_runtime_status::current_terminal_layout_storage_key(&self.state).as_deref()
            != Some(layout_key.as_str())
        {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip stale bottom ratio layout={layout_key}"),
            );
            return;
        }
        let bottom_ratio = clamp_terminal_bottom_ratio(bottom_ratio);
        if (clamp_terminal_bottom_ratio(self.state.terminal_layout.bottom_ratio) - bottom_ratio)
            .abs()
            < 0.001
        {
            return;
        }
        if self.terminal_layout_loading {
            self.runtime_trace(
                "terminal-layout",
                &format!(
                    "skip resize_bottom while loading layout={} next={}",
                    layout_key, bottom_ratio
                ),
            );
            return;
        }
        self.state.terminal_layout.bottom_ratio = bottom_ratio;
        self.persist_current_terminal_layout();
        self.invalidate_terminal_workspace(cx);
    }

    pub(super) fn apply_terminal_layout_from_summary(
        &mut self,
        terminal_layout: TerminalLayoutSummary,
        terminal_runtime: TerminalRuntimeSummary,
        cx: &mut Context<Self>,
    ) {
        let restore_started_at = Instant::now();
        self.terminal_layout_loading = false;
        let owner_id = super::ai_runtime_status::terminal_layout_owner_id(&self.state);
        let (terminal_layout, terminal_runtime) = normalize_terminal_restore_state(
            owner_id.as_deref(),
            terminal_layout,
            terminal_runtime,
            &self.state.settings.language,
        );
        self.state.terminal_layout = terminal_layout;
        self.state.terminal_runtime = terminal_runtime;
        let plan_started_at = Instant::now();
        let restore_plan = terminal_restore_plan_for_language(
            &self.state.terminal_layout,
            &self.state.terminal_runtime,
            &self.state.settings.language,
            self.remembered_active_terminal_runtime_id(),
            self.remembered_active_bottom_terminal_id(),
        );
        self.state.terminal_layout.active_terminal_id =
            restore_plan.active_terminal_id.clone().unwrap_or_default();
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "plan elapsed_ms={} owner={} tabs={} active_index={} active_runtime={} active_bottom={}",
                plan_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none"),
                restore_plan.tabs.len(),
                restore_plan.active_index,
                restore_plan.active_terminal_id.as_deref().unwrap_or("none"),
                restore_plan
                    .active_bottom_terminal_id
                    .as_deref()
                    .unwrap_or("none")
            ),
        );
        let artifacts_started_at = Instant::now();
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "artifacts elapsed_ms={} owner={}",
                artifacts_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none")
            ),
        );
        let launch_context = self.current_terminal_launch_context();
        let base_pty_config = launch_context
            .as_ref()
            .map(TerminalLaunchContext::to_config)
            .unwrap_or_default();
        let terminal_config = self.terminal_config_from_settings();
        let spawn_started_at = Instant::now();
        match spawn_terminal_tabs(
            &restore_plan,
            self.terminal_manager.clone(),
            launch_context.as_ref(),
            &base_pty_config,
            terminal_config,
            &self.terminal_pane_registry,
            cx,
        ) {
            Ok((terminals, active_terminal_id, next_terminal_index)) => {
                let tab_count = terminals.len();
                self.terminals = terminals;
                self.active_terminal_id = active_terminal_id;
                self.next_terminal_index = next_terminal_index;
                self.register_terminal_panes(cx);
                self.status_message = format!(
                    "terminal layout reloaded · {} tab{}",
                    self.terminals.len(),
                    if self.terminals.len() == 1 { "" } else { "s" }
                );
                self.runtime_trace(
                    "terminal-restore",
                    &format!(
                        "spawn_tabs elapsed_ms={} owner={} tabs={}",
                        spawn_started_at.elapsed().as_millis(),
                        owner_id.as_deref().unwrap_or("none"),
                        tab_count
                    ),
                );
                self.sync_terminal_state_for_project_switch();
            }
            Err(error) => {
                self.status_message = format!("failed to rebuild terminal layout: {error}");
                self.runtime_trace(
                    "terminal-restore",
                    &format!(
                        "spawn_tabs failed elapsed_ms={} owner={} error={error}",
                        spawn_started_at.elapsed().as_millis(),
                        owner_id.as_deref().unwrap_or("none")
                    ),
                );
            }
        }
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "total elapsed_ms={} owner={}",
                restore_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none")
            ),
        );
        self.invalidate_terminal_workspace(cx);
    }

    pub(super) fn remove_worktree_by_id(
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

        match self.runtime_service.remove_worktree(
            &project.id,
            &project.path,
            &worktree_id,
            remove_branch,
        ) {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = self.text("worktree.remove.success", "Removed worktree.");
            }
            Err(error) => {
                let title = self.text("worktree.remove.title", "Remove Worktree");
                self.status_message = title.clone();
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_worktree_context(cx);
    }

    pub(super) fn request_remove_worktree_by_id(
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

    pub(super) fn merge_worktree_by_id(&mut self, worktree_id: String, cx: &mut Context<Self>) {
        self.request_merge_worktree_by_id(worktree_id, cx);
    }

    pub(super) fn request_merge_worktree_by_id(
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

    pub(super) fn select_worktree(
        &mut self,
        worktree_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project for worktree selection".to_string();
            self.invalidate_status_bar(cx);
            return;
        };
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
        self.remember_active_bottom_terminal_for_current_scope();
        self.sync_terminal_state_for_project_switch();
        self.state.worktrees.selected_worktree_id = Some(worktree_id.clone());
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
        self.terminal_layout_loading = true;
        self.status_message = format!("selected worktree: {worktree_id}");
        if let Some(key) = current_worktree_scope_key(&self.state) {
            let storage_key = super::app_state::worktree_terminal_storage_key(&key);
            let terminal_layout = self
                .runtime_service
                .reload_terminal_layout(Some(&storage_key));
            self.runtime_trace(
                "terminal-layout",
                &format!(
                    "select_sync_layout key={} bottom_ratio={} top={} tabs={}",
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
                    let terminal_layout = runtime_service.reload_terminal_layout(Some(
                        &super::app_state::worktree_terminal_storage_key(&scope_key),
                    ));
                    Ok::<_, String>(WorktreeSwitchLoad {
                        project_id: project.id,
                        generation,
                        scope_key,
                        ai_history,
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
                    app.runtime_trace(
                        "workspace-switch",
                        "load_failed_or_cancelled project=unknown worktree=unknown",
                    );
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
                "worktrees={} tasks={} loaded_top_panes={} loaded_bottom_tabs={}",
                self.state.worktrees.worktrees.len(),
                self.state.worktrees.tasks.len(),
                load.terminal_layout.top_panes.len(),
                load.terminal_layout.tabs.len()
            ),
        );
        self.state.ai_history = load.ai_history;
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

    pub(super) fn current_terminal_launch_context(&self) -> Option<TerminalLaunchContext> {
        terminal_launch_context(&self.state, &self.runtime, &self.state.tool_permissions)
    }

    pub(super) fn current_terminal_base_pty_config(&self) -> TerminalPtyConfig {
        self.current_terminal_launch_context()
            .map(|context| context.to_config())
            .unwrap_or_default()
    }

    pub(super) fn terminal_pty_config_for_slot(
        &self,
        slot: &TerminalPaneSlot,
    ) -> TerminalPtyConfig {
        terminal_pty_config_for_terminal_id(
            &self.current_terminal_base_pty_config(),
            slot.terminal_id.as_deref(),
            &slot.title,
        )
    }

    pub(super) fn active_terminal(&self) -> Option<&TerminalTab> {
        self.terminals
            .iter()
            .find(|tab| tab.id == self.active_terminal_id)
            .or_else(|| self.terminals.first())
    }

    pub(super) fn active_terminal_runtime_id(&self) -> String {
        self.active_terminal_slot()
            .and_then(|(_, slot)| slot.terminal_id.clone())
            .or_else(|| {
                self.active_terminal()
                    .and_then(|tab| tab.terminal_id.clone())
            })
            .unwrap_or_default()
    }

    pub(super) fn set_active_terminal_runtime_id(&mut self, terminal_id: Option<&str>) -> bool {
        let Some(terminal_id) = terminal_id
            .map(str::trim)
            .filter(|terminal_id| !terminal_id.is_empty())
        else {
            return false;
        };
        let changed = self.state.terminal_layout.active_terminal_id != terminal_id;
        self.state.terminal_layout.active_terminal_id = terminal_id.to_string();
        changed
    }

    pub(super) fn select_active_terminal_runtime_id(&mut self, terminal_id: Option<&str>) -> bool {
        self.set_active_terminal_runtime_id(terminal_id)
    }

    pub(super) fn activate_first_terminal(&mut self) {
        self.normalize_active_bottom_terminal_id();
        let Some(terminal_id) = self
            .terminals
            .iter()
            .find(|tab| tab.placement == TerminalTabPlacement::Top)
            .and_then(|tab| tab.panes.first().and_then(|slot| slot.terminal_id.clone()))
            .or_else(|| {
                self.active_bottom_terminal()
                    .and_then(|tab| tab.panes.first())
                    .and_then(|slot| slot.terminal_id.clone())
            })
            .or_else(|| {
                self.terminals
                    .first()
                    .and_then(|tab| tab.panes.first())
                    .and_then(|slot| slot.terminal_id.clone())
            })
        else {
            return;
        };
        self.set_active_terminal_runtime_id(Some(&terminal_id));
    }

    pub(super) fn active_terminal_slot(&self) -> Option<(&TerminalTab, &TerminalPaneSlot)> {
        let (tab_index, slot_index) = active_terminal_slot_indices(
            &self.terminals,
            &self.state.terminal_layout.active_terminal_id,
            self.active_terminal_id,
        )?;
        self.terminals
            .get(tab_index)
            .and_then(|tab| tab.panes.get(slot_index).map(|slot| (tab, slot)))
    }

    pub(super) fn active_terminal_slot_mut(&mut self) -> Option<(&mut TerminalTab, usize)> {
        let (tab_index, slot_index) = active_terminal_slot_indices(
            &self.terminals,
            &self.state.terminal_layout.active_terminal_id,
            self.active_terminal_id,
        )?;
        self.terminals
            .get_mut(tab_index)
            .map(|tab| (tab, slot_index))
    }

    pub(super) fn active_terminal_view(&self) -> Option<gpui::Entity<TerminalView>> {
        self.active_terminal_slot()
            .map(|(_, slot)| slot)
            .and_then(|slot| slot.pane.as_ref())
            .map(|pane| pane.view.clone())
    }

    pub(super) fn focus_active_terminal_view(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some((_, slot)) = self.active_terminal_slot() else {
            return false;
        };
        let Some(pane) = slot.pane.as_ref() else {
            return false;
        };
        let terminal_id = slot.terminal_id.clone();
        let view = pane.view.clone();
        view.read(cx).focus_handle().focus(window, cx);
        if let Some(terminal_id) = terminal_id {
            self.record_focused_terminal_runtime_id(&terminal_id, cx);
        }
        true
    }

    pub(super) fn focus_active_terminal(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        self.focus_active_terminal_view(window, cx)
    }

    pub(super) fn focused_terminal_view(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<gpui::Entity<TerminalView>> {
        self.terminals
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .filter_map(|slot| slot.pane.as_ref())
            .find_map(|pane| {
                if pane.view.read(cx).is_focused(window) {
                    Some(pane.view.clone())
                } else {
                    None
                }
            })
    }

    pub(super) fn focused_terminal_runtime_id(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<String> {
        self.terminals.iter().find_map(|tab| {
            tab.panes.iter().find_map(|slot| {
                let pane = slot.pane.as_ref()?;
                if pane.view.read(cx).is_focused(window) {
                    slot.terminal_id.clone().or_else(|| tab.terminal_id.clone())
                } else {
                    None
                }
            })
        })
    }
}

#[derive(Clone)]
pub(super) struct TerminalLayoutSnapshot {
    pub(super) tabs: Vec<TerminalTabSummary>,
    pub(super) top_panes: Vec<TerminalPaneSummary>,
    pub(super) top_ratios: Vec<f64>,
    pub(super) bottom_ratio: f64,
}

pub(in crate::app) fn active_terminal_slot_indices(
    terminals: &[TerminalTab],
    active_terminal_id: &str,
    active_tab_id: usize,
) -> Option<(usize, usize)> {
    let active_terminal_id = active_terminal_id.trim();
    if !active_terminal_id.is_empty() {
        for (tab_index, tab) in terminals.iter().enumerate() {
            if let Some(slot_index) = tab
                .panes
                .iter()
                .position(|slot| slot.terminal_id.as_deref() == Some(active_terminal_id))
            {
                return Some((tab_index, slot_index));
            }
        }
    }

    let tab_index = terminals
        .iter()
        .position(|tab| tab.id == active_tab_id)
        .or_else(|| (!terminals.is_empty()).then_some(0))?;
    (!terminals[tab_index].panes.is_empty()).then_some((tab_index, 0))
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

fn terminal_runtime_summary_from_inputs(
    existing: &TerminalRuntimeSummary,
    active_terminal_id: String,
    sessions: Vec<TerminalRuntimeSessionInput>,
) -> TerminalRuntimeSummary {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default();
    let created_at_by_key = existing
        .sessions
        .iter()
        .map(|session| (session.terminal_id.clone(), session.created_at))
        .collect::<HashMap<_, _>>();
    let sessions = sessions
        .into_iter()
        .map(|session| TerminalRuntimeSessionSummary {
            created_at: created_at_by_key
                .get(&session.terminal_id)
                .copied()
                .unwrap_or(now),
            terminal_id: session.terminal_id,
            title: session.title,
            project_id: session.project_id,
            project_name: session.project_name,
            project_path: session.project_path,
            cwd: session.cwd,
            status: "running".to_string(),
            is_running: true,
            last_active_at: now,
            has_buffer: false,
            buffer_characters: 0,
            input_bytes: session.input_bytes,
            last_input_at: session.input_history.last().map(|input| input.timestamp),
            input_history: session.input_history,
            output_bytes: session.output_bytes,
            output_tail: session.output_tail,
        })
        .collect::<Vec<_>>();
    let open_count = sessions.len();
    TerminalRuntimeSummary {
        path: String::new(),
        active_terminal_id,
        open_count,
        closed_count: 0,
        sessions,
        error: None,
    }
}
