use super::*;
use crate::app::window_actions::{AuxiliaryWindowSlot, AuxiliaryWindowSpec};
use gpui_component::input::{Input, InputEvent, InputState};

impl CoduxApp {
    pub(in crate::app) fn confirm_or_close_active_terminal_target(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        const CLOSE_CONFIRM_WINDOW: Duration = Duration::from_secs(2);

        let Some(target) = self.active_terminal_close_target(window, cx) else {
            self.pending_terminal_close = None;
            self.status_message = "no terminal split or tab to close".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };

        let now = Instant::now();
        if self.pending_terminal_close.is_some_and(|pending| {
            pending.target == target
                && now.duration_since(pending.requested_at) <= CLOSE_CONFIRM_WINDOW
        }) {
            self.pending_terminal_close = None;
            match target {
                TerminalCloseTarget::Split { pane_index } => {
                    self.close_terminal_pane(pane_index, window, cx);
                }
                TerminalCloseTarget::Tab { terminal_id } => {
                    self.close_terminal_tab(terminal_id, window, cx);
                }
            }
            return;
        }

        self.pending_terminal_close = Some(PendingTerminalClose {
            target,
            requested_at: now,
        });
        let shortcut = if cfg!(target_os = "macos") {
            "Cmd+W"
        } else {
            "Ctrl+W"
        };
        let confirm_message = match target {
            TerminalCloseTarget::Split { .. } => self
                .text(
                    "terminal.close.confirm_split",
                    "Press %@ again to close the current split",
                )
                .replace("%@", shortcut),
            TerminalCloseTarget::Tab { .. } => self
                .text(
                    "terminal.close.confirm_tab",
                    "Press %@ again to close the current tab",
                )
                .replace("%@", shortcut),
        };
        self.show_toast(confirm_message, cx);
        self.invalidate_terminal_workspace(cx);
    }

