use super::ai_runtime_status::AIActivityState;
use super::app_state::CoduxTooltipPlacement;
use super::ui_helpers::{codux_tooltip_container_with_placement, titlebar_drag_area};
use super::*;
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};

#[derive(Clone)]
struct ProjectRowDrag {
    project_id: String,
    project: ProjectInfo,
    active: bool,
    collapsed: bool,
}

impl Render for ProjectRowDrag {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .w(px(44.0))
            .h(px(44.0))
            .rounded(px(8.0))
            .flex()
            .items_center()
            .justify_center()
            .child(project_icon(&self.project, self.active, self.collapsed))
    }
}

pub(in crate::app) struct ProjectColumnView {
    pub(in crate::app) app_entity: gpui::Entity<CoduxApp>,
    pub(in crate::app) project_list_state: gpui::Entity<ProjectListState>,
    pub(in crate::app) collapsed: bool,
    pub(in crate::app) language: String,
    pub(in crate::app) has_project: bool,
    pub(in crate::app) has_projects: bool,
    pub(in crate::app) has_worktree: bool,
    pub(in crate::app) scroll_handle: UniformListScrollHandle,
    pub(in crate::app) _observe_project_list_state: Option<Subscription>,
}

pub(in crate::app) struct ProjectListState {
    pub(in crate::app) projects: Rc<Vec<ProjectInfo>>,
    pub(in crate::app) selected_project_id: Option<String>,
    pub(in crate::app) activity: HashMap<String, AIActivityState>,
    revision: u64,
}

impl ProjectListState {
    pub(in crate::app) fn new(
        projects: Vec<ProjectInfo>,
        selected_project_id: Option<String>,
    ) -> Self {
        Self {
            projects: Rc::new(projects),
            selected_project_id,
            activity: HashMap::new(),
            revision: 0,
        }
    }

    pub(in crate::app) fn set_snapshot(
        &mut self,
        projects: Vec<ProjectInfo>,
        selected_project_id: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let same_projects = self.projects.len() == projects.len()
            && self
                .projects
                .iter()
                .zip(projects.iter())
                .all(|(left, right)| {
                    left.id == right.id
                        && left.name == right.name
                        && left.path == right.path
                        && left.exists == right.exists
                        && left.badge == right.badge
                        && left.badge_symbol == right.badge_symbol
                        && left.badge_color_hex == right.badge_color_hex
                });
        if same_projects && self.selected_project_id == selected_project_id {
            return;
        }
        self.projects = Rc::new(projects);
        self.selected_project_id = selected_project_id;
        self.revision = self.revision.wrapping_add(1);
        cx.notify();
    }

    pub(in crate::app) fn set_activity(
        &mut self,
        activity: HashMap<String, AIActivityState>,
        cx: &mut Context<Self>,
    ) {
        if self.activity == activity {
            return;
        }
        self.activity = activity;
        self.revision = self.revision.wrapping_add(1);
        cx.notify();
    }
}

impl Render for ProjectColumnView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let collapsed = self.collapsed;
        let (projects, selected_project_id, activity) =
            self.project_list_state.update(cx, |state, _cx| {
                (
                    state.projects.clone(),
                    state.selected_project_id.clone(),
                    state.activity.clone(),
                )
            });
        let app_entity = self.app_entity.clone();
        let scroll_handle = self.scroll_handle.clone();
        let language = self.language.clone();
        let row_menu_labels = project_row_menu_labels(language.as_str());
        let project_order = projects
            .iter()
            .map(|project| project.id.clone())
            .collect::<Vec<_>>();

        div()
            .flex()
            .flex_col()
            .w(px(if collapsed {
                PROJECT_COLUMN_COLLAPSED_WIDTH
            } else {
                PROJECT_COLUMN_EXPANDED_WIDTH
            }))
            .h_full()
            .bg(cx.theme().sidebar)
            .border_r_1()
            .border_color(cx.theme().sidebar_border)
            .child(project_column_header(collapsed))
            .child(
                div()
                    .id("project-list-scroll")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_h_0()
                    .px(if collapsed { px(7.0) } else { px(10.0) })
                    .pt(if collapsed { px(10.0) } else { px(10.0) })
                    .pb(if collapsed { px(10.0) } else { px(10.0) })
                    .relative()
                    .overflow_hidden()
                    .child(codux_uniform_list(
                        "project-list",
                        projects.clone(),
                        scroll_handle,
                        None,
                        cx,
                        move |project, _index, window, cx| {
                            let project_id = project.id.clone();
                            let active = selected_project_id
                                .as_deref()
                                .map(|selected| selected == project.id)
                                .unwrap_or(false);
                            let activity_state = activity
                                .get(project.id.as_str())
                                .copied()
                                .unwrap_or(AIActivityState::Idle);
                            div()
                                .w_full()
                                .pb(px(4.0))
                                .child(project_row(
                                    project,
                                    active,
                                    app_entity.clone(),
                                    project_id,
                                    project_order.clone(),
                                    activity_state,
                                    collapsed,
                                    row_menu_labels.clone(),
                                    window,
                                    cx,
                                ))
                                .into_any_element()
                        },
                    )),
            )
            .child(project_tools_snapshot(
                collapsed,
                self.language.as_str(),
                self.has_project,
                self.has_projects,
                self.has_worktree,
                self.app_entity.clone(),
                window,
                cx,
            ))
    }
}

