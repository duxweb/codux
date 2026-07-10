use super::*;

pub struct TerminalManager {
    sessions: Arc<parking_lot::Mutex<HashMap<String, Arc<TerminalPtySession>>>>,
    ai_runtime: Option<Arc<AIRuntimeBridge>>,
    viewport_lease_watcher_started: std::sync::Once,
    viewport_owner_resolver: Arc<parking_lot::Mutex<Option<ViewportOwnerResolver>>>,
}

impl TerminalManager {
    pub fn new() -> Self {
        Self {
            sessions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            ai_runtime: None,
            viewport_lease_watcher_started: std::sync::Once::new(),
            viewport_owner_resolver: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    pub fn with_ai_runtime(ai_runtime: Arc<AIRuntimeBridge>) -> Self {
        Self {
            sessions: Arc::new(parking_lot::Mutex::new(HashMap::new())),
            ai_runtime: Some(ai_runtime),
            viewport_lease_watcher_started: std::sync::Once::new(),
            viewport_owner_resolver: Arc::new(parking_lot::Mutex::new(None)),
        }
    }

    /// Register a policy that picks the next viewport owner when a remote lease
    /// expires, so a still-active second viewer inherits it instead of the lease
    /// snapping back to the host desktop. Without a resolver, expiry reverts to
    /// the host (the previous behavior).
    pub fn set_viewport_owner_resolver(&self, resolver: ViewportOwnerResolver) {
        *self.viewport_owner_resolver.lock() = Some(resolver);
    }

    pub fn list(&self) -> Vec<TerminalSessionSnapshot> {
        self.sessions
            .lock()
            .values()
            .map(|session| session.info())
            .collect()
    }

    pub fn create<F>(&self, config: TerminalPtyConfig, emit: F) -> Result<String>
    where
        F: Fn(TerminalEvent) + Send + Sync + 'static,
    {
        self.create_with_sink(
            config,
            Arc::new(move |event| {
                emit(event);
                true
            }),
        )
    }

    pub fn create_with_sink(&self, config: TerminalPtyConfig, emit: EventSink) -> Result<String> {
        let (session, _) = self.attach_or_create_with_context(config, None, emit)?;
        let id = session.id().to_string();
        Ok(id)
    }

    pub fn create_with_event_key(
        &self,
        config: TerminalPtyConfig,
        event_key: impl Into<String>,
        emit: EventSink,
    ) -> Result<String> {
        let (session, _) =
            self.attach_or_create_with_context_and_event_key(config, None, event_key, emit)?;
        let id = session.id().to_string();
        Ok(id)
    }

    pub fn ensure_session_with_context(
        &self,
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
    ) -> Result<String> {
        let requested_id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let requested_identity = RequestedTerminalIdentity::from_config(&config, context);
        if let Some(session) = requested_id
            .as_deref()
            .and_then(|id| self.sessions.lock().get(id).cloned())
        {
            if session.matches_requested_identity(&requested_identity) {
                return Ok(session.id().to_string());
            }
            self.remove_incompatible_session(&session, &requested_identity);
        }

        if let Some(ai_runtime) = &self.ai_runtime {
            ai_runtime.ensure_started().map_err(anyhow::Error::msg)?;
        }
        let (session, _writer, reader) = TerminalPtySession::spawn(config, context, None)?;
        let session = Arc::new(session);
        let id = session.id().to_string();
        self.register_ai_runtime_terminal(&session);
        let event_subscribers = session.event_subscribers.clone();
        self.sessions.lock().insert(id.clone(), session);
        self.ensure_viewport_lease_watcher();
        spawn_headless_reader(id.clone(), reader, event_subscribers);
        Ok(id)
    }

    pub fn attach_or_create_with_context(
        &self,
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
        emit: EventSink,
    ) -> Result<(Arc<TerminalPtySession>, flume::Receiver<Vec<u8>>)> {
        self.attach_or_create_with_context_internal(config, context, None, emit)
    }

    pub fn attach_or_create_with_context_and_event_key(
        &self,
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
        event_key: impl Into<String>,
        emit: EventSink,
    ) -> Result<(Arc<TerminalPtySession>, flume::Receiver<Vec<u8>>)> {
        self.attach_or_create_with_context_internal(config, context, Some(event_key.into()), emit)
    }

    fn attach_or_create_with_context_internal(
        &self,
        config: TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
        event_key: Option<String>,
        emit: EventSink,
    ) -> Result<(Arc<TerminalPtySession>, flume::Receiver<Vec<u8>>)> {
        let requested_id = config
            .terminal_id
            .clone()
            .filter(|value| !value.trim().is_empty());
        let event_key = event_key
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty());
        let requested_identity = RequestedTerminalIdentity::from_config(&config, context);
        if let Some(session) = requested_id
            .as_deref()
            .and_then(|id| self.sessions.lock().get(id).cloned())
        {
            if session.matches_requested_identity(&requested_identity) {
                if let Some(event_key) = event_key {
                    session.subscribe_events_keyed(event_key, emit);
                } else {
                    session.subscribe_events(emit);
                }
                let rx = session.subscribe_output(true);
                return Ok((session, rx));
            }
            self.remove_incompatible_session(&session, &requested_identity);
        }

        if let Some(ai_runtime) = &self.ai_runtime {
            ai_runtime.ensure_started().map_err(anyhow::Error::msg)?;
        }
        let (session, _writer, reader) =
            TerminalPtySession::spawn(config, context, Some((event_key, emit.clone())))?;
        let session = Arc::new(session);
        let id = session.id().to_string();
        self.register_ai_runtime_terminal(&session);
        let rx = session.subscribe_output(true);
        let event_subscribers = session.event_subscribers.clone();
        self.sessions.lock().insert(id.clone(), session.clone());
        self.ensure_viewport_lease_watcher();
        spawn_headless_reader(id, reader, event_subscribers);
        Ok((session, rx))
    }

    pub fn write(&self, session_id: &str, data: &[u8]) -> Result<()> {
        self.session(session_id)?.write(data)
    }

    pub fn resize(&self, session_id: &str, cols: u16, rows: u16) -> Result<()> {
        self.session(session_id)?.resize(cols, rows)
    }

    pub fn claim_viewport(&self, session_id: &str, owner: &str) -> Result<TerminalViewportState> {
        self.session(session_id)?.claim_viewport(owner)
    }

    pub fn claim_viewport_auto(
        &self,
        session_id: &str,
        owner: &str,
    ) -> Result<TerminalViewportState> {
        self.session(session_id)?.claim_viewport_auto(owner)
    }

    pub fn touch_viewport_lease(&self, session_id: &str, owner: &str) {
        if let Ok(session) = self.session(session_id) {
            session.clone_handle().touch_viewport_lease(owner);
        }
    }

    pub fn scroll_screen_lines(
        &self,
        session_id: &str,
        lines: i32,
    ) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.scroll_screen_lines(lines))
    }

    pub fn scroll_screen_to_bottom(&self, session_id: &str) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.scroll_screen_to_bottom())
    }

    pub fn remote_viewport_snapshot(
        &self,
        session_id: &str,
        display_offset: usize,
        overscan_rows: usize,
        max_lines: usize,
    ) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.remote_viewport_snapshot(
            display_offset,
            overscan_rows,
            max_lines,
        ))
    }

    pub fn set_screen_scrollback(&self, session_id: &str, lines: usize) {
        if let Ok(session) = self.session(session_id) {
            session.set_screen_scrollback(lines);
        }
    }

    pub fn restore_remote_screen_scrollback(&self, session_id: &str) {
        if let Ok(session) = self.session(session_id) {
            session.restore_remote_screen_scrollback();
        }
    }

    pub fn shrink_remote_screen_scrollback(&self, session_id: &str) {
        if let Ok(session) = self.session(session_id) {
            session.shrink_remote_screen_scrollback();
        }
    }

    pub fn release_viewport(
        &self,
        session_id: &str,
        owner: &str,
    ) -> Result<Option<TerminalViewportState>> {
        self.session(session_id)?.release_viewport(owner)
    }

    pub fn resize_viewport(
        &self,
        session_id: &str,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> Result<Option<TerminalViewportState>> {
        self.session(session_id)?.resize_viewport(owner, cols, rows)
    }

    pub fn set_viewport_owner_label(&self, session_id: &str, owner: &str, label: Option<String>) {
        if let Ok(session) = self.session(session_id) {
            session.set_viewport_owner_label(owner, label);
        }
    }

    pub fn viewport_state(&self, session_id: &str) -> Result<TerminalViewportState> {
        Ok(self.session(session_id)?.viewport_state())
    }

    #[cfg(any(test, feature = "test-support"))]
    pub fn expire_viewport_lease_for_test(
        &self,
        session_id: &str,
    ) -> Result<Option<TerminalViewportState>> {
        let session = self
            .sessions
            .lock()
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("terminal session not found: {session_id}"))?;
        {
            let mut viewport = session.viewport.lock();
            viewport.expires_at = Instant::now() - Duration::from_secs(1);
        }
        Ok(session.clone_handle().release_expired_viewport_lease())
    }

    fn ensure_viewport_lease_watcher(&self) {
        let sessions = Arc::downgrade(&self.sessions);
        let resolver_slot = Arc::clone(&self.viewport_owner_resolver);
        self.viewport_lease_watcher_started.call_once(move || {
            std::thread::Builder::new()
                .name("codux-terminal-viewport-lease".to_string())
                .spawn(move || {
                    loop {
                        std::thread::sleep(Duration::from_secs(1));
                        let Some(sessions) = sessions.upgrade() else {
                            break;
                        };
                        let resolver = resolver_slot.lock().clone();
                        let entries = sessions
                            .lock()
                            .iter()
                            .map(|(id, session)| (id.clone(), session.clone()))
                            .collect::<Vec<_>>();
                        for (session_id, session) in entries {
                            // On expiry, hand off to another active viewer the
                            // host's resolver names; fall back to the host.
                            session
                                .clone_handle()
                                .reclaim_expired_viewport_lease(|expired| {
                                    resolver
                                        .as_ref()
                                        .and_then(|resolve| resolve(&session_id, expired))
                                });
                        }
                    }
                })
                .expect("spawn terminal viewport lease watcher");
        });
    }

    pub fn subscribe_events(&self, session_id: &str, emit: EventSink) -> Result<()> {
        self.session(session_id)?.subscribe_events(emit);
        Ok(())
    }

    pub fn subscribe_events_keyed(
        &self,
        session_id: &str,
        key: impl Into<String>,
        emit: EventSink,
    ) -> Result<()> {
        self.session(session_id)?.subscribe_events_keyed(key, emit);
        Ok(())
    }

    pub fn kill(&self, session_id: &str) -> Result<()> {
        let Some(session) = self.sessions.lock().remove(session_id) else {
            return Err(anyhow!("terminal session not found: {session_id}"));
        };
        self.remove_ai_runtime_terminal(&session);
        session.kill()
    }

    pub fn kill_and_wait(&self, session_id: &str, timeout: Duration) -> Result<()> {
        if !self.kill_and_wait_if_present(session_id, timeout)? {
            return Err(anyhow!("terminal session not found: {session_id}"));
        }
        Ok(())
    }

    pub fn kill_and_wait_if_present(&self, session_id: &str, timeout: Duration) -> Result<bool> {
        let Some(session) = self.sessions.lock().get(session_id).cloned() else {
            return Ok(false);
        };
        let kill_error = if session.has_exited() {
            None
        } else {
            session.kill().err()
        };
        if session.wait_for_exit(timeout) {
            self.sessions.lock().remove(session_id);
            self.remove_ai_runtime_terminal(&session);
            return Ok(true);
        }
        if let Some(error) = kill_error {
            return Err(error.context(format!(
                "terminal session did not exit after kill request: {session_id}"
            )));
        }
        Err(anyhow!(
            "terminal session did not exit within {} ms: {session_id}",
            timeout.as_millis()
        ))
    }

    pub fn snapshot(&self, session_id: &str) -> Result<String> {
        Ok(self.session(session_id)?.snapshot())
    }

    pub fn snapshot_tail(&self, session_id: &str, max_chars: usize) -> Result<(String, usize)> {
        Ok(self.session(session_id)?.snapshot_tail(max_chars))
    }

    pub fn screen_snapshot(&self, session_id: &str) -> Result<TerminalScreenSnapshot> {
        Ok(self.session(session_id)?.screen_snapshot())
    }

    pub fn input_snapshot(&self, session_id: &str) -> Result<TerminalInputSnapshot> {
        Ok(self.session(session_id)?.input_snapshot())
    }

    pub fn output_snapshot(&self, session_id: &str) -> Result<TerminalOutputSnapshot> {
        Ok(self.session(session_id)?.output_snapshot())
    }

    pub fn buffer_characters(&self, session_id: &str) -> Result<usize> {
        Ok(self.session(session_id)?.buffer_characters())
    }

    pub fn clear_history(&self, session_id: &str) -> Result<()> {
        self.session(session_id)?.clear_history();
        Ok(())
    }

    pub(super) fn session(&self, session_id: &str) -> Result<Arc<TerminalPtySession>> {
        self.sessions
            .lock()
            .get(session_id)
            .cloned()
            .ok_or_else(|| anyhow!("terminal session not found: {session_id}"))
    }

    fn remove_incompatible_session(
        &self,
        session: &Arc<TerminalPtySession>,
        requested: &RequestedTerminalIdentity,
    ) {
        let existing = session.info();
        crate::ai_runtime::runtime_log_line(
            "terminal-pty",
            &format!(
                "replace incompatible session id={} existing_project={} existing_cwd={} requested_project={} requested_cwd={} requested_session_key={}",
                existing.id,
                existing.project_id,
                existing.cwd,
                requested.project_id.as_deref().unwrap_or(""),
                requested.cwd.as_deref().unwrap_or(""),
                requested.session_key.as_deref().unwrap_or("")
            ),
        );
        let removed = {
            let mut sessions = self.sessions.lock();
            if sessions
                .get(&existing.id)
                .map(|current| Arc::ptr_eq(current, session))
                .unwrap_or(false)
            {
                sessions.remove(&existing.id)
            } else {
                None
            }
        };
        if let Some(removed) = removed {
            self.remove_ai_runtime_terminal(&removed);
            let _ = removed.kill();
        }
    }

    fn register_ai_runtime_terminal(&self, session: &TerminalPtySession) {
        let Some(ai_runtime) = &self.ai_runtime else {
            return;
        };
        ai_runtime.registry().upsert(session.ai_runtime_binding());
        // Hand the runtime a weak ref to the rendered screen so it can scrape the
        // universal "waiting for approval" prompt (works for every CLI, even the
        // ones that never persist that state to a file).
        ai_runtime
            .registry()
            .register_screen(session.id(), Arc::downgrade(&session.screen));
        // Shell PID → walk the process tree to identify the terminal's AI tool (hook-free).
        if let Some(shell_pid) = session.pty_control.process_id() {
            ai_runtime
                .registry()
                .register_shell_pid(session.id(), shell_pid);
        }
        attach_ai_runtime_terminal_output_watcher(session, Arc::clone(ai_runtime));
    }

    fn remove_ai_runtime_terminal(&self, session: &TerminalPtySession) {
        let Some(ai_runtime) = &self.ai_runtime else {
            return;
        };
        ai_runtime.registry().remove(session.id());
        ai_runtime.remove_session(session.id());
    }
}

