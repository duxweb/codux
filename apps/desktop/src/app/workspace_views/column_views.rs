use super::*;

pub(in crate::app) struct WorkspaceToolbarView {
    app_entity: gpui::Entity<CoduxApp>,
    snapshot: WorkspaceToolbarSnapshot,
}

impl WorkspaceToolbarView {
    pub(in crate::app) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: WorkspaceToolbarSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: WorkspaceToolbarSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for WorkspaceToolbarView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .w_full()
            .h_full()
            .child(self.app_entity.update(cx, |app, cx| {
                app.workspace_toolbar(window, cx).into_any_element()
            }))
    }
}

pub(in crate::app) struct WorkspaceBodyView {
    app_entity: gpui::Entity<CoduxApp>,
    pub(in crate::app) terminal_workspace_view: Option<gpui::Entity<TerminalWorkspaceView>>,
    pub(in crate::app) file_editor_workspace_view:
        Option<gpui::Entity<file_editor::FileEditorWorkspaceView>>,
    pub(in crate::app) review_workspace_view: Option<gpui::Entity<ReviewWorkspaceView>>,
    pub(in crate::app) stats_workspace_view: Option<gpui::Entity<StatsWorkspaceView>>,
}

impl WorkspaceBodyView {
    pub(in crate::app) fn new(app_entity: gpui::Entity<CoduxApp>) -> Self {
        Self {
            app_entity,
            terminal_workspace_view: None,
            file_editor_workspace_view: None,
            review_workspace_view: None,
            stats_workspace_view: None,
        }
    }
}

impl Render for WorkspaceBodyView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let app_entity = self.app_entity.clone();
        self.app_entity.update(cx, |app, app_cx| {
            if app.state.selected_project.is_none() && app.workspace_view != WorkspaceView::Stats {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                self.terminal_workspace_view = None;
                self.file_editor_workspace_view = None;
                self.review_workspace_view = None;
                self.stats_workspace_view = None;
                return app
                    .empty_project_workspace(window, app_cx)
                    .into_any_element();
            }
            if app.workspace_view == WorkspaceView::Terminal {
                let snapshot = app.terminal_workspace_snapshot();
                let terminal_view = if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(true, cx));
                    view.update(app_cx, |view, cx| view.set_snapshot(snapshot, cx));
                    view.clone()
                } else {
                    let view =
                        app_cx.new(|_| TerminalWorkspaceView::new(app_entity.clone(), snapshot));
                    self.terminal_workspace_view = Some(view.clone());
                    view
                };
                let split_file_editor = app.workspace_split == Some(WorkspaceSplitKind::FileEditor)
                    && !app.file_editor_tabs.is_empty();
                if !split_file_editor {
                    return workspace_body_any_view(terminal_view).into_any_element();
                }
                // Split mode: terminal on the left, the existing file-editor
                // workspace (with its own tab bar) on the right, in a draggable
                // horizontal split. h_resizable persists the dragged sizes in
                // its own keyed state for the session.
                let fe_snapshot = app.file_editor_workspace_snapshot();
                let file_editor_view = if let Some(view) = &self.file_editor_workspace_view {
                    view.update(app_cx, |view, cx| view.set_snapshot(fe_snapshot, cx));
                    view.clone()
                } else {
                    let view = app_cx.new(|_| {
                        file_editor::FileEditorWorkspaceView::new(app_entity.clone(), fe_snapshot)
                    });
                    self.file_editor_workspace_view = Some(view.clone());
                    view
                };
                // Both panels carry equal flex (no fixed `.size`), so the split
                // defaults to an even 50/50 — matching the terminal splits —
                // while h_resizable still lets the user drag it. The "close
                // split" control lives in the file editor's tab bar (a dedicated
                // slot), so it never overlaps the tabs.
                h_resizable("workspace-body-file-split")
                    .child(
                        resizable_panel()
                            .size_range(px(320.0)..Pixels::MAX)
                            .child(gpui::AnyView::from(terminal_view)),
                    )
                    .child(
                        resizable_panel()
                            .size_range(px(320.0)..Pixels::MAX)
                            .child(gpui::AnyView::from(file_editor_view)),
                    )
                    .into_any_element()
            } else if app.workspace_view == WorkspaceView::Files {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                let snapshot = app.file_editor_workspace_snapshot();
                let file_editor_view = if let Some(view) = &self.file_editor_workspace_view {
                    view.clone()
                } else {
                    let view = app_cx.new(|_| {
                        file_editor::FileEditorWorkspaceView::new(app_entity.clone(), snapshot)
                    });
                    self.file_editor_workspace_view = Some(view.clone());
                    view
                };
                workspace_body_any_view(file_editor_view).into_any_element()
            } else if app.workspace_view == WorkspaceView::Review {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                let snapshot = app.review_workspace_snapshot();
                let review_view = if let Some(view) = &self.review_workspace_view {
                    view.clone()
                } else {
                    let view =
                        app_cx.new(|_| ReviewWorkspaceView::new(app_entity.clone(), snapshot));
                    self.review_workspace_view = Some(view.clone());
                    view
                };
                workspace_body_any_view(review_view).into_any_element()
            } else if app.workspace_view == WorkspaceView::Stats {
                if let Some(view) = &self.terminal_workspace_view {
                    view.update(app_cx, |view, cx| view.set_render_visible(false, cx));
                }
                let snapshot = app.stats_workspace_snapshot();
                let stats_view = if let Some(view) = &self.stats_workspace_view {
                    view.update(app_cx, |view, cx| view.set_snapshot(snapshot, cx));
                    view.clone()
                } else {
                    let view = app_cx.new(|cx| {
                        StatsWorkspaceView::new(app_entity.clone(), snapshot, window, cx)
                    });
                    self.stats_workspace_view = Some(view.clone());
                    view
                };
                workspace_body_any_view(stats_view).into_any_element()
            } else {
                app.workspace_body(window, app_cx).into_any_element()
            }
        })
    }
}
pub(in crate::app) struct WorkspaceAssistantView {
    app_entity: gpui::Entity<CoduxApp>,
    pub(super) snapshot: WorkspaceAssistantSnapshot,
}

