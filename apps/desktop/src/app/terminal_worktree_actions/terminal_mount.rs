use super::terminal_layout::normalized_terminal_osc_title;
use super::*;

pub(in crate::app) struct TerminalCleanupResult {
    pub(in crate::app) lifecycle_removed: bool,
}

impl CoduxApp {
    pub(in crate::app) fn close_terminal_sessions_for_owners(
        &mut self,
        owner_ids: &HashSet<String>,
        cx: &mut Context<Self>,
    ) {
        let belongs_to_project = |terminal_id: &str| {
            owner_ids
                .iter()
                .any(|owner_id| terminal_id_belongs_to_owner(terminal_id, owner_id))
        };
        let mut terminal_ids = self
            .terminal_pane_registry
            .keys()
            .filter(|terminal_id| belongs_to_project(terminal_id))
            .cloned()
            .collect::<HashSet<_>>();
        terminal_ids.extend(
            self.terminal_manager
                .list()
                .into_iter()
                .filter(|session| {
                    owner_ids.contains(&session.project_id) || belongs_to_project(&session.id)
                })
                .map(|session| session.id),
        );
        for tab in &self.terminals {
            for (pane_index, slot) in tab.panes.iter().enumerate() {
                if let Some(terminal_id) = Self::terminal_slot_terminal_id(tab, pane_index, slot)
                    && belongs_to_project(&terminal_id)
                {
                    terminal_ids.insert(terminal_id);
                }
            }
        }
        terminal_ids.extend(
            self.collapsed_terminal_panes
                .iter()
                .filter_map(|slot| slot.terminal_id.clone())
                .filter(|terminal_id| belongs_to_project(terminal_id)),
        );

        let mut lifecycle_removed = false;
        for terminal_id in terminal_ids {
            self.terminal_attach_in_flight.remove(&terminal_id);
            match self.kill_terminal_session_if_present(&terminal_id) {
                Ok(cleanup) => lifecycle_removed |= cleanup.lifecycle_removed,
                Err(error) => self.runtime_trace(
                    "terminal",
                    &format!(
                        "close_for_project_rebind_failed terminal_id={terminal_id} error={error}"
                    ),
                ),
            }
        }
        if lifecycle_removed {
            self.sync_project_lifecycle_state(cx);
            self.invalidate_task_column(cx);
        }
    }

    pub(in crate::app) fn ensure_active_terminal_mounted(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let Some((tab_index, slot_index)) = active_terminal_slot_indices(
            &self.terminals,
            &self.state.terminal_layout.active_terminal_id,
            self.active_terminal_id,
        ) else {
            return Ok(());
        };
        self.ensure_terminal_slot_mounted(tab_index, slot_index, cx)
    }

    pub(in crate::app) fn ensure_terminal_slot_mounted(
        &mut self,
        tab_index: usize,
        slot_index: usize,
        cx: &mut Context<Self>,
    ) -> Result<(), String> {
        let config = self.terminal_config_from_settings();
        let base_pty_config = self.current_terminal_base_pty_config();
        let terminal_pane_registry = self.terminal_pane_registry.clone();
        let mut pending = Vec::new();

        let Some(slot) = self
            .terminals
            .get_mut(tab_index)
            .and_then(|tab| tab.panes.get_mut(slot_index))
        else {
            return Ok(());
        };
        if slot.pane.is_some() {
            return Ok(());
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
            .filter(|pane| pane.matches_pty_config(&pty_config))
            .cloned()
        {
            refresh_terminal_pane_config(&pane, &config, cx);
            slot.pane = Some(pane);
            return Ok(());
        }

        let restored_output = TerminalOutputSnapshot {
            bytes: slot.restored_output_bytes,
            tail: slot.restored_output_tail.clone(),
        };
        let terminal_id = slot.terminal_id.clone();
        let (pane, attach) = TerminalPane::pending_with_restored_output(
            cx,
            pty_config.clone(),
            config,
            Some(restored_output),
        );
        slot.pane = Some(pane.clone());
        if let Some(terminal_id) = terminal_id.as_deref() {
            self.register_terminal_pane(Some(terminal_id), &pane, cx);
        }
        pending.push((pty_config, attach));
        self.spawn_attach_pending_terminals(None, pending, cx);
        Ok(())
    }

    pub(in crate::app) fn refresh_terminal_slot_snapshots(&mut self) {
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

    pub(in crate::app) fn reconcile_hosted_terminal_bindings(&mut self, cx: &mut Context<Self>) {
        let targets = self
            .terminal_pane_registry
            .values()
            .chain(
                self.terminals
                    .iter()
                    .flat_map(|tab| tab.panes.iter())
                    .filter_map(|slot| slot.pane.as_ref()),
            )
            .filter_map(TerminalPane::hosted_runtime_target)
            .collect::<HashSet<_>>();
        for target in targets {
            if let Ok(Some(controller)) =
                self.runtime_service.terminal_controller_for_target(&target)
            {
                self.apply_hosted_terminal_controller(&target, controller, cx);
                continue;
            }
            if !matches!(target, ProjectRuntimeTarget::Wsl { .. })
                || !self.hosted_terminal_rebind_in_flight.insert(target.clone())
            {
                continue;
            }
            let runtime_service = self.runtime_service.clone();
            cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                let target_for_resolve = target.clone();
                let result = codux_runtime::async_runtime::spawn_blocking(move || {
                    runtime_service.terminal_controller_for_target_blocking(&target_for_resolve)
                })
                .await;
                let _ = this.update(cx, |app, cx| {
                    app.hosted_terminal_rebind_in_flight.remove(&target);
                    if let Ok(Ok(Some(controller))) = result {
                        app.apply_hosted_terminal_controller(&target, controller, cx);
                    }
                });
            })
            .detach();
        }
    }