fn project_column_header(collapsed: bool) -> impl IntoElement {
    if collapsed {
        titlebar_drag_area(
            "project-column-titlebar-drag-collapsed",
            div()
                .h(px(48.0))
                .flex()
                .items_center()
                .justify_center()
                .when(!cfg!(target_os = "macos"), |this| {
                    this.child(
                        div()
                            .max_w(px(PROJECT_COLUMN_COLLAPSED_WIDTH - 12.0))
                            .overflow_hidden()
                            .text_ellipsis()
                            .text_size(rems(1.0))
                            .line_height(rems(1.25))
                            .text_color(color(theme::TEXT))
                            .child("Codux"),
                    )
                }),
        )
        .into_any_element()
    } else {
        div()
            .h(px(44.0))
            .px(px(10.0))
            .flex()
            .items_center()
            .border_b_1()
            .border_color(color(theme::BORDER_SOFT))
            .child(titlebar_drag_area(
                "project-column-titlebar-drag",
                div()
                    .min_w_0()
                    .flex_1()
                    .h_full()
                    .flex()
                    .items_center()
                    .text_size(rems(1.0))
                    .line_height(rems(1.25))
                    .text_color(color(theme::TEXT))
                    .when(cfg!(target_os = "macos"), |this| this.invisible())
                    .child("Codux"),
            ))
            .into_any_element()
    }
}

fn project_tools_snapshot(
    collapsed: bool,
    language: &str,
    has_project: bool,
    has_projects: bool,
    has_worktree: bool,
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<ProjectColumnView>,
) -> AnyElement {
    let base = div()
        .flex()
        .flex_shrink_0()
        .gap(if collapsed { px(10.0) } else { px(4.0) })
        .px(if collapsed { px(20.0) } else { px(10.0) })
        .py_3();

    if collapsed {
        let add_project_label =
            project_column_text(language, "sidebar.footer.add_project", "Add Project");
        let settings_label = project_column_text(language, "menu.settings", "Settings");
        let more_label = project_column_text(language, "sidebar.footer.more", "More");
        base.flex_col()
            .items_center()
            .child(project_column_toggle_button(
                collapsed,
                language,
                None,
                app_entity.clone(),
                window,
                cx,
            ))
            .child(project_tool_button(
                HeroIconName::Plus,
                None,
                add_project_label,
                "project-add-footer",
                app_entity.clone(),
                window,
                cx,
                |app, _event, window, cx| app.open_project_create_window(window, cx),
            ))
            .child(project_tool_button(
                HeroIconName::Cog6Tooth,
                None,
                settings_label,
                "project-settings-footer",
                app_entity.clone(),
                window,
                cx,
                |app, _event, window, cx| app.open_settings_window(window, cx),
            ))
            .child(project_more_button(
                None,
                more_label,
                language,
                has_project,
                has_projects,
                has_worktree,
                app_entity,
                cx,
            ))
            .into_any_element()
    } else {
        let add_project_label =
            project_column_text(language, "sidebar.footer.add_project", "Add Project");
        let settings_label = project_column_text(language, "menu.settings", "Settings");
        let more_label = project_column_text(language, "sidebar.footer.more", "More");
        let toggle_label = project_column_text(language, "sidebar.collapse", "Collapse Sidebar");
        base.flex_col()
            .items_start()
            .child(project_column_toggle_button(
                collapsed,
                language,
                Some(toggle_label),
                app_entity.clone(),
                window,
                cx,
            ))
            .child(project_tool_button(
                HeroIconName::Plus,
                Some(add_project_label.clone()),
                add_project_label,
                "project-add-footer",
                app_entity.clone(),
                window,
                cx,
                |app, _event, window, cx| app.open_project_create_window(window, cx),
            ))
            .child(project_tool_button(
                HeroIconName::Cog6Tooth,
                Some(settings_label.clone()),
                settings_label,
                "project-settings-footer",
                app_entity.clone(),
                window,
                cx,
                |app, _event, window, cx| app.open_settings_window(window, cx),
            ))
            .child(project_more_button(
                Some(more_label.clone()),
                more_label,
                language,
                has_project,
                has_projects,
                has_worktree,
                app_entity,
                cx,
            ))
            .into_any_element()
    }
}

