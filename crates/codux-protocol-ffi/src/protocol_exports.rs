use crate::common::{c_to_string, string_to_c};
use codux_protocol::{
    REMOTE_AI_STATS, REMOTE_DEVICE_CONNECTED, REMOTE_DEVICE_DISCONNECTED, REMOTE_DEVICE_INFO,
    REMOTE_ERROR, REMOTE_FILE_DELETE, REMOTE_FILE_DELETED, REMOTE_FILE_LIST, REMOTE_FILE_READ,
    REMOTE_FILE_RENAME, REMOTE_FILE_RENAMED, REMOTE_FILE_WRITE, REMOTE_FILE_WRITTEN,
    REMOTE_GIT_STATUS, REMOTE_HELLO, REMOTE_HOST_INFO, REMOTE_HOST_OFFLINE,
    REMOTE_PAIRING_CONFIRMED, REMOTE_PAIRING_REJECTED, REMOTE_PAIRING_REQUEST, REMOTE_PROJECT_ADD,
    REMOTE_PROJECT_EDIT, REMOTE_PROJECT_LIST, REMOTE_PROJECT_REMOVE, REMOTE_PROJECT_SELECT,
    REMOTE_PROJECT_SELECTED, REMOTE_PROJECT_UPDATED, REMOTE_PROTOCOL_VERSION, REMOTE_RELAY_ERROR,
    REMOTE_RESOURCE_AI_STATS, REMOTE_RESOURCE_FILES, REMOTE_RESOURCE_GIT_STATUS,
    REMOTE_RESOURCE_PROJECTS, REMOTE_RESOURCE_SUBSCRIBE, REMOTE_RESOURCE_TERMINALS,
    REMOTE_RESOURCE_UNSUBSCRIBE, REMOTE_RESOURCE_WORKTREES, REMOTE_SECURE_MESSAGE,
    REMOTE_SECURE_REQUIRED, REMOTE_TERMINAL_BUFFER, REMOTE_TERMINAL_CLOSE, REMOTE_TERMINAL_CLOSED,
    REMOTE_TERMINAL_CREATE, REMOTE_TERMINAL_CREATED, REMOTE_TERMINAL_INPUT,
    REMOTE_TERMINAL_INPUT_ACK, REMOTE_TERMINAL_LIST, REMOTE_TERMINAL_OUTPUT,
    REMOTE_TERMINAL_OUTPUT_ACK, REMOTE_TERMINAL_SUBSCRIBE, REMOTE_TERMINAL_UNSUBSCRIBE,
    REMOTE_TERMINAL_UPLOAD_ACK, REMOTE_TERMINAL_UPLOAD_CHUNK, REMOTE_TERMINAL_UPLOAD_FINISH,
    REMOTE_TERMINAL_UPLOAD_START, REMOTE_TERMINAL_UPLOADED, REMOTE_TERMINAL_VIEWPORT_CLAIM,
    REMOTE_TERMINAL_VIEWPORT_RELEASE, REMOTE_TERMINAL_VIEWPORT_RESIZE,
    REMOTE_TERMINAL_VIEWPORT_SCROLL, REMOTE_TERMINAL_VIEWPORT_SCROLLED,
    REMOTE_TERMINAL_VIEWPORT_STATE, REMOTE_TRANSPORT_PING, REMOTE_TRANSPORT_PONG,
    REMOTE_TRANSPORT_WEBRTC, REMOTE_TRANSPORT_WEBSOCKET_RELAY, REMOTE_WORKTREE_CREATE,
    REMOTE_WORKTREE_DELETE, REMOTE_WORKTREE_LIST, REMOTE_WORKTREE_MERGE, REMOTE_WORKTREE_SELECT,
    REMOTE_WORKTREE_UPDATED, relay_blocks_message_type,
};
use serde_json::json;
use std::ffi::c_char;
use std::ptr;

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_version() -> *mut c_char {
    string_to_c(REMOTE_PROTOCOL_VERSION)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_message_type(name: *const c_char) -> *mut c_char {
    let Some(name) = c_to_string(name) else {
        return ptr::null_mut();
    };
    string_to_c(message_type_by_name(&name).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_resource_type(name: *const c_char) -> *mut c_char {
    let Some(name) = c_to_string(name) else {
        return ptr::null_mut();
    };
    string_to_c(resource_type_by_name(&name).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_transport_kind(name: *const c_char) -> *mut c_char {
    let Some(name) = c_to_string(name) else {
        return ptr::null_mut();
    };
    string_to_c(transport_kind_by_name(&name).unwrap_or_default())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_relay_blocks_message(kind: *const c_char) -> bool {
    let Some(kind) = c_to_string(kind) else {
        return false;
    };
    relay_blocks_message_type(&kind)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_resource_subscribe_json(
    resource: *const c_char,
    project_id: *const c_char,
    session_id: *const c_char,
    baseline: bool,
    max_chars: i32,
    chunk_chars: i32,
) -> *mut c_char {
    let Some(resource) = c_to_string(resource) else {
        return ptr::null_mut();
    };
    let project_id = c_to_string(project_id).filter(|value| !value.trim().is_empty());
    let session_id = c_to_string(session_id).filter(|value| !value.trim().is_empty());
    let mut payload = json!({
        "resource": resource,
    });
    if let Some(project_id) = project_id {
        payload["projectId"] = json!(project_id);
    }
    if let Some(session_id) = session_id.as_deref() {
        payload["sessionId"] = json!(session_id);
    }
    if baseline {
        payload["baseline"] = json!(true);
    }
    if max_chars > 0 {
        payload["maxChars"] = json!(max_chars);
    }
    if chunk_chars > 0 {
        payload["chunkChars"] = json!(chunk_chars);
    }
    let envelope = json!({
        "type": REMOTE_RESOURCE_SUBSCRIBE,
        "sessionId": session_id,
        "payload": payload,
    });
    string_to_c(envelope.to_string())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_protocol_resource_unsubscribe_json(
    resource: *const c_char,
    project_id: *const c_char,
    session_id: *const c_char,
) -> *mut c_char {
    let Some(resource) = c_to_string(resource) else {
        return ptr::null_mut();
    };
    let project_id = c_to_string(project_id).filter(|value| !value.trim().is_empty());
    let session_id = c_to_string(session_id).filter(|value| !value.trim().is_empty());
    let mut payload = json!({
        "resource": resource,
    });
    if let Some(project_id) = project_id {
        payload["projectId"] = json!(project_id);
    }
    if let Some(session_id) = session_id.as_deref() {
        payload["sessionId"] = json!(session_id);
    }
    let envelope = json!({
        "type": REMOTE_RESOURCE_UNSUBSCRIBE,
        "sessionId": session_id,
        "payload": payload,
    });
    string_to_c(envelope.to_string())
}

fn message_type_by_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "hello" => REMOTE_HELLO,
        "error" => REMOTE_ERROR,
        "relayError" => REMOTE_RELAY_ERROR,
        "secureMessage" => REMOTE_SECURE_MESSAGE,
        "secureRequired" => REMOTE_SECURE_REQUIRED,
        "hostInfo" => REMOTE_HOST_INFO,
        "hostOffline" => REMOTE_HOST_OFFLINE,
        "deviceInfo" => REMOTE_DEVICE_INFO,
        "deviceConnected" => REMOTE_DEVICE_CONNECTED,
        "deviceDisconnected" => REMOTE_DEVICE_DISCONNECTED,
        "pairingRequest" => REMOTE_PAIRING_REQUEST,
        "pairingConfirmed" => REMOTE_PAIRING_CONFIRMED,
        "pairingRejected" => REMOTE_PAIRING_REJECTED,
        "transportPing" => REMOTE_TRANSPORT_PING,
        "transportPong" => REMOTE_TRANSPORT_PONG,
        "resourceSubscribe" => REMOTE_RESOURCE_SUBSCRIBE,
        "resourceUnsubscribe" => REMOTE_RESOURCE_UNSUBSCRIBE,
        "projectList" => REMOTE_PROJECT_LIST,
        "projectSelect" => REMOTE_PROJECT_SELECT,
        "projectSelected" => REMOTE_PROJECT_SELECTED,
        "projectAdd" => REMOTE_PROJECT_ADD,
        "projectEdit" => REMOTE_PROJECT_EDIT,
        "projectRemove" => REMOTE_PROJECT_REMOVE,
        "projectUpdated" => REMOTE_PROJECT_UPDATED,
        "worktreeList" => REMOTE_WORKTREE_LIST,
        "worktreeSelect" => REMOTE_WORKTREE_SELECT,
        "worktreeCreate" => REMOTE_WORKTREE_CREATE,
        "worktreeMerge" => REMOTE_WORKTREE_MERGE,
        "worktreeDelete" => REMOTE_WORKTREE_DELETE,
        "worktreeUpdated" => REMOTE_WORKTREE_UPDATED,
        "terminalList" => REMOTE_TERMINAL_LIST,
        "terminalSubscribe" => REMOTE_TERMINAL_SUBSCRIBE,
        "terminalUnsubscribe" => REMOTE_TERMINAL_UNSUBSCRIBE,
        "terminalCreate" => REMOTE_TERMINAL_CREATE,
        "terminalCreated" => REMOTE_TERMINAL_CREATED,
        "terminalClose" => REMOTE_TERMINAL_CLOSE,
        "terminalClosed" => REMOTE_TERMINAL_CLOSED,
        "terminalBuffer" => REMOTE_TERMINAL_BUFFER,
        "terminalOutput" => REMOTE_TERMINAL_OUTPUT,
        "terminalOutputAck" => REMOTE_TERMINAL_OUTPUT_ACK,
        "terminalInput" => REMOTE_TERMINAL_INPUT,
        "terminalInputAck" => REMOTE_TERMINAL_INPUT_ACK,
        "terminalViewportClaim" => REMOTE_TERMINAL_VIEWPORT_CLAIM,
        "terminalViewportResize" => REMOTE_TERMINAL_VIEWPORT_RESIZE,
        "terminalViewportRelease" => REMOTE_TERMINAL_VIEWPORT_RELEASE,
        "terminalViewportScroll" => REMOTE_TERMINAL_VIEWPORT_SCROLL,
        "terminalViewportScrolled" => REMOTE_TERMINAL_VIEWPORT_SCROLLED,
        "terminalViewportState" => REMOTE_TERMINAL_VIEWPORT_STATE,
        "terminalUploadStart" => REMOTE_TERMINAL_UPLOAD_START,
        "terminalUploadChunk" => REMOTE_TERMINAL_UPLOAD_CHUNK,
        "terminalUploadFinish" => REMOTE_TERMINAL_UPLOAD_FINISH,
        "terminalUploadAck" => REMOTE_TERMINAL_UPLOAD_ACK,
        "terminalUploaded" => REMOTE_TERMINAL_UPLOADED,
        "fileList" => REMOTE_FILE_LIST,
        "fileRead" => REMOTE_FILE_READ,
        "fileWrite" => REMOTE_FILE_WRITE,
        "fileWritten" => REMOTE_FILE_WRITTEN,
        "fileRename" => REMOTE_FILE_RENAME,
        "fileRenamed" => REMOTE_FILE_RENAMED,
        "fileDelete" => REMOTE_FILE_DELETE,
        "fileDeleted" => REMOTE_FILE_DELETED,
        "gitStatus" => REMOTE_GIT_STATUS,
        "aiStats" => REMOTE_AI_STATS,
        _ => return None,
    })
}

fn resource_type_by_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "projects" => REMOTE_RESOURCE_PROJECTS,
        "terminals" => REMOTE_RESOURCE_TERMINALS,
        "worktrees" => REMOTE_RESOURCE_WORKTREES,
        "gitStatus" => REMOTE_RESOURCE_GIT_STATUS,
        "aiStats" => REMOTE_RESOURCE_AI_STATS,
        "files" => REMOTE_RESOURCE_FILES,
        _ => return None,
    })
}

fn transport_kind_by_name(name: &str) -> Option<&'static str> {
    Some(match name {
        "websocketRelay" => REMOTE_TRANSPORT_WEBSOCKET_RELAY,
        "webRtc" => REMOTE_TRANSPORT_WEBRTC,
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_protocol_names() {
        assert_eq!(
            message_type_by_name("terminalOutput"),
            Some("terminal.output")
        );
        // Regression: these names are declared on the Dart RemoteMessageType side;
        // a missing arm here silently resolves them to "" at the FFI boundary.
        assert_eq!(message_type_by_name("deviceInfo"), Some("device.info"));
        assert_eq!(
            message_type_by_name("secureRequired"),
            Some("secure.required")
        );
        assert_eq!(
            message_type_by_name("terminalViewportScroll"),
            Some("terminal.viewport.scroll")
        );
        assert_eq!(
            message_type_by_name("pairingConfirmed"),
            Some("pairing.confirmed")
        );
        assert_eq!(resource_type_by_name("gitStatus"), Some("git.status"));
        assert_eq!(
            transport_kind_by_name("websocketRelay"),
            Some("websocketRelay")
        );
        assert_eq!(transport_kind_by_name("webRtc"), Some("webRtc"));
    }
}
