use super::*;

impl CoduxApp {
    pub(in crate::app) fn clear_git_review_derived_content(&mut self) {
        self.git_review_content = None;
        self.git_review_derived_rows = None;
    }

    pub(in crate::app) fn restore_git_review_derived_content(
        &mut self,
        content: GitReviewContentSummary,
    ) {
        self.git_review_content = Some(content);
        self.git_review_derived_rows = None;
    }

    pub(in crate::app) fn ensure_git_review_derived_rows(&mut self) {
        if self.git_review_derived_rows.is_some() {
            return;
        }
        let Some(content) = self.git_review_content.as_ref() else {
            return;
        };
        let original_content = if self.git_review.mode == "taskBranch" {
            content.base_content.as_deref().unwrap_or("")
        } else {
            content.head_content.as_str()
        };
        let current_content = if self.git_review.mode == "taskBranch" {
            content.head_content.as_str()
        } else {
            content.worktree_content.as_str()
        };
        self.git_review_derived_rows = Some(super::sidebars::build_git_review_derived_rows(
            original_content,
            current_content,
            &content.deleted_lines,
            &content.added_lines,
        ));
    }

    pub(in crate::app) fn set_git_review_derived_content(
        &mut self,
        content: GitReviewContentSummary,
    ) {
        self.git_review_content = Some(content);
        self.git_review_derived_rows = None;
        self.ensure_git_review_derived_rows();
    }

    pub(in crate::app) fn toggle_git_status_section(
        &mut self,
        section: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.git_expanded_sections.contains(section) {
            self.git_expanded_sections.remove(section);
        } else {
            self.git_expanded_sections.insert(section.to_string());
        }
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn toggle_git_status_dir(
        &mut self,
        section_id: String,
        directory_path: String,
        cx: &mut Context<Self>,
    ) {
        let tree_key = git_status_tree_key(&section_id, &directory_path);
        if self.git_expanded_dirs.contains(&tree_key) {
            self.git_expanded_dirs.remove(&tree_key);
            self.invalidate_git_panel(cx);
            return;
        }

        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git tree".to_string();
            self.invalidate_git_panel(cx);
            return;
        };

        if !self.git_tree_children.contains_key(&tree_key) {
            match self
                .runtime_service
                .read_project_git_path_status(&project.path, &directory_path)
            {
                Ok(files) => {
                    self.git_tree_children.insert(tree_key.clone(), files);
                }
                Err(error) => {
                    self.status_message = format!("failed to load Git tree: {error}");
                    self.invalidate_git_panel(cx);
                    return;
                }
            }
        }

        self.git_expanded_dirs.insert(tree_key);
        self.status_message = format!("git tree expanded: {directory_path}");
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn toggle_git_review_dir(
        &mut self,
        directory_path: String,
        cx: &mut Context<Self>,
    ) {
        let tree_key = git_status_tree_key("review", &directory_path);
        if self.git_expanded_dirs.contains(&tree_key) {
            self.git_expanded_dirs.remove(&tree_key);
            self.invalidate_ui(cx, [UiRegion::GitSidebar, UiRegion::WorkspaceBody]);
            return;
        }

        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git review tree".to_string();
            self.invalidate_ui(cx, [UiRegion::GitSidebar, UiRegion::WorkspaceBody]);
            return;
        };

        if !self.git_tree_children.contains_key(&tree_key) {
            match self
                .runtime_service
                .read_project_git_path_status(&project.path, &directory_path)
            {
                Ok(files) => {
                    self.merge_git_review_path_status_files(&files);
                    self.git_tree_children.insert(tree_key.clone(), files);
                }
                Err(error) => {
                    self.status_message = format!("failed to load Git review tree: {error}");
                    self.invalidate_ui(cx, [UiRegion::GitSidebar, UiRegion::WorkspaceBody]);
                    return;
                }
            }
        } else {
            let files = self
                .git_tree_children
                .get(&tree_key)
                .cloned()
                .unwrap_or_default();
            self.merge_git_review_path_status_files(&files);
        }
        self.git_expanded_dirs.insert(tree_key);
        self.invalidate_ui(cx, [UiRegion::GitSidebar, UiRegion::WorkspaceBody]);
    }

