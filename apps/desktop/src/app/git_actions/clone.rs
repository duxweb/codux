use super::*;

impl CoduxApp {
    pub(in crate::app) fn init_project_git(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.show_git_action_error(
                self.text("git.error.init_failed", "Git init failed"),
                self.text(
                    "git.error.no_selected_init_project",
                    "No selected project for Git init.",
                ),
                cx,
            );
            return;
        };
        if self.git_running_operation.is_some() {
            self.show_git_action_error(
                self.text("git.error.init_failed", "Git init failed"),
                self.text(
                    "git.error.operation_running",
                    "Git operation is already running.",
                ),
                cx,
            );
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
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn set_git_clone_remote_url(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_clone_remote_url = value;
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn set_git_credential_username(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_credential_username = value;
        self.git_credential_error = None;
        resize_git_credentials_window(window, false);
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn set_git_credential_password_or_token(
        &mut self,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.git_credential_password_or_token = value;
        self.git_credential_error = None;
        resize_git_credentials_window(window, false);
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn open_git_clone_dialog(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_git_clone_window(cx);
    }

    pub(in crate::app) fn close_git_credentials_dialog(
        &mut self,
        window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        window.remove_window();
    }

    pub(in crate::app) fn set_git_commit_message(
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

    pub(in crate::app) fn generate_git_commit_message_with_ai(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message =
                "no selected project for Git commit message generation".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            self.invalidate_git_panel(cx);
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
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn apply_generated_git_commit_message(
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
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn show_git_commit_message_generation_error(
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
                    app.invalidate_git_panel(cx);
                });
            }
        })
        .detach();
    }

    pub(in crate::app) fn clone_project_git(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git clone".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        let remote_url = self.git_clone_remote_url.trim().to_string();
        if remote_url.is_empty() {
            self.status_message = "Git clone failed: remote URL is empty".to_string();
            self.invalidate_git_panel(cx);
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            self.invalidate_git_panel(cx);
            return;
        }

        let project_id = project.id.clone();
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        let action = "clone".to_string();
        let service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: false,
        });
        publish_child_window_git_operation(Some(action.clone()));
        self.status_message = format!("Git clone started for {project_name}");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let worker_remote_url = remote_url.clone();
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.clone_project_git(&worker_project_path, &worker_remote_url)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                let should_close = result.is_ok();
                publish_child_window_git_operation(None);
                if let Err(error) = result.as_ref() {
                    app.prepare_git_credentials_retry(
                        project_id.clone(),
                        project_name.clone(),
                        project_path.clone(),
                        remote_url.clone(),
                        error.clone(),
                        cx,
                    );
                }
                app.apply_project_git_repository_result(
                    project_id,
                    project_path,
                    action,
                    result,
                    true,
                    format!("Git repository cloned for {project_name}"),
                    cx,
                );
                if should_close {
                    let _ = window_handle.update(cx, |_view, window, _cx| window.remove_window());
                }
            });
        })
        .detach();
        self.invalidate_git_panel(cx);
    }

    fn prepare_git_credentials_retry(
        &mut self,
        project_id: String,
        project_name: String,
        project_path: String,
        remote_url: String,
        error: String,
        cx: &mut Context<Self>,
    ) {
        if !git_error_needs_credentials(&error) {
            return;
        }
        self.git_credential_project_id = Some(project_id);
        self.git_credential_project_name = project_name;
        self.git_credential_project_path = project_path;
        self.git_credential_remote_url = remote_url;
        self.git_credential_error = Some(error);
        self.git_credential_retrying = false;
        self.open_git_credentials_window(cx);
    }

    pub(in crate::app) fn retry_git_clone_with_credentials(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project_id) = self.git_credential_project_id.clone() else {
            self.git_credential_error = Some("No pending Git clone request.".to_string());
            self.invalidate_git_panel(cx);
            return;
        };
        let project_name = self.git_credential_project_name.clone();
        let project_path = self.git_credential_project_path.clone();
        let remote_url = self.git_credential_remote_url.clone();
        let username = self.git_credential_username.trim().to_string();
        let password_or_token = self.git_credential_password_or_token.trim().to_string();
        if username.is_empty() || password_or_token.is_empty() {
            self.git_credential_error = Some(
                GitSidebarLabels::load(&self.state.settings.language).auth_credentials_required,
            );
            resize_git_credentials_window(window, true);
            self.invalidate_git_panel(cx);
            return;
        }
        if self.git_running_operation.is_some() || self.git_credential_retrying {
            self.status_message = "Git operation is already running".to_string();
            self.invalidate_git_panel(cx);
            return;
        }

        let action = "clone".to_string();
        let service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.git_credential_retrying = true;
        self.git_credential_error = None;
        resize_git_credentials_window(window, false);
        self.git_running_operation = Some(GitRunningOperation {
            label: action.clone(),
            cancellable: false,
        });
        publish_child_window_git_operation(Some(action.clone()));
        self.status_message = format!("Git clone retry started for {project_name}");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let worker_project_path = project_path.clone();
            let worker_remote_url = remote_url.clone();
            let credentials = GitCredentials {
                username,
                password_or_token,
            };
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                service.clone_project_git_with_credentials(
                    &worker_project_path,
                    &worker_remote_url,
                    credentials,
                )
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                let should_close = result.is_ok();
                app.git_credential_retrying = false;
                publish_child_window_git_operation(None);
                if let Err(error) = result.as_ref() {
                    app.git_credential_error = Some(error.clone());
                    let _ = window_handle.update(cx, |_view, window, _cx| {
                        resize_git_credentials_window(window, true);
                    });
                }
                app.apply_project_git_repository_result(
                    project_id,
                    project_path,
                    action,
                    result,
                    true,
                    format!("Git repository cloned for {project_name}"),
                    cx,
                );
                if should_close {
                    let _ = window_handle.update(cx, |_view, window, _cx| window.remove_window());
                }
            });
        })
        .detach();
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn apply_project_git_repository_result(
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
                if action == "clone" {
                    self.clear_git_credentials_state();
                }
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
                    self.record_ui_state_clear("git_tree");
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    if refresh_files {
                        self.state.files = self.runtime_service.reload_project_files(
                            &project_path,
                            file_directory_option(&self.file_directory),
                        );
                        self.refresh_file_tree_state();
                        self.normalize_selected_file_entry();
                        self.git_clone_remote_url.clear();
                    }
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                    self.invalidate_task_column(cx);
                }
                self.status_message = success_message;
                publish_child_window_update(ChildWindowUpdateKind::Git);
            }
            Err(error) => {
                let title = format!("Git {action} failed");
                self.status_message = title.clone();
                self.show_system_error_alert(format!("Git {action} failed"), error, cx);
            }
        }
        self.invalidate_git_panel(cx);
    }

    fn clear_git_credentials_state(&mut self) {
        self.git_credential_project_id = None;
        self.git_credential_project_name.clear();
        self.git_credential_project_path.clear();
        self.git_credential_remote_url.clear();
        self.git_credential_username.clear();
        self.git_credential_password_or_token.clear();
        self.git_credential_error = None;
        self.git_credential_retrying = false;
    }
}
