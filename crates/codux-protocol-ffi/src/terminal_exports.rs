use crate::common::{
    FfiRemotePtySession, c_to_string, clean_ffi_string, optional_sequence, optional_usize,
    remote_sequence_guard_mut, string_to_c, terminal_buffer_assembler_mut,
    terminal_output_sequencer_mut, terminal_output_sequencer_ref, terminal_screen_mut,
    terminal_screen_ref, terminal_session_mut, terminal_session_ref,
};
use codux_terminal_core::{
    HeadlessTerminalScreen, RemotePtySession, RemoteSequenceGuard, TerminalBufferAssembler,
    TerminalInputMode, TerminalKeyInput, TerminalKeyInputModifiers, TerminalMouseAction,
    TerminalMouseButton, TerminalMouseInput, TerminalOutputSequencer, terminal_insert_input_bytes,
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
pub extern "C" fn codux_terminal_session_new(
    session_id: *const c_char,
    max_cached_chars: i64,
) -> *mut FfiRemotePtySession {
    let Some(session_id) = c_to_string(session_id) else {
        return ptr::null_mut();
    };
    let max_cached_chars = usize::try_from(max_cached_chars.max(0)).unwrap_or(0);
    Box::into_raw(Box::new(RemotePtySession::new(
        session_id,
        max_cached_chars,
    )))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_free(session: *mut FfiRemotePtySession) {
    if session.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(session));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_snapshot_json(
    session: *const FfiRemotePtySession,
) -> *mut c_char {
    let Some(session) = terminal_session_ref(session) else {
        return ptr::null_mut();
    };
    let snapshot = session.snapshot();
    string_to_c(
        json!({
            "sessionId": snapshot.session_id,
            "content": snapshot.content,
            "bufferLength": snapshot.buffer_length,
            "sequence": snapshot.sequence,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_screen_snapshot_json(
    session: *const FfiRemotePtySession,
) -> *mut c_char {
    let Some(session) = terminal_session_ref(session) else {
        return ptr::null_mut();
    };
    string_to_c(
        serde_json::to_string(&session.screen_snapshot()).unwrap_or_else(|_| "{}".to_string()),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_resize_screen(
    session: *mut FfiRemotePtySession,
    cols: i64,
    rows: i64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    let cols = usize::try_from(cols.max(1)).unwrap_or(80);
    let rows = usize::try_from(rows.max(1)).unwrap_or(24);
    session.resize_screen(cols, rows);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_scroll_screen_lines(
    session: *mut FfiRemotePtySession,
    lines: i64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    let lines = i32::try_from(lines).unwrap_or(if lines.is_negative() {
        i32::MIN
    } else {
        i32::MAX
    });
    session.scroll_screen_lines(lines);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_scroll_screen_pixels(
    session: *mut FfiRemotePtySession,
    pixels: f64,
    cell_height: f64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    session.scroll_screen_pixels(pixels, cell_height);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_settle_screen_pixel_scroll(
    session: *mut FfiRemotePtySession,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    session.settle_screen_pixel_scroll();
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_apply_host_scroll(
    session: *mut FfiRemotePtySession,
    screen_data: *const c_char,
    display_offset: i64,
    total_lines: i64,
    margin_rows: i64,
    margin_rows_below: i64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    let Some(screen_data) = c_to_string(screen_data) else {
        return;
    };
    session.apply_host_scroll_snapshot(
        &screen_data,
        usize::try_from(display_offset).unwrap_or(0),
        usize::try_from(total_lines).unwrap_or(0),
        usize::try_from(margin_rows).unwrap_or(0),
        usize::try_from(margin_rows_below).unwrap_or(0),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_scroll_screen_to_bottom(
    session: *mut FfiRemotePtySession,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    session.scroll_screen_to_bottom();
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_content(
    session: *const FfiRemotePtySession,
) -> *mut c_char {
    let Some(session) = terminal_session_ref(session) else {
        return ptr::null_mut();
    };
    string_to_c(session.content())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_buffer_length(session: *const FfiRemotePtySession) -> i64 {
    terminal_session_ref(session)
        .map(|session| i64::try_from(session.buffer_length()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_sequence(session: *const FfiRemotePtySession) -> i64 {
    terminal_session_ref(session)
        .map(|session| session.sequence())
        .unwrap_or(0)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_is_restoring_baseline(
    session: *const FfiRemotePtySession,
) -> bool {
    terminal_session_ref(session)
        .map(|session| session.is_restoring_baseline())
        .unwrap_or(false)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_require_baseline(session: *mut FfiRemotePtySession) {
    if let Some(session) = terminal_session_mut(session) {
        session.require_baseline();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_reset_transient(
    session: *mut FfiRemotePtySession,
    reset_sequence: bool,
) {
    if let Some(session) = terminal_session_mut(session) {
        session.reset_transient(reset_sequence);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_set_sequence(
    session: *mut FfiRemotePtySession,
    sequence: i64,
) {
    if let Some(session) = terminal_session_mut(session) {
        session.set_sequence(sequence);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_hold_live_token(
    session: *mut FfiRemotePtySession,
    sequence: i64,
    token: i64,
) -> bool {
    let Some(session) = terminal_session_mut(session) else {
        return false;
    };
    session.hold_live(optional_sequence(sequence), token)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_accept_baseline_page_json(
    session: *mut FfiRemotePtySession,
    data: *const c_char,
    offset: i64,
    buffer_length: i64,
    truncated: bool,
) -> *mut c_char {
    let Some(session) = terminal_session_mut(session) else {
        return ptr::null_mut();
    };
    let Some(data) = c_to_string(data) else {
        return ptr::null_mut();
    };
    let offset = usize::try_from(offset.max(0)).unwrap_or(0);
    let buffer_length = optional_usize(buffer_length);
    let page = session.accept_baseline_page(&data, offset, buffer_length, truncated);
    string_to_c(
        json!({
            "accepted": page.accepted,
            "duplicate": page.duplicate,
            "ready": page.ready,
            "data": page.data,
            "nextOffset": page.next_offset,
            "progress": page.progress,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_replace_from_baseline(
    session: *mut FfiRemotePtySession,
    content: *const c_char,
    buffer_length: i64,
    sequence: i64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    let Some(content) = c_to_string(content) else {
        return;
    };
    session.replace_from_baseline(
        &content,
        optional_usize(buffer_length),
        optional_sequence(sequence),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_replace_from_baseline_json(
    session: *mut FfiRemotePtySession,
    content: *const c_char,
    buffer_length: i64,
    sequence: i64,
) -> *mut c_char {
    let Some(session) = terminal_session_mut(session) else {
        return ptr::null_mut();
    };
    let Some(content) = c_to_string(content) else {
        return ptr::null_mut();
    };
    let replay_tokens = session.replace_from_baseline(
        &content,
        optional_usize(buffer_length),
        optional_sequence(sequence),
    );
    string_to_c(
        json!({
            "replayTokens": replay_tokens,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_replace_from_baseline_screen_json(
    session: *mut FfiRemotePtySession,
    content: *const c_char,
    screen_data: *const c_char,
    buffer_length: i64,
    sequence: i64,
) -> *mut c_char {
    let Some(session) = terminal_session_mut(session) else {
        return ptr::null_mut();
    };
    let Some(content) = c_to_string(content) else {
        return ptr::null_mut();
    };
    let screen_data = c_to_string(screen_data);
    let replay_tokens = session.replace_from_baseline_screen(
        &content,
        screen_data.as_deref().filter(|value| !value.is_empty()),
        optional_usize(buffer_length),
        optional_sequence(sequence),
    );
    string_to_c(
        json!({
            "replayTokens": replay_tokens,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_append_live(
    session: *mut FfiRemotePtySession,
    data: *const c_char,
    buffer_length: i64,
    sequence: i64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    let Some(data) = c_to_string(data) else {
        return;
    };
    session.append_live(
        &data,
        optional_usize(buffer_length),
        optional_sequence(sequence),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_append_live_screen(
    session: *mut FfiRemotePtySession,
    data: *const c_char,
    screen_data: *const c_char,
    buffer_length: i64,
    sequence: i64,
) {
    let Some(session) = terminal_session_mut(session) else {
        return;
    };
    let Some(data) = c_to_string(data) else {
        return;
    };
    let screen_data = c_to_string(screen_data);
    session.append_live_screen(
        &data,
        screen_data.as_deref().filter(|value| !value.is_empty()),
        optional_usize(buffer_length),
        optional_sequence(sequence),
    );
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_session_clear(session: *mut FfiRemotePtySession) {
    if let Some(session) = terminal_session_mut(session) {
        session.clear();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_screen_new(
    cols: i64,
    rows: i64,
    scrollback: i64,
) -> *mut HeadlessTerminalScreen {
    let cols = usize::try_from(cols.max(1)).unwrap_or(80);
    let rows = usize::try_from(rows.max(1)).unwrap_or(24);
    let scrollback = usize::try_from(scrollback.max(0)).unwrap_or(0);
    Box::into_raw(Box::new(HeadlessTerminalScreen::new(
        cols, rows, scrollback,
    )))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_screen_free(screen: *mut HeadlessTerminalScreen) {
    if screen.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(screen));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_screen_process(
    screen: *mut HeadlessTerminalScreen,
    data: *const c_char,
) {
    let Some(screen) = terminal_screen_mut(screen) else {
        return;
    };
    let Some(data) = c_to_string(data) else {
        return;
    };
    screen.process(data.as_bytes());
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_screen_resize(
    screen: *mut HeadlessTerminalScreen,
    cols: i64,
    rows: i64,
) {
    let Some(screen) = terminal_screen_mut(screen) else {
        return;
    };
    let cols = usize::try_from(cols.max(1)).unwrap_or(80);
    let rows = usize::try_from(rows.max(1)).unwrap_or(24);
    screen.resize(cols, rows);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_screen_clear(screen: *mut HeadlessTerminalScreen) {
    if let Some(screen) = terminal_screen_mut(screen) {
        screen.clear();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_screen_snapshot_json(
    screen: *const HeadlessTerminalScreen,
) -> *mut c_char {
    let Some(screen) = terminal_screen_ref(screen) else {
        return ptr::null_mut();
    };
    string_to_c(serde_json::to_string(&screen.snapshot()).unwrap_or_else(|_| "{}".to_string()))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_output_sequencer_new() -> *mut TerminalOutputSequencer {
    Box::into_raw(Box::new(TerminalOutputSequencer::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_output_sequencer_free(sequencer: *mut TerminalOutputSequencer) {
    if sequencer.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(sequencer));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_output_sequencer_sequence_for(
    sequencer: *const TerminalOutputSequencer,
    session_id: *const c_char,
) -> i64 {
    let Some(sequencer) = terminal_output_sequencer_ref(sequencer) else {
        return 0;
    };
    let Some(session_id) = c_to_string(session_id) else {
        return 0;
    };
    sequencer.sequence_for(&session_id)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_output_sequencer_observe_json(
    sequencer: *mut TerminalOutputSequencer,
    session_id: *const c_char,
    is_buffer: bool,
    output_seq: i64,
    offset: i64,
    resets_sequence: bool,
) -> *mut c_char {
    let Some(sequencer) = terminal_output_sequencer_mut(sequencer) else {
        return ptr::null_mut();
    };
    let Some(session_id) = c_to_string(session_id) else {
        return ptr::null_mut();
    };
    let result = sequencer.observe(
        &session_id,
        is_buffer,
        optional_sequence(output_seq),
        optional_usize(offset),
        resets_sequence,
    );
    string_to_c(
        json!({
            "action": result.action.as_str(),
            "previousSeq": result.previous_seq,
            "shouldRender": result.should_render(),
            "gap": result.gap,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_output_sequencer_remove(
    sequencer: *mut TerminalOutputSequencer,
    session_id: *const c_char,
) {
    let Some(sequencer) = terminal_output_sequencer_mut(sequencer) else {
        return;
    };
    let Some(session_id) = c_to_string(session_id) else {
        return;
    };
    sequencer.remove(&session_id);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_output_sequencer_reset(sequencer: *mut TerminalOutputSequencer) {
    if let Some(sequencer) = terminal_output_sequencer_mut(sequencer) {
        sequencer.reset();
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_buffer_assembler_new(
    max_chars: i64,
) -> *mut TerminalBufferAssembler {
    Box::into_raw(Box::new(TerminalBufferAssembler::new(
        usize::try_from(max_chars.max(0)).unwrap_or(0),
    )))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_buffer_assembler_free(assembler: *mut TerminalBufferAssembler) {
    if assembler.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(assembler));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_buffer_assembler_accept_json(
    assembler: *mut TerminalBufferAssembler,
    session_id: *const c_char,
    payload_json: *const c_char,
) -> *mut c_char {
    let Some(assembler) = terminal_buffer_assembler_mut(assembler) else {
        return ptr::null_mut();
    };
    let Some(session_id) = c_to_string(session_id) else {
        return ptr::null_mut();
    };
    let Some(payload_json) = c_to_string(payload_json) else {
        return ptr::null_mut();
    };
    let payload = serde_json::from_str(&payload_json).unwrap_or(serde_json::Value::Null);
    let result = assembler.accept(&session_id, payload);
    string_to_c(
        json!({
            "ready": result.ready,
            "progress": result.progress,
            "payload": result.payload,
        })
        .to_string(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_buffer_assembler_remove(
    assembler: *mut TerminalBufferAssembler,
    session_id: *const c_char,
) {
    let Some(assembler) = terminal_buffer_assembler_mut(assembler) else {
        return;
    };
    let Some(session_id) = c_to_string(session_id) else {
        return;
    };
    assembler.remove(&session_id);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_terminal_buffer_assembler_reset(assembler: *mut TerminalBufferAssembler) {
    if let Some(assembler) = terminal_buffer_assembler_mut(assembler) {
        assembler.reset();
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
