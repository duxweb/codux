use super::ui_helpers::dialog_primary_button;
use super::{CoduxApp, UiRegion, empty_label};
use crate::app::{
    AIProviderTestResult,
    app_select::{CoduxSelectOption, codux_select},
    scroll_compat::ScrollableElement,
};
use crate::heroicons::HeroIconName;
use crate::theme::{self, color};
use codux_runtime::{
    i18n::translate,
    memory::MemorySummary,
    notification::NotificationSummary,
    remote::{RemotePairingInfo, RemotePendingPairing, RemoteSummary},
    settings::{SettingsSummary, locale_from_language_setting},
    tool_permissions::ToolPermissionsSummary,
    update::UpdateSummary,
};
use gpui::{
    AnyElement, AppContext, Context, InteractiveElement, IntoElement, ObjectFit, ParentElement,
    Pixels, Rems, SharedString, StatefulInteractiveElement, Styled, StyledImage, Window,
    WindowControlArea, div, img, prelude::FluentBuilder as _, px, relative, rems,
};
use gpui_component::{
    ActiveTheme, Disableable, Icon, Sizable,
    button::{Button, ButtonVariants},
    group_box::{GroupBox, GroupBoxVariants},
    input::{Input, InputEvent, InputState},
    menu::{DropdownMenu, PopupMenuItem},
    spinner::Spinner,
    switch::Switch,
};
use qrcode::{EcLevel, QrCode, types::Color as QrColor};
use std::ops::RangeInclusive;

const CODUX_MOBILE_DOWNLOAD_URL: &str = "https://codux.dux.cn/features/mobile/";
const SETTINGS_FORM_TEXT_SIZE: Rems = Rems(0.875);
const SETTINGS_FORM_LINE_HEIGHT: Rems = Rems(1.125);
const SETTINGS_FORM_DESCRIPTION_TEXT_SIZE: Rems = Rems(0.75);
const SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT: Rems = Rems(1.0625);
const SETTINGS_ROW_LABEL_MIN_WIDTH: f32 = 180.0;
const SETTINGS_ROW_CONTROL_MIN_WIDTH: f32 = 160.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum SettingsPane {
    General,
    Appearance,
    Pet,
    AI,
    Git,
    Memory,
    Notifications,
    Remote,
    Shortcuts,
    Developer,
}

impl SettingsPane {
    pub(super) fn label(self, language: &str) -> String {
        let key = match self {
            Self::General => "settings.tab.general",
            Self::Appearance => "settings.tab.appearance",
            Self::Pet => "settings.tab.pet",
            Self::AI => "settings.tab.ai",
            Self::Git => "settings.tab.git",
            Self::Memory => "settings.tab.memory",
            Self::Notifications => "settings.tab.notifications",
            Self::Remote => "settings.tab.remote",
            Self::Shortcuts => "settings.tab.shortcuts",
            Self::Developer => "settings.tab.developer",
        };
        match self {
            Self::General => settings_text(language, key, "General"),
            Self::Appearance => settings_text(language, key, "Appearance"),
            Self::Pet => settings_text(language, key, "Pet"),
            Self::AI => settings_text(language, key, "AI"),
            Self::Git => settings_text(language, key, "Git"),
            Self::Memory => settings_text(language, key, "Memory"),
            Self::Notifications => settings_text(language, key, "Notifications"),
            Self::Remote => settings_text(language, key, "Remote"),
            Self::Shortcuts => settings_text(language, key, "Shortcuts"),
            Self::Developer => settings_text(language, key, "Developer"),
        }
    }

    fn icon(self) -> HeroIconName {
        match self {
            Self::General => HeroIconName::Cog6Tooth,
            Self::Appearance => HeroIconName::Swatch,
            Self::Pet => HeroIconName::Heart,
            Self::AI => HeroIconName::CpuChip,
            Self::Git => HeroIconName::ArrowPathRoundedSquare,
            Self::Memory => HeroIconName::BookOpen,
            Self::Notifications => HeroIconName::Bell,
            Self::Remote => HeroIconName::GlobeAlt,
            Self::Shortcuts => HeroIconName::CommandLine,
            Self::Developer => HeroIconName::WrenchScrewdriver,
        }
    }
}

const SETTINGS_PANES: [SettingsPane; 10] = [
    SettingsPane::General,
    SettingsPane::Appearance,
    SettingsPane::Pet,
    SettingsPane::AI,
    SettingsPane::Git,
    SettingsPane::Memory,
    SettingsPane::Notifications,
    SettingsPane::Remote,
    SettingsPane::Shortcuts,
    SettingsPane::Developer,
];

pub(super) fn settings_text(language: &str, key: &str, fallback: &str) -> String {
    let locale = locale_from_language_setting(language);
    translate(&locale, key, fallback)
}

impl CoduxApp {
    fn ensure_terminal_font_families_loaded(&mut self, cx: &mut Context<Self>) {
        if self.terminal_font_families_loaded || self.terminal_font_families_loading {
            return;
        }
        self.terminal_font_families_loading = true;
        let service = self.runtime_service.clone();
        self.runtime_trace("settings", "terminal_font_families load queued");
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let families = codux_runtime::async_runtime::run_limited_blocking(move || {
                service.terminal_font_families()
            })
            .await
            .unwrap_or_default();

            let _ = this.update(cx, |app, cx| {
                app.terminal_font_families = families;
                app.terminal_font_families_loaded = true;
                app.terminal_font_families_loading = false;
                app.runtime_trace(
                    "settings",
                    &format!(
                        "terminal_font_families loaded count={}",
                        app.terminal_font_families.len()
                    ),
                );
                app.invalidate_ui_region(cx, UiRegion::Root);
            });
        })
        .detach();
    }

    pub(super) fn settings_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        self.ensure_terminal_font_families_loaded(cx);
        let pane = self.active_settings_pane;
        let language = self.state.settings.language.as_str();

        div()
            .relative()
            .flex()
            .flex_1()
            .w_full()
            .min_w_0()
            .h_full()
            .bg(cx.theme().background)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .w(px(200.0))
                    .h_full()
                    .flex_shrink_0()
                    .border_r_1()
                    .border_color(cx.theme().sidebar_border)
                    .bg(cx.theme().sidebar)
                    .child(
                        div()
                            .h(if cfg!(target_os = "macos") {
                                px(48.0)
                            } else {
                                px(16.0)
                            })
                            .flex_shrink_0(),
                    )
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
                                settings_nav_row(item, pane == item, language, cx)
                                    .into_any_element()
                            })),
                    ),
            )
            .child(
                div()
                    .relative()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .w_full()
                    .min_w_0()
                    .h_full()
                    .bg(color(theme::BG_COLUMN))
                    .child(
                        div()
                            .h(px(68.0))
                            .flex_shrink_0()
                            .pl(px(28.0))
                            .pr(if cfg!(target_os = "macos") {
                                px(28.0)
                            } else {
                                px(16.0)
                            })
                            .pb(px(14.0))
                            .flex()
                            .items_end()
                            .justify_between()
                            .gap_3()
                            .child(
                                div()
                                    .min_w_0()
                                    .flex_1()
                                    .h_full()
                                    .flex()
                                    .items_end()
                                    .window_control_area(WindowControlArea::Drag)
                                    .text_size(rems(1.25))
                                    .line_height(rems(1.625))
                                    .text_color(cx.theme().foreground)
                                    .child(pane.label(language)),
                            )
                            .when(!cfg!(target_os = "macos"), |this| {
                                this.child(
                                    Button::new("settings-window-close")
                                        .flex_none()
                                        .compact()
                                        .ghost()
                                        .h(px(28.0))
                                        .w(px(28.0))
                                        .text_color(cx.theme().muted_foreground)
                                        .window_control_area(WindowControlArea::Close)
                                        .on_click(|_, window, _| window.remove_window())
                                        .child(Icon::new(HeroIconName::XMark).size_3()),
                                )
                            }),
                    )
                    .child(
                        div()
                            .flex_1()
                            .w_full()
                            .min_w_0()
                            .min_h_0()
                            .overflow_y_scrollbar()
                            .px(px(28.0))
                            .pb(px(28.0))
                            .child(settings_pane_body(self, pane, window, cx)),
                    ),
            )
            .when(
                self.remote_pairing_sheet_open || self.state.remote.pairing.is_some(),
                |this| {
                    this.child(remote_pairing_overlay(
                        self.state.remote.pairing.clone(),
                        self.remote_pairing_creating,
                        self.remote_pairing_error.as_deref(),
                        language,
                        cx,
                    ))
                },
            )
            .when_some(
                self.state.remote.pending_pairing_list.first().cloned(),
                |this, pairing| this.child(remote_pending_pairing_overlay(pairing, language, cx)),
            )
            .when(self.remote_connect_open, |this| {
                this.child(remote_connect_overlay(
                    &self.remote_connect_ticket,
                    &self.remote_connect_name,
                    self.remote_connect_error.as_deref(),
                    self.remote_connect_busy,
                    language,
                    window,
                    cx,
                ))
            })
    }
}

