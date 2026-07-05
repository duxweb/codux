use super::*;
impl CoduxApp {
    pub(in crate::app) fn confirm_or_close_active_terminal_target(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        const CLOSE_CONFIRM_WINDOW: Duration = Duration::from_secs(2);

        let Some(target) = self.active_terminal_close_target(window, cx) else {
            self.pending_terminal_close = None;
            self.status_message = "no terminal split to close".to_string();
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
                    return Some(TerminalCloseTarget::Split { pane_index });
                }
            }
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

    pub(in crate::app) fn select_terminal_pane(
        &mut self,
        pane_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.refresh_terminal_slot_snapshots();
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
            return;
        };
        let Some(terminal_id) = self.terminals[tab_index]
            .panes
            .get(pane_index)
            .and_then(|slot| slot.terminal_id.clone())
        else {
            return;
        };
        self.select_active_terminal_runtime_id(Some(&terminal_id));
        if let Err(error) = self.ensure_terminal_slot_mounted(tab_index, pane_index, cx) {
            self.status_message = format!("failed to select terminal split: {error}");
            self.invalidate_terminal_workspace(cx);
            return;
        }
        self.focus_active_terminal(window, cx);
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_terminal_workspace(cx);
    }

    /// Reconcile the in-memory terminal layout with sessions a connected mobile
    /// client created (split/tab) for the current scope. Mobile-created PTYs are
    /// already live in the shared `TerminalManager` and persisted into the
    /// layout; here we attach the desktop view to each one we are not yet showing
    /// — without re-mounting existing panes (no flicker), reusing the running PTY
    /// via the attach path. Driven by the runtime's terminal-layout generation.
    pub(in crate::app) fn reconcile_remote_terminal_layout(&mut self, cx: &mut Context<Self>) {
        if self.terminal_layout_loading {
            return;
        }
        let Some(launch_context) = self.current_terminal_launch_context() else {
            return;
        };
        let Some(layout_key) =
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state)
        else {
            return;
        };
        let base_pty_config = launch_context.to_config();
        let layout = self
            .runtime_service
            .reload_terminal_layout(Some(&layout_key));
        let layout_ids: std::collections::HashSet<String> = layout
            .top_panes
            .iter()
            .map(|pane| pane.terminal_id.clone())
            .filter(|terminal_id| !terminal_id.trim().is_empty())
            .collect();
        // An empty layout is NOT an early-out. reconcile runs only on a real
        // TerminalLayoutChanged event (create/close/move) and reloads the
        // just-persisted summary, so an empty result means the last terminal was
        // genuinely closed (e.g. a controller closed the only terminal in a
        // worktree) -- fall through and tear the now-stale desktop panes down
        // instead of leaving an orphaned dead pane behind.
        let mut removed_stale = false;
        let mut removed_terminal_ids = Vec::new();
        for tab in &mut self.terminals {
            let tab_terminal_id = tab.terminal_id.clone();
            let mut index = 0;
            while tab.panes.len() > 1 && index < tab.panes.len() {
                let terminal_id = tab.panes[index]
                    .terminal_id
                    .clone()
                    .or_else(|| tab_terminal_id.clone());
                if terminal_id
                    .as_deref()
                    .is_some_and(|terminal_id| !layout_ids.contains(terminal_id))
                {
                    if let Some(terminal_id) = terminal_id.as_deref() {
                        removed_terminal_ids.push(terminal_id.to_string());
                    }
                    tab.panes.remove(index);
                    removed_stale = true;
                } else {
                    index += 1;
                }
            }
        }
        for terminal_id in removed_terminal_ids {
            self.remove_registered_terminal_pane(&terminal_id);
        }
        if removed_stale {
            let active_id = self.active_terminal_runtime_id();
            if active_id.trim().is_empty() || !layout_ids.contains(&active_id) {
                self.activate_first_terminal();
            }
        }

        let shown_ids: std::collections::HashSet<String> = self
            .terminals
            .iter()
            .flat_map(|tab| tab.panes.iter())
            .filter_map(|slot| slot.terminal_id.clone())
            .collect();

        let mut pending: Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)> =
            Vec::new();

        // New top split panes -> append to the main (Top) tab.
        for pane in &layout.top_panes {
            let raw_id = pane.terminal_id.trim();
            if raw_id.is_empty() {
                continue;
            }
            let pane_plan = TerminalPanePlan {
                terminal_id: Some(raw_id.to_string()),
                title: pane.title.clone(),
                restored_output_bytes: 0,
                restored_output_tail: String::new(),
            };
            let Some(pane_terminal_id) =
                terminal_pane_terminal_id(Some(&launch_context), &pane_plan)
            else {
                continue;
            };
            if shown_ids.contains(&pane_terminal_id) {
                continue;
            }
            let pty_config = terminal_pty_config_for_terminal_id(
                &base_pty_config,
                Some(&pane_terminal_id),
                &pane.title,
            );
            let (terminal_pane, attach) = TerminalPane::pending_with_pty_config(
                cx,
                pty_config.clone(),
                self.terminal_config_from_settings(),
            );
            self.register_terminal_pane(Some(&pane_terminal_id), &terminal_pane, cx);
            if let Some(tab) = self.main_terminal_mut() {
                tab.panes.push(TerminalPaneSlot {
                    title: pane.title.clone(),
                    terminal_id: Some(pane_terminal_id),
                    pane: Some(terminal_pane),
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                });
            }
            pending.push((pty_config, attach));
        }

        if pending.is_empty() {
            if removed_stale {
                self.sync_terminal_state_after_layout_change(cx);
                self.refresh_terminal_slot_snapshots();
                self.invalidate_terminal_workspace(cx);
            }
            return;
        }
        // Do not steal focus/active selection — the desktop user may be working;
        // the new pane/tab just appears.
        self.sync_terminal_state_after_layout_change(cx);
        self.spawn_attach_pending_terminals(None, pending, cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn split_terminal(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source_index = self
            .focused_terminal_runtime_id(window, cx)
            .or_else(|| {
                let active_id = self.active_terminal_runtime_id();
                (!active_id.trim().is_empty()).then_some(active_id)
            })
            .and_then(|terminal_id| {
                self.main_terminal().and_then(|tab| {
                    tab.panes
                        .iter()
                        .position(|slot| slot.terminal_id.as_deref() == Some(terminal_id.as_str()))
                })
            })
            .unwrap_or(0);
        self.split_terminal_direction(
            TerminalSplitDirection::Right,
            TerminalSplitScope::Inner,
            source_index,
            window,
            cx,
        );
    }

    /// Single sync point after any split-tree mutation: the tree is the source
    /// of truth; `top_grid`/`top_ratios` are derived mirrors kept only so older
    /// app versions can still read the persisted layout.
    pub(in crate::app) fn set_terminal_split_tree(&mut self, tree: Option<TerminalSplitNode>) {
        self.state.terminal_layout.top_grid = tree
            .as_ref()
            .map(|tree| {
                codux_runtime::terminal_layout::top_grid_from_split_tree(
                    tree,
                    codux_runtime::terminal_layout::split_tree_leaf_count(tree),
                )
            })
            .unwrap_or_default();
        self.state.terminal_layout.top_ratios =
            terminal_top_ratios_from_grid(&self.state.terminal_layout.top_grid);
        self.state.terminal_layout.split_tree = tree;
    }

    pub(in crate::app) fn split_terminal_direction(
        &mut self,
        direction: TerminalSplitDirection,
        scope: TerminalSplitScope,
        source_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        let launch_context = self.current_terminal_launch_context();
        let base_pty_config = launch_context
            .as_ref()
            .map(TerminalLaunchContext::to_config)
            .unwrap_or_default();
        let Some(active_tab) = self.main_terminal() else {
            return;
        };
        let pane_count = active_tab.panes.len();
        if pane_count >= codux_runtime::terminal_layout::TERMINAL_SPLIT_CAP {
            self.status_message = "main split limit reached".to_string();
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
        let source_index = source_index.min(pane_count.saturating_sub(1));
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let before = matches!(
            direction,
            TerminalSplitDirection::Left | TerminalSplitDirection::Up
        );
        let insert_index = match scope {
            TerminalSplitScope::Inner => {
                if before {
                    source_index
                } else {
                    source_index + 1
                }
            }
            TerminalSplitScope::Root => {
                if before {
                    0
                } else {
                    pane_count
                }
            }
        };
        let split_tree_result = match scope {
            TerminalSplitScope::Inner => {
                terminal_split_tree_insert_pane(&tree, source_index, insert_index, direction)
            }
            TerminalSplitScope::Root => {
                terminal_split_tree_insert_pane_root(&tree, insert_index, direction)
            }
        };
        let split_tree = match split_tree_result {
            Ok(result) => result,
            Err(error) => {
                self.status_message = error.to_string();
                self.invalidate_terminal_workspace(cx);
                return;
            }
        };
        let pane_index = insert_index;
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
        let (terminal, attach) = TerminalPane::pending_with_pty_config(
            cx,
            pty_config.clone(),
            self.terminal_config_from_settings(),
        );
        self.register_terminal_pane(pane_terminal_id.as_deref(), &terminal, cx);
        let active_runtime_id = pane_terminal_id.clone();
        terminal.view.read(cx).focus_handle().focus(window, cx);
        if let Some(tab) = self.main_terminal_mut() {
            let insert_index = insert_index.min(tab.panes.len());
            tab.panes.insert(
                insert_index,
                TerminalPaneSlot {
                    title,
                    terminal_id: pane_terminal_id,
                    pane: Some(terminal),
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                },
            );
        }
        self.set_terminal_split_tree(Some(split_tree));
        self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
        self.focus_active_terminal(window, cx);
        self.status_message = "terminal split added".to_string();
        self.sync_terminal_state_after_layout_change(cx);
        self.spawn_attach_pending_terminals(None, vec![(pty_config, attach)], cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn swap_terminal_top_panes(
        &mut self,
        from_index: usize,
        target_index: usize,
        cx: &mut Context<Self>,
    ) {
        let pane_count = self.main_terminal().map(|tab| tab.panes.len()).unwrap_or(0);
        if pane_count <= 1
            || from_index >= pane_count
            || target_index >= pane_count
            || from_index == target_index
        {
            return;
        }
        let Some(tab) = self.main_terminal_mut() else {
            return;
        };
        tab.panes.swap(from_index, target_index);
        self.refresh_terminal_slot_snapshots();
        self.sync_terminal_state_after_layout_change(cx);
        self.status_message = "terminal panes swapped".to_string();
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn collapse_terminal_pane(
        &mut self,
        pane_index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
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

        let pane_count = self.terminals[tab_index].panes.len();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let split_tree = terminal_split_tree_remove_pane(&tree, pane_index);
        self.refresh_terminal_slot_snapshots();
        let mut slot = self.terminals[tab_index].panes.remove(pane_index);
        let title = slot.title.clone();
        if slot.pane.is_none() {
            if slot.terminal_id.is_none() {
                self.terminals[tab_index].panes.insert(pane_index, slot);
                self.status_message =
                    "terminal pane cannot be collapsed without a stable session".to_string();
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
                        format!("failed to attach collapsed terminal pane: {error}");
                    self.invalidate_terminal_workspace(cx);
                    return;
                }
            }
        }
        self.set_terminal_split_tree(split_tree);
        self.collapsed_terminal_panes.push(slot);
        self.status_message = format!("terminal pane collapsed: {title}");
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_task_column(cx);
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn restore_collapsed_terminal(
        &mut self,
        collapsed_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if collapsed_index >= self.collapsed_terminal_panes.len() {
            return;
        }
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
            return;
        };
        let slot = self.collapsed_terminal_panes.remove(collapsed_index);
        let title = slot.title.clone();

        let pane_count = self.terminals[tab_index].panes.len();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let insert_index = self.terminals[tab_index].panes.len();
        let (split_tree, insert_index) =
            match terminal_split_tree_with_restored_location(&tree, None, insert_index) {
                Ok(result) => result,
                Err(error) => {
                    self.collapsed_terminal_panes.insert(collapsed_index, slot);
                    self.status_message = error.to_string();
                    self.invalidate_terminal_workspace(cx);
                    return;
                }
            };
        let insert_index = insert_index.min(self.terminals[tab_index].panes.len());
        self.terminals[tab_index].panes.insert(insert_index, slot);
        self.set_terminal_split_tree(Some(split_tree));
        self.status_message = format!("terminal pane restored: {title}");
        self.sync_terminal_state_after_layout_change(cx);
        self.invalidate_task_column(cx);
        self.invalidate_terminal_workspace(cx);
        let pane_to_focus = insert_index;
        self.select_terminal_pane(pane_to_focus, window, cx);
    }

    pub(in crate::app) fn float_terminal_pane(
        &mut self,
        pane_index: usize,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
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

        let pane_count = self.terminals[tab_index].panes.len();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let split_location = terminal_split_tree_location_for_pane(&tree, pane_index);
        let split_tree = terminal_split_tree_remove_pane(&tree, pane_index);
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
        self.set_terminal_split_tree(split_tree);
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
            split_location,
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
        split_location: Option<TerminalSplitLocation>,
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
            .or_else(|| (!self.terminals.is_empty()).then_some(0))
        else {
            return;
        };
        let pane_count = self.terminals[tab_index].panes.len();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let insert_index = pane_index.min(self.terminals[tab_index].panes.len());
        let (split_tree, insert_index) =
            match terminal_split_tree_with_restored_location(&tree, split_location, insert_index) {
                Ok(result) => result,
                Err(error) => {
                    self.status_message = error.to_string();
                    self.invalidate_terminal_workspace(cx);
                    return;
                }
            };
        let insert_index = insert_index.min(self.terminals[tab_index].panes.len());
        self.terminals[tab_index].panes.insert(insert_index, slot);
        self.set_terminal_split_tree(Some(split_tree));
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
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
            return;
        };
        if self.terminals[tab_index].panes.len() <= 1 {
            self.reset_terminal_pane(pane_index, window, cx);
            return;
        }
        if pane_index >= self.terminals[tab_index].panes.len() {
            return;
        }
        let pane_count = self.terminals[tab_index].panes.len();
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let split_tree = terminal_split_tree_remove_pane(&tree, pane_index);
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
        self.set_terminal_split_tree(split_tree);
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
        let Some(tab_index) = (!self.terminals.is_empty()).then_some(0) else {
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
        self.refresh_terminal_slot_snapshots();
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

    pub(in crate::app) fn paste_ai_session_restore_to_main_pane(
        &mut self,
        terminal_id: Option<&str>,
        session_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(session) = self
            .state
            .ai_history
            .sessions
            .iter()
            .find(|session| session.id == session_id)
            .cloned()
        else {
            self.status_message = self.text(
                "terminal.ai_restore.no_session",
                "No AI session to restore.",
            );
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let Some(target_terminal_id) = terminal_id
            .map(str::trim)
            .filter(|terminal_id| !terminal_id.is_empty())
        else {
            self.status_message =
                self.text("terminal.split.not_found", "Terminal split not found.");
            self.invalidate_terminal_workspace(cx);
            return;
        };
        prepare_memory_launch_artifacts(&self.runtime_service, &self.state);
        self.state.tool_permissions = self.runtime_service.sync_tool_permissions();
        // Paste without enter so the user can review the command before running it.
        let command = ai_session_restore_command(&session);
        let target = self.terminal_pane_registry.get(target_terminal_id);
        match target.map(|pane| pane.send_text(&command)) {
            Some(Ok(())) => {
                self.select_active_terminal_runtime_id(Some(target_terminal_id));
                self.focus_active_terminal(window, cx);
                self.status_message = self.text(
                    "terminal.ai_restore.pasted",
                    "AI session restore command pasted.",
                );
            }
            Some(Err(error)) => {
                self.status_message = self
                    .text(
                        "terminal.ai_restore.paste_failed_format",
                        "Failed to paste restore command: %@",
                    )
                    .replace("%@", &error.to_string());
            }
            None => {
                self.status_message =
                    self.text("terminal.split.not_found", "Terminal split not found.");
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
            self.status_message = self.text(
                "terminal.ai_restore.no_main_terminal",
                "No main terminal to restore the session.",
            );
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let pane_count = active_tab.panes.len();
        if pane_count >= codux_runtime::terminal_layout::TERMINAL_SPLIT_CAP {
            self.status_message = self.text(
                "terminal.ai_restore.split_limit",
                "Main split limit reached.",
            );
            self.invalidate_terminal_workspace(cx);
            return;
        }

        let Some(owner_id) = launch_context
            .as_ref()
            .map(|context| context.project_id.as_str())
        else {
            self.status_message = self.text(
                "terminal.ai_restore.no_workspace",
                "No selected workspace for the terminal.",
            );
            self.invalidate_terminal_workspace(cx);
            return;
        };
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &grid,
            &top_ratios,
            pane_count,
        )
        .unwrap_or(TerminalSplitNode::Leaf { pane: 0 });
        let pane_index = pane_count;
        let split_tree = match terminal_split_tree_insert_pane(
            &tree,
            pane_count.saturating_sub(1),
            pane_index,
            TerminalSplitDirection::Right,
        ) {
            Ok(result) => result,
            Err(error) => {
                self.status_message = error.to_string();
                self.invalidate_terminal_workspace(cx);
                return;
            }
        };
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
        let (terminal, attach) = TerminalPane::pending_with_pty_config(
            cx,
            pty_config.clone(),
            self.terminal_config_from_settings(),
        );
        self.register_terminal_pane(pane_terminal_id.as_deref(), &terminal, cx);
        let active_runtime_id = pane_terminal_id.clone();
        let send_result = terminal.send_text(&terminal_command_text(&command));
        if let Some(window) = window.as_deref_mut() {
            terminal.view.read(cx).focus_handle().focus(window, cx);
        }
        if let Some(tab) = self.main_terminal_mut() {
            let insert_index = pane_index.min(tab.panes.len());
            tab.panes.insert(
                insert_index,
                TerminalPaneSlot {
                    title,
                    terminal_id: pane_terminal_id,
                    pane: Some(terminal),
                    restored_output_bytes: 0,
                    restored_output_tail: String::new(),
                },
            );
        }
        self.set_terminal_split_tree(Some(split_tree));
        self.select_active_terminal_runtime_id(active_runtime_id.as_deref());
        if let Some(window) = window {
            self.focus_active_terminal(window, cx);
        }
        if let Err(error) = send_result {
            self.status_message = self
                .text(
                    "terminal.ai_restore.restore_send_failed_format",
                    "AI session split created; restore send failed: %@",
                )
                .replace("%@", &error.to_string());
        } else {
            self.status_message = self.text(
                "terminal.ai_restore.restored",
                "AI session restored in the main split.",
            );
        }
        self.sync_terminal_state_after_layout_change(cx);
        self.spawn_attach_pending_terminals(None, vec![(pty_config, attach)], cx);
        self.invalidate_terminal_workspace(cx);
    }
}
