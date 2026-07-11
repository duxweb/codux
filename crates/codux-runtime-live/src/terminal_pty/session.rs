use super::*;

type TerminalPtySpawnResult = (
    TerminalPtySession,
    Box<dyn Write + Send>,
    Box<dyn Read + Send>,
);

pub struct TerminalPtySession {
    pub(super) id: String,
    pub(super) stdin_writer: Arc<parking_lot::Mutex<Box<dyn Write + Send>>>,
    pub(super) input_capture: Arc<parking_lot::Mutex<TerminalInputCapture>>,
    pub(super) output_capture: Arc<parking_lot::Mutex<TerminalOutputCapture>>,
    pub(super) history: Arc<parking_lot::Mutex<RingHistory>>,
    pub(super) screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
    pub(super) output_subscribers: Arc<parking_lot::Mutex<Vec<flume::Sender<Vec<u8>>>>>,
    pub(super) event_subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
    pub(super) info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    pub(super) exit_signal: Arc<TerminalExitSignal>,
    pub(super) ai_runtime_binding: AIRuntimeTerminalBinding,
    pub(super) pty_control: LocalPtyProcessHandle,
    pub(super) viewport: Arc<parking_lot::Mutex<TerminalViewportLease>>,
    pub(super) remote_screen_scrollback: usize,
    pub(super) remote_screen_current_scrollback: parking_lot::Mutex<usize>,
}

pub(super) struct TerminalExitSignal {
    exit_code: StdMutex<Option<Option<i32>>>,
    ready: Condvar,
}

#[derive(Clone)]
pub struct TerminalPtySessionHandle {
    pub(super) pty_control: LocalPtyProcessHandle,
    pub(super) info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    pub(super) viewport: Arc<parking_lot::Mutex<TerminalViewportLease>>,
    pub(super) event_subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
    pub(super) screen: Arc<parking_lot::Mutex<HeadlessTerminalScreen>>,
}

impl TerminalPtySession {
    pub(super) fn spawn(
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
        event_sink: Option<(Option<String>, EventSink)>,
    ) -> Result<TerminalPtySpawnResult> {
        let id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| Uuid::new_v4().to_string());
        let cols = config.cols.unwrap_or(100).max(20);
        let rows = config.rows.unwrap_or(32).max(8);
        let shell = config
            .shell
            .as_deref()
            .and_then(normalize_terminal_shell)
            .unwrap_or_else(default_shell);
        let cwd = requested_terminal_cwd(&config, context);
        let initial_command = config
            .command
            .clone()
            .filter(|value| !value.trim().is_empty());

