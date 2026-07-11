use super::ui_helpers::{dialog_primary_button, window_close_control};
use super::{CoduxApp, UiRegion, empty_label};
use crate::app::{
    AIProviderTestResult,
    app_select::{CoduxSelectConfig, CoduxSelectOption, codux_select},
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

mod options;
mod widgets;

#[path = "panes/ai.rs"]
mod ai;
#[path = "panes/appearance.rs"]
mod appearance;
#[path = "panes/developer.rs"]
mod developer;
#[path = "panes/general.rs"]
mod general;
#[path = "panes/git.rs"]
mod git;
#[path = "panes/memory.rs"]
mod memory;
#[path = "panes/notifications.rs"]
mod notifications;
#[path = "panes/pet.rs"]
mod pet;
#[path = "panes/remote/mod.rs"]
mod remote;
#[path = "panes/shortcuts.rs"]
mod shortcuts;

use self::{
    ai::settings_ai_pane,
    appearance::settings_appearance_pane,
    developer::settings_developer_pane,
    general::settings_general_pane,
    git::settings_git_pane,
    memory::settings_memory_pane,
    notifications::settings_notifications_pane,
    pet::settings_pet_pane,
    remote::{
        SettingsRemotePaneInput, remote_connect_overlay, remote_pairing_overlay,
        remote_pending_pairing_overlay, settings_remote_pane,
    },
    shortcuts::settings_shortcuts_pane,
};

const CODUX_MOBILE_DOWNLOAD_URL: &str = "https://codux.dux.cn/features/mobile/";
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
                                this.child(window_close_control(
                                    "settings-window-close",
                                    28.0,
                                    true,
                                    cx,
                                ))
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
                SettingsRemotePaneInput {
                    settings: &app.state.settings,
                    remote: &app.state.remote,
                    saved_hosts: &saved_hosts,
                    link_states: &link_states,
                    link_paths: &link_paths,
                    language: app.state.settings.language.as_str(),
                    remote_reconnecting: app.remote_reconnecting,
                    remote_pairing_creating: app.remote_pairing_creating,
                },
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
