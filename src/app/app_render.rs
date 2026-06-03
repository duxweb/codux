use super::*;

impl Render for CoduxApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.focus_root_if_needed(window, cx);

        if self.window_mode == AppWindowMode::DesktopPet {
            return self.desktop_pet_window(window, cx).into_any_element();
        }

        if self.window_mode == AppWindowMode::About {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.about_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::UpdateDialog {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.update_dialog_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::GitDiff {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(git_diff_window_workspace(
                    self.git_diff_window_path.as_deref(),
                    &self.git_diff_window_content,
                    self.git_diff_window_error.as_deref(),
                    &self.state.settings.language,
                    cx,
                ))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::GitClone {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(git_clone_window_workspace(
                    &self.git_clone_remote_url,
                    self.git_running_operation.as_ref(),
                    &self.state.settings.language,
                    window,
                    cx,
                ))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::GitCredentials {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(git_credentials_window_workspace(
                    self,
                    &self.state.settings.language,
                    window,
                    cx,
                ))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::MemoryManager {
            let memory_queue_active = self.state.memory_manager.extraction.queued > 0
                || self.state.memory_manager.extraction.running > 0;
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(memory_manager_window_workspace(
                    &self.state.memory_manager,
                    self.memory_manager_tab,
                    &self.memory_manager_scope,
                    self.memory_manager_project_id.as_deref(),
                    self.selected_memory_entry_id.as_deref(),
                    self.selected_memory_summary_id.as_deref(),
                    self.memory_processing || memory_queue_active,
                    self.memory_manager_refreshing,
                    self.memory_project_profile_refreshing,
                    &self.state.settings.language,
                    window,
                    cx,
                ))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::PetClaim {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.pet_claim_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::PetCustomInstall {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.pet_custom_install_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::PetDex {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.pet_dex_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::Settings {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.settings_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::ProjectEditor {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.project_editor_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::WorktreeCreator {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(self.worktree_creator_workspace(window, cx))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        if self.window_mode == AppWindowMode::SshProfileEditor {
            let root = div()
                .size_full()
                .text_color(cx.theme().foreground)
                .bg(cx.theme().background)
                .on_key_down(cx.listener(Self::on_key_down))
                .child(ssh_profile_editor_workspace(
                    self,
                    self.ssh_testing,
                    window,
                    cx,
                ))
                .child(self.codux_tooltip_layer(cx));
            return self
                .register_child_window_actions(root, cx)
                .into_any_element();
        }

        let project_column_view = self.project_column_view(cx);
        let task_column_view = self.task_column_view(cx);
        let workspace_column_view = self.workspace_column_view(cx);
        let status_bar_view = self.status_bar_view(cx);
        let project_column_width = px(if self.project_column_collapsed {
            PROJECT_COLUMN_COLLAPSED_WIDTH
        } else {
            PROJECT_COLUMN_EXPANDED_WIDTH
        });
        let task_column_width = TASK_COLUMN_FIXED_WIDTH;

        let focus_handle = self.root_focus_handle(cx);
        if !self.main_window_close_handler_registered {
            self.main_window_close_handler_registered = true;
            let app_entity = cx.entity();
            window.on_window_should_close(cx, move |_window, cx| {
                let _ = app_entity.update(cx, |app, cx| {
                    app.shutdown_main_window(cx);
                });
                true
            });
        }
        let root = div()
            .size_full()
            .flex()
            .flex_col()
            .text_color(color(theme::TEXT))
            .bg(cx.theme().background)
            .track_focus(&focus_handle)
            .on_key_down(cx.listener(Self::on_key_down))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .w_full()
                    .h_full()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(
                        gpui::AnyView::from(project_column_view).cached(
                            gpui::StyleRefinement::default()
                                .flex()
                                .flex_shrink_0()
                                .w(project_column_width)
                                .h_full(),
                        ),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_1()
                            .w_full()
                            .min_w_0()
                            .min_h_0()
                            .h_full()
                            .overflow_hidden()
                            .when(!self.task_column_collapsed, |this| {
                                this.child(
                                    div()
                                        .flex_none()
                                        .flex_shrink_0()
                                        .flex_basis(px(task_column_width))
                                        .w(px(task_column_width))
                                        .min_w(px(task_column_width))
                                        .max_w(px(task_column_width))
                                        .h_full()
                                        .overflow_hidden()
                                        .border_r_1()
                                        .border_color(cx.theme().border)
                                        .child(
                                            gpui::AnyView::from(task_column_view).cached(
                                                gpui::StyleRefinement::default()
                                                    .flex()
                                                    .flex_none()
                                                    .h_full()
                                                    .min_w(px(task_column_width))
                                                    .max_w(px(task_column_width))
                                                    .w_full(),
                                            ),
                                        ),
                                )
                            })
                            .child(
                                div()
                                    .flex()
                                    .flex_1()
                                    .flex_basis(px(0.0))
                                    .w_full()
                                    .h_full()
                                    .min_w_0()
                                    .min_h_0()
                                    .overflow_hidden()
                                    .child(
                                        gpui::AnyView::from(workspace_column_view).cached(
                                            gpui::StyleRefinement::default()
                                                .flex()
                                                .flex_1()
                                                .flex_basis(px(0.0))
                                                .w_full()
                                                .h_full()
                                                .min_w(px(0.0))
                                                .min_h(px(0.0)),
                                        ),
                                    ),
                            ),
                    ),
            )
            .child(
                gpui::AnyView::from(status_bar_view).cached(
                    gpui::StyleRefinement::default()
                        .flex()
                        .flex_none()
                        .w_full()
                        .h(px(28.0)),
                ),
            )
            .child(self.codux_tooltip_layer(cx));

        self.register_native_menu_actions(root, cx)
            .into_any_element()
    }
}
