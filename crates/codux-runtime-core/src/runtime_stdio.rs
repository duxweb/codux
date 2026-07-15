use serde::{Deserialize, Serialize};
use serde_json::Value;

pub const RUNTIME_STDIO_PROTOCOL_VERSION: u32 = 3;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub enum RuntimeStdioFrame {
    Hello {
        protocol_version: u32,
        runtime_version: String,
        platform: String,
        capabilities: Vec<String>,
    },
    Request {
        id: u64,
        method: String,
        #[serde(default)]
        params: Value,
    },
    Notify {
        method: String,
        #[serde(default)]
        params: Value,
    },
    Response {
        id: u64,
        result: Value,
    },
    Error {
        id: Option<u64>,
        message: String,
    },
    Event {
        method: String,
        #[serde(default)]
        params: Value,
    },
}

pub fn encode_runtime_stdio_frame(frame: &RuntimeStdioFrame) -> Result<Vec<u8>, serde_json::Error> {
    let mut bytes = serde_json::to_vec(frame)?;
    bytes.push(b'\n');
    Ok(bytes)
}

pub fn decode_runtime_stdio_frame(line: &[u8]) -> Result<RuntimeStdioFrame, serde_json::Error> {
    serde_json::from_slice(line)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn runtime_stdio_frame_round_trips_as_single_line_json() {
        let frame = RuntimeStdioFrame::Request {
            id: 42,
            method: "file.list".to_string(),
            params: json!({ "path": "/home/user/project" }),
        };

        let encoded = encode_runtime_stdio_frame(&frame).unwrap();
        assert_eq!(encoded.last(), Some(&b'\n'));
        assert_eq!(encoded.iter().filter(|byte| **byte == b'\n').count(), 1);
        assert_eq!(decode_runtime_stdio_frame(&encoded).unwrap(), frame);
    }

    #[test]
    fn runtime_stdio_hello_has_no_remote_identity_fields() {
        let frame = RuntimeStdioFrame::Hello {
            protocol_version: RUNTIME_STDIO_PROTOCOL_VERSION,
            runtime_version: "2.0.0".to_string(),
            platform: "linux".to_string(),
            capabilities: vec!["terminal".to_string()],
        };

        let value = serde_json::to_value(frame).unwrap();
        assert!(value.get("deviceId").is_none());
        assert!(value.get("hostId").is_none());
        assert_eq!(value["kind"], "hello");
        assert_eq!(value["protocolVersion"], RUNTIME_STDIO_PROTOCOL_VERSION);
        assert!(value.get("protocol_version").is_none());
    }

    #[test]
    fn runtime_stdio_notification_has_no_request_id() {
        let value = serde_json::to_value(RuntimeStdioFrame::Notify {
            method: "terminal.resize".to_string(),
            params: json!({ "sessionId": "terminal-1", "cols": 120, "rows": 40 }),
        })
        .unwrap();

        assert_eq!(value["kind"], "notify");
        assert!(value.get("id").is_none());
    }
}