fn project_tool_button(
    icon: HeroIconName,
    label: Option<String>,
    tooltip: String,
    id: &'static str,
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<ProjectColumnView>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    let has_label = label.is_some();
    let button = Button::new(SharedString::from(format!("project-tool-{id}")))
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .w(if has_label { px(212.0) } else { px(40.0) });

    let button = if has_label {
        button.justify_start()
    } else {
        button
    };

    let button = button
        .on_click(window.listener_for(&app_entity, on_click))
        .child(project_tool_content(icon, label, cx));

    if has_label {
        return button.into_any_element();
    }

    codux_tooltip_container_with_placement(
        app_entity.clone(),
        SharedString::from(format!("project-tool-{id}-tooltip")),
        tooltip,
        CoduxTooltipPlacement::Right,
    )
    .child(button)
    .into_any_element()
}

fn project_tool_content(
    icon: HeroIconName,
    label: Option<String>,
    cx: &mut Context<ProjectColumnView>,
) -> AnyElement {
    if let Some(label) = label {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_start()
            .gap(px(16.0))
            .child(
                div()
                    .w(px(20.0))
                    .flex()
                    .justify_center()
                    .text_color(cx.theme().secondary_foreground)
                    .child(Icon::new(icon).text_color(cx.theme().secondary_foreground)),
            )
            .child(
                div()
                    .text_size(rems(0.75))
                    .text_color(cx.theme().secondary_foreground)
                    .child(label),
            )
            .into_any_element()
    } else {
        div()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(20.0))
                    .flex()
                    .justify_center()
                    .text_color(cx.theme().secondary_foreground)
                    .child(Icon::new(icon).text_color(cx.theme().secondary_foreground)),
            )
            .into_any_element()
    }
}

