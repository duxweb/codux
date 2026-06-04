use super::*;
use crate::app::app_state::UpdateDialogPhase;
use crate::app::window_actions::{AuxiliaryWindowSlot, AuxiliaryWindowSpec};
use codux_runtime::{
    app_info::{DiagnosticsExportRequest, UpdateInstallProgressEvent},
    dialog::{DialogFilter, LocalizedAlertDialogRequest, LocalizedSaveDialogRequest},
    update::UpdateStatus,
};

const CODUX_WEBSITE_URL: &str = "https://codux.dux.cn";
const CODUX_GITHUB_URL: &str = "https://github.com/duxweb/codux";
const CODUX_IDENTIFIER: &str = "com.duxweb.codux";
const UPDATE_DIALOG_WIDTH: f32 = 440.0;
const UPDATE_DIALOG_DEFAULT_HEIGHT: f32 = 200.0;
const UPDATE_DIALOG_AVAILABLE_HEIGHT: f32 = 340.0;
const UPDATE_DIALOG_PROGRESS_HEIGHT: f32 = 320.0;
const UPDATE_DIALOG_MIN_HEIGHT: f32 = 200.0;

impl CoduxApp {
    pub(in crate::app) fn about_workspace(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let locale = locale_from_language_setting(&self.state.settings.language);
        let about = self
            .runtime_service
            .about_metadata(env!("CARGO_PKG_VERSION"), CODUX_IDENTIFIER);
        child_window_shell(
            translate(&locale, "menu.app.about_format", "About Codux").replace("%@", "Codux"),
            cx,
        )
        .child(
            div()
                .flex_1()
                .min_h_0()
                .flex()
                .flex_col()
                .items_center()
                .bg(color(theme::BG))
                .text_color(color(theme::TEXT))
                .child(div().h(px(18.0)).flex_shrink_0())
                .child(about_icon_mark())
                .child(
                    div()
                        .mt(px(14.0))
                        .text_size(rems(1.25))
                        .line_height(rems(1.5))
                        .font_weight(FontWeight::BOLD)
                        .child(about.name.clone()),
                )
                .child(
                    div()
                        .mt(px(6.0))
                        .text_size(rems(0.75))
                        .line_height(rems(1.0))
                        .text_color(color(theme::TEXT_MUTED))
                        .child(about.version.clone()),
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
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_MUTED))
                                .child(translate(
                                    &locale,
                                    "about.tagline",
                                    "AI-Powered Terminal Workspace",
                                )),
                        )
                        .child(
                            div()
                                .text_size(rems(0.75))
                                .line_height(rems(1.0))
                                .text_color(color(theme::TEXT_DIM))
                                .child(translate(
                                    &locale,
                                    "about.copyright",
                                    "Copyright (c) 2025 Codux contributors",
                                )),
                        ),
                )
                .child(about_action_row(&locale, cx)),
        )
    }

    pub(in crate::app) fn open_about_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::About,
                title: SharedString::from("About Codux"),
                size: size(px(380.0), px(380.0)),
                min_size: size(px(360.0), px(360.0)),
                already_open_message: "about window already opened",
                opened_message: "about window opened",
                failed_prefix: "failed to open about window",
            },
            cx,
            |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::About;
                app
            },
            |_view, _window, _cx| {},
        );
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_update_dialog_window(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.window_mode == AppWindowMode::About {
            window.remove_window();
            self.about_window = None;
        }
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::UpdateDialog,
                title: SharedString::from("Check for Updates"),
                size: size(px(UPDATE_DIALOG_WIDTH), px(UPDATE_DIALOG_DEFAULT_HEIGHT)),
                min_size: size(px(400.0), px(UPDATE_DIALOG_MIN_HEIGHT)),
                already_open_message: "update window already opened",
                opened_message: "update window opened",
                failed_prefix: "failed to open update window",
            },
            cx,
            |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::UpdateDialog;
                app.update_dialog_phase = UpdateDialogPhase::Checking;
                app.update_dialog_status = None;
                app.update_dialog_progress = None;
                app.update_dialog_result = None;
                app.update_dialog_error = None;
                app
            },
            |view, _window, cx| {
                view.update(cx, |app, cx| app.check_update_in_dialog(cx));
            },
        );
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_memory_manager_window(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.memory_status_seen_failed_count = self.state.memory_manager.extraction.failed.max(0);
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::MemoryManager,
                title: SharedString::from("Memory Manager"),
                size: size(px(900.0), px(720.0)),
                min_size: size(px(720.0), px(560.0)),
                already_open_message: "memory manager window already opened",
                opened_message: "memory manager window opened",
                failed_prefix: "failed to open memory manager window",
            },
            cx,
            |state, runtime, runtime_service, _window, _cx| {
                CoduxApp::new_memory_manager_window(state, runtime, runtime_service)
            },
            |view, _window, cx| {
                view.update(cx, |app, cx| app.reload_memory_manager_snapshot_async(cx));
            },
        );
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_codux_website(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.open_url(CODUX_WEBSITE_URL) {
            Ok(()) => self.status_message = "Codux website opened".to_string(),
            Err(error) => self.status_message = format!("failed to open Codux website: {error}"),
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_codux_github(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.open_url(CODUX_GITHUB_URL) {
            Ok(()) => self.status_message = "Codux GitHub opened".to_string(),
            Err(error) => self.status_message = format!("failed to open Codux GitHub: {error}"),
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_user_agreement(&mut self, cx: &mut Context<Self>) {
        let language = self.state.settings.language.clone();
        let service = self.runtime_service.clone();
        self.status_message = "opening user agreement".to_string();
        cx.spawn(async move |_: gpui::WeakEntity<Self>, _cx| {
            let _ = service.localized_alert_dialog(LocalizedAlertDialogRequest {
                title: translate(&language, "about.user_agreement", "User Agreement"),
                message: [
                    translate(
                        &language,
                        "about.user_agreement_body",
                        "By using it, you understand that terminal, Git, and AI activity features read local project metadata and runtime state, but do not proactively upload your project contents.",
                    ),
                    translate(
                        &language,
                        "about.user_agreement_data",
                        "Codux only reads the local state needed to display terminal sessions, Git repository status, AI tool activity, and local statistics.",
                    ),
                    translate(
                        &language,
                        "about.user_agreement_responsibility",
                        "You are responsible for your local environment, file permissions, repository credentials, notification permissions, and any commands executed inside the terminal.",
                    ),
                    translate(
                        &language,
                        "about.user_agreement_license",
                        "Codux is distributed as open-source software under the GPL-3.0 license.",
                    ),
                ]
                .join("\n\n"),
                button_label: translate(&language, "common.ok", "OK"),
            });
        })
        .detach();
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn check_update_in_dialog(&mut self, cx: &mut Context<Self>) {
        self.update_dialog_phase = UpdateDialogPhase::Checking;
        self.update_dialog_error = None;
        self.update_dialog_result = None;
        self.update_dialog_progress = None;
        self.status_message = "checking updates".to_string();
        let service = self.runtime_service.clone();
        let repo_root = std::env::current_dir().unwrap_or_default();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let status_result = codux_runtime::async_runtime::spawn(async move {
                service.update_status(repo_root, env!("CARGO_PKG_VERSION"))
            })
            .await;
            let _ = this.update(cx, |app, cx| {
                match status_result {
                    Ok(status) => {
                        app.update_dialog_status = Some(status.clone());
                        app.update_dialog_phase = if !status.configured {
                            UpdateDialogPhase::NotConfigured
                        } else if status.available {
                            UpdateDialogPhase::Available
                        } else {
                            UpdateDialogPhase::Latest
                        };
                        app.status_message = status.message;
                    }
                    Err(error) => {
                        app.update_dialog_status = None;
                        app.update_dialog_error = Some(error.to_string());
                        app.update_dialog_phase = UpdateDialogPhase::Error;
                        app.status_message = format!("update check failed: {error}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn download_update_in_dialog(&mut self, cx: &mut Context<Self>) {
        let Some(status) = self.update_dialog_status.clone() else {
            self.update_dialog_error = Some("No update status is available.".to_string());
            self.update_dialog_phase = UpdateDialogPhase::Error;
            cx.notify();
            return;
        };
        if !status.available {
            self.update_dialog_phase = UpdateDialogPhase::Latest;
            cx.notify();
            return;
        }
        self.update_dialog_phase = UpdateDialogPhase::Downloading;
        self.update_dialog_error = None;
        self.update_dialog_result = None;
        self.update_dialog_progress = Some(UpdateInstallProgressEvent {
            phase: "downloading".to_string(),
            version: status.latest_version.clone(),
            downloaded_bytes: 0,
            total_bytes: None,
        });
        let service = self.runtime_service.clone();
        let repo_root = std::env::current_dir().unwrap_or_default();
        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let (progress_tx, progress_rx) = flume::unbounded::<UpdateInstallProgressEvent>();
            let install_service = service.clone();
            let install_task = codux_runtime::async_runtime::spawn(async move {
                install_service.install_update_with_progress(
                    repo_root,
                    env!("CARGO_PKG_VERSION"),
                    move |event| {
                        let _ = progress_tx.send(event);
                    },
                )
            });
            loop {
                match progress_rx.recv_timeout(std::time::Duration::from_millis(80)) {
                    Ok(progress) => {
                        let _ = this.update(cx, |app, cx| {
                            app.update_dialog_progress = Some(progress);
                            cx.notify();
                        });
                    }
                    Err(flume::RecvTimeoutError::Timeout) => {
                        if install_task.is_finished() {
                            break;
                        }
                    }
                    Err(flume::RecvTimeoutError::Disconnected) => break,
                }
            }
            let result = install_task.await;
            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(Ok(result)) => {
                        app.update_dialog_progress = Some(UpdateInstallProgressEvent {
                            phase: "finished".to_string(),
                            version: result.version.clone(),
                            downloaded_bytes: result.downloaded_bytes,
                            total_bytes: result.total_bytes,
                        });
                        app.update_dialog_result = Some(result.clone());
                        app.update_dialog_phase = UpdateDialogPhase::Finished;
                        app.status_message = result.message;
                    }
                    Ok(Err(error)) => {
                        app.update_dialog_error = Some(error.clone());
                        app.update_dialog_phase = UpdateDialogPhase::Error;
                        app.status_message = format!("failed to download update: {error}");
                    }
                    Err(error) => {
                        let message = error.to_string();
                        app.update_dialog_error = Some(message.clone());
                        app.update_dialog_phase = UpdateDialogPhase::Error;
                        app.status_message = format!("failed to download update: {message}");
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(in crate::app) fn open_update_download_window(
        &mut self,
        status: UpdateStatus,
        cx: &mut Context<Self>,
    ) {
        self.open_auxiliary_window(
            AuxiliaryWindowSpec {
                slot: AuxiliaryWindowSlot::UpdateDialog,
                title: SharedString::from("Download Update"),
                size: size(px(UPDATE_DIALOG_WIDTH), px(UPDATE_DIALOG_PROGRESS_HEIGHT)),
                min_size: size(px(400.0), px(UPDATE_DIALOG_MIN_HEIGHT)),
                already_open_message: "update download window already opened",
                opened_message: "update download window opened",
                failed_prefix: "failed to open update download window",
            },
            cx,
            move |state, runtime, runtime_service, _window, _cx| {
                let mut app =
                    CoduxApp::new_settings_window_from_state(state, runtime, runtime_service);
                app.window_mode = AppWindowMode::UpdateDialog;
                app.update_dialog_phase = UpdateDialogPhase::Downloading;
                app.update_dialog_status = Some(status.clone());
                app.update_dialog_progress = Some(UpdateInstallProgressEvent {
                    phase: "downloading".to_string(),
                    version: status.latest_version.clone(),
                    downloaded_bytes: 0,
                    total_bytes: None,
                });
                app.update_dialog_result = None;
                app.update_dialog_error = None;
                app
            },
            |view, _window, cx| {
                view.update(cx, |app, cx| app.download_update_in_dialog(cx));
            },
        );
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn update_dialog_workspace(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let language = self.state.settings.language.as_str();
        let window_title = translate(language, "about.updates", "Check for Updates");
        child_window_shell(window_title, cx)
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .p(px(16.0))
                    .child(update_dialog_content(self, language, cx)),
            )
            .child(update_dialog_footer(self, language, cx))
    }

    pub(in crate::app) fn open_runtime_log(&mut self, cx: &mut Context<Self>) {
        self.runtime_trace("help", "open_runtime_log");
        match self.runtime_service.open_runtime_log() {
            Ok(()) => self.status_message = "runtime log opened".to_string(),
            Err(error) => self.status_message = format!("failed to open runtime log: {error}"),
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn open_live_log(&mut self, cx: &mut Context<Self>) {
        self.runtime_trace("help", "open_live_log");
        match self.runtime_service.open_live_log() {
            Ok(()) => self.status_message = "live log opened".to_string(),
            Err(error) => self.status_message = format!("failed to open live log: {error}"),
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn request_restart(&mut self, cx: &mut Context<Self>) {
        match self.runtime_service.request_restart() {
            Ok(()) => self.status_message = "restart requested".to_string(),
            Err(error) => self.status_message = format!("failed to request restart: {error}"),
        }
        self.invalidate_status_bar(cx);
    }

    pub(in crate::app) fn export_diagnostics(&mut self, cx: &mut Context<Self>) {
        self.runtime_trace("help", "export_diagnostics choose_destination");
        self.status_message = "choosing diagnostics destination".to_string();
        self.invalidate_status_bar(cx);

        let service = self.runtime_service.clone();
        let save_request = LocalizedSaveDialogRequest {
            title: self.text("about.diagnostics.export", "Export Diagnostics"),
            message: self.text(
                "about.diagnostics.export.message",
                "Choose where to save the diagnostics report.",
            ),
            prompt: self.text("common.save", "Save"),
            default_path: Some(format!("codux-diagnostics-{}.json", timestamp_slug())),
            filters: vec![DialogFilter {
                _name: "JSON".to_string(),
                extensions: vec!["json".to_string()],
            }],
            can_create_directories: Some(true),
        };
        let about = service.about_metadata(env!("CARGO_PKG_VERSION"), CODUX_IDENTIFIER);
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        let repo_root = std::env::current_dir().unwrap_or_default();

        cx.spawn(async move |this: gpui::WeakEntity<Self>, cx| {
            let result = codux_runtime::async_runtime::spawn_blocking(move || {
                let Some(destination) = service.localized_save_dialog(save_request)? else {
                    return Ok(None);
                };
                let update = service.update_status(repo_root, &current_version);
                service
                    .export_diagnostics(
                        DiagnosticsExportRequest {
                            destination_path: destination,
                        },
                        about,
                        update,
                    )
                    .map(Some)
            })
            .await
            .map_err(|error| error.to_string())
            .and_then(|result| result);

            let _ = this.update(cx, |app, cx| {
                match result {
                    Ok(Some(result)) => {
                        app.runtime_trace(
                            "help",
                            &format!(
                                "export_diagnostics success path={} bytes={}",
                                result.path, result.bytes
                            ),
                        );
                        app.status_message = format!(
                            "diagnostics exported: {} ({} bytes)",
                            result.path, result.bytes
                        );
                    }
                    Ok(None) => {
                        app.status_message = "diagnostics export canceled".to_string();
                    }
                    Err(error) => {
                        app.runtime_trace(
                            "help",
                            &format!("export_diagnostics failed error={error}"),
                        );
                        app.status_message = format!("failed to export diagnostics: {error}");
                    }
                }
                app.invalidate_status_bar(cx);
            });
        })
        .detach();
    }
}

fn about_icon_mark() -> impl IntoElement {
    div()
        .size(px(96.0))
        .rounded(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .child(
            img("icons/icon.png")
                .size(px(96.0))
                .object_fit(ObjectFit::Contain),
        )
}

fn about_action_row(locale: &str, cx: &mut Context<CoduxApp>) -> impl IntoElement {
    let tr = |key: &str, fallback: &str| translate(locale, key, fallback);
    div()
        .mt(px(24.0))
        .flex()
        .flex_wrap()
        .justify_center()
        .gap(px(8.0))
        .child(about_button(
            "about-agreement",
            tr("about.agreement", "Agreement"),
            HeroIconName::DocumentText,
            cx,
            |app, _event, _window, cx| app.open_user_agreement(cx),
        ))
        .child(about_button(
            "about-website",
            tr("about.website", "Website"),
            HeroIconName::ArrowTopRightOnSquare,
            cx,
            |app, _event, _window, cx| app.open_codux_website(cx),
        ))
        .child(about_button(
            "about-check-updates",
            tr("about.updates", "Check for Updates"),
            HeroIconName::ArrowPath,
            cx,
            |app, _event, window, cx| app.open_update_dialog_window(window, cx),
        ))
}

fn about_button(
    id: &'static str,
    label: String,
    icon: HeroIconName,
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
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(cx.theme().secondary_foreground)
                .child(Icon::new(icon).size_3())
                .child(label),
        )
}

fn update_dialog_content(app: &CoduxApp, language: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    let phase = app.update_dialog_phase;
    if matches!(phase, UpdateDialogPhase::Available) {
        return div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .w_full()
            .child(update_dialog_body(app, language, cx))
            .into_any_element();
    }

    let title = update_dialog_title(phase, language);
    let subtitle = update_dialog_subtitle(app, language);
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .flex_col()
        .w_full()
        .items_start()
        .justify_start()
        .gap(px(18.0))
        .pt(px(18.0))
        .child(
            div()
                .flex()
                .w_full()
                .items_center()
                .gap(px(12.0))
                .child(update_dialog_icon(phase, cx))
                .child(
                    div()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.0))
                        .child(
                            div()
                                .text_size(rems(0.875))
                                .line_height(rems(1.25))
                                .text_color(color(theme::TEXT))
                                .child(title),
                        )
                        .when(!subtitle.is_empty(), |this| {
                            this.child(
                                div()
                                    .text_size(rems(0.75))
                                    .line_height(rems(1.0))
                                    .text_color(color(theme::TEXT_MUTED))
                                    .child(subtitle),
                            )
                        }),
                ),
        )
        .child(update_dialog_body(app, language, cx))
        .into_any_element()
}

fn update_dialog_icon(phase: UpdateDialogPhase, cx: &mut Context<CoduxApp>) -> AnyElement {
    let busy = matches!(
        phase,
        UpdateDialogPhase::Checking | UpdateDialogPhase::Downloading
    );
    div()
        .size(px(36.0))
        .flex_shrink_0()
        .rounded(px(9.0))
        .flex()
        .items_center()
        .justify_center()
        .bg(if matches!(phase, UpdateDialogPhase::Error) {
            color(0x3A1E22)
        } else if matches!(
            phase,
            UpdateDialogPhase::Latest | UpdateDialogPhase::Finished
        ) {
            color(0x173224)
        } else {
            color(0x1C2C48)
        })
        .child(if busy {
            Spinner::new().small().into_any_element()
        } else {
            Icon::new(if matches!(phase, UpdateDialogPhase::Error) {
                HeroIconName::ExclamationTriangle
            } else if matches!(
                phase,
                UpdateDialogPhase::Latest | UpdateDialogPhase::Finished
            ) {
                HeroIconName::CheckCircle
            } else {
                HeroIconName::ArrowPath
            })
            .size_4()
            .text_color(if matches!(phase, UpdateDialogPhase::Error) {
                color(0xF87171)
            } else if matches!(
                phase,
                UpdateDialogPhase::Latest | UpdateDialogPhase::Finished
            ) {
                color(0x34D399)
            } else {
                cx.theme().accent
            })
            .into_any_element()
        })
        .into_any_element()
}

fn update_dialog_body(app: &CoduxApp, language: &str, _cx: &mut Context<CoduxApp>) -> AnyElement {
    let body = div().w_full().min_h_0().flex().flex_col().gap(px(10.0));
    match app.update_dialog_phase {
        UpdateDialogPhase::Available => body
            .child(
                div()
                    .mt(px(2.0))
                    .text_size(rems(0.875))
                    .line_height(rems(1.25))
                    .text_color(color(theme::TEXT))
                    .child(update_dialog_available_title(app, language)),
            )
            .child(
                div()
                    .flex_1()
                    .min_h(px(170.0))
                    .overflow_y_scrollbar()
                    .rounded(px(6.0))
                    .border_1()
                    .border_color(color(theme::BORDER_SOFT))
                    .bg(color(theme::BG_PANEL))
                    .p(px(12.0))
                    .text_size(rems(0.875))
                    .line_height(rems(1.375))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(
                        app.update_dialog_status
                            .as_ref()
                            .and_then(|status| status.notes.clone())
                            .filter(|notes| !notes.trim().is_empty())
                            .unwrap_or_else(|| {
                                translate(
                                    language,
                                    "update.release_notes.empty",
                                    "No release notes were provided for this update.",
                                )
                            }),
                    ),
            )
            .into_any_element(),
        UpdateDialogPhase::Downloading => body
            .child(update_progress_view(app.update_dialog_progress.as_ref(), language))
            .into_any_element(),
        UpdateDialogPhase::Finished => body
            .child(
                div()
                    .text_size(rems(0.875))
                    .line_height(rems(1.375))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(
                        app.update_dialog_result
                            .as_ref()
                            .map(|result| result.message.clone())
                            .unwrap_or_else(|| {
                                translate(
                                    language,
                                    "update.installed.message",
                                    "The update was downloaded. Restart Codux to finish applying it.",
                                )
                            }),
                    ),
            )
            .child(update_progress_view(app.update_dialog_progress.as_ref(), language))
            .into_any_element(),
        UpdateDialogPhase::Error => body
            .mt(px(10.0))
            .child(
                div()
                    .w_full()
                    .text_size(rems(0.875))
                    .line_height(rems(1.375))
                    .text_color(color(0xF87171))
                    .child(app.update_dialog_error.clone().unwrap_or_else(|| {
                        translate(
                            language,
                            "update.error.message",
                            "Please check your network connection and try again.",
                        )
                    })),
            )
            .into_any_element(),
        UpdateDialogPhase::Latest => body.hidden().into_any_element(),
        UpdateDialogPhase::NotConfigured => body
            .mt(px(10.0))
            .child(
                div()
                    .w_full()
                    .text_size(rems(0.875))
                    .line_height(rems(1.375))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(
                        app.update_dialog_status
                            .as_ref()
                            .map(|status| status.message.clone())
                            .unwrap_or_else(|| {
                                translate(
                                    language,
                                    "update.not_configured.preview",
                                    "Update channel is not configured.",
                                )
                            }),
                    ),
            )
            .into_any_element(),
        UpdateDialogPhase::Checking => body
            .mt(px(10.0))
            .child(
                div()
                    .w_full()
                    .text_size(rems(0.875))
                    .line_height(rems(1.375))
                    .text_color(color(theme::TEXT_MUTED))
                    .child(translate(
                        language,
                        "update.checking",
                        "Checking for updates...",
                    )),
            )
            .into_any_element(),
    }
}

fn update_dialog_footer(app: &CoduxApp, language: &str, cx: &mut Context<CoduxApp>) -> AnyElement {
    let phase = app.update_dialog_phase;
    let busy = matches!(
        phase,
        UpdateDialogPhase::Checking | UpdateDialogPhase::Downloading
    );
    let mut footer = div().flex().items_center().justify_end().gap(px(8.0));
    if !busy {
        let cancel_label = if matches!(phase, UpdateDialogPhase::Finished) {
            translate(language, "common.later", "Later")
        } else {
            translate(language, "common.cancel", "Cancel")
        };
        footer = footer.child(update_dialog_close_button(
            "update-dialog-cancel",
            cancel_label,
            cx,
        ));
    }
    match phase {
        UpdateDialogPhase::Available => {
            footer = footer.child(update_dialog_button(
                "update-dialog-download",
                translate(language, "common.update", "Update"),
                true,
                cx,
                |app, _event, window, cx| {
                    let Some(status) = app.update_dialog_status.clone() else {
                        app.update_dialog_error =
                            Some("No update status is available.".to_string());
                        app.update_dialog_phase = UpdateDialogPhase::Error;
                        cx.notify();
                        return;
                    };
                    let app_entity = cx.entity();
                    window.remove_window();
                    window.defer(cx, move |_window, cx| {
                        app_entity.update(cx, |app, cx| {
                            app.open_update_download_window(status, cx);
                        });
                    });
                },
            ));
        }
        UpdateDialogPhase::Finished => {
            footer = footer.child(update_dialog_button(
                "update-dialog-restart",
                translate(language, "common.restart_now", "Restart Now"),
                true,
                cx,
                |app, _event, _window, cx| app.request_restart(cx),
            ));
        }
        UpdateDialogPhase::Error | UpdateDialogPhase::NotConfigured => {
            footer = footer.child(update_dialog_button(
                "update-dialog-retry",
                translate(language, "about.updates", "Check for Updates"),
                false,
                cx,
                |app, _event, window, cx| {
                    resize_update_dialog_window(window, UpdateDialogPhase::Checking);
                    app.check_update_in_dialog(cx);
                },
            ));
        }
        UpdateDialogPhase::Latest
        | UpdateDialogPhase::Checking
        | UpdateDialogPhase::Downloading => {}
    }
    div()
        .h(px(54.0))
        .flex_shrink_0()
        .border_t_1()
        .border_color(color(theme::BORDER_SOFT))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_end()
        .gap(px(10.0))
        .child(footer)
        .into_any_element()
}

fn resize_update_dialog_window(window: &mut Window, phase: UpdateDialogPhase) {
    let height = match phase {
        UpdateDialogPhase::Available => UPDATE_DIALOG_AVAILABLE_HEIGHT,
        UpdateDialogPhase::Downloading | UpdateDialogPhase::Finished => {
            UPDATE_DIALOG_PROGRESS_HEIGHT
        }
        _ => UPDATE_DIALOG_DEFAULT_HEIGHT,
    };
    window.resize(size(px(UPDATE_DIALOG_WIDTH), px(height)));
}

fn update_dialog_button_label(
    label: impl Into<SharedString>,
    text_color: gpui::Hsla,
) -> impl IntoElement {
    div()
        .text_size(rems(0.75))
        .line_height(rems(1.0))
        .text_color(text_color)
        .child(label.into())
}

fn update_dialog_button(
    id: &'static str,
    label: String,
    primary: bool,
    cx: &mut Context<CoduxApp>,
    on_click: impl Fn(&mut CoduxApp, &gpui::ClickEvent, &mut Window, &mut Context<CoduxApp>) + 'static,
) -> Button {
    let text_color = if primary {
        cx.theme().primary_foreground
    } else {
        cx.theme().secondary_foreground
    };
    let button = Button::new(id)
        .compact()
        .text_color(text_color)
        .child(update_dialog_button_label(label, text_color))
        .on_click(cx.listener(on_click));
    if primary {
        button.primary()
    } else {
        button.secondary()
    }
}

fn update_dialog_close_button(
    id: &'static str,
    label: String,
    cx: &mut Context<CoduxApp>,
) -> Button {
    let text_color = cx.theme().secondary_foreground;
    Button::new(id)
        .compact()
        .secondary()
        .text_color(text_color)
        .child(update_dialog_button_label(label, text_color))
        .on_click(|_, window, _| window.remove_window())
}

fn update_progress_view(
    progress: Option<&UpdateInstallProgressEvent>,
    language: &str,
) -> AnyElement {
    let downloaded = progress
        .map(|progress| progress.downloaded_bytes)
        .unwrap_or(0);
    let total = progress.and_then(|progress| progress.total_bytes);
    let ratio = total
        .filter(|total| *total > 0)
        .map(|total| (downloaded as f32 / total as f32).clamp(0.0, 1.0));
    div()
        .w_full()
        .flex()
        .flex_col()
        .gap(px(8.0))
        .child(
            div()
                .w_full()
                .text_size(rems(0.875))
                .line_height(rems(1.375))
                .text_color(color(theme::TEXT_MUTED))
                .child(translate(
                    language,
                    "update.progress.downloading",
                    "Downloading update...",
                )),
        )
        .child(
            div()
                .w_full()
                .h(px(6.0))
                .rounded(px(999.0))
                .overflow_hidden()
                .bg(color(0x303442))
                .child(
                    div()
                        .h(px(6.0))
                        .w(relative(ratio.unwrap_or(0.35)))
                        .rounded(px(999.0))
                        .bg(color(theme::ACCENT)),
                ),
        )
        .child(
            div()
                .w_full()
                .text_size(rems(0.875))
                .line_height(rems(1.375))
                .text_color(color(theme::TEXT_DIM))
                .child(match total {
                    Some(total) => {
                        format!("{} / {}", format_bytes(downloaded), format_bytes(total))
                    }
                    None => format_bytes(downloaded),
                }),
        )
        .into_any_element()
}

fn update_dialog_title(phase: UpdateDialogPhase, language: &str) -> String {
    match phase {
        UpdateDialogPhase::Available => {
            translate(language, "update.available.title", "Update Available")
        }
        UpdateDialogPhase::Latest => translate(language, "update.latest.title", "Up to Date"),
        UpdateDialogPhase::NotConfigured => translate(
            language,
            "update.not_configured.title",
            "Updates Not Configured",
        ),
        UpdateDialogPhase::Downloading => {
            translate(language, "update.progress.title", "Installing Update")
        }
        UpdateDialogPhase::Finished => {
            translate(language, "update.installed.title", "Update Ready")
        }
        UpdateDialogPhase::Error => translate(
            language,
            "update.error.title",
            "Unable to Check for Updates",
        ),
        UpdateDialogPhase::Checking => translate(language, "about.updates", "Check for Updates"),
    }
}

fn update_dialog_subtitle(app: &CoduxApp, language: &str) -> String {
    if app.update_dialog_phase == UpdateDialogPhase::Latest {
        let current_version = app
            .update_dialog_status
            .as_ref()
            .map(|status| status.current_version.as_str())
            .unwrap_or(env!("CARGO_PKG_VERSION"));
        return translate(language, "update.progress.version_format", "Version v%@")
            .replace("%@", current_version);
    }
    if let Some(status) = &app.update_dialog_status {
        if app.update_dialog_phase == UpdateDialogPhase::Available {
            return translate(
                language,
                "update.version.summary_format",
                "Current v%@ · Latest v%@",
            )
            .replacen("%@", &status.current_version, 1)
            .replacen(
                "%@",
                status
                    .latest_version
                    .as_deref()
                    .unwrap_or(&status.current_version),
                1,
            );
        }
    }
    if let Some(progress) = &app.update_dialog_progress
        && let Some(version) = &progress.version
    {
        return translate(language, "update.progress.version_format", "Version v%@")
            .replace("%@", version);
    }
    String::new()
}

fn update_dialog_available_title(app: &CoduxApp, language: &str) -> String {
    let Some(status) = &app.update_dialog_status else {
        return translate(language, "update.available.title", "Update Available");
    };
    translate(
        language,
        "update.available.version_title_format",
        "New version v%@: ",
    )
    .replace("%@", status.latest_version.as_deref().unwrap_or(""))
}

fn format_bytes(bytes: u64) -> String {
    let units = ["B", "KB", "MB", "GB"];
    let mut value = bytes as f64;
    let mut index = 0;
    while value >= 1024.0 && index < units.len() - 1 {
        value /= 1024.0;
        index += 1;
    }
    if index == 0 || value >= 10.0 {
        format!("{value:.0} {}", units[index])
    } else {
        format!("{value:.1} {}", units[index])
    }
}

fn timestamp_slug() -> String {
    let seconds = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or(0);
    seconds.to_string()
}