    fn apply_hosted_terminal_controller(
        &mut self,
        target: &ProjectRuntimeTarget,
        controller: Arc<dyn codux_runtime::runtime_terminal::RuntimeTerminalController>,
        cx: &mut Context<Self>,
    ) {
        let mut rebound = 0_usize;
        let mounted = self
            .terminals
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .filter_map(|slot| slot.pane.as_ref());
        for pane in self.terminal_pane_registry.values().chain(mounted) {
            if pane.hosted_runtime_target().as_ref() != Some(target) {
                continue;
            }
            if pane.rebind_hosted_controller(controller.clone()) {
                rebound += 1;
            }
        }
        if rebound > 0 && matches!(target, ProjectRuntimeTarget::Remote { .. }) {
            self.status_message = "remote host reconnected — terminal resumed".to_string();
            self.invalidate_status_bar(cx);
        }
    }

    pub(in crate::app) fn main_terminal(&self) -> Option<&TerminalTab> {
        self.terminals.first().or_else(|| self.active_terminal())
    }

    pub(in crate::app) fn main_terminal_mut(&mut self) -> Option<&mut TerminalTab> {
        self.terminals.first_mut()
    }

    pub(in crate::app) fn terminal_slot_terminal_id(
        tab: &TerminalTab,
        _pane_index: usize,
        slot: &TerminalPaneSlot,
    ) -> Option<String> {
        slot.terminal_id
            .clone()
            .or_else(|| tab.terminal_id.clone())
            .filter(|id| !id.trim().is_empty())
    }

    pub(in crate::app) fn kill_terminal_session_if_present(
        &mut self,
        terminal_id: &str,
    ) -> Result<TerminalCleanupResult, String> {
        // A hosted terminal is not owned by the local manager, so close it through
        // its runtime controller before removing the desktop binding.
        if let Some(pane) = self.terminal_pane_registry.get(terminal_id) {
            pane.close_hosted_session();
        }
        let lifecycle_removed = self.remove_registered_terminal_pane(terminal_id);
        let exists = self
            .terminal_manager
            .list()
            .iter()
            .any(|session| session.id == terminal_id);
        if exists {
            self.terminal_manager
                .kill(terminal_id)
                .map_err(|error| error.to_string())?;
            // Tell connected mobile clients the terminal set changed so they
            // reconcile their view instead of showing the closed session's
            // stale content. (A no-op when no device is connected.)
            self.runtime_service.broadcast_remote_terminal_list();
        }
        Ok(TerminalCleanupResult { lifecycle_removed })
    }

    pub(in crate::app) fn detach_terminal_session_if_present(
        &mut self,
        terminal_id: &str,
    ) -> TerminalCleanupResult {
        let lifecycle_removed = self.remove_registered_terminal_pane(terminal_id);
        TerminalCleanupResult { lifecycle_removed }
    }

    pub(in crate::app) fn register_terminal_panes(&mut self, cx: &mut Context<Self>) {
        let registrations = self.terminal_pane_registrations();
        for (terminal_id, pane) in registrations {
            self.register_terminal_pane(Some(&terminal_id), &pane, cx);
        }
    }

