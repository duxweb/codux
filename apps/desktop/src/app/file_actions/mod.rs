use super::*;

struct FileMutationResult {
    files: Vec<FileEntry>,
    file_tree_children: HashMap<String, Vec<FileEntry>>,
    expanded_dirs: HashSet<String>,
    selection: FileMutationSelection,
    preview: Option<(String, bool, bool)>,
    git: GitSummary,
    status: String,
    clear_draft: bool,
    saved_editor_path: Option<String>,
    saved_editor_content: Option<String>,
}

type FileMutationTree = (Vec<FileEntry>, HashMap<String, Vec<FileEntry>>);

#[derive(Default)]
enum FileMutationSelection {
    #[default]
    Keep,
    Clear,
    Single(String),
    Multiple(Vec<String>),
}

enum FileMoveConflictCheckResult {
    Clear,
    Conflicts(Vec<String>),
}

#[derive(Clone, Default, PartialEq, Eq)]
struct FileMutationSelectionState {
    selected_entry: Option<String>,
    selected_entries: HashSet<String>,
    selection_anchor: Option<String>,
}

#[derive(Clone, Default, PartialEq, Eq)]
struct FileMutationDraftState {
    kind: Option<FileNameDraftKind>,
    target: Option<String>,
    parent: Option<String>,
    value: String,
}

fn load_file_mutation_tree(
    runtime_service: &RuntimeService,
    project_path: &str,
    directory: Option<&str>,
    expanded_dirs: &[String],
) -> Result<FileMutationTree, String> {
    let files = runtime_service.try_reload_project_files(project_path, directory)?;
    let file_tree_children =
        load_file_mutation_children(runtime_service, project_path, expanded_dirs)?;
    Ok((files, file_tree_children))
}

fn load_file_mutation_children(
    runtime_service: &RuntimeService,
    project_path: &str,
    expanded_dirs: &[String],
) -> Result<HashMap<String, Vec<FileEntry>>, String> {
    expanded_dirs
        .iter()
        .cloned()
        .map(|directory_path| {
            let children =
                runtime_service.try_reload_project_files(project_path, Some(&directory_path))?;
            Ok((directory_path, children))
        })
        .collect::<Result<HashMap<_, _>, String>>()
}

fn file_mutation_text_preview(
    runtime_service: &RuntimeService,
    project_path: &str,
    path: &str,
    fallback: &str,
) -> (String, bool, bool) {
    if matches!(
        crate::app::file_editor::file_preview_kind_for_path(path),
        crate::app::file_editor::FilePreviewKind::Image
            | crate::app::file_editor::FilePreviewKind::External
    ) {
        return (fallback.to_string(), false, false);
    }
    runtime_service
        .read_project_file_edit_buffer(project_path, path)
        .map(|(content, editable)| (content, editable, false))
        .unwrap_or_else(|error| (format!("failed to load file: {error}"), false, false))
}

fn file_mutation_prune_expanded(expanded_dirs: &[String], removed_paths: &[String]) -> Vec<String> {
    expanded_dirs
        .iter()
        .filter(|expanded| {
            !removed_paths
                .iter()
                .any(|path| *expanded == path || expanded.starts_with(&format!("{path}/")))
        })
        .cloned()
        .collect()
}

fn file_mutation_refresh_error(action: &str, error: String) -> String {
    format!("{action}, but the first file tree refresh failed: {error}")
}

impl CoduxApp {
    fn spawn_file_mutation<F>(
        &mut self,
        pending_status: String,
        task_failed_status: &'static str,
        cx: &mut Context<Self>,
        task: F,
    ) where
        F: FnOnce() -> Result<FileMutationResult, String> + Send + 'static,
    {
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let started_selection = self.file_mutation_selection_state();
        let started_draft = self.file_mutation_draft_state();
        self.file_mutation_generation = self.file_mutation_generation.wrapping_add(1);
        let mutation_generation = self.file_mutation_generation;
        self.status_message = pending_status;
        self.invalidate_file_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                task,
            )
            .await
            .ok()
            .unwrap_or_else(|| Err(task_failed_status.to_string()));