impl WorkspaceAssistantView {
    pub(in crate::app) fn new(
        app_entity: gpui::Entity<CoduxApp>,
        snapshot: WorkspaceAssistantSnapshot,
    ) -> Self {
        Self {
            app_entity,
            snapshot,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        snapshot: WorkspaceAssistantSnapshot,
        cx: &mut Context<Self>,
    ) {
        if self.snapshot == snapshot {
            return;
        }
        self.snapshot = snapshot;
        cx.notify();
    }
}

impl Render for WorkspaceAssistantView {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let snapshot = self.snapshot.clone();
        let app_entity = self.app_entity.clone();
        div().when_some(snapshot.panel, |this, panel| {
            this.when(assistant_panel_available(panel, &snapshot), |this| {
                this.flex()
                    .flex_col()
                    .flex_none()
                    .flex_shrink_0()
                    .w(px(ASSISTANT_PANEL_WIDTH))
                    .min_w(px(ASSISTANT_PANEL_WIDTH))
                    .max_w(px(ASSISTANT_PANEL_WIDTH))
                    .h_full()
                    .bg(theme::vibrancy_panel(color(theme::BG_COLUMN)))
                    .border_l_1()
                    .border_color(cx.theme().sidebar_border)
                    .child(match panel {
                        AssistantPanel::AIStats => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.ai_stats_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::ServerInfo => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.server_info_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::Ssh => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.ssh_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::DB => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.db_sidebar_view(cx)).into_any_element()
                        }),
                        AssistantPanel::FileManager => self.app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.file_sidebar_view(cx))
                                .cached(
                                    gpui::StyleRefinement::default()
                                        .flex()
                                        .flex_col()
                                        .w_full()
                                        .h_full()
                                        .min_h_0(),
                                )
                                .into_any_element()
                        }),
                        AssistantPanel::Git => app_entity.update(cx, |app, cx| {
                            gpui::AnyView::from(app.git_sidebar_view(cx)).into_any_element()
                        }),
                    })
            })
        })
    }
}

