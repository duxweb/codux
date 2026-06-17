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
            TerminalEvent::Viewport {
                owner, generation, ..
            } => session_event_tx
                .send(TerminalUiEvent::Viewport {
                    remote_owner: owner != terminal_viewport_local_owner(),
                    generation,
                })
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
                None,
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
        Self::pending_with_restored_output(cx, pty_config, terminal_config, None)
    }

    pub fn pending_with_restored_output<C>(
        cx: &mut C,
        pty_config: TerminalPtyConfig,
        terminal_config: TerminalConfig,
        restored_output: Option<TerminalOutputSnapshot>,
    ) -> (Self, PendingTerminalAttach)
    where
        C: AppContext,
    {
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let terminal_id = config.terminal_id.clone();
        let (session_event_tx, session_event_rx) = mpsc::channel();
        let (output_tx, output_rx) = flume::unbounded();
        let (session, initial_layout_rx) = TerminalSessionBinding::pending(config.clone());
        let writer = TerminalSessionWriter::new(session.clone());
        let restored_output_bytes = restored_output
            .as_ref()
            .map(|output| output.bytes)
            .unwrap_or_default();
        let restored_tail_bytes = restored_output
            .as_ref()
            .map(|output| output.tail.len())
            .unwrap_or_default();
        let view_started_at = Instant::now();
        let view = cx.new(|cx| {
            TerminalView::new(
                writer,
                output_rx,
                session_event_rx,
                session.clone(),
                terminal_config,
                restored_output,
                cx,
            )
        });
        codux_runtime::runtime_trace::runtime_trace(
            "terminal-restore",
            &format!(
                "pending_view elapsed_ms={} terminal_id={} restored_bytes={} restored_tail_bytes={}",
                view_started_at.elapsed().as_millis(),
                terminal_id.as_deref().unwrap_or("none"),
                restored_output_bytes,
                restored_tail_bytes
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
        let attach_total_started_at = Instant::now();
        let layout_started_at = Instant::now();
        let initial_layout = pending.wait_for_initial_layout();
        let layout_wait_ms = layout_started_at.elapsed().as_millis();
        let config = terminal_pty_config_with_view(pty_config, &terminal_config);
        let terminal_id = config.terminal_id.clone();
        let session_event_tx = pending.session_event_tx.clone();
        let emit = Arc::new(move |event| match event {
            TerminalEvent::Exit { .. } => session_event_tx.send(TerminalUiEvent::Exit).is_ok(),
            TerminalEvent::Error { message, .. } => session_event_tx
                .send(TerminalUiEvent::Error(message))
                .is_ok(),
            TerminalEvent::Output { .. } => session_event_tx.send(TerminalUiEvent::Wakeup).is_ok(),
            TerminalEvent::Viewport {
                owner, generation, ..
            } => session_event_tx
                .send(TerminalUiEvent::Viewport {
                    remote_owner: owner != terminal_viewport_local_owner(),
                    generation,
                })
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
                    "initial_layout_ready terminal_id={} cols={} rows={} wait_ms={}",
                    terminal_id.as_deref().unwrap_or("none"),
                    cols,
                    rows,
                    layout_wait_ms
                ),
            ),
            None => codux_runtime::runtime_trace::runtime_trace(
                "terminal-restore",
                &format!(
                    "initial_layout_timeout terminal_id={} wait_ms={}",
                    terminal_id.as_deref().unwrap_or("none"),
                    layout_wait_ms
                ),
            ),
        }
        let output_tx = pending.output_tx;
        let output_terminal_id = terminal_id.clone().unwrap_or_else(|| attached_id.clone());
        codux_runtime::async_runtime::spawn(async move {
            let mut first_output = true;
            while let Ok(bytes) = output_rx.recv_async().await {
                if first_output {
                    first_output = false;
                    codux_runtime::runtime_trace::runtime_trace(
                        "terminal-restore",
                        &format!(
                            "first_output terminal_id={} bytes={} attach_total_ms={}",
                            output_terminal_id,
                            bytes.len(),
                            attach_total_started_at.elapsed().as_millis()
                        ),
                    );
                }
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

    pub fn matches_pty_config(&self, config: &TerminalPtyConfig) -> bool {
        self.session.matches_pty_config(config)
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
    pending_match_config: Option<TerminalPtyConfig>,
    pending_writes: VecDeque<Vec<u8>>,
    pending_write_bytes: usize,
    last_resize: Option<(u16, u16)>,
    initial_layout_tx: Option<mpsc::Sender<(u16, u16)>>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct TerminalLayoutRecord {
    initialized: bool,
    resized: bool,
}

impl TerminalSessionBinding {
    fn pending(config: TerminalPtyConfig) -> (Self, mpsc::Receiver<(u16, u16)>) {
        let (initial_layout_tx, initial_layout_rx) = mpsc::channel();
        (
            Self {
                inner: Arc::new(Mutex::new(TerminalSessionBindingInner {
                    session: None,
                    pending_match_config: Some(config),
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
                pending_match_config: None,
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
            inner.pending_match_config = None;
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
        let (session, last_resize) = {
            let inner = self.inner.lock();
            (inner.session.clone(), inner.last_resize)
        };
        if let Some(session) = session {
            let handle = session.clone_handle();
            let state = handle.claim_viewport_passive(terminal_viewport_local_owner())?;
            if state.owner == terminal_viewport_local_owner()
                && let Some((cols, rows)) = last_resize
                && (state.cols, state.rows) != (cols, rows)
            {
                handle.resize_viewport(terminal_viewport_local_owner(), cols, rows)?;
            }
        }
        Ok(())
    }

    fn restore_local_viewport(&self) -> Result<()> {
        let (session, last_resize) = {
            let inner = self.inner.lock();
            (inner.session.clone(), inner.last_resize)
        };
        if let Some(session) = session {
            let handle = session.clone_handle();
            let state = handle.claim_viewport(terminal_viewport_local_owner())?;
            if let Some((cols, rows)) = last_resize
                && (state.cols, state.rows) != (cols, rows)
            {
                handle.resize_viewport(terminal_viewport_local_owner(), cols, rows)?;
            }
        }
        Ok(())
    }

    fn force_local_viewport_if_current_owner(&self) -> Result<()> {
        if self.local_viewport_owns() {
            self.restore_local_viewport()?;
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

    fn record_layout(&self, cols: u16, rows: u16) -> TerminalLayoutRecord {
        let (initial_layout_tx, record) = {
            let mut inner = self.inner.lock();
            let previous = inner.last_resize;
            inner.last_resize = Some((cols, rows));
            (
                inner.initial_layout_tx.take(),
                TerminalLayoutRecord {
                    initialized: previous.is_none(),
                    resized: previous.is_some_and(|size| size != (cols, rows)),
                },
            )
        };
        if let Some(tx) = initial_layout_tx {
            let _ = tx.send((cols, rows));
        }
        record
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

    fn matches_pty_config(&self, config: &TerminalPtyConfig) -> bool {
        let inner = self.inner.lock();
        if let Some(session) = inner.session.as_ref() {
            return session.matches_config(config, None);
        }
        inner
            .pending_match_config
            .as_ref()
            .is_some_and(|pending| terminal_pty_configs_match(pending, config))
    }
}

fn terminal_pty_configs_match(left: &TerminalPtyConfig, right: &TerminalPtyConfig) -> bool {
    normalized_config_path(left.cwd.as_deref()) == normalized_config_path(right.cwd.as_deref())
        && normalized_config_value(left.project_id.as_deref())
            == normalized_config_value(right.project_id.as_deref())
        && normalized_config_value(left.session_key.as_deref())
            == normalized_config_value(right.session_key.as_deref())
        && normalized_config_value(left.terminal_id.as_deref())
            == normalized_config_value(right.terminal_id.as_deref())
}

fn normalized_config_path(value: Option<&str>) -> Option<String> {
    normalized_config_value(value).map(|path| {
        PathBuf::from(path)
            .components()
            .as_path()
            .to_string_lossy()
            .to_string()
    })
}

fn normalized_config_value(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}
