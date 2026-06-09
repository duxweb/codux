#[derive(Clone)]
pub struct TerminalPane {
    pub view: Entity<TerminalView>,
    session: TerminalSessionBinding,
}

impl TerminalPane {
    pub fn spawn_with_pty_config<C>(
        cx: &mut C,
        terminal_manager: Arc<TerminalManager>,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
    ) -> Result<Self>
    where
        C: AppContext,
    {
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let (session_event_tx, session_event_rx) = mpsc::channel();
        let emit = Arc::new(move |event| match event {
            TerminalEvent::Exit { .. } => session_event_tx.send(TerminalUiEvent::Exit).is_ok(),
            TerminalEvent::Error { message, .. } => session_event_tx
                .send(TerminalUiEvent::Error(message))
                .is_ok(),
            TerminalEvent::Output { .. } => session_event_tx.send(TerminalUiEvent::Wakeup).is_ok(),
            TerminalEvent::Viewport { cols, rows, .. } => session_event_tx
                .send(TerminalUiEvent::Viewport { cols, rows })
                .is_ok(),
        });
        let terminal_id = config.terminal_id.clone();
        let attach_started_at = Instant::now();
        let (session, output_rx) =
            terminal_manager.attach_or_create_with_context(config, None, emit)?;
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "pty_attach elapsed_ms={} terminal_id={}",
                attach_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );
        let session = TerminalSessionBinding::attached(session);
        let writer = TerminalSessionWriter::new(session.clone());
        let view_started_at = Instant::now();
        let view = cx.new(|cx| {
            TerminalView::new(
                writer,
                output_rx,
                session_event_rx,
                session.clone(),
                terminal_config,
                cx,
            )
        });
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "view_create elapsed_ms={} terminal_id={}",
                view_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );

        Ok(Self { view, session })
    }

    pub fn pending_with_pty_config<C>(
        cx: &mut C,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
    ) -> (Self, PendingTerminalAttach)
    where
        C: AppContext,
    {
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let terminal_id = config.terminal_id.clone();
        let (session_event_tx, session_event_rx) = mpsc::channel();
        let (output_tx, output_rx) = flume::unbounded();
        let (session, initial_layout_rx) = TerminalSessionBinding::pending();
        let writer = TerminalSessionWriter::new(session.clone());
        let view_started_at = Instant::now();
        let view = cx.new(|cx| {
            TerminalView::new(
                writer,
                output_rx,
                session_event_rx,
                session.clone(),
                terminal_config,
                cx,
            )
        });
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "view_create elapsed_ms={} terminal_id={}",
                view_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );
        (
            Self {
                view,
                session: session.clone(),
            },
            PendingTerminalAttach {
                session,
                output_tx,
                session_event_tx,
                terminal_id,
                initial_layout_rx,
            },
        )
    }

    pub fn attach_pending_session(
        terminal_manager: Arc<TerminalManager>,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
        pending: PendingTerminalAttach,
    ) -> Result<String> {
        let initial_layout = pending.wait_for_initial_layout();
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let terminal_id = config.terminal_id.clone();
        let session_event_tx = pending.session_event_tx.clone();
        let emit = Arc::new(move |event| match event {
            TerminalEvent::Exit { .. } => session_event_tx.send(TerminalUiEvent::Exit).is_ok(),
            TerminalEvent::Error { message, .. } => session_event_tx
                .send(TerminalUiEvent::Error(message))
                .is_ok(),
            TerminalEvent::Output { .. } => session_event_tx.send(TerminalUiEvent::Wakeup).is_ok(),
            TerminalEvent::Viewport { cols, rows, .. } => session_event_tx
                .send(TerminalUiEvent::Viewport { cols, rows })
                .is_ok(),
        });
        let attach_started_at = Instant::now();
        let (session, output_rx) =
            terminal_manager.attach_or_create_with_context(config, None, emit)?;
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "pty_attach elapsed_ms={} terminal_id={}",
                attach_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none")
            ),
        );
        let attached_id = session.id().to_string();
        pending.session.attach(session)?;
        match initial_layout {
            Some((cols, rows)) => codux_runtime::runtime_trace::runtime_trace(
                "terminal-restore",
                &format!(
                    "initial_layout_ready terminal_id={} cols={} rows={}",
                    terminal_id.as_deref().unwrap_or("none"),
                    cols,
                    rows
                ),
            ),
            None => codux_runtime::runtime_trace::runtime_trace(
                "terminal-restore",
                &format!(
                    "initial_layout_timeout terminal_id={}",
                    terminal_id.as_deref().unwrap_or("none")
                ),
            ),
        }
        let output_tx = pending.output_tx;
        codux_runtime::async_runtime::spawn(async move {
            while let Ok(bytes) = output_rx.recv_async().await {
                if output_tx.send_async(bytes).await.is_err() {
                    break;
                }
            }
        });
        Ok(attached_id)
    }

    pub fn send_text(&self, text: &str) -> Result<()> {
        self.session.write(text.as_bytes())
    }

    pub fn input_snapshot(&self) -> TerminalInputSnapshot {
        self.session.input_snapshot()
    }

    pub fn output_snapshot(&self) -> TerminalOutputSnapshot {
        self.session.output_snapshot()
    }
}

pub struct PendingTerminalAttach {
    session: TerminalSessionBinding,
    output_tx: flume::Sender<Vec<u8>>,
    session_event_tx: mpsc::Sender<TerminalUiEvent>,
    terminal_id: Option<String>,
    initial_layout_rx: mpsc::Receiver<(u16, u16)>,
}

