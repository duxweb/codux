struct TermSize {
    cols: usize,
    rows: usize,
}

impl TermSize {
    fn new(cols: usize, rows: usize) -> Self {
        Self { cols, rows }
    }
}

impl Dimensions for TermSize {
    fn total_lines(&self) -> usize {
        self.rows
    }

    fn screen_lines(&self) -> usize {
        self.rows
    }

    fn columns(&self) -> usize {
        self.cols
    }
}

#[derive(Clone)]
enum TerminalUiEvent {
    Wakeup,
    Bell,
    Title(String),
    Error(String),
    Viewport { cols: u16, rows: u16 },
    ClipboardStore(String),
    ClipboardLoad,
    PtyWrite(Vec<u8>),
    ColorRequest(usize, Arc<dyn Fn(Rgb) -> String + Sync + Send + 'static>),
    TextAreaSizeRequest(Arc<dyn Fn(WindowSize) -> String + Sync + Send + 'static>),
    Exit,
}

#[derive(Clone)]
struct GpuiEventProxy {
    tx: mpsc::Sender<TerminalUiEvent>,
}

impl GpuiEventProxy {
    fn new(tx: mpsc::Sender<TerminalUiEvent>) -> Self {
        Self { tx }
    }

    fn send(&self, event: TerminalUiEvent) {
        let _ = self.tx.send(event);
    }
}

impl EventListener for GpuiEventProxy {
    fn send_event(&self, event: Event) {
        match event {
            Event::Wakeup => self.send(TerminalUiEvent::Wakeup),
            Event::Bell => self.send(TerminalUiEvent::Bell),
            Event::Title(title) => self.send(TerminalUiEvent::Title(title)),
            Event::ClipboardStore(_, text) => self.send(TerminalUiEvent::ClipboardStore(text)),
            Event::ClipboardLoad(_, _) => self.send(TerminalUiEvent::ClipboardLoad),
            Event::PtyWrite(text) => self.send(TerminalUiEvent::PtyWrite(text.into_bytes())),
            Event::ColorRequest(index, format) => {
                self.send(TerminalUiEvent::ColorRequest(index, format))
            }
            Event::TextAreaSizeRequest(format) => {
                self.send(TerminalUiEvent::TextAreaSizeRequest(format))
            }
            Event::Exit | Event::ChildExit(_) => self.send(TerminalUiEvent::Exit),
            Event::ResetTitle => self.send(TerminalUiEvent::Title(String::new())),
            Event::MouseCursorDirty | Event::CursorBlinkingChange => {}
        }
    }
}
