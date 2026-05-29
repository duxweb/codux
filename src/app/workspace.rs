use super::*;
use gpui::{Anchor, relative};
use gpui_component::{
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    popover::Popover,
    resizable::{resizable_panel, v_resizable},
};

impl CoduxApp {
    pub(in crate::app) fn main_workspace_column(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .h_full()
            .bg(color(theme::BG))
            .child(self.workspace_toolbar(window, cx))
            .child(
                div()
                    .flex()
                    .flex_1()
                    .min_h_0()
                    .child(div().flex().flex_col().flex_1().min_w_0().child(
                        match self.workspace_view {
                            WorkspaceView::Terminal => {
                                self.terminal_workspace_body(cx).into_any_element()
                            }
                            WorkspaceView::Files => {
                                self.files_workspace_body(window, cx).into_any_element()
                            }
                            WorkspaceView::Review => {
                                self.review_workspace_body(cx).into_any_element()
                            }
                        },
                    ))
                    .child(self.assistant_column(window, cx)),
            )
    }

    fn workspace_toolbar(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_index = match self.workspace_view {
            WorkspaceView::Terminal => 0,
            WorkspaceView::Files => 1,
            WorkspaceView::Review => 2,
        };
        let pet_snapshot = self.runtime_service.pet_snapshot().ok();
        let today_level_tokens = workspace_today_level_tokens(&self.state);
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
                        .child(workspace_segmented_tabs(active_index, cx)),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(workspace_open_button(
                            &self.project_open_applications,
                            self.state.selected_project.is_some(),
                            cx,
                        ))
                        .child(workspace_pet_button(
                            &self.state.pet,
                            pet_snapshot.as_ref(),
                            &self.pet_custom_pets,
                            &self.runtime.source_root,
                            &self.state.support_dir,
                            &self.pet_install_url,
                            &self.pet_install_display_name,
                            self.pet_install_preview.as_ref(),
                            self.pet_install_previewing,
                            self.pet_installing,
                            window,
                            cx,
                        ))
                        .child(workspace_level_button(today_level_tokens, cx))
                        .child(workspace_assistant_button(
                            "AI",
                            AssistantPanel::AIStats,
                            self.assistant_panel,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "SSH",
                            AssistantPanel::SSH,
                            self.assistant_panel,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "Files",
                            AssistantPanel::FileManager,
                            self.assistant_panel,
                            cx,
                        ))
                        .child(workspace_assistant_button(
                            "Git",
                            AssistantPanel::Git,
                            self.assistant_panel,
                            cx,
                        )),
                ),
        )
    }

    fn terminal_workspace_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .bg(color(theme::BG_TERMINAL))
            .child(
                v_resizable("workspace-terminal-split")
                    .child(
                        resizable_panel()
                            .size(px(420.0))
                            .size_range(px(220.0)..px(900.0))
                            .child(self.terminal_main_split_area(cx)),
                    )
                    .child(
                        resizable_panel()
                            .size(px(220.0))
                            .size_range(px(44.0)..px(520.0))
                            .child(self.terminal_bottom_tabs_area(cx)),
                    ),
            )
    }

    fn terminal_main_split_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(self.terminal_panes(cx))
    }

    fn terminal_bottom_tabs_area(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.active_terminal();

        div()
            .flex()
            .flex_col()
            .size_full()
            .min_h_0()
            .child(
                div()
                    .h(px(40.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .px_2()
                    .border_t_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .children(self.terminals.iter().map(|terminal| {
                                terminal_bottom_tab_button(
                                    terminal.id,
                                    terminal.label.clone(),
                                    terminal.id == self.active_terminal_id,
                                    cx,
                                )
                                .into_any_element()
                            })),
                    )
                    .child(terminal_bottom_add_button(cx)),
            )
            .child(
                div().flex_1().min_h_0().child(match active {
                    Some(tab) => terminal_bottom_summary(tab).into_any_element(),
                    None => div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_xs()
                        .text_color(color(theme::TEXT_DIM))
                        .child("No terminal")
                        .into_any_element(),
                }),
            )
    }

    fn files_workspace_body(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        div()
            .flex()
            .flex_1()
            .bg(color(theme::BG))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(292.0))
                    .border_r_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .child(
                        div()
                            .h(px(34.0))
                            .px_3()
                            .flex()
                            .items_center()
                            .gap_1()
                            .border_b_1()
                            .border_color(color(theme::BORDER_SOFT))
                            .child(header_icon_button(
                                "files-up",
                                IconName::ArrowUp,
                                cx,
                                |app, _event, window, cx| {
                                    app.open_parent_file_directory(window, cx)
                                },
                            ))
                            .child(header_icon_button(
                                "files-new",
                                IconName::Plus,
                                cx,
                                |app, _event, window, cx| app.create_project_file(window, cx),
                            ))
                            .child(header_icon_button(
                                "files-import",
                                IconName::ExternalLink,
                                cx,
                                |app, _event, window, cx| {
                                    app.import_external_file_entries(window, cx)
                                },
                            ))
                            .child(header_icon_button(
                                "files-save",
                                IconName::Check,
                                cx,
                                |app, _event, window, cx| {
                                    app.save_selected_file_preview(window, cx)
                                },
                            )),
                    )
                    .child(file_section(
                        self.state
                            .selected_project
                            .as_ref()
                            .map(|project| project.name.as_str())
                            .unwrap_or("Project"),
                        &self.state.files,
                        &self.file_tree_children,
                        &self.file_tree_expanded_dirs,
                        &self.file_directory,
                        self.selected_file_entry.as_deref(),
                        self.file_name_draft_kind,
                        &self.file_name_draft_value,
                        window,
                        cx,
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .bg(color(theme::BG_TERMINAL))
                    .child(file_preview_workspace(
                        &self.file_preview,
                        self.file_editable,
                        self.file_dirty,
                        self.file_search_open,
                        &self.file_search_query,
                        self.file_search_match_index,
                        window,
                        cx,
                    )),
            )
    }

    fn review_workspace_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .flex()
            .flex_col()
            .flex_1()
            .bg(color(theme::BG))
            .child(
                div()
                    .h(px(44.0))
                    .px_3()
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_3()
                            .child(metric_inline("Branch", self.state.git.branch.clone()))
                            .child(metric_inline(
                                "Files",
                                self.state.git.changed_files.len().to_string(),
                            ))
                            .child(metric_inline("Staged", self.state.git.staged.to_string())),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_1()
                            .child(workspace_text_button(
                                "review-commit",
                                "Commit",
                                cx,
                                |app, _event, window, cx| app.commit_staged_git(window, cx),
                            ))
                            .child(workspace_text_button(
                                "review-push",
                                "Push",
                                cx,
                                |app, _event, window, cx| app.push_project_git(window, cx),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .w(px(360.0))
                            .border_r_1()
                            .border_color(color(theme::BORDER_SOFT))
                            .child(git_workspace_section(
                                &self.state.git,
                                self.selected_git_file.as_deref(),
                                self.selected_git_branch.as_deref(),
                                cx,
                            )),
                    )
                    .child(div().flex_1().child(git_review_workspace(
                        self.selected_git_file.as_deref(),
                        &self.git_diff_preview,
                        self.git_review_content.as_ref(),
                    ))),
            )
    }

    pub(in crate::app) fn terminal_panes(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(active) = self.active_terminal() else {
            return div()
                .flex_1()
                .flex()
                .items_center()
                .justify_center()
                .text_color(color(theme::TEXT_DIM))
                .child("No terminal");
        };

        div()
            .flex()
            .flex_1()
            .overflow_hidden()
            .children(active.panes.iter().enumerate().map(|(index, slot)| {
                let close_id = SharedString::from(format!("terminal-pane-close-{index}"));
                div()
                    .relative()
                    .group("terminal-pane")
                    .flex()
                    .flex_col()
                    .flex_1()
                    .overflow_hidden()
                    .border_l_1()
                    .border_color(color(if index == 0 {
                        theme::BG_TERMINAL
                    } else {
                        theme::BORDER_SOFT
                    }))
                    .child(
                        div()
                            .absolute()
                            .top_1()
                            .right_1()
                            .invisible()
                            .flex()
                            .items_center()
                            .gap_1()
                            .group_hover("terminal-pane", |style| style.visible())
                            .child(
                                Button::new(SharedString::from(format!(
                                    "terminal-pane-add-{index}"
                                )))
                                .ghost()
                                .text_color(cx.theme().secondary_foreground)
                                .icon(
                                    Icon::new(IconName::Plus)
                                        .text_color(cx.theme().secondary_foreground),
                                )
                                .on_click(cx.listener(
                                    move |app, _event, window, cx| app.split_terminal(window, cx),
                                )),
                            )
                            .child(
                                Button::new(close_id)
                                    .ghost()
                                    .text_color(cx.theme().secondary_foreground)
                                    .icon(
                                        Icon::new(IconName::Close)
                                            .text_color(cx.theme().secondary_foreground),
                                    )
                                    .on_click(cx.listener(move |app, _event, window, cx| {
                                        app.close_terminal_pane(index, window, cx)
                                    })),
                            ),
                    )
                    .child(div().flex_1().child(slot.pane.view.clone()))
                    .into_any_element()
            }))
    }
}

