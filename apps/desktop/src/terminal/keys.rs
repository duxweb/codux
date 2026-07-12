fn keystroke_to_bytes(keystroke: &Keystroke, mode: TerminalInputMode) -> Option<Vec<u8>> {
    codux_terminal_core::terminal_key_input_bytes(core_key_input(keystroke, mode))
}

fn is_copy_keystroke(keystroke: &Keystroke) -> bool {
    codux_terminal_core::terminal_is_copy_shortcut(core_key_input(
        keystroke,
        TerminalInputMode::default(),
    ))
}

fn is_paste_keystroke(keystroke: &Keystroke) -> bool {
    codux_terminal_core::terminal_is_paste_shortcut(core_key_input(
        keystroke,
        TerminalInputMode::default(),
    ))
}

fn is_select_all_keystroke(keystroke: &Keystroke) -> bool {
    let modifiers = &keystroke.modifiers;
    keystroke.key.eq_ignore_ascii_case("a")
        && modifiers.platform
        && !modifiers.control
        && !modifiers.alt
        && !modifiers.shift
}

/// cmd+up / cmd+down navigate OSC 133 prompt marks.
fn prompt_jump_direction(keystroke: &Keystroke) -> Option<i32> {
    let modifiers = &keystroke.modifiers;
    if !modifiers.platform || modifiers.control || modifiers.alt || modifiers.shift {
        return None;
    }
    match keystroke.key.as_str() {
        "up" => Some(-1),
        "down" => Some(1),
        _ => None,
    }
}

fn core_key_input(
    keystroke: &Keystroke,
    mode: TerminalInputMode,
) -> codux_terminal_core::TerminalKeyInput<'_> {
    codux_terminal_core::TerminalKeyInput {
        key: &keystroke.key,
        key_char: keystroke.key_char.as_deref(),
        modifiers: codux_terminal_core::TerminalKeyInputModifiers {
            shift: keystroke.modifiers.shift,
            alt: keystroke.modifiers.alt,
            control: keystroke.modifiers.control,
            platform: keystroke.modifiers.platform,
        },
        mode,
    }
}

fn terminal_clipboard_paste_text(cx: &mut App, paste_images_as_paths: bool) -> Option<String> {
    #[cfg(target_os = "windows")]
    match windows_terminal_clipboard_text() {
        Ok(text) => match terminal_clipboard_text_preference(text, paste_images_as_paths) {
            TerminalClipboardTextPreference::Text(text) => return Some(text),
            TerminalClipboardTextPreference::RichClipboard => {}
            TerminalClipboardTextPreference::None => return None,
        },
        Err(()) => return None,
    }

    let item = cx.read_from_clipboard()?;
    let text = item
        .text()
        .filter(|text| !paste_images_as_paths || !clipboard_text_looks_like_image_payload(text));
    if text.is_some() {
        return text;
    }
    if !paste_images_as_paths {
        return None;
    }
    item.entries().iter().find_map(|entry| match entry {
        ClipboardEntry::Image(image) if !image.bytes.is_empty() => {
            write_terminal_clipboard_image(image.format, &image.bytes)
                .ok()
                .map(|path| terminal_path_input(&path))
        }
        _ => None,
    })
}

#[cfg(any(target_os = "windows", test))]
#[derive(Debug, PartialEq, Eq)]
enum TerminalClipboardTextPreference {
    Text(String),
    RichClipboard,
    None,
}

#[cfg(any(target_os = "windows", test))]
fn terminal_clipboard_text_preference(
    text: Option<String>,
    paste_images_as_paths: bool,
) -> TerminalClipboardTextPreference {
    match text {
        Some(text)
            if !paste_images_as_paths || !clipboard_text_looks_like_image_payload(&text) =>
        {
            TerminalClipboardTextPreference::Text(text)
        }
        Some(_) if paste_images_as_paths => TerminalClipboardTextPreference::RichClipboard,
        None if paste_images_as_paths => TerminalClipboardTextPreference::RichClipboard,
        _ => TerminalClipboardTextPreference::None,
    }
}

fn clipboard_text_looks_like_image_payload(text: &str) -> bool {
    let trimmed = text.trim_start();
    trimmed.starts_with("data:image/")
        || trimmed.starts_with("<img ")
        || trimmed.starts_with("<img\n")
        || trimmed.starts_with("<img\t")
}

fn write_terminal_clipboard_image(format: ImageFormat, bytes: &[u8]) -> std::io::Result<PathBuf> {
    let directory = codux_runtime::runtime_paths::runtime_temp_dir().join("clipboard-images");
    fs::create_dir_all(&directory)?;
    let file_name = format!(
        "terminal-paste-{}-{}.{}",
        std::process::id(),
        terminal_clipboard_image_timestamp(),
        terminal_clipboard_image_extension(format)
    );
    let path = next_available_terminal_clipboard_path(&directory, &file_name);
    fs::write(&path, bytes)?;
    Ok(path)
}

fn next_available_terminal_clipboard_path(directory: &Path, file_name: &str) -> PathBuf {
    let candidate = directory.join(file_name);
    if !candidate.exists() {
        return candidate;
    }
    let source = Path::new(file_name);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(file_name);
    let extension = source.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let next_name = match extension {
            Some(extension) => format!("{stem}-{index}.{extension}"),
            None => format!("{stem}-{index}"),
        };
        let next = directory.join(next_name);
        if !next.exists() {
            return next;
        }
    }
    candidate
}

fn terminal_clipboard_image_timestamp() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_millis())
        .unwrap_or(0)
}

fn terminal_clipboard_image_extension(format: ImageFormat) -> &'static str {
    match format {
        ImageFormat::Png => "png",
        ImageFormat::Jpeg => "jpg",
        ImageFormat::Webp => "webp",
        ImageFormat::Gif => "gif",
        ImageFormat::Svg => "svg",
        ImageFormat::Bmp => "bmp",
        ImageFormat::Tiff => "tiff",
        ImageFormat::Ico => "ico",
        ImageFormat::Pnm => "pnm",
    }
}

fn terminal_path_input(path: &Path) -> String {
    shell_quote_path(&path.to_string_lossy())
}

fn terminal_paths_input(paths: &[PathBuf]) -> Option<String> {
    let mut values = paths
        .iter()
        .map(|path| terminal_path_input(path))
        .filter(|path| !path.trim().is_empty())
        .collect::<Vec<_>>();
    if values.is_empty() {
        return None;
    }
    values.push(String::new());
    Some(values.join(" "))
}

fn shell_quote_path(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}
