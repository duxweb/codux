mod dispatch;
mod terminal;

use codux_runtime_core::runtime_stdio::{
    RUNTIME_STDIO_PROTOCOL_VERSION, RuntimeStdioFrame, decode_runtime_stdio_frame,
    encode_runtime_stdio_frame,
};
use std::io::{BufRead, BufReader, Write};
use std::sync::{Arc, Mutex};

#[derive(Clone)]
pub(super) struct RuntimeStdioWriter {
    stdout: Arc<Mutex<std::io::Stdout>>,
}

impl RuntimeStdioWriter {
    fn new() -> Self {
        Self {
            stdout: Arc::new(Mutex::new(std::io::stdout())),
        }
    }

    pub(super) fn write(&self, frame: &RuntimeStdioFrame) -> Result<(), String> {
        let bytes = encode_runtime_stdio_frame(frame).map_err(|error| error.to_string())?;
        let mut stdout = self
            .stdout
            .lock()
            .map_err(|_| "runtime stdio output is unavailable".to_string())?;
        stdout
            .write_all(&bytes)
            .map_err(|error| error.to_string())?;
        stdout.flush().map_err(|error| error.to_string())
    }
}

pub fn run(version: &str) -> Result<(), String> {
    let data_dir = wsl_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|error| error.to_string())?;
    unsafe {
        std::env::set_var("CODUX_AGENT_DATA_DIR", &data_dir);
    }
    let runtime_root = data_dir.join("runtime-root");
    let runtime_temp = data_dir.join("runtime-temp");
    let home_dir = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| data_dir.clone());
    codux_runtime_live::ai_runtime::AIRuntimeBridge::with_runtime_paths(
        runtime_root,
        runtime_temp,
        home_dir,
    )
    .stage_assets()?;

    let writer = RuntimeStdioWriter::new();
    writer.write(&RuntimeStdioFrame::Hello {
        protocol_version: RUNTIME_STDIO_PROTOCOL_VERSION,
        runtime_version: version.to_string(),
        platform: std::env::consts::OS.to_string(),
        capabilities: vec![
            "file".to_string(),
            "git".to_string(),
            "worktree".to_string(),
            "terminal".to_string(),
        ],
    })?;

    let runtime = Arc::new(dispatch::RuntimeStdioService::new(data_dir, writer.clone()));
    let (terminal_tx, terminal_rx) = std::sync::mpsc::channel();
    let terminal_runtime = Arc::clone(&runtime);
    let terminal_writer = writer.clone();
    let terminal_worker = std::thread::spawn(move || {
        while let Ok(frame) = terminal_rx.recv() {
            dispatch_frame(&terminal_runtime, &terminal_writer, frame);
        }
    });
    let (service_tx, service_rx) = std::sync::mpsc::channel();
    let service_runtime = Arc::clone(&runtime);
    let service_writer = writer.clone();
    let service_worker = std::thread::spawn(move || {
        while let Ok(frame) = service_rx.recv() {
            dispatch_frame(&service_runtime, &service_writer, frame);
        }
    });
    let stdin = std::io::stdin();
    let read_result = (|| {
        for line in BufReader::new(stdin.lock()).split(b'\n') {
            let line = line.map_err(|error| error.to_string())?;
            if line.iter().all(u8::is_ascii_whitespace) {
                continue;
            }
            let frame = match decode_runtime_stdio_frame(&line) {
                Ok(frame) => frame,
                Err(error) => {
                    writer.write(&RuntimeStdioFrame::Error {
                        id: None,
                        message: format!("invalid runtime stdio frame: {error}"),
                    })?;
                    continue;
                }
            };
            let method = match &frame {
                RuntimeStdioFrame::Request { method, .. }
                | RuntimeStdioFrame::Notify { method, .. } => method,
                _ => {
                    writer.write(&RuntimeStdioFrame::Error {
                        id: None,
                        message: "runtime stdio accepts request and notify frames only".to_string(),
                    })?;
                    continue;
                }
            };
            if method.starts_with("terminal.") {
                terminal_tx
                    .send(frame)
                    .map_err(|_| "runtime stdio terminal worker exited".to_string())?;
            } else {
                service_tx
                    .send(frame)
                    .map_err(|_| "runtime stdio service worker exited".to_string())?;
            }
        }
        Ok(())
    })();
    drop(terminal_tx);
    drop(service_tx);
    terminal_worker
        .join()
        .map_err(|_| "runtime stdio terminal worker panicked".to_string())?;
    service_worker
        .join()
        .map_err(|_| "runtime stdio service worker panicked".to_string())?;
    read_result
}

fn dispatch_frame(
    runtime: &dispatch::RuntimeStdioService,
    writer: &RuntimeStdioWriter,
    frame: RuntimeStdioFrame,
) {
    match frame {
        RuntimeStdioFrame::Request { id, method, params } => {
            let frame = match runtime.dispatch(&method, &params) {
                Ok(result) => RuntimeStdioFrame::Response { id, result },
                Err(message) => RuntimeStdioFrame::Error {
                    id: Some(id),
                    message,
                },
            };
            let _ = writer.write(&frame);
        }
        RuntimeStdioFrame::Notify { method, params } => {
            let _ = runtime.dispatch(&method, &params);
        }
        _ => {}
    }
}

fn wsl_data_dir() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home).join(".codux-wsl")
}