    pub(in crate::app) fn register_terminal_panes_without_observers(&mut self) {
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

    pub(in crate::app) fn remove_registered_terminal_pane(&mut self, terminal_id: &str) -> bool {
        self.terminal_pane_registry.remove(terminal_id);
        self.terminal_osc_titles.remove(terminal_id);
        self.terminal_search_open.remove(terminal_id);
        self.clear_pane_agent_lifecycle(terminal_id)
    }

    pub(in crate::app) fn register_terminal_pane(
        &mut self,
        terminal_id: Option<&str>,
        pane: &TerminalPane,
        cx: &mut Context<Self>,
    ) {
        let Some(terminal_id) = terminal_id.filter(|id| !id.trim().is_empty()) else {
            return;
        };
        let app = cx.entity().downgrade();
        let app_for_link = app.clone();
        let app_for_title = app.clone();
        let app_for_search = app.clone();
        let terminal_id = terminal_id.to_string();
        let observer_terminal_id = terminal_id.clone();
        let title_terminal_id = terminal_id.clone();
        let search_terminal_id = terminal_id.clone();
        pane.view.update(cx, |terminal, _| {
            terminal.set_focus_observer(move |_window, cx| {
                let terminal_id = observer_terminal_id.clone();
                let _ = app.update(cx, |app, cx| {
                    app.record_focused_terminal_runtime_id(&terminal_id, cx);
                });
            });
            let title_terminal_id = title_terminal_id.clone();
            terminal.set_title_observer(move |title, cx| {
                let terminal_id = title_terminal_id.clone();
                let _ = app_for_title.update(cx, |app, cx| {
                    app.set_terminal_osc_title(&terminal_id, title, cx);
                });
            });
            let search_terminal_id = search_terminal_id.clone();
            terminal.set_search_observer(move |open, cx| {
                let terminal_id = search_terminal_id.clone();
                let app = app_for_search.clone();
                // open_search may run inside a CoduxApp update (cmd-f action
                // handler) — updating it inline would re-enter; defer instead.
                cx.defer(move |cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.set_terminal_search_open(&terminal_id, open, cx);
                    });
                });
            });
            terminal.set_link_opener(move |url, _window, cx| {
                let _ = app_for_link.update(cx, |app, cx| {
                    app.open_terminal_web_link(url, cx);
                });
            });
        });
        // Seed from the view's cached title: registration may follow output
        // that already carried an OSC title.
        let seeded_title = pane
            .view
            .read(cx)
            .osc_title()
            .and_then(normalized_terminal_osc_title);
        if let Some(title) = seeded_title {
            self.terminal_osc_titles.insert(terminal_id.clone(), title);
        }
        if pane.view.read(cx).search_is_open() {
            self.terminal_search_open.insert(terminal_id.clone());
        } else {
            self.terminal_search_open.remove(&terminal_id);
        }
        self.terminal_pane_registry
            .insert(terminal_id, pane.clone());
    }

    /// Called by the pane's title observer while that view is mid-update; only
    /// stores the value and invalidates — must never read the view back.
    pub(in crate::app) fn set_terminal_osc_title(
        &mut self,
        terminal_id: &str,
        title: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let title = title.as_deref().and_then(normalized_terminal_osc_title);
        let changed = match title {
            Some(title) => {
                self.terminal_osc_titles
                    .insert(terminal_id.to_string(), title.clone())
                    .as_ref()
                    != Some(&title)
            }
            None => self.terminal_osc_titles.remove(terminal_id).is_some(),
        };
        if changed {
            self.invalidate_task_column(cx);
        }
    }

    /// Called by the pane's search observer while that view is mid-update; only
    /// stores the value and invalidates — must never read the view back.
    pub(in crate::app) fn set_terminal_search_open(
        &mut self,
        terminal_id: &str,
        open: bool,
        cx: &mut Context<Self>,
    ) {
        let changed = if open {
            self.terminal_search_open.insert(terminal_id.to_string())
        } else {
            self.terminal_search_open.remove(terminal_id)
        };
        if changed {
            self.invalidate_ui(cx, [UiRegion::WorkspaceBody]);
        }
    }

    pub(in crate::app) fn record_focused_terminal_runtime_id(
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
        let Some(tab_id) = self.terminals.iter().find_map(|tab| {
            let matches_tab = tab.terminal_id.as_deref() == Some(terminal_id);
            let matches_pane = tab
                .panes
                .iter()
                .any(|slot| slot.terminal_id.as_deref() == Some(terminal_id));
            (matches_tab || matches_pane).then_some(tab.id)
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
        if !runtime_changed && !remembered_runtime_changed {
            return;
        }

        self.state.terminal_layout.active_terminal_id = terminal_id.to_string();
        self.active_terminal_runtime_ids
            .insert(key.clone(), terminal_id.to_string());
        self.runtime_trace(
            "terminal-focus",
            &format!("focus_in terminal_id={terminal_id} tab={tab_id}"),
        );
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn open_terminal_web_link(&mut self, url: String, cx: &mut Context<Self>) {
        let remote_device = self
            .state
            .selected_project
            .as_ref()
            .and_then(|project| project.remote_device_id().map(str::to_string));
        if let Some(device_id) = remote_device {
            let locale = locale_from_language_setting(&self.state.settings.language);
            self.open_remote_project_web_url(
                device_id,
                url,
                translate(
                    &locale,
                    "workspace.web_tunnel.open_failed",
                    "Open Web Tunnel Failed",
                ),
                cx,
            );
            return;
        }
        if let Err(error) = self.runtime_service.open_url(&url) {
            self.status_message = "failed to open link".to_string();
            self.show_system_error_alert("Open Link Failed".to_string(), error, cx);
            self.invalidate_status_bar(cx);
        }
    }
}
