use super::RuntimeStdioWriter;
use codux_runtime_core::runtime_stdio::RuntimeStdioFrame;
use codux_runtime_live::ai_runtime::AIRuntimeBridge;
use codux_runtime_live::terminal_pty::{TerminalManager, TerminalPtyConfig};
use codux_terminal_core::TerminalEvent;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::Arc;

#[derive(Clone)]
pub(super) struct RuntimeStdioTerminals {
    manager: Arc<TerminalManager>,
    data_dir: PathBuf,
    writer: RuntimeStdioWriter,
}

impl RuntimeStdioTerminals {
    pub(super) fn new(
        data_dir: PathBuf,
        writer: RuntimeStdioWriter,
        ai_runtime: Arc<AIRuntimeBridge>,
    ) -> Self {
        Self {
            manager: Arc::new(TerminalManager::with_ai_runtime(ai_runtime)),
            data_dir,
            writer,
        }
    }

    pub(super) fn manager(&self) -> Arc<TerminalManager> {
        Arc::clone(&self.manager)
    }

    pub(super) fn list(&self) -> Result<Value, String> {
        serde_json::to_value(self.manager.list()).map_err(|error| error.to_string())
    }

    pub(super) fn create(&self, params: &Value) -> Result<Value, String> {
        let config = serde_json::from_value::<TerminalPtyConfig>(params.clone())
            .map_err(|error| format!("invalid terminal config: {error}"))?;
        let terminal = self.create_config(config)?;
        serde_json::to_value(terminal).map_err(|error| error.to_string())
    }

    pub(super) fn create_config(
        &self,
        mut config: TerminalPtyConfig,
    ) -> Result<codux_terminal_core::TerminalSessionSnapshot, String> {
        let runtime_root = self.data_dir.join("runtime-root");
        config.support_dir = Some(self.data_dir.clone());
        config.runtime_root = Some(runtime_root.clone());
        config.tool_permissions_file = Some(self.data_dir.join("tool_permissions.json"));
        crate::memory::prepare_terminal_launch_context(&mut config, &self.data_dir, &runtime_root)?;
        if config.terminal_id.is_none() {
            config.terminal_id = Some(uuid::Uuid::new_v4().to_string());
        }
        let writer = self.writer.clone();
        let session_id = self
            .manager
            .create(config, move |event| emit_terminal_event(&writer, event))
            .map_err(|error| error.to_string())?;
        let terminal = self
            .manager
            .list()
            .into_iter()
            .find(|terminal| terminal.id == session_id)
            .ok_or_else(|| "created terminal is unavailable".to_string())?;
        Ok(terminal)
    }

    pub(super) fn input(&self, params: &Value) -> Result<Value, String> {
        use base64::Engine;
        let session_id = required_str(params, "sessionId")?;
        let data = base64::engine::general_purpose::STANDARD
            .decode(required_str(params, "bytes")?)
            .map_err(|error| error.to_string())?;
        self.manager
            .write(session_id, &data)
            .map_err(|error| error.to_string())?;
        Ok(json!({ "sessionId": session_id }))
    }

    pub(super) fn resize(&self, params: &Value) -> Result<Value, String> {
        let session_id = required_str(params, "sessionId")?;
        let cols = required_u16(params, "cols")?;
        let rows = required_u16(params, "rows")?;
        self.manager
            .resize(session_id, cols, rows)
            .map_err(|error| error.to_string())?;
        Ok(json!({ "sessionId": session_id, "cols": cols, "rows": rows }))
    }

    pub(super) fn close(&self, params: &Value) -> Result<Value, String> {
        let session_id = required_str(params, "sessionId")?;
        self.manager
            .kill(session_id)
            .map_err(|error| error.to_string())?;
        Ok(json!({ "sessionId": session_id }))
    }
}

fn emit_terminal_event(writer: &RuntimeStdioWriter, event: TerminalEvent) {
    use base64::Engine;
    let (method, params) = match event {
        TerminalEvent::Output {
            session_id, bytes, ..
        } => (
            "terminal.output",
            json!({
                "sessionId": session_id,
                "bytes": base64::engine::general_purpose::STANDARD.encode(bytes),
            }),
        ),
        TerminalEvent::Exit {
            session_id,
            exit_code,
        } => (
            "terminal.exit",
            json!({ "sessionId": session_id, "exitCode": exit_code }),
        ),
        TerminalEvent::Error {
            session_id,
            message,
        } => (
            "terminal.error",
            json!({ "sessionId": session_id, "message": message }),
        ),
        TerminalEvent::Viewport {
            session_id,
            owner,
            cols,
            rows,
            generation,
        } => (
            "terminal.viewport",
            json!({
                "sessionId": session_id,
                "owner": owner,
                "cols": cols,
                "rows": rows,
                "generation": generation,
            }),
        ),
    };
    let _ = writer.write(&RuntimeStdioFrame::Event {
        method: method.to_string(),
        params,
    });
}

fn number(params: &Value, key: &str) -> Option<u16> {
    params
        .get(key)
        .and_then(Value::as_u64)
        .and_then(|value| u16::try_from(value).ok())
        .filter(|value| *value > 0)
}

fn required_str<'a>(params: &'a Value, key: &str) -> Result<&'a str, String> {
    params
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{key} is required"))
}

fn required_u16(params: &Value, key: &str) -> Result<u16, String> {
    number(params, key).ok_or_else(|| format!("{key} must be a positive integer"))
}