fn metric_inline(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .text_xs()
        .child(div().text_color(color(theme::TEXT_DIM)).child(label))
        .child(
            div()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child(value),
        )
}

fn workspace_open_button(
    applications: &[ProjectOpenApplicationSummary],
    has_project: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let applications = applications.to_vec();
    let app_entity = cx.entity();

    workspace_header_button("workspace-open", cx)
        .secondary()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .h(px(20.0))
                .flex()
                .items_center()
                .gap_1()
                .text_color(cx.theme().foreground)
                .child(
                    Icon::new(IconName::ExternalLink)
                        .size_3p5()
                        .text_color(cx.theme().foreground),
                )
                .child(
                    div()
                        .w(px(1.0))
                        .h(px(14.0))
                        .bg(color(0xFFFFFF).opacity(0.10)),
                )
                .child(
                    Icon::new(IconName::ChevronDown)
                        .size_2()
                        .text_color(cx.theme().foreground),
                ),
        )
        .dropdown_menu(move |menu, _window, _cx| {
            let reveal_entity = app_entity.clone();
            let refresh_entity = app_entity.clone();
            let menu = menu
                .item(
                    PopupMenuItem::new("在文件管理器中显示")
                        .icon(IconName::FolderOpen)
                        .disabled(!has_project)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&reveal_entity, |app, cx| {
                                app.reveal_selected_project_in_file_manager(window, cx);
                            });
                        }),
                )
                .item(
                    PopupMenuItem::new("刷新应用列表")
                        .icon(IconName::Redo2)
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&refresh_entity, |app, cx| {
                                app.reload_project_open_applications(window, cx);
                            });
                        }),
                )
                .separator();

            if applications.is_empty() {
                menu.item(PopupMenuItem::new("暂无可用应用").icon(IconName::ExternalLink))
            } else {
                applications.iter().fold(menu, |menu, application| {
                    let app_entity = app_entity.clone();
                    let application_id = application.id.clone();
                    let label = if application.installed {
                        application.label.clone()
                    } else {
                        format!("{}（未安装）", application.label)
                    };
                    menu.item(
                        PopupMenuItem::new(label)
                            .icon(if application.category == "primary" {
                                IconName::ExternalLink
                            } else {
                                IconName::File
                            })
                            .disabled(!has_project || !application.installed)
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
        })
}

fn workspace_level_button(tokens: i64, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    let tokens = tokens.max(0);
    let tier = daily_level_tier(tokens);

    Popover::new("workspace-level-popover")
        .anchor(Anchor::TopRight)
        .w(px(304.0))
        .trigger(
            workspace_header_button("workspace-level", cx)
                .secondary()
                .text_color(cx.theme().foreground)
                .child(workspace_daily_level_button_content(tier.clone(), cx)),
        )
        .content(move |_, _, _| workspace_level_popover_content(tokens, tier.clone()))
}

fn workspace_today_level_tokens(state: &RuntimeState) -> i64 {
    let history_tokens = state.ai_global_history.today_total_tokens.max(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs_f64())
        .unwrap_or_default();
    let day_start = now - (now % 86_400.0);
    let live_tokens = state
        .ai_runtime_state
        .sessions
        .iter()
        .filter(|session| session.updated_at >= day_start)
        .map(|session| session.total_tokens.max(0))
        .sum::<i64>();

    history_tokens + live_tokens
}

fn workspace_pet_button(
    pet: &PetSummary,
    pet_snapshot: Option<&PetSnapshot>,
    custom_pets: &[PetCustomPet],
    runtime_asset_root: &std::path::Path,
    support_dir: &std::path::Path,
    _install_url: &str,
    _install_display_name: &str,
    _install_preview: Option<&PetCustomPetInstallPreview>,
    _install_previewing: bool,
    _installing: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let app_entity = cx.entity();
    let pet = pet.clone();
    let pet_snapshot = pet_snapshot.cloned();
    let custom_pets = custom_pets.to_vec();
    let pet_sprite_path = pet_sprite_path(runtime_asset_root, support_dir, &pet, &custom_pets);
    let label = if pet.claimed {
        format!("Lv.{}", pet.level.max(1))
    } else {
        "领取".to_string()
    };
    let trigger = workspace_header_button("workspace-pet", cx)
        .secondary()
        .text_color(cx.theme().foreground)
        .child(workspace_header_badge_button_content(
            IconName::Heart,
            color(0x7C4DFF),
            label,
            cx,
        ));

    if !pet.claimed {
        return trigger
            .on_click(cx.listener(|app, _event, window, cx| {
                app.open_pet_claim_window(window, cx);
            }))
            .into_any_element();
    }

    let content = workspace_pet_popover_content(
        pet.clone(),
        pet_snapshot,
        pet_sprite_path,
        app_entity.clone(),
        window,
        cx,
    );

    Popover::new("workspace-pet-popover")
        .anchor(Anchor::TopRight)
        .w(px(324.0))
        .trigger(trigger)
        .child(content)
        .into_any_element()
}

fn workspace_assistant_button(
    label: &'static str,
    panel: AssistantPanel,
    active_panel: Option<AssistantPanel>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let active = active_panel == Some(panel);

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

    button
        .tooltip(label)
        .on_click(
            cx.listener(move |app, _event, window, cx| {
                app.toggle_assistant_panel(panel, window, cx)
            }),
        )
        .child(
            div()
                .h(px(20.0))
                .w(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .child(
                    Icon::new(match panel {
                        AssistantPanel::AIStats => IconName::Bot,
                        AssistantPanel::SSH => IconName::SquareTerminal,
                        AssistantPanel::FileManager => IconName::File,
                        AssistantPanel::Git => IconName::Github,
                    })
                    .size_3p5()
                    .text_color(if active {
                        cx.theme().foreground
                    } else {
                        cx.theme().secondary_foreground
                    }),
                ),
        )
}

fn workspace_header_button(id: &'static str, cx: &mut Context<CoduxApp>) -> Button {
    Button::new(id)
        .compact()
        .h(px(28.0))
        .text_color(cx.theme().foreground)
}

fn workspace_header_badge_button_content(
    icon: IconName,
    icon_bg: gpui::Hsla,
    label: impl Into<SharedString>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .h(px(20.0))
        .flex()
        .items_center()
        .gap_2()
        .text_color(cx.theme().foreground)
        .child(
            div()
                .size(px(18.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .bg(icon_bg)
                .text_color(color(0xFFFFFF))
                .child(Icon::new(icon).size_2()),
        )
        .child(
            div()
                .text_xs()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(label.into()),
        )
}

#[derive(Clone)]
struct DailyLevelTier {
    id: &'static str,
    title: &'static str,
    min: i64,
    color: u32,
    icon: IconName,
}

const DAILY_LEVEL_TIERS: [DailyLevelTier; 8] = [
    DailyLevelTier {
        id: "iron",
        title: "黑铁",
        min: 0,
        color: 0x5B616D,
        icon: IconName::Minus,
    },
    DailyLevelTier {
        id: "bronze",
        title: "青铜",
        min: 1_000_000,
        color: 0xC98663,
        icon: IconName::Star,
    },
    DailyLevelTier {
        id: "silver",
        title: "白银",
        min: 3_000_000,
        color: 0xC8D1E3,
        icon: IconName::Check,
    },
    DailyLevelTier {
        id: "gold",
        title: "黄金",
        min: 6_000_000,
        color: 0xE8AA34,
        icon: IconName::Star,
    },
    DailyLevelTier {
        id: "platinum",
        title: "铂金",
        min: 10_000_000,
        color: 0x7ED6D8,
        icon: IconName::Star,
    },
    DailyLevelTier {
        id: "diamond",
        title: "钻石",
        min: 18_000_000,
        color: 0x59A7FF,
        icon: IconName::Star,
    },
    DailyLevelTier {
        id: "master",
        title: "大师",
        min: 30_000_000,
        color: 0x9A72FF,
        icon: IconName::Star,
    },
    DailyLevelTier {
        id: "grandmaster",
        title: "宗师",
        min: 50_000_000,
        color: 0xFF5E8E,
        icon: IconName::Heart,
    },
];

fn daily_level_tier(tokens: i64) -> DailyLevelTier {
    DAILY_LEVEL_TIERS
        .iter()
        .rev()
        .find(|tier| tokens >= tier.min)
        .cloned()
        .unwrap_or_else(|| DAILY_LEVEL_TIERS[0].clone())
}

fn workspace_daily_level_button_content(
    tier: DailyLevelTier,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .h(px(20.0))
        .flex()
        .items_center()
        .gap_1()
        .text_color(cx.theme().foreground)
        .child(daily_level_badge(&tier, 18.0, 8.0))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(12.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(tier.title),
        )
}

fn workspace_level_popover_content(tokens: i64, current_tier: DailyLevelTier) -> impl IntoElement {
    let tokens = tokens.max(0);

    div()
        .flex()
        .flex_col()
        .text_color(color(theme::TEXT))
        .child(
            div()
                .flex()
                .items_center()
                .gap_3()
                .child(daily_level_badge(&current_tier, 34.0, 14.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .child(
                            div()
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(theme::TEXT_MUTED))
                                .child("今日等级"),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(15.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .child(current_tier.title),
                        ),
                )
                .child(
                    div()
                        .text_right()
                        .child(
                            div()
                                .text_size(px(11.0))
                                .line_height(px(14.0))
                                .font_weight(FontWeight::MEDIUM)
                                .text_color(color(theme::TEXT_MUTED))
                                .child("今日 Tokens"),
                        )
                        .child(
                            div()
                                .mt(px(2.0))
                                .text_size(px(15.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::BOLD)
                                .child(compact_number(tokens)),
                        ),
                ),
        )
        .child(div().mt(px(12.0)).flex().flex_col().gap_1().children(
            DAILY_LEVEL_TIERS.into_iter().map(|tier| {
                let current = tier.id == current_tier.id;
                div()
                    .rounded(px(8.0))
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .items_center()
                    .gap_2()
                    .bg(if current {
                        color(0xFFFFFF).opacity(0.075)
                    } else {
                        color(0xFFFFFF).opacity(0.0)
                    })
                    .border_1()
                    .border_color(if current {
                        color(tier.color).opacity(0.28)
                    } else {
                        color(0xFFFFFF).opacity(0.0)
                    })
                    .child(daily_level_badge(&tier, 24.0, 10.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .child(
                                div()
                                    .text_size(px(13.0))
                                    .line_height(px(16.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .child(tier.title),
                            )
                            .child(
                                div()
                                    .mt(px(2.0))
                                    .text_size(px(11.0))
                                    .line_height(px(14.0))
                                    .text_color(color(theme::TEXT_MUTED))
                                    .child(format!("需要 {}", compact_number(tier.min))),
                            ),
                    )
                    .when(current, |this| {
                        this.child(
                            div()
                                .rounded_full()
                                .px(px(8.0))
                                .py(px(4.0))
                                .text_size(px(11.0))
                                .line_height(px(12.0))
                                .font_weight(FontWeight::BOLD)
                                .bg(color(tier.color).opacity(0.14))
                                .text_color(color(tier.color))
                                .child("当前"),
                        )
                    })
                    .into_any_element()
            }),
        ))
}

fn daily_level_badge(tier: &DailyLevelTier, box_size: f32, icon_size: f32) -> impl IntoElement {
    div()
        .size(px(box_size))
        .rounded_full()
        .flex()
        .items_center()
        .justify_center()
        .bg(linear_gradient(
            135.0,
            linear_color_stop(color(tier.color), 0.0),
            linear_color_stop(color(tier.color).opacity(0.72), 1.0),
        ))
        .text_color(color(0xFFFFFF))
        .child(
            Icon::new(tier.icon.clone())
                .with_size(px(icon_size))
                .text_color(color(0xFFFFFF)),
        )
}

fn workspace_pet_popover_content(
    pet: PetSummary,
    pet_snapshot: Option<PetSnapshot>,
    pet_sprite_path: std::path::PathBuf,
    app_entity: gpui::Entity<CoduxApp>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let name = if pet.claimed && !pet.display_name.is_empty() {
        pet.display_name.clone()
    } else {
        "还没有领取宠物".to_string()
    };
    let species_name = pet_snapshot
        .as_ref()
        .and_then(|snapshot| {
            snapshot
                .custom_pet
                .as_ref()
                .map(|pet| pet.display_name.clone())
        })
        .unwrap_or_else(|| workspace_pet_species_name(&pet.species));
    let subtitle = if pet.custom_name.trim().is_empty() {
        None
    } else {
        Some(species_name.clone())
    };
    let sprite_fallback_color = cx.theme().primary;
    let rename_form = workspace_pet_rename_form(&pet, window, cx);
    let progress = pet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.progress.clone())
        .unwrap_or_else(|| codux_runtime::pet::PetProgressInfo {
            level: pet.level.max(1),
            xp_in_level: 0,
            xp_for_level: 0,
            total_xp: pet.total_xp.max(0),
            progress: pet.progress,
            is_at_max_level: false,
        });
    let stats = pet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.current_stats.clone())
        .unwrap_or_default();
    let persona = pet_snapshot
        .as_ref()
        .map(|snapshot| snapshot.persona_id.clone())
        .unwrap_or_else(|| "observer".to_string());

    div()
        .flex()
        .flex_col()
        .text_color(color(theme::TEXT))
        .child(
            div()
                .relative()
                .flex()
                .flex_col()
                .items_center()
                .px(px(14.0))
                .pt(px(18.0))
                .pb(px(14.0))
                .child(
                    div()
                        .size(px(110.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(pet_sprite_element(
                            pet_sprite_path,
                            110.0,
                            sprite_fallback_color,
                        )),
                )
                .child(
                    Button::new("workspace-pet-dex-open")
                        .compact()
                        .ghost()
                        .tooltip("打开图鉴")
                        .absolute()
                        .right(px(12.0))
                        .top(px(12.0))
                        .icon(Icon::new(IconName::BookOpen).size_3p5())
                        .on_click(move |_, window, cx| {
                            cx.update_entity(&app_entity, |app, cx| {
                                app.open_pet_dex_window(window, cx);
                            });
                        }),
                )
                .child(
                    div()
                        .mt(px(12.0))
                        .flex()
                        .items_baseline()
                        .justify_center()
                        .gap_1()
                        .min_w_0()
                        .child(
                            div()
                                .text_size(px(17.0))
                                .line_height(px(22.0))
                                .font_weight(FontWeight::BOLD)
                                .truncate()
                                .child(name),
                        )
                        .when_some(subtitle, |this, subtitle| {
                            this.child(
                                div()
                                    .max_w(px(92.0))
                                    .truncate()
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .font_weight(FontWeight::MEDIUM)
                                    .text_color(color(theme::TEXT_MUTED))
                                    .child(subtitle),
                            )
                        }),
                )
                .child(
                    div()
                        .mt(px(8.0))
                        .rounded_full()
                        .bg(color(theme::ACCENT).opacity(0.14))
                        .px(px(10.0))
                        .py(px(4.0))
                        .text_size(px(12.0))
                        .line_height(px(12.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::ACCENT))
                        .child(pet_persona_label(&persona)),
                )
                .child(
                    div()
                        .mt(px(10.0))
                        .text_size(px(26.0))
                        .line_height(px(32.0))
                        .font_weight(FontWeight::BLACK)
                        .child(format!("Lv.{}", progress.level.max(1))),
                ),
        )
        .child(workspace_popover_separator())
        .child(div().px(px(14.0)).py(px(12.0)).child(workspace_pet_meter(
            "经验",
            format!(
                "{} / {}",
                compact_number(progress.xp_in_level),
                compact_number(progress.xp_for_level)
            ),
            progress.progress,
            theme::ACCENT,
        )))
        .child(workspace_popover_separator())
        .child(
            div()
                .px(px(14.0))
                .py(px(12.0))
                .child(
                    div()
                        .mb(px(8.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child("特质"),
                )
                .child(
                    div()
                        .flex()
                        .flex_col()
                        .gap_2()
                        .child(workspace_pet_trait("智慧", stats.wisdom, 0x2F8FFF))
                        .child(workspace_pet_trait("混沌", stats.chaos, 0xFF6030))
                        .child(workspace_pet_trait("夜行", stats.night, 0x6060CC))
                        .child(workspace_pet_trait("耐力", stats.stamina, 0x20A060))
                        .child(workspace_pet_trait("共情", stats.empathy, 0xE060A0)),
                ),
        )
        .child(workspace_popover_separator())
        .child(
            div()
                .py(px(10.0))
                .text_center()
                .child(
                    div()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child("总 XP"),
                )
                .child(
                    div()
                        .mt(px(2.0))
                        .text_size(px(13.0))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(compact_number(progress.total_xp)),
                ),
        )
        .child(div().px(px(14.0)).pb(px(12.0)).child(rename_form))
        .when_some(pet.error, |this, error| {
            this.child(
                div()
                    .px(px(14.0))
                    .pb(px(12.0))
                    .child(workspace_popover_notice(error)),
            )
        })
}

fn workspace_popover_separator() -> impl IntoElement {
    div().mx(px(14.0)).h(px(1.0)).bg(color(theme::BORDER_SOFT))
}

fn workspace_pet_meter(
    label: &'static str,
    value: String,
    progress: f64,
    accent: u32,
) -> impl IntoElement {
    div()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_MUTED))
                        .child(label),
                )
                .child(
                    div()
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(color(theme::TEXT_DIM))
                        .child(value),
                ),
        )
        .child(
            div()
                .mt(px(6.0))
                .h(px(7.0))
                .rounded_full()
                .overflow_hidden()
                .bg(color(accent).opacity(0.15))
                .child(
                    div()
                        .h_full()
                        .w(relative(progress.clamp(0.0, 1.0) as f32))
                        .rounded_full()
                        .bg(color(accent)),
                ),
        )
}

fn workspace_pet_trait(label: &'static str, value: i64, accent: u32) -> impl IntoElement {
    let ratio = (value as f32 / 330.0).clamp(0.0, 1.0);
    div()
        .grid()
        .grid_cols(4)
        .items_center()
        .gap_2()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .child(
            div()
                .text_color(color(theme::TEXT_MUTED))
                .font_weight(FontWeight::MEDIUM)
                .child(label),
        )
        .child(
            div()
                .col_span(2)
                .h(px(5.0))
                .rounded_full()
                .overflow_hidden()
                .bg(color(accent).opacity(0.12))
                .child(
                    div()
                        .h_full()
                        .w(relative(ratio))
                        .rounded_full()
                        .bg(color(accent).opacity(0.75)),
                ),
        )
        .child(
            div()
                .text_right()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT_DIM))
                .child(compact_number(value)),
        )
}

fn pet_persona_label(persona: &str) -> String {
    match persona {
        "observer" => "观察者",
        "sprinter" => "疾行者",
        "guardian" => "守护者",
        "nightowl" => "夜行者",
        "maker" => "创造者",
        value => value,
    }
    .to_string()
}

fn workspace_pet_species_name(species: &str) -> String {
    match species.strip_prefix("custom:") {
        Some(id) if !id.trim().is_empty() => id.to_string(),
        _ => match species {
            "voidcat" => "Voidcat",
            "fox" => "Fox",
            "panda" => "Panda",
            "otter" => "Otter",
            "owl" => "Owl",
            "dragon" => "Dragon",
            value if !value.trim().is_empty() => value,
            _ => "Pet",
        }
        .to_string(),
    }
}

fn workspace_pet_rename_form(
    pet: &PetSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let value = pet.custom_name.clone();
    let name_state = window.use_keyed_state("pet-rename-custom-name", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder("宠物昵称")
    });
    name_state.update(cx, |state, cx| {
        if state.value().as_ref() != pet.custom_name {
            state.set_value(pet.custom_name.clone(), window, cx);
        }
    });
    let save_state = name_state.clone();

    div()
        .rounded(px(6.0))
        .bg(color(0xFFFFFF).opacity(0.055))
        .p_2()
        .child(
            div()
                .mb_2()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child("宠物昵称"),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap_2()
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(Input::new(&name_state).with_size(gpui_component::Size::Small)),
                )
                .child(
                    Button::new("pet-rename-current")
                        .compact()
                        .secondary()
                        .tooltip("保存宠物昵称")
                        .text_color(cx.theme().secondary_foreground)
                        .icon(
                            Icon::new(IconName::Check)
                                .size_3p5()
                                .text_color(cx.theme().secondary_foreground),
                        )
                        .on_click(cx.listener(move |app, _event, window, cx| {
                            let custom_name = save_state.read(cx).value().to_string();
                            app.rename_current_pet_to(custom_name, window, cx)
                        })),
                ),
        )
        .into_any_element()
}

pub(in crate::app) fn workspace_pet_install_form(
    install_url: &str,
    install_display_name: &str,
    install_preview: Option<&PetCustomPetInstallPreview>,
    install_previewing: bool,
    installing: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let url_value = install_url.to_string();
    let name_value = install_display_name.to_string();
    let url_state = window.use_keyed_state("pet-install-url", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(url_value.clone())
            .placeholder("Petdex URL")
    });
    url_state.update(cx, |state, cx| {
        if state.value().as_ref() != install_url {
            state.set_value(install_url.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&url_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_pet_install_url(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    let name_state = window.use_keyed_state("pet-install-display-name", cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(name_value.clone())
            .placeholder("显示名")
    });
    name_state.update(cx, |state, cx| {
        if state.value().as_ref() != install_display_name {
            state.set_value(install_display_name.to_string(), window, cx);
        }
    });
    cx.subscribe_in(&name_state, window, |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            app.set_pet_install_display_name(state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .rounded(px(6.0))
        .bg(color(0xFFFFFF).opacity(0.055))
        .p_2()
        .child(
            div()
                .mb_2()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(theme::TEXT))
                .child("自定义宠物"),
        )
        .child(
            div()
                .flex()
                .flex_col()
                .gap_2()
                .child(Input::new(&url_state).with_size(gpui_component::Size::Small))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap_2()
                        .child(
                            div().flex_1().min_w_0().child(
                                Input::new(&name_state).with_size(gpui_component::Size::Small),
                            ),
                        )
                        .child(
                            Button::new("pet-preview-custom")
                                .compact()
                                .secondary()
                                .loading(install_previewing)
                                .disabled(install_previewing || installing)
                                .tooltip("预览自定义宠物")
                                .text_color(cx.theme().secondary_foreground)
                                .icon(
                                    Icon::new(IconName::Eye)
                                        .size_3p5()
                                        .text_color(cx.theme().secondary_foreground),
                                )
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.preview_custom_pet_install(window, cx)
                                })),
                        )
                        .child(
                            Button::new("pet-install-custom")
                                .compact()
                                .secondary()
                                .loading(installing)
                                .disabled(install_previewing || installing)
                                .tooltip("安装自定义宠物")
                                .text_color(cx.theme().secondary_foreground)
                                .icon(
                                    Icon::new(IconName::Plus)
                                        .size_3p5()
                                        .text_color(cx.theme().secondary_foreground),
                                )
                                .on_click(cx.listener(|app, _event, window, cx| {
                                    app.install_custom_pet(window, cx)
                                })),
                        ),
                ),
        )
        .when_some(install_preview.cloned(), |this, preview| {
            this.child(workspace_pet_install_preview(preview))
        })
}

fn workspace_pet_install_preview(preview: PetCustomPetInstallPreview) -> impl IntoElement {
    div()
        .mt_2()
        .rounded(px(6.0))
        .bg(color(0x000000).opacity(0.16))
        .p_2()
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap_2()
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .truncate()
                        .child(preview.display_name),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(preview.slug),
                ),
        )
        .child(
            div()
                .mt_1()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .truncate()
                .child(empty_label(&preview.description)),
        )
}

fn workspace_popover_notice(message: String) -> impl IntoElement {
    div()
        .rounded(px(6.0))
        .bg(color(theme::ORANGE).opacity(0.12))
        .px_2()
        .py_1()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .text_color(color(theme::ORANGE))
        .child(message)
}

fn workspace_segmented_tabs(active_index: usize, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .gap_1()
        .h(px(32.0))
        .p(px(4.0))
        .rounded_sm()
        .bg(color(0xFFFFFF).opacity(0.06))
        .child(workspace_segmented_tab(
            0,
            "终端",
            IconName::SquareTerminal,
            active_index == 0,
            cx,
        ))
        .child(workspace_segmented_tab(
            1,
            "文件",
            IconName::File,
            active_index == 1,
            cx,
        ))
        .child(workspace_segmented_tab(
            2,
            "评审",
            IconName::Github,
            active_index == 2,
            cx,
        ))
}

fn workspace_segmented_tab(
    index: usize,
    label: &'static str,
    icon: IconName,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("workspace-view-tab-{index}")))
        .h(px(22.0))
        .px_3()
        .flex()
        .items_center()
        .justify_center()
        .rounded_sm()
        .text_color(if active {
            cx.theme().primary_foreground
        } else {
            cx.theme().secondary_foreground
        })
        .bg(if active {
            cx.theme().primary
        } else {
            cx.theme().transparent
        })
        .cursor_pointer()
        .hover(|style| {
            if active {
                style.bg(cx.theme().primary)
            } else {
                style.bg(cx.theme().secondary_hover.opacity(0.72))
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
                        .text_xs()
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(label),
                ),
        )
}

