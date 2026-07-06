use super::*;

impl CoduxApp {
    pub(in crate::app) fn fetch_project_git(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_project_git_remote_action("fetch", cx);
    }

    pub(in crate::app) fn pull_project_git(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_project_git_remote_action("pull", cx);
    }

    pub(in crate::app) fn push_project_git(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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

    pub(in crate::app) fn force_push_project_git(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.state.selected_project.is_none() {
            self.status_message = "no selected project for Git force-push".to_string();
            self.invalidate_git_panel(cx);
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            self.invalidate_git_panel(cx);
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
                    app.invalidate_git_panel(cx);
                }
                Err(error) => {
                    app.status_message = format!("failed to show force-push confirmation: {error}");
                    app.invalidate_git_panel(cx);
                }
            });
        })
        .detach();
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn run_project_git_remote_action(
        &mut self,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = format!("no selected project for Git {action}");
            self.invalidate_git_panel(cx);
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            self.invalidate_git_panel(cx);
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
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn push_project_git_remote(
        &mut self,
        remote_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_project_git_push_remote(&remote_name, cx);
    }

    pub(in crate::app) fn run_project_git_push_remote(
        &mut self,
        remote_name: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git push".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        if self.git_running_operation.is_some() {
            self.status_message = "Git remote operation is already running".to_string();
            self.invalidate_git_panel(cx);
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
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn cancel_project_git(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git cancel".to_string();
            self.invalidate_git_panel(cx);
            return;
        };
        match self.runtime_service.cancel_project_git(&project.path) {
            Ok(()) => {
                self.status_message = "Git cancel requested".to_string();
            }
            Err(error) => {
                let title = self.text("git.error.operation_failed", "Git operation failed");
                self.show_git_action_error(title, error, cx);
            }
        }
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn apply_project_git_remote_result(
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
                    self.runtime_service
                        .broadcast_remote_git_status(&project_id, &project_path);
                    self.normalize_selected_git_file();
                    self.normalize_selected_git_branch();
                    self.state.worktrees = self
                        .runtime_service
                        .reload_worktrees(Some(&project_id), Some(&project_path));
                    self.invalidate_task_column(cx);
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
                let title = format!("Git {} failed", git_remote_action_label(&action));
                self.status_message = title.clone();
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn set_project_default_push_remote(
        &mut self,
        remote_name: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for default Git remote".to_string();
            self.invalidate_git_panel(cx);
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
                self.sync_project_list_state(cx);
                self.status_message = match remote_name {
                    Some(remote_name) => format!("default Git push remote saved: {remote_name}"),
                    None => "default Git push remote cleared".to_string(),
                };
            }
            Err(error) => {
                let title = self.text("git.error.operation_failed", "Git operation failed");
                self.show_git_action_error(title, error, cx);
            }
        }
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn add_git_remote(
        &mut self,
        name: String,
        url: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let worker_name = name.clone();
        let worker_url = url.clone();
        self.run_simple_git_operation(
            format!("add-remote:{name}"),
            false,
            format!("Git remote added: {name}"),
            "failed to add Git remote".to_string(),
            move |service, path| service.add_project_git_remote(&path, &worker_name, &worker_url),
            cx,
        );
    }

    pub(in crate::app) fn remove_project_git_remote(
        &mut self,
        remote_name: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if remote_name.trim().is_empty() {
            self.status_message = "no Git remote selected to remove".to_string();
            self.invalidate_git_panel(cx);
            return;
        }
        if self.git_running_operation.is_some() {
            self.status_message = "Git operation is already running".to_string();
            self.invalidate_git_panel(cx);
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
                    app.invalidate_git_panel(cx);
                }
                Err(error) => {
                    app.status_message =
                        format!("failed to show remote removal confirmation: {error}");
                    app.invalidate_git_panel(cx);
                }
            });
        })
        .detach();
        self.invalidate_git_panel(cx);
    }

    pub(in crate::app) fn remove_project_git_remote_confirmed(
        &mut self,
        remote_name: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = &self.state.selected_project else {
            self.status_message = "no selected project for Git remote".to_string();
            self.invalidate_git_panel(cx);
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
}
