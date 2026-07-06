use super::*;

impl CoduxApp {
    pub(super) fn show_git_action_error(
        &mut self,
        title: impl Into<String>,
        message: impl Into<String>,
        cx: &mut Context<Self>,
    ) {
        let title = title.into();
        let message = message.into();
        self.status_message = title.clone();
        self.show_system_error_alert(title, message, cx);
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn start_project_git_operation(
        &mut self,
        project_id: String,
        project_path: String,
        operation: GitRunningOperation,
        action: impl FnOnce(RuntimeService, String) -> Result<GitSummary, String> + Send + 'static,
        completion: GitOperationCompletion,
        cx: &mut Context<Self>,
    ) {
        if self.git_running_operation.is_some() {
            self.show_git_action_error(
                self.text("git.error.operation_failed", "Git operation failed"),
                self.text(
                    "git.error.operation_running",
                    "Git operation is already running.",
                ),
                cx,
            );
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
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn apply_project_git_operation_result(
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
                        self.sync_project_list_state(cx);
                    }
                    self.state.git = summary;
                    self.runtime_service
                        .broadcast_remote_git_status(&project_id, &project_path);
                    if completion.clear_commit_message {
                        self.git_commit_message.clear();
                        self.git_commit_message_revision =
                            self.git_commit_message_revision.saturating_add(1);
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
                    if completion.clear_git_tree_state {
                        self.git_expanded_dirs.clear();
                        self.git_tree_children.clear();
                        self.record_ui_state_clear("git_tree");
                    }
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    if completion.clear_git_diff_preview {
                        self.git_diff_preview =
                            "select a changed file to preview its diff".to_string();
                        self.clear_git_review_derived_content();
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
                        let content = self.runtime_service.read_project_git_review_file_content(
                            &project_path,
                            file_path,
                            self.git_review.base_branch.as_deref(),
                        );
                        self.set_git_review_derived_content(content);
                    }
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                    self.invalidate_task_column(cx);
                }
                self.status_message = completion.success_message;
                publish_child_window_update(ChildWindowUpdateKind::Git);
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
                self.status_message = completion.failure_prefix.clone();
                self.show_system_error_alert(completion.failure_prefix, error, cx);
            }
        }
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn show_system_error_alert(
        &mut self,
        title: String,
        message: String,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        let button_label = self.text("common.ok", "OK");
        cx.spawn(async move |_: gpui::WeakEntity<Self>, _cx| {
            let _ = codux_runtime::async_runtime::spawn_blocking(move || {
                service.localized_alert_dialog(LocalizedAlertDialogRequest {
                    title,
                    message,
                    button_label,
                })
            })
            .await;
        })
        .detach();
    }

    pub(super) fn confirm_git_action(
        &mut self,
        title: String,
        message: String,
        confirm_label: String,
        on_confirm: impl FnOnce(&mut Self, &mut Context<Self>) + 'static,
        cx: &mut Context<Self>,
    ) {
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for Git confirmation".to_string();
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
                Ok(true) => on_confirm(app, cx),
                Ok(false) => {
                    app.status_message = "Git action canceled".to_string();
                    app.invalidate_git_panel(cx);
                }
                Err(error) => {
                    app.status_message = format!("failed to show Git confirmation: {error}");
                    app.invalidate_git_panel(cx);
                }
            });
        })
        .detach();
        self.invalidate_git_panel(cx);
    }

    /// Uniform runner for the simple branch/stash/tag operations.
    pub(super) fn run_simple_git_operation(
        &mut self,
        label: String,
        cancellable: bool,
        success_message: String,
        failure_prefix: String,
        action: impl FnOnce(RuntimeService, String) -> Result<GitSummary, String> + Send + 'static,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for Git {label}");
            self.invalidate_git_panel(cx);
            return;
        };
        let project_id = project.id.clone();
        let project_path = project.path.clone();
        self.start_project_git_operation(
            project_id,
            project_path,
            GitRunningOperation { label, cancellable },
            action,
            GitOperationCompletion {
                success_message,
                failure_prefix,
                ..Default::default()
            },
            cx,
        );
    }
}