        let environment = terminal_environment(&shell, cwd.as_deref(), &id, &config, context);
        let process = spawn_local_pty(LocalPtySpawnConfig {
            shell: shell.clone(),
            cwd: cwd.clone(),
            initial_command: initial_command.clone(),
            cols,
            rows,
            env: environment,
            clear_env: true,
            command_mode: LocalPtyCommandMode::InteractiveLogin,
        })
        .map_err(|error| {
            // Log the raw errno; this is the only record when a terminal silently fails to open (EMFILE/EPERM under fd pressure or a sandbox denial).
            crate::ai_runtime::runtime_log_line(
                "terminal",
                &format!("spawn failed shell={shell} cwd={cwd:?}: {error}"),
            );
            anyhow::Error::msg(error)
        })
        .with_context(|| format!("failed to spawn shell {shell}"))?;
        let stdin_writer = Arc::new(parking_lot::Mutex::new(process.writer));
        let input_capture = Arc::new(parking_lot::Mutex::new(TerminalInputCapture::new(
            INPUT_CAPTURE_LIMIT,
        )));
        let output_capture = Arc::new(parking_lot::Mutex::new(TerminalOutputCapture::new(
            OUTPUT_CAPTURE_LIMIT,
        )));
        let history = Arc::new(parking_lot::Mutex::new(RingHistory::new(
            terminal_history_bytes(config.scrollback_lines, cols),
        )));
        let remote_screen_scrollback = remote_screen_scrollback_lines(config.scrollback_lines);
        let initial_remote_screen_scrollback =
            initial_remote_screen_scrollback_lines(remote_screen_scrollback);
        let screen = Arc::new(parking_lot::Mutex::new(HeadlessTerminalScreen::new(
            cols as usize,
            rows as usize,
            initial_remote_screen_scrollback,
        )));
        let output_subscribers = Arc::new(parking_lot::Mutex::new(Vec::new()));
        let event_subscribers = Arc::new(parking_lot::Mutex::new(Vec::new()));
        if let Some((event_key, event_sink)) = event_sink {
            if let Some(event_key) = event_key {
                insert_keyed_event_subscriber(&event_subscribers, event_key, event_sink);
            } else {
                event_subscribers
                    .lock()
                    .push(EventSubscriber::anonymous(event_sink));
            }
        }
        let now = rfc3339_now();
        let project_path = context
            .map(|context| context.project_path.display().to_string())
            .unwrap_or_else(|| cwd.clone().unwrap_or_default());
        let project_name = config
            .project_name
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.map(|context| context.project_name.clone()))
            .or_else(|| project_path_name(&project_path))
            .unwrap_or_else(|| "Codux".to_string());
        let project_id = config
            .project_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.map(|context| context.project_id.clone()))
            .unwrap_or_else(|| project_name.clone());
        let root_project_id = config
            .root_project_id
            .clone()
            .or_else(|| context.map(|context| context.root_project_id.clone()))
            .filter(|value| !value.trim().is_empty());
        let worktree_id = config
            .worktree_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let binding_worktree_id = worktree_id.clone();
        let slot_id = config
            .slot_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.slot_id.clone()))
            .unwrap_or_default();
        let session_key = config
            .session_key
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.session_key.clone()));
        let session_instance_id = config
            .session_instance_id
            .clone()
            .filter(|value| !value.trim().is_empty())
            .or_else(|| context.and_then(|context| context.session_instance_id.clone()))
            .or_else(|| Some(Uuid::new_v4().to_string().to_lowercase()));
        let title = config
            .title
            .clone()
            .unwrap_or_else(|| "Terminal".to_string());
        let info_cwd = cwd.clone().unwrap_or(project_path);
        let info = Arc::new(parking_lot::Mutex::new(TerminalSessionSnapshot {
            id: id.clone(),
            title: title.clone(),
            slot_id: slot_id.clone(),
            session_key: session_key.clone(),
            project_id: project_id.clone(),
            worktree_id,
            project_name,
            cwd: info_cwd.clone(),
            shell: shell.clone(),
            command: initial_command.clone().unwrap_or_default(),
            cols,
            rows,
            status: "running".to_string(),
            is_running: true,
            created_at: now.clone(),
            last_active_at: now,
            buffer_characters: 0,
            has_buffer: false,
            tool: config.tool.clone(),
        }));
        let exit_signal = Arc::new(TerminalExitSignal::new());
        let viewport = Arc::new(parking_lot::Mutex::new(TerminalViewportLease {
            state: TerminalViewportState {
                owner: terminal_viewport_local_owner().to_string(),
                cols,
                rows,
                generation: 0,
                owner_label: None,
            },
            expires_at: Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL,
            explicit_owner: false,
        }));
        let ai_runtime_binding = AIRuntimeTerminalBinding {
            terminal_id: id.clone(),
            root_project_id,
            worktree_id: binding_worktree_id,
            project_id,
            slot_id,
            title,
            cwd: info_cwd,
            tool: config.tool.clone(),
            is_active: false,
            session_key,
            terminal_instance_id: session_instance_id,
        };
        spawn_waiter(
            id.clone(),
            process.control.clone(),
            info.clone(),
            event_subscribers.clone(),
            exit_signal.clone(),
        );

        let terminal_writer = CaptureWriter::new(stdin_writer.clone(), input_capture.clone());
        let terminal_reader = CaptureReader::new(
            id.clone(),
            process.reader,
            CaptureReaderShared {
                output_capture: output_capture.clone(),
                history: history.clone(),
                screen: screen.clone(),
                output_subscribers: output_subscribers.clone(),
                event_subscribers: event_subscribers.clone(),
                info: info.clone(),
            },
        );
        Ok((
            Self {
                id,
                stdin_writer,
                input_capture,
                output_capture,
                history,
                screen,
                output_subscribers,
                event_subscribers,
                info,
                exit_signal,
                ai_runtime_binding,
                pty_control: process.control,
                viewport,
                remote_screen_scrollback,
                remote_screen_current_scrollback: parking_lot::Mutex::new(
                    initial_remote_screen_scrollback,
                ),
            },
            Box::new(terminal_writer),
            Box::new(terminal_reader),
        ))
    }

    pub fn id(&self) -> &str {
        &self.id
    }

    pub fn clone_handle(&self) -> TerminalPtySessionHandle {
        TerminalPtySessionHandle {
            pty_control: self.pty_control.clone(),
            info: self.info.clone(),
            viewport: self.viewport.clone(),
            event_subscribers: self.event_subscribers.clone(),
            screen: self.screen.clone(),
        }
    }

    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.clone_handle().resize(cols, rows)
    }

    pub fn claim_viewport(&self, owner: &str) -> Result<TerminalViewportState> {
        self.clone_handle().claim_viewport(owner)
    }

    pub fn claim_viewport_auto(&self, owner: &str) -> Result<TerminalViewportState> {
        self.clone_handle().claim_viewport_auto(owner)
    }

    pub fn release_viewport(&self, owner: &str) -> Result<Option<TerminalViewportState>> {
        self.clone_handle().release_viewport(owner)
    }

    pub fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>> {
        self.clone_handle().resize_viewport(owner, cols, rows)
    }

    pub fn set_viewport_owner_label(&self, owner: &str, label: Option<String>) {
        self.clone_handle().set_viewport_owner_label(owner, label)
    }

    pub fn viewport_state(&self) -> TerminalViewportState {
        self.clone_handle().viewport_state()
    }

    pub fn write(&self, data: &[u8]) -> Result<()> {
        let mut writer = self.stdin_writer.lock();
        writer.write_all(data)?;
        writer.flush()?;
        self.input_capture.lock().push(data);
        Ok(())
    }

    pub fn subscribe_output(&self, replay_snapshot: bool) -> flume::Receiver<Vec<u8>> {
        let (tx, rx) = flume::unbounded();
        if replay_snapshot {
            let mut snapshot = self.snapshot();
            // An alt-screen TUI (e.g. Claude Code) keeps its UI in the alternate
            // buffer, which has no scrollback and so never reaches the raw
            // history above. A re-attaching viewer that only replays the raw
            // history therefore renders blank until the TUI happens to repaint
            // (which is why re-entering the terminal "fixed" it). Append the
            // live screen keyframe -- it carries the active DEC modes
            // (alt-screen, mouse) and its ESC[2J clears only the visible screen
            // (alacritty keeps scrollback) -- so the current screen and its
            // modes are reconstructed immediately. Normal-screen sessions
            // reconstruct fully from the raw history and skip this.
            let screen = self.screen_snapshot();
            if screen.input_mode.alternate_screen && !screen.data.is_empty() {
                snapshot.push_str(&screen.data);
            }
            if !snapshot.is_empty() {
                let _ = tx.send(snapshot.into_bytes());
            }
        }
        self.output_subscribers.lock().push(tx);
        rx
    }

    pub fn subscribe_events(&self, emit: EventSink) {
        self.event_subscribers
            .lock()
            .push(EventSubscriber::anonymous(emit));
    }

    pub fn subscribe_events_keyed(&self, key: impl Into<String>, emit: EventSink) {
        insert_keyed_event_subscriber(&self.event_subscribers, key.into(), emit);
    }

    pub fn kill(&self) -> Result<()> {
        self.pty_control.kill().map_err(anyhow::Error::msg)
    }

    pub fn has_exited(&self) -> bool {
        self.exit_signal.has_exited()
    }

    pub fn wait_for_exit(&self, timeout: Duration) -> bool {
        self.exit_signal.wait(timeout).is_some()
    }

    pub fn snapshot(&self) -> String {
        self.history.lock().to_text()
    }

    pub fn snapshot_tail(&self, max_chars: usize) -> (String, usize) {
        self.history.lock().tail_text(max_chars)
    }

    pub fn screen_snapshot(&self) -> TerminalScreenSnapshot {
        // Queue the snapshot under the lock, but wait for the worker reply
        // outside of it: holding the screen mutex across the round-trip
        // convoys the PTY reader and every remote handler behind one
        // slow snapshot.
        let request = self.screen.lock().snapshot_request(true);
        request.snapshot()
    }

    /// Scroll the host-side screen viewport (serves remote scrollback
    /// views: the host screen owns the authoritative scrollback at the
    /// current grid size). Returns a snapshot of the scrolled viewport
    /// stacked with one screen of overscan context above it, so the client
    /// can pre-render content revealed by inertial scrolling.
    pub fn scroll_screen_lines(&self, lines: i32) -> TerminalScreenSnapshot {
        {
            let mut screen = self.screen.lock();
            screen.scroll_lines(lines);
        }
        self.scrolled_view_snapshot()
    }

    pub fn scroll_screen_to_bottom(&self) -> TerminalScreenSnapshot {
        {
            let mut screen = self.screen.lock();
            screen.scroll_to_bottom();
        }
        self.screen_snapshot()
    }

    pub fn remote_viewport_snapshot(
        &self,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> TerminalScreenSnapshot {
        let request = self.screen.lock().remote_viewport_snapshot_request(
            display_offset,
            overscan_rows,
            max_lines,
        );
        request.snapshot()
    }

    fn scrolled_view_snapshot(&self) -> TerminalScreenSnapshot {
        let viewport = self.screen_snapshot();
        let above_offset = viewport.display_offset + viewport.rows;
        // Queue both overscan requests before waiting on either, so the
        // worker round-trips overlap instead of running serially.
        let (above_request, below_request) = {
            let screen = self.screen.lock();
            let above = screen.snapshot_at_offset_request(above_offset);
            let below = (viewport.display_offset > 0).then(|| {
                screen.snapshot_at_offset_request(
                    viewport.display_offset.saturating_sub(viewport.rows),
                )
            });
            (above, below)
        };
        let above = above_request.snapshot();
        let below = below_request.map(|request| request.snapshot());
        codux_terminal_core::stack_scrolled_snapshots(&above, &viewport, below.as_ref())
    }

    pub fn buffer_characters(&self) -> usize {
        self.history.lock().len_chars()
    }

    pub fn set_screen_scrollback(&self, lines: usize) {
        let mut current = self.remote_screen_current_scrollback.lock();
        if *current == lines {
            return;
        }
        *current = lines;
        self.screen.lock().set_scrollback(lines);
    }

    pub fn restore_remote_screen_scrollback(&self) {
        self.set_screen_scrollback(self.remote_screen_scrollback);
    }

    pub fn shrink_remote_screen_scrollback(&self) {
        self.set_screen_scrollback(initial_remote_screen_scrollback_lines(
            self.remote_screen_scrollback,
        ));
    }

    pub fn clear_history(&self) {
        self.history.lock().clear();
        self.screen.lock().clear();
        let mut info = self.info.lock();
        info.buffer_characters = 0;
        info.has_buffer = false;
        info.last_active_at = rfc3339_now();
    }

    pub fn info(&self) -> TerminalSessionSnapshot {
        let mut info = self.info.lock().clone();
        info.buffer_characters = self.buffer_characters();
        info.has_buffer = info.buffer_characters > 0;
        info
    }

    pub fn matches_config(
        &self,
        config: &TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
    ) -> bool {
        self.matches_requested_identity(&RequestedTerminalIdentity::from_config(config, context))
    }

    pub(super) fn matches_requested_identity(&self, requested: &RequestedTerminalIdentity) -> bool {
        let info = self.info();
        if let Some(cwd) = requested.cwd.as_deref()
            && normalize_terminal_path(&info.cwd) != cwd
        {
            return false;
        }
        if let Some(project_id) = requested.project_id.as_deref()
            && info.project_id != project_id
        {
            return false;
        }
        if let Some(session_key) = requested.session_key.as_deref()
            && info.session_key.as_deref() != Some(session_key)
        {
            return false;
        }
        true
    }

    pub fn ai_runtime_binding(&self) -> AIRuntimeTerminalBinding {
        self.ai_runtime_binding.clone()
    }

    pub fn input_snapshot(&self) -> TerminalInputSnapshot {
        self.input_capture.lock().snapshot()
    }

    pub fn output_snapshot(&self) -> TerminalOutputSnapshot {
        self.output_capture.lock().snapshot()
    }
}

