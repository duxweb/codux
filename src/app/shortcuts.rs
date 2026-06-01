use std::collections::HashMap;

pub(in crate::app) fn shortcut_display_from_keystroke(keystroke: &gpui::Keystroke) -> String {
    let mut parts = Vec::new();
    if keystroke.modifiers.platform {
        parts.push(
            if cfg!(target_os = "macos") {
                "⌘"
            } else {
                "Ctrl+"
            }
            .to_string(),
        );
    }
    if keystroke.modifiers.control {
        parts.push(
            if cfg!(target_os = "macos") {
                "⌃"
            } else {
                "Ctrl+"
            }
            .to_string(),
        );
    }
    if keystroke.modifiers.alt {
        parts.push(
            if cfg!(target_os = "macos") {
                "⌥"
            } else {
                "Alt+"
            }
            .to_string(),
        );
    }
    if keystroke.modifiers.shift {
        parts.push(
            if cfg!(target_os = "macos") {
                "⇧"
            } else {
                "Shift+"
            }
            .to_string(),
        );
    }
    let key = if keystroke.key.chars().count() == 1 {
        keystroke.key.to_uppercase()
    } else {
        keystroke.key.clone()
    };
    parts.push(key);
    parts.join("")
}

fn default_shortcut_display(shortcut_id: &str) -> Option<&'static str> {
    let primary = if cfg!(target_os = "macos") {
        "⌘"
    } else {
        "Ctrl+"
    };
    match (shortcut_id, primary) {
        ("view.terminal", "⌘") => Some("⌘⌥1"),
        ("view.files", "⌘") => Some("⌘⌥2"),
        ("view.review", "⌘") => Some("⌘⌥3"),
        ("project.create", "⌘") => Some("⌘N"),
        ("project.open_folder", "⌘") => Some("⌘O"),
        ("settings.open", "⌘") => Some("⌘,"),
        ("task.create", "⌘") => Some("⌘⇧N"),
        ("editor.save", "⌘") => Some("⌘S"),
        ("editor.search", "⌘") => Some("⌘F"),
        ("close.active", "⌘") => Some("⌘W"),
        ("sidebar.projects.toggle", "⌘") => Some("⌘⌥P"),
        ("sidebar.tasks.toggle", "⌘") => Some("⌘⌥T"),
        ("assistant.git.open", "⌘") => Some("⌘⇧G"),
        ("panel.git", "⌘") => Some("⌘⇧G"),
        ("assistant.files.open", "⌘") => Some("⌘⇧F"),
        ("assistant.ai.open", "⌘") => Some("⌘⇧A"),
        ("panel.ai", "⌘") => Some("⌘⇧A"),
        ("assistant.ssh.open", "⌘") => Some("⌘⇧S"),
        ("terminal.split", "⌘") => Some("⌘⇧\\"),
        ("terminal.split.create", "⌘") => Some("⌘⇧\\"),
        ("terminal.tab", "⌘") => Some("⌘⇧T"),
        ("terminal.tab.create", "⌘") => Some("⌘⇧T"),
        ("view.terminal", _) => Some("Ctrl+Alt+1"),
        ("view.files", _) => Some("Ctrl+Alt+2"),
        ("view.review", _) => Some("Ctrl+Alt+3"),
        ("project.create", _) => Some("Ctrl+N"),
        ("project.open_folder", _) => Some("Ctrl+O"),
        ("settings.open", _) => Some("Ctrl+,"),
        ("task.create", _) => Some("Ctrl+Shift+N"),
        ("editor.save", _) => Some("Ctrl+S"),
        ("editor.search", _) => Some("Ctrl+F"),
        ("close.active", _) => Some("Ctrl+W"),
        ("sidebar.projects.toggle", _) => Some("Ctrl+Alt+P"),
        ("sidebar.tasks.toggle", _) => Some("Ctrl+Alt+T"),
        ("assistant.git.open", _) => Some("Ctrl+Shift+G"),
        ("panel.git", _) => Some("Ctrl+Shift+G"),
        ("assistant.files.open", _) => Some("Ctrl+Shift+F"),
        ("assistant.ai.open", _) => Some("Ctrl+Shift+A"),
        ("panel.ai", _) => Some("Ctrl+Shift+A"),
        ("assistant.ssh.open", _) => Some("Ctrl+Shift+S"),
        ("terminal.split", _) => Some("Ctrl+Shift+\\"),
        ("terminal.split.create", _) => Some("Ctrl+Shift+\\"),
        ("terminal.tab", _) => Some("Ctrl+Shift+T"),
        ("terminal.tab.create", _) => Some("Ctrl+Shift+T"),
        _ => None,
    }
}

pub(in crate::app) fn normalized_shortcut_text(value: &str) -> Option<String> {
    let mut rest = value.trim().to_lowercase();
    if rest.is_empty() {
        return None;
    }

    let platform = rest.contains("command") || rest.contains("cmd") || rest.contains('⌘');
    let control = rest.contains("control") || rest.contains("ctrl") || rest.contains('⌃');
    let alt = rest.contains("option") || rest.contains("alt") || rest.contains('⌥');
    let shift = rest.contains("shift") || rest.contains('⇧');

    for token in [
        "command", "cmd", "control", "ctrl", "option", "alt", "shift", "⌘", "⌃", "⌥", "⇧", "+",
    ] {
        rest = rest.replace(token, "");
    }
    rest.retain(|character| !character.is_whitespace());
    if rest.is_empty() {
        return None;
    }

    let key = if rest.chars().count() == 1 {
        rest.to_uppercase()
    } else {
        rest
    };
    Some(format!(
        "{}{}{}{}{}",
        if platform { "Meta+" } else { "" },
        if control { "Ctrl+" } else { "" },
        if alt { "Alt+" } else { "" },
        if shift { "Shift+" } else { "" },
        key
    ))
}

fn shortcut_value_matches(configured: &str, actual: &str) -> bool {
    let Some(actual) = normalized_shortcut_text(actual) else {
        return false;
    };
    configured
        .split('/')
        .filter_map(normalized_shortcut_text)
        .any(|candidate| candidate == actual)
}

pub(in crate::app) fn shortcut_matches(
    shortcuts: &HashMap<String, String>,
    shortcut_id: &str,
    actual: &str,
) -> bool {
    shortcuts
        .get(shortcut_id)
        .filter(|value| !value.trim().is_empty())
        .map(|value| shortcut_value_matches(value, actual))
        .unwrap_or_else(|| {
            default_shortcut_display(shortcut_id)
                .map(|value| shortcut_value_matches(value, actual))
                .unwrap_or(false)
        })
}
