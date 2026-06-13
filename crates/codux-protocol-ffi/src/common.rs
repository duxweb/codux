use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteTransport, RemoteTransportCandidate, remote_stun_urls,
};
use codux_terminal_core::{RemoteRuntimeModel, RemoteSequenceGuard};
use std::any::Any;
use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char};
use std::ptr;
use std::sync::{Arc, Mutex};
use tokio::runtime::Runtime;

pub type FfiRemoteRuntimeModel = RemoteRuntimeModel;

pub struct FfiControllerTransport {
    pub(crate) transport: Mutex<Arc<dyn RemoteTransport>>,
    pub(crate) events: Arc<Mutex<VecDeque<String>>>,
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
                })
                .filter(|candidate| {
                    !candidate.kind.trim().is_empty() && !candidate.url.trim().is_empty()
                })
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    let stun_urls = value
        .get("stunUrls")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(serde_json::Value::as_str)
                .map(str::to_string)
                .filter(|value| !value.trim().is_empty())
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(remote_stun_urls);
    Ok(RemoteControllerTransportConfig {
        server_url: json_string_field(&value, "serverUrl"),
        host_id: json_string_field(&value, "hostId"),
        device_id: json_string_field(&value, "deviceId"),
        device_token: json_string_field(&value, "deviceToken"),
        transports,
        stun_urls,
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
    events: &Arc<Mutex<VecDeque<String>>>,
    event: serde_json::Value,
) {
    if let Ok(mut events) = events.lock() {
        events.push_back(event.to_string());
    }
}
