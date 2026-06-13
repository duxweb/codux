use crate::common::{
    c_to_string, clean_ffi_string, optional_sequence, remote_sequence_guard_mut, string_to_c,
};
use codux_terminal_core::{
    RemoteSequenceGuard, TerminalInputMode, TerminalKeyInput, TerminalKeyInputModifiers,
    TerminalMouseAction, TerminalMouseButton, TerminalMouseInput, terminal_insert_input_bytes,
    terminal_key_input_bytes, terminal_mouse_input_bytes, terminal_selector_input_bytes,
    terminal_text_input_bytes,
};
use serde_json::json;
use std::ffi::c_char;
use std::ptr;

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_text_input_json(text: *const c_char) -> *mut c_char {
    let Some(text) = c_to_string(text) else {
        return ptr::null_mut();
    };
    let bytes = terminal_text_input_bytes(&text);
    terminal_input_json(bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_insert_input_json(text: *const c_char) -> *mut c_char {
    let Some(text) = c_to_string(text) else {
        return ptr::null_mut();
    };
    let bytes = terminal_insert_input_bytes(&text);
    terminal_input_json(bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_key_input_json(
    key: *const c_char,
    key_char: *const c_char,
    shift: bool,
    alt: bool,
    control: bool,
    platform: bool,
    application_cursor: bool,
) -> *mut c_char {
    let Some(key) = c_to_string(key) else {
        return ptr::null_mut();
    };
    let key_char = clean_ffi_string(c_to_string(key_char));
    let bytes = terminal_key_input_bytes(TerminalKeyInput {
        key: &key,
        key_char: key_char.as_deref(),
        modifiers: TerminalKeyInputModifiers {
            shift,
            alt,
            control,
            platform,
        },
        mode: TerminalInputMode {
            application_cursor,
            ..Default::default()
        },
    })
    .unwrap_or_default();
    terminal_input_json(bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_selector_input_json(
    selector: *const c_char,
    application_cursor: bool,
) -> *mut c_char {
    let Some(selector) = c_to_string(selector) else {
        return ptr::null_mut();
    };
    let bytes = terminal_selector_input_bytes(
        &selector,
        TerminalInputMode {
            application_cursor,
            ..Default::default()
        },
    )
    .unwrap_or_default();
    terminal_input_json(bytes)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_mouse_input_json(
    action: *const c_char,
    button: *const c_char,
    row: i64,
    col: i64,
    shift: bool,
    alt: bool,
    control: bool,
    platform: bool,
    mouse_motion: bool,
    mouse_drag: bool,
    sgr_mouse: bool,
    utf8_mouse: bool,
) -> *mut c_char {
    let Some(action) = c_to_string(action) else {
        return ptr::null_mut();
    };
    let button = clean_ffi_string(c_to_string(button));
    let Some(action) = terminal_mouse_action_from_ffi(&action) else {
        return terminal_input_json(Vec::new());
    };
    let Some(button) = terminal_mouse_button_from_ffi(button.as_deref()) else {
        return terminal_input_json(Vec::new());
    };
    let bytes = terminal_mouse_input_bytes(TerminalMouseInput {
        action,
        button,
        row: usize::try_from(row.max(0)).unwrap_or(0),
        col: usize::try_from(col.max(0)).unwrap_or(0),
        modifiers: TerminalKeyInputModifiers {
            shift,
            alt,
            control,
            platform,
        },
        mode: TerminalInputMode {
            mouse_tracking: true,
            mouse_motion,
            mouse_drag,
            sgr_mouse,
            utf8_mouse,
            ..Default::default()
        },
    })
    .unwrap_or_default();
    terminal_input_json(bytes)
}

fn terminal_input_json(bytes: Vec<u8>) -> *mut c_char {
    string_to_c(
        json!({
            "input": String::from_utf8_lossy(&bytes),
            "bytes": bytes,
        })
        .to_string(),
    )
}

fn terminal_mouse_action_from_ffi(action: &str) -> Option<TerminalMouseAction> {
    match action {
        "press" => Some(TerminalMouseAction::Press),
        "release" => Some(TerminalMouseAction::Release),
        "move" => Some(TerminalMouseAction::Move),
        _ => None,
    }
}

fn terminal_mouse_button_from_ffi(button: Option<&str>) -> Option<Option<TerminalMouseButton>> {
    match button.unwrap_or_default() {
        "" | "none" => Some(None),
        "left" => Some(Some(TerminalMouseButton::Left)),
        "middle" => Some(Some(TerminalMouseButton::Middle)),
        "right" => Some(Some(TerminalMouseButton::Right)),
        "wheelUp" | "wheel-up" | "scrollUp" | "scroll-up" => {
            Some(Some(TerminalMouseButton::WheelUp))
        }
        "wheelDown" | "wheel-down" | "scrollDown" | "scroll-down" => {
            Some(Some(TerminalMouseButton::WheelDown))
        }
        _ => None,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_sequence_guard_new(
    max_entries_per_channel: i64,
) -> *mut RemoteSequenceGuard {
    Box::into_raw(Box::new(RemoteSequenceGuard::new(
        usize::try_from(max_entries_per_channel.max(1)).unwrap_or(128),
    )))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_sequence_guard_free(guard: *mut RemoteSequenceGuard) {
    if guard.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(guard));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_sequence_guard_accept(
    guard: *mut RemoteSequenceGuard,
    kind: *const c_char,
    session_id: *const c_char,
    sequence: i64,
) -> bool {
    let Some(guard) = remote_sequence_guard_mut(guard) else {
        return true;
    };
    let kind = c_to_string(kind).unwrap_or_default();
    let session_id = c_to_string(session_id).unwrap_or_default();
    guard.accept(
        &kind,
        Some(session_id.as_str()).filter(|value| !value.trim().is_empty()),
        optional_sequence(sequence),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_sequence_guard_reset(guard: *mut RemoteSequenceGuard) {
    if let Some(guard) = remote_sequence_guard_mut(guard) {
        guard.reset();
    }
}
