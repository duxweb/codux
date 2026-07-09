use super::*;

impl CoduxApp {
    pub(in crate::app) fn sync_terminal_state_after_layout_change(
        &mut self,
        _cx: &mut Context<Self>,
    ) {
        self.terminal_restore_epoch = self.terminal_restore_epoch.saturating_add(1);
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

    pub(in crate::app) fn sync_terminal_state_for_project_switch(&mut self) {
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
        let previous_active_terminal_id = self
            .state
            .terminal_layout
            .active_terminal_id
            .trim()
            .to_string();
        let layout_snapshot = self.terminal_layout_snapshot();
        if layout_snapshot.tabs.is_empty() && layout_snapshot.top_panes.is_empty() {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip {reason} layout sync because snapshot is empty"),
            );
            return;
        }
        if let Some(owner_id) = owner_id.as_deref()
            && terminal_panes_are_foreign_to_owner(
                layout_snapshot
                    .top_panes
                    .iter()
                    .chain(layout_snapshot.collapsed_panes.iter()),
                owner_id,
            )
        {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip {reason} layout sync because snapshot owner changed"),
            );
            return;
        }
        let layout = TerminalLayoutSummary {
            active_terminal_id: String::new(),
            top_panes: layout_snapshot.top_panes.clone(),
            tabs: layout_snapshot.tabs.clone(),
            top_ratios: layout_snapshot.top_ratios.clone(),
            top_grid: layout_snapshot.top_grid.clone(),
            split_tree: layout_snapshot.split_tree.clone(),
            bottom_ratio: layout_snapshot.bottom_ratio,
            collapsed_panes: layout_snapshot.collapsed_panes.clone(),
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
        if let Some(remembered) = restored_live_active_terminal_id(
            &self.terminals,
            &previous_active_terminal_id,
            self.remembered_active_terminal_runtime_id().as_deref(),
        ) {
            self.state.terminal_layout.active_terminal_id = remembered;
        }
        self.cache_current_terminal_layout_state();
        self.spawn_persist_terminal_layout_snapshot(storage_key, layout_snapshot);
    }

    pub(in crate::app) fn cache_current_terminal_layout_state(&mut self) {
        let Some(key) = current_worktree_scope_key(&self.state) else {
            return;
        };
        // Don't stamp a layout under a key it doesn't belong to. During a
        // project switch `selected_project` (→ key) can update a beat before
        // `terminal_layout` is swapped, which would cache the PREVIOUS project's
        // panes under the NEW project's key; the runtime-cache restore path would
        // then replay them into the new project (terminal cross-talk).
        if terminal_layout_is_foreign_to_owner(&self.state.terminal_layout, &key.worktree_id) {
            return;
        }
        self.terminal_layout_cache.insert(
            key,
            super::app_state::TerminalLayoutCacheEntry {
                layout: self.state.terminal_layout.clone(),
                runtime: self.state.terminal_runtime.clone(),
            },
        );
    }

