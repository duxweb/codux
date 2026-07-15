use super::widgets::*;
use super::*;
use gpui::FontWeight;

pub(super) struct SettingsWslPaneInput<'a> {
    pub(super) settings: &'a SettingsSummary,
    pub(super) catalog: Option<&'a codux_runtime::wsl::WslDistributionCatalog>,
    pub(super) loading: bool,
    pub(super) selected_distribution: &'a str,
    pub(super) progress: Option<&'a codux_runtime::wsl::WslInstallProgress>,
    pub(super) error: Option<&'a str>,
}

pub(super) fn settings_wsl_pane(
    input: SettingsWslPaneInput<'_>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let SettingsWslPaneInput {
        settings,
        catalog,
        loading,
        selected_distribution,
        progress,
        error,
    } = input;
    let language = settings.language.as_str();
    let mut sections =
        vec![
            settings_card(
                None,
                None,
                vec![settings_row(
            settings_text(language, "settings.wsl.enabled", "Enable WSL Integration"),
            Some(settings_text(
                language,
                "settings.wsl.enabled.description",
                "Use installed WSL distributions for projects, files, Git, and terminals.",
            )),
            settings_toggle_state(
                "settings-wsl-enabled",
                settings.wsl_enabled,
                progress.is_some(),
                cx,
                |app, window, cx| app.toggle_wsl_enabled(window, cx),
            ),
        )
        .into_any_element()],
                cx,
            )
            .into_any_element(),
        ];

    if settings.wsl_enabled {
        sections.extend(wsl_distribution_sections(
            language,
            catalog,
            loading,
            selected_distribution,
            progress,
            window,
            cx,
        ));
    }
    if let Some(error) = error {
        let error = wsl_error_text(language, error);
        sections.push(wsl_error_notice(&error, cx));
    }
    settings_form(sections).into_any_element()
}

fn wsl_distribution_sections(
    language: &str,
    catalog: Option<&codux_runtime::wsl::WslDistributionCatalog>,
    loading: bool,
    selected_distribution: &str,
    progress: Option<&codux_runtime::wsl::WslInstallProgress>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> Vec<AnyElement> {
    if loading {
        return vec![
            settings_card(
                Some(settings_text(
                    language,
                    "settings.wsl.section.distributions",
                    "WSL Distributions",
                )),
                None,
                vec![wsl_message(
                    settings_text(
                        language,
                        "settings.wsl.loading",
                        "Detecting WSL distributions…",
                    ),
                    true,
                    cx,
                )],
                cx,
            )
            .into_any_element(),
        ];
    }
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let installed = catalog
        .distributions
        .iter()
        .filter(|status| status.distribution_installed)
        .collect::<Vec<_>>();
    let available = catalog
        .distributions
        .iter()
        .filter(|status| !status.distribution_installed)
        .collect::<Vec<_>>();
    let mut sections = Vec::new();
    let installed_rows = if installed.is_empty() {
        vec![wsl_message(
            settings_text(
                language,
                "settings.wsl.installed.empty",
                "No WSL distributions are installed yet.",
            ),
            false,
            cx,
        )]
    } else {
        installed
            .into_iter()
            .map(|status| wsl_installed_distribution_row(language, status, progress, cx))
            .collect()
    };
    sections.push(
        settings_card(
            Some(settings_text(
                language,
                "settings.wsl.section.installed",
                "Installed Distributions",
            )),
            catalog.installed_error.clone(),
            installed_rows,
            cx,
        )
        .into_any_element(),
    );

    let install_rows = if available.is_empty() {
        vec![wsl_message(
            settings_text(
                language,
                "settings.wsl.available.empty",
                "No additional WSL distributions are available.",
            ),
            false,
            cx,
        )]
    } else {
        vec![wsl_distribution_installer(
            language,
            &available,
            selected_distribution,
            progress,
            window,
            cx,
        )]
    };
    sections.push(
        settings_card(
            Some(settings_text(
                language,
                "settings.wsl.section.install",
                "Install a Distribution",
            )),
            catalog.online_error.clone(),
            install_rows,
            cx,
        )
        .into_any_element(),
    );
    sections
}

fn wsl_installed_distribution_row(
    language: &str,
    status: &codux_runtime::wsl::WslDistributionStatus,
    progress: Option<&codux_runtime::wsl::WslInstallProgress>,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let distribution = status.distribution.clone();
    let current_progress = progress.filter(|progress| progress.distribution == distribution);
    let busy = progress.is_some();
    let runtime_compatible = status
        .runtime
        .as_ref()
        .is_some_and(codux_runtime::wsl::WslRuntimeInfo::is_compatible);
    let description = wsl_runtime_description(language, status);
    div()
        .min_h(px(76.0))
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
                .items_center()
                .gap(px(12.0))
                .child(wsl_icon(cx))
                .child(
                    div()
                        .min_w_0()
                        .flex_1()
                        .flex()
                        .flex_col()
                        .child(wsl_distribution_name(status))
                        .when_some(current_progress, |this, progress| {
                            this.child(wsl_install_progress(language, progress, cx))
                        })
                        .when(current_progress.is_none(), |this| {
                            this.child(
                                div()
                                    .mt(px(3.0))
                                    .text_size(rems(0.75))
                                    .line_height(rems(1.0))
                                    .text_color(color(theme::TEXT_DIM))
                                    .child(description),
                            )
                        }),
                ),
        )
        .child(div().flex_none().child(settings_status_tag(
            settings_text(
                language,
                if runtime_compatible {
                    "settings.wsl.status.ready"
                } else if status.runtime.is_some() {
                    "settings.wsl.status.runtime_incompatible"
                } else {
                    "settings.wsl.status.runtime_missing"
                },
                if runtime_compatible {
                    "Ready"
                } else if status.runtime.is_some() {
                    "Update Required"
                } else {
                    "Runtime Missing"
                },
            ),
            if runtime_compatible {
                theme::GREEN
            } else {
                theme::ORANGE
            },
        )))
        .child(div().flex_none().child(settings_small_button_state(
            format!("settings-wsl-runtime-{}", status.distribution),
            settings_text(
                language,
                if status.runtime.is_some() {
                    "settings.wsl.runtime.update"
                } else {
                    "settings.wsl.runtime.install"
                },
                if status.runtime.is_some() {
                    "Update Runtime"
                } else {
                    "Install Runtime"
                },
            ),
            current_progress.is_some(),
            busy,
            cx,
            move |app, _event, window, cx| {
                app.install_wsl_runtime(distribution.clone(), window, cx)
            },
        )))
        .into_any_element()
}

