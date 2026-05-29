use super::{CoduxApp, empty_label};
use crate::theme::{self, color};
use codux_runtime::{
    memory::MemorySummary,
    notification::NotificationSummary,
    remote::RemoteSummary,
    settings::SettingsSummary,
    ssh::{SSHProfileSummary, SSHSummary},
    tool_permissions::ToolPermissionsSummary,
    update::UpdateSummary,
};
use gpui::{
    AnyElement, AppContext, Context, FontWeight, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder as _, px,
    relative,
};
use gpui_component::{
    Disableable, Icon, IconName, Sizable,
    button::{Button, ButtonVariants},
    input::{Input, InputEvent, InputState},
    scroll::ScrollableElement,
    select::{Select, SelectEvent, SelectItem, SelectState},
    switch::Switch,
};

#[derive(Clone)]
struct SettingsSelectOption {
    value: String,
    label: SharedString,
}

impl SettingsSelectOption {
    fn new(value: impl Into<String>, label: impl Into<SharedString>) -> Self {
        Self {
            value: value.into(),
            label: label.into(),
        }
    }
}

impl SelectItem for SettingsSelectOption {
    type Value = String;

    fn title(&self) -> SharedString {
        self.label.clone()
    }

    fn value(&self) -> &Self::Value {
        &self.value
    }
}

fn settings_select_options(options: Vec<(String, SharedString)>) -> Vec<SettingsSelectOption> {
    options
        .into_iter()
        .map(|(value, label)| SettingsSelectOption::new(value, label))
        .collect()
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsPane {
    General,
    Appearance,
    Pet,
    AI,
    Git,
    Memory,
    Notifications,
    SSH,
    Remote,
    Shortcuts,
    Experiments,
    Developer,
}

impl SettingsPane {
    pub(super) fn label(self) -> &'static str {
        match self {
            Self::General => "通用",
            Self::Appearance => "外观",
            Self::Pet => "宠物",
            Self::AI => "AI",
            Self::Git => "Git",
            Self::Memory => "记忆",
            Self::Notifications => "通知",
            Self::SSH => "SSH",
            Self::Remote => "远程",
            Self::Shortcuts => "快捷键",
            Self::Experiments => "实验",
            Self::Developer => "开发",
        }
    }

    fn description(self) -> &'static str {
        match self {
            Self::General => "语言、默认 Shell、刷新频率和统计方式。",
            Self::Appearance => "主题、强调色、图标和终端文字。",
            Self::Pet => "桌面宠物、语音、提醒和 LLM 润色。",
            Self::AI => "AI CLI 工具、全局提示词和 API 通道。",
            Self::Git => "Git 提交消息生成与格式。",
            Self::Memory => "记忆注入、提取和索引。",
            Self::Notifications => "推送渠道和通知开关。",
            Self::SSH => "SSH 连接配置、凭据方式和连接测试。",
            Self::Remote => "远程连接、配对设备和中继状态。",
            Self::Shortcuts => "应用快捷键和项目切换快捷键。",
            Self::Experiments => "实验性功能开关。",
            Self::Developer => "开发者 HUD 和刷新间隔。",
        }
    }

    fn icon(self) -> IconName {
        match self {
            Self::General => IconName::Settings,
            Self::Appearance => IconName::Palette,
            Self::Pet => IconName::CircleUser,
            Self::AI => IconName::Bot,
            Self::Git => IconName::Github,
            Self::Memory => IconName::BookOpen,
            Self::Notifications => IconName::Bell,
            Self::SSH => IconName::SquareTerminal,
            Self::Remote => IconName::Globe,
            Self::Shortcuts => IconName::CaseSensitive,
            Self::Experiments => IconName::Asterisk,
            Self::Developer => IconName::Settings2,
        }
    }
}

const SETTINGS_PANES: [SettingsPane; 12] = [
    SettingsPane::General,
    SettingsPane::Appearance,
    SettingsPane::Pet,
    SettingsPane::AI,
    SettingsPane::Git,
    SettingsPane::Memory,
    SettingsPane::Notifications,
    SettingsPane::SSH,
    SettingsPane::Remote,
    SettingsPane::Shortcuts,
    SettingsPane::Experiments,
    SettingsPane::Developer,
];

impl CoduxApp {
    pub(super) fn settings_workspace(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let pane = self.active_settings_pane;

        div()
            .flex()
            .flex_1()
            .h_full()
            .bg(color(theme::BG))
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(200.0))
                    .h_full()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .bg(color(theme::BG_COLUMN))
                    .child(div().h(px(54.0)).flex_shrink_0())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .flex_1()
                            .min_h_0()
                            .px_3()
                            .pb_3()
                            .overflow_y_scrollbar()
                            .children(SETTINGS_PANES.into_iter().map(|item| {
                                settings_nav_row(item, pane == item, cx).into_any_element()
                            })),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .bg(color(0xFFFFFF).opacity(0.025))
                    .child(
                        div()
                            .h(px(92.0))
                            .flex_shrink_0()
                            .px(px(28.0))
                            .pb(px(16.0))
                            .flex()
                            .flex_col()
                            .justify_end()
                            .child(
                                div()
                                    .text_size(px(20.0))
                                    .line_height(px(26.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(theme::TEXT))
                                    .child(pane.label()),
                            )
                            .child(
                                div()
                                    .mt(px(8.0))
                                    .text_size(px(14.0))
                                    .line_height(px(20.0))
                                    .text_color(color(theme::TEXT_MUTED))
                                    .child(pane.description()),
                            ),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px(px(28.0))
                            .pb(px(28.0))
                            .child(settings_pane_body(self, pane, window, cx)),
                    ),
            )
    }
}

fn settings_nav_row(
    pane: SettingsPane,
    active: bool,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    div()
        .id(SharedString::from(format!("settings-nav-{:?}", pane)))
        .h(px(32.0))
        .px(px(10.0))
        .mb(px(6.0))
        .rounded(px(7.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .cursor_pointer()
        .text_color(color(if active {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        }))
        .bg(if active {
            color(theme::BG_ROW_ACTIVE)
        } else {
            color(0xFFFFFF).opacity(0.0)
        })
        .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
        .on_click(cx.listener(move |app, _event, _window, cx| app.set_settings_pane(pane, cx)))
        .child(Icon::new(pane.icon()).size_3p5())
        .child(
            div()
                .text_size(px(14.0))
                .line_height(px(18.0))
                .font_weight(if active {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .child(pane.label()),
        )
}

fn settings_pane_body(
    app: &CoduxApp,
    pane: SettingsPane,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    match pane {
        SettingsPane::General => {
            settings_general_pane(&app.state.settings, &app.state.update, window, cx)
        }
        SettingsPane::Appearance => settings_appearance_pane(&app.state.settings, window, cx),
        SettingsPane::Pet => settings_pet_pane(&app.state.settings, window, cx),
        SettingsPane::AI => settings_ai_pane(
            &app.state.settings,
            &app.state.tool_permissions,
            app.selected_ai_provider_id.as_deref(),
            app.ai_provider_testing_id.as_deref(),
            window,
            cx,
        ),
        SettingsPane::Git => settings_git_pane(&app.state.settings, window, cx),
        SettingsPane::Memory => {
            settings_memory_pane(&app.state.settings, &app.state.memory, window, cx)
        }
        SettingsPane::Notifications => settings_notifications_pane(
            &app.state.notifications,
            app.selected_notification_channel_id.as_deref(),
            app.notification_testing_channel_id.as_deref(),
            window,
            cx,
        ),
        SettingsPane::SSH => settings_ssh_pane(
            &app.state.ssh,
            app.selected_ssh_profile_id.as_deref(),
            app,
            app.ssh_testing,
            window,
            cx,
        ),
        SettingsPane::Remote => settings_remote_pane(
            &app.state.remote,
            app.selected_remote_device_id.as_deref(),
            window,
            cx,
        ),
        SettingsPane::Shortcuts => settings_shortcuts_pane(
            &app.state.settings,
            app.recording_shortcut_id.as_deref(),
            cx,
        ),
        SettingsPane::Experiments => settings_experiments_pane(app.agent_split_enabled, cx),
        SettingsPane::Developer => settings_developer_pane(&app.state.settings, window, cx),
    }
    .into_any_element()
}

fn settings_form(children: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .max_w(px(720.0))
        .gap(px(20.0))
        .children(children)
}

fn settings_card(
    title: Option<&'static str>,
    description: Option<String>,
    children: Vec<AnyElement>,
) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .rounded(px(12.0))
        .bg(color(0x000000).opacity(0.14))
        .px(px(22.0))
        .py(px(18.0))
        .child(if title.is_some() || description.is_some() {
            div()
                .mb(px(10.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .when(title.is_none(), |this| this.hidden())
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(title.unwrap_or("").to_string()),
                )
                .child(
                    div()
                        .when(description.is_none(), |this| this.hidden())
                        .mt(px(4.0))
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(description.unwrap_or_default()),
                )
                .into_any_element()
        } else {
            div().hidden().into_any_element()
        })
        .children(children.into_iter().enumerate().map(|(index, child)| {
            div()
                .when(index > 0, |this| {
                    this.border_t_1().border_color(color(theme::BORDER_SOFT))
                })
                .child(child)
                .into_any_element()
        }))
}

fn settings_row(
    label: &'static str,
    description: Option<String>,
    control: AnyElement,
) -> impl IntoElement {
    div()
        .min_h(px(58.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(
                    div()
                        .when(description.is_none(), |this| this.hidden())
                        .mt(px(3.0))
                        .max_w(px(420.0))
                        .text_size(px(12.0))
                        .line_height(px(17.0))
                        .text_color(color(theme::TEXT_DIM))
                        .child(description.unwrap_or_default()),
                ),
        )
        .child(control)
}

fn settings_small_button(
    id: impl Into<String>,
    value: impl Into<String>,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_small_button_state(id, value, false, false, cx, action)
}

fn settings_small_button_state(
    id: impl Into<String>,
    value: impl Into<String>,
    loading: bool,
    disabled: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    Button::new(SharedString::from(id.into()))
        .secondary()
        .loading(loading)
        .disabled(disabled)
        .text_color(color(theme::TEXT))
        .on_click(cx.listener(action))
        .child(
            div()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT))
                .child(value.into()),
        )
        .into_any_element()
}

fn settings_text_input(
    id: impl Into<SharedString>,
    value: impl Into<String>,
    placeholder: impl Into<String>,
    masked: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.into();
    let placeholder = placeholder.into();
    let key = SharedString::from(format!("settings-input-{}", id.into()));
    let state = window.use_keyed_state(key, cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .placeholder(placeholder.clone())
            .masked(masked)
    });
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, move |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            action(app, state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .w(relative(0.3))
        .child(Input::new(&state).with_size(gpui_component::Size::Medium))
        .into_any_element()
}

fn settings_textarea(
    id: &'static str,
    value: &str,
    rows: usize,
    placeholder: &'static str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.to_string();
    let state = window.use_keyed_state(
        SharedString::from(format!("settings-textarea-{id}")),
        cx,
        |window, cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(rows)
                .default_value(value.clone())
                .placeholder(placeholder)
        },
    );
    state.update(cx, |state, cx| {
        if state.value().as_ref() != value.as_str() {
            state.set_value(value.clone(), window, cx);
        }
    });
    cx.subscribe_in(&state, window, move |app, state, event, window, cx| {
        if matches!(event, InputEvent::Change) {
            action(app, state.read(cx).value().to_string(), window, cx);
        }
    })
    .detach();

    div()
        .w_full()
        .child(
            Input::new(&state)
                .with_size(gpui_component::Size::Medium)
                .h(px((rows as f32 * 28.0).max(84.0))),
        )
        .into_any_element()
}

fn settings_toggle(
    id: impl Into<String>,
    checked: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let app_entity = cx.entity();
    Switch::new(SharedString::from(id.into()))
        .checked(checked)
        .with_size(gpui_component::Size::Medium)
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                action(app, window, cx);
            });
        })
        .into_any_element()
}