impl PendingTerminalAttach {
    pub fn terminal_id(&self) -> Option<&str> {
        self.terminal_id.as_deref()
    }

    fn wait_for_initial_layout(&self) -> Option<(u16, u16)> {
        self.initial_layout_rx
            .recv_timeout(TERMINAL_INITIAL_LAYOUT_WAIT)
            .ok()
    }
}

pub fn terminal_pty_config_with_view(
    mut config: TerminalPtyConfig,
    terminal_config: &TerminalConfig,
) -> TerminalPtyConfig {
    config.cols = Some(terminal_config.cols as u16);
    config.rows = Some(terminal_config.rows as u16);
    config.scrollback_lines = Some(terminal_config.scrollback);
    config
}

#[derive(Clone)]
struct TerminalSessionWriter {
    session: TerminalSessionBinding,
}

impl TerminalSessionWriter {
    fn new(session: TerminalSessionBinding) -> Self {
        Self { session }
    }
}

impl Write for TerminalSessionWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.session.write(buf).map_err(std::io::Error::other)?;
        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

#[derive(Clone)]
struct TerminalSessionBinding {
    inner: Arc<Mutex<TerminalSessionBindingInner>>,
}

struct TerminalSessionBindingInner {
    session: Option<Arc<TerminalPtySession>>,
    pending_writes: VecDeque<Vec<u8>>,
    pending_write_bytes: usize,
    last_resize: Option<(u16, u16)>,
    initial_layout_tx: Option<mpsc::Sender<(u16, u16)>>,
}

impl TerminalSessionBinding {
    fn pending() -> (Self, mpsc::Receiver<(u16, u16)>) {
        let (initial_layout_tx, initial_layout_rx) = mpsc::channel();
        (
            Self {
                inner: Arc::new(Mutex::new(TerminalSessionBindingInner {
                    session: None,
                    pending_writes: VecDeque::new(),
                    pending_write_bytes: 0,
                    last_resize: None,
                    initial_layout_tx: Some(initial_layout_tx),
                })),
            },
            initial_layout_rx,
        )
    }

    fn attached(session: Arc<TerminalPtySession>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(TerminalSessionBindingInner {
                session: Some(session),
                pending_writes: VecDeque::new(),
                pending_write_bytes: 0,
                last_resize: None,
                initial_layout_tx: None,
            })),
        }
    }

    fn attach(&self, session: Arc<TerminalPtySession>) -> Result<()> {
        let (pending_writes, last_resize) = {
            let mut inner = self.inner.lock();
            inner.session = Some(session.clone());
            inner.pending_write_bytes = 0;
            (std::mem::take(&mut inner.pending_writes), inner.last_resize)
        };
        if let Some((cols, rows)) = last_resize {
            session
                .clone_handle()
                .resize_viewport(terminal_viewport_local_owner(), cols, rows)?;
        }
        for bytes in pending_writes {
            session.write(&bytes)?;
        }
        Ok(())
    }

    fn write(&self, bytes: &[u8]) -> Result<()> {
        if let Some(session) = self.inner.lock().session.clone() {
            return session.write(bytes);
        }
        const MAX_PENDING_WRITE_BYTES: usize = 64 * 1024;
        let mut inner = self.inner.lock();
        if inner.pending_write_bytes + bytes.len() > MAX_PENDING_WRITE_BYTES {
            return Ok(());
        }
        inner.pending_write_bytes += bytes.len();
        inner.pending_writes.push_back(bytes.to_vec());
        Ok(())
    }

    fn resize(&self, cols: u16, rows: u16) -> Result<()> {
        let (session, initial_layout_tx) = {
            let mut inner = self.inner.lock();
            inner.last_resize = Some((cols, rows));
            (inner.session.clone(), inner.initial_layout_tx.take())
        };
        if let Some(tx) = initial_layout_tx {
            let _ = tx.send((cols, rows));
        }
        if let Some(session) = session {
            session
                .clone_handle()
                .resize_viewport(terminal_viewport_local_owner(), cols, rows)?;
        }
        Ok(())
    }

    fn claim_local_viewport(&self) -> Result<()> {
        if let Some(session) = self.inner.lock().session.clone() {
            session
                .clone_handle()
                .claim_viewport(terminal_viewport_local_owner())?;
        }
        Ok(())
    }

    fn local_viewport_owns(&self) -> bool {
        self.inner
            .lock()
            .session
            .as_ref()
            .map(|session| session.viewport_state().owner == terminal_viewport_local_owner())
            .unwrap_or(true)
    }

    fn record_layout(&self, cols: u16, rows: u16) -> bool {
        let initial_layout_tx = {
            let mut inner = self.inner.lock();
            let changed = inner.last_resize != Some((cols, rows));
            inner.last_resize = Some((cols, rows));
            (inner.initial_layout_tx.take(), changed)
        };
        if let Some(tx) = initial_layout_tx.0 {
            let _ = tx.send((cols, rows));
        }
        initial_layout_tx.1
    }

    fn input_snapshot(&self) -> TerminalInputSnapshot {
        self.inner
            .lock()
            .session
            .as_ref()
            .map(|session| session.input_snapshot())
            .unwrap_or_default()
    }

    fn output_snapshot(&self) -> TerminalOutputSnapshot {
        self.inner
            .lock()
            .session
            .as_ref()
            .map(|session| session.output_snapshot())
            .unwrap_or_default()
    }
}