fn project_more_button(
    label: Option<String>,
    tooltip: String,
    language: &str,
    _has_project: bool,
    _has_projects: bool,
    _has_worktree: bool,
    app_entity: gpui::Entity<CoduxApp>,
    cx: &mut Context<ProjectColumnView>,
) -> impl IntoElement {
    let has_label = label.is_some();
    let language = language.to_string();
    let button = Button::new("project-tool-project-more-footer")
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .w(if has_label { px(212.0) } else { px(40.0) });
    let button = if has_label {
        button.justify_start()
    } else {
        button
    };

    let menu_entity = app_entity.clone();
    let button = button
        .child(project_tool_content(
            HeroIconName::EllipsisHorizontal,
            label,
            cx,
        ))
        .dropdown_menu_with_anchor(gpui::Anchor::BottomLeft, move |menu, _window, _cx| {
            let fallback_entity = menu_entity.clone();
            let about_entity = menu_entity.clone();
            let updates_entity = menu_entity.clone();
            let diagnostics_entity = menu_entity.clone();
            let runtime_log_entity = menu_entity.clone();
            let live_log_entity = menu_entity.clone();
            let open_folder_entity = menu_entity.clone();
            let website_entity = menu_entity.clone();
            let github_entity = menu_entity.clone();
            let entries = project_help_menu_entries(&language);
            entries
                .into_iter()
                .fold(menu, move |menu, entry| match entry {
                    ProjectHelpMenuEntry::Separator => menu.separator(),
                    ProjectHelpMenuEntry::Item {
                        label,
                        icon,
                        action_id,
                    } => {
                        let entity = match action_id {
                            "help:about" => about_entity.clone(),
                            "help:check-updates" => updates_entity.clone(),
                            "help:export-diagnostics" => diagnostics_entity.clone(),
                            "help:runtime-log" => runtime_log_entity.clone(),
                            "help:live-log" => live_log_entity.clone(),
                            "help:open-folder" => open_folder_entity.clone(),
                            "help:website" => website_entity.clone(),
                            "help:github" => github_entity.clone(),
                            _ => fallback_entity.clone(),
                        };
                        menu.item(PopupMenuItem::new(label).icon(icon).on_click(
                            move |_, window, cx| {
                                cx.update_entity(&entity, |app, cx| {
                                    app.apply_project_help_action(action_id, window, cx);
                                });
                            },
                        ))
                    }
                })
        });

    if has_label {
        return button.into_any_element();
    }

    codux_tooltip_container_with_placement(
        app_entity.clone(),
        "project-tool-project-more-footer-tooltip",
        tooltip,
        CoduxTooltipPlacement::Right,
    )
    .child(button)
    .into_any_element()
}

enum ProjectHelpMenuEntry {
    Item {
        label: String,
        icon: HeroIconName,
        action_id: &'static str,
    },
    Separator,
}

fn project_help_menu_entries(language: &str) -> Vec<ProjectHelpMenuEntry> {
    use ProjectHelpMenuEntry::{Item, Separator};
    let label = |key: &str, fallback: &str| project_column_text(language, key, fallback);
    vec![
        Item {
            label: label("menu.file.open_folder", "Open Folder..."),
            icon: HeroIconName::FolderOpen,
            action_id: "help:open-folder",
        },
        Separator,
        Item {
            label: label("menu.app.about_format", "About Codux").replace("%@", "Codux"),
            icon: HeroIconName::InformationCircle,
            action_id: "help:about",
        },
        Item {
            label: label("menu.app.check_updates", "Check for Updates..."),
            icon: HeroIconName::ArrowPath,
            action_id: "help:check-updates",
        },
        Separator,
        Item {
            label: label("menu.help.export_diagnostics", "Export Diagnostics..."),
            icon: HeroIconName::Document,
            action_id: "help:export-diagnostics",
        },
        Item {
            label: label("menu.help.open_runtime_log", "Open Runtime Log"),
            icon: HeroIconName::Document,
            action_id: "help:runtime-log",
        },
        Item {
            label: label("menu.help.open_live_log", "Open Live Log"),
            icon: HeroIconName::Document,
            action_id: "help:live-log",
        },
        Separator,
        Item {
            label: label("menu.help.website", "Official Website"),
            icon: HeroIconName::ArrowTopRightOnSquare,
            action_id: "help:website",
        },
        Item {
            label: label("menu.help.github", "GitHub"),
            icon: HeroIconName::ArrowPathRoundedSquare,
            action_id: "help:github",
        },
    ]
}

fn project_column_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

fn project_column_toggle_button(
    collapsed: bool,
    language: &str,
    label: Option<String>,
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<ProjectColumnView>,
) -> impl IntoElement {
    let icon = if collapsed {
        HeroIconName::ChevronDoubleRight
    } else {
        HeroIconName::ChevronDoubleLeft
    };
    let tooltip = project_column_text(
        language,
        if collapsed {
            "sidebar.expand"
        } else {
            "sidebar.collapse"
        },
        if collapsed {
            "Expand Sidebar"
        } else {
            "Collapse Sidebar"
        },
    );
    let has_label = label.is_some();
    let button = Button::new("project-column-toggle")
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .w(if has_label { px(212.0) } else { px(40.0) })
        .when(has_label, |this| this.justify_start())
        .on_click(window.listener_for(&app_entity, |app, _event, window, cx| {
            app.toggle_project_column(window, cx)
        }))
        .child(project_tool_content(icon, label, cx));

    if has_label {
        return button.into_any_element();
    }

    codux_tooltip_container_with_placement(
        app_entity.clone(),
        "project-column-toggle-tooltip",
        tooltip,
        CoduxTooltipPlacement::Right,
    )
    .child(button)
    .into_any_element()
}

