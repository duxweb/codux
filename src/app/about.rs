use super::*;
use codux_runtime::{
    app_info::{AppAboutMetadata, DiagnosticsExportRequest},
    dialog::{DialogFilter, LocalizedSaveDialogRequest},
};

const CODUX_WEBSITE_URL: &str = "https://codux.dux.cn";
const CODUX_IDENTIFIER: &str = "com.duxweb.codux";

impl CoduxApp {
    pub(in crate::app) fn about_workspace(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let about = self
            .runtime_service
            .about_metadata(env!("CARGO_PKG_VERSION"), CODUX_IDENTIFIER);
        let update = self.runtime_service.update_status(
            std::env::current_dir().unwrap_or_default(),
            env!("CARGO_PKG_VERSION"),
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .items_center()
            .bg(color(theme::BG))
            .text_color(color(theme::TEXT))
            .child(div().h(px(28.0)).flex_shrink_0())
            .child(about_icon_mark())
            .child(
                div()
                    .mt(px(14.0))
                    .text_size(px(20.0))
                    .line_height(px(24.0))
                    .font_weight(FontWeight::BOLD)
                    .child(about.name.clone()),
            )
            .child(
                div()
                    .mt(px(6.0))
                    .text_size(px(12.0))
                    .line_height(px(16.0))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(format!(
                        "{} · {}/{} · {}",
                        about.version, about.target_os, about.target_arch, about.build_profile
                    )),
            )
            .child(
                div()
                    .mt(px(22.0))
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(4.0))
                    .child(
                        div()
                            .text_size(px(12.0))
                            .line_height(px(16.0))
                            .text_color(color(theme::TEXT_MUTED))
                            .child("AI-Powered Terminal Workspace"),
                    )
                    .child(
                        div()
                            .text_size(px(11.0))
                            .line_height(px(15.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child("Copyright (c) 2025 Codux contributors"),
                    ),
            )
            .child(about_status_card(&about, &update))
            .child(about_action_row(cx))
            .child(
                div()
                    .mt(px(18.0))
                    .max_w(px(300.0))
                    .truncate()
                    .text_size(px(11.0))
                    .line_height(px(15.0))
                    .text_color(color(theme::TEXT_DIM))
                    .child(about.identifier),
            )
    }

    pub(in crate::app) fn open_about_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let bounds = Bounds::centered(None, size(px(420.0), px(520.0)), cx);
        let result = cx.open_window(
            WindowOptions {
                titlebar: Some(gpui::TitlebarOptions {
                    title: Some("About Codux".into()),
                    appears_transparent: true,
                    ..Default::default()
                }),
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(380.0), px(480.0))),
                ..Default::default()
            },
            |window, cx| {
                let mut app = CoduxApp::new_settings_window();
                app.window_mode = AppWindowMode::About;
                theme::apply_component_theme_for_name(&app.state.settings.theme, Some(window), cx);
                let view = cx.new(|_| app);
                cx.new(|cx| Root::new(view, window, cx))
            },
        );

        self.status_message = match result {
            Ok(_) => "about window opened".to_string(),
            Err(error) => format!("failed to open about window: {error}"),
        };
        cx.notify();
    }

    pub(in crate::app) fn open_codux_website(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.open_url(CODUX_WEBSITE_URL) {
            Ok(()) => self.status_message = "Codux website opened".to_string(),
            Err(error) => self.status_message = format!("failed to open Codux website: {error}"),
        }
        cx.notify();
    }

    pub(in crate::app) fn open_runtime_log(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.open_runtime_log() {
            Ok(()) => self.status_message = "runtime log opened".to_string(),
            Err(error) => self.status_message = format!("failed to open runtime log: {error}"),
        }
        cx.notify();
    }

    pub(in crate::app) fn open_live_log(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.open_live_log() {
            Ok(()) => self.status_message = "live log opened".to_string(),
            Err(error) => self.status_message = format!("failed to open live log: {error}"),
        }
        cx.notify();
    }

    pub(in crate::app) fn request_restart(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.request_restart() {
            Ok(()) => self.status_message = "restart requested".to_string(),
            Err(error) => self.status_message = format!("failed to request restart: {error}"),
        }
        cx.notify();
    }

    pub(in crate::app) fn export_diagnostics(&mut self, cx: &mut Context<Self>) {
        let destination =
            match self
                .runtime_service
                .localized_save_dialog(LocalizedSaveDialogRequest {
                    title: "导出诊断".to_string(),
                    message: "选择诊断报告保存位置。".to_string(),
                    prompt: "保存".to_string(),
                    default_path: Some(format!("codux-diagnostics-{}.json", timestamp_slug())),
                    filters: vec![DialogFilter {
                        _name: "JSON".to_string(),
                        extensions: vec!["json".to_string()],
                    }],
                    can_create_directories: Some(true),
                }) {
                Ok(Some(path)) => path,
                Ok(None) => {
                    self.status_message = "diagnostics export canceled".to_string();
                    cx.notify();
                    return;
                }
                Err(error) => {
                    self.status_message = format!("failed to choose diagnostics path: {error}");
                    cx.notify();
                    return;
                }
            };

        let about = self
            .runtime_service
            .about_metadata(env!("CARGO_PKG_VERSION"), CODUX_IDENTIFIER);
        let update = self.runtime_service.update_status(
            std::env::current_dir().unwrap_or_default(),
            env!("CARGO_PKG_VERSION"),
        );
        match self.runtime_service.export_diagnostics(
            DiagnosticsExportRequest {
                destination_path: destination,
            },
            about,
            update,
        ) {
            Ok(result) => {
                self.status_message = format!(
                    "diagnostics exported: {} ({} bytes)",
                    result.path, result.bytes
                );
            }
            Err(error) => {
                self.status_message = format!("failed to export diagnostics: {error}");
            }
        }
        cx.notify();
    }
}