    fn merge_git_review_path_status_files(&mut self, files: &[GitFileStatus]) {
        let mut seen = self
            .git_review
            .files
            .iter()
            .map(|file| file.path.clone())
            .collect::<HashSet<_>>();
        for file in files {
            if file.path.trim().is_empty()
                || file.path.ends_with('/')
                || !seen.insert(file.path.clone())
            {
                continue;
            }
            let status = if file.index_status.trim() == "?" {
                "added"
            } else if !file.index_status.trim().is_empty() && file.index_status.trim() != "?" {
                "staged"
            } else if !file.worktree_status.trim().is_empty() {
                "modified"
            } else {
                continue;
            };
            self.git_review.files.push(GitReviewFile {
                path: file.path.clone(),
                status: status.to_string(),
                additions: 0,
                deletions: 0,
            });
        }
    }

    pub(in crate::app) fn select_git_file_only(
        &mut self,
        file_path: String,
        cx: &mut Context<Self>,
    ) {
        if !self
            .git_review
            .files
            .iter()
            .any(|file| file.path == file_path)
            && !self
                .state
                .git
                .changed_files
                .iter()
                .any(|file| file.path == file_path)
        {
            self.status_message = "Git file is no longer available".to_string();
            self.invalidate_git_panel(cx);
            return;
        }
        self.selected_git_files.clear();
        self.selected_git_files.insert(file_path.clone());
        self.selected_git_file = Some(file_path);
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn toggle_git_file_selection(
        &mut self,
        file_path: String,
        cx: &mut Context<Self>,
    ) {
        if !self
            .git_review
            .files
            .iter()
            .any(|file| file.path == file_path)
            && !self
                .state
                .git
                .changed_files
                .iter()
                .any(|file| file.path == file_path)
        {
            self.status_message = "Git file is no longer available".to_string();
            self.invalidate_git_panel(cx);
            return;
        }
        if !self.selected_git_files.insert(file_path.clone()) {
            self.selected_git_files.remove(&file_path);
        }
        if self.selected_git_files.is_empty() {
            self.selected_git_file = None;
            self.git_diff_preview = "select a changed file to preview its diff".to_string();
            self.clear_git_review_derived_content();
        } else {
            self.selected_git_file = Some(file_path);
        }
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn selected_git_action_paths(&self, fallback: &str) -> Vec<String> {
        if self.selected_git_files.contains(fallback) && !self.selected_git_files.is_empty() {
            self.selected_git_files.iter().cloned().collect()
        } else {
            vec![fallback.to_string()]
        }
    }

    pub(in crate::app) fn load_git_file_diff_async(
        &mut self,
        file_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for Git diff".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        let Some(scope_key) = super::app_state::current_worktree_scope_key(&self.state) else {
            return;
        };
        let base_branch = self.git_review.base_branch.clone();
        let runtime_service = self.runtime_service.clone();
        let generation = self.project_switch_generation;
        self.selected_git_file = Some(file_path.clone());
        self.selected_git_files.clear();
        self.selected_git_files.insert(file_path.clone());
        self.clear_git_review_derived_content();
        self.git_diff_preview = "loading diff...".to_string();
        self.invalidate_ui(cx, [UiRegion::GitSidebar, UiRegion::WorkspaceBody]);
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking_with_priority(
                codux_runtime::async_runtime::BLOCKING_PRIORITY_FOREGROUND + generation,
                move || {
                    let diff = runtime_service.read_project_git_review_diff(
                        &project_path,
                        &file_path,
                        base_branch.as_deref(),
                    );
                    let content = runtime_service.read_project_git_review_file_content(
                        &project_path,
                        &file_path,
                        base_branch.as_deref(),
                    );
                    (scope_key, generation, file_path, diff, content)
                },
            )
            .await
            .ok();
            let _ = this.update(cx, |app, cx| {
                let Some((scope_key, generation, file_path, diff, content)) = result else {
                    return;
                };
                if app.project_switch_generation != generation
                    || super::app_state::current_worktree_scope_key(&app.state).as_ref()
                        != Some(&scope_key)
                    || app.selected_git_file.as_deref() != Some(file_path.as_str())
                {
                    return;
                }
                match diff {
                    Ok(diff) => {
                        app.git_diff_preview = diff;
                        app.set_git_review_derived_content(content);
                        app.status_message = format!("diff loaded: {file_path}");
                    }
                    Err(error) => {
                        app.git_diff_preview = format!("failed to load diff: {error}");
                        app.clear_git_review_derived_content();
                        app.status_message = format!("failed to load diff: {error}");
                    }
                }
                app.invalidate_ui(cx, [UiRegion::GitSidebar, UiRegion::WorkspaceBody]);
            });
        })
        .detach();
    }

    pub(in crate::app) fn ensure_selected_git_review_file_loaded_async(
        &mut self,
        _cx: &mut Context<Self>,
    ) {
        if self
            .selected_git_file
            .as_deref()
            .is_some_and(|path| self.git_review.files.iter().any(|file| file.path == path))
            && self.git_review_content.is_some()
        {
            return;
        }
        if self.selected_git_file.as_deref().is_some_and(|path| {
            !self.git_review.files.iter().any(|file| file.path == path)
                && self
                    .git_review_content
                    .as_ref()
                    .map(|content| content.path.as_str())
                    != Some(path)
        }) {
            self.selected_git_file = None;
            self.selected_git_files.clear();
            self.clear_git_review_derived_content();
        }
    }

    pub(in crate::app) fn open_git_diff_window(
        &mut self,
        file_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git diff".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        if file_path.trim().is_empty() || file_path.ends_with('/') {
            self.status_message = "no Git file selected for diff window".to_string();
            self.invalidate_git_panel(cx);
            return;
        }

        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for Git diff".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        let selected_project_id = project.id.clone();
        let selected_project_name = project.name.clone();
        let selected_file = file_path.clone();
        let state = self.state.clone();
        let runtime = self.runtime.clone();
        let runtime_service = self.runtime_service.clone();
        let bounds = Bounds::centered(None, size(px(920.0), px(680.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_child_titlebar(format!(
                    "Diff - {selected_file}"
                ))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.0), px(520.0))),
                ..Default::default()
            },
            move |window, cx| {
                macos_window::configure_document_child_window_controls(window);
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::GitDiff;
                app.git_diff_window_path = Some(selected_file.clone());
                app.git_diff_window_content = "Loading diff...".to_string();
                app.git_diff_window_error = None;
                app.clear_git_review_derived_content();
                app.state.selected_project = Some(ProjectInfo {
                    id: selected_project_id.clone(),
                    name: selected_project_name.clone(),
                    path: project_path.clone(),
                    exists: true,
                    badge: String::new(),
                    badge_symbol: None,
                    badge_color_hex: None,
                    git_default_push_remote_name: None,
                    environment_variables: Default::default(),
                    runtime_target: Default::default(),
                });
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                let service = view.read(cx).runtime_service.clone();
                let base_branch = view.read(cx).git_review.base_branch.clone();
                let diff_project_path = project_path.clone();
                let diff_selected_file = selected_file.clone();
                view.update(cx, |_app, cx| {
                    cx.spawn(async move |this: gpui::WeakEntity<CoduxApp>, cx| {
                        let result = codux_runtime::async_runtime::spawn_blocking(move || {
                            let diff = service.read_project_git_review_diff(
                                &diff_project_path,
                                &diff_selected_file,
                                base_branch.as_deref(),
                            )?;
                            let content = service.read_project_git_review_file_content(
                                &diff_project_path,
                                &diff_selected_file,
                                base_branch.as_deref(),
                            );
                            Ok::<_, String>((diff, content))
                        })
                        .await
                        .map_err(|error| error.to_string())
                        .and_then(|result| result);
                        let _ = this.update(cx, |app, cx| {
                            match result {
                                Ok((diff, content)) => {
                                    app.git_diff_window_content = diff;
                                    app.git_diff_window_error = None;
                                    app.set_git_review_derived_content(content);
                                }
                                Err(error) => {
                                    app.git_diff_window_content.clear();
                                    app.git_diff_window_error = Some(error);
                                    app.clear_git_review_derived_content();
                                }
                            }
                            app.invalidate_git_panel(cx);
                        });
                    })
                    .detach();
                    _app.invalidate_git_panel(cx);
                });
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(handle) => {
                self.register_child_window_handle(handle.into());
                format!("Git diff window opened: {file_path}")
            }
            Err(error) => format!("failed to open Git diff window: {error}"),
        };
        self.invalidate_git_panel(cx);
    }
}