fn project_row(
    project: ProjectInfo,
    active: bool,
    app_entity: gpui::Entity<CoduxApp>,
    project_id: String,
    project_order: Vec<String>,
    activity_state: AIActivityState,
    collapsed: bool,
    labels: ProjectRowMenuLabels,
    window: &mut Window,
    cx: &mut Context<ProjectColumnView>,
) -> AnyElement {
    let menu_project_id = project.id.clone();
    let menu_project_name = project.name.clone();
    let menu_project_path = project.path.clone();
    if collapsed {
        let target_project_id = project.id.clone();
        let drag_project = project.clone();
        let drop_app_entity = app_entity.clone();
        let drop_project_order = project_order.clone();
        return div()
            .id(SharedString::from(format!("project-{}", project.id)))
            .on_drag(
                ProjectRowDrag {
                    project_id: drag_project.id.clone(),
                    project: drag_project,
                    active,
                    collapsed: true,
                },
                move |drag, _, _, cx| {
                    cx.new(|_| ProjectRowDrag {
                        project_id: drag.project_id.clone(),
                        project: drag.project.clone(),
                        active: drag.active,
                        collapsed: drag.collapsed,
                    })
                },
            )
            .drag_over::<ProjectRowDrag>(move |this, _drag, _window, _cx| this)
            .on_drop(cx.listener({
                let target_project_id = target_project_id.clone();
                move |_view, drag: &ProjectRowDrag, window, cx| {
                    let Some(next_project_ids) =
                        reordered_ids(&drop_project_order, &drag.project_id, &target_project_id)
                    else {
                        return;
                    };
                    defer_codux_app_update(
                        drop_app_entity.clone(),
                        window,
                        cx,
                        move |app, _, cx| {
                            app.reorder_projects_by_ids(next_project_ids, cx);
                        },
                    );
                    cx.stop_propagation();
                }
            }))
            .w_full()
            .h(px(44.0))
            .flex()
            .items_center()
            .justify_center()
            .child(
                codux_tooltip_container_with_placement(
                    app_entity.clone(),
                    SharedString::from(format!("project-icon-{}-tooltip", project.id)),
                    project.name.clone(),
                    CoduxTooltipPlacement::Right,
                )
                .child(
                    div()
                        .id(SharedString::from(format!("project-icon-{}", project.id)))
                        .w(px(44.0))
                        .h(px(44.0))
                        .rounded(px(8.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .cursor_pointer()
                        .when(active, |this| this.bg(cx.theme().list_hover))
                        .hover(|style| style.bg(cx.theme().list_hover))
                        .on_click(window.listener_for(
                            &app_entity,
                            move |app, _event, window, cx| {
                                app.select_project(project_id.clone(), window, cx)
                            },
                        ))
                        .context_menu({
                            let app_entity = app_entity.clone();
                            let labels = labels.clone();
                            let project_id = menu_project_id.clone();
                            let project_name = menu_project_name.clone();
                            let project_path = menu_project_path.clone();
                            move |menu, _window, _cx| {
                                project_row_context_menu(
                                    menu,
                                    app_entity.clone(),
                                    project_id.clone(),
                                    project_name.clone(),
                                    project_path.clone(),
                                    labels.clone(),
                                )
                            }
                        })
                        .child(
                            div()
                                .relative()
                                .child(project_icon(&project, active, true))
                                .when(activity_state.is_active(), |this| {
                                    this.child(project_activity_badge(activity_state, cx))
                                }),
                        ),
                ),
            )
            .into_any_element();
    }

    let target_project_id = project.id.clone();
    let drag_project = project.clone();
    let drop_app_entity = app_entity.clone();
    div()
        .id(SharedString::from(format!("project-{}", project.id)))
        .on_drag(
            ProjectRowDrag {
                project_id: drag_project.id.clone(),
                project: drag_project,
                active,
                collapsed: false,
            },
            move |drag, _, _, cx| {
                cx.new(|_| ProjectRowDrag {
                    project_id: drag.project_id.clone(),
                    project: drag.project.clone(),
                    active: drag.active,
                    collapsed: drag.collapsed,
                })
            },
        )
        .drag_over::<ProjectRowDrag>(move |this, _drag, _window, _cx| this)
        .on_drop(cx.listener({
            let target_project_id = target_project_id.clone();
            move |_view, drag: &ProjectRowDrag, window, cx| {
                let Some(next_project_ids) =
                    reordered_ids(&project_order, &drag.project_id, &target_project_id)
                else {
                    return;
                };
                defer_codux_app_update(drop_app_entity.clone(), window, cx, move |app, _, cx| {
                    app.reorder_projects_by_ids(next_project_ids, cx);
                });
                cx.stop_propagation();
            }
        }))
        .w_full()
        .min_w_0()
        .h(px(52.0))
        .flex()
        .flex_col()
        .justify_start()
        .child(
            div()
                .id(SharedString::from(format!(
                    "project-row-inner-{}",
                    project.id
                )))
                .flex()
                .items_center()
                .gap_2()
                .h(px(52.0))
                .w_full()
                .min_w_0()
                .px(px(8.0))
                .rounded(px(8.0))
                .when(active, |this| this.bg(cx.theme().list_hover))
                .cursor_pointer()
                .hover(|style| style.bg(cx.theme().list_hover))
                .on_click(
                    window.listener_for(&app_entity, move |app, _event, window, cx| {
                        app.select_project(project_id.clone(), window, cx)
                    }),
                )
                .context_menu({
                    let app_entity = app_entity.clone();
                    move |menu, _window, _cx| {
                        project_row_context_menu(
                            menu,
                            app_entity.clone(),
                            menu_project_id.clone(),
                            menu_project_name.clone(),
                            menu_project_path.clone(),
                            labels.clone(),
                        )
                    }
                })
                .child(
                    div()
                        .relative()
                        .child(project_icon(&project, active, false))
                        .when(activity_state.is_active(), |this| {
                            this.child(project_activity_badge(activity_state, cx))
                        }),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .min_w_0()
                        .flex_1()
                        .overflow_hidden()
                        .child(
                            div()
                                .text_sm()
                                .text_color(color(if project.exists {
                                    theme::TEXT
                                } else {
                                    theme::TEXT_DIM
                                }))
                                .truncate()
                                .child(project.name.clone()),
                        )
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(project.path.clone()),
                        ),
                ),
        )
        .into_any_element()
}

#[derive(Clone)]
struct ProjectRowMenuLabels {
    open_folder: String,
    edit: String,
    remove: String,
}

fn project_row_menu_labels(language: &str) -> ProjectRowMenuLabels {
    let locale = locale_from_language_setting(language);
    ProjectRowMenuLabels {
        open_folder: translate(&locale, "sidebar.project.open_folder", "Open Folder"),
        edit: translate(&locale, "sidebar.project.edit", "Edit Project"),
        remove: translate(&locale, "sidebar.project.remove", "Remove Project"),
    }
}

fn project_row_context_menu(
    menu: PopupMenu,
    app_entity: gpui::Entity<CoduxApp>,
    project_id: String,
    project_name: String,
    project_path: String,
    labels: ProjectRowMenuLabels,
) -> PopupMenu {
    let open_entity = app_entity.clone();
    let open_name = project_name.clone();
    let open_path = project_path.clone();
    let edit_entity = app_entity.clone();
    let edit_id = project_id.clone();
    let remove_entity = app_entity;

    menu.item(
        PopupMenuItem::new(labels.open_folder.clone())
            .icon(HeroIconName::FolderOpen)
            .on_click(move |_, _window, cx| {
                cx.update_entity(&open_entity, |app, cx| {
                    app.reveal_project_in_file_manager(open_name.clone(), open_path.clone(), cx);
                });
            }),
    )
    .item(
        PopupMenuItem::new(labels.edit.clone())
            .icon(HeroIconName::PencilSquare)
            .on_click(move |_, window, cx| {
                cx.update_entity(&edit_entity, |app, cx| {
                    app.edit_project_by_id(edit_id.clone(), window, cx);
                });
            }),
    )
    .separator()
    .item(
        PopupMenuItem::new(labels.remove)
            .icon(HeroIconName::Trash)
            .on_click(move |_, _window, cx| {
                cx.update_entity(&remove_entity, |app, cx| {
                    app.request_remove_project_by_id(project_id.clone(), cx);
                });
            }),
    )
}

fn project_activity_badge(
    state: AIActivityState,
    _cx: &mut Context<ProjectColumnView>,
) -> AnyElement {
    match state {
        AIActivityState::Running => div()
            .absolute()
            .right(px(-2.0))
            .top(px(-2.0))
            .w(px(10.0))
            .h(px(10.0))
            .rounded_full()
            .bg(color(theme::ORANGE))
            .into_any_element(),
        AIActivityState::Review => div()
            .absolute()
            .right(px(-2.0))
            .top(px(-2.0))
            .w(px(10.0))
            .h(px(10.0))
            .rounded_full()
            .border_2()
            .border_color(color(theme::ORANGE))
            .bg(color(theme::BG_COLUMN))
            .into_any_element(),
        AIActivityState::Done => div()
            .absolute()
            .right(px(-2.0))
            .top(px(-2.0))
            .w(px(10.0))
            .h(px(10.0))
            .rounded_full()
            .bg(color(theme::GREEN))
            .into_any_element(),
        AIActivityState::Idle => div().into_any_element(),
    }
}

fn project_icon(project: &ProjectInfo, active: bool, collapsed: bool) -> impl IntoElement {
    let (background, _accent, text) = match project
        .badge_color_hex
        .as_deref()
        .and_then(project_icon_hex_color)
    {
        Some(base) => project_custom_icon_palette(base, active),
        None => project_icon_palette(&project.id, active),
    };
    let symbol_icon = project
        .badge_symbol
        .as_deref()
        .and_then(project_badge_symbol_icon);
    let badge = project_badge_label(project);
    let size = if collapsed { 36.0 } else { 38.0 };

    div()
        .w(px(size))
        .h(px(size))
        .rounded(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .flex_shrink_0()
        .bg(color(background))
        .text_size(rems(0.875))
        .line_height(rems(0.875))
        .text_color(color(text))
        .font_weight(FontWeight::BOLD)
        .child(match symbol_icon {
            Some(icon) => Icon::new(icon)
                .size_4()
                .text_color(color(text))
                .into_any_element(),
            None => project_badge_text_element(&badge, text),
        })
}

fn project_badge_text_element(badge: &str, text_color: u32) -> AnyElement {
    let chars = badge.chars().take(4).collect::<Vec<_>>();
    let len = chars.len();
    let text_size = match len {
        0 | 1 => rems(0.875),
        2 => rems(0.6875),
        _ => rems(0.5625),
    };

    let content = if len <= 2 {
        div()
            .text_size(text_size)
            .line_height(rems(1.0))
            .child(chars.into_iter().collect::<String>())
            .into_any_element()
    } else {
        let first_line_len = if len == 3 { 1 } else { 2 };
        let first = chars
            .iter()
            .take(first_line_len)
            .copied()
            .collect::<String>();
        let second = chars
            .iter()
            .skip(first_line_len)
            .copied()
            .collect::<String>();
        div()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .text_size(text_size)
            .line_height(rems(0.625))
            .child(div().child(first))
            .child(div().child(second))
            .into_any_element()
    };

    div()
        .flex()
        .items_center()
        .justify_center()
        .text_color(color(text_color))
        .font_weight(FontWeight::BOLD)
        .child(content)
        .into_any_element()
}

fn project_icon_palette(key: &str, active: bool) -> (u32, u32, u32) {
    let active_palettes = [
        (0x39D77A, 0x2CC96D, 0xF6FFF9),
        (0x5276E8, 0x4265CC, 0xEEF3FF),
        (0xF18A5C, 0xD96D45, 0xFFF4ED),
        (0x9B72F4, 0x7755D7, 0xF6F1FF),
        (0x35C7D7, 0x269CAD, 0xF0FDFF),
    ];
    let inactive_palettes = [
        (0x4A8664, 0x3A7458, 0xD6EBDD),
        (0x4A63B8, 0x3F56A1, 0xD8DEF6),
        (0xA7694F, 0x8F5A43, 0xF2DCD2),
        (0x7358A8, 0x624B94, 0xE2D9F3),
        (0x44838B, 0x39747D, 0xD8EFF2),
    ];
    let index = key
        .bytes()
        .fold(0usize, |acc, byte| acc.wrapping_add(byte as usize))
        % active_palettes.len();

    if active {
        active_palettes[index]
    } else {
        inactive_palettes[index]
    }
}

fn project_custom_icon_palette(base: u32, active: bool) -> (u32, u32, u32) {
    if active {
        (mix_rgb(base, 0xFFFFFF, 18), base, 0xFFFFFF)
    } else {
        (
            mix_rgb(base, 0x4A5260, 58),
            mix_rgb(base, 0x242A35, 52),
            0xE3E8EF,
        )
    }
}

fn mix_rgb(base: u32, other: u32, other_percent: u8) -> u32 {
    let other_percent = other_percent.min(100) as u32;
    let base_percent = 100 - other_percent;
    let channel = |shift: u32| {
        let base_value = (base >> shift) & 0xFF;
        let other_value = (other >> shift) & 0xFF;
        ((base_value * base_percent + other_value * other_percent) / 100) & 0xFF
    };
    (channel(16) << 16) | (channel(8) << 8) | channel(0)
}

fn project_icon_hex_color(value: &str) -> Option<u32> {
    let value = value.trim().trim_start_matches('#');
    if value.len() == 6 {
        u32::from_str_radix(value, 16).ok()
    } else {
        None
    }
}

fn project_badge_symbol_icon(symbol: &str) -> Option<HeroIconName> {
    match symbol {
        "terminal" => Some(HeroIconName::CommandLine),
        "folder" => Some(HeroIconName::Folder),
        "shippingbox" | "shippingbox.fill" | "cube.box" | "laptopcomputer" => {
            Some(HeroIconName::Sparkles)
        }
        "hammer" => Some(HeroIconName::WrenchScrewdriver),
        "server.rack" | "globe" => Some(HeroIconName::GlobeAlt),
        "bolt" | "sparkles" => Some(HeroIconName::Star),
        "wrench" | "paintpalette" => Some(HeroIconName::Cog6Tooth),
        "doc.text" => Some(HeroIconName::Document),
        "book" => Some(HeroIconName::BookOpen),
        "person.2" => Some(HeroIconName::UserCircle),
        _ => None,
    }
}

fn project_badge_label(project: &ProjectInfo) -> String {
    let badge = project.badge.trim();
    if badge.is_empty() {
        return project_initial(&project.name);
    }
    badge.chars().take(4).collect::<String>().to_uppercase()
}

fn project_initial(name: &str) -> String {
    name.chars()
        .find(|ch| ch.is_alphanumeric())
        .map(|ch| ch.to_uppercase().collect::<String>())
        .unwrap_or_else(|| "C".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_badge(badge: &str) -> ProjectInfo {
        ProjectInfo {
            id: "project-a".to_string(),
            name: "Project A".to_string(),
            path: "/workspace/project-a".to_string(),
            exists: true,
            badge: badge.to_string(),
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
        }
    }

    #[test]
    fn project_badge_label_prefers_runtime_badge() {
        assert_eq!(project_badge_label(&project_with_badge("cd")), "CD");
        assert_eq!(project_badge_label(&project_with_badge("abcd")), "ABCD");
        assert_eq!(project_badge_label(&project_with_badge("abcde")), "ABCD");
        assert_eq!(project_badge_label(&project_with_badge("项目")), "项目");
        assert_eq!(
            project_badge_label(&project_with_badge("用户中心")),
            "用户中心"
        );
        assert_eq!(project_badge_label(&project_with_badge(" ")), "P");
    }

    #[test]
    fn project_icon_hex_color_accepts_saved_project_colors() {
        assert_eq!(project_icon_hex_color("#0A84FF"), Some(0x0A84FF));
        assert_eq!(project_icon_hex_color("FFB020"), Some(0xFFB020));
        assert_eq!(project_icon_hex_color("bad"), None);
    }
}
