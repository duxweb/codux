use super::overlays::remote_add_dropdown;
use super::relay::settings_remote_relay_custom_fields;
use super::*;

pub(in crate::app::settings) struct SettingsRemotePaneInput<'a> {
    pub(in crate::app::settings) settings: &'a SettingsSummary,
    pub(in crate::app::settings) remote: &'a RemoteSummary,
    pub(in crate::app::settings) saved_hosts: &'a [codux_runtime::remote::SavedRemoteHost],
    pub(in crate::app::settings) link_states:
        &'a std::collections::HashMap<String, codux_runtime::remote::ControllerLinkState>,
    pub(in crate::app::settings) link_paths:
        &'a std::collections::HashMap<String, codux_runtime::remote::ControllerLinkPath>,
    pub(in crate::app::settings) language: &'a str,
    pub(in crate::app::settings) remote_reconnecting: bool,
    pub(in crate::app::settings) remote_pairing_creating: bool,
}

pub(in crate::app::settings) fn settings_remote_pane(
    input: SettingsRemotePaneInput<'_>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let SettingsRemotePaneInput {
        settings,
        remote,
        saved_hosts,
        link_states,
        link_paths,
        language,
        remote_reconnecting,
        remote_pairing_creating,
    } = input;
    let mut device_rows: Vec<AnyElement> = remote
        .device_list
        .iter()
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
                            settings_remote_relay_custom_fields(settings, window, cx, language)
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
                                window,
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

pub(super) fn remote_mobile_download_banner(
    language: &str,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
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