    fn active_terminal_close_target(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<TerminalCloseTarget> {
        if let Some(focused) = self.focused_terminal_view(window, cx) {
            for tab in &self.terminals {
                for (pane_index, slot) in tab.panes.iter().enumerate() {
                    let Some(pane) = slot.pane.as_ref() else {
                        continue;
                    };
                    if pane.view != focused {
                        continue;
                    }
                    return match tab.placement {
                        TerminalTabPlacement::Bottom => Some(TerminalCloseTarget::Tab {
                            terminal_id: tab.id,
                        }),
                        TerminalTabPlacement::Top => {
                            Some(TerminalCloseTarget::Split { pane_index })
                        }
                    };
                }
            }
        }

        if self.terminals.iter().any(|tab| {
            tab.placement == TerminalTabPlacement::Bottom && tab.id == self.active_terminal_id
        }) {
            return Some(TerminalCloseTarget::Tab {
                terminal_id: self.active_terminal_id,
            });
        }

        let tab = self.main_terminal()?;
        let active_runtime_id = self.active_terminal_runtime_id();
        let pane_index = tab
            .panes
            .iter()
            .position(|slot| {
                !active_runtime_id.is_empty()
                    && slot.terminal_id.as_deref() == Some(active_runtime_id.as_str())
            })
            .unwrap_or(0);
        Some(TerminalCloseTarget::Split { pane_index })
    }

    pub(in crate::app) fn add_terminal_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        let launch_context = self.current_terminal_launch_context();
        let base_pty_config = launch_context
            .as_ref()
            .map(TerminalLaunchContext::to_config)
            .unwrap_or_default();
        let Some(owner_id) = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
        else {
            self.status_message = "no selected workspace for terminal".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let id = self.next_terminal_index;
        let tab_number = self.bottom_terminals().count() + 1;
        let title = format!("Tab {tab_number}");
        let pane_plan = TerminalPanePlan {
            terminal_id: Some(bottom_terminal_id(owner_id, tab_number.saturating_sub(1))),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_terminal_id = terminal_pane_terminal_id(launch_context.as_ref(), &pane_plan);
        let pty_config = terminal_pty_config_for_terminal_id(
            &base_pty_config,
            pane_terminal_id.as_deref(),
            &title,
        );
        match TerminalPane::spawn_with_pty_config(
            cx,
            self.terminal_manager.clone(),
            pty_config,
            self.terminal_config_from_settings(),
        ) {
            Ok(pane) => {
                self.refresh_terminal_slot_snapshots();
                self.register_terminal_pane(pane_terminal_id.as_deref(), &pane, cx);
                self.next_terminal_index += 1;
                let active_runtime_id = pane_terminal_id.clone();
                self.terminals.push(TerminalTab {
                    id,
                    label: title.clone(),
                    placement: TerminalTabPlacement::Bottom,
                    terminal_id: pane_terminal_id.clone(),
                    panes: vec![TerminalPaneSlot {
                        title: title.clone(),
                        terminal_id: pane_terminal_id,
                        pane: Some(pane),
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    }],
                });
                self.active_terminal_id = id;
                self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
                self.detach_inactive_terminal_views();
                self.focus_active_terminal(window, cx);
                self.status_message = format!("terminal tab added: {title}");
                self.sync_terminal_state_after_layout_change(cx);
                self.invalidate_terminal_workspace(cx);
            }
            Err(error) => eprintln!("failed to create terminal tab: {error}"),
        }
    }

    pub(in crate::app) fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        let launch_context = self.current_terminal_launch_context();
        let base_pty_config = launch_context
            .as_ref()
            .map(TerminalLaunchContext::to_config)
            .unwrap_or_default();
        let Some(active_tab) = self.main_terminal() else {
            return;
        };
        if active_tab.panes.len() >= 6 {
            self.status_message = "main split limit reached: 6 panes".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let Some(owner_id) = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
        else {
            self.status_message = "no selected workspace for terminal".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let pane_index = active_tab.panes.len();
        let title = self
            .text("terminal.split.default_format", "Split %d")
            .replace("%d", &(active_tab.panes.len() + 1).to_string());
        let pane_plan = TerminalPanePlan {
            terminal_id: Some(top_terminal_id(owner_id, pane_index)),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_terminal_id = terminal_pane_terminal_id(launch_context.as_ref(), &pane_plan);
        let pty_config = terminal_pty_config_for_terminal_id(
            &base_pty_config,
            pane_terminal_id.as_deref(),
            &title,
        );
        match TerminalPane::spawn_with_pty_config(
            cx,
            self.terminal_manager.clone(),
            pty_config,
            self.terminal_config_from_settings(),
        ) {
            Ok(terminal) => {
                self.register_terminal_pane(pane_terminal_id.as_deref(), &terminal, cx);
                let active_runtime_id = pane_terminal_id.clone();
                terminal.view.read(cx).focus_handle().focus(window, cx);
                if let Some(tab) = self.main_terminal_mut() {
                    tab.panes.push(TerminalPaneSlot {
                        title,
                        terminal_id: pane_terminal_id,
                        pane: Some(terminal),
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    });
                }
                self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
                self.focus_active_terminal(window, cx);
                self.status_message = "terminal split added".to_string();
                self.sync_terminal_state_after_layout_change(cx);
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
            .position(|tab| tab.placement == TerminalTabPlacement::Top)
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
        let tab_view_id = self.terminals[tab_index].id;
        let mut slot = self.terminals[tab_index].panes.remove(pane_index);
        let title = slot.title.clone();
        if slot.pane.is_none() {
            if slot.terminal_id.is_none() {
                self.terminals[tab_index].panes.insert(pane_index, slot);
                self.status_message =
                    "terminal pane cannot be floated without a stable session".to_string();
                self.invalidate_terminal_workspace(cx);
                return;
            }
            let pty_config = self.terminal_pty_config_for_slot(&slot);
            match TerminalPane::spawn_with_pty_config(
                cx,
                self.terminal_manager.clone(),
                pty_config,
                self.terminal_config_from_settings(),
            ) {
                Ok(pane) => {
                    self.register_terminal_pane(slot.terminal_id.as_deref(), &pane, cx);
                    slot.pane = Some(pane);
                }
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
        self.sync_terminal_state_after_layout_change(cx);

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
            tab_view_id,
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
                titlebar: Some(theme::codux_child_titlebar(window_title.clone())),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(640.0), px(360.0))),
                is_minimizable: false,
                ..Default::default()
            },
            move |window, cx| {
                macos_window::configure_child_window_controls(window);
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
        match result {
            Ok(handle) => self.register_child_window_handle(handle.into()),
            Err(error) => {
                restore_view.update(cx, |view, cx| view.restore_to_parent(cx));
                self.status_message = format!("failed to float terminal pane: {error}");
                self.invalidate_terminal_workspace(cx);
                return;
            }
        }
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn restore_floated_terminal_slot(
        &mut self,
        project_id: Option<String>,
        tab_view_id: usize,
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
            .position(|tab| tab.id == tab_view_id)
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.placement == TerminalTabPlacement::Top)
            })
        else {
            return;
        };
        let insert_index = pane_index.min(self.terminals[tab_index].panes.len());
        self.terminals[tab_index].panes.insert(insert_index, slot);
        self.status_message = format!("terminal pane restored: {title}");
        self.sync_terminal_state_after_layout_change(cx);
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
            .position(|tab| tab.placement == TerminalTabPlacement::Top)
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.id == self.active_terminal_id)
            })
        else {
            return;
        };
        if self.terminals[tab_index].panes.len() <= 1 {
            self.reset_terminal_pane(pane_index, window, cx);
            return;
        }
        if pane_index >= self.terminals[tab_index].panes.len() {
            return;
        }
        self.refresh_terminal_slot_snapshots();
        let removed = self.terminals[tab_index].panes.remove(pane_index);
        let terminal_id = removed
            .terminal_id
            .clone()
            .or_else(|| self.terminals[tab_index].terminal_id.clone())
            .unwrap_or_default();
        if terminal_id.trim().is_empty() {
            self.terminals[tab_index].panes.insert(pane_index, removed);
            self.status_message = "terminal split has no terminal id".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let still_referenced = self.terminals.iter().any(|tab| {
            tab.panes.iter().enumerate().any(|(index, slot)| {
                Self::terminal_slot_terminal_id(tab, index, slot).as_deref()
                    == Some(terminal_id.as_str())
            })
        });
        let kill_result = if still_referenced {
            Ok(())
        } else {
            self.kill_terminal_session_if_present(&terminal_id)
        };
        let next_active_terminal_id = self.terminals[tab_index]
            .panes
            .get(pane_index.saturating_sub(1))
            .or_else(|| self.terminals[tab_index].panes.first())
            .and_then(|slot| slot.terminal_id.clone());
        self.select_active_terminal_runtime_id(next_active_terminal_id.as_deref());
        self.focus_active_terminal(window, cx);
        self.sync_terminal_state_after_layout_change(cx);
        if let Err(error) = kill_result {
            self.status_message = format!("terminal split closed; PTY cleanup failed: {error}");
        }
        self.invalidate_terminal_workspace(cx);
    }

    fn reset_terminal_pane(
        &mut self,
        pane_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = self
            .terminals
            .iter()
            .position(|tab| tab.placement == TerminalTabPlacement::Top)
            .or_else(|| {
                self.terminals
                    .iter()
                    .position(|tab| tab.id == self.active_terminal_id)
            })
        else {
            return;
        };
        if pane_index >= self.terminals[tab_index].panes.len() {
            return;
        }
        self.refresh_terminal_slot_snapshots();
        let terminal_id = self.terminals[tab_index].panes[pane_index]
            .terminal_id
            .clone()
            .or_else(|| self.terminals[tab_index].terminal_id.clone())
            .unwrap_or_default();
        if terminal_id.trim().is_empty() {
            self.status_message = "terminal split has no terminal id".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        }
        self.terminals[tab_index].panes[pane_index].pane = None;
        self.terminals[tab_index].panes[pane_index].restored_output_bytes = 0;
        self.terminals[tab_index].panes[pane_index]
            .restored_output_tail
            .clear();
        let kill_result = self.kill_terminal_session_if_present(&terminal_id);
        self.select_active_terminal_runtime_id(Some(&terminal_id));
        let mount_result = self.ensure_active_terminal_mounted(cx);
        self.detach_inactive_terminal_views();
        self.focus_active_terminal(window, cx);
        self.sync_terminal_state_after_layout_change(cx);
        if let Err(error) = kill_result {
            self.status_message = format!("terminal reset; PTY cleanup failed: {error}");
        } else if let Err(error) = mount_result {
            self.status_message = format!("terminal reset; mount failed: {error}");
        } else {
            self.status_message = "terminal reset".to_string();
        }
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn send_to_active_terminal(&mut self, text: &str, cx: &mut Context<Self>) {
        if let Err(error) = self.ensure_active_terminal_mounted(cx) {
            self.status_message = format!("failed to mount active terminal: {error}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        let (result, tab_label) = {
            let Some((tab, slot_index)) = self.active_terminal_slot_mut() else {
                self.status_message = "active terminal has no pane".to_string();
                self.invalidate_terminal_workspace(cx);
                return;
            };
            let result = tab.panes[slot_index]
                .pane
                .as_ref()
                .expect("active terminal pane should be mounted")
                .send_text(text);
            (result, tab.label.clone())
        };
        match result {
            Ok(()) => {
                self.status_message = format!("sent command to {tab_label}");
                self.sync_terminal_state_after_layout_change(cx);
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
        self.restore_ai_session_in_main_split_internal(title, command, Some(window), cx);
    }

    pub(in crate::app) fn restore_ai_session_in_main_split_without_focus(
        &mut self,
        title: String,
        command: String,
        cx: &mut Context<Self>,
    ) {
        self.restore_ai_session_in_main_split_internal(title, command, None, cx);
    }

    fn restore_ai_session_in_main_split_internal(
        &mut self,
        title: String,
        command: String,
        mut window: Option<&mut Window>,
        cx: &mut Context<Self>,
    ) {
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        let launch_context = self.current_terminal_launch_context();
        let base_pty_config = launch_context
            .as_ref()
            .map(TerminalLaunchContext::to_config)
            .unwrap_or_default();
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

        let Some(owner_id) = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
        else {
            self.status_message = "no selected workspace for terminal".to_string();
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let pane_index = active_tab.panes.len();
        let pane_plan = TerminalPanePlan {
            terminal_id: Some(top_terminal_id(owner_id, pane_index)),
            title: title.clone(),
            restored_output_bytes: 0,
            restored_output_tail: String::new(),
        };
        let pane_terminal_id = terminal_pane_terminal_id(launch_context.as_ref(), &pane_plan);
        let pty_config = terminal_pty_config_for_terminal_id(
            &base_pty_config,
            pane_terminal_id.as_deref(),
            &title,
        );
        match TerminalPane::spawn_with_pty_config(
            cx,
            self.terminal_manager.clone(),
            pty_config,
            self.terminal_config_from_settings(),
        ) {
            Ok(terminal) => {
                self.register_terminal_pane(pane_terminal_id.as_deref(), &terminal, cx);
                let active_runtime_id = pane_terminal_id.clone();
                let send_result = terminal.send_text(&terminal_command_text(&command));
                if let Some(window) = window.as_deref_mut() {
                    terminal.view.read(cx).focus_handle().focus(window, cx);
                }
                if let Some(tab) = self.main_terminal_mut() {
                    tab.panes.push(TerminalPaneSlot {
                        title,
                        terminal_id: pane_terminal_id,
                        pane: Some(terminal),
                        restored_output_bytes: 0,
                        restored_output_tail: String::new(),
                    });
                }
                self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
                if let Some(window) = window {
                    self.focus_active_terminal(window, cx);
                }
                if let Err(error) = send_result {
                    self.status_message =
                        format!("AI session split created; restore send failed: {error}");
                } else {
                    self.status_message = "AI session restored in main split".to_string();
                }
                self.sync_terminal_state_after_layout_change(cx);
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
        let terminal_ids = removed
            .panes
            .iter()
            .enumerate()
            .filter_map(|(pane_index, slot)| {
                Self::terminal_slot_terminal_id(&removed, pane_index, slot)
            })
            .collect::<Vec<_>>();
        let kill_errors = terminal_ids
            .iter()
            .filter_map(|terminal_id| self.kill_terminal_session_if_present(terminal_id).err())
            .collect::<Vec<_>>();
        self.active_terminal_id = self
            .terminals
            .get(index.saturating_sub(1))
            .filter(|tab| tab.placement == TerminalTabPlacement::Bottom)
            .or_else(|| self.bottom_terminals().next())
            .map(|tab| tab.id)
            .unwrap_or(0);
        let active_runtime_id = self
            .active_bottom_terminal()
            .and_then(|tab| tab.panes.first())
            .and_then(|slot| slot.terminal_id.clone())
            .or_else(|| {
                self.main_terminal()
                    .and_then(|tab| tab.panes.first())
                    .and_then(|slot| slot.terminal_id.clone())
            });
        self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
        let mount_result = self.ensure_active_terminal_mounted(cx);
        self.detach_inactive_terminal_views();
        self.focus_active_terminal(window, cx);
        self.sync_terminal_state_after_layout_change(cx);
        if let Err(error) = mount_result {
            self.status_message = format!("terminal tab closed; mount failed: {error}");
        } else if let Some(error) = kill_errors.first() {
            self.status_message = format!("terminal tab closed; PTY cleanup failed: {error}");
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
        self.remember_active_bottom_terminal_for_current_scope();
        let active_runtime_id = self
            .terminals
            .iter()
            .find(|tab| tab.id == terminal_id)
            .and_then(|tab| tab.panes.first())
            .and_then(|slot| slot.terminal_id.clone());
        self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
        if let Err(error) = self.ensure_active_terminal_mounted(cx) {
            self.status_message = format!("failed to select terminal: {error}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        self.focus_active_terminal(window, cx);
        self.detach_inactive_terminal_views();
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn reorder_bottom_terminal_tabs(
        &mut self,
        next_terminal_ids: Vec<usize>,
        cx: &mut Context<Self>,
    ) {
        let current_bottom_ids = self
            .terminals
            .iter()
            .filter(|tab| tab.placement == TerminalTabPlacement::Bottom)
            .map(|tab| tab.id)
            .collect::<Vec<_>>();
        if current_bottom_ids == next_terminal_ids
            || current_bottom_ids.len() != next_terminal_ids.len()
        {
            return;
        }

        let terminals = std::mem::take(&mut self.terminals);
        let (mut bottom_tabs, mut other_tabs): (Vec<_>, Vec<_>) = terminals
            .into_iter()
            .partition(|tab| tab.placement == TerminalTabPlacement::Bottom);
        let mut reordered = Vec::with_capacity(bottom_tabs.len());
        for terminal_id in next_terminal_ids {
            let Some(index) = bottom_tabs.iter().position(|tab| tab.id == terminal_id) else {
                other_tabs.extend(bottom_tabs);
                self.terminals = other_tabs;
                return;
            };
            reordered.push(bottom_tabs.remove(index));
        }
        if !bottom_tabs.is_empty() {
            other_tabs.extend(bottom_tabs);
            self.terminals = other_tabs;
            return;
        }

        other_tabs.extend(reordered);
        self.terminals = other_tabs;
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn rename_bottom_terminal_tab(
        &mut self,
        terminal_id: usize,
        label: String,
        cx: &mut Context<Self>,
    ) {
        let label = normalized_terminal_tab_label(&label);
        let Some(tab) = self
            .terminals
            .iter_mut()
            .find(|tab| tab.id == terminal_id && tab.placement == TerminalTabPlacement::Bottom)
        else {
            return;
        };
        if tab.label == label {
            return;
        }
        tab.label = label;
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn open_terminal_tab_editor_window(
        &mut self,
        terminal_id: usize,
        label: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = self.text("terminal.tab.rename.title", "Rename Tab");
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::TerminalTabEditor,
                title: SharedString::from(title.clone()),
                size: gpui::size(px(360.0), px(190.0)),
                min_size: gpui::size(px(360.0), px(190.0)),
                already_open_message: "terminal tab editor already opened",
                opened_message: "terminal tab editor opened",
                failed_prefix: "failed to open terminal tab editor",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::TerminalTabEditor;
                app.terminal_tab_editor_id = Some(terminal_id);
                app.terminal_tab_editor_label = label;
                app.status_message = title;
                app
            },
            {
                let parent = cx.entity().downgrade();
                move |view, _window, cx| {
                    view.update(cx, |app, _cx| {
                        app.parent_main_window = Some(parent);
                    });
                }
            },
        );
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn set_terminal_tab_editor_label(
        &mut self,
        label: String,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        self.terminal_tab_editor_label = label;
    }

    pub(in crate::app) fn save_terminal_tab_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal_id) = self.terminal_tab_editor_id else {
            window.remove_window();
            return;
        };
        let label = self.terminal_tab_editor_label.clone();
        if let Some(parent) = self
            .parent_main_window
            .as_ref()
            .and_then(|parent| parent.upgrade())
        {
            let _ = parent.update(cx, |app, cx| {
                app.rename_bottom_terminal_tab(terminal_id, label, cx);
                app.terminal_tab_editor_window = None;
                app.invalidate_status_bar(cx);
            });
        }
        window.remove_window();
    }

    pub(in crate::app) fn terminal_tab_editor_workspace(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let can_submit = !self.terminal_tab_editor_label.trim().is_empty();
        let title = self.text("terminal.tab.rename.title", "Rename Tab");
        let field_label = self.text("terminal.tab.rename.field", "Tab Title");
        let placeholder = self.text("terminal.tab.rename.placeholder", "Tab Name");
        let cancel_label = self.text("common.cancel", "Cancel");
        let save_label = self.text("common.save", "Save");
        child_window_shell(title, cx)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .p(px(18.0))
                    .child(terminal_tab_editor_input(
                        &self.terminal_tab_editor_label,
                        field_label,
                        placeholder,
                        window,
                        cx,
                    )),
            )
            .child(dialog_footer_bar(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(dialog_cancel_button(
                        "terminal-tab-editor-cancel",
                        cancel_label,
                        cx,
                        |_app, _event, window, _cx| window.remove_window(),
                    ))
                    .child(
                        dialog_primary_button(
                            "terminal-tab-editor-save",
                            save_label,
                            cx,
                            |app, _event, window, cx| app.save_terminal_tab_editor(window, cx),
                        )
                        .disabled(!can_submit),
                    ),
                cx,
            ))
    }
}

fn normalized_terminal_tab_label(label: &str) -> String {
    let label = label.trim();
    if label.is_empty() {
        "Tab".to_string()
    } else {
        label.chars().take(48).collect()
    }
}

fn terminal_tab_editor_input(
    value: &str,
    field_label: String,
    placeholder: String,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let value = value.to_string();
    let input = window.use_keyed_state("terminal-tab-editor-label", cx, {
        let value = value.clone();
        move |window, cx| {
            InputState::new(window, cx)
                .default_value(value.clone())
                .placeholder(placeholder.clone())
        }
    });
    input.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
        state.focus(window, cx);
    });
    cx.subscribe_in(
        &input,
        window,
        |app, state, event, window, cx| match event {
            InputEvent::Change => {
                app.set_terminal_tab_editor_label(state.read(cx).value().to_string(), window, cx);
            }
            InputEvent::PressEnter { .. } => {
                cx.stop_propagation();
                window.prevent_default();
                app.save_terminal_tab_editor(window, cx);
            }
            InputEvent::Focus | InputEvent::Blur => {}
        },
    )
    .detach();

    div()
        .flex()
        .flex_col()
        .gap(px(6.0))
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT))
                .child(field_label),
        )
        .child(Input::new(&input).with_size(gpui_component::Size::Medium))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::normalized_terminal_tab_label;

    #[test]
    fn normalizes_terminal_tab_label() {
        assert_eq!(normalized_terminal_tab_label("  dev  "), "dev");
        assert_eq!(normalized_terminal_tab_label("   "), "Tab");
        assert_eq!(
            normalized_terminal_tab_label("abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuvwxyz"),
            "abcdefghijklmnopqrstuvwxyzabcdefghijklmnopqrstuv"
        );
    }
}
