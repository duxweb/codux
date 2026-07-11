use super::*;

impl RemoteHostRuntime {
    pub(super) fn send_terminal_buffer(
        self: &Arc<Self>,
        session_id: &str,
        device_id: Option<&str>,
        offset: usize,
        mut options: TerminalBaselineOptions,
    ) {
        self.register_terminal_viewer(session_id, device_id);
        if !options.tail
            || (options.viewport.is_some()
                && !self.apply_terminal_baseline_viewport(session_id, device_id, options.viewport))
        {
            options.viewport = None;
        }
        let fallback_request_id = options.request_id.clone();
        let chunk_chars = options.chunk_chars;
        let tail = options.tail;
        match self.terminal_buffer_window(session_id, offset, options) {
            Ok(window) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!(
                        "terminal_baseline_send session={session_id} device={} chars={} tail={} screen={}",
                        device_id.unwrap_or("none"),
                        window.total_characters,
                        window.tail,
                        window.screen_data.is_some()
                    ),
                );
                let output_seq = window
                    .output_seq
                    .unwrap_or_else(|| self.current_terminal_output_seq(session_id));
                for payload in terminal_buffer_payloads(&window, output_seq, chunk_chars) {
                    self.send_terminal_data(
                        REMOTE_TERMINAL_OUTPUT,
                        device_id,
                        Some(session_id),
                        payload,
                    );
                }
            }
            Err(error) => {
                crate::runtime_trace::runtime_trace(
                    "remote",
                    &format!("terminal_buffer baseline_failed session={session_id} error={error}"),
                );
                let output_seq = self.current_terminal_output_seq(session_id);
                let fallback = RemoteTerminalBufferWindow {
                    data: String::new(),
                    screen_data: None,
                    offset: 0,
                    total_characters: 0,
                    truncated: false,
                    output_seq: Some(output_seq),
                    request_id: fallback_request_id,
                    tail,
                    has_previous: false,
                    baseline_failed: true,
                };
                for payload in terminal_buffer_payloads(&fallback, output_seq, chunk_chars) {
                    self.send_terminal_data(
                        REMOTE_TERMINAL_OUTPUT,
                        device_id,
                        Some(session_id),
                        payload,
                    );
                }
            }
        }
    }

    pub(super) fn apply_terminal_baseline_viewport(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        viewport: Option<BaselineViewport>,
    ) -> bool {
        let (Some(device_id), Some(viewport)) = (
            device_id.map(str::trim).filter(|value| !value.is_empty()),
            viewport,
        ) else {
            return false;
        };
        let owner = terminal_viewport_remote_owner(device_id);
        let Ok(state) = self.terminals.viewport_state(session_id) else {
            return false;
        };
        if state.owner != owner {
            return false;
        }
        self.terminals.touch_viewport_lease(session_id, &owner);
        self.terminals
            .resize_viewport(session_id, &owner, viewport.cols, viewport.rows)
            .ok()
            .flatten()
            .is_some()
    }

    pub(super) fn send_terminal_viewport_state(&self, session_id: &str, device_id: Option<&str>) {
        if let Ok(state) = self.terminals.viewport_state(session_id) {
            self.send_terminal_viewport_state_payload(session_id, device_id, None, &state);
        }
    }

    pub(super) fn send_terminal_viewport_state_payload(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        request_id: Option<&str>,
        state: &TerminalViewportState,
    ) {
        self.send_transport_with_request_id(
            REMOTE_TERMINAL_VIEWPORT_STATE,
            device_id,
            Some(session_id),
            request_id,
            self.terminal_viewport_state_payload(session_id, device_id, state),
        );
    }

    pub(super) fn terminal_viewport_state_payload(
        &self,
        session_id: &str,
        device_id: Option<&str>,
        state: &TerminalViewportState,
    ) -> Value {
        json!({
            "owner": state.owner,
            "cols": state.cols,
            "rows": state.rows,
            "generation": state.generation,
            "staleOutput": device_id.is_some() && self.terminal_viewer_is_stale(session_id, device_id),
            "outputSeq": self.current_terminal_output_seq(session_id),
        })
    }

    pub(super) fn terminal_buffer_window(
        &self,
        session_id: &str,
        offset: usize,
        options: TerminalBaselineOptions,
    ) -> Result<RemoteTerminalBufferWindow, anyhow::Error> {
        let max_chars = options.max_chars.max(1);
        if options.tail {
            // ROOT FIX for the stale remote screen (e.g. Claude's classic-mode
            // input box frozen at its old row while "working").
            //
            // The per-frame sequence the viewer uses to order/dedup output is the
            // host's flush counter (one ++ per forwarded batch), but the screen
            // and history advance continuously as the PTY is processed. So at any
            // instant the screen/history already reflect a pending un-flushed
            // batch, while the sequence still reads the previous flushed frame —
            // a baseline built from "current screen + current sequence" is
            // therefore inconsistent (its screen is ahead of its label). The
            // viewer then either drops the in-between frames (stale, what was
            // reported) or re-appends them (double).
            //
            // Flush the pending batch FIRST so its data is assigned a sequence
            // and forwarded before the baseline. Read the resulting sequence and
            // release the process-wide sequence map lock before snapshotting the
            // PTY history: snapshot_tail competes with the live PTY writer for the
            // per-session history lock, and holding the global sequence lock while
            // waiting on that would let one busy terminal block unrelated sessions.
            // The consumer-side sequence guard already tolerates the tiny
            // seq/history skew that can happen after releasing the map lock.
            self.flush_terminal_output_batch(session_id);
            let output_seq = self
                .terminal_output_seq_by_session
                .lock()
                .as_ref()
                .ok()
                .and_then(|sequences| sequences.get(session_id).copied())
                .unwrap_or(0);
            let (data, start_offset) = self.terminals.snapshot_tail(session_id, max_chars)?;
            let total_characters = self
                .terminals
                .buffer_characters(session_id)
                .unwrap_or_else(|_| start_offset + data.chars().count());
            // Every tail baseline ships a screen keyframe. Raw history replay
            // alone cannot reconstruct the screen of a primary-buffer TUI that
            // redraws in place (codex spinner): the tail starts mid-stream, its
            // cursor-up/erase frames replay against the wrong rows, and the
            // viewer shows stacked garbage lines. The keyframe opens with a
            // home+2J wipe, so it authoritatively replaces whatever the tail
            // replay left on the visible screen; since the single-owner grid
            // model (2985838) viewers render the owner's grid 1:1, so a
            // host-grid keyframe can no longer land at reflowed rows (the old
            // ghost-prompt reason to omit it). When the requester owns the
            // viewport the keyframe is rendered at the just-reflowed target grid.
            let screen_data = if options.viewport.is_some() {
                let max_lines = options
                    .viewport
                    .map(|viewport| viewport.rows.max(8) as usize)
                    .unwrap_or(0);
                self.terminals
                    .remote_viewport_snapshot(session_id, 0, 0, max_lines)
                    .ok()
                    .map(|snapshot| snapshot.data)
            } else {
                self.terminals
                    .screen_snapshot(session_id)
                    .ok()
                    .map(|snapshot| snapshot.data)
            }
            .filter(|data| !data.is_empty());
            return Ok(RemoteTerminalBufferWindow {
                data,
                screen_data,
                offset: start_offset,
                total_characters,
                truncated: false,
                output_seq: Some(output_seq),
                request_id: options.request_id,
                tail: true,
                has_previous: start_offset > 0,
                baseline_failed: false,
            });
        }

        let request_id_for_window = options.request_id.clone();
        let frozen = options
            .request_id
            .as_deref()
            .and_then(|request_id| {
                self.terminal_buffer_baseline(session_id, request_id, offset, max_chars)
                    .transpose()
            })
            .transpose()?;
        let (data, start_offset, total_characters, output_seq) = match frozen {
            Some(baseline) => (
                baseline.data,
                baseline.start_offset,
                baseline.total_characters,
                Some(baseline.output_seq),
            ),
            None => {
                let data = self.terminals.snapshot(session_id)?;
                let total_characters = data.chars().count();
                (data, 0, total_characters, None)
            }
        };
        let clamped = offset.max(start_offset).min(total_characters);
        let chunk = data
            .chars()
            .skip(clamped.saturating_sub(start_offset))
            .take(max_chars)
            .collect::<String>();
        let truncated = clamped + chunk.chars().count() < total_characters;
        if !truncated && let Some(request_id) = request_id_for_window.as_deref() {
            self.remove_terminal_buffer_baseline(session_id, request_id);
        }
        Ok(RemoteTerminalBufferWindow {
            data: chunk,
            screen_data: None,
            offset: clamped,
            total_characters,
            truncated,
            output_seq,
            request_id: request_id_for_window,
            tail: false,
            has_previous: clamped > 0,
            baseline_failed: false,
        })
    }

    pub(super) fn terminal_buffer_baseline(
        &self,
        session_id: &str,
        request_id: &str,
        offset: usize,
        max_chars: usize,
    ) -> Result<Option<RemoteTerminalBufferBaseline>, anyhow::Error> {
        let key = terminal_buffer_baseline_key(session_id, request_id);
        let now = Instant::now();
        if let Ok(mut baselines) = self.terminal_buffer_baselines.lock() {
            baselines.retain(|_, baseline| {
                now.duration_since(baseline.created_at) <= REMOTE_TERMINAL_BUFFER_BASELINE_TTL
            });
            if let Some(baseline) = baselines.get(&key) {
                return Ok(Some(RemoteTerminalBufferBaseline {
                    data: baseline.data.clone(),
                    start_offset: baseline.start_offset,
                    total_characters: baseline.total_characters,
                    output_seq: baseline.output_seq,
                    created_at: baseline.created_at,
                }));
            }
        }
        if offset != 0 {
            return Ok(None);
        }

        let data = self.terminals.snapshot(session_id)?;
        let total_characters = data.chars().count();
        let baseline = RemoteTerminalBufferBaseline {
            data,
            start_offset: 0,
            total_characters,
            output_seq: self.current_terminal_output_seq(session_id),
            created_at: now,
        };
        let returned = RemoteTerminalBufferBaseline {
            data: baseline.data.clone(),
            start_offset: baseline.start_offset,
            total_characters: baseline.total_characters,
            output_seq: baseline.output_seq,
            created_at: baseline.created_at,
        };
        if max_chars < total_characters
            && let Ok(mut baselines) = self.terminal_buffer_baselines.lock()
        {
            baselines.insert(key, baseline);
        }
        Ok(Some(returned))
    }

    pub(super) fn remove_terminal_buffer_baseline(&self, session_id: &str, request_id: &str) {
        if let Ok(mut baselines) = self.terminal_buffer_baselines.lock() {
            baselines.remove(&terminal_buffer_baseline_key(session_id, request_id));
        }
    }

    pub(super) fn remote_terminal_payload(&self, session_id: &str) -> Option<Value> {
        self.remote_terminals()
            .into_iter()
            .find(|value| value.get("id").and_then(Value::as_str) == Some(session_id))
    }

    pub(super) fn remote_terminals(&self) -> Vec<Value> {
        let baseline = ProjectStore::new(self.support_dir.clone()).snapshot();
        let mut workspace_scopes = HashMap::new();
        for project in &baseline.projects {
            workspace_scopes.insert(project.id.clone(), (project.id.clone(), project.id.clone()));
        }
        for worktree in &baseline.worktrees {
            workspace_scopes.insert(
                worktree.id.clone(),
                (worktree.project_id.clone(), worktree.id.clone()),
            );
        }
        let scopes = self.remote_terminal_layout_scopes();
        let mut terminals = self
            .terminals
            .list()
            .into_iter()
            .filter(|terminal| terminal.is_running)
            .map(|terminal| {
                let fallback_worktree_id = terminal.project_id.clone();
                let workspace_scope = workspace_scopes.get(&terminal.project_id);
                let layout_scope = scopes.get(&terminal.id);
                let project_id = layout_scope
                    .map(|scope| scope.project_id.as_str())
                    .or_else(|| workspace_scope.map(|(project_id, _)| project_id.as_str()));
                let worktree_id = layout_scope
                    .map(|scope| scope.worktree_id.as_str())
                    .or_else(|| workspace_scope.map(|(_, worktree_id)| worktree_id.as_str()))
                    .or_else(|| {
                        (!fallback_worktree_id.trim().is_empty())
                            .then_some(fallback_worktree_id.as_str())
                    });
                let layout_order = layout_scope.map(|scope| scope.layout_order);
                let mut payload =
                    remote_terminal_snapshot_payload(terminal, worktree_id, layout_order);
                if let Some(project_id) = project_id.filter(|value| !value.trim().is_empty()) {
                    payload["projectId"] = json!(project_id);
                }
                payload
            })
            .collect::<Vec<_>>();
        terminals.sort_by_key(remote_terminal_order_key);
        terminals
    }

    pub(super) fn remote_terminal_layout_scopes(
        &self,
    ) -> HashMap<String, RemoteTerminalLayoutScope> {
        let project_store = ProjectStore::new(self.support_dir.clone());
        let baseline = project_store.snapshot();
        let mut keys = Vec::new();
        let mut seen = HashSet::new();
        for project in &baseline.projects {
            let default_key = terminal_layout_storage_key(&project.id, &project.id);
            if seen.insert(default_key.clone()) {
                keys.push(default_key);
            }
            for worktree in baseline
                .worktrees
                .iter()
                .filter(|worktree| worktree.project_id == project.id)
            {
                let worktree_key = terminal_layout_storage_key(&project.id, &worktree.id);
                if seen.insert(worktree_key.clone()) {
                    keys.push(worktree_key);
                }
            }
        }
        let layouts = TerminalLayoutService::new(self.support_dir.clone())
            .load_many(keys.iter().map(String::as_str));
        let mut result = HashMap::new();
        for layout_key in keys {
            let Some(layout) = layouts.get(&layout_key) else {
                continue;
            };
            let Some((project_id, worktree_id)) = runtime_scope_parts(&layout_key) else {
                continue;
            };
            let project_id = project_id.to_string();
            let worktree_id = worktree_id.to_string();
            let mut layout_order = 0;
            for pane in &layout.top_panes {
                result.insert(
                    pane.terminal_id.clone(),
                    RemoteTerminalLayoutScope {
                        layout_key: layout_key.clone(),
                        project_id: project_id.clone(),
                        worktree_id: worktree_id.clone(),
                        layout_order,
                    },
                );
                layout_order += 1;
            }
            for tab in &layout.tabs {
                result.insert(
                    tab.terminal_id.clone(),
                    RemoteTerminalLayoutScope {
                        layout_key: layout_key.clone(),
                        project_id: project_id.clone(),
                        worktree_id: worktree_id.clone(),
                        layout_order,
                    },
                );
                layout_order += 1;
            }
        }
        result
    }

    pub(super) fn remove_remote_terminal_from_layout(&self, terminal_id: &str) -> bool {
        let scopes = self.remote_terminal_layout_scopes();
        let Some(scope) = scopes.get(terminal_id) else {
            return true;
        };
        let service = TerminalLayoutService::new(self.support_dir.clone());
        let mut layout = service.load(Some(&scope.layout_key));
        let before_total = layout.top_panes.len() + layout.tabs.len();
        layout
            .top_panes
            .retain(|pane| pane.terminal_id != terminal_id);
        layout.tabs.retain(|tab| tab.terminal_id != terminal_id);
        let after_total = layout.top_panes.len() + layout.tabs.len();
        // Nothing matched: this layout summary didn't actually own the terminal.
        if after_total == before_total {
            return false;
        }
        // A controller may close the LAST terminal in a layout (after_total == 0).
        // Persist the now-empty summary so the desktop reconcile tears its pane
        // down too. Previously closing the only terminal in a worktree bailed out
        // here (before_total <= 1 / after_total == 0), so it silently no-opped on
        // both ends -- the desktop split AND the pad tab both lingered.
        let _ = service.save_summary(&scope.layout_key, layout);
        true
    }
}

pub(crate) fn remote_terminal_order_key(value: &Value) -> (String, String) {
    runtime_terminal::terminal_order_key(value)
}

pub(super) fn terminal_buffer_baseline_key(session_id: &str, request_id: &str) -> String {
    format!("{session_id}:{request_id}")
}

pub(crate) fn remote_terminal_snapshot_payload(
    terminal: TerminalSessionSnapshot,
    worktree_id: Option<&str>,
    layout_order: Option<usize>,
) -> Value {
    let mut payload = runtime_terminal::terminal_snapshot_payload(terminal);
    if let Some(worktree_id) = worktree_id.filter(|value| !value.trim().is_empty()) {
        payload["worktreeId"] = json!(worktree_id);
    }
    if let Some(layout_order) = layout_order {
        payload["layoutOrder"] = json!(layout_order);
    }
    payload
}