fn about_icon_mark() -> impl IntoElement {
    div()
        .size(px(96.0))
        .rounded(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(linear_gradient(
            145.0,
            linear_color_stop(color(theme::ACCENT), 0.0),
            linear_color_stop(color(0x7C4DFF), 1.0),
        ))
        .child(
            div()
                .text_size(px(36.0))
                .line_height(px(40.0))
                .font_weight(FontWeight::BOLD)
                .text_color(color(0xFFFFFF))
                .child("C"),
        )
}

fn about_status_card(
    about: &AppAboutMetadata,
    update: &codux_runtime::update::UpdateStatus,
) -> impl IntoElement {
    let update_label = if !update.configured {
        "更新通道未配置".to_string()
    } else if update.available {
        update
            .latest_version
            .as_ref()
            .map(|version| format!("发现新版本 {version}"))
            .unwrap_or_else(|| "发现新版本".to_string())
    } else {
        "当前已是最新版本".to_string()
    };

    div()
        .mt(px(22.0))
        .w(px(312.0))
        .rounded(px(8.0))
        .bg(color(0xFFFFFF).opacity(0.055))
        .border_1()
        .border_color(color(theme::BORDER_SOFT))
        .p(px(12.0))
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(about_info_row("描述", about.description.clone()))
        .child(about_info_row("更新", update_label))
        .child(about_info_row("模式", update.installation_mode.clone()))
}

fn about_info_row(label: &'static str, value: String) -> impl IntoElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(12.0))
        .child(
            div()
                .flex_shrink_0()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_DIM))
                .child(label),
        )
        .child(
            div()
                .min_w_0()
                .truncate()
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(color(theme::TEXT_MUTED))
                .child(value),
        )
}

fn about_action_row(cx: &mut Context<CoduxApp>) -> impl IntoElement {
    div()
        .mt(px(20.0))
        .flex()
        .flex_wrap()
        .justify_center()
        .gap(px(8.0))
        .child(about_button(
            "about-website",
            "官网",
            IconName::ExternalLink,
            cx,
            |app, _event, _window, cx| app.open_codux_website(cx),
        ))
        .child(about_button(
            "about-check-updates",
            "检查更新",
            IconName::Redo2,
            cx,
            |app, _event, window, cx| app.reload_update(window, cx),
        ))
        .child(about_button(
            "about-install-update",
            "安装更新",
            IconName::ExternalLink,
            cx,
            |app, _event, window, cx| app.install_update(window, cx),
        ))
        .child(about_button(
            "about-diagnostics",
            "导出诊断",
            IconName::File,
            cx,
            |app, _event, _window, cx| app.export_diagnostics(cx),
        ))
        .child(about_button(
            "about-runtime-log",
            "Runtime Log",
            IconName::File,
            cx,
            |app, _event, _window, cx| app.open_runtime_log(cx),
        ))
        .child(about_button(
            "about-live-log",
            "Live Log",
            IconName::File,
            cx,
            |app, _event, _window, cx| app.open_live_log(cx),
        ))
        .child(about_button(
            "about-restart",
            "重启",
            IconName::Redo2,
            cx,
            |app, _event, _window, cx| app.request_restart(cx),
        ))
}

fn about_button(
    id: &'static str,
    label: &'static str,
    icon: IconName,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> impl IntoElement {
    Button::new(id)
        .secondary()
        .compact()
        .text_color(cx.theme().secondary_foreground)
        .on_click(cx.listener(on_click))
        .child(
            div()
                .h(px(22.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .text_size(px(12.0))
                .line_height(px(16.0))
                .text_color(cx.theme().secondary_foreground)
                .child(Icon::new(icon).size_3())
                .child(label),
        )
}

fn timestamp_slug() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    seconds.to_string()
}
