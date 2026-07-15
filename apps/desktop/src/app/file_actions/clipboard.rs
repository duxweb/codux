use super::*;

impl CoduxApp {
    pub(in crate::app) fn selected_file_entry_paths(&self) -> Vec<String> {
        let mut paths = if self.selected_file_entries.is_empty() {
            self.selected_file_entry
                .clone()
                .into_iter()
                .collect::<Vec<_>>()
        } else {
            self.selected_file_entries
                .iter()
                .cloned()
                .collect::<Vec<_>>()
        };
        paths.sort();
        paths.dedup();
        paths
    }

    pub(in crate::app) fn copy_selected_file_paths_to_clipboard(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        };
        let paths = self.selected_file_entry_paths();
        if paths.is_empty() {
            self.status_message = "no selected file entry to copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        }
        let full_paths = paths
            .iter()
            .map(|path| codux_runtime::path::join_path(&project_path, path))
            .collect::<Vec<_>>();
        let mut entries = vec![gpui::ClipboardEntry::String(gpui::ClipboardString::new(
            full_paths.join("\n"),
        ))];
        if self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| project.runtime_target.is_local())
        {
            let external_paths = full_paths.iter().map(PathBuf::from).collect::<Vec<_>>();
            entries.insert(
                0,
                gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(external_paths.into())),
            );
        }
        cx.write_to_clipboard(ClipboardItem { entries });
        self.status_message = format!("copied {} file path{}", paths.len(), plural(paths.len()));
        self.invalidate_file_panel(cx);
        true
    }

    pub(in crate::app) fn copy_active_file_editor_path_to_clipboard(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(path) = self.active_file_editor_tab.clone() else {
            self.status_message = "no active file to copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        };
        self.copy_file_path_to_clipboard(path, cx)
    }

    pub(in crate::app) fn copy_file_path_to_clipboard(
        &mut self,
        path: String,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file copy".to_string();
            self.invalidate_file_panel(cx);
            return true;
        };
        let full_path = codux_runtime::path::join_path(&project_path, &path);
        let mut entries = vec![gpui::ClipboardEntry::String(gpui::ClipboardString::new(
            full_path.clone(),
        ))];
        if self
            .state
            .selected_project
            .as_ref()
            .is_some_and(|project| project.runtime_target.is_local())
        {
            entries.insert(
                0,
                gpui::ClipboardEntry::ExternalPaths(gpui::ExternalPaths(
                    vec![PathBuf::from(full_path)].into(),
                )),
            );
        }
        cx.write_to_clipboard(ClipboardItem { entries });
        self.status_message = format!("copied file path: {path}");
        self.invalidate_file_panel(cx);
        true
    }

    pub(in crate::app) fn paste_clipboard_file_entries(
        &mut self,
        payload: ClipboardFilePayload,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let target_directory = self
            .selected_file_entry()
            .map(|entry| {
                if matches!(entry.kind, FileKind::Directory) {
                    entry.relative_path
                } else {
                    parent_relative_directory(&entry.relative_path)
                }
            })
            .unwrap_or_else(|| self.file_directory.clone());
        self.paste_file_payload_into_directory(payload, target_directory, cx);
        true
    }

    pub(in crate::app) fn copy_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for file copy".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = "no selected file entry to copy".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "selected file entry is no longer available".to_string();
            self.normalize_selected_file_entry();
            self.invalidate_file_panel(cx);
            return;
        };
        let project_path = project.path.clone();
        let file_directory = self.file_directory.clone();
        let directory = file_directory_option(&file_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let was_file = matches!(entry.kind, FileKind::File);
        self.spawn_file_mutation(
            format!("copying file entry: {entry_path}"),
            "file copy task failed",
            cx,
            move || {
                let (files, copied_path) = runtime_service.copy_project_file_entry(
                    &project_path,
                    &entry_path,
                    directory.as_deref(),
                )?;
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &expanded_dirs)
                        .map_err(|error| file_mutation_refresh_error("file copied", error))?;
                let preview = if was_file {
                    file_mutation_text_preview(
                        &runtime_service,
                        &project_path,
                        &copied_path,
                        "file entry copied",
                    )
                } else {
                    ("directory copied".to_string(), false, false)
                };
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: expanded_dirs.into_iter().collect(),
                    selection: FileMutationSelection::Single(copied_path.clone()),
                    preview: Some(preview),
                    git,
                    status: format!("copied file entry: {copied_path}"),
                    clear_draft: false,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }

    pub(in crate::app) fn paste_external_file_entries(
        &mut self,
        payload: ClipboardFilePayload,
        target_entry: FileEntry,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let target_directory = if matches!(target_entry.kind, FileKind::Directory) {
            target_entry.relative_path.clone()
        } else {
            parent_relative_directory(&target_entry.relative_path)
        };
        self.paste_file_payload_into_directory(payload, target_directory, cx);
    }

    fn paste_file_payload_into_directory(
        &mut self,
        payload: ClipboardFilePayload,
        target_directory: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for file paste".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if payload.paths.is_empty() && payload.images.is_empty() {
            self.status_message = "clipboard has no files or images to paste".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        let directory = file_directory_option(&target_directory).map(str::to_string);
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        self.spawn_file_mutation(
            "pasting clipboard files".to_string(),
            "file paste task failed",
            cx,
            move || {
                let paths = payload
                    .paths
                    .into_iter()
                    .filter(|path| Path::new(path).exists())
                    .collect::<Vec<_>>();
                if paths.is_empty() && payload.images.is_empty() {
                    return Err("clipboard has no files or images to paste".to_string());
                }
                let mut selected = None;
                let mut files = None;
                if !paths.is_empty() {
                    let (next_files, pasted) = runtime_service.import_external_project_files(
                        &project_path,
                        paths,
                        directory.as_deref(),
                    )?;
                    files = Some(next_files);
                    selected = pasted;
                }
                let mut pasted_image_count = 0usize;
                for image in payload.images {
                    let (next_files, path) = runtime_service.write_project_file_bytes(
                        &project_path,
                        directory.as_deref(),
                        &image.file_name,
                        image.bytes,
                    )?;
                    files = Some(next_files);
                    selected = Some(path);
                    pasted_image_count += 1;
                }
                let files = files.unwrap_or_default();
                let file_tree_children =
                    load_file_mutation_children(&runtime_service, &project_path, &expanded_dirs)
                        .map_err(|error| file_mutation_refresh_error("clipboard pasted", error))?;
                let preview = selected.as_ref().map(|path| {
                    file_mutation_text_preview(
                        &runtime_service,
                        &project_path,
                        path,
                        "clipboard file pasted",
                    )
                });
                let git = runtime_service.reload_project_git(&project_path);
                Ok(FileMutationResult {
                    files,
                    file_tree_children,
                    expanded_dirs: expanded_dirs.into_iter().collect(),
                    selection: selected
                        .clone()
                        .map(FileMutationSelection::Single)
                        .unwrap_or_default(),
                    preview,
                    git,
                    status: if pasted_image_count > 0 {
                        format!("clipboard image{} pasted", plural(pasted_image_count))
                    } else {
                        "clipboard file pasted".to_string()
                    },
                    clear_draft: false,
                    saved_editor_path: None,
                    saved_editor_content: None,
                })
            },
        );
    }
}