            let _ = this.update_in(cx, |app, window, cx| {
                let current_key = super::app_state::current_worktree_scope_key(&app.state);
                if app.project_switch_generation != generation || current_key != scope_key {
                    app.invalidate_file_panel(cx);
                    return;
                }
                let mutation_is_current = app.file_mutation_generation == mutation_generation;
                app.apply_file_mutation_result(
                    result,
                    mutation_is_current,
                    started_selection,
                    started_draft,
                    window,
                    cx,
                );
            });
        })
        .detach();
    }

    fn file_mutation_selection_state(&self) -> FileMutationSelectionState {
        FileMutationSelectionState {
            selected_entry: self.selected_file_entry.clone(),
            selected_entries: self.selected_file_entries.clone(),
            selection_anchor: self.file_selection_anchor.clone(),
        }
    }

    fn file_mutation_draft_state(&self) -> FileMutationDraftState {
        FileMutationDraftState {
            kind: self.file_name_draft_kind,
            target: self.file_name_draft_target.clone(),
            parent: self.file_name_draft_parent.clone(),
            value: self.file_name_draft_value.clone(),
        }
    }

    pub(in crate::app) fn clear_file_name_draft(&mut self) {
        self.file_name_draft_kind = None;
        self.file_name_draft_target = None;
        self.file_name_draft_parent = None;
        self.file_name_draft_value.clear();
        self.file_name_draft_select_all = false;
    }

    fn spawn_file_mutation_failure_sync(&mut self, error: String, cx: &mut Context<Self>) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = error;
            self.invalidate_file_panel(cx);
            return;
        };
        let file_directory = self.file_directory.clone();
        let expanded_dirs = self
            .file_tree_expanded_dirs
            .iter()
            .cloned()
            .collect::<Vec<_>>();
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        let scope_key = super::app_state::current_worktree_scope_key(&self.state);
        let mutation_generation = self.file_mutation_generation;
        self.status_message = format!("{error}. Retrying file tree refresh…");
        self.invalidate_file_panel(cx);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                {
                    let project_path = project_path.clone();
                    let file_directory = file_directory.clone();
                    let expanded_dirs = expanded_dirs.clone();
                    move || {
                        let (files, file_tree_children) = load_file_mutation_tree(
                            &runtime_service,
                            &project_path,
                            file_directory_option(&file_directory),
                            &expanded_dirs,
                        )?;
                        let git = runtime_service.reload_project_git(&project_path);
                        Ok::<_, String>((
                            files,
                            file_tree_children,
                            expanded_dirs.into_iter().collect::<HashSet<_>>(),
                            git,
                        ))
                    }
                },
            )
            .await
            .ok()
            .unwrap_or_else(|| Err("file refresh retry task failed".to_string()));

            let _ = this.update(cx, |app, cx| {
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state) != scope_key
                    || app.file_mutation_generation != mutation_generation
                {
                    app.invalidate_file_panel(cx);
                    return;
                }
                match result {
                    Ok((files, file_tree_children, expanded_dirs, git)) => {
                        app.state.files = files;
                        app.file_tree_children = file_tree_children;
                        app.file_tree_expanded_dirs = expanded_dirs;
                        app.prune_missing_file_tree_directories();
                        app.normalize_selected_file_entry();
                        app.state.git = git;
                        app.normalize_selected_git_file();
                        app.normalize_selected_git_branch();
                        app.status_message = format!("{error}. File tree refreshed.");
                        app.remember_current_file_panel_state();
                    }
                    Err(refresh_error) => {
                        app.status_message = format!(
                            "{error}. Refresh retry failed: {refresh_error}. Please refresh files manually."
                        );
                    }
                }
                app.invalidate_file_panel(cx);
            });
        })
        .detach();
    }

    fn apply_file_mutation_result(
        &mut self,
        result: Result<FileMutationResult, String>,
        mutation_is_current: bool,
        started_selection: FileMutationSelectionState,
        started_draft: FileMutationDraftState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !mutation_is_current {
            self.invalidate_file_panel(cx);
            return;
        }
        match result {
            Ok(result) => {
                self.state.files = result.files;
                self.file_tree_expanded_dirs = result.expanded_dirs;
                self.file_tree_children = result.file_tree_children;
                self.prune_missing_file_tree_directories();
                let selection_unchanged = self.file_mutation_selection_state() == started_selection;
                if selection_unchanged {
                    match result.selection {
                        FileMutationSelection::Keep => self.normalize_selected_file_entry(),
                        FileMutationSelection::Clear => self.clear_file_selection(),
                        FileMutationSelection::Single(path) => self.set_single_file_selection(path),
                        FileMutationSelection::Multiple(paths) => {
                            self.selected_file_entries = paths.iter().cloned().collect();
                            self.selected_file_entry = paths.last().cloned();
                            self.file_selection_anchor = self.selected_file_entry.clone();
                        }
                    }
                } else {
                    self.normalize_selected_file_entry();
                }
                let saved_editor_unchanged =
                    result.saved_editor_path.as_deref().is_none_or(|path| {
                        result.saved_editor_content.as_ref().is_none_or(|content| {
                            self.file_editor_states
                                .get(&self.file_editor_state_key(path))
                                .map(|state| state.read(cx).value().as_ref() == content)
                                .unwrap_or(self.file_preview == *content)
                        })
                    });
                if selection_unchanged
                    && saved_editor_unchanged
                    && let Some((preview, editable, dirty)) = result.preview
                {
                    self.file_preview = preview;
                    self.file_editable = editable;
                    self.file_dirty = dirty;
                }
                self.state.git = result.git;
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                if result.clear_draft && self.file_mutation_draft_state() == started_draft {
                    self.clear_file_name_draft();
                }
                if saved_editor_unchanged && let Some(path) = result.saved_editor_path.as_deref() {
                    self.mark_file_editor_dirty(path, false, window, cx);
                    self.normalize_file_search_index();
                    self.persist_file_editor_layout_async(cx);
                }
                self.status_message = result.status;
                self.remember_current_file_panel_state();
            }
            Err(error) => {
                self.spawn_file_mutation_failure_sync(error, cx);
                return;
            }
        }
        self.invalidate_file_panel(cx);
    }
}

mod clipboard;
mod create_rename;
mod move_delete;
mod panel;
mod save;
mod selection;
mod system;