fn settings_nav_row(
    pane: SettingsPane,
    active: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let label = pane.label(language);
    div()
        .id(SharedString::from(format!("settings-nav-{:?}", pane)))
        .h(px(32.0))
        .px(px(10.0))
        .mb(px(6.0))
        .rounded(px(6.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .cursor_pointer()
        .text_color(if active {
            cx.theme().foreground
        } else {
            cx.theme().muted_foreground
        })
        .bg(if active {
            cx.theme().accent
        } else {
            cx.theme().transparent
        })
        .hover(|style| style.bg(cx.theme().list_hover))
        .on_click(cx.listener(move |app, _event, _window, cx| app.set_settings_pane(pane, cx)))
        .child(Icon::new(pane.icon()).size_3p5())
        .child(
            div()
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .child(label),
        )
}

fn settings_pane_body(
    app: &CoduxApp,
    pane: SettingsPane,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    match pane {
        SettingsPane::General => settings_general_pane(
            &app.state.settings,
            app.pending_restart_language.as_deref(),
            &app.terminal_font_families,
            &app.state.update,
            window,
            cx,
        ),
        SettingsPane::Appearance => settings_appearance_pane(
            &app.state.settings,
            app.appearance_vibrancy_slider.clone(),
            window,
            cx,
        ),
        SettingsPane::Pet => settings_pet_pane(&app.state.settings, window, cx),
        SettingsPane::AI => settings_ai_pane(
            &app.state.settings,
            &app.state.tool_permissions,
            app.selected_ai_provider_id.as_deref(),
            app.ai_provider_testing_id.as_deref(),
            app.ai_provider_test_result.as_ref(),
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
            app.state.settings.language.as_str(),
            window,
            cx,
        ),
        SettingsPane::Remote => {
            let saved_hosts = app.runtime_service.saved_remote_hosts();
            let link_states = app.runtime_service.remote_controller_link_states();
            let link_paths = app.runtime_service.remote_controller_link_paths();
            settings_remote_pane(
                &app.state.settings,
                &app.state.remote,
                &saved_hosts,
                &link_states,
                &link_paths,
                app.selected_remote_device_id.as_deref(),
                app.state.settings.language.as_str(),
                app.remote_reconnecting,
                app.remote_pairing_creating,
                window,
                cx,
            )
        }
        SettingsPane::Shortcuts => settings_shortcuts_pane(
            &app.state.settings,
            app.recording_shortcut_id.as_deref(),
            cx,
        ),
        SettingsPane::Developer => settings_developer_pane(&app.state.settings, window, cx),
    }
    .into_any_element()
}

fn settings_form(children: Vec<AnyElement>) -> impl IntoElement {
    div()
        .flex()
        .flex_col()
        .w_full()
        .gap(px(22.0))
        .children(children)
}

fn settings_card(
    title: Option<String>,
    description: Option<String>,
    children: Vec<AnyElement>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    settings_card_with_actions(title, description, None, children, cx)
}

fn settings_card_with_actions(
    title: Option<String>,
    description: Option<String>,
    actions: Option<AnyElement>,
    children: Vec<AnyElement>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let title_element = if title.is_some() || description.is_some() || actions.is_some() {
        Some(
            div()
                .min_h(px(28.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(SETTINGS_FORM_TEXT_SIZE)
                                .line_height(SETTINGS_FORM_LINE_HEIGHT)
                                .text_color(color(theme::TEXT))
                                .child(title.clone().unwrap_or_default()),
                        )
                        .when_some(description, |this, description| {
                            this.child(
                                div()
                                    .mt(px(3.0))
                                    .max_w(px(520.0))
                                    .text_size(SETTINGS_FORM_DESCRIPTION_TEXT_SIZE)
                                    .line_height(SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT)
                                    .text_color(color(theme::TEXT_DIM))
                                    .child(description),
                            )
                        }),
                )
                .child(actions.unwrap_or_else(|| div().hidden().into_any_element())),
        )
    } else {
        None
    };

    div().w_full().child(
        GroupBox::new()
            .w_full()
            .fill()
            .when_some(title_element, |this, title| this.title(title))
            .content_style(
                div()
                    .w_full()
                    .px(px(22.0))
                    .py(px(10.0))
                    .gap(px(0.0))
                    .style()
                    .clone(),
            )
            .children(children.into_iter().enumerate().flat_map(|(index, child)| {
                let mut elements = Vec::with_capacity(if index == 0 { 1 } else { 2 });
                if index > 0 {
                    elements.push(settings_form_separator(cx));
                }
                elements.push(div().w_full().child(child).into_any_element());
                elements
            })),
    )
}

fn settings_form_separator(cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .w_full()
        .h(px(1.0))
        .flex_none()
        .bg(settings_form_divider(cx))
        .into_any_element()
}

fn settings_form_divider(cx: &mut Context<CoduxApp>) -> gpui::Hsla {
    theme::divider_for_surface(cx.theme().background)
}

fn settings_row(
    label: impl Into<String>,
    description: Option<String>,
    control: AnyElement,
) -> impl IntoElement {
    let label = label.into();
    div()
        .min_h(px(58.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(24.0))
        .child(
            div()
                .min_w(px(SETTINGS_ROW_LABEL_MIN_WIDTH))
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(SETTINGS_FORM_TEXT_SIZE)
                        .line_height(SETTINGS_FORM_LINE_HEIGHT)
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(
                    div()
                        .when(description.is_none(), |this| this.hidden())
                        .mt(px(3.0))
                        .max_w(px(420.0))
                        .text_size(SETTINGS_FORM_DESCRIPTION_TEXT_SIZE)
                        .line_height(SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT)
                        .text_color(color(theme::TEXT_DIM))
                        .child(description.unwrap_or_default()),
                ),
        )
        .child(
            div()
                .w(relative(0.3))
                .min_w(px(SETTINGS_ROW_CONTROL_MIN_WIDTH))
                .max_w(relative(0.3))
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_end()
                .child(control),
        )
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
                .text_size(SETTINGS_FORM_TEXT_SIZE)
                .line_height(SETTINGS_FORM_LINE_HEIGHT)
                .text_color(color(theme::TEXT))
                .child(value.into()),
        )
        .into_any_element()
}

fn settings_icon_button_state(
    id: impl Into<SharedString>,
    icon: impl Into<Icon>,
    disabled: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let icon = icon.into();
    Button::new(id.into())
        .compact()
        .ghost()
        .disabled(disabled)
        .text_color(cx.theme().secondary_foreground)
        .bg(cx.theme().transparent)
        .icon(icon.size_3p5().text_color(cx.theme().secondary_foreground))
        .on_click(cx.listener(action))
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
    settings_text_input_sized(id, value, placeholder, masked, false, window, cx, action)
}

fn settings_text_input_sized(
    id: impl Into<SharedString>,
    value: impl Into<String>,
    placeholder: impl Into<String>,
    masked: bool,
    full_width: bool,
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
        .when(full_width, |this| this.w_full().min_w_0())
        .when(!full_width, |this| this.w_full().min_w_0())
        .child(
            Input::new(&state)
                .with_size(gpui_component::Size::Medium)
                .w_full(),
        )
        .into_any_element()
}

fn settings_remote_relay_url_editor(
    value: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    _language: &str,
) -> AnyElement {
    let value = value.to_string();
    let state_key = SharedString::from(format!("settings-remote-relay-url-draft-{value}"));
    let state = window.use_keyed_state(state_key, cx, |window, cx| {
        InputState::new(window, cx).default_value(value.clone())
    });
    cx.subscribe_in(&state, window, |app, _state, event, _window, cx| {
        if matches!(event, InputEvent::Change) {
            app.invalidate_remote_panel(cx);
        }
    })
    .detach();
    let has_changes = state.read(cx).value().as_ref() != value.as_str();
    let input_state = state.clone();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            Input::new(&state)
                .with_size(gpui_component::Size::Medium)
                .w_full(),
        )
        .when(has_changes, |this| {
            this.child(settings_icon_button_state(
                "settings-remote-relay-url-apply",
                HeroIconName::Check,
                false,
                cx,
                move |app, _event, window, cx| {
                    app.set_remote_relay_url(input_state.read(cx).value().to_string(), window, cx)
                },
            ))
        })
        .into_any_element()
}

fn settings_remote_relay_authentication_editor(
    value: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let value = value.to_string();
    let state_key = SharedString::from(format!(
        "settings-remote-relay-authentication-draft-{}",
        value.len()
    ));
    let state = window.use_keyed_state(state_key, cx, |window, cx| {
        InputState::new(window, cx)
            .default_value(value.clone())
            .masked(true)
    });
    cx.subscribe_in(&state, window, |app, _state, event, _window, cx| {
        if matches!(event, InputEvent::Change) {
            app.invalidate_remote_panel(cx);
        }
    })
    .detach();
    let has_changes = state.read(cx).value().as_ref() != value.as_str();
    let input_state = state.clone();
    div()
        .w_full()
        .min_w_0()
        .flex()
        .items_center()
        .gap(px(8.0))
        .child(
            Input::new(&state)
                .with_size(gpui_component::Size::Medium)
                .w_full(),
        )
        .when(has_changes, |this| {
            this.child(settings_icon_button_state(
                "settings-remote-relay-authentication-apply",
                HeroIconName::Check,
                false,
                cx,
                move |app, _event, window, cx| {
                    app.set_remote_relay_authentication(
                        input_state.read(cx).value().to_string(),
                        window,
                        cx,
                    )
                },
            ))
        })
        .into_any_element()
}

fn settings_textarea(
    id: &'static str,
    value: &str,
    rows: usize,
    placeholder: impl Into<String>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let value = value.to_string();
    let placeholder = placeholder.into();
    let state = window.use_keyed_state(
        SharedString::from(format!("settings-textarea-{id}")),
        cx,
        |window, cx| {
            InputState::new(window, cx)
                .multi_line(true)
                .rows(rows)
                .default_value(value.clone())
                .placeholder(placeholder.clone())
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
        .min_w_0()
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
    settings_toggle_state(id, checked, false, cx, action)
}

fn settings_toggle_state(
    id: impl Into<String>,
    checked: bool,
    disabled: bool,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let app_entity = cx.entity();
    Switch::new(SharedString::from(id.into()))
        .checked(checked)
        .disabled(disabled)
        .with_size(gpui_component::Size::Medium)
        .on_click(move |_, window, cx| {
            cx.update_entity(&app_entity, |app, cx| {
                action(app, window, cx);
            });
        })
        .into_any_element()
}

fn settings_select_impl(
    id: impl Into<String>,
    value: &str,
    options: Vec<(String, SharedString)>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    language: &str,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    settings_select_state(id, value, options, false, window, cx, language, action)
}

fn settings_select_state(
    id: impl Into<String>,
    value: &str,
    options: Vec<(String, SharedString)>,
    disabled: bool,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    language: &str,
    action: impl Fn(&mut CoduxApp, String, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    let select_id = format!("settings-select-{}", id.into());
    let options = options
        .into_iter()
        .map(|(value, label)| CoduxSelectOption::new(value, label))
        .collect();
    codux_select(
        select_id.as_str(),
        value,
        options,
        settings_text(&language, "common.choose", "Choose"),
        relative(1.0),
        px(220.0),
        disabled,
        window,
        cx,
        action,
    )
}

/// A friendly device-type label from an OS id (`std::env::consts::OS` or a
/// client-reported platform). Falls back to a generic label when unknown.
fn device_type_label(platform: &str, language: &str) -> String {
    match platform.trim().to_ascii_lowercase().as_str() {
        "macos" | "darwin" | "mac" => "macOS".to_string(),
        "ios" | "ipados" => "iOS".to_string(),
        "android" => settings_text(language, "device.type.android", "Android"),
        "linux" => "Linux".to_string(),
        "windows" => "Windows".to_string(),
        "" => settings_text(language, "device.type.unknown", "Remote device"),
        other => other.to_string(),
    }
}

/// Connection-status tag for a host this desktop controls, from its client link
/// state. Absent (never connected this session) reads as disconnected.
fn host_link_status_tag(
    link: Option<codux_runtime::remote::ControllerLinkState>,
    path: Option<codux_runtime::remote::ControllerLinkPath>,
    language: &str,
) -> AnyElement {
    use codux_runtime::remote::{ControllerLinkPath, ControllerLinkState};
    match link {
        Some(ControllerLinkState::Connected) => {
            let connected = settings_text(language, "remote.status.connected_label", "Connected");
            // Append the route so a LAN/p2p direct link is distinguishable from a
            // relay-routed one (the path arrives a beat after "connected").
            let label = match path {
                Some(ControllerLinkPath::Direct) => format!(
                    "{connected} · {}",
                    settings_text(language, "remote.path.direct_label", "Direct")
                ),
                Some(ControllerLinkPath::Relay) => format!(
                    "{connected} · {}",
                    settings_text(language, "remote.path.relay_label", "Relay")
                ),
                None => connected,
            };
            settings_status_tag(label, theme::GREEN)
        }
        Some(ControllerLinkState::Connecting) => settings_status_tag(
            settings_text(language, "remote.status.connecting_label", "Connecting"),
            theme::ORANGE,
        ),
        _ => settings_status_tag(
            settings_text(language, "remote.status.disconnected_label", "Disconnected"),
            theme::TEXT_DIM,
        ),
    }
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
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .child(value.into())
        .into_any_element()
}

fn settings_checkmark(selected: bool) -> AnyElement {
    div()
        .when(!selected, |this| this.hidden())
        .absolute()
        .top(px(4.0))
        .right(px(4.0))
        .size(px(13.0))
        .rounded_full()
        .bg(color(theme::ACCENT))
        .flex()
        .items_center()
        .justify_center()
        .text_color(color(0xFFFFFF))
        .child(Icon::new(HeroIconName::Check).size_2())
        .into_any_element()
}

fn settings_selectable_tile(
    id: impl Into<String>,
    label: impl Into<String>,
    preview: AnyElement,
    cx: &mut Context<CoduxApp>,
    action: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id.into()))
        .w_full()
        .min_w(px(112.0))
        .flex()
        .flex_col()
        .items_center()
        .gap(px(6.0))
        .cursor_pointer()
        .text_color(color(theme::TEXT))
        .on_click(cx.listener(action))
        .child(preview)
        .child(
            div()
                .w_full()
                .text_align(gpui::TextAlign::Center)
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .truncate()
                .child(label.into()),
        )
        .into_any_element()
}

fn settings_selectable_tile_cell(tile: AnyElement) -> AnyElement {
    div().min_w_0().flex_1().child(tile).into_any_element()
}

fn settings_selectable_tile_rows(
    tiles: Vec<AnyElement>,
    columns: usize,
    gap: Pixels,
) -> AnyElement {
    let columns = columns.max(1);
    let mut rows = Vec::new();
    let mut row = Vec::new();
    for tile in tiles {
        row.push(settings_selectable_tile_cell(tile));
        if row.len() == columns {
            rows.push(row);
            row = Vec::new();
        }
    }
    if !row.is_empty() {
        let filler_count = columns.saturating_sub(row.len());
        row.extend((0..filler_count).map(|_| div().min_w_0().flex_1().into_any_element()));
        rows.push(row);
    }

    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(gap)
        .children(rows.into_iter().map(move |row| {
            div()
                .w_full()
                .flex()
                .gap(gap)
                .children(row)
                .into_any_element()
        }))
        .into_any_element()
}

fn remote_pairing_overlay(
    pairing: Option<RemotePairingInfo>,
    loading: bool,
    error: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let title = settings_text(language, "settings.remote.pairing", "Pairing");
    div()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .p(px(16.0))
        .bg(cx.theme().overlay)
        // Block clicks (e.g. on the confirm button) from passing through to the
        // settings content behind the modal backdrop.
        .occlude()
        .child(
            div()
                .w(px(420.0))
                .max_w(relative(1.0))
                .rounded(px(16.0))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .shadow_lg()
                .p(px(20.0))
                .child(
                    div()
                        .text_size(rems(1.125))
                        .line_height(rems(1.5))
                        .text_color(cx.theme().foreground)
                        .child(title),
                )
                .child(
                    div()
                        .mt(px(24.0))
                        .flex()
                        .flex_col()
                        .items_center()
                        .child(if let Some(pairing) = pairing.as_ref() {
                            remote_pairing_qr(&pairing.qr_payload)
                        } else {
                            remote_pairing_placeholder(cx)
                        })
                        .child(remote_pairing_detail(
                            pairing.as_ref(),
                            loading,
                            error,
                            language,
                            cx,
                        )),
                )
                .child(
                    div()
                        .mt(px(24.0))
                        .flex()
                        .gap(px(8.0))
                        .justify_center()
                        .when_some(
                            pairing.as_ref().map(|pairing| pairing.qr_payload.clone()),
                            |row, payload| {
                                row.child(settings_small_button(
                                    "settings-remote-pairing-copy",
                                    settings_text(language, "remote.copyLink", "Copy link"),
                                    cx,
                                    move |app, _event, _window, cx| {
                                        app.copy_remote_pairing_link(payload.clone(), cx)
                                    },
                                ))
                            },
                        )
                        .child(remote_pairing_cancel_button(pairing, language, cx)),
                ),
        )
        .into_any_element()
}

/// The Devices "+" dropdown, using the shared popup-menu component (auto-anchored
/// to the button): Share this device (advertise via QR/link) or Connect to a
/// device (paste another host's ticket).
fn remote_add_dropdown(language: &str, disabled: bool, cx: &mut Context<CoduxApp>) -> AnyElement {
    let app_entity = cx.entity();
    let share = settings_text(language, "remote.add.share", "Share this device");
    let connect = settings_text(language, "remote.add.connect", "Connect to a device");
    Button::new("settings-remote-add")
        .compact()
        .ghost()
        .disabled(disabled)
        .text_color(cx.theme().secondary_foreground)
        .bg(cx.theme().transparent)
        .icon(
            Icon::new(HeroIconName::Plus)
                .size_3p5()
                .text_color(cx.theme().secondary_foreground),
        )
        .dropdown_menu(move |menu, _window, _cx| {
            let share_entity = app_entity.clone();
            let connect_entity = app_entity.clone();
            menu.item(
                PopupMenuItem::new(share.clone())
                    .icon(HeroIconName::QrCode)
                    .on_click(move |_, window, cx| {
                        cx.update_entity(&share_entity, |app, cx| {
                            app.create_remote_pairing(window, cx)
                        });
                    }),
            )
            .item(
                PopupMenuItem::new(connect.clone())
                    .icon(HeroIconName::Link)
                    .on_click(move |_, _window, cx| {
                        cx.update_entity(&connect_entity, |app, cx| app.open_remote_connect(cx));
                    }),
            )
        })
        .into_any_element()
}

/// "Connect to a device" overlay: paste another host's `codux://pair` ticket to
/// pair this desktop to it (controller direction). Mirrors the project-editor
/// pairing panel but lives in Settings → Remote.
fn remote_connect_overlay(
    ticket: &str,
    name: &str,
    error: Option<&str>,
    busy: bool,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut card = div()
        .w(px(420.0))
        .rounded(px(16.0))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().background)
        .shadow_lg()
        .p(px(20.0))
        .flex()
        .flex_col()
        .gap(px(12.0))
        .child(
            div()
                .text_size(rems(1.125))
                .line_height(rems(1.5))
                .text_color(cx.theme().foreground)
                .child(settings_text(
                    language,
                    "remote.connect.title",
                    "Connect to a device",
                )),
        )
        .child(
            div()
                .text_size(rems(0.8125))
                .text_color(cx.theme().muted_foreground)
                .child(settings_text(
                    language,
                    "remote.connect.hint",
                    "Paste the codux://pair link from the host. The name below is how this desktop will appear on that host.",
                )),
        )
        .child(settings_textarea(
            "settings-remote-connect-ticket",
            ticket,
            3,
            settings_text(
                language,
                "remote.connect.ticket_placeholder",
                "codux://pair?payload=…",
            ),
            window,
            cx,
            |app, value, window, cx| app.set_remote_connect_ticket(value, window, cx),
        ))
        .child(
            div()
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(
                    div()
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(cx.theme().muted_foreground)
                        .child(settings_text(
                            language,
                            "remote.connect.name_label",
                            "This desktop name",
                        )),
                )
                .child(settings_text_input(
                    "settings-remote-connect-name",
                    name,
                    settings_text(
                        language,
                        "remote.connect.name_placeholder",
                        "This desktop name",
                    ),
                    false,
                    window,
                    cx,
                    |app, value, window, cx| app.set_remote_connect_name(value, window, cx),
                )),
        );
    if let Some(error) = error {
        card = card.child(
            div()
                .text_size(rems(0.8125))
                .text_color(cx.theme().danger)
                .child(error.to_string()),
        );
    }
    let card = card.child(
        div()
            .mt(px(4.0))
            .flex()
            .gap(px(8.0))
            .justify_end()
            .child(settings_small_button(
                "settings-remote-connect-cancel",
                settings_text(language, "common.cancel", "Cancel"),
                cx,
                |app, _event, _window, cx| app.close_remote_connect(cx),
            ))
            .child(
                dialog_primary_button(
                    "settings-remote-connect-submit",
                    settings_text(language, "remote.connect.submit", "Connect"),
                    cx,
                    |app, _event, window, cx| app.submit_remote_connect(window, cx),
                )
                .disabled(busy)
                .loading(busy),
            ),
    );

    div()
        .absolute()
        .inset_0()
        .flex()
        .items_center()
        .justify_center()
        .bg(cx.theme().overlay)
        .occlude()
        .child(card)
        .into_any_element()
}

fn remote_pairing_placeholder(cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .size(px(242.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(cx.theme().border)
        .bg(color(0xFFFFFF))
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .size(px(64.0))
                .rounded_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(color(0xF3F4F6))
                .child(
                    Spinner::new()
                        .with_size(px(34.0))
                        .color(color(theme::TEXT_DIM)),
                ),
        )
        .into_any_element()
}

fn remote_pairing_detail(
    pairing: Option<&RemotePairingInfo>,
    loading: bool,
    error: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    if let Some(error) = error.filter(|value| !value.trim().is_empty()) {
        return div()
            .mt(px(18.0))
            .max_w(px(320.0))
            .text_align(gpui::TextAlign::Center)
            .text_size(rems(0.8125))
            .line_height(rems(1.125))
            .text_color(color(theme::RED))
            .child(error.to_string())
            .into_any_element();
    }

    if let Some(pairing) = pairing {
        return div()
            .mt(px(16.0))
            .text_align(gpui::TextAlign::Center)
            .child(
                div()
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .text_color(cx.theme().muted_foreground)
                    .child(settings_text(
                        language,
                        "settings.remote.waiting_scan",
                        "Waiting for mobile scan...",
                    )),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .text_color(cx.theme().muted_foreground)
                    .child(settings_text(
                        language,
                        "settings.remote.scan_code",
                        "Scan code",
                    )),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(rems(1.25))
                    .line_height(rems(1.625))
                    .text_color(cx.theme().foreground)
                    .child(pairing.code.clone()),
            )
            .into_any_element();
    }

    div()
        .h(px(54.0))
        .flex()
        .items_center()
        .justify_center()
        .text_size(rems(0.875))
        .line_height(rems(1.25))
        .text_color(cx.theme().muted_foreground)
        .child(if loading {
            settings_text(
                language,
                "settings.remote.creating_pairing",
                "Creating pairing QR...",
            )
        } else {
            settings_text(
                language,
                "settings.remote.configure_hint",
                "Enable Remote Host before pairing mobile devices.",
            )
        })
        .into_any_element()
}

fn remote_pairing_cancel_button(
    pairing: Option<RemotePairingInfo>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    if let Some(pairing) = pairing {
        let pairing_id = pairing.pairing_id;
        return settings_small_button(
            "settings-remote-pairing-cancel",
            settings_text(language, "common.cancel", "Cancel"),
            cx,
            move |app, _event, window, cx| {
                app.cancel_remote_pairing(pairing_id.clone(), window, cx)
            },
        );
    }

    settings_small_button(
        "settings-remote-pairing-close",
        settings_text(language, "common.cancel", "Cancel"),
        cx,
        |app, _event, _window, cx| app.close_remote_pairing_sheet(cx),
    )
}

fn remote_pending_pairing_overlay(
    pairing: RemotePendingPairing,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let confirm_id = pairing.id.clone();
    let reject_id = pairing.id.clone();
    div()
        .absolute()
        .top(px(0.0))
        .right(px(0.0))
        .bottom(px(0.0))
        .left(px(0.0))
        .flex()
        .items_center()
        .justify_center()
        .p(px(24.0))
        .bg(cx.theme().overlay)
        // Block clicks (e.g. on the confirm button) from passing through to the
        // settings content behind the modal backdrop.
        .occlude()
        .child(
            div()
                .w(px(400.0))
                .max_w(relative(1.0))
                .rounded(px(12.0))
                .border_1()
                .border_color(cx.theme().border)
                .bg(cx.theme().background)
                .shadow_lg()
                .p(px(20.0))
                .flex()
                .flex_col()
                .gap(px(18.0))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(
                            div()
                                .size(px(40.0))
                                .flex_shrink_0()
                                .rounded(px(10.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .bg(cx.theme().primary.opacity(0.14))
                                .child(
                                    Icon::new(HeroIconName::DevicePhoneMobile)
                                        .size_5()
                                        .text_color(cx.theme().primary),
                                ),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(
                                    div()
                                        .text_size(rems(0.9375))
                                        .line_height(rems(1.25))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .text_color(cx.theme().foreground)
                                        .child(settings_text(
                                            language,
                                            "settings.remote.confirm_pairing_title",
                                            "Confirm Device Pairing",
                                        )),
                                )
                                .child(
                                    div()
                                        .text_size(rems(0.75))
                                        .line_height(rems(1.0))
                                        .text_color(cx.theme().muted_foreground)
                                        .child(settings_text(
                                            language,
                                            "settings.remote.confirm_pairing_hint",
                                            "Verify the device and pairing code before confirming.",
                                        )),
                                ),
                        ),
                )
                .child(remote_pending_pairing_details(&pairing, language, cx))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_end()
                        .gap(px(8.0))
                        .child(
                            Button::new("settings-remote-pending-reject")
                                .ghost()
                                .text_color(cx.theme().danger)
                                .on_click(cx.listener(move |app, _event, window, cx| {
                                    app.reject_remote_pairing(reject_id.clone(), window, cx)
                                }))
                                .child(
                                    div()
                                        .text_size(rems(0.8125))
                                        .line_height(rems(1.125))
                                        .child(settings_text(
                                            language,
                                            "settings.remote.reject_pairing",
                                            "Reject",
                                        )),
                                ),
                        )
                        .child(
                            Button::new("settings-remote-pending-confirm")
                                .primary()
                                .text_color(cx.theme().primary_foreground)
                                .on_click(cx.listener(move |app, _event, window, cx| {
                                    app.confirm_remote_pairing(confirm_id.clone(), window, cx)
                                }))
                                .child(
                                    div()
                                        .text_size(rems(0.8125))
                                        .line_height(rems(1.125))
                                        .child(settings_text(
                                            language,
                                            "common.confirm",
                                            "Confirm",
                                        )),
                                ),
                        ),
                ),
        )
        .into_any_element()
}

fn remote_pending_pairing_details(
    pairing: &RemotePendingPairing,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .w_full()
        .rounded(px(10.0))
        .border_1()
        .border_color(cx.theme().border)
        .bg(cx.theme().secondary)
        .child(remote_pending_pairing_row(
            HeroIconName::DevicePhoneMobile,
            settings_text(language, "settings.remote.device", "Device"),
            div()
                .min_w_0()
                .text_size(rems(0.8125))
                .line_height(rems(1.125))
                .text_color(cx.theme().foreground)
                .truncate()
                .child(empty_label(&pairing.device_name))
                .into_any_element(),
            cx,
        ))
        .child(div().h(px(1.0)).w_full().bg(cx.theme().border))
        .child(remote_pending_pairing_row(
            HeroIconName::LockClosed,
            settings_text(language, "settings.remote.code", "Code"),
            div()
                .px(px(10.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .bg(cx.theme().primary.opacity(0.14))
                .text_size(rems(1.0))
                .line_height(rems(1.25))
                .font_weight(gpui::FontWeight::SEMIBOLD)
                .text_color(cx.theme().primary)
                .child(pairing.code.clone())
                .into_any_element(),
            cx,
        ))
        .into_any_element()
}

fn remote_pending_pairing_row(
    icon: HeroIconName,
    label: String,
    value: AnyElement,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(8.0))
        .px(px(12.0))
        .py(px(11.0))
        .child(
            Icon::new(icon)
                .size_4()
                .flex_shrink_0()
                .text_color(cx.theme().muted_foreground),
        )
        .child(
            div()
                .flex_1()
                .min_w_0()
                .text_size(rems(0.8125))
                .line_height(rems(1.125))
                .text_color(cx.theme().muted_foreground)
                .child(label),
        )
        .child(value)
        .into_any_element()
}

fn remote_pairing_qr(payload: &str) -> AnyElement {
    const OUTER_SIZE: f32 = 242.0;
    const QR_SIZE: f32 = 220.0;
    // Pair the trimmed payload with the lowest error-correction level: the QR is
    // shown on a clean screen at close range, so error-correction redundancy buys
    // little and a lower level keeps the version (and module count) down, making
    // the code larger-celled and easier for phones to scan.
    let Ok(code) = QrCode::with_error_correction_level(payload.as_bytes(), EcLevel::L) else {
        return div()
            .size(px(OUTER_SIZE))
            .rounded(px(12.0))
            .bg(color(0xFFFFFF))
            .into_any_element();
    };
    let width = code.width();
    let module_size = QR_SIZE / width as f32;

    div()
        .relative()
        .flex_none()
        .size(px(OUTER_SIZE))
        .rounded(px(12.0))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .bg(color(0xFFFFFF))
        .children(
            code.to_colors()
                .into_iter()
                .enumerate()
                .filter_map(|(index, module)| {
                    if module != QrColor::Dark {
                        return None;
                    }
                    let x = index % width;
                    let y = index / width;
                    Some(
                        div()
                            .absolute()
                            .left(px(11.0 + x as f32 * module_size))
                            .top(px(11.0 + y as f32 * module_size))
                            .size(px(module_size))
                            .bg(color(0x111827))
                            .into_any_element(),
                    )
                }),
        )
        .into_any_element()
}

fn theme_preview_grid(
    title: Option<String>,
    options: Vec<(&'static str, &'static str)>,
    selected: &str,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .when(title.is_some(), |this| {
            this.child(
                div()
                    .px(px(2.0))
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .text_color(color(theme::TEXT_DIM))
                    .child(title.clone().unwrap_or_default()),
            )
        })
        .child(settings_selectable_tile_rows(
            options
                .into_iter()
                .map(|(value, label)| {
                    theme_preview_button(value, label, selected == value, language, cx)
                })
                .collect(),
            5,
            px(10.0),
        ))
        .into_any_element()
}

fn theme_preview_button(
    value: &'static str,
    label: &'static str,
    selected: bool,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let preview = terminal_theme_preview(value);
    let label = if value == "Auto" {
        settings_text(language, "settings.theme.system", label)
    } else {
        label.to_string()
    };
    let tile_id = format!("settings-theme-preview-{value}");
    settings_selectable_tile(
        tile_id,
        label,
        div()
            .relative()
            .w_full()
            .min_w(px(112.0))
            .h(px(50.0))
            .rounded(px(8.0))
            .border_1()
            .border_color(color(if selected {
                theme::ACCENT
            } else {
                theme::BORDER_SOFT
            }))
            .bg(theme::fixed_color(preview.background))
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
                            .bg(theme::fixed_color(preview.muted_foreground)),
                    )
                    .child(
                        div()
                            .h(px(3.0))
                            .w(px(46.0))
                            .rounded(px(1.0))
                            .bg(theme::fixed_color(preview.foreground)),
                    )
                    .child(
                        div()
                            .h(px(8.0))
                            .w(px(58.0))
                            .rounded(px(2.0))
                            .bg(theme::fixed_color(preview.selection)),
                    ),
            )
            .when(selected, |this| this.child(settings_checkmark(true)))
            .into_any_element(),
        cx,
        move |app, _event, window, cx| app.set_theme(value.to_string(), window, cx),
    )
}

fn theme_color_grid(selected: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    settings_selectable_tile_rows(
        theme_color_values()
            .into_iter()
            .map(|item| {
                let selected = selected == item.label;
                let value = item.label;
                settings_selectable_tile(
                    format!("settings-theme-color-{value}"),
                    value,
                    div()
                        .relative()
                        .size(px(28.0))
                        .rounded_full()
                        .border(px(3.0))
                        .border_color(color(if selected {
                            0xFFFFFF
                        } else {
                            theme::BORDER_SOFT
                        }))
                        .bg(color(item.color))
                        .shadow_sm()
                        .into_any_element(),
                    cx,
                    move |app, _event, window, cx| {
                        app.set_theme_color(value.to_string(), window, cx)
                    },
                )
            })
            .collect(),
        4,
        px(8.0),
    )
}

fn app_icon_grid(selected: &str, language: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    settings_selectable_tile_rows(
        icon_style_values()
            .into_iter()
            .map(|item| {
                let selected = selected == item.value;
                let value = item.value;
                let label = settings_text(language, item.label_key, item.fallback);
                settings_selectable_tile(
                    format!("settings-app-icon-{value}"),
                    label,
                    app_icon_preview(item.value, selected),
                    cx,
                    move |app, _event, window, cx| {
                        app.set_icon_style(value.to_string(), window, cx)
                    },
                )
            })
            .collect(),
        4,
        px(14.0),
    )
}

fn app_icon_preview(style: &'static str, selected: bool) -> AnyElement {
    let path = app_icon_asset_path(style);
    div()
        .relative()
        .size(px(52.0))
        .flex()
        .items_center()
        .justify_center()
        .child(img(path).size(px(48.0)).object_fit(ObjectFit::Contain))
        .child(
            div()
                .absolute()
                .left(px(2.0))
                .top(px(2.0))
                .size(px(48.0))
                .rounded(px(12.0))
                .border_2()
                .border_color(
                    color(if selected { 0xFFFFFF } else { 0x000000 }).opacity(if selected {
                        1.0
                    } else {
                        0.0
                    }),
                ),
        )
        .into_any_element()
}

fn settings_general_pane(
    settings: &SettingsSummary,
    pending_restart_language: Option<&str>,
    terminal_font_families: &[String],
    update: &UpdateSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    let visible_language = pending_restart_language.unwrap_or(language);
    settings_form(vec![
        settings_card(
            None,
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.language", "Language"),
                    Some(settings_text(
                        language,
                        "settings.language.restart_message",
                        "Restart Codux to apply the selected language.",
                    )),
                    settings_select_impl(
                        "settings-language",
                        visible_language,
                        language_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_language(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.dock_badge", "Dock Badge"),
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
                    settings_text(language, "settings.sleep_prevention", "Prevent System Sleep"),
                    Some(settings_text(
                        language,
                        "settings.sleep_prevention.help",
                        "Allows the display to turn off, but prevents this device from idle sleeping while enabled.",
                    )),
                    settings_select_impl(
                        "settings-sleep-mode",
                        &settings.sleep_mode,
                        sleep_mode_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_sleep_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.file_open_default", "File Open Default"),
                    Some(settings_text(
                        language,
                        "settings.file_open_default.help",
                        "Used when opening files outside the Files view.",
                    )),
                    settings_select_impl(
                        "settings-file-open-default",
                        &settings.file_open_default,
                        file_open_default_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_file_open_default(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            None,
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.git_auto_refresh", "Git Auto Refresh"),
                    None,
                    settings_select_impl(
                        "settings-git-refresh",
                        &settings.git_refresh,
                        git_refresh_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_git_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.ai_auto_refresh", "AI Auto Refresh"),
                    None,
                    settings_select_impl(
                        "settings-ai-refresh",
                        &settings.ai_refresh,
                        ai_refresh_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_ai_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.ai_background_refresh",
                        "AI Background Refresh",
                    ),
                    None,
                    settings_select_impl(
                        "settings-ai-background-refresh",
                        &settings.ai_background_refresh,
                        ai_background_refresh_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_ai_background_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.ai_statistics_mode", "AI Statistics Mode"),
                    None,
                    settings_select_impl(
                        "settings-statistics-mode",
                        &settings.statistics_mode,
                        statistics_mode_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_statistics_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.terminal_text", "Terminal Text")),
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.terminal_font_family", "Terminal Font"),
                    Some(settings_text(
                        language,
                        "settings.terminal_font_family.help",
                        "Only monospaced fonts are shown to keep terminal layout accurate.",
                    )),
                    settings_select_impl(
                        "settings-terminal-font-family",
                        &settings.terminal_font_family,
                        terminal_font_family_options(
                            language,
                            &settings.terminal_font_family,
                            terminal_font_families,
                        ),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_terminal_font_family(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.terminal_font_size", "Terminal Font Size"),
                    None,
                    settings_select_impl(
                        "settings-terminal-font-size",
                        &settings.terminal_font_size,
                        terminal_font_size_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_terminal_font_size(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.terminal_padding", "Terminal Padding"),
                    None,
                    settings_select_impl(
                        "settings-terminal-padding",
                        &settings.terminal_padding,
                        terminal_padding_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_terminal_padding(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.terminal_scrollback", "Terminal Scrollback"),
                    Some(settings_text(
                        language,
                        "settings.terminal_scrollback.help",
                        "Limit terminal scrollback and restored output to reduce long-session memory usage.",
                    )),
                    settings_select_impl(
                        "settings-terminal-scrollback",
                        &settings.terminal_scrollback_lines,
                        terminal_scrollback_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_terminal_scrollback_lines(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.update.section", "Updates")),
            Some(settings_text(
                language,
                "settings.update.description",
                "Updates are checked from the selected GitHub Release channel.",
            )),
            vec![
                settings_row(
                    settings_text(language, "settings.update.enabled", "Enable Update Checks"),
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
                    settings_text(language, "settings.update.channel", "Update Channel"),
                    None,
                    settings_select_impl(
                        "settings-update-channel",
                        &settings.update_channel,
                        update_channel_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_update_channel(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.update.status", "Update Status"),
                    Some(update_status_text(update, language)),
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(settings_small_button_state(
                            "settings-check-update",
                            settings_text(language, "about.updates", "Check for Updates"),
                            false,
                            !settings.update_enabled,
                            cx,
                            |app, _event, window, cx| app.open_update_dialog_window(window, cx),
                        ))
                        .into_any_element(),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
    ])
    .into_any_element()
}

fn update_status_text(update: &UpdateSummary, language: &str) -> String {
    if let Some(error) = &update.error {
        return format!(
            "{}: {error}",
            settings_text(
                language,
                "settings.update.status.error",
                "Update check failed"
            )
        );
    }
    if let Some(version) = &update.latest_version {
        if !update.available {
            return settings_text(
                language,
                "settings.update.status.latest_format",
                "Current version %@ is up to date.",
            )
            .replace("%@", env!("CARGO_PKG_VERSION"));
        }
        let notes = update.notes_preview.trim();
        let available = settings_text(
            language,
            "settings.update.status.available_format",
            "Version %@ is available. Current version: %@.",
        )
        .replacen("%@", version, 1)
        .replacen("%@", env!("CARGO_PKG_VERSION"), 1);
        if notes.is_empty() {
            return available;
        }
        return format!("{available} · {notes}");
    }
    if update.enabled {
        String::new()
    } else {
        settings_text(
            language,
            "settings.update.status.disabled",
            "Update checks are turned off.",
        )
    }
}

fn settings_appearance_pane(
    settings: &SettingsSummary,
    vibrancy_slider: Option<gpui::Entity<gpui_component::slider::SliderState>>,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    let mut cards = vec![
        settings_card(
            Some(settings_text(language, "settings.terminal_theme", "Terminal Theme")),
            Some(settings_text(
                language,
                "settings.terminal_theme.help",
                "Applies to the app surface and all terminals.",
            )),
            vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(theme_preview_grid(
                        None,
                        system_theme_options(),
                        &settings.theme,
                        language,
                        cx,
                    ))
                    .child(theme_preview_grid(
                        Some(settings_text(language, "settings.theme.group.dark", "Dark")),
                        dark_theme_options(),
                        &settings.theme,
                        language,
                        cx,
                    ))
                    .child(theme_preview_grid(
                        Some(settings_text(language, "settings.theme.group.light", "Light")),
                        light_theme_options(),
                        &settings.theme,
                        language,
                        cx,
                    ))
                    .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.theme_color", "Theme Color")),
            Some(settings_text(
                language,
                "settings.theme_color.help",
                "Used for buttons, selected states, tabs, focus rings, links, and other highlights.",
            )),
            vec![theme_color_grid(&settings.theme_color, cx)],
            cx,)
        .into_any_element(),
    ];

    if cfg!(target_os = "macos") {
        cards.push(
            settings_card(
                Some(settings_text(language, "settings.app_icon", "App Icon")),
                Some(settings_text(
                    language,
                    "settings.app_icon.restart_message",
                    "Icon changes fully apply after restart.",
                )),
                vec![app_icon_grid(&settings.icon_style, language, cx)],
                cx,
            )
            .into_any_element(),
        );
    }

    // App Style sits at the top of the Appearance pane.
    cards.insert(0, appearance_style_card(vibrancy_slider, language, cx));

    settings_form(cards).into_any_element()
}

fn appearance_style_card(
    vibrancy_slider: Option<gpui::Entity<gpui_component::slider::SliderState>>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut children = Vec::new();
    if let Some(state) = vibrancy_slider {
        children.push(
            appearance_slider_row(
                settings_text(language, "settings.window_style.ui_opacity", "Opacity"),
                settings_text(
                    language,
                    "settings.window_style.ui_help",
                    "Frosted-glass opacity for the sidebar, headers, panels and cards. The terminal stays a bit more opaque. Drag to 100% for solid.",
                ),
                state,
                cx,
            )
            .into_any_element(),
        );
    }

    settings_card(
        Some(settings_text(
            language,
            "settings.window_style.title",
            "App Style",
        )),
        None,
        children,
        cx,
    )
    .into_any_element()
}

/// A settings row whose right-hand control is an opacity slider with a
/// percentage readout. The control slot mirrors `settings_row` exactly
/// (`relative(0.3)` width, `justify_end`) so it lines up flush-right with the
/// other settings controls.
fn appearance_slider_row(
    label: String,
    help: String,
    state: gpui::Entity<gpui_component::slider::SliderState>,
    cx: &mut Context<CoduxApp>,
) -> impl IntoElement {
    let percent = (state.read(cx).value().start() * 100.0).round() as i64;
    div()
        .w_full()
        .min_h(px(58.0))
        .py(px(10.0))
        .flex()
        .items_center()
        .gap(px(24.0))
        .child(
            div()
                .min_w(px(160.0))
                .max_w(px(420.0))
                .flex()
                .flex_col()
                .child(
                    div()
                        .text_size(SETTINGS_FORM_TEXT_SIZE)
                        .line_height(SETTINGS_FORM_LINE_HEIGHT)
                        .text_color(color(theme::TEXT))
                        .child(label),
                )
                .child(
                    div()
                        .mt(px(3.0))
                        .max_w(px(420.0))
                        .text_size(SETTINGS_FORM_DESCRIPTION_TEXT_SIZE)
                        .line_height(SETTINGS_FORM_DESCRIPTION_LINE_HEIGHT)
                        .text_color(color(theme::TEXT_DIM))
                        .child(help),
                ),
        )
        .child(
            div()
                .min_w(px(220.0))
                .flex_1()
                .flex()
                .items_center()
                .gap(px(10.0))
                // The slider fills the slot (grow + allow it to size below its
                // content min via min_w_0); the percent readout trails it. No
                // `justify_end` here — unlike the small fixed controls in
                // `settings_row`, a growing slider defines the row width itself,
                // and justify_end would fight that growth and leave a gap.
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .child(gpui_component::slider::Slider::new(&state)),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .min_w(px(34.0))
                        .text_align(gpui::TextAlign::Right)
                        .text_size(rems(0.8125))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(format!("{percent}%")),
                ),
        )
}

fn settings_pet_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    let speech_disabled = settings.pet_speech_mode == "off";
    let pet_desktop_disabled = !settings.pet_enabled;
    let pet_speech_llm_provider_disabled = speech_disabled || !settings.pet_speech_llm_enabled;
    settings_form(vec![
        settings_card(
            Some(settings_text(language, "settings.pet.section.general", "General")),
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.pet.enabled", "Enable Pet"),
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
                    settings_text(language, "settings.pet.desktop_widget", "Desktop Pet"),
                    None,
                    settings_toggle_state(
                        "settings-pet-desktop",
                        settings.pet_desktop_widget,
                        pet_desktop_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_desktop_widget(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.static_mode", "Static Pet Sprite"),
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
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.pet.speech.section", "Pet Speech")),
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.pet.speech.mode", "Mode"),
                    None,
                    settings_select_impl(
                        "settings-pet-speech-mode",
                        &settings.pet_speech_mode,
                        pet_speech_mode_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_pet_speech_mode(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.speech.frequency", "Frequency"),
                    Some(settings_text(
                        language,
                        "settings.pet.speech.frequency_help",
                        "Frequency is estimated per hour, not a daily cap. The shortest global cooldown is 30 seconds.",
                    )),
                    settings_select_state(
                        "settings-pet-speech-frequency",
                        &settings.pet_speech_frequency,
                        pet_speech_frequency_options(language),
                        speech_disabled,
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_pet_speech_frequency(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.quiet_during_work",
                        "Speak Less During Work",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-speech-work",
                        settings.pet_speech_quiet_during_work,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_quiet_during_work(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.louder_at_night",
                        "Speak More at Night",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-speech-night",
                        settings.pet_speech_louder_at_night,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_louder_at_night(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.mute_on_fullscreen",
                        "Mute in Full Screen",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-speech-fullscreen",
                        settings.pet_speech_mute_on_fullscreen,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_mute_on_fullscreen(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.speech.quiet_hours",
                        "Quiet Hours 22:00-08:00",
                    ),
                    None,
                    settings_toggle_state(
                        "settings-pet-quiet-hours",
                        settings.pet_speech_quiet_hours_enabled,
                        speech_disabled,
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
                    .child(settings_small_button_state(
                        "settings-pet-mute-30",
                        settings_text(
                            language,
                            "settings.pet.speech.mute_30_minutes",
                            "Mute for 30 Minutes",
                        ),
                        false,
                        speech_disabled,
                        cx,
                        |app, _event, _window, cx| app.set_pet_speech_temporary_mute(true, cx),
                    ))
                    .child(settings_small_button_state(
                        "settings-pet-unmute",
                        settings_text(
                            language,
                            "settings.pet.speech.unmute",
                            "Clear Temporary Mute",
                        ),
                        false,
                        speech_disabled || !settings.pet_speech_temporary_muted,
                        cx,
                        |app, _event, _window, cx| app.set_pet_speech_temporary_mute(false, cx),
                    ))
                    .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(language, "settings.pet.llm.section", "Pet LLM")),
            Some(settings_text(
                language,
                "settings.pet.llm.help",
                "Only rhythm and milestone messages use LLM refinement. Templates are used on failure.",
            )),
            vec![
                settings_row(
                    settings_text(language, "settings.pet.llm.enabled", "Enable LLM Refinement"),
                    None,
                    settings_toggle_state(
                        "settings-pet-llm",
                        settings.pet_speech_llm_enabled,
                        speech_disabled,
                        cx,
                        |app, window, cx| app.toggle_pet_speech_llm_enabled(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.pet.llm.channel", "LLM Provider"),
                    None,
                    settings_select_state(
                        "pet-speech-provider",
                        &settings.pet_speech_provider_id,
                        ai_provider_options(settings, "petSpeech", language),
                        pet_speech_llm_provider_disabled,
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_pet_speech_provider(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
        settings_card(
            Some(settings_text(
                language,
                "settings.pet.section.reminders",
                "Reminders",
            )),
            None,
            vec![
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.hydration",
                        "Hydration Reminder",
                    ),
                    None,
                    settings_toggle(
                        "settings-pet-reminders",
                        settings.pet_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_reminders(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.hydration_interval",
                        "Hydration Interval",
                    ),
                    None,
                    settings_select_state(
                        "settings-pet-hydration-reminder-interval",
                        &settings.pet_hydration_reminder_minutes,
                        pet_reminder_interval_options(language),
                        !settings.pet_reminders,
                        window,
                        cx,
                        language,
                        |app, value, window, cx| {
                            app.set_pet_hydration_reminder_minutes(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.sedentary",
                        "Sedentary Reminder",
                    ),
                    None,
                    settings_toggle(
                        "settings-pet-sedentary-reminders",
                        settings.pet_sedentary_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_sedentary_reminders(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.sedentary_interval",
                        "Sedentary Interval",
                    ),
                    None,
                    settings_select_state(
                        "settings-pet-sedentary-reminder-interval",
                        &settings.pet_sedentary_reminder_minutes,
                        pet_reminder_interval_options(language),
                        !settings.pet_sedentary_reminders,
                        window,
                        cx,
                        language,
                        |app, value, window, cx| {
                            app.set_pet_sedentary_reminder_minutes(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.late_night",
                        "Late-Night Reminder",
                    ),
                    None,
                    settings_toggle(
                        "settings-pet-late-night-reminders",
                        settings.pet_late_night_reminders,
                        cx,
                        |app, window, cx| app.toggle_pet_late_night_reminders(window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.pet.reminder.late_night_interval",
                        "Late-Night Interval",
                    ),
                    None,
                    settings_select_state(
                        "settings-pet-late-night-reminder-interval",
                        &settings.pet_late_night_reminder_minutes,
                        pet_reminder_interval_options(language),
                        !settings.pet_late_night_reminders,
                        window,
                        cx,
                        language,
                        |app, value, window, cx| {
                            app.set_pet_late_night_reminder_minutes(value, window, cx)
                        },
                    ),
                )
                .into_any_element(),
            ],
            cx,)
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_ai_pane(
    settings: &SettingsSummary,
    permissions: &ToolPermissionsSummary,
    selected_provider_id: Option<&str>,
    testing_provider_id: Option<&str>,
    test_result: Option<&AIProviderTestResult>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    let provider_rows = if settings.ai_providers.is_empty() {
        vec![
            div()
                .py(px(12.0))
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_DIM))
                .child(settings_text(
                    language,
                    "settings.ai.provider.empty",
                    "No API providers yet.",
                ))
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
                    test_result,
                    language,
                    window,
                    cx,
                )
                .into_any_element()
            })
            .collect::<Vec<_>>()
    };
    let mut runtime_tool_rows = Vec::new();
    runtime_tool_rows.extend(vec![
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "Codex"),
            "codex",
            "codexModel",
            &permissions.codex,
            &permissions.codex_model,
            "gpt-5.5",
            true,
            true,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "Claude Code"),
            "claudeCode",
            "claudeCodeModel",
            &permissions.claude_code,
            &permissions.claude_code_model,
            "claude-sonnet-4.5",
            true,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "Agy"),
            "agy",
            "agyModel",
            &permissions.agy,
            &permissions.agy_model,
            "gemini-2.5-pro",
            true,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "OpenCode"),
            "opencode",
            "opencodeModel",
            &permissions.opencode,
            &permissions.opencode_model,
            "gpt-5.5",
            true,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "Kiro"),
            "kiro",
            "kiroModel",
            &permissions.kiro,
            &permissions.kiro_model,
            "auto",
            false,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "CodeWhale"),
            "codewhale",
            "codewhaleModel",
            &permissions.codewhale,
            &permissions.codewhale_model,
            "deepseek-chat",
            true,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "Kimi Code"),
            "kimi",
            "kimiModel",
            &permissions.kimi,
            &permissions.kimi_model,
            "kimi-k2",
            false,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
        settings_runtime_tool_block(
            settings_text(
                language,
                "settings.ai.tool.configuration_format",
                "%@ Configuration",
            )
            .replace("%@", "MiMo-Code"),
            "mimo",
            "mimoModel",
            &permissions.mimo,
            &permissions.mimo_model,
            "kimi-k2",
            true,
            false,
            &permissions.codex_effort,
            language,
            window,
            cx,
        ),
    ]);

    settings_form(vec![
        settings_card(
            Some(settings_text(
                language,
                "settings.ai.section.runtime_tools",
                "Runtime Tools",
            )),
            Some(settings_text(
                language,
                "settings.tools.hint",
                "These defaults are written to the runtime wrapper permission file.",
            )),
            runtime_tool_rows,
            cx,
        )
        .into_any_element(),
        settings_card(
            Some(settings_text(
                language,
                "settings.ai.global_prompt",
                "Global Prompt",
            )),
            Some(settings_text(
                language,
                "settings.ai.global_prompt_help",
                "Injected when supported tools start and merged with memory context.",
            )),
            vec![settings_textarea(
                "ai-global-prompt",
                &settings.ai_global_prompt,
                4,
                settings_text(
                    language,
                    "settings.ai.global_prompt",
                    "Global prompt for supported tools",
                ),
                window,
                cx,
                |app, value, window, cx| app.set_ai_global_prompt(value, window, cx),
            )],
            cx,
        )
        .into_any_element(),
        settings_card_with_actions(
            Some(settings_text(
                language,
                "settings.ai.section.providers",
                "AI Providers",
            )),
            None,
            Some(settings_icon_button_state(
                "settings-add-ai-provider",
                Icon::new(HeroIconName::Key),
                false,
                cx,
                |app, _event, window, cx| app.add_ai_provider(window, cx),
            )),
            vec![
                div()
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .children(provider_rows)
                    .into_any_element(),
            ],
            cx,
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_git_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    settings_form(vec![
        settings_card(
            Some(settings_text(
                language,
                "settings.ai.git_commit_message",
                "Git Commit Message",
            )),
            None,
            vec![
                settings_row(
                    settings_text(
                        language,
                        "settings.ai.git_commit_message_provider",
                        "AI Provider",
                    ),
                    None,
                    settings_select_impl(
                        "settings-git-provider-auto",
                        &settings.git_commit_provider_id,
                        git_provider_options(settings, language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_git_commit_provider(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.ai.git_commit_message_tone", "Tone"),
                    None,
                    settings_select_impl(
                        "settings-git-tone",
                        &settings.git_commit_tone,
                        git_tone_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_git_commit_tone(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(language, "settings.language", "Language"),
                    None,
                    settings_select_impl(
                        "settings-git-language",
                        &settings.git_commit_language,
                        git_language_options(language),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_git_commit_language(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.ai.git_commit_message_style_rules",
                        "Style Rules",
                    ),
                    Some(settings_text(
                        language,
                        "settings.ai.git_commit_message_style_rules_placeholder",
                        "Example: use Conventional Commits, keep subject under 72 characters.",
                    )),
                    settings_textarea(
                        "git-style-rules",
                        &settings.git_commit_style_rules,
                        3,
                        settings_text(
                            language,
                            "settings.ai.git_commit_message_style_rules_placeholder",
                            "Example: use Conventional Commits, keep subject under 72 characters.",
                        ),
                        window,
                        cx,
                        |app, value, window, cx| app.set_git_commit_style_rules(value, window, cx),
                    ),
                )
                .into_any_element(),
                settings_row(
                    settings_text(
                        language,
                        "settings.terminal_paste_images_as_paths",
                        "Paste Images as Paths",
                    ),
                    Some(settings_text(
                        language,
                        "settings.terminal_paste_images_as_paths.help",
                        "When pasting an image into a terminal, save it to a temporary file and paste the local path instead of image data.",
                    )),
                    settings_toggle(
                        "settings-terminal-paste-images-as-paths",
                        settings.terminal_paste_images_as_paths,
                        cx,
                        |app, window, cx| app.toggle_terminal_paste_images_as_paths(window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,
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
    let language = settings.language.as_str();
    let mut cards = vec![
        settings_card(
            Some(settings_text(
                language,
                "settings.ai.section.memory",
                "Memory",
            )),
            None,
            vec![
                settings_row(
                    settings_text(language, "settings.ai.memory.enabled", "Enable Memory"),
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
            cx,
        )
        .into_any_element(),
    ];

    if settings.memory_enabled {
        cards.push(
            settings_card(
                Some(settings_text(
                    language,
                    "settings.ai.memory.automatic_injection",
                    "Automatic Injection",
                )),
                None,
                vec![
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.automatic_injection",
                            "Automatic Injection",
                        ),
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
                        settings_text(
                            language,
                            "settings.ai.memory.automatic_extraction",
                            "Automatic Extraction",
                        ),
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
                        settings_text(
                            language,
                            "settings.ai.memory.extraction_interval",
                            "Extraction Interval",
                        ),
                        None,
                        settings_select_impl(
                            "settings-memory-extraction-interval",
                            &settings.memory_extraction_idle_delay_seconds,
                            memory_extraction_interval_options(),
                            window,
                            cx,
                            language,
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
                        settings_text(
                            language,
                            "settings.ai.memory.heuristic_gate",
                            "Skip Low-Signal Sessions",
                        ),
                        Some(settings_text(
                            language,
                            "settings.ai.memory.heuristic_gate.help",
                            "Avoid LLM extraction for transcripts that do not contain durable memory signals.",
                        )),
                        settings_toggle(
                            "settings-memory-heuristic-gate",
                            settings.memory_extraction_heuristic_gate_enabled,
                            cx,
                            |app, window, cx| {
                                let next =
                                    !app.state.settings.memory_extraction_heuristic_gate_enabled;
                                app.set_ai_memory_bool(
                                    "extractionHeuristicGateEnabled",
                                    next,
                                    window,
                                    cx,
                                )
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.growth_threshold",
                            "Minimum Transcript Growth",
                        ),
                        Some(settings_text(
                            language,
                            "settings.ai.memory.growth_threshold.help",
                            "Skip repeat extraction until a session grows by at least this many lines.",
                        )),
                        settings_text_input(
                            "settings-memory-growth-threshold",
                            &settings.memory_extraction_growth_threshold_lines,
                            "8",
                            false,
                            window,
                            cx,
                            |app, value, window, cx| {
                                app.set_ai_memory_number(
                                    "extractionGrowthThresholdLines",
                                    value,
                                    window,
                                    cx,
                                )
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.max_index_sessions",
                            "Maximum Recent Sessions",
                        ),
                        None,
                        settings_select_impl(
                            "settings-memory-max-index",
                            &settings.memory_max_index_sessions,
                            memory_max_index_options(language),
                            window,
                            cx,
                            language,
                            |app, value, window, cx| {
                                app.set_ai_memory_number("maxIndexSessions", value, window, cx)
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.recall_fts",
                            "Use FTS Recall",
                        ),
                        Some(settings_text(
                            language,
                            "settings.ai.memory.recall_fts.help",
                            "Use local SQLite BM25 search to recall older relevant memories.",
                        )),
                        settings_toggle(
                            "settings-memory-recall-fts",
                            settings.memory_recall_use_fts,
                            cx,
                            |app, window, cx| {
                                let next = !app.state.settings.memory_recall_use_fts;
                                app.set_ai_memory_bool("recallUseFts", next, window, cx)
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.privacy_scrub",
                            "Redact Secrets Before Saving",
                        ),
                        Some(settings_text(
                            language,
                            "settings.ai.memory.privacy_scrub.help",
                            "Redact common API keys, tokens and private keys before memory is stored.",
                        )),
                        settings_toggle(
                            "settings-memory-privacy-scrub",
                            settings.memory_privacy_scrub_enabled,
                            cx,
                            |app, window, cx| {
                                let next = !app.state.settings.memory_privacy_scrub_enabled;
                                app.set_ai_memory_bool("privacyScrubEnabled", next, window, cx)
                            },
                        ),
                    )
                    .into_any_element(),
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.cross_project_user",
                            "Cross-Project User Memory",
                        ),
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
                cx,
            )
            .into_any_element(),
        );
        cards.push(
            settings_card(
                Some(settings_text(
                    language,
                    "settings.ai.memory.default_extraction_provider",
                    "Default Extraction Provider",
                )),
                None,
                vec![
                    settings_row(
                        settings_text(
                            language,
                            "settings.ai.memory.default_extraction_provider",
                            "Default Extraction Provider",
                        ),
                        None,
                        settings_select_impl(
                            "settings-memory-provider",
                            &settings.memory_default_extractor_provider_id,
                            ai_provider_options(settings, "memory", language),
                            window,
                            cx,
                            language,
                            |app, value, window, cx| app.set_ai_memory_provider(value, window, cx),
                        ),
                    )
                    .into_any_element(),
                ],
                cx,
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
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    settings_form(
        notifications
            .channels
            .iter()
            .cloned()
            .map(|channel| {
                settings_notification_card(channel, testing_channel_id, language, window, cx)
                    .into_any_element()
            })
            .collect::<Vec<_>>(),
    )
    .into_any_element()
}

fn settings_remote_pane(
    settings: &SettingsSummary,
    remote: &RemoteSummary,
    saved_hosts: &[codux_runtime::remote::SavedRemoteHost],
    link_states: &std::collections::HashMap<String, codux_runtime::remote::ControllerLinkState>,
    link_paths: &std::collections::HashMap<String, codux_runtime::remote::ControllerLinkPath>,
    _selected_device_id: Option<&str>,
    language: &str,
    remote_reconnecting: bool,
    remote_pairing_creating: bool,
    _window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut device_rows: Vec<AnyElement> = remote
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
                .min_h(px(64.0))
                .py(px(12.0))
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
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                // Devices in this segment paired INTO this
                                // machine, so the peer is a controller.
                                .child(settings_status_tag(
                                    settings_text(
                                        language,
                                        "settings.remote.role.controller",
                                        "Controller",
                                    ),
                                    theme::ACCENT,
                                ))
                                .child(
                                    div()
                                        .min_w(px(64.0))
                                        .flex_1()
                                        .text_size(rems(0.9375))
                                        .line_height(rems(1.25))
                                        .text_color(color(theme::TEXT))
                                        .truncate()
                                        .child(empty_label(&device.name)),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(3.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(device_type_label(&device.platform, language)),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(if device.online.unwrap_or(false) {
                            settings_status_tag(
                                settings_text(
                                    language,
                                    "remote.status.connected_label",
                                    "Connected",
                                ),
                                theme::GREEN,
                            )
                        } else {
                            settings_status_tag(
                                settings_text(
                                    language,
                                    "remote.status.disconnected_label",
                                    "Disconnected",
                                ),
                                theme::TEXT_DIM,
                            )
                        })
                        .child(settings_icon_button_state(
                            SharedString::from(format!("settings-remote-remove-{}", device.id)),
                            HeroIconName::Trash,
                            false,
                            cx,
                            move |app, _event, window, cx| {
                                app.select_remote_device(remove_id.clone(), window, cx);
                                app.revoke_selected_remote_device(window, cx);
                            },
                        )),
                )
                .into_any_element()
        })
        .collect::<Vec<_>>();

    // Connected hosts (the desktops / headless agents this Mac pairs to as a
    // controller) share the same list, tagged "Host", with a Forget action.
    for host in saved_hosts {
        let device_id = host.device_id.clone();
        let name = if host.host_name.trim().is_empty() {
            host.host_id.clone()
        } else {
            host.host_name.clone()
        };
        device_rows.push(
            div()
                .id(SharedString::from(format!(
                    "settings-remote-host-{}",
                    host.device_id
                )))
                .min_h(px(64.0))
                .py(px(12.0))
                .flex()
                .items_center()
                .justify_between()
                .gap(px(18.0))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .min_w_0()
                                .flex()
                                .items_center()
                                .gap(px(6.0))
                                // This machine paired INTO these as a controller,
                                // so the peer is a host.
                                .child(settings_status_tag(
                                    settings_text(language, "settings.remote.role.host", "Host"),
                                    theme::TEXT_DIM,
                                ))
                                .child(
                                    div()
                                        .min_w(px(64.0))
                                        .flex_1()
                                        .text_size(rems(0.9375))
                                        .line_height(rems(1.25))
                                        .text_color(color(theme::TEXT))
                                        .truncate()
                                        .child(empty_label(&name)),
                                ),
                        )
                        .child(
                            div()
                                .mt(px(3.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(device_type_label(&host.platform, language)),
                        ),
                )
                .child(
                    div()
                        .flex_shrink_0()
                        .flex()
                        .items_center()
                        .gap(px(12.0))
                        .child(host_link_status_tag(
                            link_states.get(host.device_id.as_str()).copied(),
                            link_paths.get(host.device_id.as_str()).copied(),
                            language,
                        ))
                        .child(settings_icon_button_state(
                            SharedString::from(format!(
                                "settings-remote-forget-{}",
                                host.device_id
                            )),
                            HeroIconName::Trash,
                            false,
                            cx,
                            move |app, _event, _window, cx| {
                                app.forget_remote_host_device(device_id.clone(), cx)
                            },
                        )),
                )
                .into_any_element(),
        );
    }

    if device_rows.is_empty() {
        device_rows.push(
            div()
                .py(px(12.0))
                .text_size(rems(0.875))
                .line_height(rems(1.125))
                .text_color(color(theme::TEXT_DIM))
                .child(if remote.enabled {
                    settings_text(language, "settings.remote.no_devices", "No paired devices.")
                } else {
                    settings_text(
                        language,
                        "remote.devices.empty_hint",
                        "Pair a phone to control terminals on the go.",
                    )
                })
                .into_any_element(),
        );
    }

    div()
        .relative()
        .size_full()
        .child(settings_form(vec![
            settings_card(
                Some(settings_text(
                    language,
                    "settings.remote.server",
                    "Remote Host",
                )),
                None,
                vec![
                    settings_row(
                        settings_text(language, "settings.remote.enabled", "Enable Remote Host"),
                        None,
                        settings_toggle(
                            "settings-remote-enabled",
                            remote.enabled,
                            cx,
                            |app, window, cx| app.toggle_remote_host(window, cx),
                        ),
                    )
                    .into_any_element(),
                    {
                        // The custom URL/auth fields render as sub-content of the
                        // relay row (one card slot), and only when "custom" — so
                        // there's no empty slot drawing a stray separator.
                        let custom = (settings.remote_relay_preset == "custom").then(|| {
                            settings_remote_relay_custom_fields(settings, _window, cx, language)
                        });
                        let relay_row = settings_row(
                            settings_text(language, "settings.remote.relay_mode", "Relay Network"),
                            Some(settings_text(
                                language,
                                "settings.remote.relay_mode.help",
                                "Changing the relay requires pairing again.",
                            )),
                            settings_select_impl(
                                "settings-remote-relay-preset",
                                settings.remote_relay_preset.as_str(),
                                remote_relay_preset_options(language),
                                _window,
                                cx,
                                language,
                                |app, value, window, cx| {
                                    app.set_remote_relay_preset(value, window, cx)
                                },
                            ),
                        );
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(12.0))
                            .child(relay_row)
                            .children(custom)
                            .into_any_element()
                    },
                    div()
                        .py(px(10.0))
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(
                            div()
                                .size(px(8.0))
                                .rounded_full()
                                .bg(color(remote_status_color(remote))),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .flex_1()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .truncate()
                                .child(remote_status_label(remote, language)),
                        )
                        .child(settings_small_button_state(
                            "settings-remote-reconnect",
                            settings_text(language, "settings.remote.reconnect", "Reconnect"),
                            remote_reconnecting,
                            !remote.enabled,
                            cx,
                            |app, _event, window, cx| app.reconnect_remote(window, cx),
                        ))
                        .into_any_element(),
                ],
                cx,
            )
            .into_any_element(),
            remote_mobile_download_banner(language, cx),
            settings_card_with_actions(
                Some(settings_text(
                    language,
                    "settings.remote.devices",
                    "Devices",
                )),
                None,
                Some(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.0))
                        .child(div().child(remote_add_dropdown(
                            language,
                            remote_pairing_creating || !remote.enabled,
                            cx,
                        )))
                        .child(settings_icon_button_state(
                            "settings-remote-refresh",
                            HeroIconName::ArrowPath,
                            !remote.enabled,
                            cx,
                            |app, _event, window, cx| app.refresh_remote_devices(window, cx),
                        ))
                        .into_any_element(),
                ),
                device_rows,
                cx,
            )
            .into_any_element(),
        ]))
        .into_any_element()
}

fn remote_mobile_download_banner(language: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    let title = settings_text(
        language,
        "settings.remote.mobile_download.title",
        "Codux Mobile",
    );
    let description = settings_text(
        language,
        "settings.remote.mobile_download.description",
        "Download the mobile app to connect Codux and keep AI coding from your phone",
    );
    let action = settings_text(
        language,
        "settings.remote.mobile_download.action",
        "Get Mobile App",
    );

    div()
        .id("settings-remote-mobile-download")
        .min_h(px(72.0))
        .rounded(px(12.0))
        .border_1()
        .border_color(color(theme::ACCENT).opacity(0.28))
        .bg(color(theme::ACCENT).opacity(0.08))
        .px(px(18.0))
        .py(px(14.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(16.0))
        .cursor_pointer()
        .hover(|style| style.bg(color(theme::ACCENT).opacity(0.12)))
        .on_click(cx.listener(|app, _event, _window, _cx| {
            let _ = app.runtime_service.open_url(CODUX_MOBILE_DOWNLOAD_URL);
        }))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .items_center()
                .gap(px(12.0))
                .child(
                    div()
                        .size(px(36.0))
                        .flex_none()
                        .rounded(px(10.0))
                        .flex()
                        .items_center()
                        .justify_center()
                        .bg(color(theme::ACCENT).opacity(0.14))
                        .child(
                            Icon::new(HeroIconName::DevicePhoneMobile)
                                .size_4()
                                .text_color(color(theme::ACCENT)),
                        ),
                )
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .line_height(rems(1.125))
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_color(color(theme::TEXT))
                                .child(title),
                        )
                        .child(
                            div()
                                .mt(px(3.0))
                                .text_size(rems(0.75))
                                .line_height(rems(1.0625))
                                .text_color(color(theme::TEXT_DIM))
                                .child(description),
                        ),
                ),
        )
        .child(
            div()
                .flex_none()
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .font_weight(gpui::FontWeight::MEDIUM)
                .text_color(color(theme::ACCENT))
                .child(action)
                .child(
                    Icon::new(HeroIconName::ArrowTopRightOnSquare)
                        .size_3p5()
                        .text_color(color(theme::ACCENT)),
                ),
        )
        .into_any_element()
}

fn settings_shortcuts_pane(
    settings: &SettingsSummary,
    recording_id: Option<&str>,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    settings_form(vec![
        settings_card(
            Some(settings_text(
                language,
                "settings.tab.shortcuts",
                "Shortcuts",
            )),
            None,
            shortcut_definitions()
                .into_iter()
                .map(|shortcut| shortcut_row(shortcut, settings, recording_id, language, cx))
                .collect(),
            cx,
        )
        .into_any_element(),
        settings_card(
            Some(settings_text(
                language,
                "settings.shortcut.project_switch",
                "Project Switch Shortcuts",
            )),
            None,
            vec![
                div()
                    .py(px(8.0))
                    .text_size(rems(0.75))
                    .line_height(rems(1.0))
                    .text_color(color(theme::TEXT_DIM))
                    .child(settings_text(
                        language,
                        "settings.shortcut.project_switch_hint",
                        if cfg!(target_os = "macos") {
                            "Use ⌘1-⌘9 to switch projects in sidebar order."
                        } else {
                            "Use Ctrl+1-Ctrl+9 to switch projects in sidebar order."
                        },
                    ))
                    .into_any_element(),
            ],
            cx,
        )
        .into_any_element(),
    ])
    .into_any_element()
}

#[derive(Clone, Copy)]
struct ShortcutDefinition {
    id: &'static str,
    label_key: &'static str,
    fallback: &'static str,
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
            label_key: "shortcut.view.terminal",
            fallback: "Terminal View",
            default_value: primary_static(primary, "Alt+1"),
        },
        ShortcutDefinition {
            id: "view.files",
            label_key: "shortcut.view.files",
            fallback: "Files View",
            default_value: primary_static(primary, "Alt+2"),
        },
        ShortcutDefinition {
            id: "view.review",
            label_key: "shortcut.view.review",
            fallback: "Review View",
            default_value: primary_static(primary, "Alt+3"),
        },
        ShortcutDefinition {
            id: "project.create",
            label_key: "shortcut.project.create",
            fallback: "New Project",
            default_value: primary_static(primary, "N"),
        },
        ShortcutDefinition {
            id: "project.open_folder",
            label_key: "settings.shortcut.open_project_folder",
            fallback: "Open Project Folder",
            default_value: primary_static(primary, "O"),
        },
        ShortcutDefinition {
            id: "settings.open",
            label_key: "shortcut.settings.open",
            fallback: "Open Settings",
            default_value: primary_static(primary, ","),
        },
        ShortcutDefinition {
            id: "task.create",
            label_key: "shortcut.task.create",
            fallback: "New Worktree",
            default_value: primary_static(primary, "Shift+N"),
        },
        ShortcutDefinition {
            id: "editor.save",
            label_key: "common.save",
            fallback: "Save",
            default_value: primary_static(primary, "S"),
        },
        ShortcutDefinition {
            id: "editor.search",
            label_key: "shortcut.editor.search",
            fallback: "Search Files",
            default_value: primary_static(primary, "F"),
        },
        ShortcutDefinition {
            id: "close.active",
            label_key: "shortcut.close.active",
            fallback: "Close Current Split",
            default_value: primary_static(primary, "W"),
        },
        ShortcutDefinition {
            id: "sidebar.projects.toggle",
            label_key: "menu.view.projects_sidebar",
            fallback: "Projects Sidebar",
            default_value: primary_static(primary, "Alt+P"),
        },
        ShortcutDefinition {
            id: "sidebar.tasks.toggle",
            label_key: "menu.view.tasks_sidebar",
            fallback: "Worktree Sidebar",
            default_value: primary_static(primary, "Alt+T"),
        },
        ShortcutDefinition {
            id: "assistant.git.open",
            label_key: "settings.shortcut.open_git_panel",
            fallback: "Git Panel",
            default_value: primary_static(primary, "Shift+G"),
        },
        ShortcutDefinition {
            id: "assistant.files.open",
            label_key: "settings.shortcut.open_files_panel",
            fallback: "Files Panel",
            default_value: primary_static(primary, "Shift+F"),
        },
        ShortcutDefinition {
            id: "assistant.ai.open",
            label_key: "settings.shortcut.open_ai_panel",
            fallback: "AI Panel",
            default_value: primary_static(primary, "Shift+A"),
        },
        ShortcutDefinition {
            id: "assistant.ssh.open",
            label_key: "settings.shortcut.open_ssh_panel",
            fallback: "SSH Panel",
            default_value: primary_static(primary, "Shift+S"),
        },
        ShortcutDefinition {
            id: "terminal.split.create",
            label_key: "settings.shortcut.create_split",
            fallback: "Create Split",
            default_value: primary_static(primary, "T"),
        },
    ]
}

fn primary_static(primary: &str, key: &str) -> &'static str {
    match (primary, key) {
        ("⌘", "Alt+1") => "⌘⌥1",
        ("⌘", "Alt+2") => "⌘⌥2",
        ("⌘", "Alt+3") => "⌘⌥3",
        ("⌘", "N") => "⌘N",
        ("⌘", "O") => "⌘O",
        ("⌘", "Shift+N") => "⌘⇧N",
        ("⌘", ",") => "⌘,",
        ("⌘", "S") => "⌘S",
        ("⌘", "F") => "⌘F",
        ("⌘", "T") => "⌘T",
        ("⌘", "W") => "⌘W",
        ("⌘", "Alt+P") => "⌘⌥P",
        ("⌘", "Alt+T") => "⌘⌥T",
        ("⌘", "Shift+G") => "⌘⇧G",
        ("⌘", "Shift+F") => "⌘⇧F",
        ("⌘", "Shift+A") => "⌘⇧A",
        ("⌘", "Shift+S") => "⌘⇧S",
        ("⌘", "Shift+Backslash") => "⌘⇧\\",
        ("⌘", "Shift+T") => "⌘⇧T",
        (_, "Alt+1") => "Ctrl+Alt+1",
        (_, "Alt+2") => "Ctrl+Alt+2",
        (_, "Alt+3") => "Ctrl+Alt+3",
        (_, "N") => "Ctrl+N",
        (_, "O") => "Ctrl+O",
        (_, "Shift+N") => "Ctrl+Shift+N",
        (_, ",") => "Ctrl+,",
        (_, "S") => "Ctrl+S",
        (_, "F") => "Ctrl+F",
        (_, "T") => "Ctrl+T",
        (_, "W") => "Ctrl+W",
        (_, "Alt+P") => "Ctrl+Alt+P",
        (_, "Alt+T") => "Ctrl+Alt+T",
        (_, "Shift+G") => "Ctrl+Shift+G",
        (_, "Shift+F") => "Ctrl+Shift+F",
        (_, "Shift+A") => "Ctrl+Shift+A",
        (_, "Shift+S") => "Ctrl+Shift+S",
        (_, "Shift+Backslash") => "Ctrl+Shift+\\",
        (_, "Shift+T") => "Ctrl+Shift+T",
        _ => "",
    }
}

fn shortcut_row(
    shortcut: ShortcutDefinition,
    settings: &SettingsSummary,
    recording_id: Option<&str>,
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let is_recording = recording_id == Some(shortcut.id);
    let customized = settings.shortcuts.contains_key(shortcut.id);
    let value = if is_recording {
        settings_text(language, "settings.shortcut.record", "Record Shortcut")
    } else {
        settings
            .shortcuts
            .get(shortcut.id)
            .cloned()
            .unwrap_or_else(|| shortcut.default_value.to_string())
    };

    let shortcut_id = shortcut.id;
    settings_row(
        settings_text(language, shortcut.label_key, shortcut.fallback),
        None,
        div()
            .w_full()
            .min_w_0()
            .flex()
            .items_center()
            .justify_end()
            .gap(px(8.0))
            .child(
                Button::new(SharedString::from(format!("shortcut-record-{shortcut_id}")))
                    .secondary()
                    .text_color(color(theme::TEXT))
                    .bg(if is_recording {
                        cx.theme().secondary_hover
                    } else {
                        cx.theme().secondary
                    })
                    .flex_1()
                    .justify_start()
                    .on_click(cx.listener(move |app, _event, window, cx| {
                        app.record_shortcut(shortcut_id, window, cx)
                    }))
                    .child(
                        div()
                            .text_size(rems(0.875))
                            .line_height(rems(1.125))
                            .truncate()
                            .child(value),
                    ),
            )
            .when(customized, |this| {
                this.child(settings_small_button(
                    format!("shortcut-reset-{shortcut_id}"),
                    settings_text(language, "common.undo", "Undo"),
                    cx,
                    move |app, _event, window, cx| app.reset_shortcut(shortcut_id, window, cx),
                ))
            })
            .into_any_element(),
    )
    .into_any_element()
}

fn settings_developer_pane(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let language = settings.language.as_str();
    settings_form(vec![
        settings_card(
            None,
            None,
            vec![
                settings_row(
                    settings_text(
                        language,
                        "settings.developer.performance_monitor",
                        "Performance Monitor HUD",
                    ),
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
                    settings_text(
                        language,
                        "settings.developer.performance_monitor_interval",
                        "Performance Monitor Interval",
                    ),
                    None,
                    settings_select_impl(
                        "settings-dev-refresh",
                        &settings.developer_refresh,
                        developer_refresh_options(),
                        window,
                        cx,
                        language,
                        |app, value, window, cx| app.set_developer_refresh(value, window, cx),
                    ),
                )
                .into_any_element(),
            ],
            cx,
        )
        .into_any_element(),
    ])
    .into_any_element()
}

fn settings_runtime_tool_block(
    label: String,
    tool_key: &'static str,
    model_key: &'static str,
    permission: &str,
    model: &str,
    placeholder: &'static str,
    include_permission: bool,
    include_codex_effort: bool,
    codex_effort: &str,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let mut children = vec![
        div()
            .text_size(rems(0.875))
            .line_height(rems(1.125))
            .text_color(color(theme::TEXT))
            .child(label)
            .into_any_element(),
    ];
    if include_permission {
        children.push(
            settings_row(
                settings_text(
                    language,
                    "settings.ai.permission.full_access_toggle",
                    "Full Access",
                ),
                None,
                settings_select_impl(
                    tool_key,
                    permission,
                    runtime_tool_permission_options(language),
                    window,
                    cx,
                    language,
                    move |app, value, window, cx| {
                        app.set_runtime_tool_permission(tool_key, value, window, cx)
                    },
                ),
            )
            .into_any_element(),
        );
    }
    children.push(
        settings_row(
            settings_text(language, "settings.ai.tool.default_model", "Default Model"),
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
    );
    if include_codex_effort {
        children.push(
            settings_row(
                settings_text(
                    language,
                    "settings.ai.tool.reasoning_effort",
                    "Reasoning Effort",
                ),
                None,
                settings_select_impl(
                    "settings-codex-effort",
                    codex_effort,
                    codex_effort_options(language),
                    window,
                    cx,
                    language,
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

fn settings_ai_provider_card(
    provider: codux_runtime::settings::AIProviderSummary,
    selected_provider_id: Option<&str>,
    testing_provider_id: Option<&str>,
    test_result: Option<&AIProviderTestResult>,
    language: &str,
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
    let result = test_result.filter(|result| result.provider_id == provider.id);
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
                        .text_size(rems(0.875))
                        .line_height(rems(1.125))
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
            settings_text(language, "settings.ai.provider.kind", "Kind"),
            None,
            settings_select_impl(
                format!("settings-provider-kind-{}", provider.id),
                &provider.kind,
                ai_provider_kind_options(),
                window,
                cx,
                language,
                move |app, value, window, cx| {
                    app.update_ai_provider_string(kind_id.clone(), "kind", value, window, cx)
                },
            ),
        ))
        .child(settings_row(
            settings_text(language, "settings.ai.provider.name", "Name"),
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
            settings_text(language, "settings.ai.provider.model", "Model"),
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
            settings_text(language, "settings.ai.provider.base_url", "Base URL"),
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
            settings_text(language, "settings.ai.provider.api_key", "API Key"),
            None,
            settings_text_input(
                SharedString::from(format!("settings-provider-api-key-{}", provider.id)),
                "",
                if provider.api_key_configured {
                    settings_text(language, "common.configured", "Configured")
                } else {
                    settings_text(language, "settings.ai.provider.api_key", "API Key")
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
            settings_text(
                language,
                "settings.ai.provider.use_for_memory_extraction",
                "Use For Memory Extraction",
            ),
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
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(if let Some(result) = result {
                    settings_status_tag(
                        result.message.clone(),
                        if result.ok {
                            theme::ACCENT
                        } else {
                            theme::ORANGE
                        },
                    )
                } else {
                    div().hidden().into_any_element()
                })
                .child(
                    div()
                        .flex()
                        .items_center()
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
                                    .text_size(rems(0.75))
                                    .line_height(rems(1.0))
                                    .text_color(color(theme::TEXT))
                                    .child(if testing {
                                        settings_text(
                                            language,
                                            "settings.ai.provider.test.running",
                                            "Testing...",
                                        )
                                    } else {
                                        settings_text(language, "common.test", "Test")
                                    }),
                            ),
                        )
                        .child(settings_small_button(
                            format!("settings-provider-remove-{}", provider.id),
                            settings_text(language, "common.remove", "Remove"),
                            cx,
                            {
                                let remove_id = provider.id.clone();
                                move |app, _event, window, cx| {
                                    app.remove_ai_provider(remove_id.clone(), window, cx)
                                }
                            },
                        )),
                ),
        )
        .into_any_element()
}

fn settings_notification_card(
    channel: codux_runtime::notification::NotificationChannelSummary,
    testing_channel_id: Option<&str>,
    language: &str,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let enabled_id = channel.id.clone();
    let endpoint_id = channel.id.clone();
    let token_id = channel.id.clone();
    let testing = testing_channel_id
        .map(|id| id == channel.id)
        .unwrap_or(false);
    let test_disabled = testing_channel_id.is_some() || channel.endpoint.trim().is_empty();
    settings_card(
        None,
        None,
        {
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
                                    .text_size(rems(0.875))
                                    .line_height(rems(1.125))
                                    .text_color(color(theme::TEXT))
                                    .child(channel.label.clone()),
                            )
                            .child(
                                div()
                                    .mt(px(4.0))
                                    .text_size(rems(0.75))
                                    .line_height(rems(1.0))
                                    .text_color(color(theme::TEXT_DIM))
                                    .child(notification_channel_description(&channel.id, language)),
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
                            app.set_notification_channel_enabled(
                                enabled_id.clone(),
                                next,
                                window,
                                cx,
                            )
                        },
                    ))
                    .into_any_element(),
            ];
            if channel.enabled {
                rows.extend([
                    settings_row(
                        notification_endpoint_label(&channel.id, language),
                        None,
                        settings_text_input(
                            SharedString::from(format!(
                                "settings-notification-endpoint-{}",
                                channel.id
                            )),
                            channel.endpoint.clone(),
                            notification_endpoint_label(&channel.id, language),
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
                        notification_token_label(&channel.id, language),
                        None,
                        settings_text_input(
                            SharedString::from(format!(
                                "settings-notification-token-{}",
                                channel.id
                            )),
                            channel.token.clone(),
                            notification_token_label(&channel.id, language),
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
                            if testing {
                                settings_text(
                                    language,
                                    "settings.ai.provider.test.running",
                                    "Testing...",
                                )
                            } else {
                                settings_text(language, "common.test", "Test")
                            },
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
        },
        cx,
    )
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
    label_key: &'static str,
    fallback: &'static str,
}

fn opt(value: &'static str, label: &'static str) -> (String, SharedString) {
    (value.to_string(), SharedString::from(label))
}

fn language_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "system",
            settings_text(language, "settings.language.follow_system", "Follow System"),
        ),
        ("simplifiedChinese", "简体中文".to_string()),
        ("traditionalChinese", "繁體中文".to_string()),
        ("english", "English".to_string()),
        ("japanese", "日本語".to_string()),
        ("korean", "한국어".to_string()),
        ("french", "Français".to_string()),
        ("german", "Deutsch".to_string()),
        ("spanish", "Español".to_string()),
        ("portugueseBrazil", "Português (Brasil)".to_string()),
        ("russian", "Русский".to_string()),
    ]
    .into_iter()
    .map(|(value, label)| (value.to_string(), SharedString::from(label)))
    .collect()
}

fn sleep_mode_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "off",
            settings_text(language, "settings.sleep_prevention.mode.off", "Off"),
        ),
        (
            "always",
            settings_text(language, "settings.sleep_prevention.mode.always", "Always"),
        ),
        (
            "powerAdapterOnly",
            settings_text(
                language,
                "settings.sleep_prevention.mode.power_adapter_only",
                "On Power Only",
            ),
        ),
    ]
    .into_iter()
    .map(|(value, label)| (value.to_string(), SharedString::from(label)))
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

fn statistics_mode_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "normalized",
            settings_text(
                language,
                "settings.ai_statistics_mode.normalized",
                "Exclude Cache",
            ),
        ),
        (
            "includingCache",
            settings_text(
                language,
                "settings.ai_statistics_mode.including_cache",
                "Include Cache",
            ),
        ),
    ]
    .into_iter()
    .map(|(value, label)| (value.to_string(), SharedString::from(label)))
    .collect()
}

fn file_open_default_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "edit",
            settings_text(language, "settings.file_open_default.edit", "Editor"),
        ),
        (
            "preview",
            settings_text(language, "settings.file_open_default.preview", "Preview"),
        ),
        (
            "split",
            settings_text(language, "settings.file_open_default.split", "Split"),
        ),
    ]
    .into_iter()
    .map(|(value, label)| (value.to_string(), SharedString::from(label)))
    .collect()
}

fn update_channel_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "stable",
            settings_text(language, "settings.update.channel.stable", "Stable"),
        ),
        (
            "beta",
            settings_text(language, "settings.update.channel.beta", "Beta"),
        ),
    ]
    .into_iter()
    .map(|(value, label)| (value.to_string(), SharedString::from(label)))
    .collect()
}

fn system_theme_options() -> Vec<(&'static str, &'static str)> {
    vec![("Auto", "Follow System")]
}

fn dark_theme_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Codux Dark", "Codux Dark"),
        ("Deep Ocean", "Deep Ocean"),
        ("Arctic Night", "Arctic Night"),
        ("Forest Night", "Forest Night"),
        ("Ember", "Ember"),
        ("Amethyst Dusk", "Amethyst Dusk"),
        ("Rose Noir", "Rose Noir"),
        ("Carbon", "Carbon"),
    ]
}

fn light_theme_options() -> Vec<(&'static str, &'static str)> {
    vec![
        ("Codux Light", "Codux Light"),
        ("Glacier", "Glacier"),
        ("Morning Mist", "Morning Mist"),
        ("Matcha", "Matcha"),
        ("Ivory", "Ivory"),
        ("Lavender", "Lavender"),
        ("Rosewater", "Rosewater"),
        ("Sandstone", "Sandstone"),
    ]
}

fn terminal_theme_preview(value: &str) -> TerminalThemePreview {
    let palette = theme::terminal_theme_palette(value);
    TerminalThemePreview {
        background: palette.background,
        foreground: palette.foreground,
        muted_foreground: palette.muted_foreground,
        selection: palette.selection,
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
            label_key: "settings.app_icon.option.default",
            fallback: "Default",
        },
        IconStyleValue {
            value: "cobalt",
            label_key: "settings.app_icon.option.cobalt",
            fallback: "Cobalt",
        },
        IconStyleValue {
            value: "sunset",
            label_key: "settings.app_icon.option.sunset",
            fallback: "Sunset",
        },
        IconStyleValue {
            value: "forest",
            label_key: "settings.app_icon.option.forest",
            fallback: "Forest",
        },
    ]
}

fn app_icon_asset_path(style: &str) -> &'static str {
    match style {
        "cobalt" => "app-icons/codux-cobalt.svg",
        "sunset" => "app-icons/codux-sunset.svg",
        "forest" => "app-icons/codux-forest.svg",
        _ => "app-icons/codux-default.svg",
    }
}

fn terminal_scrollback_options(language: &str) -> Vec<(String, SharedString)> {
    ["500", "1000", "2000", "5000", "10000"]
        .into_iter()
        .map(|value| {
            let label = settings_text(
                language,
                "settings.terminal_scrollback.option_format",
                "%@ lines",
            )
            .replace("%@", value);
            (value.to_string(), SharedString::from(label))
        })
        .collect()
}

fn terminal_font_family_options(
    language: &str,
    selected: &str,
    families: &[String],
) -> Vec<(String, SharedString)> {
    let mut options = vec![(
        String::new(),
        SharedString::from(settings_text(
            language,
            "settings.terminal_font_family.default",
            "System Default",
        )),
    )];
    let selected = selected.trim();
    for family in families {
        let family = family.trim();
        if family.is_empty() {
            continue;
        }
        options.push((family.to_string(), SharedString::from(family.to_string())));
    }
    if !selected.is_empty()
        && !options
            .iter()
            .any(|(value, _)| value.eq_ignore_ascii_case(selected))
    {
        options.push((
            selected.to_string(),
            SharedString::from(selected.to_string()),
        ));
    }
    options
}

fn numeric_range_options(range: RangeInclusive<i64>) -> Vec<(String, SharedString)> {
    range
        .map(|value| {
            let value = value.to_string();
            (value.clone(), SharedString::from(value))
        })
        .collect()
}

fn terminal_font_size_options() -> Vec<(String, SharedString)> {
    numeric_range_options(8..=28)
}

fn terminal_padding_options() -> Vec<(String, SharedString)> {
    numeric_range_options(0..=40)
}

fn pet_speech_mode_options(language: &str) -> Vec<(String, SharedString)> {
    ["mixed", "off", "encourage", "roast", "flirty", "chuunibyou"]
        .into_iter()
        .map(|value| {
            (
                value.to_string(),
                SharedString::from(settings_text(
                    language,
                    &format!("pet.speech.mode.{value}"),
                    value,
                )),
            )
        })
        .collect()
}

fn pet_speech_frequency_options(language: &str) -> Vec<(String, SharedString)> {
    ["quiet", "normal", "lively", "chatterbox"]
        .into_iter()
        .map(|value| {
            (
                value.to_string(),
                SharedString::from(pet_speech_frequency_option_label(language, value)),
            )
        })
        .collect()
}

fn pet_speech_frequency_option_label(language: &str, value: &str) -> String {
    let (hourly, cooldown_seconds) = pet_speech_frequency_config(value);
    let cooldown = pet_speech_cooldown_label(language, cooldown_seconds);
    settings_text(
        language,
        "settings.pet.speech.frequency_option_format",
        "%@ · %@/hour · cooldown %@",
    )
    .replacen(
        "%@",
        &settings_text(language, &format!("pet.speech.frequency.{value}"), value),
        1,
    )
    .replacen("%@", hourly, 1)
    .replacen("%@", &cooldown, 1)
}

fn pet_speech_frequency_config(value: &str) -> (&'static str, u32) {
    match value {
        "quiet" => ("0-1", 300),
        "lively" => ("3-8", 30),
        "chatterbox" => ("8-15", 30),
        _ => ("1-3", 60),
    }
}

fn pet_speech_cooldown_label(language: &str, seconds: u32) -> String {
    if seconds >= 60 {
        settings_text(
            language,
            "settings.pet.speech.cooldown.minutes_format",
            "%d min",
        )
        .replace("%d", &(seconds / 60).to_string())
    } else {
        settings_text(
            language,
            "settings.pet.speech.cooldown.seconds_format",
            "%d sec",
        )
        .replace("%d", &seconds.to_string())
    }
}

fn pet_reminder_interval_options(language: &str) -> Vec<(String, SharedString)> {
    ["15", "30", "45", "60", "90", "120", "180", "240"]
        .into_iter()
        .map(|value| {
            let label = settings_text(language, "settings.interval.minutes_format", "%@ min")
                .replace("%@", value);
            (value.to_string(), SharedString::from(label))
        })
        .collect()
}

fn runtime_tool_permission_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "default",
            settings_text(language, "settings.tools.permission.default", "Default"),
        ),
        (
            "fullAccess",
            settings_text(
                language,
                "settings.tools.permission.full_access",
                "Full Access",
            ),
        ),
    ]
    .into_iter()
    .map(|(value, label)| (value.to_string(), SharedString::from(label)))
    .collect()
}

fn remote_relay_preset_options(language: &str) -> Vec<(String, SharedString)> {
    let _ = language;
    codux_runtime::remote::remote_relay_presets()
        .iter()
        .map(|preset| (preset.key.clone(), SharedString::from(preset.name.clone())))
        .collect()
}

fn settings_remote_relay_custom_fields(
    settings: &SettingsSummary,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
    language: &str,
) -> AnyElement {
    if settings.remote_relay_preset != "custom" {
        return div().into_any_element();
    }
    div()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(settings_row(
            settings_text(language, "settings.remote.relay_url", "Custom Relay URL"),
            Some(settings_text(
                language,
                "settings.remote.relay_url.help",
                "Leave empty to use the public network. Pair again after changing it",
            )),
            settings_remote_relay_url_editor(
                settings.remote_relay_url.as_str(),
                window,
                cx,
                language,
            ),
        ))
        .child(settings_row(
            settings_text(
                language,
                "settings.remote.relay_authentication",
                "Relay Authentication",
            ),
            Some(settings_text(
                language,
                "settings.remote.relay_authentication.help",
                "Optional Bearer token for custom Iroh relays. Pair again after changing it",
            )),
            settings_remote_relay_authentication_editor(
                settings.remote_relay_authentication.as_str(),
                window,
                cx,
            ),
        ))
        .into_any_element()
}

fn codex_effort_options(language: &str) -> Vec<(String, SharedString)> {
    vec![
        (
            "none".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.tool.reasoning_effort.default",
                "Default",
            )),
        ),
        (
            "minimal".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.tool.reasoning_effort.minimal",
                "Minimal",
            )),
        ),
        (
            "low".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.tool.reasoning_effort.low",
                "Low",
            )),
        ),
        (
            "medium".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.tool.reasoning_effort.medium",
                "Medium",
            )),
        ),
        (
            "high".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.tool.reasoning_effort.high",
                "High",
            )),
        ),
        (
            "xhigh".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.tool.reasoning_effort.xhigh",
                "XHigh",
            )),
        ),
    ]
}

fn git_provider_options(settings: &SettingsSummary, language: &str) -> Vec<(String, SharedString)> {
    let mut options = vec![
        (
            "automatic".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.git_commit_message_provider.automatic",
                "Automatic",
            )),
        ),
        (
            "off".to_string(),
            SharedString::from(settings_text(
                language,
                "settings.ai.git_commit_message_provider.off",
                "Off",
            )),
        ),
    ];
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
        ("concise", "Concise"),
        ("sentence", "Sentence"),
        ("changelog", "Changelog"),
    ]
    .into_iter()
    .map(|(value, label)| opt(value, label))
    .collect()
}

fn git_language_options(language: &str) -> Vec<(String, SharedString)> {
    let mut options = vec![(
        "application".to_string(),
        SharedString::from(settings_text(
            language,
            "settings.ai.git_commit_message_language.follow",
            "Follow App",
        )),
    )];
    options.extend(
        language_options(language)
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

fn ai_provider_options(
    settings: &SettingsSummary,
    purpose: &str,
    language: &str,
) -> Vec<(String, SharedString)> {
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

    let mut options = vec![(
        "automatic".to_string(),
        SharedString::from(settings_text(
            language,
            "settings.ai.memory.extraction_provider.automatic",
            "Automatic",
        )),
    )];
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

fn memory_max_index_options(language: &str) -> Vec<(String, SharedString)> {
    ["5", "10", "20", "50", "100"]
        .into_iter()
        .map(|value| {
            let label = settings_text(
                language,
                "settings.ai.memory.max_index_sessions.option_format",
                "%@ sessions",
            )
            .replace("%@", value);
            (value.to_string(), SharedString::from(label))
        })
        .collect()
}

fn notification_endpoint_label(channel_id: &str, language: &str) -> String {
    let fallback = match channel_id {
        "bark" => "Server URL",
        "ntfy" => "Topic URL",
        "wxpusher" => "SPT Token",
        "telegram" => "Chat ID",
        "webhook" => "Request URL",
        _ => "Webhook URL",
    };
    settings_text(
        language,
        &format!("settings.notifications.channel.{channel_id}.endpoint"),
        fallback,
    )
}

fn notification_token_label(channel_id: &str, language: &str) -> String {
    let fallback = match channel_id {
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
    };
    settings_text(
        language,
        &format!("settings.notifications.channel.{channel_id}.token"),
        fallback,
    )
}

fn notification_channel_description(channel_id: &str, language: &str) -> String {
    let fallback = match channel_id {
        "bark" => "Send push notifications through Bark service and device key.",
        "ntfy" => "Publish notifications to an ntfy topic.",
        "wxpusher" => "Send notifications to a WxPusher SPT target.",
        "feishu" => "Send notifications through a Feishu bot webhook.",
        "dingtalk" => "Send notifications through a DingTalk bot webhook.",
        "wecom" => "Send notifications to a WeCom group bot.",
        "telegram" => "Send notifications with a Telegram bot token and chat id.",
        "discord" => "Send notifications to a Discord webhook.",
        "slack" => "Send notifications to a Slack incoming webhook.",
        "webhook" => "Send a JSON POST request to a custom endpoint.",
        _ => "Custom notification channel.",
    };
    settings_text(
        language,
        &format!("settings.notifications.channel.{channel_id}.description"),
        fallback,
    )
}

fn remote_status_label(remote: &RemoteSummary, language: &str) -> String {
    match remote.status.as_str() {
        "connected" => settings_text(language, "remote.status.connected_label", "Connected"),
        "connecting" => settings_text(language, "remote.status.connecting_label", "Connecting"),
        "failed" => settings_text(language, "settings.ai.local_model.status.failed", "Failed"),
        _ => settings_text(language, "remote.status.disconnected_label", "Disconnected"),
    }
}

fn remote_status_color(remote: &RemoteSummary) -> u32 {
    match remote.status.as_str() {
        "connected" => theme::GREEN,
        "connecting" => theme::ORANGE,
        "failed" => theme::RED,
        _ => theme::TEXT_DIM,
    }
}