impl Default for TerminalManager {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
pub struct DesktopTerminalSessionHandle(pub(super) Arc<TerminalPtySession>);

impl CoreTerminalSessionHandle for DesktopTerminalSessionHandle {
    fn id(&self) -> &str {
        self.0.id()
    }

    fn info(&self) -> TerminalSessionSnapshot {
        self.0.info()
    }

    fn write(&self, data: &[u8]) -> std::result::Result<(), String> {
        self.0.write(data).map_err(|error| error.to_string())
    }

    fn resize(&self, cols: u16, rows: u16) -> std::result::Result<(), String> {
        self.0.resize(cols, rows).map_err(|error| error.to_string())
    }

    fn claim_viewport(&self, owner: &str) -> std::result::Result<TerminalViewportState, String> {
        self.0
            .claim_viewport(owner)
            .map_err(|error| error.to_string())
    }

    fn release_viewport(
        &self,
        owner: &str,
    ) -> std::result::Result<Option<TerminalViewportState>, String> {
        self.0
            .release_viewport(owner)
            .map_err(|error| error.to_string())
    }

    fn resize_viewport(
        &self,
        owner: &str,
        cols: u16,
        rows: u16,
    ) -> std::result::Result<Option<TerminalViewportState>, String> {
        self.0
            .resize_viewport(owner, cols, rows)
            .map_err(|error| error.to_string())
    }

