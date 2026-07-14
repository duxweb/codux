use super::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProjectEditorStateTransition {
    Refresh,
    Switch,
    Rebind,
}

fn project_editor_state_transition(
    previous: Option<&ProjectInfo>,
    next: Option<&ProjectInfo>,
) -> ProjectEditorStateTransition {
    match (previous, next) {
        (Some(previous), Some(next)) if previous.id == next.id => {
            if previous.path == next.path && previous.runtime_target == next.runtime_target {
                ProjectEditorStateTransition::Refresh
            } else {
                ProjectEditorStateTransition::Rebind
            }
        }
        (None, None) => ProjectEditorStateTransition::Refresh,
        _ => ProjectEditorStateTransition::Switch,
    }
}

impl CoduxApp {
    pub(in crate::app) fn apply_project_editor_state(
        &mut self,
        next: RuntimeState,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let transition = project_editor_state_transition(
            self.state.selected_project.as_ref(),
            next.selected_project.as_ref(),
        );
        if transition == ProjectEditorStateTransition::Refresh {
            self.apply_project_list_state(next, cx);
            return;
        }

        if self.state.selected_project.is_some() {
            self.sync_terminal_state_for_project_switch();
        }
        if transition == ProjectEditorStateTransition::Rebind
            && let Some(project) = self.state.selected_project.as_ref()
        {
            let project_id = project.id.clone();
            let mut owner_ids = HashSet::from([project_id.clone()]);
            owner_ids.extend(
                self.state
                    .worktrees
                    .worktrees
                    .iter()
                    .filter(|worktree| worktree.project_id == project_id)
                    .map(|worktree| worktree.id.clone()),
            );
            self.close_terminal_sessions_for_owners(&owner_ids, cx);
            for (key, entry) in &mut self.terminal_layout_cache {
                if key.project_id == project_id {
                    entry.runtime = TerminalRuntimeSummary::default();
                }
            }
            self.file_panel_cache
                .retain(|key, _| key.project_id != project_id);
        }

        let selected_project_id = next
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        self.state.projects = next.projects;
        self.prune_worktree_scoped_caches();
        let Some(project_id) = selected_project_id else {
            self.state.selected_project = None;
            self.sync_project_list_state(cx);
            return;
        };
        self.select_project_after_state_reload(project_id, window, cx);
        self.restore_selected_project_terminal_layout_now(
            self.project_switch_generation,
            window,
            cx,
        );
    }

