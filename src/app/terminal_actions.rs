use super::*;
use uuid::Uuid;

impl CoduxApp {
    pub(in crate::app) fn add_terminal_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let owner_id = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
            .unwrap_or("unscoped");
        let id = self.next_terminal_index;
        let tab_number = self.bottom_terminals().count() + 1;
        let title = format!("Tab {tab_number}");
        let pane_plan = TerminalPanePlan {
            source_id: Some(bottom_slot_id(owner_id, tab_number.saturating_sub(1))),
            terminal_id: Some(bottom_terminal_id(owner_id, tab_number.saturating_sub(1))),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_context = terminal_pane_launch_context(launch_context.as_ref(), id, 0, &pane_plan);
        match TerminalPane::spawn_with_context_and_config(
            cx,
            self.terminal_manager.clone(),
            pane_context.as_ref(),
            self.terminal_config_from_settings(),
        ) {
            Ok(pane) => {
                self.refresh_terminal_slot_snapshots();
                self.next_terminal_index += 1;
                self.terminals.push(TerminalTab {
                    id,
                    label: title.clone(),
                    source_id: pane_plan.source_id.clone(),
                    terminal_id: pane_plan.terminal_id.clone(),
                    panes: vec![TerminalPaneSlot {
                        title: title.clone(),
                        launch_context: pane_context,
                        pane: Some(pane),
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    }],
                });
                self.active_terminal_id = id;
                self.detach_inactive_terminal_views();
                if let Some(view) = self.active_bottom_terminal_view() {
                    view.read(cx).focus_handle().focus(window, cx);
                }
                self.status_message = format!("terminal tab added: {title}");
                self.sync_terminal_state_for_background_persist(cx);
                self.invalidate_terminal_workspace(cx);
            }
            Err(error) => eprintln!("failed to create terminal tab: {error}"),
        }
    }