    fn viewport_state(&self) -> TerminalViewportState {
        self.0.viewport_state()
    }

    fn snapshot(&self) -> String {
        self.0.snapshot()
    }

    fn snapshot_tail(&self, max_chars: usize) -> (String, usize) {
        self.0.snapshot_tail(max_chars)
    }

    fn buffer_characters(&self) -> usize {
        self.0.buffer_characters()
    }

    fn clear_history(&self) {
        self.0.clear_history();
    }

    fn kill(&self) -> std::result::Result<(), String> {
        self.0.kill().map_err(|error| error.to_string())
    }
}

impl CoreTerminalDriver for TerminalManager {
    type Session = DesktopTerminalSessionHandle;

    fn list(&self) -> Vec<TerminalSessionSnapshot> {
        TerminalManager::list(self)
    }

    fn create(
        &self,
        config: TerminalLaunchConfig,
        emit: TerminalEventSink,
    ) -> std::result::Result<Self::Session, String> {
        let config = TerminalPtyConfig::from(config);
        let (session, _) = self
            .attach_or_create_with_context(config, None, Arc::from(emit))
            .map_err(|error| error.to_string())?;
        Ok(DesktopTerminalSessionHandle(session))
    }

    fn session(&self, session_id: &str) -> std::result::Result<Self::Session, String> {
        self.session(session_id)
            .map(DesktopTerminalSessionHandle)
            .map_err(|error| error.to_string())
    }

    fn remove(&self, session_id: &str) -> std::result::Result<(), String> {
        self.kill(session_id).map_err(|error| error.to_string())
    }

    fn subscribe_events(
        &self,
        session_id: &str,
        emit: TerminalEventSink,
    ) -> std::result::Result<(), String> {
        self.subscribe_events(session_id, Arc::from(emit))
            .map_err(|error| error.to_string())
    }
}
