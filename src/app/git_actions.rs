use super::*;

impl CoduxApp {
    pub(super) fn reload_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project to refresh".to_string();
            cx.notify();
            return;
        };
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        self.state.git = self.runtime_service.reload_project_git(&project_path);
        self.refresh_git_review_for_project(&project_path);
        self.git_expanded_dirs.clear();
        self.git_tree_children.clear();
        self.normalize_selected_git_file();
        self.normalize_selected_git_branch();
        self.status_message = format!("git status reloaded for {project_name}");
        self.runtime_trace(
            "git",
            &format!(
                "manual_reload project={} changed={} staged={} unstaged={} untracked={}",
                project_name,
                self.state.git.changed_files.len(),
                self.state.git.staged,
                self.state.git.unstaged,
                self.state.git.untracked
            ),
        );
        cx.notify();
    }

    pub(super) fn init_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git init".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let action = "init".to_string();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: false,
        });
        self.status_message = "Git init started".to_string();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.init_project_git(&worker_project_path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_repository_result(
                    project_id,
                    project_path,
                    action,
                    result,
                    false,
                    "Git repository initialized with git2".to_string(),
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn set_git_clone_remote_url(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_clone_remote_url = value;
        cx.notify();
    }

    pub(super) fn open_git_remote_editor(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.git_remote_editor_open = true;
        if self.git_remote_name.trim().is_empty() {
            self.git_remote_name = "origin".to_string();
        }
        self.status_message = "Git remote editor opened".to_string();
        cx.notify();
    }

    pub(super) fn close_git_remote_editor(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.git_remote_editor_open = false;
        self.status_message = "Git remote editor closed".to_string();
        cx.notify();
    }

    pub(super) fn set_git_remote_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_remote_name = value;
        cx.notify();
    }

    pub(super) fn set_git_remote_url(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_remote_url = value;
        cx.notify();
    }

    pub(super) fn set_git_commit_message(
        &mut self,
        value: String,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        if self.git_commit_message == value {
            return;
        }
        self.git_commit_message = value;
    }

    pub(super) fn generate_git_commit_message_with_ai(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message =
                "no selected project for Git commit message generation".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: "aiCommitMessage".to_string(),
            cancellable: false,
        });
        self.status_message = "AI commit message generation started".to_string();
        self.runtime_trace(
            "git",
            &format!("ai_commit_message start project={project_path}"),
        );
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.generate_project_git_commit_message(&worker_project_path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_generated_git_commit_message(project_id, project_path, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn apply_generated_git_commit_message(
        &mut self,
        project_id: String,
        project_path: String,
        result: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == "aiCommitMessage")
        {
            self.git_running_operation = None;
        }
        match result {
            Ok(message) => {
                let selected_matches =
                    self.state.selected_project.as_ref().is_some_and(|project| {
                        project.id == project_id && project.path == project_path
                    });
                self.runtime_trace(
                    "git",
                    &format!(
                        "ai_commit_message ok selected_matches={} chars={}",
                        selected_matches,
                        message.chars().count()
                    ),
                );
                if selected_matches {
                    self.git_commit_message = message.clone();
                    self.git_commit_message_revision =
                        self.git_commit_message_revision.saturating_add(1);
                    self.status_message = format!("AI commit message generated: {message}");
                } else {
                    self.status_message =
                        "AI commit message ignored because selected project changed".to_string();
                }
            }
            Err(error) => {
                self.runtime_trace("git", &format!("ai_commit_message failed error={error}"));
                self.status_message = format!("failed to generate commit message: {error}");
                self.show_git_commit_message_generation_error(error, cx);
            }
        }
        cx.notify();
    }

    pub(super) fn show_git_commit_message_generation_error(
        &self,
        error: String,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title = translate(
            &locale,
            "git.commit.generate_message",
            "Generate Commit Message",
        );
        let button_label = translate(&locale, "common.ok", "OK");
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
            let dialog_error = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_alert_dialog(LocalizedAlertDialogRequest {
                    title,
                    message: error,
                    button_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result)
            .err();

            if let Some(dialog_error) = dialog_error {
                let _ = this.update(cx, |app, cx| {
                    app.status_message =
                        format!("failed to show commit message alert: {dialog_error}");
                    cx.notify();
                });
            }
        })
        .detach();
    }

    pub(super) fn clone_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git clone".to_string();
            cx.notify();
            return;
        };
        let remote_url = self.git_clone_remote_url.trim().to_string();
        if remote_url.is_empty() {
            self.status_message = "Git clone failed: remote URL is empty".to_string();
            cx.notify();
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }

        let project_id = project.id.clone();
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        let action = "clone".to_string();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: false,
        });
        self.status_message = format!("Git clone started for {project_name}");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.clone_project_git(&worker_project_path, &remote_url)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_repository_result(
                    project_id,
                    project_path,
                    action,
                    result,
                    true,
                    format!("Git repository cloned for {project_name}"),
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn apply_project_git_repository_result(
        &mut self,
        project_id: String,
        project_path: String,
        action: String,
        result: Result<GitSummary, String>,
        refresh_files: bool,
        success_message: String,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == action)
        {
            self.git_running_operation = None;
        }
        match result {
            Ok(summary) => {
                let selected_matches = self
                    .state
                    .selected_project
                    .as_ref()
                    .is_some_and(|project| project.path == project_path);
                if selected_matches {
                    self.state.git = summary;
                    self.refresh_git_review_for_project(&project_path);
                    self.git_expanded_sections.insert("changed".to_string());
                    self.git_expanded_sections.insert("untracked".to_string());
                    self.git_expanded_dirs.clear();
                    self.git_tree_children.clear();
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    if refresh_files {
                        self.state.files = self.runtime_service.reload_project_files(
                            &project_path,
                            file_directory_option(&self.file_directory),
                        );
                        self.reset_file_tree_cache();
                        self.normalize_selected_file_entry();
                        self.git_clone_remote_url.clear();
                    }
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                    self.save_current_worktree_view_state();
                    self.notify_task_column(cx);
                }
                self.status_message = success_message;
            }
            Err(error) => {
                self.status_message = format!("Git {action} failed: {error}");
            }
        }
        cx.notify();
    }

    pub(super) fn toggle_git_status_section(
        &mut self,
        section: &'static str,
        cx: &mut Context<Self>,
    ) {
        if self.git_expanded_sections.contains(section) {
            self.git_expanded_sections.remove(section);
        } else {
            self.git_expanded_sections.insert(section.to_string());
        }
        cx.notify();
    }

    pub(super) fn toggle_git_status_dir(
        &mut self,
        section_id: String,
        directory_path: String,
        cx: &mut Context<Self>,
    ) {
        let tree_key = git_status_tree_key(&section_id, &directory_path);
        if self.git_expanded_dirs.contains(&tree_key) {
            self.git_expanded_dirs.remove(&tree_key);
            cx.notify();
            return;
        }

        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git tree".to_string();
            cx.notify();
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
                    cx.notify();
                    return;
                }
            }
        }

        self.git_expanded_dirs.insert(tree_key);
        self.status_message = format!("git tree expanded: {directory_path}");
        cx.notify();
    }

    pub(super) fn select_git_file(
        &mut self,
        file_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.selected_git_files.clear();
        self.selected_git_files.insert(file_path.clone());
        self.load_git_file_diff(file_path, cx);
    }

    pub(super) fn toggle_git_file_selection(&mut self, file_path: String, cx: &mut Context<Self>) {
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
            cx.notify();
            return;
        }
        if !self.selected_git_files.insert(file_path.clone()) {
            self.selected_git_files.remove(&file_path);
        }
        if self.selected_git_files.is_empty() {
            self.selected_git_file = None;
            self.git_diff_preview = "select a changed file to preview its diff".to_string();
            self.git_review_content = None;
        } else {
            self.load_git_file_diff(file_path, cx);
        }
        cx.notify();
    }

    pub(super) fn selected_git_action_paths(&self, fallback: &str) -> Vec<String> {
        if self.selected_git_files.contains(fallback) && !self.selected_git_files.is_empty() {
            self.selected_git_files.iter().cloned().collect()
        } else {
            vec![fallback.to_string()]
        }
    }

    pub(super) fn load_git_file_diff(&mut self, file_path: String, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git diff".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.read_project_git_review_diff(
            &project.path,
            &file_path,
            self.git_review.base_branch.as_deref(),
        ) {
            Ok(diff) => {
                let content = self.runtime_service.read_project_git_review_file_content(
                    &project.path,
                    &file_path,
                    self.git_review.base_branch.as_deref(),
                );
                self.selected_git_file = Some(file_path.clone());
                self.git_diff_preview = diff;
                self.git_review_content = Some(content);
                self.status_message = format!("diff loaded: {file_path}");
            }
            Err(error) => self.status_message = format!("failed to load diff: {error}"),
        }
        cx.notify();
    }

    pub(super) fn open_git_diff_window(
        &mut self,
        file_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git diff".to_string();
            cx.notify();
            return;
        };
        if file_path.trim().is_empty() || file_path.ends_with('/') {
            self.status_message = "no Git file selected for diff window".to_string();
            cx.notify();
            return;
        }

        let project_path = project.path.clone();
        let selected_project_id = project.id.clone();
        let selected_project_name = project.name.clone();
        let selected_file = file_path.clone();
        let bounds = Bounds::centered(None, size(px(920.0), px(680.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(theme::codux_titlebar(format!("Diff - {selected_file}"))),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(720.0), px(520.0))),
                ..Default::default()
            },
            move |window, cx| {
                let mut app = CoduxApp::new_settings_window();
                app.window_mode = AppWindowMode::GitDiff;
                app.git_diff_window_path = Some(selected_file.clone());
                match app.runtime_service.read_project_git_review_diff(
                    &project_path,
                    &selected_file,
                    app.git_review.base_branch.as_deref(),
                ) {
                    Ok(diff) => {
                        app.git_diff_window_content = diff;
                        app.git_diff_window_error = None;
                    }
                    Err(error) => {
                        app.git_diff_window_content.clear();
                        app.git_diff_window_error = Some(error);
                    }
                }
                app.state.selected_project = Some(ProjectInfo {
                    id: selected_project_id.clone(),
                    name: selected_project_name.clone(),
                    path: project_path.clone(),
                    exists: true,
                    badge: String::new(),
                    badge_symbol: None,
                    badge_color_hex: None,
                    git_default_push_remote_name: None,
                });
                theme::apply_component_theme(
                    &app.state.settings.theme,
                    &app.state.settings.theme_color,
                    Some(window),
                    cx,
                );
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(_) => format!("Git diff window opened: {file_path}"),
            Err(error) => format!("failed to open Git diff window: {error}"),
        };
        cx.notify();
    }

    pub(super) fn open_git_diff_window_file(&mut self, file_path: String, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for opening diff file".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .open_project_file_entry(&project.path, &file_path)
        {
            Ok(()) => self.status_message = format!("file open requested: {file_path}"),
            Err(error) => self.status_message = format!("failed to open diff file: {error}"),
        }
        cx.notify();
    }

    pub(super) fn normalize_selected_git_file(&mut self) {
        let selected_still_exists = self
            .selected_git_file
            .as_deref()
            .map(|path| {
                self.git_review.files.iter().any(|file| file.path == path)
                    || self
                        .state
                        .git
                        .changed_files
                        .iter()
                        .any(|file| file.path == path)
            })
            .unwrap_or(false);
        if !selected_still_exists {
            self.selected_git_file = None;
            self.selected_git_files.clear();
            self.git_diff_preview = "select a changed file to preview its diff".to_string();
            self.git_review_content = None;
        }
    }

    pub(super) fn refresh_git_review_for_project(&mut self, project_path: &str) {
        self.git_review = self
            .runtime_service
            .reload_project_git_review(project_path, self.git_review.base_branch.as_deref());
    }

    pub(super) fn normalize_selected_git_branch(&mut self) {
        let selected_still_exists = self
            .selected_git_branch
            .as_deref()
            .map(|name| {
                self.state
                    .git
                    .branches
                    .iter()
                    .any(|branch| branch.name == name)
            })
            .unwrap_or(false);
        if selected_still_exists {
            return;
        }
        self.selected_git_branch = self
            .state
            .git
            .branches
            .iter()
            .find(|branch| branch.is_current)
            .or_else(|| self.state.git.branches.first())
            .map(|branch| branch.name.clone());
    }

    pub(super) fn select_git_branch(
        &mut self,
        branch_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .git
            .branches
            .iter()
            .any(|branch| branch.name == branch_name)
        {
            self.selected_git_branch = Some(branch_name.clone());
            self.status_message = format!("selected Git branch: {branch_name}");
        } else {
            self.normalize_selected_git_branch();
            self.status_message = "Git branch is no longer available".to_string();
        }
        cx.notify();
    }

    pub(super) fn stage_selected_git_file(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.update_selected_git_file_stage(true, cx);
    }

    pub(super) fn unstage_selected_git_file(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_selected_git_file_stage(false, cx);
    }

    pub(super) fn stage_git_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_git_paths_stage(paths, true, cx);
    }

    pub(super) fn unstage_git_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.update_git_paths_stage(paths, false, cx);
    }

    pub(super) fn discard_selected_git_file(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git discard".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let Some(file_path) = self.selected_git_file.clone() else {
            self.status_message = "no selected Git file to discard".to_string();
            cx.notify();
            return;
        };
        let worker_file = file_path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("discard:{file_path}"),
                cancellable: false,
            },
            move |service, path| service.discard_project_git_file(&path, &worker_file),
            GitOperationCompletion {
                success_message: format!("discarded Git file: {file_path}"),
                failure_prefix: "failed to discard Git file".to_string(),
                clear_git_diff_preview: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn discard_git_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git discard".to_string();
            cx.notify();
            return;
        };
        let paths = normalized_git_action_paths(paths);
        if paths.is_empty() {
            self.status_message = "no Git files to discard".to_string();
            cx.notify();
            return;
        };
        let count = paths.len();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("discard-batch:{count}"),
                cancellable: false,
            },
            move |service, path| service.discard_project_git_paths(&path, &paths),
            GitOperationCompletion {
                success_message: format!("discarded {count} Git file paths"),
                failure_prefix: "failed to discard Git file paths".to_string(),
                clear_git_diff_preview: true,
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn append_project_gitignore_path(
        &mut self,
        file_path: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for .gitignore".to_string();
            cx.notify();
            return;
        };
        let normalized_path = file_path.trim().to_string();
        if normalized_path.is_empty() {
            self.status_message = "no Git path to ignore".to_string();
            cx.notify();
            return;
        }

        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_path = normalized_path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("ignore:{normalized_path}"),
                cancellable: false,
            },
            move |service, path| service.append_project_gitignore(&path, &[worker_path]),
            GitOperationCompletion {
                success_message: format!("added to .gitignore: {normalized_path}"),
                failure_prefix: "failed to update .gitignore".to_string(),
                clear_git_diff_preview: true,
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn append_project_gitignore_paths(
        &mut self,
        paths: Vec<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for .gitignore".to_string();
            cx.notify();
            return;
        };
        let paths = normalized_git_action_paths(paths);
        if paths.is_empty() {
            self.status_message = "no Git paths to ignore".to_string();
            cx.notify();
            return;
        }

        let count = paths.len();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("ignore-batch:{count}"),
                cancellable: false,
            },
            move |service, path| service.append_project_gitignore(&path, &paths),
            GitOperationCompletion {
                success_message: format!("added {count} Git paths to .gitignore"),
                failure_prefix: "failed to update .gitignore".to_string(),
                clear_git_diff_preview: true,
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn update_selected_git_file_stage(&mut self, stage: bool, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git file operation".to_string();
            cx.notify();
            return;
        };
        let project_path = project.path.clone();
        let Some(file_path) = self.selected_git_file.clone() else {
            self.status_message = "no selected Git file".to_string();
            cx.notify();
            return;
        };

        let worker_file = file_path.clone();
        self.start_project_git_operation(
            project.id.clone(),
            project_path,
            GitRunningOperation {
                label: format!("{}:{file_path}", if stage { "stage" } else { "unstage" }),
                cancellable: false,
            },
            move |service, path| {
                if stage {
                    service.stage_project_git_file(&path, &worker_file)
                } else {
                    service.unstage_project_git_file(&path, &worker_file)
                }
            },
            GitOperationCompletion {
                success_message: format!(
                    "{} Git file: {file_path}",
                    if stage { "staged" } else { "unstaged" }
                ),
                failure_prefix: format!(
                    "failed to {} Git file",
                    if stage { "stage" } else { "unstage" }
                ),
                diff_file_to_reload: Some(file_path),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn update_git_paths_stage(
        &mut self,
        paths: Vec<String>,
        stage: bool,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git file operation".to_string();
            cx.notify();
            return;
        };
        let paths = normalized_git_action_paths(paths);
        if paths.is_empty() {
            self.status_message = "no Git files selected".to_string();
            cx.notify();
            return;
        }

        let count = paths.len();
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let label = if stage { "stage" } else { "unstage" };
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("{label}-batch:{count}"),
                cancellable: false,
            },
            move |service, path| {
                if stage {
                    service.stage_project_git_paths(&path, &paths)
                } else {
                    service.unstage_project_git_paths(&path, &paths)
                }
            },
            GitOperationCompletion {
                success_message: format!(
                    "{} {count} Git file paths",
                    if stage { "staged" } else { "unstaged" }
                ),
                failure_prefix: format!(
                    "failed to {} Git file paths",
                    if stage { "stage" } else { "unstage" }
                ),
                clear_git_tree_cache: true,
                refresh_review: true,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn commit_staged_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_git_with_action("commit", cx);
    }

    pub(super) fn commit_and_push_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_git_with_action("commitAndPush", cx);
    }

    pub(super) fn commit_and_sync_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.commit_git_with_action("commitAndSync", cx);
    }

    pub(super) fn commit_git_with_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git commit".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let message = self
            .git_commit_message
            .trim()
            .to_string()
            .chars()
            .take(500)
            .collect::<String>();
        let message = if message.is_empty() {
            generated_git_commit_message(&self.state.git)
        } else {
            message
        };
        let action = action.to_string();
        let worker_action = action.clone();
        let worker_message = message.clone();
        let success_message = match action.as_str() {
            "commitAndPush" => format!("committed and pushed staged changes: {message}"),
            "commitAndSync" => format!("committed and synced staged changes: {message}"),
            _ => format!("committed staged changes: {message}"),
        };
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: action.clone(),
                cancellable: false,
            },
            move |service, path| match worker_action.as_str() {
                "commit" => service.commit_project_git(&path, &worker_message),
                "commitAndPush" | "commitAndSync" => {
                    service.commit_project_git_action(&path, &worker_message, &worker_action)
                }
                _ => Err(format!("unknown Git commit action: {worker_action}")),
            },
            GitOperationCompletion {
                success_message,
                failure_prefix: "failed to commit staged changes".to_string(),
                clear_commit_message: true,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn load_last_git_commit_message(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git commit message".to_string();
            cx.notify();
            return;
        };
        match self
            .runtime_service
            .read_project_git_last_commit_message(&project.path)
        {
            Ok(message) if !message.trim().is_empty() => {
                self.git_commit_message = message;
                self.git_commit_message_revision =
                    self.git_commit_message_revision.saturating_add(1);
                self.status_message = "loaded last Git commit message".to_string();
            }
            Ok(_) => {
                self.status_message = "last Git commit has no summary".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to load last Git commit message: {error}");
            }
        }
        cx.notify();
    }

    pub(super) fn amend_last_git_commit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git amend".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let draft_message = self
            .git_commit_message
            .trim()
            .to_string()
            .chars()
            .take(500)
            .collect::<String>();
        let message = if draft_message.is_empty() {
            match self
                .runtime_service
                .read_project_git_last_commit_message(&project_path)
            {
                Ok(message) if !message.trim().is_empty() => message,
                Ok(_) => {
                    self.status_message = "last Git commit has no summary".to_string();
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.status_message =
                        format!("failed to load last Git commit message: {error}");
                    cx.notify();
                    return;
                }
            }
        } else {
            draft_message
        };

        let worker_message = message.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: "amend".to_string(),
                cancellable: false,
            },
            move |service, path| service.amend_project_git_last_commit(&path, &worker_message),
            GitOperationCompletion {
                success_message: format!("amended last Git commit: {message}"),
                failure_prefix: "failed to amend last Git commit".to_string(),
                clear_commit_message: true,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn undo_last_git_commit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git undo".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: "undo".to_string(),
                cancellable: false,
            },
            |service, path| service.undo_project_git_last_commit(&path),
            GitOperationCompletion {
                success_message: "undid last Git commit".to_string(),
                failure_prefix: "failed to undo last Git commit".to_string(),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn fetch_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("fetch", cx);
    }

    pub(super) fn pull_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("pull", cx);
    }

    pub(super) fn push_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if let Some(remote_name) = self
            .state
            .selected_project
            .as_ref()
            .and_then(|project| project.git_default_push_remote_name.clone())
        {
            self.run_project_git_push_remote(&remote_name, cx);
            return;
        }
        self.run_project_git_remote_action("push", cx);
    }

    pub(super) fn sync_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        self.run_project_git_remote_action("sync", cx);
    }

    pub(super) fn force_push_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.state.selected_project.is_none() {
            self.status_message = "no selected project for Git force-push".to_string();
            cx.notify();
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }

        let title = self.text("git.remote.force_push", "Force Push");
        let message = self.text(
            "git.remote.force_push.message",
            "Overwrite the current remote branch. Only use this when you intentionally want to rewrite remote history.",
        );
        let confirm_label = self.text("git.remote.force_push", "Force Push");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for Git force-push confirmation".to_string();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(true) => app.run_project_git_remote_action("force-push", cx),
                Ok(false) => {
                    app.status_message = "Git force-push canceled".to_string();
                    cx.notify();
                }
                Err(error) => {
                    app.status_message = format!("failed to show force-push confirmation: {error}");
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn run_project_git_remote_action(&mut self, action: &str, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for Git {action}");
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let action = action.to_string();
        let worker_action = action.clone();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: true,
        });
        self.status_message = format!("Git {action} started");
        self.runtime_trace(
            "git",
            &format!("remote_action start action={action} project={project_path}"),
        );
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || match worker_action
                .as_str()
            {
                "fetch" => service.fetch_project_git(&worker_project_path),
                "pull" => service.pull_project_git(&worker_project_path),
                "push" => service.push_project_git(&worker_project_path),
                "sync" => service.sync_project_git(&worker_project_path),
                "force-push" => service.force_push_project_git(&worker_project_path),
                _ => Err(format!("unknown Git action: {worker_action}")),
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_remote_result(project_id, project_path, action, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn push_project_git_remote(
        &mut self,
        remote_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_project_git_push_remote(&remote_name, cx);
    }

    pub(super) fn run_project_git_push_remote(
        &mut self,
        remote_name: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git push".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let remote_name = remote_name.to_string();
        let action = format!("push:{remote_name}");
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: true,
        });
        self.status_message = format!("Git push to {remote_name} started");
        self.runtime_trace(
            "git",
            &format!(
                "remote_action start action={} project={project_path}",
                action
            ),
        );
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let worker_remote_name = remote_name.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.push_project_git_remote(&worker_project_path, &worker_remote_name)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_remote_result(project_id, project_path, action, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn cancel_project_git(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git cancel".to_string();
            cx.notify();
            return;
        };
        match self.runtime_service.cancel_project_git(&project.path) {
            Ok(()) => {
                self.status_message = "Git cancel requested".to_string();
            }
            Err(error) => {
                self.status_message = format!("Git cancel failed: {error}");
            }
        }
        cx.notify();
    }

    pub(super) fn apply_project_git_remote_result(
        &mut self,
        project_id: String,
        project_path: String,
        action: String,
        result: Result<GitSummary, String>,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == action)
        {
            self.git_running_operation = None;
        }
        match result {
            Ok(summary) => {
                let selected_matches = self
                    .state
                    .selected_project
                    .as_ref()
                    .is_some_and(|project| project.path == project_path);
                if selected_matches {
                    self.state.git = summary;
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                    self.save_current_worktree_view_state();
                    self.notify_task_column(cx);
                }
                self.status_message = format!("Git {} completed", git_remote_action_label(&action));
                self.runtime_trace(
                    "git",
                    &format!("remote_action ok action={action} project={project_path}"),
                );
            }
            Err(error) => {
                self.runtime_trace(
                    "git",
                    &format!(
                        "remote_action failed action={action} project={project_path} error={error}"
                    ),
                );
                self.status_message =
                    format!("Git {} failed: {error}", git_remote_action_label(&action));
            }
        }
        cx.notify();
    }

    pub(super) fn start_project_git_operation(
        &mut self,
        project_id: String,
        project_path: String,
        operation: GitRunningOperation,
        action: impl FnOnce(RuntimeService, String) -> Result<GitSummary, String> + Send + 'static,
        completion: GitOperationCompletion,
        cx: &mut Context<Self>,
    ) {
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }

        let operation_label = operation.label.clone();
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(operation);
        self.status_message = format!("Git {operation_label} started");
        self.runtime_trace(
            "git",
            &format!("operation start label={operation_label} project={project_path}"),
        );
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                action(service, worker_project_path)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_operation_result(
                    project_id,
                    project_path,
                    operation_label,
                    result,
                    completion,
                    cx,
                );
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn apply_project_git_operation_result(
        &mut self,
        project_id: String,
        project_path: String,
        operation_label: String,
        result: Result<GitSummary, String>,
        completion: GitOperationCompletion,
        cx: &mut Context<Self>,
    ) {
        if self
            .git_running_operation
            .as_ref()
            .is_some_and(|operation| operation.label == operation_label)
        {
            self.git_running_operation = None;
        }

        match result {
            Ok(summary) => {
                let selected_matches = self
                    .state
                    .selected_project
                    .as_ref()
                    .is_some_and(|project| project.path == project_path);
                if selected_matches {
                    if completion.reload_state {
                        self.state = self.runtime_service.reload_state();
                        self.sync_project_list_store(cx);
                    }
                    self.state.git = summary;
                    if completion.clear_commit_message {
                        self.git_commit_message.clear();
                        self.git_commit_message_revision =
                            self.git_commit_message_revision.saturating_add(1);
                    }
                    if completion.clear_remote_url {
                        self.git_remote_url.clear();
                        self.git_remote_editor_open = false;
                    }
                    if completion.clear_selected_branch {
                        self.selected_git_branch = None;
                    }
                    if let Some(branch) = completion.selected_branch.clone() {
                        self.selected_git_branch = Some(branch);
                    }
                    if completion.refresh_review {
                        self.refresh_git_review_for_project(&project_path);
                    }
                    if completion.clear_git_tree_cache {
                        self.git_expanded_dirs.clear();
                        self.git_tree_children.clear();
                    }
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    if completion.clear_git_diff_preview {
                        self.git_diff_preview =
                            "select a changed file to preview its diff".to_string();
                        self.git_review_content = None;
                    } else if let Some(file_path) = completion.diff_file_to_reload.as_deref()
                        && self.selected_git_file.is_some()
                    {
                        self.git_diff_preview = self
                            .runtime_service
                            .read_project_git_review_diff(
                                &project_path,
                                file_path,
                                self.git_review.base_branch.as_deref(),
                            )
                            .unwrap_or_else(|error| format!("failed to reload diff: {error}"));
                        self.git_review_content =
                            Some(self.runtime_service.read_project_git_review_file_content(
                                &project_path,
                                file_path,
                                self.git_review.base_branch.as_deref(),
                            ));
                    }
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                    self.save_current_worktree_view_state();
                    self.notify_task_column(cx);
                }
                self.status_message = completion.success_message;
                self.runtime_trace(
                    "git",
                    &format!("operation ok label={operation_label} project={project_path}"),
                );
            }
            Err(error) => {
                self.runtime_trace(
                    "git",
                    &format!(
                        "operation failed label={operation_label} project={project_path} error={error}"
                    ),
                );
                self.status_message = format!("{}: {error}", completion.failure_prefix);
            }
        }
        cx.notify();
    }

    pub(super) fn set_project_default_push_remote(
        &mut self,
        remote_name: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for default Git remote".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        match self.runtime_service.project_set_default_push_remote(
            ProjectDefaultPushRemoteRequest {
                project_id,
                remote_name: remote_name.clone(),
            },
        ) {
            Ok(_) => {
                self.state = self.runtime_service.reload_state();
                self.normalize_selected_git_file();
                self.normalize_selected_git_branch();
                self.normalize_selected_ai_session();
                self.normalize_selected_runtime_session();
                self.normalize_selected_ssh_profile();
                self.sync_project_list_store(cx);
                self.status_message = match remote_name {
                    Some(remote_name) => format!("default Git push remote saved: {remote_name}"),
                    None => "default Git push remote cleared".to_string(),
                };
            }
            Err(error) => {
                self.status_message = format!("failed to save default Git push remote: {error}");
            }
        }
        cx.notify();
    }

    pub(super) fn add_project_git_remote(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let remote_name = self.git_remote_name.trim().to_string();
        let remote_url = self.git_remote_url.trim().to_string();
        let worker_remote_name = remote_name.clone();
        let worker_remote_url = remote_url.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("add-remote:{remote_name}"),
                cancellable: false,
            },
            move |service, path| {
                service.add_project_git_remote(&path, &worker_remote_name, &worker_remote_url)
            },
            GitOperationCompletion {
                success_message: format!("Git remote added: {remote_name}"),
                failure_prefix: "failed to add Git remote".to_string(),
                clear_remote_url: true,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn remove_project_git_remote(
        &mut self,
        remote_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if remote_name.trim().is_empty() {
            self.status_message = "no Git remote selected to remove".to_string();
            cx.notify();
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }
        let title = self.text("git.remote.remove", "Remove Remote");
        let message = self
            .text("git.remote.remove.confirm_format", "Remove remote %@?")
            .replace("%@", &remote_name);
        let confirm_label = self.text("common.delete", "Delete");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for Git remote removal confirmation".to_string();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(true) => app.remove_project_git_remote_confirmed(remote_name, cx),
                Ok(false) => {
                    app.status_message = "Git remote removal canceled".to_string();
                    cx.notify();
                }
                Err(error) => {
                    app.status_message =
                        format!("failed to show remote removal confirmation: {error}");
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn remove_project_git_remote_confirmed(
        &mut self,
        remote_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let default_remote = project.git_default_push_remote_name.clone();
        let clears_default_remote = default_remote.as_deref() == Some(remote_name.as_str());
        let worker_project_id = project_id.clone();
        let worker_remote_name = remote_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("remove-remote:{remote_name}"),
                cancellable: false,
            },
            move |service, path| {
                let summary = service.remove_project_git_remote(&path, &worker_remote_name)?;
                if clears_default_remote {
                    let _ =
                        service.project_set_default_push_remote(ProjectDefaultPushRemoteRequest {
                            project_id: worker_project_id,
                            remote_name: None,
                        });
                }
                Ok(summary)
            },
            GitOperationCompletion {
                success_message: format!("Git remote removed: {remote_name}"),
                failure_prefix: "failed to remove Git remote".to_string(),
                reload_state: clears_default_remote,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn checkout_selected_git_branch(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git checkout".to_string();
            cx.notify();
            return;
        };
        let Some(branch_name) = self.selected_git_branch.clone() else {
            self.status_message = "no selected Git branch".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("checkout:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.checkout_project_git_branch(&path, &worker_branch),
            GitOperationCompletion {
                success_message: format!("checked out Git branch: {branch_name}"),
                failure_prefix: "Git checkout failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: Some(branch_name),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn checkout_git_remote_branch(
        &mut self,
        remote_branch: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote checkout".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = remote_branch.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("checkout-remote:{remote_branch}"),
                cancellable: false,
            },
            move |service, path| service.checkout_project_git_remote_branch(&path, &worker_branch),
            GitOperationCompletion {
                success_message: format!("checked out remote Git branch: {remote_branch}"),
                failure_prefix: "Git remote checkout failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: true,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn push_project_git_remote_branch(
        &mut self,
        remote_branch: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote branch push".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            cx.notify();
            return;
        }

        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let local_branch = self.state.git.branch.trim().to_string();
        let local_branch = if local_branch.is_empty() || local_branch == "HEAD" {
            None
        } else {
            Some(local_branch)
        };
        let action = format!("push-branch:{remote_branch}");
        let service = self.runtime_service.clone();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: true,
        });
        self.status_message = format!("Git push to {remote_branch} started");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let worker_remote_branch = remote_branch.clone();
            let worker_local_branch = local_branch.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.push_project_git_remote_branch(
                    &worker_project_path,
                    &worker_remote_branch,
                    worker_local_branch.as_deref(),
                )
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                app.apply_project_git_remote_result(project_id, project_path, action, result, cx);
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn checkout_git_commit(
        &mut self,
        commit: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_git_commit_history_action("checkout", &commit, cx);
    }

    pub(super) fn revert_git_commit(
        &mut self,
        commit: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_git_commit_history_action("revert", &commit, cx);
    }

    pub(super) fn restore_git_commit(
        &mut self,
        commit: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_git_commit_history_action("restore", &commit, cx);
    }

    pub(super) fn run_git_commit_history_action(
        &mut self,
        action: &str,
        commit: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for Git {action}");
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let action = action.to_string();
        let commit = commit.to_string();
        let worker_action = action.clone();
        let worker_commit = commit.clone();
        let success_message = match action.as_str() {
            "checkout" => format!("checked out Git commit: {commit}"),
            "revert" => format!("reverted Git commit: {commit}"),
            "restore" => format!("restored Git branch to commit: {commit}"),
            _ => format!("Git history action completed: {commit}"),
        };
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("{action}:{commit}"),
                cancellable: false,
            },
            move |service, path| match worker_action.as_str() {
                "checkout" => service.checkout_project_git_commit(&path, &worker_commit),
                "revert" => service.revert_project_git_commit(&path, &worker_commit),
                "restore" => service.restore_project_git_commit(&path, &worker_commit, false),
                _ => Err(format!("unknown Git history action: {worker_action}")),
            },
            GitOperationCompletion {
                success_message,
                failure_prefix: format!("Git {action} commit failed"),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: action == "checkout",
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn create_git_branch(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git branch creation".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let branch_name = generated_git_branch_name();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("create-branch:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.create_project_git_branch(&path, &worker_branch, true),
            GitOperationCompletion {
                success_message: format!("created and checked out Git branch: {branch_name}"),
                failure_prefix: "Git branch creation failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: Some(branch_name),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn create_git_branch_from(
        &mut self,
        from_branch: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git branch creation".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let branch_name = generated_git_branch_name();
        let worker_branch = branch_name.clone();
        let worker_from_branch = from_branch.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("create-branch:{branch_name}"),
                cancellable: false,
            },
            move |service, path| {
                service.create_project_git_branch_from(
                    &path,
                    &worker_branch,
                    Some(&worker_from_branch),
                    true,
                )
            },
            GitOperationCompletion {
                success_message: format!("created Git branch {branch_name} from {from_branch}"),
                failure_prefix: format!("Git branch creation from {from_branch} failed"),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: Some(branch_name),
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn merge_git_branch(
        &mut self,
        branch_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git merge".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("merge:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.merge_project_git_branch(&path, &worker_branch, false),
            GitOperationCompletion {
                success_message: format!("merged Git branch: {branch_name}"),
                failure_prefix: format!("Git merge {branch_name} failed"),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn squash_merge_git_branch(
        &mut self,
        branch_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git squash merge".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("squash-merge:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.merge_project_git_branch(&path, &worker_branch, true),
            GitOperationCompletion {
                success_message: format!("squash merged Git branch: {branch_name}"),
                failure_prefix: format!("Git squash merge {branch_name} failed"),
                clear_commit_message: false,
                refresh_review: true,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }

    pub(super) fn delete_selected_git_branch(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.selected_project.is_none() {
            self.status_message = "no selected project for Git branch deletion".to_string();
            cx.notify();
            return;
        };
        let Some(branch_name) = self.selected_git_branch.clone() else {
            self.status_message = "no selected Git branch to delete".to_string();
            cx.notify();
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            cx.notify();
            return;
        }

        let title = self.text("git.branch.delete_local", "Delete Local Branch");
        let message = self
            .text(
                "git.branch.delete.confirm_format",
                "Delete local branch %@?",
            )
            .replace("%@", &branch_name);
        let confirm_label = self.text("common.delete", "Delete");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for Git branch deletion confirmation".to_string();
        let timer = cx.background_executor().clone();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            timer.timer(Duration::from_millis(120)).await;
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_confirm_dialog(LocalizedConfirmDialogRequest {
                    title,
                    message,
                    confirm_label,
                    cancel_label,
                })
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| match result {
                Ok(true) => app.delete_selected_git_branch_confirmed(branch_name, cx),
                Ok(false) => {
                    app.status_message = "Git branch deletion canceled".to_string();
                    cx.notify();
                }
                Err(error) => {
                    app.status_message =
                        format!("failed to show branch deletion confirmation: {error}");
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    pub(super) fn delete_selected_git_branch_confirmed(
        &mut self,
        branch_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git branch deletion".to_string();
            cx.notify();
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        let worker_branch = branch_name.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation {
                label: format!("delete-branch:{branch_name}"),
                cancellable: false,
            },
            move |service, path| service.delete_project_git_branch(&path, &worker_branch, false),
            GitOperationCompletion {
                success_message: format!("deleted Git branch: {branch_name}"),
                failure_prefix: "Git branch deletion failed".to_string(),
                clear_commit_message: false,
                refresh_review: false,
                clear_selected_branch: false,
                selected_branch: None,
                ..Default::default()
            },
            cx,
        );
    }
}
