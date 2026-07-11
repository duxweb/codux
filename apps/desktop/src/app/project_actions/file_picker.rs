use super::*;

pub(in crate::app) struct FilePickerOpenRequest {
    pub(in crate::app) mode: FilePickerMode,
    pub(in crate::app) target: FilePickerTarget,
    pub(in crate::app) device_id: Option<String>,
    pub(in crate::app) start_path: Option<String>,
    pub(in crate::app) default_filename: Option<String>,
}

impl CoduxApp {
    pub(in crate::app) fn open_file_picker_window(
        &mut self,
        request: FilePickerOpenRequest,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let FilePickerOpenRequest {
            mode,
            target,
            device_id,
            start_path,
            default_filename,
        } = request;
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title = translate(
            &locale,
            match mode {
                FilePickerMode::OpenFolder => "project.editor.browse.title",
                FilePickerMode::OpenFile => "file.picker.open.title",
                FilePickerMode::Save => "file.picker.save.title",
            },
            match mode {
                FilePickerMode::OpenFolder => "Choose Folder",
                FilePickerMode::OpenFile => "Open File",
                FilePickerMode::Save => "Save As",
            },
        );
        let parent = cx.entity().downgrade();
        let device_for_build = device_id.clone();
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::FilePicker,
                title: SharedString::from(title),
                size: size(px(740.0), px(560.0)),
                min_size: size(px(640.0), px(460.0)),
                already_open_message: "file picker already opened",
                opened_message: "file picker opened",
                failed_prefix: "failed to open file picker",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::FilePicker;
                app.file_picker_mode = mode;
                app.file_picker_target = target;
                app.file_picker_filename = default_filename.unwrap_or_default();
                app.file_picker_selected = None;
                app.file_picker_active_path = None;
                app.project_editor_host_device_id = device_for_build;
                app.project_editor_browse_path = String::new();
                app.project_editor_browse_parent = None;
                app.project_editor_browse_entries = Vec::new();
                app.project_editor_browse_new_folder = String::new();
                app.file_picker_new_folder_active = false;
                app.project_editor_browse_error = None;
                app.parent_main_window = Some(parent);
                app
            },
            move |view, window, cx| {
                let handle = window.window_handle();
                let device = device_id.clone();
                let start = start_path.clone();
                view.update(cx, |app, cx| {
                    app.load_project_editor_browse(device, start, handle, cx);
                });
            },
        );
    }

    pub(in crate::app) fn project_editor_browse_navigate(
        &mut self,
        path: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_picker_rename_draft = None;
        self.file_picker_active_path = None;
        let device_id = self.project_editor_host_device_id.clone();
        self.load_project_editor_browse(device_id, path, window.window_handle(), cx);
    }

    /// Click an entry: directories navigate, files are selected (file/save mode).
    pub(in crate::app) fn file_picker_choose_entry(
        &mut self,
        path: String,
        is_dir: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_picker_active_path = Some(path.clone());
        if is_dir {
            self.project_editor_browse_navigate(Some(path), window, cx);
            return;
        }
        // Selecting a file (Save mode prefills the filename from it).
        if self.file_picker_mode == FilePickerMode::Save
            && let Some(name) = std::path::Path::new(&path)
                .file_name()
                .and_then(|name| name.to_str())
        {
            self.file_picker_filename = name.to_string();
        }
        self.file_picker_selected = Some(path);
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn select_file_picker_context_entry(
        &mut self,
        entry: RemoteBrowseEntry,
        cx: &mut Context<Self>,
    ) {
        self.file_picker_active_path = Some(entry.path.clone());
        if !entry.is_dir {
            self.file_picker_selected = Some(entry.path.clone());
        }
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn set_file_picker_filename(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_picker_filename = value;
        self.invalidate_project_management(cx);
    }

    /// The path the picker would return for its current mode/selection, if valid.
    pub(in crate::app) fn file_picker_result_path(&self) -> Option<String> {
        match self.file_picker_mode {
            FilePickerMode::OpenFolder => {
                let path = self.project_editor_browse_path.trim();
                (!path.is_empty()).then(|| path.to_string())
            }
            FilePickerMode::OpenFile => self.file_picker_selected.clone(),
            FilePickerMode::Save => {
                let dir = self.project_editor_browse_path.trim();
                let name = self.file_picker_filename.trim();
                (!dir.is_empty() && !name.is_empty())
                    .then(|| codux_runtime::path::join_path(dir, name))
            }
        }
    }

    /// Confirm the picker: compute the result path for the mode, deliver it to
    /// the target on the opener window, and close the picker.
    pub(in crate::app) fn file_picker_select(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = self.file_picker_result_path() else {
            return;
        };
        let target = self.file_picker_target.clone();
        if let Some(parent) = self
            .parent_main_window
            .clone()
            .and_then(|parent| parent.upgrade())
        {
            let device = self.project_editor_host_device_id.clone();
            parent.update(cx, |opener, cx| {
                opener.apply_file_picker_result(target, device.clone(), path.clone(), cx);
            });
        }
        window.remove_window();
    }

    /// Deliver a picked path (and the device it was browsed on) to its target on
    /// the opener window. Add a match arm per new `FilePickerTarget`.
    pub(in crate::app) fn apply_file_picker_result(
        &mut self,
        target: FilePickerTarget,
        dest_device: Option<String>,
        path: String,
        cx: &mut Context<Self>,
    ) {
        match target {
            FilePickerTarget::ProjectEditorPath => {
                // The picker chose both the device and the directory.
                self.project_editor_host_device_id = dest_device;
                self.project_editor_path = path;
                self.invalidate_project_management(cx);
            }
            FilePickerTarget::SaveFileAs {
                source_path,
                device_id: source_device,
            } => {
                let runtime_service = self.runtime_service.clone();
                let dest = path;
                self.status_message = "saving a copy…".to_string();
                self.invalidate_status_bar(cx);
                cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
                    let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                        runtime_service.save_file_as(
                            source_device.as_deref(),
                            &source_path,
                            dest_device.as_deref(),
                            &dest,
                        )
                    })
                    .await
                    .unwrap_or_else(|error| Err(format!("failed to join save-as: {error}")));
                    let _ = this.update(cx, |app, cx| {
                        app.status_message = match result {
                            Ok(()) => "saved a copy".to_string(),
                            Err(error) => format!("save-as failed: {error}"),
                        };
                        app.invalidate_status_bar(cx);
                    });
                })
                .detach();
            }
            FilePickerTarget::SshPrivateKeyPath => {
                self.ssh_draft_private_key_path = path;
                self.clear_ssh_test_result();
                self.status_message = "SSH private key selected".to_string();
                self.sync_project_lifecycle_state(cx);
                self.invalidate_task_column(cx);
                self.invalidate_remote_panel(cx);
            }
        }
    }

    /// Switch the device being browsed in the file picker (left device sidebar):
    /// re-list from that device's root.
    pub(in crate::app) fn file_picker_switch_device(
        &mut self,
        device_id: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project_editor_browse_busy {
            return;
        }
        self.project_editor_host_device_id = device_id.clone();
        self.file_picker_selected = None;
        self.file_picker_active_path = None;
        self.file_picker_rename_draft = None;
        self.load_project_editor_browse(device_id, None, window.window_handle(), cx);
    }

    pub(in crate::app) fn set_project_editor_browse_new_folder(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_browse_new_folder = value;
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn start_file_picker_rename(
        &mut self,
        entry: RemoteBrowseEntry,
        cx: &mut Context<Self>,
    ) {
        self.file_picker_rename_draft = Some(FilePickerRenameDraft {
            path: entry.path,
            name: entry.name,
        });
        self.file_picker_new_folder_active = false;
        self.project_editor_browse_error = None;
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn set_file_picker_rename_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(draft) = &mut self.file_picker_rename_draft {
            draft.name = value;
        }
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn cancel_file_picker_rename(&mut self, cx: &mut Context<Self>) {
        self.file_picker_rename_draft = None;
        self.invalidate_project_management(cx);
    }

    fn clear_file_picker_rename_draft(&mut self) {
        self.file_picker_rename_draft = None;
    }

    pub(in crate::app) fn confirm_file_picker_rename(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project_editor_browse_busy {
            return;
        }
        let Some(draft) = self.file_picker_rename_draft.clone() else {
            return;
        };
        let name = draft.name.trim().to_string();
        self.clear_file_picker_rename_draft();
        if name.is_empty() || name.contains('/') || name.contains('\\') {
            self.project_editor_browse_error = Some(self.text(
                "file.picker.rename.invalid",
                "Enter a valid name without path separators.",
            ));
            self.invalidate_project_management(cx);
            return;
        }
        if name == file_picker_path_name(&draft.path) {
            self.invalidate_project_management(cx);
            return;
        }
        let new_path = file_picker_sibling_path(&draft.path, &name);
        let old_path = draft.path.clone();
        let selected_old_path = old_path.clone();
        let renamed_path = new_path.clone();
        let device_id = self.project_editor_host_device_id.clone();
        let reload_path = self.project_editor_browse_path.clone();
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.project_editor_browse_busy = true;
        self.project_editor_browse_error = None;
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                match device_id.as_deref() {
                    Some(device_id) => {
                        runtime_service.remote_rename_path(device_id, &old_path, &new_path)
                    }
                    None => runtime_service.rename_local_path(&old_path, &new_path),
                }
                .map(|_| (device_id, reload_path))
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join rename: {error}")));

            let _ = this.update(cx, |app, cx| {
                app.project_editor_browse_busy = false;
                match result {
                    Ok((device_id, reload_path)) => {
                        if app.file_picker_selected.as_deref() == Some(selected_old_path.as_str()) {
                            app.file_picker_selected = Some(renamed_path);
                        }
                        app.load_project_editor_browse(
                            device_id,
                            Some(reload_path),
                            window_handle,
                            cx,
                        );
                    }
                    Err(error) => {
                        app.project_editor_browse_error = Some(error);
                        app.invalidate_project_management(cx);
                    }
                }
            });
        })
        .detach();
    }

    /// Show the inline new-folder name editor in the file listing.
    pub(in crate::app) fn begin_file_picker_new_folder(&mut self, cx: &mut Context<Self>) {
        self.project_editor_browse_new_folder.clear();
        self.project_editor_browse_error = None;
        self.file_picker_rename_draft = None;
        self.file_picker_new_folder_active = true;
        self.invalidate_project_management(cx);
    }

    /// Dismiss the inline new-folder editor without creating anything.
    pub(in crate::app) fn cancel_file_picker_new_folder(&mut self, cx: &mut Context<Self>) {
        self.clear_file_picker_new_folder_draft();
        self.invalidate_project_management(cx);
    }

    fn clear_file_picker_new_folder_draft(&mut self) {
        self.file_picker_new_folder_active = false;
        self.project_editor_browse_new_folder.clear();
    }

    pub(in crate::app) fn handle_file_picker_key(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.window_mode != AppWindowMode::FilePicker {
            return false;
        }
        let keystroke = &event.keystroke;
        let unmodified = !keystroke.modifiers.platform
            && !keystroke.modifiers.control
            && !keystroke.modifiers.alt
            && !keystroke.modifiers.shift
            && !keystroke.modifiers.function;
        if !unmodified {
            return false;
        }
        let key = keystroke.key.as_str();
        if self.file_picker_rename_draft.is_some() || self.file_picker_new_folder_active {
            if matches!(key, "escape" | "Escape") {
                if self.file_picker_rename_draft.is_some() {
                    self.cancel_file_picker_rename(cx);
                } else {
                    self.cancel_file_picker_new_folder(cx);
                }
                return true;
            }
            return false;
        }
        if self.project_editor_browse_busy {
            return false;
        }
        if key.eq_ignore_ascii_case("up") || key.eq_ignore_ascii_case("arrowup") {
            self.move_file_picker_active(-1, cx);
            return true;
        }
        if key.eq_ignore_ascii_case("down") || key.eq_ignore_ascii_case("arrowdown") {
            self.move_file_picker_active(1, cx);
            return true;
        }
        if key.eq_ignore_ascii_case("enter") || key.eq_ignore_ascii_case("return") {
            self.open_file_picker_active_or_select(window, cx);
            return true;
        }
        if key.eq_ignore_ascii_case("escape") {
            window.remove_window();
            return true;
        }
        if key.eq_ignore_ascii_case("backspace") || key.eq_ignore_ascii_case("arrowleft") {
            if let Some(parent) = self.project_editor_browse_parent.clone() {
                self.project_editor_browse_navigate(Some(parent), window, cx);
            }
            return true;
        }
        false
    }

    fn file_picker_keyboard_entries(&self) -> Vec<RemoteBrowseEntry> {
        self.project_editor_browse_parent
            .clone()
            .map(|parent| RemoteBrowseEntry {
                name: "..".to_string(),
                path: parent,
                is_dir: true,
            })
            .into_iter()
            .chain(self.project_editor_browse_entries.iter().cloned())
            .collect()
    }

    fn move_file_picker_active(&mut self, delta: isize, cx: &mut Context<Self>) {
        let entries = self.file_picker_keyboard_entries();
        if entries.is_empty() {
            self.status_message = "no file picker items to select".to_string();
            self.invalidate_project_management(cx);
            return;
        }
        let next_index = match self
            .file_picker_active_path
            .as_ref()
            .and_then(|path| entries.iter().position(|entry| &entry.path == path))
        {
            Some(current_index) => current_index
                .saturating_add_signed(delta)
                .min(entries.len().saturating_sub(1)),
            None if delta < 0 => entries.len().saturating_sub(1),
            None => 0,
        };
        let entry = &entries[next_index];
        self.file_picker_active_path = Some(entry.path.clone());
        if !entry.is_dir {
            self.file_picker_selected = Some(entry.path.clone());
            if self.file_picker_mode == FilePickerMode::Save {
                self.file_picker_filename = entry.name.clone();
            }
        }
        self.invalidate_project_management(cx);
    }

    fn open_file_picker_active_or_select(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let entries = self.file_picker_keyboard_entries();
        let entry = self
            .file_picker_active_path
            .as_ref()
            .and_then(|path| entries.iter().find(|entry| &entry.path == path))
            .cloned();
        if let Some(entry) = entry {
            self.file_picker_choose_entry(entry.path, entry.is_dir, window, cx);
            return;
        }
        self.file_picker_select(window, cx);
    }

    pub(in crate::app) fn request_delete_file_picker_entry(
        &mut self,
        entry: RemoteBrowseEntry,
        cx: &mut Context<Self>,
    ) {
        if self.project_editor_browse_busy {
            return;
        }
        let title = self
            .text("file.picker.delete.confirm_format", "Delete \"%@\"?")
            .replace("%@", &entry.name);
        let message = self.text(
            "file.picker.delete.confirm.message",
            "Deleted items will be moved to Trash when possible.",
        );
        let confirm_label = self.text("common.delete", "Delete");
        let cancel_label = self.text("common.cancel", "Cancel");
        let runtime_service = self.runtime_service.clone();
        let device_id = self.project_editor_host_device_id.clone();
        let reload_path = self.project_editor_browse_path.clone();
        let entry_path = entry.path.clone();
        let window_handle = self.file_picker_window;
        self.project_editor_browse_busy = true;
        self.project_editor_browse_error = None;
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let confirmed = codux_runtime::async_runtime::spawn_blocking({
                let service = runtime_service.clone();
                move || {
                    service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                        title,
                        message,
                        confirm_label,
                        cancel_label,
                    })
                }
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to show delete confirmation: {error}")));

            let result = match confirmed {
                Ok(true) => codux_runtime::async_runtime::spawn_blocking(move || {
                    match device_id.as_deref() {
                        Some(device_id) => {
                            runtime_service.remote_delete_path(device_id, &entry_path)
                        }
                        None => runtime_service.delete_local_path(&entry_path),
                    }
                    .map(|_| (device_id, reload_path, entry_path))
                })
                .await
                .unwrap_or_else(|error| Err(format!("failed to join delete: {error}"))),
                Ok(false) => {
                    let _ = this.update(cx, |app, cx| {
                        app.project_editor_browse_busy = false;
                        app.invalidate_project_management(cx);
                    });
                    return;
                }
                Err(error) => Err(error),
            };

            let _ = this.update(cx, |app, cx| match result {
                Ok((device_id, reload_path, deleted_path)) => {
                    app.project_editor_browse_busy = false;
                    app.file_picker_rename_draft = None;
                    if app.file_picker_selected.as_deref() == Some(deleted_path.as_str()) {
                        app.file_picker_selected = None;
                    }
                    if let Some(handle) = window_handle {
                        app.load_project_editor_browse(device_id, Some(reload_path), handle, cx);
                    } else {
                        app.invalidate_project_management(cx);
                    }
                }
                Err(error) => {
                    app.project_editor_browse_busy = false;
                    app.project_editor_browse_error = Some(error);
                    app.invalidate_project_management(cx);
                }
            });
        })
        .detach();
    }

    pub(in crate::app) fn project_editor_browse_create_folder(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Guard against a fast double Enter creating the folder twice — the first
        // succeeds, the second then hits "already exists" while the listing is
        // still reloading.
        if self.project_editor_browse_busy {
            return;
        }
        let name = self.project_editor_browse_new_folder.trim().to_string();
        let device_id = self.project_editor_host_device_id.clone();
        self.clear_file_picker_new_folder_draft();
        if name.is_empty() || self.project_editor_browse_path.trim().is_empty() {
            self.invalidate_project_management(cx);
            return;
        }
        // Reload the directory the folder is created in using the *untrimmed*
        // browse path, so a Windows drive root stays `F:\` — trimming it to `F:`
        // makes the host re-list the drive's current dir, not its root, and the
        // new folder appears to vanish. `join_path` trims internally for `target`.
        let browse_path = self.project_editor_browse_path.trim().to_string();
        let target = codux_runtime::path::join_path(&browse_path, &name);
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.project_editor_browse_busy = true;
        self.project_editor_browse_error = None;
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            // `spawn_blocking` (unbounded pool), not `run_limited_blocking`: a
            // remote create may wait for the host to connect, and that wait must
            // not occupy the single-worker priority queue (which would freeze
            // every other blocking load — file tree, git — meanwhile).
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                match device_id.as_deref() {
                    Some(device_id) => runtime_service
                        .remote_create_directory(device_id, &target)
                        .map(|_| ()),
                    None => runtime_service.create_local_directory(&target),
                }
                .map(|_| (device_id, browse_path))
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join create directory: {error}")));

            // Update the entity directly (not via `window_handle.update`, whose
            // swallowed `Err` could otherwise leave `browse_busy` stuck true).
            let _ = this.update(cx, |app, cx| {
                app.project_editor_browse_busy = false;
                match result {
                    Ok((device_id, reload_path)) => {
                        app.load_project_editor_browse(
                            device_id,
                            Some(reload_path),
                            window_handle,
                            cx,
                        );
                    }
                    Err(error) => {
                        app.project_editor_browse_error = Some(error);
                        app.invalidate_project_management(cx);
                    }
                }
            });
        })
        .detach();
    }

    fn load_project_editor_browse(
        &mut self,
        device_id: Option<String>,
        path: Option<String>,
        // Retained for call-site symmetry; the completion updates the picker
        // entity directly (see below) rather than through a window handle.
        _window_handle: gpui::AnyWindowHandle,
        cx: &mut Context<Self>,
    ) {
        let runtime_service = self.runtime_service.clone();
        let path_for_call = path.clone();
        let expected_device_id = device_id.clone();
        self.project_editor_browse_generation =
            self.project_editor_browse_generation.wrapping_add(1);
        let browse_generation = self.project_editor_browse_generation;
        let purpose = match self.file_picker_target {
            FilePickerTarget::SshPrivateKeyPath => Some("sshKey"),
            _ => Some("projectFiles"),
        };
        self.project_editor_browse_busy = true;
        self.project_editor_browse_error = None;
        // Clear the previous device/dir's listing immediately, so switching to a
        // not-yet-ready remote host shows a loading/empty state instead of the
        // stale entries (and path) from the last device.
        self.project_editor_browse_path = String::new();
        self.project_editor_browse_parent = None;
        self.project_editor_browse_entries = Vec::new();
        self.file_picker_active_path = None;
        self.file_picker_rename_draft = None;
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            // `spawn_blocking` (unbounded pool), not `run_limited_blocking`: the
            // first browse of a remote host waits (bounded) for it to connect,
            // and that wait must not occupy the single-worker priority queue —
            // doing so would freeze every other blocking load until it returns.
            let result =
                codux_runtime::async_runtime::spawn_blocking(move || match device_id.as_deref() {
                    Some(device_id) => runtime_service.remote_browse_directory(
                        device_id,
                        path_for_call.as_deref(),
                        purpose,
                    ),
                    None => {
                        runtime_service.browse_local_directory(path_for_call.as_deref(), purpose)
                    }
                })
                .await
                .unwrap_or_else(|error| Err(format!("failed to join browse: {error}")));

            // Update the entity directly. The previous code nested this inside a
            // `window_handle.update(...)` whose `Err` was discarded; when that
            // update failed (window mid-update / not found) the `browse_busy`
            // reset never ran, leaving the picker's confirm button disabled
            // forever even though the listing had loaded.
            let _ = this.update(cx, |app, cx| {
                if app.project_editor_browse_generation != browse_generation
                    || app.project_editor_host_device_id != expected_device_id
                {
                    if app.project_editor_browse_generation == browse_generation {
                        app.project_editor_browse_busy = false;
                    }
                    app.invalidate_project_management(cx);
                    return;
                }
                app.project_editor_browse_busy = false;
                match result {
                    Ok(listing) => app.apply_project_editor_browse(listing),
                    Err(error) => app.project_editor_browse_error = Some(error),
                }
                app.invalidate_project_management(cx);
            });
        })
        .detach();
    }

    fn apply_project_editor_browse(
        &mut self,
        listing: codux_runtime::remote::RemoteDirectoryListing,
    ) {
        self.project_editor_browse_path = listing.path;
        self.project_editor_browse_parent = listing.parent;
        // Folder mode lists only directories; file/save modes list files too
        // (files are selectable, directories navigate).
        let folders_only = self.file_picker_mode == FilePickerMode::OpenFolder;
        self.project_editor_browse_entries = listing
            .entries
            .into_iter()
            .filter(|entry| !folders_only || entry.is_dir)
            .map(|entry| RemoteBrowseEntry {
                name: entry.name,
                path: entry.path,
                is_dir: entry.is_dir,
            })
            .collect();
        // Navigating to a new directory clears any prior file selection.
        self.file_picker_selected = None;
        self.file_picker_active_path = None;
        self.project_editor_browse_error = None;
    }

    pub(in crate::app) fn save_project_editor(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.project_editor_saving {
            return;
        }
        let name = self.project_editor_name.trim().to_string();
        let path = clean_dialog_path(&self.project_editor_path);
        if name.is_empty() || path.is_empty() {
            self.status_message = "project name and path are required".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        let project_id = self.project_editor_project_id.clone();
        let badge_symbol = self.project_editor_badge_symbol.clone();
        let badge_color_hex = self.project_editor_badge_color_hex.clone();
        let host_device_id = self.project_editor_host_device_id.clone();
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.project_editor_saving = true;
        self.status_message = if project_id.is_some() {
            format!("saving project: {name}")
        } else {
            format!("creating project: {name}")
        };
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                let save_result = if let Some(project_id) = project_id {
                    runtime_service.project_update(ProjectUpdateRequest {
                        project_id,
                        name: name.clone(),
                        path,
                        badge_text: project_badge_text_from_name(&name),
                        badge_symbol,
                        badge_color_hex: Some(badge_color_hex),
                        host_device_id,
                    })
                } else {
                    runtime_service.project_create(ProjectCreateRequest {
                        name: name.clone(),
                        path,
                        badge_text: project_badge_text_from_name(&name),
                        badge_symbol,
                        badge_color_hex: Some(badge_color_hex),
                        host_device_id,
                    })
                };
                save_result.map(|_| (runtime_service.reload_state(), name))
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join project save: {error}")));

            // Apply the result on the entity directly (not nested inside a
            // `window_handle.update`, whose swallowed Err would otherwise leave
            // `project_editor_saving` stuck true — making the Create/Save button
            // silently do nothing on the next click). Only the window removal,
            // which genuinely needs the window, stays on the window handle.
            let should_close = this
                .update(cx, |app, cx| {
                    app.project_editor_saving = false;
                    let close = match result {
                        Ok((state, name)) => {
                            let was_editing = app.project_editor_project_id.is_some();
                            app.state = state;
                            app.sync_project_list_state(cx);
                            app.status_message = if was_editing {
                                format!("project saved: {name}")
                            } else {
                                format!("project created: {name}")
                            };
                            publish_child_window_update(ChildWindowUpdateKind::Project);
                            true
                        }
                        Err(error) => {
                            app.status_message = if app.project_editor_project_id.is_some() {
                                format!("failed to save project: {error}")
                            } else {
                                format!("failed to create project: {error}")
                            };
                            false
                        }
                    };
                    app.invalidate_project_management(cx);
                    close
                })
                .unwrap_or(false);
            if should_close {
                let _ = window_handle.update(cx, |_root, window, _cx| window.remove_window());
            }
        })
        .detach();
    }
}