fn settings_select(
    id: impl Into<String>,
    value: &str,
    options: Vec<(String, SharedString)>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_select_impl(id, value, options, false, window, cx, action)
}

fn settings_select_impl(
    id: impl Into<String>,
    value: &str,
    options: Vec<(String, SharedString)>,
    searchable: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let id = id.into();
    let items = settings_select_options(options.clone());
    let selected_index = items.iter().position(|item| item.value == value);
    let state = window.use_keyed_state(
        SharedString::from(format!("settings-select-{id}")),
        cx,
        |window, cx| {
            SelectState::new(
                items,
                selected_index.map(|row| gpui_component::IndexPath::default().row(row)),
                window,
                cx,
            )
            .searchable(searchable)
        },
    );
    state.update(cx, |state, cx| {
        let items = settings_select_options(options);
        let selected_index = items.iter().position(|item| item.value == value);
        state.set_items(items, window, cx);
        state.set_selected_index(
            selected_index.map(|row| gpui_component::IndexPath::default().row(row)),
            window,
            cx,
        );
    });
    cx.subscribe_in(&state, window, move |app, _, event, window, cx| {
        let SelectEvent::Confirm(selected) = event;
        if let Some(value) = selected.clone() {
            action(app, value, window, cx);
        }
    })
    .detach();

    div()
        .w(relative(0.3))
        .child(
            Select::new(&state)
                .placeholder("选择")
                .menu_width(if searchable { px(320.0) } else { px(220.0) })
                .with_size(gpui_component::Size::Medium),
        )
        .into_any_element()
}

fn settings_status_tag(value: impl Into<String>, accent: u32) -> AnyElement {
    div()
        .h(px(24.0))
        .px(px(9.0))
        .rounded(px(6.0))
        .bg(color(accent).opacity(0.14))
        .text_color(color(accent))
        .flex()
        .items_center()
        .text_size(px(12.0))
        .line_height(px(16.0))
        .font_weight(FontWeight::SEMIBOLD)
        .child(value.into())
        .into_any_element()
}

fn settings_checkmark(selected: bool) -> AnyElement {
    div()
        .when(!selected, |this| this.hidden())
        .absolute()
        .top(px(6.0))
        .right(px(6.0))
        .size(px(16.0))
        .rounded_full()
        .bg(color(theme::ACCENT))
        .flex()
        .items_center()
        .justify_center()
        .text_color(color(theme::TEXT))
        .child(Icon::new(IconName::Check).size_2p5())
        .into_any_element()
}

