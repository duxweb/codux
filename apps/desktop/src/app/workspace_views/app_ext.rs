use super::*;

impl CoduxApp {
    pub(super) fn empty_project_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let title =
            translate(&locale, "welcome.title_format", "Welcome to %@").replace("%@", "Codux");
        let subtitle = translate(
            &locale,
            "welcome.subtitle",
            "Create a new project or open an existing folder to get started",
        );
        let new_project = translate(&locale, "menu.file.new_project", "New Project");
        let open_project = translate(&locale, "welcome.open_project", "Open Project");

        div()
            .size_full()
            .min_h_0()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .bg(color(theme::BG_TERMINAL))
            .child(
                div()
                    .w(px(360.0))
                    .max_w_full()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(14.0))
                    .px_4()
                    .text_center()
                    .child(
                        img("app-icons/codux-default.svg")
                            .size(px(72.0))
                            .object_fit(ObjectFit::Contain),
                    )
                    .child(
                        div()
                            .text_size(rems(1.375))
                            .line_height(rems(1.75))
                            .text_color(color(theme::TEXT))
                            .child(title),
                    )
                    .child(
                        div()
                            .max_w(px(320.0))
                            .text_size(rems(0.8125))
                            .line_height(rems(1.25))
                            .text_color(color(theme::TEXT_DIM))
                            .child(subtitle),
                    )
                    .child(
                        div()
                            .mt_2()
                            .flex()
                            .items_center()
                            .gap_2()
                            .child(
                                Button::new("welcome-new-project")
                                    .primary()
                                    .text_size(rems(0.875))
                                    .on_click(window.listener_for(
                                        &cx.entity(),
                                        |app, _event, window, cx| {
                                            app.open_project_create_window(window, cx);
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::new(HeroIconName::FolderPlus).size_3p5())
                                            .child(new_project),
                                    ),
                            )
                            .child(
                                Button::new("welcome-open-project")
                                    .secondary()
                                    .text_size(rems(0.875))
                                    .on_click(window.listener_for(
                                        &cx.entity(),
                                        |app, _event, window, cx| {
                                            app.open_project_folder_from_dialog(window, cx);
                                        },
                                    ))
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap_2()
                                            .child(Icon::new(HeroIconName::FolderOpen).size_3p5())
                                            .child(open_project),
                                    ),
                            ),
                    ),
            )
    }
}
impl CoduxApp {
    pub(in crate::app) fn workspace_toolbar_snapshot(&self) -> WorkspaceToolbarSnapshot {
        WorkspaceToolbarSnapshot {
            fingerprint: workspace_toolbar_fingerprint(self),
        }
    }

    pub(in crate::app) fn workspace_assistant_snapshot(&self) -> WorkspaceAssistantSnapshot {
        WorkspaceAssistantSnapshot {
            panel: self.assistant_panel,
            has_project: self.state.selected_project.is_some(),
            is_remote_project: self
                .state
                .selected_project
                .as_ref()
                .is_some_and(|project| project.is_remote()),
        }
    }

    pub(in crate::app) fn workspace_column_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceColumnView> {
        if let Some(view) = &self.workspace_column_view {
            return view.clone();
        }
        let toolbar_view = self.workspace_toolbar_view(cx);
        let body_view = self.workspace_body_view(cx);
        let assistant_view = self.workspace_assistant_view(cx);
        let view = cx.new(|_| WorkspaceColumnView::new(toolbar_view, body_view, assistant_view));
        self.workspace_column_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn workspace_toolbar_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceToolbarView> {
        if let Some(view) = &self.workspace_toolbar_view {
            let snapshot = self.workspace_toolbar_snapshot();
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let snapshot = self.workspace_toolbar_snapshot();
        let view = cx.new(|_| WorkspaceToolbarView::new(app_entity, snapshot));
        self.workspace_toolbar_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn workspace_body_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceBodyView> {
        if let Some(view) = &self.workspace_body_view {
            return view.clone();
        }
        let app_entity = cx.entity();
        let view = cx.new(|_| WorkspaceBodyView::new(app_entity));
        self.workspace_body_view = Some(view.clone());
        view
    }

    pub(in crate::app) fn workspace_assistant_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> gpui::Entity<WorkspaceAssistantView> {
        if let Some(view) = &self.workspace_assistant_view {
            let snapshot = self.workspace_assistant_snapshot();
            view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
            return view.clone();
        }
        let app_entity = cx.entity();
        let snapshot = self.workspace_assistant_snapshot();
        let view = cx.new(|_| WorkspaceAssistantView::new(app_entity, snapshot));
        self.workspace_assistant_view = Some(view.clone());
        view
    }
}
impl CoduxApp {
    pub(in crate::app) fn update_file_editor_workspace_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).file_editor_workspace_view.clone())
        else {
            return false;
        };
        let snapshot = self.file_editor_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn update_review_workspace_view(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).review_workspace_view.clone())
        else {
            return false;
        };
        let snapshot = self.review_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn update_stats_workspace_view(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(view) = self
            .workspace_body_view
            .as_ref()
            .and_then(|view| view.read(cx).stats_workspace_view.clone())
        else {
            return false;
        };
        let snapshot = self.stats_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn update_terminal_workspace_view(
        &mut self,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(body_view) = self.workspace_body_view.as_ref().cloned() else {
            return false;
        };
        let Some(view) = body_view.read(cx).terminal_workspace_view.clone() else {
            body_view.update(cx, |_view, cx| cx.notify());
            return true;
        };
        let snapshot = self.terminal_workspace_snapshot();
        view.update(cx, |view, cx| view.set_snapshot(snapshot, cx));
        true
    }

    pub(in crate::app) fn terminal_workspace_snapshot(&self) -> TerminalWorkspaceSnapshot {
        let main_panes = self
            .main_terminal()
            .map(|tab| {
                tab.panes
                    .iter()
                    .enumerate()
                    .map(|(index, slot)| {
                        let terminal_id = Self::terminal_slot_terminal_id(tab, index, slot);
                        let search_open = terminal_id
                            .as_deref()
                            .is_some_and(|id| self.terminal_search_open.contains(id));
                        TerminalPaneViewSnapshot {
                            terminal_id,
                            view: slot.pane.as_ref().map(|pane| pane.view.clone()),
                            search_open,
                        }
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();

        let top_ratios = terminal_top_ratios_for_panes(
            self.state.terminal_layout.top_ratios.clone(),
            main_panes.len(),
        );
        let top_grid = terminal_top_grid_for_panes(
            self.state.terminal_layout.top_grid.clone(),
            &top_ratios,
            main_panes.len(),
        );
        let split_tree = terminal_split_tree_for_panes(
            self.state.terminal_layout.split_tree.clone(),
            &top_grid,
            &top_ratios,
            main_panes.len(),
        );
        TerminalWorkspaceSnapshot {
            loading: self.terminal_layout_loading,
            language: self.state.settings.language.clone(),
            layout_key: super::ai_runtime_status::current_terminal_layout_storage_key(&self.state)
                .unwrap_or_default(),
            top_ratios,
            top_grid,
            split_tree,
            main_panes,
        }
    }
}
