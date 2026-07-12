use super::widgets::*;
use super::*;

pub(super) fn settings_shortcuts_pane(
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
pub(super) struct ShortcutDefinition {
    id: &'static str,
    label_key: &'static str,
    fallback: &'static str,
    default_value: &'static str,
}

pub(super) fn shortcut_definitions() -> Vec<ShortcutDefinition> {
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

pub(super) fn primary_static(primary: &str, key: &str) -> &'static str {
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
        _ => "",
    }
}

pub(super) fn shortcut_row(
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

#[cfg(test)]
mod tests {
    use super::primary_static;

    #[test]
    fn removed_terminal_tab_shortcut_has_no_default() {
        assert_eq!(primary_static("⌘", "Shift+T"), "");
        assert_eq!(primary_static("Ctrl+", "Shift+T"), "");
    }
}