    pub(in crate::app) fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let Some(active_tab) = self.main_terminal() else {
            return;
        };
        if active_tab.panes.len() >= 6 {
            self.status_message = "main split limit reached: 6 panes".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let tab_id = active_tab.id;
        let pane_index = active_tab.panes.len();
        let owner_id = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
            .unwrap_or("unscoped");
        let title = self
            .text("terminal.split.default_format", "Split %d")
            .replace("%d", &(pane_index + 1).to_string());
        let pane_plan = TerminalPanePlan {
            source_id: Some(top_slot_id(owner_id, pane_index)),
            terminal_id: Some(top_terminal_id(owner_id, pane_index)),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_context =
            terminal_pane_launch_context(launch_context.as_ref(), tab_id, pane_index, &pane_plan);
        match TerminalPane::spawn_with_context_and_config(
            cx,
            self.terminal_manager.clone(),
            pane_context.as_ref(),
            self.terminal_config_from_settings(),
        ) {
            Ok(terminal) => {
                terminal.view.read(cx).focus_handle().focus(window, cx);
                if let Some(tab) = self.main_terminal_mut() {
                    tab.panes.push(TerminalPaneSlot {
                        title,
                        launch_context: pane_context,
                        pane: Some(terminal),
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    });
                }
                self.status_message = "terminal split added".to_string();
                self.sync_terminal_state_for_background_persist(cx);
                self.invalidate_terminal_workspace(cx);
            }
            Err(error) => eprintln!("failed to split terminal: {error}"),
        }
    }

    pub(in crate::app) fn float_terminal_pane(
        &mut self,
        pane_index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.source_id.is_none())
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.id == self.active_terminal_id)
            })
        else {
            return;
        };
        if self.terminals[tab_index].panes.len() <= 1 {
            self.status_message = "keep at least one main split pane".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        if pane_index >= self.terminals[tab_index].panes.len() {
            return;
        }

        self.refresh_terminal_slot_snapshots();
        let tab_id = self.terminals[tab_index].id;
        let mut slot = self.terminals[tab_index].panes.remove(pane_index);
        let title = slot.title.clone();
        if slot.pane.is_none() {
            let Some(launch_context) = slot.launch_context.clone() else {
                self.terminals[tab_index].panes.insert(pane_index, slot);
                self.status_message =
                    "terminal pane cannot be floated without a stable session".to_string();
                self.invalidate_terminal_workspace(cx);
                return;
            };
            match TerminalPane::spawn_with_context_and_config(
                cx,
                self.terminal_manager.clone(),
                Some(&launch_context),
                self.terminal_config_from_settings(),
            ) {
                Ok(pane) => slot.pane = Some(pane),
                Err(error) => {
                    self.terminals[tab_index].panes.insert(pane_index, slot);
                    self.status_message =
                        format!("failed to attach floating terminal pane: {error}");
                    self.invalidate_terminal_workspace(cx);
                    return;
                }
            }
        }
        self.status_message = format!("terminal pane floated: {title}");
        self.sync_terminal_state_for_background_persist(cx);

        let project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        let pane_view = slot.pane.as_ref().map(|pane| pane.view.clone());
        let app_entity = cx.entity();
        let float_view = terminal_float_window(
            title.clone(),
            app_entity,
            project_id,
            tab_id,
            pane_index,
            slot,
            cx,
        );
        let close_view = float_view.clone();
        let root_view = float_view.clone();
        let restore_view = float_view.clone();
        let focus_view = pane_view.clone();
        let bounds = Bounds::centered(None, size(px(920.0), px(600.0)), cx);
        let window_title = format!("Terminal - {title}");
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_titlebar(window_title.clone())),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.0), px(360.0))),
                ..Default::default()
            },
            move |window, cx| {
                let close_view = close_view.clone();
                window.on_window_should_close(cx, move |_window, cx| {
                    close_view.update(cx, |view, cx| view.restore_to_parent(cx));
                    true
                });
                if let Some(view) = &focus_view {
                    view.read(cx).focus_handle().focus(window, cx);
                }
                cx.new(|cx| Root::new(root_view.clone(), window, cx))
            },
        );
        if let Err(error) = result {
            restore_view.update(cx, |view, cx| view.restore_to_parent(cx));
            self.status_message = format!("failed to float terminal pane: {error}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn restore_floated_terminal_slot(
        &mut self,
        project_id: Option<String>,
        tab_id: usize,
        pane_index: usize,
        slot: TerminalPaneSlot,
        cx: &mut Context<Self>,
    ) {
        let title = slot.title.clone();
        let current_project_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        if project_id != current_project_id {
            self.status_message =
                format!("terminal pane not restored because project changed: {title}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.id == tab_id)
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.source_id.is_none())
            })
        else {
            return;
        };
        let insert_index = pane_index.min(self.terminals[tab_index].panes.len());
        self.terminals[tab_index].panes.insert(insert_index, slot);
        self.status_message = format!("terminal pane restored: {title}");
        self.sync_terminal_state_for_background_persist(cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn close_terminal_pane(
        &mut self,
        pane_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.source_id.is_none())
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.id == self.active_terminal_id)
            })
        else {
            return;
        };
        if self.terminals[tab_index].panes.len() <= 1 {
            self.status_message = "keep at least one main split pane".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        if pane_index >= self.terminals[tab_index].panes.len() {
            return;
        }
        self.refresh_terminal_slot_snapshots();
        let removed = self.terminals[tab_index].panes.remove(pane_index);
        let terminal_id = removed
            .launch_context
            .as_ref()
            .and_then(|context| context.terminal_id.clone())
            .or_else(|| self.terminals[tab_index].terminal_id.clone())
            .unwrap_or_else(|| {
                format!(
                    "gpui-pane-unscoped-{}-{}",
                    self.terminals[tab_index].id,
                    pane_index + 1
                )
            });
        let kill_result = self.kill_terminal_session_if_present(&terminal_id);
        if let Some(view) = self
            .main_terminal()
            .and_then(|tab| tab.panes.last())
            .and_then(|slot| slot.pane.as_ref())
            .map(|pane| pane.view.clone())
        {
            view.read(cx).focus_handle().focus(window, cx);
        }
        self.sync_terminal_state_for_background_persist(cx);
        if let Err(error) = kill_result {
            self.status_message = format!("terminal split closed; PTY cleanup failed: {error}");
        } else {
            self.status_message = "terminal split closed".to_string();
        }
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn send_to_active_terminal(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.ensure_active_terminal_mounted(cx) {
            self.status_message = format!("failed to mount active terminal: {error}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.id == self.active_terminal_id)
        else {
            self.status_message = "no active terminal".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let Some(slot_index) = self.terminals[tab_index].panes.len().checked_sub(1) else {
            self.status_message = "active terminal has no pane".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let result = self.terminals[tab_index].panes[slot_index]
            .pane
            .as_ref()
            .expect("active terminal pane should be mounted")
            .send_text(text);
        match result {
            Ok(()) => {
                let tab_label = self.terminals[tab_index].label.clone();
                self.status_message = format!("sent command to {tab_label}");
                self.sync_terminal_state_for_background_persist(cx);
            }
            Err(error) => {
                self.status_message = format!("failed to send terminal command: {error}");
            }
        }
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn restore_ai_session_in_main_split(
        &mut self,
        title: String,
        command: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        prepare_memory_launch_artifacts(&self.state);
        let launch_context = self.current_terminal_launch_context();
        let Some(active_tab) = self.main_terminal() else {
            self.status_message = "no main terminal to restore session".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };
        if active_tab.panes.len() >= 6 {
            self.status_message = "main split limit reached: 6 panes".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }

        let tab_id = active_tab.id;
        let pane_index = active_tab.panes.len();
        let owner_id = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
            .unwrap_or("unscoped");
        let terminal_id = format!("gpui-term-{owner_id}-ai-restore-{}", Uuid::new_v4());
        let slot_id = format!("gpui-pane-{owner_id}-ai-restore-{}", Uuid::new_v4());
        let pane_plan = TerminalPanePlan {
            source_id: Some(slot_id),
            terminal_id: Some(terminal_id),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_context =
            terminal_pane_launch_context(launch_context.as_ref(), tab_id, pane_index, &pane_plan);
        match TerminalPane::spawn_with_context_and_config(
            cx,
            self.terminal_manager.clone(),
            pane_context.as_ref(),
            self.terminal_config_from_settings(),
        ) {
            Ok(terminal) => {
                let send_result = terminal.send_text(&format!("{command}\n"));
                terminal.view.read(cx).focus_handle().focus(window, cx);
                if let Some(tab) = self.main_terminal_mut() {
                    tab.panes.push(TerminalPaneSlot {
                        title,
                        launch_context: pane_context,
                        pane: Some(terminal),
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    });
                }
                if let Err(error) = send_result {
                    self.status_message =
                        format!("AI session split created; restore send failed: {error}");
                } else {
                    self.status_message = "AI session restored in main split".to_string();
                }
                self.sync_terminal_state_for_background_persist(cx);
                self.invalidate_terminal_workspace(cx);
            }
            Err(error) => {
                self.status_message = format!("failed to create AI session split: {error}");
                self.invalidate_terminal_workspace(cx);
            }
        }
    }

    pub(in crate::app) fn close_terminal_tab(
        &mut self,
        terminal_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self.terminals.iter().position(|tab| tab.id == terminal_id) else {
            return;
        };
        self.refresh_terminal_slot_snapshots();
        let removed = self.terminals.remove(index);
        let kill_errors = removed
            .panes
            .iter()
            .enumerate()
            .filter_map(|(pane_index, slot)| {
                let terminal_id = Self::terminal_slot_terminal_id(&removed, pane_index, slot);
                self.kill_terminal_session_if_present(&terminal_id).err()
            })
            .collect::<Vec<_>>();
        self.active_terminal_id = self
            .terminals
            .get(index.saturating_sub(1))
            .filter(|tab| tab.source_id.is_some())
            .or_else(|| self.bottom_terminals().next())
            .map(|tab| tab.id)
            .unwrap_or(0);
        let mount_result = self.ensure_active_terminal_mounted(cx);
        self.detach_inactive_terminal_views();
        if let Some(view) = self.active_bottom_terminal_view() {
            view.read(cx).focus_handle().focus(window, cx);
        }
        self.sync_terminal_state_for_background_persist(cx);
        if let Err(error) = mount_result {
            self.status_message = format!("terminal tab closed; mount failed: {error}");
        } else if let Some(error) = kill_errors.first() {
            self.status_message = format!("terminal tab closed; PTY cleanup failed: {error}");
        } else {
            self.status_message = "terminal tab closed".to_string();
        }
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn select_terminal_tab(
        &mut self,
        terminal_id: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_terminal_slot_snapshots();
        self.active_terminal_id = terminal_id;
        if let Err(error) = self.ensure_active_terminal_mounted(cx) {
            self.status_message = format!("failed to select terminal: {error}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        if let Some(view) = self.active_bottom_terminal_view() {
            view.read(cx).focus_handle().focus(window, cx);
        }
        self.detach_inactive_terminal_views();
        self.sync_terminal_state_for_background_persist(cx);
        self.invalidate_terminal_workspace(cx);
    }
}