fn settings_selectable_tile(
    id: impl Into<String>,
    selected: bool,
    label: impl Into<String>,
    preview: AnyElement,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id.into()))
        .w(px(112.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_color(color(if selected {
            theme::TEXT
        } else {
            theme::TEXT_MUTED
        }))
        .on_click(cx.listener(action))
        .child(preview)
        .child(
            div()
                .w_full()
                .text_align(gpui::TextAlign::Center)
                .text_size(px(12.0))
                .line_height(px(16.0))
                .font_weight(if selected {
                    FontWeight::SEMIBOLD
                } else {
                    FontWeight::NORMAL
                })
                .truncate()
                .child(label.into()),
        )
        .into_any_element()
}

fn theme_preview_grid(
    title: Option<&'static str>,
    options: Vec<(&'static str, &'static str)>,
    selected: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .when(title.is_some(), |this| {
            this.child(
                div()
                    .px(px(2.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(theme::TEXT_DIM))
                    .child(title.unwrap_or_default()),
            )
        })
        .child(
            div().flex().flex_wrap().gap(px(10.0)).children(
                options.into_iter().map(|(value, label)| {
                    theme_preview_button(value, label, selected == value, cx)
                }),
            ),
        )
        .into_any_element()
}

fn theme_preview_button(
    value: &'static str,
    label: &'static str,
    selected: bool,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let preview = terminal_theme_preview(value);
    let tile_id = format!("settings-theme-preview-{value}");
    settings_selectable_tile(
        tile_id,
        selected,
        label,
        div()
            .relative()
            .w(px(112.0))
            .h(px(50.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(color(if selected {
                theme::ACCENT
            } else {
                theme::BORDER_SOFT
            }))
            .bg(color(preview.background))
            .hover(|style| style.border_color(color(theme::BORDER)))
            .child(
                div()
                    .p(px(9.0))
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .child(
                        div()
                            .h(px(3.0))
                            .w(px(20.0))
                            .rounded_full()
                            .bg(color(preview.muted_foreground)),
                    )
                    .child(
                        div()
                            .h(px(3.0))
                            .w(px(46.0))
                            .rounded(px(1.0))
                            .bg(color(preview.foreground)),
                    )
                    .child(
                        div()
                            .h(px(8.0))
                            .w(px(58.0))
                            .rounded(px(2.0))
                            .bg(color(preview.selection)),
                    ),
            )
            .child(settings_checkmark(selected))
            .into_any_element(),
        cx,
        move |app, _event, window, cx| app.set_theme(value.to_string(), window, cx),
    )
}

fn theme_color_grid(selected: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .gap(px(12.0))
        .children(theme_color_values().into_iter().map(|item| {
            let selected = selected == item.label;
            let value = item.label;
            settings_selectable_tile(
                format!("settings-theme-color-{value}"),
                selected,
                value,
                div()
                    .relative()
                    .size(px(32.0))
                    .rounded_full()
                    .border_1()
                    .border_color(color(if selected {
                        theme::ACCENT
                    } else {
                        theme::BORDER_SOFT
                    }))
                    .bg(color(item.color))
                    .hover(|style| style.border_color(color(theme::BORDER)))
                    .child(settings_checkmark(selected))
                    .into_any_element(),
                cx,
                move |app, _event, window, cx| app.set_theme_color(value.to_string(), window, cx),
            )
        }))
        .into_any_element()
}

fn app_icon_grid(selected: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .flex()
        .flex_wrap()
        .gap(px(14.0))
        .children(icon_style_values().into_iter().map(|item| {
            let selected = selected == item.value;
            let value = item.value;
            settings_selectable_tile(
                format!("settings-app-icon-{value}"),
                selected,
                item.label,
                app_icon_preview(item.value, selected),
                cx,
                move |app, _event, window, cx| app.set_icon_style(value.to_string(), window, cx),
            )
        }))
        .into_any_element()
}

fn app_icon_preview(style: &'static str, selected: bool) -> AnyElement {
    let palette = app_icon_palette(style);
    div()
        .relative()
        .size(px(52.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(color(if selected {
            theme::ACCENT
        } else {
            theme::BORDER_SOFT
        }))
        .bg(color(palette.0))
        .child(
            div()
                .absolute()
                .inset_0()
                .rounded(px(12.0))
                .bg(color(palette.1).opacity(0.38)),
        )
        .child(
            div()
                .absolute()
                .left(px(17.0))
                .top(px(17.0))
                .text_size(px(18.0))
                .line_height(px(18.0))
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(color(0xFFFFFF))
                .child(">"),
        )
        .child(settings_checkmark(selected))
        .into_any_element()
}

fn settings_general_pane(
    settings: &SettingsSummary,
    update: &UpdateSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(vec![
        settings_card(
            None,
            None,
            vec![
                settings_row(
                    "语言",
                    None,
                    settings_select(
                        "settings-language",
                        &settings.language,
                        language_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_language(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "默认终端",
                    None,
                    settings_select(
                        "settings-shell",
                        &settings.shell,
                        shell_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_shell(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "程序坞角标",
                    None,
                    settings_toggle(
                        "settings-dock-badge",
                        settings.shows_dock_badge,
                        cx,
                        |app, window, cx| app.toggle_dock_badge(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "阻止系统休眠",
                    Some(
                        "允许显示器按系统设置关闭，但启用时会阻止当前设备进入空闲休眠。"
                            .to_string(),
                    ),
                    settings_select(
                        "settings-sleep-mode",
                        &settings.sleep_mode,
                        sleep_mode_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_sleep_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            None,
            None,
            vec![
                settings_row(
                    "Git 自动刷新",
                    None,
                    settings_select(
                        "settings-git-refresh",
                        &settings.git_refresh,
                        git_refresh_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_git_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "AI 自动刷新",
                    None,
                    settings_select(
                        "settings-ai-refresh",
                        &settings.ai_refresh,
                        ai_refresh_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_ai_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "AI 后台刷新",
                    None,
                    settings_select(
                        "settings-ai-background-refresh",
                        &settings.ai_background_refresh,
                        ai_background_refresh_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_ai_background_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "AI 统计显示方式",
                    None,
                    settings_select(
                        "settings-statistics-mode",
                        &settings.statistics_mode,
                        statistics_mode_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_statistics_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            Some("更新"),
            Some("更新会直接从所选通道的 GitHub Release 检查。".to_string()),
            vec![
                settings_row(
                    "启用检查更新",
                    None,
                    settings_toggle(
                        "settings-update-enabled",
                        settings.update_enabled,
                        cx,
                        |app, window, cx| app.toggle_update_enabled(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "更新通道",
                    None,
                    settings_select(
                        "settings-update-channel",
                        &settings.update_channel,
                        update_channel_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_update_channel(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "更新状态",
                    Some(update_status_text(update)),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(settings_small_button_state(
                            "settings-check-update",
                            "检查更新",
                            false,
                            !settings.update_enabled,
                            cx,
                            |app, _event, window, cx| app.reload_update(window, cx),
                        ))
                        .into_any_element(),
                )
                .into_any_element(),
                settings_row(
                    "关于 Codux",
                    Some("查看版本信息，打开官网，导出诊断，查看运行日志。".to_string()),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(settings_small_button(
                            "settings-open-about",
                            "关于",
                            cx,
                            |app, _event, window, cx| app.open_about_window(window, cx),
                        ))
                        .child(settings_small_button(
                            "settings-export-diagnostics",
                            "导出诊断",
                            cx,
                            |app, _event, _window, cx| app.export_diagnostics(cx),
                        ))
                        .child(settings_small_button(
                            "settings-runtime-log",
                            "Runtime Log",
                            cx,
                            |app, _event, _window, cx| app.open_runtime_log(cx),
                        ))
                        .into_any_element(),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn update_status_text(update: &UpdateSummary) -> String {
    if let Some(error) = &update.error {
        return format!("检查失败: {error}");
    }
    if let Some(version) = &update.latest_version {
        let notes = update.notes_preview.trim();
        if notes.is_empty() {
            return format!("最新版本 {version} · {}", update.channel);
        }
        return format!("最新版本 {version} · {notes}");
    }
    if update.enabled {
        format!("通道 {} · 等待检查", update.channel)
    } else {
        "更新检查已关闭".to_string()
    }
}

fn settings_appearance_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut cards = vec![
        settings_card(
            Some("终端主题"),
            Some("主题和终端配色会在重启后完整应用到所有终端。".to_string()),
            vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(theme_preview_grid(
                        None,
                        system_theme_options(),
                        &settings.theme,
                        cx,
                    ))
                    .child(theme_preview_grid(
                        Some("深色"),
                        dark_theme_options(),
                        &settings.theme,
                        cx,
                    ))
                    .child(theme_preview_grid(
                        Some("浅色"),
                        light_theme_options(),
                        &settings.theme,
                        cx,
                    ))
                    .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            Some("主题色"),
            Some("应用到按钮、选中状态、顶部 Tab、焦点环、链接和其他高亮色。".to_string()),
            vec![theme_color_grid(&settings.theme_color, cx)],
        )
        .into_any_element(),
        settings_card(
            Some("终端文字"),
            None,
            vec![
                settings_row(
                    "终端字号",
                    None,
                    settings_text_input(
                        "settings-terminal-font-size",
                        &settings.terminal_font_size,
                        "14",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_terminal_font_size(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "终端历史",
                    Some("限制终端滚动历史和恢复输出，减少长会话内存占用。".to_string()),
                    settings_select(
                        "settings-terminal-scrollback",
                        &settings.terminal_scrollback_lines,
                        terminal_scrollback_options(),
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_terminal_scrollback_lines(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ];

    if cfg!(target_os = "macos") {
        cards.push(
            settings_card(
                Some("应用图标"),
                Some("图标变化会在重启后完整生效。".to_string()),
                vec![app_icon_grid(&settings.icon_style, cx)],
            )
            .into_any_element(),
        );
    }

    settings_form(cards).into_any_element()
}

fn settings_pet_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(vec![
        settings_card(
            Some("通用"),
            None,
            vec![
                settings_row(
                    "启用宠物",
                    None,
                    settings_toggle(
                        "settings-pet-enabled",
                        settings.pet_enabled,
                        cx,
                        |app, window, cx| app.toggle_pet_enabled(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "桌面宠物",
                    None,
                    settings_toggle(
                        "settings-pet-desktop",
                        settings.pet_desktop_widget,
                        cx,
                        |app, window, cx| app.toggle_pet_desktop_widget(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "静态宠物精灵",
                    None,
                    settings_toggle(
                        "settings-pet-static",
                        settings.pet_static_mode,
                        cx,
                        |app, window, cx| app.toggle_pet_static_mode(window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            Some("宠物语音"),
            None,
            vec![
                settings_row(
                    "模式",
                    None,
                    settings_select(
                        "settings-pet-speech-mode",
                        &settings.pet_speech_mode,
                        pet_speech_mode_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_pet_speech_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "频率",
                    None,
                    settings_select(
                        "settings-pet-speech-frequency",
                        &settings.pet_speech_frequency,
                        pet_speech_frequency_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_pet_speech_frequency(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "工作时少说话",
                    None,
                    settings_toggle(
                        "settings-pet-speech-work",
                        settings.pet_speech_quiet_during_work,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_quiet_during_work(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "夜间多说话",
                    None,
                    settings_toggle(
                        "settings-pet-speech-night",
                        settings.pet_speech_louder_at_night,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_louder_at_night(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "全屏静音",
                    None,
                    settings_toggle(
                        "settings-pet-speech-fullscreen",
                        settings.pet_speech_mute_on_fullscreen,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_mute_on_fullscreen(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "安静时段 22:00-08:00",
                    None,
                    settings_toggle(
                        "settings-pet-quiet-hours",
                        settings.pet_speech_quiet_hours_enabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_quiet_hours(window, cx),
                    ),
                )
                .into_any_element(),
                div()
                    .py(px(10.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(settings_small_button(
                        "settings-pet-mute-30",
                        "静音 30 分钟",
                        cx,
                        |app, _event, _window, cx| app.set_pet_speech_temporary_mute(true, cx),
                    ))
                    .child(settings_small_button(
                        "settings-pet-unmute",
                        "取消临时静音",
                        cx,
                        |app, _event, _window, cx| app.set_pet_speech_temporary_mute(false, cx),
                    ))
                    .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            Some("宠物 LLM"),
            Some("只有节奏和里程碑消息会尝试 LLM 润色，失败时会回退到模板台词。".to_string()),
            vec![
                settings_row(
                    "启用 LLM 台词润色",
                    None,
                    settings_toggle(
                        "settings-pet-llm",
                        settings.pet_speech_llm_enabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_llm_enabled(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "LLM 通道",
                    None,
                    settings_select(
                        "pet-speech-provider",
                        &settings.pet_speech_provider_id,
                        ai_provider_options(settings, "petSpeech"),
                        window,
                        cx,
                        |app, value, window, cx| app.set_pet_speech_provider(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            Some("提醒"),
            None,
            vec![
                settings_row(
                    "喝水提醒",
                    None,
                    settings_toggle(
                        "settings-pet-reminders",
                        settings.pet_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_reminders(window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_ai_pane(
    settings: &SettingsSummary,
    permissions: &ToolPermissionsSummary,
    selected_provider_id: Option<&str>,
    testing_provider_id: Option<&str>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut provider_rows = if settings.ai_providers.is_empty() {
        vec![
            div()
                .py(px(12.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_DIM))
                .child("尚未新增 API 通道。")
                .into_any_element(),
        ]
    } else {
        settings
            .ai_providers
            .iter()
            .cloned()
            .map(|provider| {
                settings_ai_provider_card(
                    provider,
                    selected_provider_id,
                    testing_provider_id,
                    window,
                    cx,
                )
                .into_any_element()
            })
            .collect::<Vec<_>>()
    };
    provider_rows.insert(
        0,
        div()
            .py(px(4.0))
            .flex()
            .items_center()
            .justify_between()
            .gap(px(16.0))
            .child(
                div()
                    .text_size(px(14.0))
                    .line_height(px(18.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(color(theme::TEXT))
                    .child("AI 提供方"),
            )
            .child(settings_small_button(
                "settings-add-ai-provider",
                "新增 API 通道",
                cx,
                |app, _event, window, cx| app.add_ai_provider(window, cx),
            ))
            .into_any_element(),
    );
    provider_rows.insert(
        1,
        div()
            .h(px(1.0))
            .my(px(10.0))
            .bg(color(theme::BORDER_SOFT))
            .into_any_element(),
    );

    let mut runtime_tool_rows = vec![settings_runtime_tools_header(permissions, cx)];
    runtime_tool_rows.extend(vec![
        settings_runtime_tool_block(
            "Codex 配置",
            "codex",
            "codexModel",
            &permissions.codex,
            &permissions.codex_model,
            "gpt-5.5",
            true,
            &permissions.codex_effort,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            "Claude Code 配置",
            "claudeCode",
            "claudeCodeModel",
            &permissions.claude_code,
            &permissions.claude_code_model,
            "claude-sonnet-4.5",
            false,
            &permissions.codex_effort,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            "Gemini 配置",
            "gemini",
            "geminiModel",
            &permissions.gemini,
            &permissions.gemini_model,
            "gemini-2.5-pro",
            false,
            &permissions.codex_effort,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            "OpenCode 配置",
            "opencode",
            "opencodeModel",
            &permissions.opencode,
            &permissions.opencode_model,
            "gpt-5.5",
            false,
            &permissions.codex_effort,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            "Kiro 配置",
            "kiro",
            "kiroModel",
            &permissions.kiro,
            &permissions.kiro_model,
            "auto",
            false,
            &permissions.codex_effort,
            window,
            cx,
        ),
    ]);

    settings_form(vec![
        settings_card(
            Some("运行时工具"),
            Some("同步后会写入运行时 wrapper 使用的权限文件。".to_string()),
            runtime_tool_rows,
        )
        .into_any_element(),
        settings_card(
            Some("全局提示词"),
            Some("支持的工具启动时会注入，并会和记忆上下文合并写入启动上下文。".to_string()),
            vec![settings_textarea(
                "ai-global-prompt",
                &settings.ai_global_prompt,
                4,
                "写入所有支持工具的全局提示词",
                window,
                cx,
                |app, value, window, cx| app.set_ai_global_prompt(value, window, cx),
            )],
        )
        .into_any_element(),
        settings_card(None, None, provider_rows).into_any_element(),
    ])
    .into_any_element()
}

fn settings_git_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(vec![
        settings_card(
            Some("Git 提交消息"),
            None,
            vec![
                settings_row(
                    "AI 提供方",
                    None,
                    settings_select(
                        "settings-git-provider-auto",
                        &settings.git_commit_provider_id,
                        git_provider_options(settings),
                        window,
                        cx,
                        |app, value, window, cx| app.set_git_commit_provider(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "语气",
                    None,
                    settings_select(
                        "settings-git-tone",
                        &settings.git_commit_tone,
                        git_tone_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_git_commit_tone(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "语言",
                    None,
                    settings_select(
                        "settings-git-language",
                        &settings.git_commit_language,
                        git_language_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_git_commit_language(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "风格规则",
                    Some("例如：使用 Conventional Commits，标题不超过 72 个字符。".to_string()),
                    settings_textarea(
                        "git-style-rules",
                        &settings.git_commit_style_rules,
                        3,
                        "Example: use Conventional Commits, keep subject under 72 characters.",
                        window,
                        cx,
                        |app, value, window, cx| app.set_git_commit_style_rules(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_memory_pane(
    settings: &SettingsSummary,
    _memory: &MemorySummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut cards = vec![
        settings_card(
            Some("记忆"),
            None,
            vec![
                settings_row(
                    "启用记忆",
                    None,
                    settings_toggle(
                        "settings-memory-enabled",
                        settings.memory_enabled,
                        cx,
                        |app, window, cx| {
                            let next = !app.state.settings.memory_enabled;
                            app.set_ai_memory_bool("enabled", next, window, cx)
                        },
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ];

    if settings.memory_enabled {
        cards.push(
            settings_card(
                Some("自动注入"),
                None,
                vec![
                    settings_row(
                        "自动注入",
                        None,
                        settings_toggle(
                            "settings-memory-auto-injection",
                            settings.memory_automatic_injection_enabled,
                            cx,
                            |app, window, cx| {
                                let next = !app.state.settings.memory_automatic_injection_enabled;
                                app.set_ai_memory_bool(
                                    "automaticInjectionEnabled",
                                    next,
                                    window,
                                    cx,
                                )
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        "自动提取",
                        None,
                        settings_toggle(
                            "settings-memory-auto-extraction",
                            settings.memory_automatic_extraction_enabled,
                            cx,
                            |app, window, cx| {
                                let next = !app.state.settings.memory_automatic_extraction_enabled;
                                app.set_ai_memory_bool(
                                    "automaticExtractionEnabled",
                                    next,
                                    window,
                                    cx,
                                )
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        "提取间隔",
                        None,
                        settings_select(
                            "settings-memory-extraction-interval",
                            &settings.memory_extraction_idle_delay_seconds,
                            memory_extraction_interval_options(),
                            window,
                            cx,
                            |app, value, window, cx| {
                                app.set_ai_memory_number(
                                    "extractionIdleDelaySeconds",
                                    value,
                                    window,
                                    cx,
                                )
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        "最大最近会话",
                        None,
                        settings_select(
                            "settings-memory-max-index",
                            &settings.memory_max_index_sessions,
                            memory_max_index_options(),
                            window,
                            cx,
                            |app, value, window, cx| {
                                app.set_ai_memory_number("maxIndexSessions", value, window, cx)
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        "跨项目用户记忆",
                        None,
                        settings_toggle(
                            "settings-memory-cross-project",
                            settings.memory_allow_cross_project_user_recall,
                            cx,
                            |app, window, cx| {
                                let next =
                                    !app.state.settings.memory_allow_cross_project_user_recall;
                                app.set_ai_memory_bool(
                                    "allowCrossProjectUserRecall",
                                    next,
                                    window,
                                    cx,
                                )
                            },
                        ),
                    )
                    .into_any_element(),
                ],
            )
            .into_any_element(),
        );
        cards.push(
            settings_card(
                Some("默认提取通道"),
                None,
                vec![
                    settings_row(
                        "默认提取通道",
                        None,
                        settings_select(
                            "settings-memory-provider",
                            &settings.memory_default_extractor_provider_id,
                            ai_provider_options(settings, "memory"),
                            window,
                            cx,
                            |app, value, window, cx| app.set_ai_memory_provider(value, window, cx),
                        ),
                    )
                    .into_any_element(),
                ],
            )
            .into_any_element(),
        );
    }

    settings_form(cards).into_any_element()
}

fn settings_notifications_pane(
    notifications: &NotificationSummary,
    _selected_channel_id: Option<&str>,
    testing_channel_id: Option<&str>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(
        notifications
            .channels
            .iter()
            .cloned()
            .map(|channel| {
                settings_notification_card(channel, testing_channel_id, window, cx)
                    .into_any_element()
            })
            .collect::<Vec<_>>(),
    )
    .into_any_element()
}

fn settings_ssh_pane(
    ssh: &SSHSummary,
    selected_profile_id: Option<&str>,
    app: &CoduxApp,
    ssh_testing: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let profile_rows = if ssh.profiles.is_empty() {
        vec![
            div()
                .py(px(12.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_DIM))
                .child("还没有 SSH 配置。")
                .into_any_element(),
        ]
    } else {
        ssh.profiles
            .iter()
            .cloned()
            .map(|profile| settings_ssh_profile_row(profile, selected_profile_id, cx))
            .collect::<Vec<_>>()
    };

    settings_form(vec![
        settings_card(
            Some("连接配置"),
            Some(format!(
                "{} 个连接，包装器{}。",
                ssh.profiles.len(),
                if ssh.wrapper_available {
                    "可用"
                } else {
                    "未就绪"
                }
            )),
            {
                let mut rows = vec![
                    div()
                        .pb(px(10.0))
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(settings_small_button(
                            "settings-ssh-new",
                            "新增",
                            cx,
                            |app, _event, window, cx| app.new_ssh_profile_draft(window, cx),
                        ))
                        .child(settings_small_button(
                            "settings-ssh-edit",
                            "编辑选中",
                            cx,
                            |app, _event, window, cx| {
                                app.load_selected_ssh_profile_draft(window, cx)
                            },
                        ))
                        .child(settings_small_button(
                            "settings-ssh-refresh",
                            "刷新",
                            cx,
                            |app, _event, window, cx| app.reload_ssh(window, cx),
                        ))
                        .into_any_element(),
                ];
                rows.extend(profile_rows);
                if let Some(error) = &ssh.error {
                    rows.push(
                        div()
                            .pt(px(10.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::ORANGE))
                            .child(error.clone())
                            .into_any_element(),
                    );
                }
                rows
            },
        )
        .into_any_element(),
        settings_card(
            Some("编辑连接"),
            app.ssh_draft_id
                .as_ref()
                .map(|id| format!("正在编辑 {}", empty_label(id)))
                .or_else(|| Some("保存时会创建新的 SSH 配置。".to_string())),
            vec![
                settings_row(
                    "名称",
                    None,
                    settings_text_input(
                        "settings-ssh-name",
                        &app.ssh_draft_name,
                        "生产服务器",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_name(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "主机",
                    None,
                    settings_text_input(
                        "settings-ssh-host",
                        &app.ssh_draft_host,
                        "example.com",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_host(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "端口",
                    None,
                    settings_text_input(
                        "settings-ssh-port",
                        &app.ssh_draft_port,
                        "22",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_port(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "用户名",
                    None,
                    settings_text_input(
                        "settings-ssh-username",
                        &app.ssh_draft_username,
                        "root",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_username(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "凭据方式",
                    None,
                    settings_select(
                        "settings-ssh-credential-kind",
                        &app.ssh_draft_credential_kind,
                        ssh_credential_options(),
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_ssh_draft_credential_kind(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    "私钥路径",
                    None,
                    settings_text_input(
                        "settings-ssh-private-key-path",
                        &app.ssh_draft_private_key_path,
                        "~/.ssh/id_ed25519",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_ssh_draft_private_key_path(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    "密码",
                    None,
                    settings_text_input(
                        "settings-ssh-password",
                        &app.ssh_draft_password,
                        "password",
                        true,
                        window,
                        cx,
                        |app, value, window, cx| app.set_ssh_draft_password(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "私钥口令",
                    None,
                    settings_text_input(
                        "settings-ssh-key-passphrase",
                        &app.ssh_draft_key_passphrase,
                        "passphrase",
                        true,
                        window,
                        cx,
                        |app, value, window, cx| {
                            app.set_ssh_draft_key_passphrase(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                div()
                    .pt(px(8.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .child(settings_small_button_state(
                        "settings-ssh-test",
                        if ssh_testing { "测试中" } else { "测试" },
                        ssh_testing,
                        ssh_testing,
                        cx,
                        |app, _event, window, cx| app.test_ssh_profile_draft(window, cx),
                    ))
                    .child(settings_small_button(
                        "settings-ssh-delete",
                        "删除",
                        cx,
                        |app, _event, window, cx| app.delete_selected_ssh_profile(window, cx),
                    ))
                    .child(settings_small_button(
                        "settings-ssh-save",
                        "保存",
                        cx,
                        |app, _event, window, cx| app.save_ssh_profile_draft(window, cx),
                    ))
                    .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_ssh_profile_row(
    profile: SSHProfileSummary,
    selected_profile_id: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let active = selected_profile_id
        .map(|id| id == profile.id)
        .unwrap_or(false);
    let profile_id = profile.id.clone();
    div()
        .id(SharedString::from(format!(
            "settings-ssh-profile-{}",
            profile.id
        )))
        .min_h(px(58.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(18.0))
        .cursor_pointer()
        .bg(if active {
            color(theme::BG_ROW_HOVER)
        } else {
            color(0xFFFFFF).opacity(0.0)
        })
        .hover(|style| style.bg(color(theme::BG_ROW_HOVER)))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_ssh_profile(profile_id.clone(), window, cx)
        }))
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(profile.name),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(profile.endpoint),
                ),
        )
        .child(settings_status_tag(
            profile.credential_kind,
            theme::TEXT_DIM,
        ))
        .into_any_element()
}

fn settings_remote_pane(
    remote: &RemoteSummary,
    _selected_device_id: Option<&str>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let active_pairing_rows = remote
        .pairing
        .as_ref()
        .map(|pairing| {
            let pairing_id = pairing.pairing_id.clone();
            vec![
                div()
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(theme::TEXT))
                                    .child("当前配对码"),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(color(theme::TEXT_DIM))
                                    .truncate()
                                    .child(format!(
                                        "有效期至 {}",
                                        empty_label(&pairing.expires_at)
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(settings_status_tag(pairing.code.clone(), theme::ACCENT))
                            .child(settings_small_button(
                                format!("settings-remote-cancel-pairing-{}", pairing.pairing_id),
                                "取消",
                                cx,
                                move |app, _event, window, cx| {
                                    app.cancel_remote_pairing(pairing_id.clone(), window, cx)
                                },
                            )),
                    )
                    .into_any_element(),
                div()
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(color(theme::TEXT_DIM))
                    .truncate()
                    .child(pairing.qr_payload.clone())
                    .into_any_element(),
            ]
        })
        .unwrap_or_default();
    let pending_pairing_rows = remote
        .pending_pairing_list
        .iter()
        .cloned()
        .map(|pairing| {
            let confirm_id = pairing.id.clone();
            let reject_id = pairing.id.clone();
            div()
                .py(px(10.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .truncate()
                                .child(empty_label(&pairing.device_name)),
                        )
                        .child(
                            div()
                                .mt(px(3.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(format!("配对码 {}", empty_label(&pairing.code))),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(settings_small_button(
                            format!("settings-remote-confirm-pairing-{}", pairing.id),
                            "确认",
                            cx,
                            move |app, _event, window, cx| {
                                app.confirm_remote_pairing(confirm_id.clone(), window, cx)
                            },
                        ))
                        .child(settings_small_button(
                            format!("settings-remote-reject-pairing-{}", pairing.id),
                            "拒绝",
                            cx,
                            move |app, _event, window, cx| {
                                app.reject_remote_pairing(reject_id.clone(), window, cx)
                            },
                        )),
                )
                .into_any_element()
        })
        .collect::<Vec<_>>();
    let device_rows = if remote.device_list.is_empty() {
        vec![
            div()
                .py(px(12.0))
                .text_size(px(14.0))
                .line_height(px(18.0))
                .text_color(color(theme::TEXT_DIM))
                .child(if remote.enabled {
                    "还没有配对设备。"
                } else {
                    "配对手机后可以在移动端控制终端。"
                })
                .into_any_element(),
        ]
    } else {
        remote
            .device_list
            .iter()
            .cloned()
            .map(|device| {
                let device_id = device.id.clone();
                let remove_id = device.id.clone();
                div()
                    .id(SharedString::from(format!(
                        "settings-remote-device-{}",
                        device.id
                    )))
                    .min_h(px(58.0))
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(18.0))
                    .cursor_pointer()
                    .on_click(cx.listener(move |app, _event, window, cx| {
                        app.select_remote_device(device_id.clone(), window, cx)
                    }))
                    .child(
                        div()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(
                                div()
                                    .text_size(px(14.0))
                                    .line_height(px(18.0))
                                    .font_weight(FontWeight::SEMIBOLD)
                                    .text_color(color(theme::TEXT))
                                    .child(empty_label(&device.name)),
                            )
                            .child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(px(12.0))
                                    .line_height(px(16.0))
                                    .text_color(color(theme::TEXT_DIM))
                                    .truncate()
                                    .child(format!(
                                        "{} · last seen {}",
                                        empty_label(&device.id),
                                        empty_label(&device.last_seen)
                                    )),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(if device.online.unwrap_or(false) {
                                settings_status_tag("在线", theme::GREEN)
                            } else {
                                settings_status_tag("离线", theme::TEXT_DIM)
                            })
                            .child(settings_small_button(
                                format!("settings-remote-remove-{}", device.id),
                                "移除",
                                cx,
                                move |app, _event, window, cx| {
                                    app.select_remote_device(remove_id.clone(), window, cx);
                                    app.revoke_selected_remote_device(window, cx);
                                },
                            )),
                    )
                    .into_any_element()
            })
            .collect::<Vec<_>>()
    };

    settings_form(vec![
        settings_card(
            Some("服务器"),
            None,
            vec![
                settings_row(
                    "中继服务器 URL",
                    None,
                    settings_text_input(
                        "settings-remote-server-url",
                        &remote.relay,
                        "https://relay.example.com",
                        false,
                        window,
                        cx,
                        |app, value, window, cx| app.set_remote_server_url(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "启用远程主机",
                    None,
                    settings_toggle(
                        "settings-remote-enabled",
                        remote.enabled,
                        cx,
                        |app, window, cx| app.toggle_remote_host(window, cx),
                    ),
                )
                .into_any_element(),
                div()
                    .py(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(
                        div()
                            .size(px(8.0))
                            .rounded_full()
                            .bg(color(if remote.enabled {
                                theme::GREEN
                            } else {
                                theme::TEXT_DIM
                            })),
                    )
                    .child(
                        div()
                            .min_w_0()
                            .flex_1()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::TEXT_DIM))
                            .truncate()
                            .child(remote_status_label(&remote.status)),
                    )
                    .child(settings_small_button(
                        "settings-remote-reconnect",
                        "重连",
                        cx,
                        |app, _event, window, cx| app.reconnect_remote(window, cx),
                    ))
                    .into_any_element(),
            ],
        )
        .into_any_element(),
        settings_card(
            Some("设备"),
            Some(format!(
                "{} 个设备，{} 个在线，{} 个等待配对。",
                remote.devices, remote.online_devices, remote.pending_pairings
            )),
            {
                let mut rows = vec![
                    div()
                        .pb(px(10.0))
                        .flex()
                        .justify_end()
                        .gap(px(8.0))
                        .child(settings_small_button(
                            "settings-remote-create-pairing",
                            "创建配对",
                            cx,
                            |app, _event, window, cx| app.create_remote_pairing(window, cx),
                        ))
                        .child(settings_small_button(
                            "settings-remote-refresh",
                            "刷新",
                            cx,
                            |app, _event, window, cx| app.refresh_remote_devices(window, cx),
                        ))
                        .into_any_element(),
                ];
                rows.extend(active_pairing_rows);
                rows.extend(pending_pairing_rows);
                rows.extend(device_rows);
                rows
            },
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_shortcuts_pane(
    settings: &SettingsSummary,
    recording_id: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(vec![
        settings_card(
            Some("快捷键"),
            None,
            shortcut_definitions()
                .into_iter()
                .map(|shortcut| shortcut_row(shortcut, settings, recording_id, cx))
                .collect(),
        )
        .into_any_element(),
        settings_card(
            Some("项目切换快捷键"),
            None,
            vec![
                div()
                    .py(px(8.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(color(theme::TEXT_DIM))
                    .child(if cfg!(target_os = "macos") {
                        "使用 ⌘1-⌘9 按侧边栏顺序切换项目。"
                    } else {
                        "使用 Ctrl+1-Ctrl+9 按侧边栏顺序切换项目。"
                    })
                    .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

#[derive(Clone, Copy)]
struct ShortcutDefinition {
    id: &'static str,
    label: &'static str,
    default_value: &'static str,
}

fn shortcut_definitions() -> Vec<ShortcutDefinition> {
    let primary = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    vec![
        ShortcutDefinition {
            id: "view.terminal",
            label: "终端视图",
            default_value: primary_static(primary, "1"),
        },
        ShortcutDefinition {
            id: "view.files",
            label: "文件视图",
            default_value: primary_static(primary, "2"),
        },
        ShortcutDefinition {
            id: "view.review",
            label: "评审视图",
            default_value: primary_static(primary, "3"),
        },
        ShortcutDefinition {
            id: "project.create",
            label: "新建项目",
            default_value: primary_static(primary, "N"),
        },
        ShortcutDefinition {
            id: "settings.open",
            label: "打开设置",
            default_value: primary_static(primary, ","),
        },
        ShortcutDefinition {
            id: "task.create",
            label: "新建工作树",
            default_value: primary_static(primary, "N"),
        },
        ShortcutDefinition {
            id: "editor.save",
            label: "保存文件",
            default_value: primary_static(primary, "S"),
        },
        ShortcutDefinition {
            id: "editor.search",
            label: "搜索文件",
            default_value: primary_static(primary, "F"),
        },
        ShortcutDefinition {
            id: "close.active",
            label: "关闭当前项目",
            default_value: primary_static(primary, "W"),
        },
    ]
}

fn primary_static(primary: &str, key: &str) -> &'static str {
    match (primary, key) {
        ("⌘", "1") => "⌘1",
        ("⌘", "2") => "⌘2",
        ("⌘", "3") => "⌘3",
        ("⌘", "N") => "⌘N",
        ("⌘", ",") => "⌘,",
        ("⌘", "S") => "⌘S",
        ("⌘", "F") => "⌘F",
        ("⌘", "W") => "⌘W",
        (_, "1") => "Ctrl+1",
        (_, "2") => "Ctrl+2",
        (_, "3") => "Ctrl+3",
        (_, "N") => "Ctrl+N",
        (_, ",") => "Ctrl+,",
        (_, "S") => "Ctrl+S",
        (_, "F") => "Ctrl+F",
        (_, "W") => "Ctrl+W",
        _ => "",
    }
}

fn shortcut_row(
    shortcut: ShortcutDefinition,
    settings: &SettingsSummary,
    recording_id: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let is_recording = recording_id == Some(shortcut.id);
    let customized = settings.shortcuts.contains_key(shortcut.id);
    let value = if is_recording {
        "录制快捷键".to_string()
    } else {
        settings
            .shortcuts
            .get(shortcut.id)
            .cloned()
            .unwrap_or_else(|| shortcut.default_value.to_string())
    };

    let shortcut_id = shortcut.id;
    settings_row(
        shortcut.label,
        None,
        div()
            .w(relative(0.3))
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(
                Button::new(SharedString::from(format!("shortcut-record-{shortcut_id}")))
                    .secondary()
                    .text_color(color(theme::TEXT))
                    .bg(color(0xFFFFFF).opacity(if is_recording { 0.10 } else { 0.055 }))
                    .flex_1()
                    .justify_start()
                    .on_click(cx.listener(move |app, _event, window, cx| {
                        app.record_shortcut(shortcut_id, window, cx)
                    }))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(18.0))
                            .truncate()
                            .child(value),
                    ),
            )
            .when(customized, |this| {
                this.child(settings_small_button(
                    format!("shortcut-reset-{shortcut_id}"),
                    "撤销",
                    cx,
                    move |app, _event, window, cx| app.reset_shortcut(shortcut_id, window, cx),
                ))
            })
            .into_any_element(),
    )
    .into_any_element()
}

fn settings_experiments_pane(agent_split_enabled: bool, cx: &mut Context<CoduxApp>) -> AnyElement {
    settings_form(vec![
        settings_card(
            Some("分屏窗格"),
            None,
            vec![
                settings_row(
                    "Agent Split",
                    Some(
                        "启用后，创建分屏时可以选择 Terminal 或 Agent。关闭时按普通终端分屏创建。"
                            .to_string(),
                    ),
                    settings_toggle(
                        "settings-agent-split",
                        agent_split_enabled,
                        cx,
                        |app, _window, cx| {
                            let next = !app.agent_split_enabled;
                            app.set_agent_split_enabled(next, cx)
                        },
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_developer_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(vec![
        settings_card(
            None,
            None,
            vec![
                settings_row(
                    "Performance Monitor HUD",
                    None,
                    settings_toggle(
                        "settings-dev-hud",
                        settings.developer_hud,
                        cx,
                        |app, window, cx| app.toggle_developer_hud(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    "Performance Monitor Interval",
                    None,
                    settings_select(
                        "settings-dev-refresh",
                        &settings.developer_refresh,
                        developer_refresh_options(),
                        window,
                        cx,
                        |app, value, window, cx| app.set_developer_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_runtime_tool_block(
    label: &'static str,
    tool_key: &'static str,
    model_key: &'static str,
    permission: &str,
    model: &str,
    placeholder: &'static str,
    include_codex_effort: bool,
    codex_effort: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut children = vec![
        div()
            .text_size(px(14.0))
            .line_height(px(18.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(color(theme::TEXT))
            .child(label)
            .into_any_element(),
        settings_row(
            "完整权限",
            None,
            settings_select(
                tool_key,
                permission,
                runtime_tool_permission_options(),
                window,
                cx,
                move |app, value, window, cx| {
                    app.set_runtime_tool_permission(tool_key, value, window, cx)
                },
            ),
        )
        .into_any_element(),
        settings_row(
            "默认模型",
            None,
            settings_text_input(
                SharedString::from(format!("settings-{model_key}")),
                model,
                placeholder,
                false,
                window,
                cx,
                move |app, value, window, cx| {
                    app.set_runtime_tool_model(model_key, value, window, cx)
                },
            ),
        )
        .into_any_element(),
    ];
    if include_codex_effort {
        children.push(
            settings_row(
                "推理强度",
                None,
                settings_select(
                    "settings-codex-effort",
                    codex_effort,
                    codex_effort_options(),
                    window,
                    cx,
                    |app, value, window, cx| app.set_codex_effort(value, window, cx),
                ),
            )
            .into_any_element(),
        );
    }

    div()
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(2.0))
        .children(children)
        .into_any_element()
}

fn settings_runtime_tools_header(
    permissions: &ToolPermissionsSummary,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let status = if permissions.available {
        ("已同步", theme::GREEN)
    } else {
        ("未同步", theme::ORANGE)
    };

    div()
        .py(px(8.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(14.0))
        .child(
            div()
                .min_w_0()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .child(format!("{} 个完整权限工具", permissions.full_access_count)),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .text_size(px(12.0))
                        .line_height(px(16.0))
                        .text_color(color(theme::TEXT_DIM))
                        .truncate()
                        .child(if permissions.path.is_empty() {
                            "权限文件尚未生成".to_string()
                        } else {
                            permissions.path.clone()
                        }),
                )
                .when_some(permissions.error.clone(), |this, error| {
                    this.child(
                        div()
                            .mt(px(4.0))
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::ORANGE))
                            .truncate()
                            .child(error),
                    )
                }),
        )
        .child(
            div()
                .flex_shrink_0()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(settings_status_tag(status.0, status.1))
                .child(settings_small_button(
                    "settings-sync-runtime-tools",
                    "同步权限",
                    cx,
                    |app, _event, window, cx| app.sync_tool_permissions(window, cx),
                )),
        )
        .into_any_element()
}

fn settings_ai_provider_card(
    provider: codux_runtime::settings::AIProviderSummary,
    selected_provider_id: Option<&str>,
    testing_provider_id: Option<&str>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let _active = selected_provider_id
        .map(|id| id == provider.id)
        .unwrap_or(false);
    let select_id = provider.id.clone();
    let enabled_id = provider.id.clone();
    let memory_id = provider.id.clone();
    let kind_id = provider.id.clone();
    let name_id = provider.id.clone();
    let model_id = provider.id.clone();
    let base_url_id = provider.id.clone();
    let api_key_id = provider.id.clone();
    let testing = testing_provider_id
        .map(|id| id == provider.id)
        .unwrap_or(false);
    let test_disabled = testing_provider_id.is_some()
        || (!provider.api_key_configured && !provider_allows_empty_api_key(&provider.kind));

    div()
        .id(SharedString::from(format!(
            "settings-provider-{}",
            provider.id
        )))
        .py(px(12.0))
        .flex()
        .flex_col()
        .gap(px(10.0))
        .on_click(cx.listener(move |app, _event, window, cx| {
            app.select_ai_provider(select_id.clone(), window, cx)
        }))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .text_size(px(14.0))
                        .line_height(px(18.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(color(theme::TEXT))
                        .truncate()
                        .child(provider.display_name.clone()),
                )
                .child(settings_toggle(
                    format!("settings-provider-enabled-{}", provider.id),
                    provider.enabled,
                    cx,
                    move |app, window, cx| {
                        let next = !app
                            .state
                            .settings
                            .ai_providers
                            .iter()
                            .find(|item| item.id == enabled_id)
                            .map(|item| item.enabled)
                            .unwrap_or(false);
                        app.set_ai_provider_bool(enabled_id.clone(), "isEnabled", next, window, cx)
                    },
                )),
        )
        .child(settings_row(
            "类型",
            None,
            settings_select(
                format!("settings-provider-kind-{}", provider.id),
                &provider.kind,
                ai_provider_kind_options(),
                window,
                cx,
                move |app, value, window, cx| {
                    app.update_ai_provider_string(kind_id.clone(), "kind", value, window, cx)
                },
            ),
        ))
        .child(settings_row(
            "名称",
            None,
            settings_text_input(
                SharedString::from(format!("settings-provider-name-{}", provider.id)),
                provider.display_name.clone(),
                "OpenAI API",
                false,
                window,
                cx,
                move |app, value, window, cx| {
                    app.update_ai_provider_string(name_id.clone(), "displayName", value, window, cx)
                },
            ),
        ))
        .child(settings_row(
            "模型",
            None,
            settings_text_input(
                SharedString::from(format!("settings-provider-model-{}", provider.id)),
                provider.model.clone(),
                "gpt-4.1-mini",
                false,
                window,
                cx,
                move |app, value, window, cx| {
                    app.update_ai_provider_string(model_id.clone(), "model", value, window, cx)
                },
            ),
        ))
        .child(settings_row(
            "Base URL",
            None,
            settings_text_input(
                SharedString::from(format!("settings-provider-base-url-{}", provider.id)),
                provider.base_url.clone(),
                "https://api.openai.com/v1",
                false,
                window,
                cx,
                move |app, value, window, cx| {
                    app.update_ai_provider_string(base_url_id.clone(), "baseUrl", value, window, cx)
                },
            ),
        ))
        .child(settings_row(
            "API Key",
            Some(if provider.api_key_configured {
                "已配置。输入新值可替换。".to_string()
            } else {
                "未配置。".to_string()
            }),
            settings_text_input(
                SharedString::from(format!("settings-provider-api-key-{}", provider.id)),
                "",
                if provider.api_key_configured {
                    "已配置"
                } else {
                    "API Key"
                },
                true,
                window,
                cx,
                move |app, value, window, cx| {
                    if !value.trim().is_empty() {
                        app.update_ai_provider_string(
                            api_key_id.clone(),
                            "apiKey",
                            value,
                            window,
                            cx,
                        )
                    }
                },
            ),
        ))
        .child(settings_row(
            "用于记忆提取",
            None,
            settings_toggle(
                format!("settings-provider-memory-{}", provider.id),
                provider.memory_extraction,
                cx,
                move |app, window, cx| {
                    let next = !app
                        .state
                        .settings
                        .ai_providers
                        .iter()
                        .find(|item| item.id == memory_id)
                        .map(|item| item.memory_extraction)
                        .unwrap_or(false);
                    app.set_ai_provider_bool(
                        memory_id.clone(),
                        "useForMemoryExtraction",
                        next,
                        window,
                        cx,
                    )
                },
            ),
        ))
        .child(
            div()
                .flex()
                .justify_end()
                .gap(px(8.0))
                .child(
                    Button::new(SharedString::from(format!(
                        "settings-provider-test-{}",
                        provider.id
                    )))
                    .secondary()
                    .loading(testing)
                    .disabled(test_disabled)
                    .text_color(color(theme::TEXT))
                    .on_click(cx.listener({
                        let test_id = provider.id.clone();
                        move |app, _event, window, cx| {
                            app.test_ai_provider(test_id.clone(), window, cx)
                        }
                    }))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::TEXT))
                            .child(if testing { "测试中" } else { "测试" }),
                    ),
                )
                .child(settings_small_button(
                    format!("settings-provider-remove-{}", provider.id),
                    "移除",
                    cx,
                    {
                        let remove_id = provider.id.clone();
                        move |app, _event, window, cx| {
                            app.remove_ai_provider(remove_id.clone(), window, cx)
                        }
                    },
                )),
        )
        .into_any_element()
}

fn settings_notification_card(
    channel: codux_runtime::notification::NotificationChannelSummary,
    testing_channel_id: Option<&str>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let enabled_id = channel.id.clone();
    let endpoint_id = channel.id.clone();
    let token_id = channel.id.clone();
    let testing = testing_channel_id
        .map(|id| id == channel.id)
        .unwrap_or(false);
    let test_disabled = testing_channel_id.is_some();
    settings_card(None, None, {
        let mut rows = vec![
            div()
                .flex()
                .items_start()
                .justify_between()
                .gap(px(16.0))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(px(14.0))
                                .line_height(px(18.0))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(color(theme::TEXT))
                                .child(channel.label.clone()),
                        )
                        .child(
                            div()
                                .mt(px(4.0))
                                .text_size(px(12.0))
                                .line_height(px(16.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(notification_channel_description(&channel.id)),
                        ),
                )
                .child(settings_toggle(
                    format!("settings-notification-enabled-{}", channel.id),
                    channel.enabled,
                    cx,
                    move |app, window, cx| {
                        let next = !app
                            .state
                            .notifications
                            .channels
                            .iter()
                            .find(|item| item.id == enabled_id)
                            .map(|item| item.enabled)
                            .unwrap_or(false);
                        app.set_notification_channel_enabled(enabled_id.clone(), next, window, cx)
                    },
                ))
                .into_any_element(),
        ];
        if channel.enabled {
            rows.extend([
                settings_row(
                    notification_endpoint_label(&channel.id),
                    None,
                    settings_text_input(
                        SharedString::from(format!(
                            "settings-notification-endpoint-{}",
                            channel.id
                        )),
                        channel.endpoint.clone(),
                        notification_endpoint_label(&channel.id),
                        false,
                        window,
                        cx,
                        move |app, value, window, cx| {
                            app.update_notification_channel_string(
                                endpoint_id.clone(),
                                "endpoint",
                                value,
                                window,
                                cx,
                            )
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    notification_token_label(&channel.id),
                    None,
                    settings_text_input(
                        SharedString::from(format!("settings-notification-token-{}", channel.id)),
                        channel.token.clone(),
                        notification_token_label(&channel.id),
                        true,
                        window,
                        cx,
                        move |app, value, window, cx| {
                            app.update_notification_channel_string(
                                token_id.clone(),
                                "token",
                                value,
                                window,
                                cx,
                            )
                        },
                    ),
                )
                .into_any_element(),
                div()
                    .flex()
                    .justify_end()
                    .child(settings_small_button_state(
                        format!("settings-notification-test-{}", channel.id),
                        if testing { "测试中" } else { "测试" },
                        testing,
                        test_disabled,
                        cx,
                        move |app, _event, window, cx| {
                            app.test_notification_channel(channel.id.clone(), window, cx)
                        },
                    ))
                    .into_any_element(),
            ]);
        }
        rows
    })
    .into_any_element()
}

#[derive(Clone, Copy)]
struct TerminalThemePreview {
    background: u32,
    foreground: u32,
    muted_foreground: u32,
    selection: u32,
}

#[derive(Clone, Copy)]
struct ThemeColorValue {
    label: &'static str,
    color: u32,
}

#[derive(Clone, Copy)]
struct IconStyleValue {
    value: &'static str,
    label: &'static str,
}

fn opt(value: &'static str, label: &'static str) -> (String, SharedString) {
    (value.to_string(), SharedString::from(label))
}

fn language_options() -> Vec<(String, SharedString)> {
    vec![
        ("system", "跟随系统"),
        ("simplifiedChinese", "简体中文"),
        ("traditionalChinese", "繁體中文"),
        ("english", "English"),
        ("japanese", "日本語"),
        ("korean", "한국어"),
        ("french", "Français"),
        ("german", "Deutsch"),
        ("spanish", "Español"),
        ("portugueseBrazil", "Português (Brasil)"),
        ("russian", "Русский"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn shell_options() -> Vec<(String, SharedString)> {
    vec![
        ("system", "跟随系统"),
        ("zsh", "zsh"),
        ("bash", "bash"),
        ("sh", "sh"),
        ("fish", "fish"),
        ("pwsh.exe", "PowerShell 7"),
        ("powershell.exe", "Windows PowerShell"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn sleep_mode_options() -> Vec<(String, SharedString)> {
    vec![
        ("off", "关闭"),
        ("always", "始终开启"),
        ("powerAdapterOnly", "仅电源适配器"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn git_refresh_options() -> Vec<(String, SharedString)> {
    interval_options(&[
        ("30", "30 sec"),
        ("60", "1 min"),
        ("120", "2 min"),
        ("300", "5 min"),
        ("600", "10 min"),
    ])
}

fn ai_refresh_options() -> Vec<(String, SharedString)> {
    interval_options(&[
        ("60", "1 min"),
        ("120", "2 min"),
        ("180", "3 min"),
        ("300", "5 min"),
        ("600", "10 min"),
    ])
}

fn ai_background_refresh_options() -> Vec<(String, SharedString)> {
    interval_options(&[
        ("300", "5 min"),
        ("600", "10 min"),
        ("900", "15 min"),
        ("1800", "30 min"),
    ])
}

fn developer_refresh_options() -> Vec<(String, SharedString)> {
    interval_options(&[
        ("1", "1 sec"),
        ("2", "2 sec"),
        ("3", "3 sec"),
        ("5", "5 sec"),
        ("10", "10 sec"),
    ])
}

fn interval_options(options: &[(&'static str, &'static str)]) -> Vec<(String, SharedString)> {
    options
        .iter()
        .map(|(value, label)| opt(value, label))
        .collect()
}

fn statistics_mode_options() -> Vec<(String, SharedString)> {
    vec![("normalized", "排除缓存"), ("includingCache", "包含缓存")]
        .into_iter()
        .map(|(value, label)| opt(value, label))
        .collect()
}

fn update_channel_options() -> Vec<(String, SharedString)> {
    vec![("stable", "稳定版"), ("beta", "测试版")]
        .into_iter()
        .map(|(value, label)| opt(value, label))
        .collect()
}

fn system_theme_options() -> Vec<(&'static str, &'static str)> {
    vec![("Auto", "跟随系统")]
}

fn dark_theme_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Tokyo Night Storm", "Tokyo Storm"),
        ("Tokyo Night Night", "Tokyo Night"),
        ("Catppuccin Mocha", "Mocha"),
        ("Rose Pine Moon", "Rose Pine"),
        ("Kanagawa Wave", "Kanagawa"),
        ("Material Ocean", "Ocean"),
        ("Ayu Mirage", "Ayu"),
        ("Dracula", "Dracula"),
        ("Dracula+", "Dracula+"),
        ("GitHub Dark", "GitHub Dark"),
        ("Gruvbox Dark", "Gruvbox"),
        ("Gruvbox Material Dark", "Gruvbox Material"),
        ("Nord", "Nord"),
        ("Flexoki Dark", "Flexoki Dark"),
    ]
}

fn light_theme_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Tokyo Night Day", "Tokyo Day"),
        ("GitHub Light", "GitHub Light"),
        ("Catppuccin Latte", "Latte"),
        ("Flexoki Light", "Flexoki Light"),
        ("Gruvbox Light", "Gruvbox Light"),
        ("Gruvbox Material Light", "Gruvbox Material"),
        ("Nord Light", "Nord Light"),
        ("Atom One Light", "Atom One"),
    ]
}

fn terminal_theme_preview(value: &str) -> TerminalThemePreview {
    match value {
        "Tokyo Night Day" => TerminalThemePreview {
            background: 0xE1E2E7,
            foreground: 0x3760BF,
            muted_foreground: 0x848CB5,
            selection: 0x99A7DF,
        },
        "GitHub Light" => TerminalThemePreview {
            background: 0xFFFFFF,
            foreground: 0x24292F,
            muted_foreground: 0x6E7781,
            selection: 0xDDEBFF,
        },
        "Catppuccin Latte" => TerminalThemePreview {
            background: 0xEFF1F5,
            foreground: 0x4C4F69,
            muted_foreground: 0x8C8FA1,
            selection: 0xBCC0CC,
        },
        "Flexoki Light" => TerminalThemePreview {
            background: 0xFFFCF0,
            foreground: 0x100F0F,
            muted_foreground: 0x6F6E69,
            selection: 0xE6E4D9,
        },
        "Gruvbox Light" | "Gruvbox Material Light" => TerminalThemePreview {
            background: 0xFBF1C7,
            foreground: 0x3C3836,
            muted_foreground: 0x7C6F64,
            selection: 0xD5C4A1,
        },
        "Nord Light" => TerminalThemePreview {
            background: 0xECEFF4,
            foreground: 0x2E3440,
            muted_foreground: 0x6B7280,
            selection: 0xD8DEE9,
        },
        "Atom One Light" => TerminalThemePreview {
            background: 0xFAFAFA,
            foreground: 0x383A42,
            muted_foreground: 0xA0A1A7,
            selection: 0xE5E5E6,
        },
        "Dracula" | "Dracula+" => TerminalThemePreview {
            background: 0x282A36,
            foreground: 0xF8F8F2,
            muted_foreground: 0x6272A4,
            selection: 0x44475A,
        },
        "Catppuccin Mocha" => TerminalThemePreview {
            background: 0x1E1E2E,
            foreground: 0xCDD6F4,
            muted_foreground: 0x7F849C,
            selection: 0x45475A,
        },
        "Rose Pine Moon" => TerminalThemePreview {
            background: 0x232136,
            foreground: 0xE0DEF4,
            muted_foreground: 0x908CAA,
            selection: 0x393552,
        },
        "Kanagawa Wave" => TerminalThemePreview {
            background: 0x1F1F28,
            foreground: 0xDCD7BA,
            muted_foreground: 0x727169,
            selection: 0x2D4F67,
        },
        "Material Ocean" => TerminalThemePreview {
            background: 0x0F111A,
            foreground: 0xA6ACCD,
            muted_foreground: 0x676E95,
            selection: 0x1F2233,
        },
        "Ayu Mirage" => TerminalThemePreview {
            background: 0x1F2430,
            foreground: 0xCBCCC6,
            muted_foreground: 0x707A8C,
            selection: 0x34455A,
        },
        "GitHub Dark" => TerminalThemePreview {
            background: 0x0D1117,
            foreground: 0xC9D1D9,
            muted_foreground: 0x8B949E,
            selection: 0x264F78,
        },
        "Gruvbox Dark" | "Gruvbox Material Dark" => TerminalThemePreview {
            background: 0x282828,
            foreground: 0xEBDBB2,
            muted_foreground: 0xA89984,
            selection: 0x504945,
        },
        "Nord" => TerminalThemePreview {
            background: 0x2E3440,
            foreground: 0xD8DEE9,
            muted_foreground: 0x81A1C1,
            selection: 0x434C5E,
        },
        "Flexoki Dark" => TerminalThemePreview {
            background: 0x100F0F,
            foreground: 0xCECDC3,
            muted_foreground: 0x878580,
            selection: 0x343331,
        },
        _ => TerminalThemePreview {
            background: theme::BG_TERMINAL,
            foreground: theme::TEXT,
            muted_foreground: theme::TEXT_DIM,
            selection: theme::BG_ROW_ACTIVE,
        },
    }
}

fn theme_color_values() -> Vec<ThemeColorValue> {
    vec![
        ThemeColorValue {
            label: "Blue",
            color: 0x3B82F6,
        },
        ThemeColorValue {
            label: "Sky",
            color: 0x0EA5E9,
        },
        ThemeColorValue {
            label: "Cyan",
            color: 0x06B6D4,
        },
        ThemeColorValue {
            label: "Teal",
            color: 0x14B8A6,
        },
        ThemeColorValue {
            label: "Emerald",
            color: 0x10B981,
        },
        ThemeColorValue {
            label: "Green",
            color: 0x22C55E,
        },
        ThemeColorValue {
            label: "Lime",
            color: 0x84CC16,
        },
        ThemeColorValue {
            label: "Amber",
            color: 0xF59E0B,
        },
        ThemeColorValue {
            label: "Orange",
            color: 0xF97316,
        },
        ThemeColorValue {
            label: "Red",
            color: 0xEF4444,
        },
        ThemeColorValue {
            label: "Rose",
            color: 0xF43F5E,
        },
        ThemeColorValue {
            label: "Pink",
            color: 0xEC4899,
        },
        ThemeColorValue {
            label: "Fuchsia",
            color: 0xD946EF,
        },
        ThemeColorValue {
            label: "Purple",
            color: 0xA855F7,
        },
        ThemeColorValue {
            label: "Violet",
            color: 0x8B5CF6,
        },
        ThemeColorValue {
            label: "Indigo",
            color: 0x6366F1,
        },
    ]
}

fn icon_style_values() -> Vec<IconStyleValue> {
    vec![
        IconStyleValue {
            value: "default",
            label: "默认",
        },
        IconStyleValue {
            value: "cobalt",
            label: "Cobalt",
        },
        IconStyleValue {
            value: "sunset",
            label: "Sunset",
        },
        IconStyleValue {
            value: "forest",
            label: "Forest",
        },
    ]
}

fn app_icon_palette(style: &str) -> (u32, u32) {
    match style {
        "cobalt" => (0x1D4ED8, 0x60A5FA),
        "sunset" => (0xEA580C, 0xF97316),
        "forest" => (0x047857, 0x34D399),
        _ => (0x111827, 0x3B82F6),
    }
}

fn terminal_scrollback_options() -> Vec<(String, SharedString)> {
    ["500", "1000", "2000", "5000", "10000"]
        .into_iter()
        .map(|value| {
            (
                value.to_string(),
                SharedString::from(format!("{value} lines")),
            )
        })
        .collect()
}

fn pet_speech_mode_options() -> Vec<(String, SharedString)> {
    vec![
        ("mixed", "混合"),
        ("off", "关闭"),
        ("encourage", "鼓励"),
        ("roast", "吐槽"),
        ("flirty", "调皮"),
        ("chuunibyou", "中二"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn pet_speech_frequency_options() -> Vec<(String, SharedString)> {
    vec![
        ("quiet", "安静"),
        ("normal", "正常"),
        ("lively", "活跃"),
        ("chatterbox", "话痨"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn runtime_tool_permission_options() -> Vec<(String, SharedString)> {
    vec![("default", "默认"), ("fullAccess", "完整权限")]
        .into_iter()
        .map(|(value, label)| opt(value, label))
        .collect()
}

fn ssh_credential_options() -> Vec<(String, SharedString)> {
    vec![
        ("none", "SSH Agent"),
        ("password", "密码"),
        ("privateKey", "私钥"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn codex_effort_options() -> Vec<(String, SharedString)> {
    vec![
        ("none", "None"),
        ("minimal", "Minimal"),
        ("low", "Low"),
        ("medium", "Medium"),
        ("high", "High"),
        ("xhigh", "XHigh"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn git_provider_options(settings: &SettingsSummary) -> Vec<(String, SharedString)> {
    let mut options = vec![opt("automatic", "自动"), opt("off", "关闭")];
    options.extend(
        settings
            .ai_providers
            .iter()
            .filter(|provider| provider.enabled && provider.kind != "localLlama")
            .map(|provider| {
                (
                    provider.id.clone(),
                    SharedString::from(provider.display_name.clone()),
                )
            }),
    );
    options
}

fn git_tone_options() -> Vec<(String, SharedString)> {
    vec![
        ("conventional", "Conventional Commits"),
        ("concise", "简洁"),
        ("sentence", "普通句子"),
        ("changelog", "更新日志"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn git_language_options() -> Vec<(String, SharedString)> {
    let mut options = vec![opt("application", "跟随应用")];
    options.extend(
        language_options()
            .into_iter()
            .filter(|(value, _)| value != "system"),
    );
    options
}

fn ai_provider_kind_options() -> Vec<(String, SharedString)> {
    vec![
        ("openai", "OpenAI"),
        ("openAICompatible", "OpenAI-Compatible API"),
        ("anthropic", "Claude API"),
        ("deepseek", "DeepSeek"),
        ("gemini", "Gemini"),
        ("groq", "Groq"),
        ("openrouter", "OpenRouter"),
        ("ollama", "Ollama"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn ai_provider_options(settings: &SettingsSummary, purpose: &str) -> Vec<(String, SharedString)> {
    let mut providers = settings
        .ai_providers
        .iter()
        .filter(|provider| {
            provider.enabled
                && provider.kind != "localLlama"
                && (purpose != "memory" || provider.memory_extraction)
        })
        .cloned()
        .collect::<Vec<_>>();
    providers.sort_by(|left, right| {
        left.priority
            .cmp(&right.priority)
            .then_with(|| left.display_name.cmp(&right.display_name))
    });

    let mut options = vec![opt("automatic", "自动")];
    options.extend(
        providers
            .into_iter()
            .map(|provider| (provider.id, SharedString::from(provider.display_name))),
    );
    options
}

fn provider_allows_empty_api_key(kind: &str) -> bool {
    matches!(kind, "ollama" | "localLlama")
}

fn memory_extraction_interval_options() -> Vec<(String, SharedString)> {
    interval_options(&[
        ("60", "1 min"),
        ("120", "2 min"),
        ("300", "5 min"),
        ("600", "10 min"),
        ("900", "15 min"),
    ])
}

fn memory_max_index_options() -> Vec<(String, SharedString)> {
    ["5", "10", "20", "50", "100"]
        .into_iter()
        .map(|value| {
            (
                value.to_string(),
                SharedString::from(format!("{value} sessions")),
            )
        })
        .collect()
}

fn notification_endpoint_label(channel_id: &str) -> &'static str {
    match channel_id {
        "bark" => "Server URL",
        "ntfy" => "Topic URL",
        "wxpusher" => "SPT Token",
        "telegram" => "Chat ID",
        "webhook" => "Request URL",
        _ => "Webhook URL",
    }
}

fn notification_token_label(channel_id: &str) -> &'static str {
    match channel_id {
        "bark" => "Device Key",
        "ntfy" => "Bearer Token",
        "wxpusher" => "Token",
        "feishu" => "Hook Token",
        "dingtalk" => "Access Token",
        "wecom" => "Webhook Key",
        "telegram" => "Bot Token",
        "discord" | "slack" => "Optional Auth Token",
        "webhook" => "Bearer Token",
        _ => "Token",
    }
}

fn notification_channel_description(channel_id: &str) -> &'static str {
    match channel_id {
        "bark" => "通过 Bark 服务和设备 Key 发送推送。",
        "ntfy" => "发布消息到 ntfy topic。",
        "wxpusher" => "发送通知到 WxPusher SPT 目标。",
        "feishu" => "通过飞书机器人 webhook 推送消息。",
        "dingtalk" => "通过钉钉机器人 webhook 推送消息。",
        "wecom" => "推送到企业微信群机器人。",
        "telegram" => "通过 Telegram bot token 和 chat id 发送消息。",
        "discord" => "发送通知到 Discord webhook。",
        "slack" => "发送通知到 Slack incoming webhook。",
        "webhook" => "向自定义端点发送 JSON POST 请求。",
        _ => "自定义通知渠道。",
    }
}

fn remote_status_label(value: &str) -> &'static str {
    match value {
        "connected" => "已连接",
        "connecting" => "连接中",
        "registering" => "注册中",
        "failed" => "失败",
        _ => "未连接",
    }
}