    pub(in crate::app) fn apply_project_list_state(
        &mut self,
        next: RuntimeState,
        cx: &mut Context<Self>,
    ) {
        let previous_selected_id = self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.clone());
        self.state.projects = next.projects;
        self.prune_worktree_scoped_caches();
        self.state.selected_project = previous_selected_id
            .as_deref()
            .and_then(|id| {
                self.state
                    .projects
                    .iter()
                    .find(|project| project.id == id)
                    .cloned()
            })
            .or(next.selected_project);
        self.sync_project_list_state(cx);
    }

    pub(in crate::app) fn reload_project_open_applications_async(
        &mut self,
        cx: &mut Context<Self>,
    ) {
        let service = self.runtime_service.clone();
        self.runtime_trace("project-open", "applications_refresh queued");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.runtime_trace_frontend("project-open", "applications_refresh start");
                let applications = service.project_open_applications();
                service.runtime_trace_frontend("project-open", "applications_refresh ok");
                applications
            })
            .await;

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(applications) => {
                        app.project_open_applications = applications;
                    }
                    Err(error) => {
                        app.runtime_trace(
                            "project-open",
                            &format!("applications_refresh failed join_error={error}"),
                        );
                    }
                }
                app.invalidate_project_management(cx);
            });
        })
        .detach();
    }

    pub(in crate::app) fn reveal_selected_project_in_file_manager(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to reveal".to_string();
            self.invalidate_project_management(cx);
            return;
        };

        match self
            .runtime_service
            .project_reveal_in_file_manager(&project.path)
        {
            Ok(()) => {
                self.status_message = format!("revealed project: {}", project.name);
            }
            Err(error) => self.status_message = format!("failed to reveal project: {error}"),
        }
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn reveal_project_in_file_manager(
        &mut self,
        project_name: String,
        project_path: String,
        cx: &mut Context<Self>,
    ) {
        match self
            .runtime_service
            .project_reveal_in_file_manager(&project_path)
        {
            Ok(()) => {
                self.status_message = format!("revealed project: {project_name}");
            }
            Err(error) => {
                let title = self.text("sidebar.project.open_folder", "Open Folder");
                self.status_message = format!("failed to reveal project: {error}");
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn open_selected_project_in_application(
        &mut self,
        application_id: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to open".to_string();
            self.invalidate_project_management(cx);
            return;
        };

        let application_label = self
            .project_open_applications
            .iter()
            .find(|application| application.id == application_id)
            .map(|application| application.label.clone())
            .unwrap_or_else(|| application_id.clone());

        match self
            .runtime_service
            .project_open_in_application(project.path, application_id)
        {
            Ok(()) => {
                self.status_message = format!("opened {} in {application_label}", project.name);
            }
            Err(error) => {
                self.status_message = format!(
                    "failed to open {} in {application_label}: {error}",
                    project.name
                );
                self.reload_project_open_applications_async(cx);
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn open_remote_project_web_url(
        &mut self,
        device_id: String,
        url: String,
        title: String,
        cx: &mut Context<Self>,
    ) {
        match self.runtime_service.open_host_browser_url(&device_id, &url) {
            Ok(opened) => {
                match self.runtime_service.open_url_with_http_proxy(
                    &opened.original_url,
                    &opened.proxy_host,
                    opened.proxy_port,
                ) {
                    Ok(()) => {
                        self.status_message =
                            format!("opened through web tunnel: {}", opened.original_url);
                    }
                    Err(error) => {
                        self.status_message = "failed to open web tunnel".to_string();
                        self.show_system_error_alert(title, error, cx);
                    }
                }
            }
            Err(error) => {
                self.status_message = "web tunnel unavailable".to_string();
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_remote_project_browser_session(
        &mut self,
        device_id: String,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title = translate(
            &locale,
            "workspace.web_tunnel.browser.open",
            "Open Web Tunnel Browser",
        );
        match self.runtime_service.open_host_browser_session(&device_id) {
            Ok(opened) => {
                match self.runtime_service.open_url_with_http_proxy(
                    &opened.original_url,
                    &opened.proxy_host,
                    opened.proxy_port,
                ) {
                    Ok(()) => {
                        self.status_message =
                            format!("opened web tunnel browser: {}", opened.original_url);
                    }
                    Err(error) => {
                        self.status_message = "failed to open web tunnel browser".to_string();
                        self.show_system_error_alert(title.clone(), error, cx);
                    }
                }
            }
            Err(error) => {
                self.status_message = "web tunnel unavailable".to_string();
                self.show_system_error_alert(title, error, cx);
            }
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_project_folder_from_dialog(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let request = LocalizedOpenDialogRequest {
            title: translate(&locale, "project.open_folder.title", "Open Folder"),
            message: translate(
                &locale,
                "project.open_folder.message",
                "Choose a project folder to import.",
            ),
            prompt: translate(&locale, "project.open_folder.prompt", "Open"),
            default_path: None,
            filters: Vec::new(),
            directory: true,
            multiple: false,
            can_create_directories: Some(false),
        };
        let runtime_service = self.runtime_service.clone();
        let window_handle = window.window_handle();
        self.status_message = "opening project folder dialog".to_string();
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                let paths = runtime_service.localized_open_dialog(request)?;
                let Some(paths) = paths else {
                    return Ok(None);
                };
                let Some(path) = paths.first().cloned() else {
                    return Ok(None);
                };
                let name = std::path::Path::new(&path)
                    .file_name()
                    .and_then(|name| name.to_str())
                    .filter(|name| !name.trim().is_empty())
                    .unwrap_or("Project")
                    .to_string();
                let project_id = runtime_service.create_or_select_project(&name, &path)?;
                let state = runtime_service.reload_state();
                Ok(Some((project_id, state)))
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join open project dialog: {error}")));

            let _ = window_handle.update(cx, |_root, window, cx| {
                let _ = this.update(cx, |app, cx| {
                    app.apply_open_project_folder_result(result, window, cx);
                });
            });
        })
        .detach();
    }

    fn apply_open_project_folder_result(
        &mut self,
        result: Result<Option<(String, RuntimeState)>, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(Some((project_id, state))) => {
                self.state = state;
                let selected_project_id = self
                    .state
                    .selected_project
                    .as_ref()
                    .map(|project| project.id.clone());
                if selected_project_id.as_deref() == Some(project_id.as_str()) {
                    self.select_project_after_state_reload(project_id.clone(), window, cx);
                } else {
                    self.normalize_selected_ai_session();
                    self.normalize_selected_runtime_session();
                    self.normalize_selected_ssh_profile();
                    self.sync_project_list_state(cx);
                }
                self.status_message = format!("project added/selected: {project_id}");
            }
            Ok(None) => {
                self.status_message = "project import canceled".to_string();
            }
            Err(error) => {
                self.status_message = format!("failed to choose project folder: {error}");
            }
        }
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn close_selected_project(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to close".to_string();
            self.invalidate_project_management(cx);
            return;
        };
        self.remove_project(project, cx);
    }

    pub(in crate::app) fn request_remove_project_by_id(
        &mut self,
        project_id: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self
            .state
            .projects
            .iter()
            .find(|project| project.id == project_id)
            .cloned()
        else {
            self.status_message = "project not found".to_string();
            self.invalidate_project_management(cx);
            return;
        };

        let title = self.text("project.remove.title", "Remove Project");
        let message = self
            .text(
                "project.remove.confirm_format",
                "Are you sure you want to remove project %@? Files on disk will not be deleted.",
            )
            .replace("%@", &project.name);
        let confirm_label = self.text("common.remove", "Remove");
        let cancel_label = self.text("common.cancel", "Cancel");
        let service = self.runtime_service.clone();
        self.status_message = "waiting for project removal confirmation".to_string();
        self.invalidate_project_management(cx);
        self.invalidate_status_bar(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
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
                Ok(true) => app.remove_project(project, cx),
                Ok(false) => {
                    app.status_message = "project removal canceled".to_string();
                    app.invalidate_project_management(cx);
                    app.invalidate_status_bar(cx);
                }
                Err(error) => {
                    let title = app.text("project.remove.title", "Remove Project");
                    app.status_message = title.clone();
                    app.show_system_error_alert(title, error, cx);
                    app.invalidate_project_management(cx);
                    app.invalidate_status_bar(cx);
                }
            });
        })
        .detach();
    }

    /// Drop worktree-scoped UI caches for projects that no longer exist. These
    /// maps (file panel state, terminal layout, active terminal ids) are keyed
    /// by (project, worktree) and are otherwise only added to — one entry per
    /// worktree ever visited — so without this they retain closed projects'
    /// state for the life of the process.
    pub(in crate::app) fn prune_worktree_scoped_caches(&mut self) {
        let live: std::collections::HashSet<String> =
            self.state.projects.iter().map(|p| p.id.clone()).collect();
        self.file_panel_cache
            .retain(|key, _| live.contains(&key.project_id));
        self.terminal_layout_cache
            .retain(|key, _| live.contains(&key.project_id));
        self.active_terminal_runtime_ids
            .retain(|key, _| live.contains(&key.project_id));
    }

    fn remove_project(&mut self, project: ProjectInfo, cx: &mut Context<Self>) {
        let runtime_service = self.runtime_service.clone();
        let project_id = project.id.clone();
        self.runtime_trace(
            "project",
            &format!("remove_project queued project_id={project_id}"),
        );
        self.status_message = format!("closing project: {}", project.name);
        self.invalidate_project_management(cx);
        self.invalidate_status_bar(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                runtime_service.runtime_trace_frontend(
                    "project",
                    &format!("remove_project start project_id={project_id}"),
                );
                let result = runtime_service
                    .close_project(&project_id)
                    .map(|next_project_id| (runtime_service.reload_state(), next_project_id));
                match &result {
                    Ok(_) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("remove_project ok project_id={project_id}"),
                    ),
                    Err(error) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("remove_project failed project_id={project_id} error={error}"),
                    ),
                }
                result
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join project removal: {error}")));

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok((state, next_project_id)) => {
                        app.state = state;
                        app.prune_worktree_scoped_caches();
                        app.normalize_selected_ai_session();
                        app.normalize_selected_runtime_session();
                        app.normalize_selected_ssh_profile();
                        app.sync_project_list_state(cx);
                        app.status_message = match next_project_id {
                            Some(next_project_id) => {
                                format!("closed {}, selected {next_project_id}", project.name)
                            }
                            None => format!("closed {}, no projects left", project.name),
                        };
                    }
                    Err(error) => {
                        app.status_message = format!("failed to close project: {error}");
                    }
                }
                app.invalidate_project_management(cx);
                app.invalidate_status_bar(cx);
            });
        })
        .detach();
    }
}

