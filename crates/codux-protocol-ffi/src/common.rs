use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteTransport, RemoteTransportCandidate,
};
use codux_terminal_core::{RemoteRuntimeModel, RemoteSequenceGuard};
use std::any::Any;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char};
use std::ptr;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

pub type FfiRemoteRuntimeModel = RemoteRuntimeModel;
pub(crate) const CONTROLLER_TRANSPORT_EVENT_LIMIT: usize = 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TransportEventKind {
    LifecycleState,
    State,
    ControlMessage,
    TerminalOutput,
    Log,
    Other,
}

pub(crate) struct TransportEvent {
    pub(crate) json: String,
    kind: TransportEventKind,
}

pub struct FfiControllerTransport {
    pub(crate) transport: Arc<Mutex<Option<Arc<dyn RemoteTransport>>>>,
    pub(crate) events: Arc<Mutex<VecDeque<TransportEvent>>>,
    pub(crate) runtime: Arc<Runtime>,
}

static LAST_ERROR: Mutex<Option<String>> = Mutex::new(None);

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_last_error() -> *mut c_char {
    let error = LAST_ERROR
        .lock()
        .ok()
        .and_then(|error| error.clone())
        .unwrap_or_default();
    string_to_c(error)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_string_free(value: *mut c_char) {
    if value.is_null() {
        return;
    }
    unsafe {
        drop(CString::from_raw(value));
    }
}

pub(crate) fn c_to_string(value: *const c_char) -> Option<String> {
    if value.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(value).to_str().ok().map(str::to_string) }
}

pub(crate) fn string_to_c(value: impl Into<String>) -> *mut c_char {
    CString::new(value.into())
        .map(CString::into_raw)
        .unwrap_or(ptr::null_mut())
}

pub(crate) fn json_string_to_c(value: &impl serde::Serialize) -> *mut c_char {
    string_to_c(serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string()))
}

pub(crate) fn clean_ffi_string(value: Option<String>) -> Option<String> {
    value.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(trimmed.to_string())
        }
    })
}

pub(crate) fn optional_sequence(value: i64) -> Option<i64> {
    if value < 0 { None } else { Some(value) }
}

pub(crate) fn remote_sequence_guard_mut<'a>(
    guard: *mut RemoteSequenceGuard,
) -> Option<&'a mut RemoteSequenceGuard> {
    if guard.is_null() {
        return None;
    }
    unsafe { guard.as_mut() }
}

pub(crate) fn remote_runtime_model_ref<'a>(
    model: *const FfiRemoteRuntimeModel,
) -> Option<&'a FfiRemoteRuntimeModel> {
    if model.is_null() {
        return None;
    }
    unsafe { model.as_ref() }
}

pub(crate) fn remote_runtime_model_mut<'a>(
    model: *mut FfiRemoteRuntimeModel,
) -> Option<&'a mut FfiRemoteRuntimeModel> {
    if model.is_null() {
        return None;
    }
    unsafe { model.as_mut() }
}

pub(crate) fn controller_transport_ref<'a>(
    transport: *mut FfiControllerTransport,
) -> Option<&'a FfiControllerTransport> {
    if transport.is_null() {
        return None;
    }
    unsafe { transport.as_ref() }
}

