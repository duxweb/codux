fn sanitize_statistics_mode(value: &str) -> String {
    if value.trim() == "includingCache" {
        "includingCache"
    } else {
        "normalized"
    }
    .to_string()
}

fn sanitize_file_open_default(value: &str) -> String {
    if value.trim() == "preview" {
        "preview"
    } else {
        "edit"
    }
    .to_string()
}

fn sanitize_terminal_theme(value: &str) -> String {
    let normalized = normalize_appearance_name(value);
    terminal_theme_options()
        .iter()
        .copied()
        .find(|theme| normalize_appearance_name(theme) == normalized)
        .unwrap_or("Auto")
        .to_string()
}

fn terminal_theme_options() -> &'static [&'static str] {
    &[
        "Auto",
        "2017 Dark",
        "GitHub Dark",
        "One Dark Pro",
        "Dracula",
        "Atom One Dark",
        "Material Theme",
        "Ayu Dark",
        "Monokai Pro",
        "Winter is Coming Dark Blue",
        "Night Owl",
        "One Monokai",
        "Tokyo Night",
        "Palenight",
        "SynthWave '84",
        "Shades of Purple",
        "2017 Light",
        "PowerShell ISE",
        "GitHub Light",
        "Material Theme Lighter",
        "Ayu Light",
        "Monokai Pro Light",
        "Winter is Coming Light",
        "Night Owl Light",
        "Tokyo Night Light",
        "Atom One Light",
        "Noctis Hibernus",
        "Catppuccin Latte",
        "Gruvbox Light Medium",
        "Eva Light",
        "Spinel Light",
    ]
}

fn sanitize_terminal_font_family(value: &str) -> String {
    value
        .trim()
        .chars()
        .filter(|c| !c.is_control())
        .take(80)
        .collect()
}

fn sanitize_theme_color(value: &str) -> String {
    let normalized = normalize_appearance_name(value);
    let canonical = [
        "Blue", "Sky", "Cyan", "Teal", "Emerald", "Green", "Lime", "Amber", "Orange", "Red",
        "Rose", "Pink", "Fuchsia", "Purple", "Violet", "Indigo",
    ]
    .into_iter()
    .find(|label| normalize_appearance_name(label) == normalized);
    if let Some(label) = canonical {
        return label.to_string();
    }
    let alias = match normalized.as_str() {
        "burnt" => "Orange",
        "crimson" => "Red",
        "gold" => "Amber",
        "iris" => "Violet",
        "lavender" => "Violet",
        "moss" => "Emerald",
        "navy" => "Blue",
        "plum" => "Rose",
        "sage" => "Green",
        _ => "Blue",
    };
    alias.to_string()
}

fn sanitize_icon_style(value: &str) -> String {
    match value.trim() {
        "default" => "default",
        "cobalt" => "cobalt",
        "sunset" => "sunset",
        "forest" => "forest",
        _ => "default",
    }
    .to_string()
}

fn sanitize_tool_permission(value: &str) -> String {
    match value.trim() {
        "fullAccess" => "fullAccess",
        _ => "default",
    }
    .to_string()
}

fn sanitize_codex_effort(value: &str) -> String {
    match value.trim() {
        "none" => "none",
        "minimal" => "minimal",
        "low" => "low",
        "medium" => "medium",
        "high" => "high",
        "xhigh" => "xhigh",
        _ => "medium",
    }
    .to_string()
}

fn sanitize_git_commit_tone(value: &str) -> String {
    match value.trim() {
        "conventional" => "conventional",
        "concise" => "concise",
        "sentence" => "sentence",
        "changelog" => "changelog",
        _ => "conventional",
    }
    .to_string()
}

fn sanitize_git_commit_language(value: &str) -> String {
    match value.trim() {
        "application" => "application",
        "english" => "english",
        "simplifiedChinese" => "simplifiedChinese",
        "traditionalChinese" => "traditionalChinese",
        "japanese" => "japanese",
        "korean" => "korean",
        "french" => "french",
        "german" => "german",
        "spanish" => "spanish",
        "portugueseBrazil" => "portugueseBrazil",
        "russian" => "russian",
        _ => "application",
    }
    .to_string()
}

fn bounded_trimmed_chars(value: &str, limit: usize) -> usize {
    value.trim().chars().take(limit).count()
}

fn numeric_string(value: &str, default: i64, min: i64, max: i64) -> i64 {
    value
        .trim()
        .parse::<f64>()
        .ok()
        .filter(|value| value.is_finite())
        .map(|value| value.round() as i64)
        .unwrap_or(default)
        .clamp(min, max)
}

fn next_interval(current: &str, options: &[i64], default: i64) -> i64 {
    let current = numeric_string(current, default, 1, 86_400);
    options
        .iter()
        .position(|value| *value == current)
        .and_then(|index| options.get(index + 1).or_else(|| options.first()))
        .copied()
        .unwrap_or(default)
}

fn next_string_option<'a>(current: &str, options: &'a [&str], default: &'a str) -> &'a str {
    let normalized = normalize_appearance_name(current);
    options
        .iter()
        .position(|option| normalize_appearance_name(option) == normalized)
        .and_then(|index| options.get(index + 1).or_else(|| options.first()))
        .copied()
        .unwrap_or(default)
}