fn clean_dialog_path(path: &str) -> String {
    let path = path.trim();
    if path.is_empty() {
        return String::new();
    }
    if let Ok(url) = url::Url::parse(path)
        && url.scheme() == "file"
        && let Ok(file_path) = url.to_file_path()
    {
        return file_path.to_string_lossy().into_owned();
    }
    codux_runtime::path::display_path(path)
}

fn file_picker_path_name(path: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or(trimmed)
        .to_string()
}

fn file_picker_sibling_path(path: &str, name: &str) -> String {
    let trimmed = path.trim_end_matches(['/', '\\']);
    let parent = trimmed.rsplit_once(['/', '\\']).map(|(parent, _)| parent);
    match parent {
        Some(parent) if !parent.is_empty() => codux_runtime::path::join_path(parent, name),
        Some("") if path.starts_with('/') => format!("/{name}"),
        _ => name.to_string(),
    }
}

pub(in crate::app) fn merge_ai_history_summary(
    current: &mut AIHistorySummary,
    incoming: AIHistorySummary,
) -> bool {
    if ai_history_should_replace(current, &incoming) {
        *current = incoming;
        return true;
    }
    if !incoming.indexed {
        current.is_loading = incoming.is_loading;
        current.queued = incoming.queued;
        current.progress = incoming.progress;
        current.detail = incoming.detail;
        current.error = incoming.error;
        return true;
    }
    false
}