pub(crate) fn controller_transport_config_from_json(
    config_json: &str,
) -> Result<RemoteControllerTransportConfig, String> {
    let value = serde_json::from_str::<serde_json::Value>(config_json)
        .map_err(|error| error.to_string())?;
    let transports = value
        .get("transports")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .map(|item| RemoteTransportCandidate {
                    kind: item
                        .get("kind")
                        .or_else(|| item.get("transport"))
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    url: item
                        .get("url")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    node_id: item
                        .get("nodeId")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relay_url: item
                        .get("relayUrl")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    ticket: item
                        .get("ticket")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                    relay_authentication: item
                        .get("relayAuthentication")
                        .and_then(serde_json::Value::as_str)
                        .unwrap_or_default()
                        .to_string(),
                })
                .filter(|candidate| {
                    !candidate.kind.trim().is_empty()
                        && (!candidate.ticket.trim().is_empty()
                            || (!candidate.node_id.trim().is_empty()
                                && !candidate.relay_url.trim().is_empty()))
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    Ok(RemoteControllerTransportConfig {
        relay_url: json_string_field(&value, "relayUrl"),
        host_id: json_string_field(&value, "hostId"),
        device_id: json_string_field(&value, "deviceId"),
        device_token: json_string_field(&value, "deviceToken"),
        transports,
    })
}

fn json_string_field(value: &serde_json::Value, key: &str) -> String {
    value
        .get(key)
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub(crate) fn set_last_error(message: impl Into<String>) {
    if let Ok(mut error) = LAST_ERROR.lock() {
        *error = Some(message.into());
    }
}

pub(crate) fn clear_last_error() {
    if let Ok(mut error) = LAST_ERROR.lock() {
        *error = None;
    }
}

pub(crate) fn panic_payload_message(payload: &(dyn Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        return (*message).to_string();
    }
    if let Some(message) = payload.downcast_ref::<String>() {
        return message.clone();
    }
    "unknown panic payload".to_string()
}

pub(crate) fn push_transport_event(
    events: &Arc<Mutex<VecDeque<TransportEvent>>>,
    event: serde_json::Value,
) {
    let kind = transport_event_kind(&event);
    let json = event.to_string();
    if let Ok(mut events) = events.lock() {
        if matches!(
            kind,
            TransportEventKind::LifecycleState | TransportEventKind::State
        ) && let Some(index) = events.iter().position(|event| event.kind == kind)
        {
            events.remove(index);
        }
        while events.len() >= CONTROLLER_TRANSPORT_EVENT_LIMIT {
            let incoming_priority = transport_event_priority(kind);
            let removable = events
                .iter()
                .enumerate()
                .filter(|(_, event)| transport_event_priority(event.kind) <= incoming_priority)
                .min_by_key(|(_, event)| transport_event_priority(event.kind))
                .map(|(index, _)| index);
            let Some(index) = removable else {
                return;
            };
            events.remove(index);
        }
        events.push_back(TransportEvent { json, kind });
    }
}

fn transport_event_kind(event: &serde_json::Value) -> TransportEventKind {
    match event.get("kind").and_then(serde_json::Value::as_str) {
        Some("state") => match event
            .get("state")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .split(':')
            .next()
            .unwrap_or_default()
        {
            "connecting" | "connected" | "closed" | "failed" | "disconnected" => {
                TransportEventKind::LifecycleState
            }
            _ => TransportEventKind::State,
        },
        Some("message") => {
            let terminal_output = event
                .get("data")
                .and_then(serde_json::Value::as_str)
                .and_then(|data| serde_json::from_str::<serde_json::Value>(data).ok())
                .and_then(|envelope| {
                    envelope
                        .get("type")
                        .and_then(serde_json::Value::as_str)
                        .map(|kind| kind == codux_protocol::REMOTE_TERMINAL_OUTPUT)
                })
                .unwrap_or(false);
            if terminal_output {
                TransportEventKind::TerminalOutput
            } else {
                TransportEventKind::ControlMessage
            }
        }
        Some("log") => TransportEventKind::Log,
        _ => TransportEventKind::Other,
    }
}

fn transport_event_priority(kind: TransportEventKind) -> u8 {
    match kind {
        TransportEventKind::Log => 0,
        TransportEventKind::TerminalOutput => 1,
        TransportEventKind::Other => 2,
        TransportEventKind::State => 3,
        TransportEventKind::ControlMessage => 4,
        TransportEventKind::LifecycleState => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn transport_event_queue_is_bounded_and_preserves_lifecycle_state() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        push_transport_event(&events, json!({ "kind": "state", "state": "connected" }));
        for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT {
            push_transport_event(
                &events,
                json!({ "kind": "message", "data": index.to_string() }),
            );
        }

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::ControlMessage)
                .count(),
            CONTROLLER_TRANSPORT_EVENT_LIMIT - 1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::LifecycleState)
                .count(),
            1
        );
    }

    #[test]
    fn transport_event_queue_drops_logs_before_messages() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        push_transport_event(&events, json!({ "kind": "log", "message": "old" }));
        for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT - 1 {
            push_transport_event(
                &events,
                json!({ "kind": "message", "data": index.to_string() }),
            );
        }
        push_transport_event(&events, json!({ "kind": "state", "state": "closed" }));

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert!(
            !events
                .iter()
                .any(|event| event.kind == TransportEventKind::Log)
        );
        assert!(
            events
                .iter()
                .any(|event| event.kind == TransportEventKind::LifecycleState)
        );
    }

    #[test]
    fn transport_event_queue_drops_new_log_before_messages() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT {
            push_transport_event(
                &events,
                json!({ "kind": "message", "data": index.to_string() }),
            );
        }

        push_transport_event(&events, json!({ "kind": "log", "message": "new" }));

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert!(
            events
                .iter()
                .all(|event| event.kind == TransportEventKind::ControlMessage)
        );
    }

    #[test]
    fn transport_event_queue_preserves_control_reply_over_old_state() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        {
            let mut queue = events.lock().unwrap();
            for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT {
                queue.push_back(TransportEvent {
                    json: index.to_string(),
                    kind: TransportEventKind::State,
                });
            }
        }

        push_transport_event(&events, json!({ "kind": "message", "data": "new" }));

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::ControlMessage)
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::State)
                .count(),
            CONTROLLER_TRANSPORT_EVENT_LIMIT - 1
        );
    }

    #[test]
    fn transport_event_queue_preserves_disconnect_over_control_replies() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT {
            push_transport_event(
                &events,
                json!({ "kind": "message", "data": index.to_string() }),
            );
        }

        push_transport_event(&events, json!({ "kind": "state", "state": "closed" }));

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::ControlMessage)
                .count(),
            CONTROLLER_TRANSPORT_EVENT_LIMIT - 1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::LifecycleState)
                .count(),
            1
        );
    }

    #[test]
    fn transport_event_queue_drops_new_path_state_before_control_replies() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT {
            push_transport_event(
                &events,
                json!({ "kind": "message", "data": index.to_string() }),
            );
        }

        push_transport_event(
            &events,
            json!({ "kind": "state", "state": "path:path=direct" }),
        );

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert!(
            events
                .iter()
                .all(|event| event.kind == TransportEventKind::ControlMessage)
        );
    }

    #[test]
    fn transport_event_queue_preserves_control_reply_over_terminal_output() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        for index in 0..CONTROLLER_TRANSPORT_EVENT_LIMIT {
            push_transport_event(
                &events,
                json!({
                    "kind": "message",
                    "data": json!({
                        "type": codux_protocol::REMOTE_TERMINAL_OUTPUT,
                        "payload": { "data": index.to_string() }
                    }).to_string()
                }),
            );
        }

        push_transport_event(
            &events,
            json!({
                "kind": "message",
                "data": json!({ "type": codux_protocol::REMOTE_PROJECT_LIST }).to_string()
            }),
        );

        let events = events.lock().unwrap();
        assert_eq!(events.len(), CONTROLLER_TRANSPORT_EVENT_LIMIT);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::ControlMessage)
                .count(),
            1
        );
    }

    #[test]
    fn transport_event_queue_coalesces_repeated_state_categories() {
        let events = Arc::new(Mutex::new(VecDeque::new()));
        push_transport_event(&events, json!({ "kind": "state", "state": "connecting" }));
        push_transport_event(
            &events,
            json!({ "kind": "state", "state": "connected:path=relay" }),
        );
        push_transport_event(
            &events,
            json!({ "kind": "state", "state": "latency:rtt=20;path=relay" }),
        );
        push_transport_event(
            &events,
            json!({ "kind": "state", "state": "latency:rtt=10;path=direct" }),
        );

        let events = events.lock().unwrap();
        assert_eq!(events.len(), 2);
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::LifecycleState)
                .count(),
            1
        );
        assert_eq!(
            events
                .iter()
                .filter(|event| event.kind == TransportEventKind::State)
                .count(),
            1
        );
        assert!(events.iter().any(|event| event.json.contains("rtt=10")));
    }
}
