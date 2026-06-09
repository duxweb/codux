use super::*;
use crate::app::{
    ui_helpers::{titlebar_drag_area, with_codux_tooltip},
    workspace_daily_level::workspace_level_button,
    workspace_pet_widgets::workspace_pet_button,
    workspace_shared::{
        workspace_header_badge_button_content, workspace_header_button, workspace_i18n,
    },
};
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui_component::menu::{DropdownMenu, PopupMenuItem};

impl CoduxApp {
    pub(in crate::app) fn workspace_toolbar(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let active_index = match self.workspace_view {
            WorkspaceView::Terminal => 0,
            WorkspaceView::Files => 1,
            WorkspaceView::Review => 2,
        };
        let pet_snapshot = self.pet_snapshot.clone();
        let has_project_context = self.state.selected_project.is_some();
        let pet_sprite_frame = self.visible_pet_sprite_frame(PET_IDLE_FRAME_COUNT);
        let pet_button = if self.state.settings.pet_enabled {
            if has_project_context {
                workspace_pet_button(
                    &self.state.pet,
                    Some(&pet_snapshot),
                    &self.pet_custom_pets,
                    &self.runtime.source_root,
                    &self.state.support_dir,
                    &self.state.settings.language,
                    &self.pet_install_url,
                    &self.pet_install_display_name,
                    self.pet_install_preview.as_ref(),
                    self.pet_install_error.as_deref(),
                    self.pet_install_previewing,
                    self.pet_installing,
                    self.pet_name_editing,
                    pet_sprite_frame,
                    window,
                    cx,
                )
                .into_any_element()
            } else {
                disabled_pet_button(&self.state, cx).into_any_element()
            }
        } else {
            gpui::Empty.into_any_element()
        };
        let level_button = if has_project_context {
            workspace_level_button(&self.state.daily_level, &self.state.settings.language, cx)
                .into_any_element()
        } else {
            disabled_level_button(&self.state.settings.language, cx).into_any_element()
        };
        column_header(
            div()
                .flex()
                .items_center()
                .justify_between()
                .w_full()
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(workspace_segmented_tabs(
                            active_index,
                            &self.state.settings.language,
                            has_project_context,
                            cx,
                        )),
                )
                .child(titlebar_drag_area(
                    "workspace-titlebar-drag",
                    div().flex_1().h_full(),
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(pet_button)
                        .child(level_button)
                        .child(workspace_open_button(
                            &self.project_open_applications,
                            has_project_context,
                            &self.state.settings.language,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "AI",
                            AssistantPanel::AIStats,
                            self.assistant_panel,
                            has_project_context,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "SSH",
                            AssistantPanel::SSH,
                            self.assistant_panel,
                            has_project_context,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "Files",
                            AssistantPanel::FileManager,
                            self.assistant_panel,
                            has_project_context,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "Git",
                            AssistantPanel::Git,
                            self.assistant_panel,
                            has_project_context,
                            cx,
                        ))
                        .when(!cfg!(target_os = "macos"), |this| {
                            this.child(workspace_window_controls(cx))
                        }),
                ),
            cx,
        )
    }
}