fn wsl_runtime_description(
    language: &str,
    status: &codux_runtime::wsl::WslDistributionStatus,
) -> String {
    let Some(runtime) = status.runtime.as_ref() else {
        return settings_text(
            language,
            "settings.wsl.runtime.missing",
            "Install Codux Runtime to use this distribution in Codux.",
        );
    };
    let version = runtime.version.clone().unwrap_or_else(|| {
        settings_text(
            language,
            "settings.wsl.runtime.version.unknown",
            "Unknown version",
        )
    });
    let protocol = runtime
        .protocol_version
        .map(|version| version.to_string())
        .unwrap_or_else(|| {
            settings_text(language, "settings.wsl.runtime.protocol.unknown", "Unknown")
        });
    if runtime.is_compatible() {
        settings_text(
            language,
            "settings.wsl.runtime.details",
            "Runtime %@ · Protocol %@",
        )
        .replacen("%@", &version, 1)
        .replacen("%@", &protocol, 1)
    } else {
        settings_text(
            language,
            "settings.wsl.runtime.details.incompatible",
            "Runtime %@ · Protocol %@ · Required %@",
        )
        .replacen("%@", &version, 1)
        .replacen("%@", &protocol, 1)
        .replacen("%@", &runtime.required_protocol_version.to_string(), 1)
    }
}

fn wsl_distribution_installer(
    language: &str,
    available: &[&codux_runtime::wsl::WslDistributionStatus],
    selected_distribution: &str,
    progress: Option<&codux_runtime::wsl::WslInstallProgress>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let options = available
        .iter()
        .map(|status| {
            CoduxSelectOption::new(
                status.distribution.clone(),
                SharedString::from(format!("{} · {}", status.display_name, status.distribution)),
            )
        })
        .collect();
    let progress_for_selection = progress.filter(|progress| {
        progress.distribution == selected_distribution
            && matches!(
                progress.operation,
                codux_runtime::wsl::WslInstallOperation::Distribution
                    | codux_runtime::wsl::WslInstallOperation::Runtime
            )
    });
    let distribution = selected_distribution.to_string();
    div()
        .min_h(px(82.0))
        .py(px(12.0))
        .flex()
        .items_center()
        .gap(px(18.0))
        .child(wsl_icon(cx))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .gap(px(8.0))
                .when_some(progress_for_selection, |this, progress| {
                    this.child(wsl_install_progress(language, progress, cx))
                })
                .when(progress_for_selection.is_none(), |this| {
                    this.child(codux_select(
                        CoduxSelectConfig {
                            id: "settings-select-wsl-distribution".to_string(),
                            value: selected_distribution.to_string(),
                            options,
                            placeholder: settings_text(language, "common.choose", "Choose").into(),
                            width: relative(1.0).into(),
                            menu_width: px(
                                (window.viewport_size().width.as_f32() - 420.0)
                                    .clamp(360.0, 720.0),
                            ),
                            disabled: progress.is_some(),
                        },
                        cx,
                        |app, value, window, cx| {
                            app.set_wsl_selected_distribution(value, window, cx)
                        },
                    ))
                    .child(
                        div()
                            .text_size(rems(0.75))
                            .line_height(rems(1.0))
                            .text_color(color(theme::TEXT_DIM))
                            .child(settings_text(
                                language,
                                "settings.wsl.install.description",
                                "Downloads the selected distribution, then installs Codux Runtime automatically.",
                            )),
                    )
                }),
        )
        .child(
            div().flex_none().child(settings_small_button_state(
                "settings-wsl-install-distribution",
                settings_text(language, "settings.wsl.distribution.install", "Install"),
                progress_for_selection.is_some(),
                progress.is_some() || selected_distribution.is_empty(),
                cx,
                move |app, _event, window, cx| {
                    app.install_wsl_distribution(distribution.clone(), window, cx)
                },
            )),
        )
        .into_any_element()
}

