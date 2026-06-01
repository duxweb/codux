use super::*;

impl CoduxApp {
    pub(super) fn ensure_active_terminal_mounted(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.source_id.is_some() && tab.id == self.active_terminal_id)
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.source_id.is_none())
            })
            .or_else(|| (!self.terminals.is_empty()).then_some(0))
        else {
            return Ok(());
        };
        let config = self.terminal_config_from_settings();
        let terminal_manager = self.terminal_manager.clone();
        let tab = &mut self.terminals[tab_index];
        for slot in &mut tab.panes {
            if slot.pane.is_some() {
                continue;
            }
            let pane = TerminalPane::spawn_with_context_and_config(
                cx,
                terminal_manager.clone(),
                slot.launch_context.as_ref(),
                config.clone(),
            )
            .map_err(|error| error.to_string())?;
            slot.pane = Some(pane);
        }
        Ok(())
    }

    pub(super) fn refresh_terminal_slot_snapshots(&mut self) {
        let terminal_manager = self.terminal_manager.clone();
        for tab in &mut self.terminals {
            let tab_id = tab.id;
            let tab_terminal_id = tab.terminal_id.clone();
            for slot in &mut tab.panes {
                if let Some(pane) = &slot.pane {
                    let output = pane.output_snapshot();
                    slot.restored_output_bytes = output.bytes;
                    slot.restored_output_tail = output.tail;
                    continue;
                }
                let terminal_id = slot
                    .launch_context
                    .as_ref()
                    .and_then(|context| context.terminal_id.clone())
                    .or_else(|| tab_terminal_id.clone())
                    .unwrap_or_else(|| format!("gpui-term-unscoped-{tab_id}"));
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
            if tab.source_id.is_none() || tab.id == self.active_terminal_id {
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
            .find(|tab| tab.source_id.is_none())
            .or_else(|| self.active_terminal())
    }

    pub(super) fn main_terminal_mut(&mut self) -> Option<&mut TerminalTab> {
        let index = self
            .terminals
            .iter()
            .position(|tab| tab.source_id.is_none())
            .or_else(|| {
                let active_id = self.active_terminal_id;
                self.terminals.iter().position(|tab| tab.id == active_id)
            })
            .or_else(|| (!self.terminals.is_empty()).then_some(0))?;
        self.terminals.get_mut(index)
    }

    pub(super) fn bottom_terminals(&self) -> impl Iterator<Item = &TerminalTab> {
        self.terminals.iter().filter(|tab| tab.source_id.is_some())
    }

    pub(super) fn active_bottom_terminal(&self) -> Option<&TerminalTab> {
        self.bottom_terminals()
            .find(|tab| tab.id == self.active_terminal_id)
            .or_else(|| self.bottom_terminals().next())
    }

    pub(super) fn active_bottom_terminal_view(&self) -> Option<gpui::Entity<TerminalView>> {
        self.active_bottom_terminal()
            .and_then(|tab| tab.panes.first())
            .and_then(|slot| slot.pane.as_ref())
            .map(|pane| pane.view.clone())
    }

    pub(super) fn terminal_slot_terminal_id(
        tab: &TerminalTab,
        _pane_index: usize,
        slot: &TerminalPaneSlot,
    ) -> String {
        slot.launch_context
            .as_ref()
            .and_then(|context| context.terminal_id.clone())
            .or_else(|| tab.terminal_id.clone())
            .unwrap_or_else(|| format!("gpui-term-unscoped-{}", tab.id))
    }

    pub(super) fn terminal_slot_slot_id(
        tab: &TerminalTab,
        pane_index: usize,
        slot: &TerminalPaneSlot,
    ) -> String {
        slot.launch_context
            .as_ref()
            .and_then(|context| context.slot_id.clone())
            .or_else(|| tab.source_id.clone())
            .unwrap_or_else(|| format!("gpui-pane-unscoped-{}-{}", tab.id, pane_index + 1))
    }

    pub(super) fn kill_terminal_session_if_present(&self, terminal_id: &str) -> Result<(), String> {
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

    pub(super) fn save_terminal_layout(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        match self.persist_terminal_layout() {
            Ok(()) => self.status_message = "terminal layout saved to state.json".to_string(),
            Err(error) => self.status_message = error,
        }
        cx.notify();
    }

    pub(super) fn persist_terminal_layout(&mut self) -> Result<(), String> {
        let Some(owner_id) = super::ai_runtime_status::terminal_layout_owner_id(&self.state) else {
            return Err("no selected project to save terminal layout".to_string());
        };
        let (tabs, active_tab_id, top_panes, active_slot_id) = self.terminal_layout_snapshot();
        let raw_layout = self.runtime_service.save_terminal_layout(
            &owner_id,
            tabs,
            active_tab_id,
            top_panes,
            active_slot_id,
        )?;
        let (layout, runtime) = normalize_terminal_restore_state(
            Some(&owner_id),
            raw_layout,
            self.state.terminal_runtime.clone(),
        );
        self.state.terminal_layout = layout;
        self.state.terminal_runtime = runtime;
        self.save_current_terminal_view_state();
        Ok(())
    }

    pub(super) fn persist_terminal_runtime(&mut self) -> Result<(), String> {
        self.refresh_terminal_slot_snapshots();
        let (active_terminal_id, active_slot_id, sessions) = self.terminal_runtime_snapshot();
        let raw_runtime = TerminalRuntimeService::new(self.state.support_dir.clone())
            .save_from_gpui(active_terminal_id, active_slot_id, sessions)?;
        let (layout, runtime) = normalize_terminal_restore_state(
            super::ai_runtime_status::terminal_layout_owner_id(&self.state).as_deref(),
            self.state.terminal_layout.clone(),
            raw_runtime,
        );
        self.state.terminal_layout = layout;
        self.state.terminal_runtime = runtime;
        self.save_current_terminal_view_state();
        Ok(())
    }

    pub(super) fn terminal_runtime_snapshot(
        &self,
    ) -> (String, String, Vec<TerminalRuntimeSessionInput>) {
        let active = self.active_terminal();
        let active_terminal_id = active
            .and_then(|tab| {
                tab.panes
                    .last()
                    .and_then(|slot| slot.launch_context.as_ref())
                    .and_then(|context| context.terminal_id.clone())
                    .or_else(|| tab.terminal_id.clone())
            })
            .unwrap_or_else(|| format!("gpui-term-unscoped-{}", self.active_terminal_id));
        let active_slot_id = active
            .and_then(|tab| {
                tab.panes
                    .last()
                    .and_then(|slot| slot.launch_context.as_ref())
                    .and_then(|context| context.slot_id.clone())
                    .or_else(|| tab.source_id.clone())
            })
            .unwrap_or_else(|| format!("bottom-unscoped-{}", self.active_terminal_id));
        let sessions = self
            .terminals
            .iter()
            .flat_map(|tab| {
                tab.panes.iter().enumerate().map(|(pane_index, slot)| {
                    let context = slot.launch_context.as_ref();
                    let project = self.state.selected_project.as_ref();
                    let terminal_id = Self::terminal_slot_terminal_id(tab, pane_index, slot);
                    let slot_id = Self::terminal_slot_slot_id(tab, pane_index, slot);
                    let project_id = context
                        .map(|context| context.project_id.clone())
                        .or_else(|| project.map(|project| project.id.clone()))
                        .unwrap_or_default();
                    let project_name = context
                        .map(|context| context.project_name.clone())
                        .or_else(|| project.map(|project| project.name.clone()))
                        .unwrap_or_default();
                    let project_path = context
                        .map(|context| context.project_path.display().to_string())
                        .or_else(|| project.map(|project| project.path.clone()))
                        .unwrap_or_default();
                    let cwd = context
                        .and_then(|context| context.session_cwd.as_ref())
                        .map(|cwd| cwd.display().to_string())
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
                    let input_bytes = input.as_ref().map(|input| input.bytes).unwrap_or_default();
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
                    TerminalRuntimeSessionInput {
                        terminal_id,
                        slot_id,
                        tab_id: tab
                            .source_id
                            .clone()
                            .unwrap_or_else(|| format!("bottom-unscoped-{}", tab.id)),
                        pane_index,
                        title: slot.title.clone(),
                        project_id,
                        project_name,
                        project_path,
                        cwd,
                        input_bytes,
                        input_history,
                        output_bytes,
                        output_tail,
                    }
                })
            })
            .collect();
        (active_terminal_id, active_slot_id, sessions)
    }

    pub(super) fn terminal_layout_snapshot(
        &self,
    ) -> (
        Vec<TerminalTabSummary>,
        String,
        Vec<TerminalPaneSummary>,
        String,
    ) {
        let tabs = self
            .terminals
            .iter()
            .filter(|tab| tab.source_id.is_some())
            .map(terminal_tab_summary)
            .collect::<Vec<_>>();
        let top_panes = self
            .main_terminal()
            .map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .map(|(index, slot)| terminal_pane_summary(tab.id, index, slot))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let active_tab_id = self
            .active_terminal()
            .and_then(|tab| tab.source_id.clone())
            .or_else(|| {
                tabs.iter()
                    .find(|tab| tab.id == format!("bottom-{}", self.active_terminal_id))
                    .or_else(|| {
                        tabs.iter().find(|tab| {
                            tab.id == format!("bottom-unscoped-{}", self.active_terminal_id)
                        })
                    })
                    .map(|tab| tab.id.clone())
            })
            .unwrap_or_else(|| format!("bottom-unscoped-{}", self.active_terminal_id));
        let active_slot_id = active_tab_id.clone();
        (tabs, active_tab_id, top_panes, active_slot_id)
    }

    pub(super) fn reload_terminal_layout(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let terminal_layout = self.runtime_service.reload_terminal_layout(
            super::ai_runtime_status::terminal_layout_owner_id(&self.state).as_deref(),
        );
        let terminal_runtime = self.runtime_service.reload_terminal_runtime();
        self.apply_terminal_layout_from_summary(terminal_layout, terminal_runtime, cx);
        if let Some(view) = self.active_terminal_view() {
            view.read(cx).focus_handle().focus(window, cx);
        }
    }

    pub(super) fn apply_terminal_layout_from_summary(
        &mut self,
        terminal_layout: TerminalLayoutSummary,
        terminal_runtime: TerminalRuntimeSummary,
        cx: &mut Context<Self>,
    ) {
        self.terminal_layout_loading = false;
        let owner_id = super::ai_runtime_status::terminal_layout_owner_id(&self.state);
        let (terminal_layout, terminal_runtime) = normalize_terminal_restore_state(
            owner_id.as_deref(),
            terminal_layout,
            terminal_runtime,
        );
        self.state.terminal_layout = terminal_layout;
        self.state.terminal_runtime = terminal_runtime;
        let restore_plan = terminal_restore_plan_for_language(
            &self.state.terminal_layout,
            &self.state.terminal_runtime,
            &self.state.settings.language,
        );
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let terminal_config = self.terminal_config_from_settings();
        match spawn_terminal_tabs(
            &restore_plan,
            self.terminal_manager.clone(),
            launch_context.as_ref(),
            terminal_config,
            cx,
        ) {
            Ok((terminals, active_terminal_id, next_terminal_index)) => {
                self.terminals = terminals;
                self.active_terminal_id = active_terminal_id;
                self.next_terminal_index = next_terminal_index;
                self.status_message = format!(
                    "terminal layout reloaded · {} tab{}",
                    self.terminals.len(),
                    if self.terminals.len() == 1 { "" } else { "s" }
                );
            }
            Err(error) => {
                self.status_message = format!("failed to rebuild terminal layout: {error}");
            }
        }
        cx.notify();
    }

    pub(super) fn reload_worktrees(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.state.worktrees = self.runtime_service.reload_worktrees(
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.id.as_str()),
            self.state
                .selected_project
                .as_ref()
                .map(|project| project.path.as_str()),
        );
        self.status_message = "worktrees reloaded".to_string();
        self.save_current_project_view_state();
        self.notify_task_column(cx);
        cx.notify();
    }

    pub(super) fn sync_worktrees_from_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to sync worktrees".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .sync_worktrees_from_git(&project.id, &project.path)
        {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = "worktrees synced from Git".to_string();
                self.save_current_project_view_state();
                self.notify_task_column(cx);
            }
            Err(error) => self.status_message = format!("failed to sync worktrees: {error}"),
        }
        cx.notify();
    }

    pub(super) fn create_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to create worktree".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .create_worktree(&project.id, &project.path)
        {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = "worktree created".to_string();
                self.save_current_project_view_state();
                self.notify_task_column(cx);
            }
            Err(error) => self.status_message = format!("failed to create worktree: {error}"),
        }
        cx.notify();
    }

    pub(super) fn remove_selected_worktree(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_selected_worktree_with_options(false, cx);
    }

    pub(super) fn remove_selected_worktree_and_branch(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_selected_worktree_with_options(true, cx);
    }

    pub(super) fn remove_selected_worktree_with_options(
        &mut self,
        remove_branch: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to remove worktree".to_string();
            cx.notify();
            return;
        };
        let Some(worktree_id) = self.state.worktrees.selected_worktree_id.clone() else {
            self.status_message = "no selected worktree to remove".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.remove_worktree(
            &project.id,
            &project.path,
            &worktree_id,
            remove_branch,
        ) {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = if remove_branch {
                    format!("worktree and branch removed: {worktree_id}")
                } else {
                    format!("worktree removed: {worktree_id}")
                };
                self.save_current_project_view_state();
                self.notify_task_column(cx);
            }
            Err(error) => self.status_message = format!("failed to remove worktree: {error}"),
        }
        cx.notify();
    }

    pub(super) fn merge_selected_worktree(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to merge worktree".to_string();
            cx.notify();
            return;
        };
        let Some(worktree_id) = self.state.worktrees.selected_worktree_id.clone() else {
            self.status_message = "no selected worktree to merge".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .merge_worktree(&project.id, &project.path, &worktree_id)
        {
            Ok(summary) => {
                self.state.worktrees = summary;
                self.status_message = format!("worktree merged: {worktree_id}");
                self.save_current_project_view_state();
                self.notify_task_column(cx);
            }
            Err(error) => self.status_message = format!("failed to merge worktree: {error}"),
        }
        cx.notify();
    }

    pub(super) fn select_worktree(
        &mut self,
        worktree_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project for worktree selection".to_string();
            cx.notify();
            return;
        };
        if self.state.worktrees.selected_worktree_id.as_deref() == Some(worktree_id.as_str()) {
            return;
        }
        match self
            .runtime_service
            .select_worktree(&project.id, &worktree_id)
        {
            Ok(()) => {
                self.save_current_worktree_view_state();
                if let Err(error) = self.persist_terminal_layout() {
                    self.runtime_trace(
                        "terminal-layout",
                        &format!("worktree switch layout save failed: {error}"),
                    );
                }
                if let Err(error) = self.persist_terminal_runtime() {
                    self.runtime_trace(
                        "terminal-runtime",
                        &format!("worktree switch runtime save failed: {error}"),
                    );
                }
                self.state.worktrees = self
                    .runtime_service
                    .reload_worktrees(Some(&project.id), Some(&project.path));
                self.apply_saved_worktree_view_state();
                self.project_switch_generation = self.project_switch_generation.wrapping_add(1);
                let generation = self.project_switch_generation;
                self.state.ai_history = AIHistorySummary {
                    is_loading: true,
                    detail: "loading".to_string(),
                    ..AIHistorySummary::default()
                };
                self.state.ai_session_detail = None;
                self.selected_ai_session_id = None;
                self.refresh_ai_history_after_project_switch(cx);
                self.spawn_worktree_sidebar_load(generation, cx);
                if let Some(key) = terminal_view_store_key(&self.state) {
                    if let Some(view_state) = self.terminal_view_store.get(&key).cloned() {
                        self.apply_terminal_layout_from_summary(
                            view_state.terminal_layout,
                            view_state.terminal_runtime,
                            cx,
                        );
                        self.save_current_terminal_view_state();
                    } else {
                        self.terminal_layout_loading = true;
                        self.state.terminal_layout = TerminalLayoutSummary::default();
                        self.state.terminal_runtime = TerminalRuntimeSummary::default();
                        self.terminals.clear();
                        self.spawn_worktree_terminal_load(project.id.clone(), key, generation, cx);
                    }
                }
                self.status_message = format!("selected worktree: {worktree_id}");
                self.save_current_project_view_state();
                self.notify_task_column(cx);
            }
            Err(error) => self.status_message = format!("failed to select worktree: {error}"),
        }
        cx.notify();
    }

    pub(super) fn spawn_worktree_terminal_load(
        &mut self,
        project_id: String,
        store_key: TerminalViewStoreKey,
        generation: u64,
        cx: &mut Context<Self>,
    ) {
        let runtime_service = self.runtime_service.clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let load = codux_runtime::async_runtime::spawn_blocking({
                let store_key = store_key.clone();
                move || WorktreeSwitchTerminalLoad {
                    terminal_layout: runtime_service
                        .reload_terminal_layout(Some(&store_key.task_id)),
                    terminal_runtime: runtime_service.reload_terminal_runtime(),
                    project_id,
                    generation,
                    store_key,
                }
            })
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                if let Some(load) = load {
                    app.apply_worktree_terminal_load(load, cx);
                }
            });
        })
        .detach();
    }

    pub(super) fn apply_worktree_terminal_load(
        &mut self,
        load: WorktreeSwitchTerminalLoad,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(load.project_id.as_str())
            || terminal_view_store_key(&self.state).as_ref() != Some(&load.store_key)
            || self.project_switch_generation != load.generation
        {
            return;
        }
        self.apply_terminal_layout_from_summary(load.terminal_layout, load.terminal_runtime, cx);
        self.save_current_terminal_view_state();
        cx.notify();
    }

    pub(super) fn preview_file(
        &mut self,
        relative_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to preview".to_string();
            cx.notify();
            return;
        };

        match self
            .runtime_service
            .read_project_file_edit_buffer(&project.path, &relative_path)
        {
            Ok((content, editable)) => {
                self.file_preview = if content.trim().is_empty() && !editable {
                    "(empty file)".to_string()
                } else {
                    content
                };
                self.file_editable = editable;
                self.file_dirty = false;
                self.normalize_file_search_index();
                self.status_message = format!(
                    "{} loaded: {relative_path}",
                    if editable { "editor buffer" } else { "preview" }
                );
            }
            Err(error) => self.status_message = format!("failed to preview file: {error}"),
        }
        cx.notify();
    }

    pub(super) fn current_terminal_launch_context(&self) -> Option<TerminalLaunchContext> {
        terminal_launch_context(&self.state, &self.runtime, &self.state.tool_permissions)
    }

    pub(super) fn active_terminal(&self) -> Option<&TerminalTab> {
        self.terminals
            .iter()
            .find(|tab| tab.id == self.active_terminal_id)
            .or_else(|| self.terminals.first())
    }

    pub(super) fn active_terminal_mut(&mut self) -> Option<&mut TerminalTab> {
        let active_id = self.active_terminal_id;
        if let Some(index) = self.terminals.iter().position(|tab| tab.id == active_id) {
            self.terminals.get_mut(index)
        } else {
            self.terminals.first_mut()
        }
    }

    pub(super) fn active_terminal_view(&self) -> Option<gpui::Entity<TerminalView>> {
        self.active_terminal()
            .and_then(|tab| tab.panes.last())
            .and_then(|slot| slot.pane.as_ref())
            .map(|pane| pane.view.clone())
    }
}
