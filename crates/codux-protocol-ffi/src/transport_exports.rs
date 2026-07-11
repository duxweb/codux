use crate::common::{
    FfiControllerTransport, TransportEvent, c_to_string, clear_last_error,
    controller_transport_config_from_json, controller_transport_ref, controller_transport_runtime,
    panic_payload_message, push_transport_event, set_last_error, string_to_c,
};
use codux_remote_transport::{
    RemoteControllerTransportConfig, RemoteTransport, RemoteTransportFactory,
    RemoteTransportUpload, preferred_controller_transport_kind, preferred_pairing_transport_kind,
    remote_relay_presets_json, remote_relay_url, remote_relay_url_for_preset,
};
use serde_json::json;
use std::collections::VecDeque;
use std::ffi::{c_char, c_uchar};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::ptr;
use std::sync::{Arc, Mutex};

#[unsafe(no_mangle)]
pub extern "C" fn codux_transport_relay_url(base: *const c_char) -> *mut c_char {
    let Some(base) = c_to_string(base) else {
        return ptr::null_mut();
    };
    string_to_c(remote_relay_url(&base))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_transport_relay_url_for_preset(
    preset: *const c_char,
    custom_url: *const c_char,
) -> *mut c_char {
    let Some(preset) = c_to_string(preset) else {
        return ptr::null_mut();
    };
    let custom_url = c_to_string(custom_url).unwrap_or_default();
    string_to_c(remote_relay_url_for_preset(&preset, &custom_url))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_transport_relay_presets_json() -> *mut c_char {
    string_to_c(remote_relay_presets_json())
}

/// Validate a DECODED pairing-payload JSON object (the caller does the stable
/// base64url/URL decode) through the SINGLE shared parser in `codux_protocol`,
/// then relay-normalize the server and stamp it onto the iroh transports. Returns
/// `{"ok": {...ParsedPairingPayload...}}` or `{"missingFields": [...]}` so the
/// mobile client stops re-implementing the format in Dart and can't drift from
/// the hosts that emit it.
#[unsafe(no_mangle)]
pub extern "C" fn codux_parse_pairing_payload(payload_json: *const c_char) -> *mut c_char {
    let Some(payload_json) = c_to_string(payload_json) else {
        return ptr::null_mut();
    };
    let result = catch_unwind(AssertUnwindSafe(|| {
        let value = serde_json::from_str::<serde_json::Value>(&payload_json)
            .unwrap_or(serde_json::Value::Null);
        match codux_protocol::parse_pairing_payload(&value) {
            Ok(mut parsed) => {
                let server = remote_relay_url(&parsed.server);
                parsed.server = server.clone();
                for candidate in &mut parsed.transports {
                    if candidate.kind == codux_protocol::REMOTE_TRANSPORT_IROH {
                        candidate.url = Some(server.clone());
                    }
                }
                json!({ "ok": parsed })
            }
            Err(missing_fields) => json!({ "missingFields": missing_fields }),
        }
    }));
    match result {
        Ok(value) => string_to_c(value.to_string()),
        Err(payload) => {
            set_last_error(panic_payload_message(&payload));
            string_to_c(json!({ "missingFields": ["invalid"] }).to_string())
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_transport_preferred_kind(
    transports_json: *const c_char,
    pairing: bool,
) -> *mut c_char {
    let Some(transports_json) = c_to_string(transports_json) else {
        return ptr::null_mut();
    };
    let transports = serde_json::from_str::<serde_json::Value>(&transports_json)
        .ok()
        .and_then(|value| value.as_array().cloned())
        .unwrap_or_default();
    let pairs = transports
        .iter()
        .map(|item| {
            (
                item.get("kind")
                    .or_else(|| item.get("transport"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
                item.get("url")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default(),
            )
        })
        .collect::<Vec<_>>();
    let kind = if pairing {
        preferred_pairing_transport_kind(pairs.iter().copied())
    } else {
        preferred_controller_transport_kind(pairs.iter().copied())
    };
    string_to_c(kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_config_summary_json(
    config_json: *const c_char,
) -> *mut c_char {
    let Some(config_json) = c_to_string(config_json) else {
        return ptr::null_mut();
    };
    let Ok(config) = controller_transport_config_from_json(&config_json) else {
        return ptr::null_mut();
    };
    let preferred = preferred_controller_transport_kind(
        config
            .transports
            .iter()
            .map(|candidate| (candidate.kind.as_str(), candidate.url.as_str())),
    );
    string_to_c(
        json!({
            "relayUrl": remote_relay_url(&config.relay_url),
            "hostId": config.host_id,
            "deviceId": config.device_id,
            "transportKind": preferred,
            "transportCount": config.transports.len(),
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_connect_json(
    config_json: *const c_char,
) -> *mut FfiControllerTransport {
    clear_last_error();
    match catch_unwind(AssertUnwindSafe(|| {
        controller_transport_connect_json_inner(config_json)
    })) {
        Ok(transport) => transport,
        Err(payload) => {
            set_last_error(format!(
                "controller transport connect panicked: {}",
                panic_payload_message(payload.as_ref())
            ));
            ptr::null_mut()
        }
    }
}

fn controller_transport_connect_json_inner(
    config_json: *const c_char,
) -> *mut FfiControllerTransport {
    let Some(config_json) = c_to_string(config_json) else {
        set_last_error("missing controller transport config json");
        return ptr::null_mut();
    };
    let config = match controller_transport_config_from_json(&config_json) {
        Ok(config) => config,
        Err(error) => {
            set_last_error(format!("invalid controller transport config: {error}"));
            return ptr::null_mut();
        }
    };
    let runtime = match controller_transport_runtime() {
        Ok(runtime) => runtime,
        Err(error) => {
            set_last_error(error);
            return ptr::null_mut();
        }
    };
    let events = Arc::new(Mutex::new(VecDeque::new()));
    push_transport_event(
        &events,
        json!({
            "kind": "state",
            "state": "connecting",
        }),
    );

    let transport = Arc::new(Mutex::new(None));
    let transport_for_connect = Arc::clone(&transport);
    let events_for_connect = Arc::clone(&events);
    let config_for_connect = config.clone();
    let connect_task = runtime.spawn(async move {
        match connect_controller_transport(&config_for_connect, Arc::clone(&events_for_connect))
            .await
        {
            Ok(connected) => {
                if let Ok(mut current) = transport_for_connect.lock() {
                    *current = Some(connected);
                }
            }
            Err(error) => {
                push_transport_event(
                    &events_for_connect,
                    json!({
                        "kind": "state",
                        "state": format!("failed:failed to connect controller transport: {error}"),
                    }),
                );
            }
        }
    });
    Box::into_raw(Box::new(FfiControllerTransport {
        transport,
        events,
        runtime,
        connect_task: Mutex::new(Some(connect_task)),
    }))
}

async fn connect_controller_transport(
    config: &RemoteControllerTransportConfig,
    events: Arc<Mutex<VecDeque<TransportEvent>>>,
) -> Result<Arc<dyn RemoteTransport>, String> {
    let events_for_message = Arc::clone(&events);
    let events_for_state = Arc::clone(&events);
    let events_for_log = Arc::clone(&events);
    RemoteTransportFactory::connect_controller(
        config,
        Arc::new(move |device_id, data| {
            let text = String::from_utf8(data).unwrap_or_default();
            push_transport_event(
                &events_for_message,
                json!({
                    "kind": "message",
                    "deviceId": device_id,
                    "data": text,
                }),
            );
        }),
        Arc::new(move |device_id, state| {
            push_transport_event(
                &events_for_state,
                json!({
                    "kind": "state",
                    "deviceId": device_id,
                    "state": state,
                }),
            );
        }),
        Some(Arc::new(move |message| {
            push_transport_event(
                &events_for_log,
                json!({
                    "kind": "log",
                    "message": message,
                }),
            );
        })),
    )
    .await
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_send_json(
    transport: *mut FfiControllerTransport,
    envelope_json: *const c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(transport) = controller_transport_ref(transport) else {
            return false;
        };
        let Some(envelope_json) = c_to_string(envelope_json) else {
            return false;
        };
        transport
            .transport
            .lock()
            .ok()
            .and_then(|transport| transport.clone())
            .map(|transport| transport.send(envelope_json.into_bytes(), None))
            .unwrap_or(false)
    }))
    .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_send_terminal_json(
    transport: *mut FfiControllerTransport,
    envelope_json: *const c_char,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(transport) = controller_transport_ref(transport) else {
            return false;
        };
        let Some(envelope_json) = c_to_string(envelope_json) else {
            return false;
        };
        transport
            .transport
            .lock()
            .ok()
            .and_then(|transport| transport.clone())
            .map(|transport| transport.send_terminal(envelope_json.into_bytes(), None))
            .unwrap_or(false)
    }))
    .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_send_terminal_upload(
    transport: *mut FfiControllerTransport,
    device_id: *const c_char,
    session_id: *const c_char,
    name: *const c_char,
    mime: *const c_char,
    kind: *const c_char,
    bytes: *const c_uchar,
    byte_len: usize,
) -> bool {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(transport) = controller_transport_ref(transport) else {
            return false;
        };
        if bytes.is_null() || byte_len == 0 {
            return false;
        }
        let Some(session_id) = c_to_string(session_id) else {
            return false;
        };
        let data = unsafe { std::slice::from_raw_parts(bytes, byte_len) }.to_vec();
        let upload = RemoteTransportUpload {
            device_id: c_to_string(device_id).unwrap_or_default(),
            session_id,
            name: c_to_string(name).unwrap_or_else(|| "upload".to_string()),
            mime: c_to_string(mime).unwrap_or_default(),
            kind: c_to_string(kind).unwrap_or_else(|| "file".to_string()),
            bytes: data,
            ticket: String::new(),
        };
        transport
            .transport
            .lock()
            .ok()
            .and_then(|transport| transport.clone())
            .map(|transport| transport.send_terminal_upload(upload))
            .unwrap_or(false)
    }))
    .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_poll_event_json(
    transport: *mut FfiControllerTransport,
) -> *mut c_char {
    catch_unwind(AssertUnwindSafe(|| {
        let Some(transport) = controller_transport_ref(transport) else {
            return ptr::null_mut();
        };
        let event = transport
            .events
            .lock()
            .ok()
            .and_then(|mut events| events.pop_front());
        match event {
            Some(event) => string_to_c(event.json),
            None => ptr::null_mut(),
        }
    }))
    .unwrap_or(ptr::null_mut())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_controller_transport_close(transport: *mut FfiControllerTransport) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        if transport.is_null() {
            return;
        }
        let transport = unsafe { Box::from_raw(transport) };
        let runtime = Arc::clone(&transport.runtime);
        let connect_task = transport
            .connect_task
            .lock()
            .ok()
            .and_then(|mut task| task.take());
        runtime.block_on(async {
            if let Some(connect_task) = connect_task {
                connect_task.abort();
                let _ = connect_task.await;
            }
            let current = transport
                .transport
                .lock()
                .ok()
                .and_then(|mut transport| transport.take());
            if let Some(current) = current {
                current.shutdown().await;
            }
        });
    }));
}