fn normalize_appearance_name(value: &str) -> String {
    value
        .trim()
        .to_lowercase()
        .replace(['_', '-'], " ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

fn sanitize_pet_speech_mode(value: &str) -> String {
    match value.trim() {
        "off" => "off",
        "encourage" => "encourage",
        "roast" => "roast",
        "flirty" => "flirty",
        "chuunibyou" => "chuunibyou",
        "mixed" => "mixed",
        _ => "off",
    }
    .to_string()
}

fn sanitize_pet_speech_frequency(value: &str) -> String {
    match value.trim() {
        "quiet" => "quiet",
        "lively" => "lively",
        "chatterbox" => "chatterbox",
        _ => "normal",
    }
    .to_string()
}

fn sanitize_pet_reminder_minutes(value: &str) -> String {
    numeric_string(value, 60, 15, 240).to_string()
}

fn sanitize_provider_reference(value: &str) -> String {
    let value = value.trim();
    if value.is_empty() {
        "automatic".to_string()
    } else {
        value.chars().take(160).collect()
    }
}

fn sanitize_provider_kind(value: &str) -> String {
    match value.trim() {
        "openai" => "openai",
        "anthropic" => "anthropic",
        "deepseek" => "deepseek",
        "gemini" => "gemini",
        "groq" => "groq",
        "openrouter" => "openrouter",
        "ollama" => "ollama",
        "localLlama" => "localLlama",
        _ => "openAICompatible",
    }
    .to_string()
}

fn normalize_hour(value: Option<i32>) -> Option<i32> {
    value.and_then(|hour| (0..=23).contains(&hour).then_some(hour))
}

pub fn locale_from_language_setting(language: &str) -> String {
    match language {
        "english" => "en",
        "simplifiedChinese" | "zh-CN" | "zh_CN" | "zh-Hans" | "zh-Hans-CN"
        | "zh_Hans_CN" => "zh-Hans",
        "traditionalChinese" | "zh-TW" | "zh_TW" | "zh-Hant" | "zh-Hant-TW"
        | "zh_Hant_TW" => "zh-Hant",
        "japanese" | "ja" => "ja",
        "korean" | "ko" => "ko",
        "french" | "fr" => "fr",
        "german" | "de" => "de",
        "spanish" | "es" => "es",
        "portugueseBrazil" | "pt-BR" => "pt-BR",
        "russian" | "ru" => "ru",
        _ => return app_settings::locale_from_language_setting(language),
    }
    .to_string()
}

fn provider_defaults(kind: &str) -> (&'static str, &'static str, &'static str) {
    match kind {
        "openai" | "openAICompatible" => {
            ("OpenAI API", "gpt-4.1-mini", "https://api.openai.com/v1")
        }
        "anthropic" => (
            "Claude API",
            "claude-3-5-haiku-latest",
            "https://api.anthropic.com/v1",
        ),
        "deepseek" => ("DeepSeek API", "deepseek-chat", "https://api.deepseek.com"),
        "gemini" => (
            "Gemini API",
            "gemini-2.5-flash",
            "https://generativelanguage.googleapis.com/v1beta",
        ),
        "groq" => (
            "Groq API",
            "llama-3.3-70b-versatile",
            "https://api.groq.com/openai/v1",
        ),
        "openrouter" => (
            "OpenRouter API",
            "openai/gpt-4.1-mini",
            "https://openrouter.ai/api/v1",
        ),
        "ollama" => ("Ollama", "llama3.2", "http://localhost:11434"),
        "localLlama" => ("Llama Model", "llama3.2", "http://localhost:11434"),
        _ => ("OpenAI API", "gpt-4.1-mini", "https://api.openai.com/v1"),
    }
}

fn sanitize_shortcut_id(value: &str) -> Result<String, String> {
    match value.trim() {
        "view.terminal" => Ok("view.terminal".to_string()),
        "view.files" => Ok("view.files".to_string()),
        "view.review" => Ok("view.review".to_string()),
        "project.create" => Ok("project.create".to_string()),
        "project.open_folder" => Ok("project.open_folder".to_string()),
        "settings.open" => Ok("settings.open".to_string()),
        "task.create" => Ok("task.create".to_string()),
        "sidebar.projects.toggle" => Ok("sidebar.projects.toggle".to_string()),
        "sidebar.tasks.toggle" => Ok("sidebar.tasks.toggle".to_string()),
        "terminal.split" => Ok("terminal.split".to_string()),
        "terminal.split.create" => Ok("terminal.split.create".to_string()),
        "terminal.tab" => Ok("terminal.tab".to_string()),
        "terminal.tab.create" => Ok("terminal.tab.create".to_string()),
        "panel.git" => Ok("panel.git".to_string()),
        "panel.ai" => Ok("panel.ai".to_string()),
        "assistant.git.open" => Ok("assistant.git.open".to_string()),
        "assistant.files.open" => Ok("assistant.files.open".to_string()),
        "assistant.ai.open" => Ok("assistant.ai.open".to_string()),
        "assistant.ssh.open" => Ok("assistant.ssh.open".to_string()),
        "editor.save" => Ok("editor.save".to_string()),
        "editor.search" => Ok("editor.search".to_string()),
        "close.active" => Ok("close.active".to_string()),
        _ => Err("Unsupported shortcut id.".to_string()),
    }
}

fn current_unix_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}

fn default_true() -> bool {
    true
}