    pub(in crate::app) fn spawn_attach_pending_terminals(
        &mut self,
        restore_token: Option<(u64, u64)>,
        pending_terminals: Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)>,
        cx: &mut Context<Self>,
    ) {
        // Drop any pending attach whose terminal is ALREADY being attached by an
        // earlier in-flight call. Each remote attach mints a fresh host PTY, and
        // the only other guard (the pane registry) misses if a recomputed
        // pty_config fails to match the still-pending pane — so without this a
        // racing restore could open two PTYs for one terminal and orphan one. A
        // skipped pane is reused (it's already registered) once the in-flight
        // attach lands. `attaching_ids` is exactly what we marked, so completion
        // clears it regardless of result (panic / generation skip).
        let mut attaching_ids: Vec<String> = Vec::new();
        let pending_terminals: Vec<_> = pending_terminals
            .into_iter()
            .filter(|(_, pending)| match pending.terminal_id() {
                Some(id) => {
                    if self.terminal_attach_in_flight.insert(id.to_string()) {
                        attaching_ids.push(id.to_string());
                        true
                    } else {
                        false
                    }
                }
                None => true,
            })
            .collect();
        if pending_terminals.is_empty() {
            if let Some((expected_generation, expected_restore_epoch)) = restore_token
                && self.project_switch_generation == expected_generation
                && self.terminal_restore_epoch == expected_restore_epoch
            {
                self.terminal_layout_loading = false;
                self.sync_terminal_state_for_project_switch();
                self.terminal_restore_epoch = self.terminal_restore_epoch.saturating_add(1);
                self.invalidate_terminal_workspace(cx);
            }
            return;
        }
        let terminal_manager = self.terminal_manager.clone();
        let runtime_service = self.runtime_service.clone();
        let terminal_config = self.terminal_config_from_settings();
        let attach_started_at = Instant::now();
        // Captured for the ad-hoc (generation=None) completion: if the user
        // switches scope while the attach is in flight, the live terminal
        // set no longer belongs to this storage key and must not be
        // persisted under the new scope's key.
        let spawn_scope_key =
            super::ai_runtime_status::current_terminal_layout_storage_key(&self.state);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let results = codux_runtime::async_runtime::spawn_blocking({
                let terminal_manager = terminal_manager.clone();
                let terminal_config = terminal_config.clone();
                let runtime_service = runtime_service.clone();
                move || {
                    let handles = pending_terminals
                        .into_iter()
                        .map(|(pty_config, pending)| {
                            let terminal_id = pending
                                .terminal_id()
                                .map(str::to_string)
                                .unwrap_or_else(|| "none".to_string());
                            let terminal_manager = terminal_manager.clone();
                            let terminal_config = terminal_config.clone();
                            let runtime_service = runtime_service.clone();
                            thread::spawn(move || {
                                // Remote-hosted projects run the terminal on the
                                // host over the controller; local ones use the PTY.
                                // Use the blocking resolver: on launch the attach
                                // fires before the iroh dial finishes, and it only
                                // runs once — without waiting it failed "not ready
                                // yet" and the pane stayed blank. This is its own
                                // thread, so the bounded wait is free.
                                let result =
                                    if let Some(device_id) = pty_config.host_device_id.clone() {
                                        match runtime_service
                                            .remote_controller_for_device_blocking(&device_id)
                                        {
                                            Ok(controller) => {
                                                TerminalPane::attach_pending_session_remote(
                                                    controller,
                                                    pty_config,
                                                    terminal_config,
                                                    pending,
                                                )
                                                .map(|_| ())
                                                .map_err(|error| error.to_string())
                                            }
                                            Err(error) => Err(error),
                                        }
                                    } else {
                                        TerminalPane::attach_pending_session(
                                            terminal_manager,
                                            pty_config,
                                            terminal_config,
                                            pending,
                                        )
                                        .map(|_| ())
                                        .map_err(|error| error.to_string())
                                    };
                                (terminal_id, result)
                            })
                        })
                        .collect::<Vec<_>>();
                    handles
                        .into_iter()
                        .map(|handle| {
                            handle
                                .join()
                                .unwrap_or_else(|_| ("none".to_string(), Err("panic".to_string())))
                        })
                        .collect::<Vec<_>>()
                }
            })
            .await
            .unwrap_or_else(|error| vec![("none".to_string(), Err(error.to_string()))]);
            let _ = this.update(cx, |app, cx| {
                // Release the in-flight guard for everything we marked, before any
                // early return below, so a terminal is never stuck "attaching".
                for id in &attaching_ids {
                    app.terminal_attach_in_flight.remove(id);
                }
                let ok_count = results.iter().filter(|(_, result)| result.is_ok()).count();
                let error = results.iter().find_map(|(terminal_id, result)| {
                    result
                        .as_ref()
                        .err()
                        .map(|error| format!("{terminal_id}: {error}"))
                });
                app.runtime_trace(
                    "terminal-restore",
                    &format!(
                        "attach_pending elapsed_ms={} ok={} total={} error={}",
                        attach_started_at.elapsed().as_millis(),
                        ok_count,
                        results.len(),
                        error.as_deref().unwrap_or("none")
                    ),
                );
                if let Some((expected_generation, expected_restore_epoch)) = restore_token
                    && (app.project_switch_generation != expected_generation
                        || app.terminal_restore_epoch != expected_restore_epoch)
                {
                    return;
                }
                if restore_token.is_some() {
                    app.terminal_layout_loading = false;
                    if error.is_none() {
                        app.sync_terminal_state_for_project_switch();
                        app.terminal_restore_epoch = app.terminal_restore_epoch.saturating_add(1);
                    }
                } else if error.is_none() {
                    let scope_unchanged =
                        super::ai_runtime_status::current_terminal_layout_storage_key(&app.state)
                            == spawn_scope_key;
                    if scope_unchanged && !app.terminal_layout_loading {
                        app.sync_terminal_state_after_layout_change(cx);
                        // A newly attached split/tab belongs to the current
                        // scope; tell subscribed mobile clients so their split
                        // list reflects it (mirrors the close path, which is the
                        // only mutation that previously broadcast).
                        app.runtime_service.broadcast_remote_terminal_list();
                    }
                }
                if let Some(error) = error {
                    app.status_message = format!("failed to prepare terminal: {error}");
                } else if restore_token.is_some() {
                    app.status_message = format!(
                        "terminal layout reloaded · {} tab{}",
                        app.terminals.len(),
                        if app.terminals.len() == 1 { "" } else { "s" }
                    );
                }
                app.invalidate_terminal_workspace(cx);
            });
        })
        .detach();
    }

    pub(in crate::app) fn cached_terminal_layout_state(
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
        let mut sessions: Vec<TerminalRuntimeSessionSummary> = self
            .terminals
            .iter()
            .flat_map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .filter_map(|(pane_index, slot)| {
                        let terminal_id = Self::terminal_slot_terminal_id(tab, pane_index, slot)?;
                        Some(self.lightweight_session_summary_for_slot(
                            terminal_id,
                            slot,
                            &base_pty_config,
                            &existing_by_key,
                            now,
                        ))
                    })
            })
            .collect::<Vec<_>>();
        for slot in &self.collapsed_terminal_panes {
            let Some(terminal_id) = Self::collapsed_slot_terminal_id(slot) else {
                continue;
            };
            if sessions
                .iter()
                .any(|session| session.terminal_id == terminal_id)
            {
                continue;
            }
            sessions.push(self.lightweight_session_summary_for_slot(
                terminal_id,
                slot,
                &base_pty_config,
                &existing_by_key,
                now,
            ));
        }
        TerminalRuntimeSummary {
            path: self.state.terminal_runtime.path.clone(),
            active_terminal_id,
            open_count: sessions.len(),
            closed_count: 0,
            sessions,
            error: None,
        }
    }

    pub(in crate::app) fn terminal_runtime_snapshot(
        &self,
    ) -> (String, Vec<TerminalRuntimeSessionInput>) {
        let active_terminal_id = self.active_terminal_runtime_id();
        let base_pty_config = self.current_terminal_base_pty_config();
        let mut sessions: Vec<TerminalRuntimeSessionInput> = self
            .terminals
            .iter()
            .flat_map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .filter_map(|(pane_index, slot)| {
                        let terminal_id = Self::terminal_slot_terminal_id(tab, pane_index, slot)?;
                        Some(self.terminal_session_input_for_slot(
                            terminal_id,
                            slot,
                            &base_pty_config,
                        ))
                    })
            })
            .collect();
        for slot in &self.collapsed_terminal_panes {
            let Some(terminal_id) = Self::collapsed_slot_terminal_id(slot) else {
                continue;
            };
            if sessions
                .iter()
                .any(|session| session.terminal_id == terminal_id)
            {
                continue;
            }
            sessions.push(self.terminal_session_input_for_slot(
                terminal_id,
                slot,
                &base_pty_config,
            ));
        }
        (active_terminal_id, sessions)
    }

    /// Trimmed stable terminal id of a collapsed slot, if it has one.
    fn collapsed_slot_terminal_id(slot: &TerminalPaneSlot) -> Option<String> {
        slot.terminal_id
            .as_deref()
            .map(str::trim)
            .filter(|terminal_id| !terminal_id.is_empty())
            .map(str::to_string)
    }

    /// Session summary for one visible or collapsed pane slot, carrying
    /// counters forward from the previous runtime summary when the session is
    /// already known.
    fn lightweight_session_summary_for_slot(
        &self,
        terminal_id: String,
        slot: &TerminalPaneSlot,
        base_pty_config: &TerminalPtyConfig,
        existing_by_key: &HashMap<String, &TerminalRuntimeSessionSummary>,
        now: f64,
    ) -> TerminalRuntimeSessionSummary {
        let project = self.state.selected_project.as_ref();
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
        TerminalRuntimeSessionSummary {
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
        }
    }

    /// Persistable session input for one visible or collapsed pane slot; live
    /// input/output snapshots come from the pane view when mounted, otherwise
    /// from the terminal manager, falling back to the restored tail.
    fn terminal_session_input_for_slot(
        &self,
        terminal_id: String,
        slot: &TerminalPaneSlot,
        base_pty_config: &TerminalPtyConfig,
    ) -> TerminalRuntimeSessionInput {
        let project = self.state.selected_project.as_ref();
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
    }

    pub(in crate::app) fn restore_collapsed_panes_for_layout(
        &mut self,
        filter_dead_sessions: bool,
        cx: &mut Context<Self>,
    ) {
        self.collapsed_terminal_panes = collapsed_terminal_slots_from_layout(
            &self.state.terminal_layout,
            &self.state.terminal_runtime,
            filter_dead_sessions,
            &self.terminal_pane_registry,
            &self.terminal_manager,
        );
        self.invalidate_task_column(cx);
    }

    pub(in crate::app) fn terminal_layout_snapshot(&self) -> TerminalLayoutSnapshot {
        let tabs = Vec::new();
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
        let top_grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            top_panes.len(),
        );
        let split_tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &top_grid,
            &top_ratios,
            top_panes.len(),
        );
        TerminalLayoutSnapshot {
            tabs,
            top_panes,
            top_ratios,
            top_grid,
            split_tree,
            bottom_ratio: self.state.terminal_layout.bottom_ratio,
            collapsed_panes: self
                .collapsed_terminal_panes
                .iter()
                .filter_map(|slot| {
                    let terminal_id = slot.terminal_id.as_deref()?.trim();
                    if terminal_id.is_empty() {
                        return None;
                    }
                    Some(terminal_pane_summary(slot))
                })
                .collect(),
        }
    }

    pub(in crate::app) fn remember_focused_terminal_for_current_scope(
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

    pub(in crate::app) fn remembered_active_terminal_runtime_id(&self) -> Option<String> {
        let key = current_worktree_scope_key(&self.state)?;
        self.active_terminal_runtime_ids.get(&key).cloned()
    }

    pub(in crate::app) fn update_terminal_split_ratios(
        &mut self,
        layout_key: String,
        path: Vec<usize>,
        ratios: Vec<f64>,
        cx: &mut Context<Self>,
    ) {
        if super::ai_runtime_status::current_terminal_layout_storage_key(&self.state).as_deref()
            != Some(layout_key.as_str())
        {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip stale grid ratios layout={layout_key}"),
            );
            return;
        }
        let pane_count = self.main_terminal().map(|tab| tab.panes.len()).unwrap_or(0);
        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            pane_count,
        );
        let current = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            pane_count,
        );
        let current_tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &current,
            &top_ratios,
            pane_count,
        );
        let Some(current_tree) = current_tree else {
            return;
        };
        let next_tree = terminal_split_tree_update_ratios(&current_tree, &path, ratios);
        if terminal_split_tree_equal(&Some(current_tree), &Some(next_tree.clone())) {
            return;
        }
        if self.terminal_layout_loading {
            self.runtime_trace(
                "terminal-layout",
                &format!("skip resize_grid while loading layout={layout_key}"),
            );
            return;
        }
        self.set_terminal_split_tree(Some(next_tree));
        self.persist_current_terminal_layout();
        self.invalidate_terminal_workspace(cx);
    }

    pub(in crate::app) fn apply_terminal_layout_from_summary(
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
        );
        self.state.terminal_layout.active_terminal_id =
            restore_plan.active_terminal_id.clone().unwrap_or_default();
        self.runtime_trace(
            "terminal-restore",
            &format!(
                "plan elapsed_ms={} owner={} tabs={} active_index={} active_runtime={}",
                plan_started_at.elapsed().as_millis(),
                owner_id.as_deref().unwrap_or("none"),
                restore_plan.tabs.len(),
                restore_plan.active_index,
                restore_plan.active_terminal_id.as_deref().unwrap_or("none")
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
        // Project-switch / layout-reload restore. A remote-hosted project's
        // terminals are deferred into `pending` and attached on the host through
        // the async chokepoint (local terminals still spawn synchronously inside
        // `spawn_terminal_tabs`).
        let mut pending: Vec<(TerminalPtyConfig, crate::terminal::PendingTerminalAttach)> =
            Vec::new();
        match spawn_terminal_tabs(
            &restore_plan,
            self.terminal_manager.clone(),
            launch_context.as_ref(),
            &base_pty_config,
            terminal_config,
            &self.terminal_pane_registry,
            Some(&mut pending),
            cx,
        ) {
            Ok((terminals, active_terminal_id, next_terminal_index)) => {
                let tab_count = terminals.len();
                self.terminals = terminals;
                self.active_terminal_id = active_terminal_id;
                self.next_terminal_index = next_terminal_index;
                self.register_terminal_panes(cx);
                self.restore_collapsed_panes_for_layout(true, cx);
                self.spawn_attach_pending_terminals(None, pending, cx);
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
}

#[derive(Clone)]
pub(in crate::app) struct TerminalLayoutSnapshot {
    pub(in crate::app) tabs: Vec<TerminalTabSummary>,
    pub(in crate::app) top_panes: Vec<TerminalPaneSummary>,
    pub(in crate::app) top_ratios: Vec<f64>,
    pub(in crate::app) top_grid: TerminalTopGrid,
    pub(in crate::app) split_tree: Option<TerminalSplitNode>,
    pub(in crate::app) bottom_ratio: f64,
    pub(in crate::app) collapsed_panes: Vec<TerminalPaneSummary>,
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

pub(in crate::app) fn terminal_runtime_id_exists_in(
    terminals: &[TerminalTab],
    terminal_id: &str,
) -> bool {
    let terminal_id = terminal_id.trim();
    !terminal_id.is_empty()
        && terminals.iter().any(|tab| {
            tab.terminal_id.as_deref() == Some(terminal_id)
                || tab
                    .panes
                    .iter()
                    .any(|slot| slot.terminal_id.as_deref() == Some(terminal_id))
        })
}

pub(in crate::app) fn restored_live_active_terminal_id(
    terminals: &[TerminalTab],
    previous_active_terminal_id: &str,
    remembered_active_terminal_id: Option<&str>,
) -> Option<String> {
    let previous = previous_active_terminal_id.trim();
    if terminal_runtime_id_exists_in(terminals, previous) {
        return Some(previous.to_string());
    }
    remembered_active_terminal_id
        .map(str::trim)
        .filter(|terminal_id| terminal_runtime_id_exists_in(terminals, terminal_id))
        .map(str::to_string)
}

// Shell prompts commonly set "user@host[:path]" or a bare cwd path as the OSC
// title; both are prompt noise — drop them so the shell-name default shows.
pub(super) fn normalized_terminal_osc_title(title: &str) -> Option<String> {
    let title = title.trim();
    if title.is_empty() {
        return None;
    }
    let head = title.split([':', ' ']).next().unwrap_or(title);
    if head.contains('@') && !head.contains('/') {
        return None;
    }
    if title.starts_with('/') || title.starts_with('~') {
        return None;
    }
    Some(title.to_string())
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

#[cfg(test)]
mod osc_title_tests {
    use super::normalized_terminal_osc_title;

    #[test]
    fn normalized_osc_title_strips_prompt_noise() {
        assert_eq!(
            normalized_terminal_osc_title("lixinhua@MacBook-Pro.local:~/project"),
            None
        );
        assert_eq!(
            normalized_terminal_osc_title("lixinhua@MacBook-Pro.local ~"),
            None
        );
        assert_eq!(
            normalized_terminal_osc_title("/Volumes/Data/Projects/demo"),
            None
        );
        assert_eq!(normalized_terminal_osc_title("~/project"), None);
        assert_eq!(
            normalized_terminal_osc_title("lixinhua@MacBook-Pro.local:"),
            None
        );
        assert_eq!(
            normalized_terminal_osc_title("lixinhua@MacBook-Pro.local"),
            None
        );
        assert_eq!(normalized_terminal_osc_title("  "), None);
        assert_eq!(
            normalized_terminal_osc_title("vim: notes.txt"),
            Some("vim: notes.txt".to_string())
        );
        assert_eq!(
            normalized_terminal_osc_title("dartvm"),
            Some("dartvm".to_string())
        );
    }
}
