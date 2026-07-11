use super::options::*;
use super::widgets::*;
use super::*;

pub(super) fn settings_developer_pane(
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

pub(super) struct RuntimeToolBlockInput<'a> {
    pub(super) label: String,
    pub(super) tool_key: &'static str,
    pub(super) model_key: &'static str,
    pub(super) permission: &'a str,
    pub(super) model: &'a str,
    pub(super) placeholder: &'static str,
    pub(super) include_permission: bool,
    pub(super) include_codex_effort: bool,
    pub(super) codex_effort: &'a str,
    pub(super) language: &'a str,
}

pub(super) fn settings_runtime_tool_block(
    input: RuntimeToolBlockInput<'_>,
    window: &mut Window,
    cx: &mut Context<CoduxApp>,
) -> AnyElement {
    let RuntimeToolBlockInput {
        label,
        tool_key,
        model_key,
        permission,
        model,
        placeholder,
        include_permission,
        include_codex_effort,
        codex_effort,
        language,
    } = input;
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