fn wsl_install_progress(
    language: &str,
    progress: &codux_runtime::wsl::WslInstallProgress,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let stage = settings_text(
        language,
        match progress.operation {
            codux_runtime::wsl::WslInstallOperation::Distribution => {
                "settings.wsl.progress.distribution"
            }
            codux_runtime::wsl::WslInstallOperation::Runtime => "settings.wsl.progress.runtime",
        },
        match progress.operation {
            codux_runtime::wsl::WslInstallOperation::Distribution => "Installing WSL distribution",
            codux_runtime::wsl::WslInstallOperation::Runtime => "Installing Codux Runtime",
        },
    );
    div()
        .mt(px(6.0))
        .w_full()
        .max_w(px(520.0))
        .flex()
        .flex_col()
        .gap(px(5.0))
        .child(
            div()
                .flex()
                .items_center()
                .justify_between()
                .gap(px(12.0))
                .text_size(rems(0.75))
                .line_height(rems(1.0))
                .text_color(color(theme::TEXT_DIM))
                .child(
                    if progress.percent.is_some() || progress.message.trim().is_empty() {
                        stage
                    } else {
                        format!("{stage} · {}", progress.message.trim())
                    },
                )
                .when_some(progress.percent, |this, percent| {
                    this.child(format!("{percent}%"))
                }),
        )
        .child(
            Progress::new(format!("wsl-install-progress-{}", progress.distribution))
                .value(progress.percent.unwrap_or(0) as f32)
                .loading(progress.percent.is_none())
                .with_size(gpui_component::Size::Small)
                .color(cx.theme().primary),
        )
        .into_any_element()
}

fn wsl_distribution_name(status: &codux_runtime::wsl::WslDistributionStatus) -> AnyElement {
    div()
        .min_w_0()
        .flex_1()
        .truncate()
        .whitespace_nowrap()
        .text_size(rems(0.875))
        .line_height(rems(1.125))
        .font_weight(FontWeight::MEDIUM)
        .text_color(color(theme::TEXT))
        .child(if status.display_name == status.distribution {
            status.display_name.clone()
        } else {
            format!("{} · {}", status.display_name, status.distribution)
        })
        .into_any_element()
}

fn wsl_icon(_cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .size(px(36.0))
        .rounded(px(6.0))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .bg(color(theme::ACCENT).opacity(0.12))
        .child(
            Icon::new(HeroIconName::CommandLine)
                .size_4()
                .text_color(color(theme::ACCENT)),
        )
        .into_any_element()
}

fn wsl_message(message: String, loading: bool, _cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .min_h(px(64.0))
        .flex()
        .items_center()
        .gap(px(10.0))
        .text_size(rems(0.8125))
        .text_color(color(theme::TEXT_DIM))
        .when(loading, |this| this.child(Spinner::new().small()))
        .child(message)
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_description_shows_installed_and_required_protocols() {
        let status = codux_runtime::wsl::WslDistributionStatus {
            distribution: "Ubuntu".to_string(),
            display_name: "Ubuntu".to_string(),
            distribution_installed: true,
            runtime: Some(codux_runtime::wsl::WslRuntimeInfo {
                version: Some("2.0.0".to_string()),
                protocol_version: Some(2),
                required_protocol_version: 3,
            }),
        };

        assert_eq!(
            wsl_runtime_description("english", &status),
            "Runtime 2.0.0 · Protocol 2 · Required 3"
        );
    }
}

fn wsl_error_notice(error: &str, _cx: &mut Context<CoduxApp>) -> AnyElement {
    div()
        .w_full()
        .rounded(px(6.0))
        .px(px(14.0))
        .py(px(12.0))
        .bg(color(theme::ORANGE).opacity(0.1))
        .text_size(rems(0.8125))
        .line_height(rems(1.125))
        .text_color(color(theme::ORANGE))
        .child(error.to_string())
        .into_any_element()
}

fn wsl_error_text(language: &str, error: &str) -> String {
    match error {
        codux_runtime::wsl::WSL_RUNTIME_PROTOCOL_MISMATCH_ERROR => settings_text(
            language,
            "settings.wsl.error.protocol_mismatch",
            "The latest Codux Runtime is not compatible with this Codux version.",
        ),
        _ => error.to_string(),
    }
}