pub(super) fn assistant_panel_available(
    panel: AssistantPanel,
    snapshot: &WorkspaceAssistantSnapshot,
) -> bool {
    match panel {
        AssistantPanel::Ssh => true,
        AssistantPanel::ServerInfo => snapshot.has_project,
        _ => snapshot.has_project,
    }
}

pub(super) fn workspace_toolbar_fingerprint(app: &CoduxApp) -> u64 {
    workspace_view_hash(&(
        workspace_view_key(app.workspace_view),
        assistant_panel_key(app.assistant_panel),
        app.state.selected_project.as_ref().map(|project| {
            (
                project.id.clone(),
                project.name.clone(),
                project.path.clone(),
                project.runtime_target.clone(),
            )
        }),
        app.state.settings.language.clone(),
        app.state.settings.pet_enabled,
        !app.state.projects.is_empty(),
        app.project_open_applications
            .iter()
            .filter(|application| application.installed)
            .map(|application| {
                (
                    application.id.clone(),
                    application.label.clone(),
                    application.category.clone(),
                )
            })
            .collect::<Vec<_>>(),
        workspace_pet_fingerprint(app),
        workspace_view_hash(&(
            app.state.daily_level.tokens,
            app.state.daily_level.current_tier.id.clone(),
            app.state.daily_level.current_tier.min,
        )),
    ))
}

pub(in crate::app) fn workspace_view_hash<T: std::hash::Hash + ?Sized>(value: &T) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    std::hash::Hash::hash(value, &mut hasher);
    std::hash::Hasher::finish(&hasher)
}

fn workspace_view_key(view: WorkspaceView) -> &'static str {
    match view {
        WorkspaceView::Terminal => "terminal",
        WorkspaceView::Files => "files",
        WorkspaceView::Review => "review",
        WorkspaceView::Stats => "stats",
    }
}

fn assistant_panel_key(panel: Option<AssistantPanel>) -> &'static str {
    match panel {
        Some(AssistantPanel::AIStats) => "ai_stats",
        Some(AssistantPanel::ServerInfo) => "server_info",
        Some(AssistantPanel::Ssh) => "ssh",
        Some(AssistantPanel::DB) => "db",
        Some(AssistantPanel::FileManager) => "file_manager",
        Some(AssistantPanel::Git) => "git",
        None => "none",
    }
}

fn workspace_pet_fingerprint(app: &CoduxApp) -> u64 {
    workspace_view_hash(&[
        workspace_view_hash(&(
            app.state.pet.available,
            app.state.pet.claimed,
            app.state.pet.species.clone(),
            app.state.pet.display_name.clone(),
            app.state.pet.custom_name.clone(),
        )),
        workspace_view_hash(&(
            app.state.pet.level,
            app.state.pet.total_xp,
            app.state.pet.daily_xp,
            app.state.pet.archived_count,
            app.state.pet.custom_pet_count,
            app.state.pet.updated_at,
        )),
        workspace_view_hash(&(app.pet_snapshot.updated_at, app.pet_snapshot.progress.level)),
        workspace_view_hash(
            &app.pet_custom_pets
                .iter()
                .map(|pet| (pet.id.clone(), pet.display_name.clone(), pet.installed_at))
                .collect::<Vec<_>>(),
        ),
        workspace_view_hash(&(
            app.pet_install_url.clone(),
            app.pet_install_display_name.clone(),
            app.pet_install_error.clone(),
            app.pet_install_previewing,
            app.pet_installing,
        )),
        workspace_view_hash(&app.pet_install_preview.as_ref().map(|preview| {
            (
                preview.page_url.clone(),
                preview.zip_url.clone(),
                preview.slug.clone(),
                preview.display_name.clone(),
                preview.image_url.clone(),
                preview.local_image_path.clone(),
            )
        })),
        workspace_view_hash(&(
            app.pet_name_editing,
            app.visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT),
        )),
    ])
}