fn workspace_open_button(
    applications: &[ProjectOpenApplicationSummary],
    has_project: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let applications = applications
        .iter()
        .filter(|application| application.installed)
        .cloned()
        .collect::<Vec<_>>();
    let app_entity = cx.entity();
    let reveal_entity = app_entity.clone();
    let language = language.to_string();

    div()
        .flex()
        .items_center()
        .rounded(px(6.0))
        .overflow_hidden()
        .bg(cx.theme().secondary)
        .child(
            div()
                .id("workspace-open-folder")
                .h(px(28.0))
                .w(px(38.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .when(!has_project, |this| this.opacity(0.45))
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(move |_, window, cx| {
                    if has_project {
                        cx.update_entity(&reveal_entity, |app, cx| {
                            app.reveal_selected_project_in_file_manager(window, cx);
                        });
                    }
                })
                .child(
                    Icon::new(HeroIconName::FolderOpen)
                        .size_3p5()
                        .text_color(cx.theme().foreground),
                ),
        )
        .child(div().w(px(1.0)).h(px(18.0)).bg(cx.theme().border))
        .child(
            Button::new("workspace-open-apps")
                .text()
                .h(px(28.0))
                .w(px(30.0))
                .cursor_pointer()
                .text_color(cx.theme().foreground)
                .child(
                    Icon::new(HeroIconName::ChevronDown)
                        .size_2()
                        .text_color(cx.theme().foreground),
                )
                .dropdown_menu(move |menu, _window, _cx| {
                    if applications.is_empty() {
                        let label = workspace_i18n(
                            &language,
                            "workspace.open.installed_apps_empty",
                            "No installed apps",
                        );
                        menu.item(
                            PopupMenuItem::new(label).icon(HeroIconName::ArrowTopRightOnSquare),
                        )
                    } else {
                        applications.iter().fold(menu, |menu, application| {
                            let app_entity = app_entity.clone();
                            let application_id = application.id.clone();
                            menu.item(
                                PopupMenuItem::new(application.label.clone())
                                    .icon(if application.category == "primary" {
                                        HeroIconName::ArrowTopRightOnSquare
                                    } else {
                                        HeroIconName::Document
                                    })
                                    .disabled(!has_project)
                                    .on_click(move |_, window, cx| {
                                        cx.update_entity(&app_entity, |app, cx| {
                                            app.open_selected_project_in_application(
                                                application_id.clone(),
                                                window,
                                                cx,
                                            );
                                        });
                                    }),
                            )
                        })
                    }
                }),
        )
}

fn workspace_window_controls(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .ml(px(4.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .child(workspace_window_control_button(
            "workspace-window-minimize",
            HeroIconName::Minus,
            WindowControlArea::Min,
            cx,
        ))
        .child(workspace_window_control_button(
            "workspace-window-zoom",
            HeroIconName::Window,
            WindowControlArea::Max,
            cx,
        ))
        .child(workspace_window_control_button(
            "workspace-window-close",
            HeroIconName::XMark,
            WindowControlArea::Close,
            cx,
        ))
}

fn workspace_window_control_button(
    id: &'static str,
    icon: HeroIconName,
    area: WindowControlArea,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    Button::new(id)
        .compact()
        .ghost()
        .h(px(28.0))
        .w(px(30.0))
        .text_color(cx.theme().muted_foreground)
        .window_control_area(area)
        .child(Icon::new(icon).size_3())
}

fn workspace_assistant_button(
    label: &'static str,
    panel: AssistantPanel,
    active_panel: Option<AssistantPanel>,
    enabled: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let active = enabled && active_panel == Some(panel);

    let button = workspace_header_button(
        match panel {
            AssistantPanel::AIStats => "workspace-assistant-ai",
            AssistantPanel::SSH => "workspace-assistant-ssh",
            AssistantPanel::FileManager => "workspace-assistant-files",
            AssistantPanel::Git => "workspace-assistant-git",
        },
        cx,
    );
    let button = if active {
        button.secondary().text_color(cx.theme().foreground)
    } else {
        button.ghost().text_color(cx.theme().secondary_foreground)
    };

    let button = button
        .disabled(!enabled)
        .when(!enabled, |this| this.opacity(0.45))
        .when(enabled, |this| {
            this.on_click(cx.listener(move |app, _event, window, cx| {
                app.toggle_assistant_panel(panel, window, cx)
            }))
        })
        .child(
            div()
                .h(px(20.0))
                .w(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(match panel {
                        AssistantPanel::AIStats => HeroIconName::Sparkles,
                        AssistantPanel::SSH => HeroIconName::CommandLine,
                        AssistantPanel::FileManager => HeroIconName::Folder,
                        AssistantPanel::Git => HeroIconName::ArrowPathRoundedSquare,
                    })
                    .size_3p5()
                    .text_color(if active {
                        cx.theme().foreground
                    } else {
                        cx.theme().secondary_foreground
                    }),
                ),
        );

    with_codux_tooltip(
        cx.entity(),
        match panel {
            AssistantPanel::AIStats => "workspace-assistant-ai-tooltip",
            AssistantPanel::SSH => "workspace-assistant-ssh-tooltip",
            AssistantPanel::FileManager => "workspace-assistant-files-tooltip",
            AssistantPanel::Git => "workspace-assistant-git-tooltip",
        },
        button,
        label,
    )
}
fn workspace_segmented_tabs(
    active_index: usize,
    language: &str,
    enabled: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let locale = locale_from_language_setting(language);
    let terminal_label = translate(&locale, "workspace.create_split.terminal", "Terminal");
    let files_label = translate(&locale, "titlebar.files", "Files");
    let review_label = translate(&locale, "titlebar.review", "Review");
    div()
        .flex()
        .items_center()
        .gap_1()
        .h(px(32.0))
        .p(px(4.0))
        .rounded_sm()
        .bg(cx.theme().secondary)
        .when(!enabled, |this| this.opacity(0.45))
        .child(workspace_segmented_tab(
            0,
            terminal_label,
            HeroIconName::CommandLine,
            active_index == 0,
            enabled,
            cx,
        ))
        .child(workspace_segmented_tab(
            1,
            files_label,
            HeroIconName::Document,
            active_index == 1,
            enabled,
            cx,
        ))
        .child(workspace_segmented_tab(
            2,
            review_label,
            HeroIconName::ArrowPathRoundedSquare,
            active_index == 2,
            enabled,
            cx,
        ))
}

fn workspace_segmented_tab(
    index: usize,
    label: impl Into<SharedString>,
    icon: HeroIconName,
    active: bool,
    enabled: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let label = label.into();
    let active_bg = cx.theme().primary.opacity(0.14);
    let inactive_hover_bg = cx.theme().secondary_hover;
    div()
        .id(SharedString::from(format!("workspace-view-tab-{index}")))
        .h(px(22.0))
        .px_3()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_color(if active {
            cx.theme().primary
        } else {
            cx.theme().secondary_foreground
        })
        .bg(if active {
            active_bg
        } else {
            cx.theme().transparent
        })
        .when(enabled, |this| {
            this.cursor_pointer()
                .hover(move |style| {
                    if active {
                        style.bg(active_bg)
                    } else {
                        style.bg(inactive_hover_bg)
                    }
                })
                .on_click(cx.listener(move |app, _event, window, cx| {
                    let view = match index {
                        0 => WorkspaceView::Terminal,
                        1 => WorkspaceView::Files,
                        _ => WorkspaceView::Review,
                    };
                    app.set_workspace_view(view, window, cx);
                }))
        })
        .child(
            div()
                .h(px(16.0))
                .flex()
                .items_center()
                .justify_center()
                .gap_2()
                .child(
                    div()
                        .size(px(14.0))
                        .flex()
                        .flex_none()
                        .items_center()
                        .justify_center()
                        .child(Icon::new(icon).size_3()),
                )
                .child(
                    div()
                        .flex_none()
                        .mt(px(1.0))
                        .text_size(rems(0.75))
                        .child(label),
                ),
        )
}

fn disabled_pet_button(state: &RuntimeState, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    let label = if state.pet.claimed {
        format!("Lv.{}", state.pet.level.max(1))
    } else {
        workspace_i18n(&state.settings.language, "pet.claim.action", "Claim Pet")
    };

    workspace_header_button("workspace-pet-disabled", cx)
        .secondary()
        .disabled(true)
        .opacity(0.45)
        .text_color(cx.theme().foreground)
        .child(workspace_header_badge_button_content(
            HeroIconName::Heart,
            color(0x7C4DFF),
            label,
            cx,
        ))
}

fn disabled_level_button(language: &str, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    let label = workspace_i18n(language, "rank.iron", "Iron");

    workspace_header_button("workspace-level-disabled", cx)
        .secondary()
        .disabled(true)
        .opacity(0.45)
        .text_color(cx.theme().foreground)
        .child(workspace_header_badge_button_content(
            HeroIconName::Minus,
            color(0x5B616D),
            label,
            cx,
        ))
}
