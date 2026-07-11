use super::FilePickerOpenRequest;
use super::*;

impl CoduxApp {
    pub(in crate::app) fn reorder_projects_by_ids(
        &mut self,
        project_ids: Vec<String>,
        cx: &mut Context<Self>,
    ) {
        if project_ids.len() != self.state.projects.len()
            || self
                .state
                .projects
                .iter()
                .zip(project_ids.iter())
                .all(|(project, project_id)| project.id == *project_id)
        {
            return;
        }

        let runtime_service = self.runtime_service.clone();
        self.runtime_trace(
            "project",
            &format!("reorder_projects queued count={}", project_ids.len()),
        );
        self.invalidate_project_management(cx);

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::run_limited_blocking(move || {
                runtime_service.runtime_trace_frontend(
                    "project",
                    &format!("reorder_projects start count={}", project_ids.len()),
                );
                let result = runtime_service.project_reorder(ProjectReorderRequest { project_ids });
                match &result {
                    Ok(snapshot) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("reorder_projects ok count={}", snapshot.projects.len()),
                    ),
                    Err(error) => runtime_service.runtime_trace_frontend(
                        "project",
                        &format!("reorder_projects failed error={error}"),
                    ),
                }
                result.map(|_| runtime_service.reload_state())
            })
            .await
            .unwrap_or_else(|error| Err(format!("failed to join project reorder: {error}")));

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(state) => {
                        app.state = state;
                        app.sync_project_list_state(cx);
                    }
                    Err(error) => {
                        app.status_message = format!("failed to reorder projects: {error}");
                    }
                }
                app.invalidate_project_management(cx);
                app.invalidate_status_bar(cx);
            });
        })
        .detach();
    }

    pub(in crate::app) fn edit_project_by_id(
        &mut self,
        project_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self
            .state
            .selected_project
            .as_ref()
            .map(|project| project.id.as_str())
            != Some(project_id.as_str())
        {
            self.select_project(project_id, window, cx);
        }
        self.open_selected_project_editor_window(window, cx);
    }

    pub(in crate::app) fn open_project_create_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let locale = locale_from_language_setting(&self.state.settings.language);
        if Self::activate_child_window(&mut self.project_editor_window, cx) {
            self.status_message = "project creator already opened".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::ProjectEditor,
                title: SharedString::from(translate(
                    &locale,
                    "project.create.title",
                    "Create Project",
                )),
                size: size(px(620.0), px(446.0)),
                min_size: size(px(520.0), px(390.0)),
                already_open_message: "project creator already opened",
                opened_message: "project creator opened",
                failed_prefix: "failed to open project creator",
            },
            cx,
            |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_project_creator_window_from_state(state, runtime, runtime_service)
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn open_selected_project_editor_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = self.state.selected_project.clone() else {
            self.status_message = "no selected project to edit".to_string();
            self.invalidate_project_management(cx);
            return;
        };
        let locale = locale_from_language_setting(&self.state.settings.language);

        if Self::activate_child_window(&mut self.project_editor_window, cx) {
            self.status_message = "project editor already opened".to_string();
            self.invalidate_project_management(cx);
            return;
        }

        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::ProjectEditor,
                title: SharedString::from(translate(&locale, "project.edit.title", "Edit Project")),
                size: size(px(620.0), px(446.0)),
                min_size: size(px(520.0), px(390.0)),
                already_open_message: "project editor already opened",
                opened_message: "project editor opened",
                failed_prefix: "failed to open project editor",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_project_editor_window_from_state(
                    project,
                    state,
                    runtime,
                    runtime_service,
                )
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn set_project_editor_name(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_name = value;
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn set_project_editor_badge_symbol(
        &mut self,
        value: Option<String>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_symbol = value;
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn set_project_editor_badge_color(
        &mut self,
        value: String,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.project_editor_badge_color_hex = value;
        self.invalidate_project_management(cx);
    }

    pub(in crate::app) fn choose_project_editor_directory(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Open the unified file-picker sub-window (a standard child window with
        // the shared title bar / footer), browsing local or the selected host.
        let device_id = self.project_editor_host_device_id.clone();
        let start = {
            let path = self.project_editor_path.trim();
            (!path.is_empty()).then(|| path.to_string())
        };
        self.open_file_picker_window(
            FilePickerOpenRequest {
                mode: FilePickerMode::OpenFolder,
                target: FilePickerTarget::ProjectEditorPath,
                device_id,
                start_path: start,
                default_filename: None,
            },
            window,
            cx,
        );
    }
}
