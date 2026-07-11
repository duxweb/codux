use super::developer::{RuntimeToolBlockInput, settings_runtime_tool_block};
use super::options::*;
use super::widgets::*;
use super::*;

pub(super) fn settings_ai_pane(
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
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "Codex"),
                tool_key: "codex",
                model_key: "codexModel",
                permission: &permissions.codex,
                model: &permissions.codex_model,
                placeholder: "gpt-5.5",
                include_permission: true,
                include_codex_effort: true,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "Claude Code"),
                tool_key: "claudeCode",
                model_key: "claudeCodeModel",
                permission: &permissions.claude_code,
                model: &permissions.claude_code_model,
                placeholder: "claude-sonnet-4.5",
                include_permission: true,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "Agy"),
                tool_key: "agy",
                model_key: "agyModel",
                permission: &permissions.agy,
                model: &permissions.agy_model,
                placeholder: "gemini-2.5-pro",
                include_permission: true,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "OpenCode"),
                tool_key: "opencode",
                model_key: "opencodeModel",
                permission: &permissions.opencode,
                model: &permissions.opencode_model,
                placeholder: "gpt-5.5",
                include_permission: true,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "Kiro"),
                tool_key: "kiro",
                model_key: "kiroModel",
                permission: &permissions.kiro,
                model: &permissions.kiro_model,
                placeholder: "auto",
                include_permission: false,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "CodeWhale"),
                tool_key: "codewhale",
                model_key: "codewhaleModel",
                permission: &permissions.codewhale,
                model: &permissions.codewhale_model,
                placeholder: "deepseek-chat",
                include_permission: true,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "Kimi Code"),
                tool_key: "kimi",
                model_key: "kimiModel",
                permission: &permissions.kimi,
                model: &permissions.kimi_model,
                placeholder: "kimi-k2",
                include_permission: false,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
            window,
            cx,
        ),
        settings_runtime_tool_block(
            RuntimeToolBlockInput {
                label: settings_text(
                    language,
                    "settings.ai.tool.configuration_format",
                    "%@ Configuration",
                )
                .replace("%@", "MiMo-Code"),
                tool_key: "mimo",
                model_key: "mimoModel",
                permission: &permissions.mimo,
                model: &permissions.mimo_model,
                placeholder: "kimi-k2",
                include_permission: true,
                include_codex_effort: false,
                codex_effort: &permissions.codex_effort,
                language,
            },
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
pub(super) fn settings_ai_provider_card(
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
