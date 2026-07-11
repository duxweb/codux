use super::*;

impl CoduxApp {
    pub(in crate::app) fn register_native_menu_actions(
        &mut self,
        mut root: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        macro_rules! register {
            ($action:ty, $handler:expr) => {
                root = root.on_action(cx.listener(|app, _action: &$action, window, cx| {
                    ($handler)(app, window, cx);
                }));
            };
        }

        register!(
            native_menu::ShowAbout,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_about_window(window, cx)
            }
        );
        register!(
            native_menu::OpenSettings,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_settings_window(window, cx)
            }
        );
        register!(
            native_menu::CheckUpdates,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_update_dialog_window(window, cx)
            }
        );
        register!(native_menu::ExportDiagnostics, |app: &mut CoduxApp,
                                                   _window: &mut Window,
                                                   cx: &mut Context<
            CoduxApp,
        >| {
            app.export_diagnostics(cx)
        });
        register!(
            native_menu::OpenRuntimeLog,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_runtime_log(cx)
            }
        );
        register!(
            native_menu::OpenLiveLog,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_live_log(cx)
            }
        );
        register!(
            native_menu::OpenWebsite,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_codux_website(cx)
            }
        );
        register!(
            native_menu::OpenGithub,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_codux_github(cx)
            }
        );
        register!(
            native_menu::HideCodux,
            |_app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| { cx.hide() }
        );
        register!(
            native_menu::HideOthers,
            |_app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                cx.hide_other_apps()
            }
        );
        register!(
            native_menu::ShowAll,
            |_app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                cx.unhide_other_apps()
            }
        );
        register!(
            native_menu::QuitCodux,
            |app: &mut CoduxApp, _window: &mut Window, cx: &mut Context<CoduxApp>| {
                if app.window_mode == AppWindowMode::Main {
                    app.request_quit(cx);
                }
            }
        );
        register!(
            native_menu::NewProject,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_project_create_window(window, cx)
            }
        );
        register!(native_menu::OpenProjectFolder, |app: &mut CoduxApp,
                                                   window: &mut Window,
                                                   cx: &mut Context<
            CoduxApp,
        >| {
            app.open_project_folder_from_dialog(window, cx)
        });
        register!(native_menu::CloseCurrentProject, |app: &mut CoduxApp,
                                                     window: &mut Window,
                                                     cx: &mut Context<
            CoduxApp,
        >| {
            app.close_selected_project(window, cx)
        });
        register!(
            native_menu::CloseActive,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.close_active_workspace_item(window, cx)
            }
        );
        register!(
            native_menu::CloseWindow,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                if app.window_mode == AppWindowMode::Main {
                    app.close_active_workspace_item(window, cx);
                } else {
                    window.remove_window();
                }
            }
        );
        register!(
            native_menu::ViewTerminal,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Terminal, window, cx)
            }
        );
        register!(
            native_menu::ViewFiles,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Files, window, cx)
            }
        );
        register!(
            native_menu::ViewReview,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Review, window, cx)
            }
        );
        register!(
            native_menu::ViewStats,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.set_workspace_view(WorkspaceView::Stats, window, cx)
            }
        );
        register!(
            native_menu::ToggleProjects,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_project_column(window, cx)
            }
        );
        register!(
            native_menu::ToggleTasks,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_task_column(window, cx)
            }
        );
        register!(
            native_menu::OpenGitPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::Git, window, cx)
            }
        );
        register!(
            native_menu::OpenFilesPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::FileManager, window, cx)
            }
        );
        register!(
            native_menu::OpenAiPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::AIStats, window, cx)
            }
        );
        register!(
            native_menu::OpenSshPanel,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.toggle_assistant_panel(AssistantPanel::Ssh, window, cx)
            }
        );
        register!(
            native_menu::CreateSplit,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.split_terminal(window, cx)
            }
        );
        register!(
            native_menu::CreateTask,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.open_worktree_creator_window(window, cx)
            }
        );
        register!(
            native_menu::EditorSave,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                app.save_selected_file_preview(window, cx)
            }
        );
        register!(
            native_menu::EditorSearch,
            |app: &mut CoduxApp, window: &mut Window, cx: &mut Context<CoduxApp>| {
                // The global cmd-f binding consumes the keystroke before the
                // terminal's key handler sees it — route to terminal search here.
                if app.terminal_search_contains_focused(window, cx) {
                    return;
                }
                if let Some(terminal) = app.focused_or_active_terminal_view(window, cx) {
                    terminal.update(cx, |terminal, cx| terminal.open_search(window, cx));
                    return;
                }
                if let Some(editor) = app.active_file_editor_state() {
                    editor.update(cx, |state, cx| state.focus(window, cx));
                    window.dispatch_action(Box::new(gpui_component::input::Search), cx);
                }
            }
        );
        register!(native_menu::MinimizeWindow, |_app: &mut CoduxApp,
                                                window: &mut Window,
                                                _cx: &mut Context<
            CoduxApp,
        >| {
            window.minimize_window()
        });
        register!(
            native_menu::ZoomWindow,
            |_app: &mut CoduxApp, window: &mut Window, _cx: &mut Context<CoduxApp>| {
                window.zoom_window()
            }
        );
        register!(native_menu::ToggleFullscreen, |app: &mut CoduxApp,
                                                  window: &mut Window,
                                                  _cx: &mut Context<
            CoduxApp,
        >| {
            window.toggle_fullscreen();
            app.main_window_fullscreen = window.is_fullscreen();
        });
        root
    }

    pub(in crate::app) fn register_child_window_actions(
        &mut self,
        root: gpui::Div,
        cx: &mut Context<Self>,
    ) -> gpui::Div {
        let focus_handle = self.root_focus_handle(cx);
        self.register_native_menu_actions(root.track_focus(&focus_handle), cx)
    }

    pub(in crate::app) fn root_focus_handle(&mut self, cx: &mut Context<Self>) -> FocusHandle {
        if let Some(handle) = &self.root_focus_handle {
            return handle.clone();
        }
        let handle = cx.focus_handle();
        self.root_focus_handle = Some(handle.clone());
        handle
    }

    pub(in crate::app) fn focus_root_if_needed(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.window_mode == AppWindowMode::Main {
            return;
        }

        let focus_handle = self.root_focus_handle(cx);
        if !focus_handle.contains_focused(window, cx) {
            focus_handle.focus(window, cx);
        }
    }
}