#[cfg(test)]
mod project_editor_state_transition_tests {
    use super::*;

    fn project(id: &str, path: &str, runtime_target: ProjectRuntimeTarget) -> ProjectInfo {
        ProjectInfo {
            id: id.to_string(),
            name: id.to_string(),
            path: path.to_string(),
            exists: true,
            badge: String::new(),
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
            runtime_target,
        }
    }

    #[test]
    fn switches_to_newly_created_project() {
        let previous = project("local", "C:\\workspace\\local", ProjectRuntimeTarget::Local);
        let next = project(
            "wsl",
            "/home/user/project",
            ProjectRuntimeTarget::Wsl {
                distribution: "Ubuntu-24.04".to_string(),
            },
        );

        assert_eq!(
            project_editor_state_transition(Some(&previous), Some(&next)),
            ProjectEditorStateTransition::Switch
        );
    }

    #[test]
    fn rebinds_changed_runtime_target() {
        let previous = project("project", "/home/user/project", ProjectRuntimeTarget::Local);
        let next = project(
            "project",
            "/home/user/project",
            ProjectRuntimeTarget::Wsl {
                distribution: "Ubuntu-24.04".to_string(),
            },
        );

        assert_eq!(
            project_editor_state_transition(Some(&previous), Some(&next)),
            ProjectEditorStateTransition::Rebind
        );
    }

    #[test]
    fn rebinds_changed_project_path() {
        let previous = project("project", "/old/project", ProjectRuntimeTarget::Local);
        let next = project("project", "/new/project", ProjectRuntimeTarget::Local);

        assert_eq!(
            project_editor_state_transition(Some(&previous), Some(&next)),
            ProjectEditorStateTransition::Rebind
        );
    }

    #[test]
    fn refreshes_metadata_only_edit() {
        let previous = project(
            "project",
            "/home/user/project",
            ProjectRuntimeTarget::Wsl {
                distribution: "Ubuntu-24.04".to_string(),
            },
        );
        let mut next = previous.clone();
        next.name = "Renamed".to_string();

        assert_eq!(
            project_editor_state_transition(Some(&previous), Some(&next)),
            ProjectEditorStateTransition::Refresh
        );
    }
}