fn terminal_bottom_tab_button(
    terminal_id: usize,
    label: String,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!(
            "terminal-bottom-tab-{terminal_id}"
        )))
        .h(px(32.0))
        .px_3()
        .flex()
        .items_center()
        .gap_2()
        .rounded_md()
        .cursor_pointer()
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().secondary_foreground
        })
        .bg(if active {
            cx.theme().secondary_hover
        } else {
            cx.theme().transparent
        })
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_terminal_tab(terminal_id, window, cx)
        }))
        .child(
            div()
                .text_xs()
                .line_height(px(14.0))
                .font_weight(FontWeight::SEMIBOLD)
                .child(label),
        )
        .child(
            div()
                .id(SharedString::from(format!(
                    "terminal-bottom-tab-close-{terminal_id}"
                )))
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .rounded_sm()
                .text_color(cx.theme().secondary_foreground)
                .hover(|style| style.bg(cx.theme().secondary_hover))
                .on_click(cx.listener(move |app, _event, window, cx| {
                    cx.stop_propagation();
                    window.prevent_default();
                    app.close_terminal_tab(terminal_id, window, cx)
                }))
                .child(Icon::new(IconName::Close).size_3()),
        )
}

fn terminal_bottom_add_button(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .id("terminal-bottom-tab-add")
        .size(px(26.0))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_sm()
        .cursor_pointer()
        .text_color(cx.theme().secondary_foreground)
        .hover(|style| style.bg(cx.theme().secondary_hover))
        .on_click(cx.listener(|app, _event, window, cx| app.add_terminal_tab(window, cx)))
        .child(Icon::new(IconName::Plus).size_3p5())
}

pub(in crate::app) fn workspace_text_button(
    id: &'static str,
    label: &'static str,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .ghost()
        .text_color(cx.theme().secondary_foreground)
        .label(label)
        .on_click(cx.listener(on_click))
}

fn terminal_bottom_summary(tab: &TerminalTab) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .text_xs()
        .text_color(color(theme::TEXT_DIM))
        .child(format!(
            "{} · {} pane{}",
            tab.label,
            tab.panes.len(),
            if tab.panes.len() == 1 { "" } else { "s" }
        ))
}