impl TerminalExitSignal {
    fn new() -> Self {
        Self {
            exit_code: StdMutex::new(None),
            ready: Condvar::new(),
        }
    }

    fn mark_exited(&self, exit_code: Option<i32>) {
        let mut guard = self
            .exit_code
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard = Some(exit_code);
        self.ready.notify_all();
    }

    fn has_exited(&self) -> bool {
        self.exit_code
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .is_some()
    }

    fn wait(&self, timeout: Duration) -> Option<Option<i32>> {
        let guard = self
            .exit_code
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if guard.is_some() {
            return *guard;
        }
        let (guard, _) = self
            .ready
            .wait_timeout_while(guard, timeout, |exit_code| exit_code.is_none())
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        *guard
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ViewportClaimIntent {
    Auto,
    Force,
    Passive,
}

impl TerminalPtySessionHandle {
    pub fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        self.resize_viewport(terminal_viewport_local_owner(), cols, rows)
            .map(|_| ())
    }

    /// Explicit claim from a manual takeover or active desktop input.
    pub fn claim_viewport(&self, owner: &str) -> Result<TerminalViewportState> {
        self.claim_viewport_with(owner, ViewportClaimIntent::Force)
    }

    /// Automatic claim from a newly-visible controller. It may take the
    /// untouched default desktop owner, but never overrides an explicit claim.
    pub fn claim_viewport_auto(&self, owner: &str) -> Result<TerminalViewportState> {
        self.claim_viewport_with(owner, ViewportClaimIntent::Auto)
    }

    /// Passive claim: ambient paths such as the desktop prepaint. Does NOT
    /// steal an unexpired lease held by a different owner — otherwise a
    /// painting desktop pane revokes a mobile claim within one frame.
    pub fn claim_viewport_passive(&self, owner: &str) -> Result<TerminalViewportState> {
        self.claim_viewport_with(owner, ViewportClaimIntent::Passive)
    }

    fn claim_viewport_with(
        &self,
        owner: &str,
        intent: ViewportClaimIntent,
    ) -> Result<TerminalViewportState> {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        let now = Instant::now();
        let owner_matches = viewport.state.owner == owner;
        let can_auto_claim =
            viewport.state.owner == terminal_viewport_local_owner() && !viewport.explicit_owner;
        let can_claim = match intent {
            ViewportClaimIntent::Force => true,
            ViewportClaimIntent::Auto => owner_matches || can_auto_claim,
            ViewportClaimIntent::Passive => owner_matches,
        };
        if !can_claim {
            return Ok(viewport.state.clone());
        }
        let state = &mut viewport.state;
        let owner_changed = state.owner != owner;
        if state.owner != owner {
            state.owner = owner;
            state.generation = state.generation.saturating_add(1);
        }
        if intent == ViewportClaimIntent::Force {
            viewport.explicit_owner = true;
        } else if owner_changed {
            viewport.explicit_owner = false;
        }
        viewport.expires_at = now + TERMINAL_VIEWPORT_LEASE_TTL;
        let state = viewport.state.clone();
        drop(viewport);
        if owner_changed {
            // A scrolled host viewport belongs to the previous owner.
            self.screen.lock().scroll_to_bottom();
            self.emit_viewport_state(&state);
        }
        Ok(state)
    }

    /// Renew the lease when traffic from the current owner proves it is
    /// still actively viewing (input, output acks). No-op for non-owners.
    pub fn touch_viewport_lease(&self, owner: &str) {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        if viewport.state.owner == owner {
            viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        }
    }

    pub fn release_expired_viewport_lease(&self) -> Option<TerminalViewportState> {
        self.reclaim_expired_viewport_lease(|_expired| None)
    }

    /// Reclaim an expired remote lease. `resolve_next` is consulted only when the
    /// lease has actually expired and is remote-owned; it receives the expired
    /// owner and may return ANOTHER owner to hand off to (e.g. a second phone
    /// still watching this terminal). A `None` -- or an owner equal to the
    /// expired one or to the local host -- reverts the lease to the host desktop,
    /// preserving the original behavior.
    pub fn reclaim_expired_viewport_lease(
        &self,
        resolve_next: impl FnOnce(&str) -> Option<String>,
    ) -> Option<TerminalViewportState> {
        let mut viewport = self.viewport.lock();
        if viewport.state.owner == terminal_viewport_local_owner()
            || viewport.expires_at > Instant::now()
        {
            return None;
        }
        let expired_owner = viewport.state.owner.clone();
        let next_owner = resolve_next(&expired_owner)
            .map(|owner| terminal_viewport_owner(&owner))
            .filter(|owner| {
                owner.as_str() != expired_owner.as_str()
                    && owner.as_str() != terminal_viewport_local_owner()
            })
            .unwrap_or_else(|| terminal_viewport_local_owner().to_string());
        viewport.state.owner = next_owner;
        viewport.state.generation = viewport.state.generation.saturating_add(1);
        viewport.explicit_owner = false;
        viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
        Some(state)
    }

    pub fn release_viewport(&self, owner: &str) -> Result<Option<TerminalViewportState>> {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        if viewport.state.owner != owner {
            return Ok(None);
        }
        viewport.state.owner = terminal_viewport_local_owner().to_string();
        viewport.state.generation = viewport.state.generation.saturating_add(1);
        viewport.explicit_owner = false;
        viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
        Ok(Some(state))
    }

    pub fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>> {
        let owner = terminal_viewport_owner(owner);
        let cols = cols.max(20);
        let rows = rows.max(8);
        // The current owner drives the FULL grid. Ownership is a HANDOFF token:
        // whoever is actively using the session (the desktop, or a remote device
        // it was handed off to) holds it, and the PTY reflows to THAT device's
        // size so the session fits whoever is driving it -- terminal handoff, not
        // simultaneous mirroring. A non-owner cannot resize (rejected just
        // below); it pauses and shows the owner's last frame instead of trying to
        // mirror a mismatched grid.
        let mut viewport = self.viewport.lock();
        if viewport.state.owner != owner {
            return Ok(None);
        }
        viewport.expires_at = Instant::now() + TERMINAL_VIEWPORT_LEASE_TTL;
        if viewport.state.cols == cols && viewport.state.rows == rows {
            return Ok(Some(viewport.state.clone()));
        }
        self.pty_control
            .resize(cols, rows)
            .map_err(anyhow::Error::msg)?;
        {
            let mut info = self.info.lock();
            info.cols = cols;
            info.rows = rows;
            info.last_active_at = rfc3339_now();
        }
        self.screen.lock().resize(cols as usize, rows as usize);
        viewport.state.cols = cols;
        viewport.state.rows = rows;
        viewport.state.generation = viewport.state.generation.saturating_add(1);
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
        Ok(Some(state))
    }

    /// Set the friendly name of the current owner (desktop "handed off" UI). No
    /// effect unless `owner` is still the active owner. Emits on change.
    pub fn set_viewport_owner_label(&self, owner: &str, label: Option<String>) {
        let owner = terminal_viewport_owner(owner);
        let mut viewport = self.viewport.lock();
        if viewport.state.owner != owner || viewport.state.owner_label == label {
            return;
        }
        viewport.state.owner_label = label;
        let state = viewport.state.clone();
        drop(viewport);
        self.emit_viewport_state(&state);
    }

    pub fn viewport_state(&self) -> TerminalViewportState {
        self.viewport.lock().state.clone()
    }

    fn emit_viewport_state(&self, state: &TerminalViewportState) {
        let session_id = self.info.lock().id.clone();
        emit_terminal_event(
            &self.event_subscribers,
            TerminalEvent::Viewport {
                session_id,
                owner: state.owner.clone(),
                cols: state.cols,
                rows: state.rows,
                generation: state.generation,
            },
        );
    }
}

pub(super) fn spawn_waiter(
    id: String,
    control: LocalPtyProcessHandle,
    info: Arc<parking_lot::Mutex<TerminalSessionSnapshot>>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
    exit_signal: Arc<TerminalExitSignal>,
) {
    std::thread::Builder::new()
        .name(format!("codux-terminal-waiter-{id}"))
        .spawn(move || {
            let exit_code = control.wait_exit_code();
            exit_signal.mark_exited(exit_code);
            {
                let mut info = info.lock();
                info.status = "exited".to_string();
                info.is_running = false;
                info.last_active_at = rfc3339_now();
            }
            emit_terminal_event(
                &event_subscribers,
                TerminalEvent::Exit {
                    session_id: id.clone(),
                    exit_code,
                },
            );
        })
        .expect("failed to spawn terminal waiter");
}

pub(super) fn spawn_headless_reader(
    id: String,
    mut reader: Box<dyn Read + Send>,
    event_subscribers: Arc<parking_lot::Mutex<Vec<EventSubscriber>>>,
) {
    std::thread::Builder::new()
        .name(format!("codux-terminal-reader-{id}"))
        .spawn(move || {
            let mut buffer = vec![0_u8; 16 * 1024];
            loop {
                match reader.read(&mut buffer) {
                    Ok(0) => return,
                    Ok(_) => {}
                    Err(error) => {
                        emit_terminal_event(
                            &event_subscribers,
                            TerminalEvent::Error {
                                session_id: id,
                                message: error.to_string(),
                            },
                        );
                        return;
                    }
                }
            }
        })
        .expect("failed to spawn terminal reader");
}
