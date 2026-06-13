use crate::common::{
    FfiRemoteRuntimeModel, c_to_string, clean_ffi_string, json_string_to_c,
    remote_runtime_model_mut, remote_runtime_model_ref, string_to_c,
};
use codux_terminal_core::{
    RemoteRuntimeProject, RemoteRuntimeTerminal, RemoteRuntimeWorktreeState,
};
use std::ffi::c_char;
use std::ptr;

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_new() -> *mut FfiRemoteRuntimeModel {
    Box::into_raw(Box::new(FfiRemoteRuntimeModel::new()))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_free(model: *mut FfiRemoteRuntimeModel) {
    if model.is_null() {
        return;
    }
    unsafe {
        drop(Box::from_raw(model));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_snapshot_json(
    model: *const FfiRemoteRuntimeModel,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_ref(model) else {
        return ptr::null_mut();
    };
    json_string_to_c(&model.snapshot())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_reset(
    model: *mut FfiRemoteRuntimeModel,
    keep_projects: bool,
) {
    if let Some(model) = remote_runtime_model_mut(model) {
        model.reset(keep_projects);
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_restore_cached_projects_json(
    model: *mut FfiRemoteRuntimeModel,
    projects_json: *const c_char,
) {
    let Some(model) = remote_runtime_model_mut(model) else {
        return;
    };
    let Some(projects_json) = c_to_string(projects_json) else {
        return;
    };
    let Ok(projects) = serde_json::from_str::<Vec<RemoteRuntimeProject>>(&projects_json) else {
        return;
    };
    model.restore_cached_projects(projects);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_apply_project_list_json(
    model: *mut FfiRemoteRuntimeModel,
    projects_json: *const c_char,
    remote_selected_project_id: *const c_char,
    remote_selected_worktree_id: *const c_char,
    terminal_visible: bool,
    terminal_list_loaded: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(projects_json) = c_to_string(projects_json) else {
        return ptr::null_mut();
    };
    let Ok(projects) = serde_json::from_str::<Vec<RemoteRuntimeProject>>(&projects_json) else {
        return ptr::null_mut();
    };
    let remote_selected_project_id = clean_ffi_string(c_to_string(remote_selected_project_id));
    let remote_selected_worktree_id = clean_ffi_string(c_to_string(remote_selected_worktree_id));
    let plan = model.apply_project_list(
        projects,
        remote_selected_project_id,
        remote_selected_worktree_id,
        terminal_visible,
        terminal_list_loaded,
    );
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_apply_terminal_list_json(
    model: *mut FfiRemoteRuntimeModel,
    terminals_json: *const c_char,
    terminal_visible: bool,
    terminal_list_loaded: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(terminals_json) = c_to_string(terminals_json) else {
        return ptr::null_mut();
    };
    let Ok(terminals) = serde_json::from_str::<Vec<RemoteRuntimeTerminal>>(&terminals_json) else {
        return ptr::null_mut();
    };
    let plan = model.apply_terminal_list(terminals, terminal_visible, terminal_list_loaded);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_user_select_project_json(
    model: *mut FfiRemoteRuntimeModel,
    project_json: *const c_char,
    terminal_visible: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(project_json) = c_to_string(project_json) else {
        return ptr::null_mut();
    };
    let Ok(project) = serde_json::from_str::<RemoteRuntimeProject>(&project_json) else {
        return ptr::null_mut();
    };
    let plan = model.user_select_project(project, terminal_visible);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_project_selected_json(
    model: *mut FfiRemoteRuntimeModel,
    project_id: *const c_char,
    worktree_id: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let plan = model.project_selected(c_to_string(project_id), c_to_string(worktree_id));
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_worktree_selected_json(
    model: *mut FfiRemoteRuntimeModel,
    project_id: *const c_char,
    worktree_id: *const c_char,
    terminal_visible: bool,
    terminal_list_loaded: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let plan = model.apply_worktree_selected(
        c_to_string(project_id),
        c_to_string(worktree_id),
        terminal_visible,
        terminal_list_loaded,
    );
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_apply_worktree_state_json(
    model: *mut FfiRemoteRuntimeModel,
    worktree_state_json: *const c_char,
    allow_runtime_selection: bool,
    terminal_visible: bool,
    terminal_list_loaded: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(worktree_state_json) = c_to_string(worktree_state_json) else {
        return ptr::null_mut();
    };
    let Ok(state) = serde_json::from_str::<RemoteRuntimeWorktreeState>(&worktree_state_json) else {
        return ptr::null_mut();
    };
    let plan = model.apply_worktree_state(
        state,
        allow_runtime_selection,
        terminal_visible,
        terminal_list_loaded,
    );
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_ensure_terminal_json(
    model: *mut FfiRemoteRuntimeModel,
    terminal_visible: bool,
    terminal_list_loaded: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let plan = model.ensure_terminal_for_selected_project(terminal_visible, terminal_list_loaded);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_select_terminal_json(
    model: *mut FfiRemoteRuntimeModel,
    terminal_json: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(terminal_json) = c_to_string(terminal_json) else {
        return ptr::null_mut();
    };
    let Ok(terminal) = serde_json::from_str::<RemoteRuntimeTerminal>(&terminal_json) else {
        return ptr::null_mut();
    };
    let plan = model.select_terminal(terminal);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_remove_terminal_json(
    model: *mut FfiRemoteRuntimeModel,
    terminal_id: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(terminal_id) = c_to_string(terminal_id) else {
        return ptr::null_mut();
    };
    let plan = model.remove_terminal(&terminal_id);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_apply_git_status_json(
    model: *mut FfiRemoteRuntimeModel,
    status_json: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(status_json) = c_to_string(status_json) else {
        return ptr::null_mut();
    };
    let Ok(status) = serde_json::from_str::<serde_json::Value>(&status_json) else {
        return ptr::null_mut();
    };
    let plan = model.apply_git_status(status);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_set_terminal_creating_project(
    model: *mut FfiRemoteRuntimeModel,
    project_id: *const c_char,
) {
    if let Some(model) = remote_runtime_model_mut(model) {
        model.set_terminal_creating_project(c_to_string(project_id));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_terminal_created_json(
    model: *mut FfiRemoteRuntimeModel,
    terminal_json: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_mut(model) else {
        return ptr::null_mut();
    };
    let Some(terminal_json) = c_to_string(terminal_json) else {
        return ptr::null_mut();
    };
    let Ok(terminal) = serde_json::from_str::<RemoteRuntimeTerminal>(&terminal_json) else {
        return ptr::null_mut();
    };
    let plan = model.terminal_created(terminal);
    json_string_to_c(&plan)
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_mark_project_select_sent(
    model: *mut FfiRemoteRuntimeModel,
    project_id: *const c_char,
) {
    let Some(model) = remote_runtime_model_mut(model) else {
        return;
    };
    let Some(project_id) = c_to_string(project_id) else {
        return;
    };
    model.mark_project_select_sent(&project_id);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_clear_project_select_sent(
    model: *mut FfiRemoteRuntimeModel,
    project_id: *const c_char,
) {
    let Some(model) = remote_runtime_model_mut(model) else {
        return;
    };
    let Some(project_id) = c_to_string(project_id) else {
        return;
    };
    model.clear_project_select_sent(&project_id);
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_pending_project_select(
    model: *const FfiRemoteRuntimeModel,
    include_sent: bool,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_ref(model) else {
        return ptr::null_mut();
    };
    string_to_c(
        model
            .pending_project_select(include_sent)
            .unwrap_or_default(),
    )
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_current_project_terminals_json(
    model: *const FfiRemoteRuntimeModel,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_ref(model) else {
        return ptr::null_mut();
    };
    json_string_to_c(&model.current_project_terminals())
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_terminal_scope_for_project_json(
    model: *const FfiRemoteRuntimeModel,
    project_id: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_ref(model) else {
        return ptr::null_mut();
    };
    let Some(project_id) = c_to_string(project_id) else {
        return ptr::null_mut();
    };
    json_string_to_c(&model.terminal_scope_for_project(&project_id))
}

#[unsafe(no_mangle)]
pub extern "C" fn codux_remote_runtime_model_terminal_scope_for_session_json(
    model: *const FfiRemoteRuntimeModel,
    session_id: *const c_char,
    terminal_json: *const c_char,
) -> *mut c_char {
    let Some(model) = remote_runtime_model_ref(model) else {
        return ptr::null_mut();
    };
    let Some(session_id) = c_to_string(session_id) else {
        return ptr::null_mut();
    };
    let terminal = c_to_string(terminal_json).and_then(|terminal_json| {
        if terminal_json.trim().is_empty() {
            None
        } else {
            serde_json::from_str::<RemoteRuntimeTerminal>(&terminal_json).ok()
        }
    });
    json_string_to_c(&model.terminal_scope_for_session(&session_id, terminal))
}
