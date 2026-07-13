use super::*;
use crate::app::{
    ui_helpers::{titlebar_drag_area, with_codux_tooltip},
    workspace_daily_level::workspace_level_button,
    workspace_pet_widgets::{WorkspacePetButtonInput, workspace_pet_button},
    workspace_shared::{
        workspace_header_badge_button_content, workspace_header_button, workspace_i18n,
    },
};
use codux_runtime::{i18n::translate, settings::locale_from_language_setting};
use gpui::Rems;
use gpui_component::menu::{DropdownMenu, PopupMenuItem};

const WORKSPACE_TAB_TEXT_SIZE: Rems = Rems(0.75);
const WORKSPACE_TAB_LINE_HEIGHT: Rems = Rems(1.0);

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
            WorkspaceView::Stats => 3,
        };
        let pet_snapshot = self.pet_snapshot.clone();
        let has_project_context = self.state.selected_project.is_some();
        let remote_project_device_id = self
            .state
            .selected_project
            .as_ref()
            .and_then(|project| project.remote_device_id().map(str::to_string));
        let connected_remote_project_device_id =
            remote_project_device_id.as_ref().and_then(|device_id| {
                (self.remote_link_states.get(device_id)
                    == Some(&codux_runtime::remote::ControllerLinkState::Connected))
                .then(|| device_id.clone())
            });
        let show_server_info_button = has_project_context
            && (remote_project_device_id.is_none() || connected_remote_project_device_id.is_some());
        let pet_button = if self.state.settings.pet_enabled {
            if has_project_context {
                workspace_pet_button(
                    WorkspacePetButtonInput {
                        pet: &self.state.pet,
                        pet_snapshot: Some(&pet_snapshot),
                        custom_pets: &self.pet_custom_pets,
                        runtime_asset_root: &self.runtime.source_root,
                        support_dir: &self.state.support_dir,
                        language: &self.state.settings.language,
                        pet_name_editing: self.pet_name_editing,
                    },
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
            // No `items_center` here: children stretch to full header height so the
            // draggable middle area fills it. Each side group centers its own content.
            div()
                .flex()
                .justify_between()
                .w_full()
                .h_full()
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
                        .child(workspace_toolbar_separator(cx))
                        .child(workspace_open_button(
                            &self.project_open_applications,
                            has_project_context,
                            &self.state.settings.language,
                            cx,
                        ))
                        .when_some(connected_remote_project_device_id, |this, device_id| {
                            this.child(workspace_toolbar_separator(cx)).child(
                                workspace_remote_browser_button(
                                    device_id,
                                    &self.state.settings.language,
                                    cx,
                                ),
                            )
                        })
                        .when(show_server_info_button, |this| {
                            this.child(workspace_toolbar_separator(cx)).child(
                                workspace_assistant_button(
                                    "Server",
                                    AssistantPanel::ServerInfo,
                                    self.assistant_panel,
                                    true,
                                    cx,
                                ),
                            )
                        })
                        .child(workspace_toolbar_separator(cx))
                        .child(workspace_assistant_button(
                            "AI",
                            AssistantPanel::AIStats,
                            self.assistant_panel,
                            has_project_context,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "SSH",
                            AssistantPanel::Ssh,
                            self.assistant_panel,
                            has_project_context,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "DB",
                            AssistantPanel::DB,
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

fn workspace_remote_browser_button(
    device_id: String,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let app_entity = cx.entity();
    let tooltip_entity = app_entity.clone();
    let tooltip = workspace_i18n(
        language,
        "workspace.web_tunnel.browser.open",
        "Open Web Tunnel Browser",
    );

    let button = Button::new("workspace-open-remote-browser")
        .compact()
        .ghost()
        .h(px(28.0))
        .w(px(38.0))
        .cursor_pointer()
        .text_color(cx.theme().secondary_foreground)
        .on_click(move |_, _window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                app.open_remote_project_browser_session(device_id.clone(), cx);
            });
        })
        .child(Icon::new(HeroIconName::GlobeAlt).size_3p5());

    with_codux_tooltip(
        tooltip_entity,
        "workspace-open-remote-browser-tooltip",
        button,
        tooltip,
    )
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
        .child(window_close_control(
            "workspace-window-close",
            30.0,
            false,
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
            AssistantPanel::ServerInfo => "workspace-assistant-server",
            AssistantPanel::Ssh => "workspace-assistant-ssh",
            AssistantPanel::DB => "workspace-assistant-db",
            AssistantPanel::FileManager => "workspace-assistant-files",
            AssistantPanel::Git => "workspace-assistant-git",
        },
        cx,
    );
    let button = if active {
        button
            .ghost()
            .bg(cx.theme().accent)
            .text_color(cx.theme().primary)
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
                        AssistantPanel::AIStats => HeroIconName::CpuChip,
                        AssistantPanel::ServerInfo => HeroIconName::ServerStack,
                        AssistantPanel::Ssh => HeroIconName::CommandLine,
                        AssistantPanel::DB => HeroIconName::CircleStack,
                        AssistantPanel::FileManager => HeroIconName::Folder,
                        AssistantPanel::Git => HeroIconName::Share,
                    })
                    .size_3p5()
                    .text_color(if active {
                        cx.theme().primary
                    } else {
                        cx.theme().secondary_foreground
                    }),
                ),
        );

    with_codux_tooltip(
        cx.entity(),
        match panel {
            AssistantPanel::AIStats => "workspace-assistant-ai-tooltip",
            AssistantPanel::ServerInfo => "workspace-assistant-server-tooltip",
            AssistantPanel::Ssh => "workspace-assistant-ssh-tooltip",
            AssistantPanel::DB => "workspace-assistant-db-tooltip",
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
    let app_entity = cx.entity();
    let locale = locale_from_language_setting(language);
    let terminal_label = translate(&locale, "workspace.create_split.terminal", "Terminal");
    let files_label = translate(&locale, "titlebar.files", "Files");
    let review_label = translate(&locale, "titlebar.review", "Review");
    let stats_label = translate(&locale, "titlebar.stats", "Stats");
    let tabs = [
        (HeroIconName::CommandLine, terminal_label),
        (HeroIconName::Document, files_label),
        (HeroIconName::ArrowPathRoundedSquare, review_label),
        (HeroIconName::ChartBar, stats_label),
    ];
    div()
        .rounded(px(8.0))
        .bg(cx.theme().tab_bar_segmented)
        .p(px(2.0))
        .flex()
        .items_center()
        .gap(px(2.0))
        .when(!enabled, |this| this.opacity(0.45))
        .children(tabs.into_iter().enumerate().map(|(index, (icon, label))| {
            let active = index == active_index;
            let click_entity = app_entity.clone();
            div()
                .id(SharedString::from(format!("workspace-view-tab-{index}")))
                .flex()
                .items_center()
                .gap(px(6.0))
                .h(px(24.0))
                .px(px(12.0))
                .rounded(px(6.0))
                .text_size(WORKSPACE_TAB_TEXT_SIZE)
                .line_height(WORKSPACE_TAB_LINE_HEIGHT)
                .map(|this| {
                    if active {
                        this.bg(cx.theme().primary)
                            .text_color(cx.theme().primary_foreground)
                    } else {
                        this.text_color(cx.theme().tab_foreground)
                            .hover(|style| style.bg(cx.theme().secondary_hover))
                    }
                })
                .when(enabled, |this| {
                    this.cursor_pointer().on_click(move |_, window, cx| {
                        cx.update_entity(&click_entity, |app, cx| {
                            if index == 3 {
                                app.show_stats_workspace_view(window, cx);
                            } else {
                                let view = match index {
                                    0 => WorkspaceView::Terminal,
                                    1 => WorkspaceView::Files,
                                    _ => WorkspaceView::Review,
                                };
                                app.set_workspace_view(view, window, cx);
                            }
                        });
                    })
                })
                .child(Icon::new(icon).size_3p5())
                .child(div().child(label))
        }))
}

fn workspace_toolbar_separator(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .w(px(1.0))
        .h(px(16.0))
        .flex_none()
        .bg(theme::divider_for_surface(cx.theme().title_bar))
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
            color(theme::ACCENT),
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
            color(theme::TEXT_MUTED),
            label,
            cx,
        ))
}
