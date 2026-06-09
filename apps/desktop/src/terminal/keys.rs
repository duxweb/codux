#[derive(Debug, PartialEq, Eq)]
enum TerminalKeyModifiers {
    None,
    Alt,
    Ctrl,
    Shift,
    Platform,
    CtrlShift,
    Other,
}

impl TerminalKeyModifiers {
    fn new(keystroke: &Keystroke) -> Self {
        match (
            keystroke.modifiers.alt,
            keystroke.modifiers.control,
            keystroke.modifiers.shift,
            keystroke.modifiers.platform,
        ) {
            (false, false, false, false) => Self::None,
            (true, false, false, false) => Self::Alt,
            (false, true, false, false) => Self::Ctrl,
            (false, false, true, false) => Self::Shift,
            (false, false, false, true) => Self::Platform,
            (false, true, true, false) => Self::CtrlShift,
            _ => Self::Other,
        }
    }

    fn any(&self) -> bool {
        !matches!(self, Self::None)
    }
}

fn keystroke_to_bytes(keystroke: &Keystroke, mode: TermMode) -> Option<Vec<u8>> {
    if keystroke.modifiers.control
        && !keystroke.modifiers.alt
        && !keystroke.modifiers.platform
        && let Some(sequence) = control_key_char_sequence(keystroke)
    {
        return Some(sequence);
    }

    let modifiers = TerminalKeyModifiers::new(keystroke);
    let key = normalize_terminal_key(&keystroke.key);
    let manual = match (key.as_str(), &modifiers) {
        ("tab", TerminalKeyModifiers::None) => Some("\x09"),
        ("escape", TerminalKeyModifiers::None) => Some("\x1b"),
        ("enter", TerminalKeyModifiers::None) => Some("\x0d"),
        ("enter", TerminalKeyModifiers::Shift) => Some("\x0a"),
        ("enter", TerminalKeyModifiers::Alt) => Some("\x1b\x0d"),
        ("backspace", TerminalKeyModifiers::None) | ("back", TerminalKeyModifiers::None) => {
            Some("\x7f")
        }
        ("tab", TerminalKeyModifiers::Shift) => Some("\x1b[Z"),
        ("backspace", TerminalKeyModifiers::Ctrl) => Some("\x08"),
        ("backspace", TerminalKeyModifiers::Alt) => Some("\x1b\x7f"),
        ("back", TerminalKeyModifiers::Alt) => Some("\x1b\x7f"),
        ("delete", TerminalKeyModifiers::Alt) => Some("\x1bd"),
        ("backspace", TerminalKeyModifiers::Platform) => Some("\x15"),
        ("back", TerminalKeyModifiers::Platform) => Some("\x15"),
        ("delete", TerminalKeyModifiers::Platform) => Some("\x0b"),
        ("backspace", TerminalKeyModifiers::Shift) => Some("\x7f"),
        ("space", TerminalKeyModifiers::Ctrl) => Some("\x00"),
        ("home", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOH")
        }
        ("home", TerminalKeyModifiers::None) => Some("\x1b[H"),
        ("end", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOF")
        }
        ("end", TerminalKeyModifiers::None) => Some("\x1b[F"),
        ("up", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => Some("\x1bOA"),
        ("up", TerminalKeyModifiers::None) => Some("\x1b[A"),
        ("down", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOB")
        }
        ("down", TerminalKeyModifiers::None) => Some("\x1b[B"),
        ("right", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOC")
        }
        ("right", TerminalKeyModifiers::None) => Some("\x1b[C"),
        ("left", TerminalKeyModifiers::None) if mode.contains(TermMode::APP_CURSOR) => {
            Some("\x1bOD")
        }
        ("left", TerminalKeyModifiers::None) => Some("\x1b[D"),
        ("right", TerminalKeyModifiers::Alt) => Some("\x1bf"),
        ("left", TerminalKeyModifiers::Alt) => Some("\x1bb"),
        ("right", TerminalKeyModifiers::Platform) => Some("\x05"),
        ("left", TerminalKeyModifiers::Platform) => Some("\x01"),
        ("end", TerminalKeyModifiers::Platform) => Some("\x05"),
        ("home", TerminalKeyModifiers::Platform) => Some("\x01"),
        ("insert", TerminalKeyModifiers::None) => Some("\x1b[2~"),
        ("delete", TerminalKeyModifiers::None) => Some("\x1b[3~"),
        ("pageup", TerminalKeyModifiers::None) => Some("\x1b[5~"),
        ("pagedown", TerminalKeyModifiers::None) => Some("\x1b[6~"),
        ("f1", TerminalKeyModifiers::None) => Some("\x1bOP"),
        ("f2", TerminalKeyModifiers::None) => Some("\x1bOQ"),
        ("f3", TerminalKeyModifiers::None) => Some("\x1bOR"),
        ("f4", TerminalKeyModifiers::None) => Some("\x1bOS"),
        ("f5", TerminalKeyModifiers::None) => Some("\x1b[15~"),
        ("f6", TerminalKeyModifiers::None) => Some("\x1b[17~"),
        ("f7", TerminalKeyModifiers::None) => Some("\x1b[18~"),
        ("f8", TerminalKeyModifiers::None) => Some("\x1b[19~"),
        ("f9", TerminalKeyModifiers::None) => Some("\x1b[20~"),
        ("f10", TerminalKeyModifiers::None) => Some("\x1b[21~"),
        ("f11", TerminalKeyModifiers::None) => Some("\x1b[23~"),
        ("f12", TerminalKeyModifiers::None) => Some("\x1b[24~"),
        ("f13", TerminalKeyModifiers::None) => Some("\x1b[25~"),
        ("f14", TerminalKeyModifiers::None) => Some("\x1b[26~"),
        ("f15", TerminalKeyModifiers::None) => Some("\x1b[28~"),
        ("f16", TerminalKeyModifiers::None) => Some("\x1b[29~"),
        ("f17", TerminalKeyModifiers::None) => Some("\x1b[31~"),
        ("f18", TerminalKeyModifiers::None) => Some("\x1b[32~"),
        ("f19", TerminalKeyModifiers::None) => Some("\x1b[33~"),
        ("f20", TerminalKeyModifiers::None) => Some("\x1b[34~"),
        (key, TerminalKeyModifiers::Ctrl | TerminalKeyModifiers::CtrlShift) => ctrl_sequence(key),
        _ => None,
    };
    if let Some(sequence) = manual {
        return Some(sequence.as_bytes().to_vec());
    }

    if modifiers.any() {
        let modifier_code = terminal_modifier_code(keystroke);
        let modified = match key.as_str() {
            "up" => Some(format!("\x1b[1;{modifier_code}A")),
            "down" => Some(format!("\x1b[1;{modifier_code}B")),
            "right" => Some(format!("\x1b[1;{modifier_code}C")),
            "left" => Some(format!("\x1b[1;{modifier_code}D")),
            "f1" => Some(format!("\x1b[1;{modifier_code}P")),
            "f2" => Some(format!("\x1b[1;{modifier_code}Q")),
            "f3" => Some(format!("\x1b[1;{modifier_code}R")),
            "f4" => Some(format!("\x1b[1;{modifier_code}S")),
            "f5" => Some(format!("\x1b[15;{modifier_code}~")),
            "f6" => Some(format!("\x1b[17;{modifier_code}~")),
            "f7" => Some(format!("\x1b[18;{modifier_code}~")),
            "f8" => Some(format!("\x1b[19;{modifier_code}~")),
            "f9" => Some(format!("\x1b[20;{modifier_code}~")),
            "f10" => Some(format!("\x1b[21;{modifier_code}~")),
            "f11" => Some(format!("\x1b[23;{modifier_code}~")),
            "f12" => Some(format!("\x1b[24;{modifier_code}~")),
            "f13" => Some(format!("\x1b[25;{modifier_code}~")),
            "f14" => Some(format!("\x1b[26;{modifier_code}~")),
            "f15" => Some(format!("\x1b[28;{modifier_code}~")),
            "f16" => Some(format!("\x1b[29;{modifier_code}~")),
            "f17" => Some(format!("\x1b[31;{modifier_code}~")),
            "f18" => Some(format!("\x1b[32;{modifier_code}~")),
            "f19" => Some(format!("\x1b[33;{modifier_code}~")),
            "f20" => Some(format!("\x1b[34;{modifier_code}~")),
            "insert" => Some(format!("\x1b[2;{modifier_code}~")),
            "delete" => Some(format!("\x1b[3;{modifier_code}~")),
            "pageup" => Some(format!("\x1b[5;{modifier_code}~")),
            "pagedown" => Some(format!("\x1b[6;{modifier_code}~")),
            "end" => Some(format!("\x1b[1;{modifier_code}F")),
            "home" => Some(format!("\x1b[1;{modifier_code}H")),
            _ => None,
        };
        if let Some(sequence) = modified {
            return Some(sequence.into_bytes());
        }
    }

    if keystroke.modifiers.alt
        && !keystroke.modifiers.control
        && !keystroke.modifiers.platform
        && key.is_ascii()
        && key.chars().count() == 1
    {
        let mut key = key;
        if keystroke.modifiers.shift {
            key = key.to_ascii_uppercase();
        }
        return Some(format!("\x1b{key}").into_bytes());
    }

    None
}

fn normalize_terminal_key(key: &str) -> String {
    let normalized = key.to_ascii_lowercase();
    match normalized.as_str() {
        "return" | "kp_enter" | "numpadenter" | "numpad_enter" => "enter",
        "esc" => "escape",
        "backtab" | "iso_left_tab" => "tab",
        "del" => "delete",
        "pgup" | "page_up" => "pageup",
        "pgdn" | "page_down" => "pagedown",
        "arrowup" | "arrow_up" | "up_arrow" => "up",
        "arrowdown" | "arrow_down" | "down_arrow" => "down",
        "arrowleft" | "arrow_left" | "left_arrow" => "left",
        "arrowright" | "arrow_right" | "right_arrow" => "right",
        _ => normalized.as_str(),
    }
    .to_string()
}

fn control_key_char_sequence(keystroke: &Keystroke) -> Option<Vec<u8>> {
    let key_char = keystroke.key_char.as_deref()?;
    let mut chars = key_char.chars();
    let ch = chars.next()?;
    if chars.next().is_some() {
        return None;
    }
    if ch.is_control() {
        return Some(vec![ch as u8]);
    }
    ctrl_sequence(&ch.to_string()).map(|sequence| sequence.as_bytes().to_vec())
}

fn ctrl_sequence(key: &str) -> Option<&'static str> {
    match key {
        "a" | "A" => Some("\x01"),
        "b" | "B" => Some("\x02"),
        "c" | "C" => Some("\x03"),
        "d" | "D" => Some("\x04"),
        "e" | "E" => Some("\x05"),
        "f" | "F" => Some("\x06"),
        "g" | "G" => Some("\x07"),
        "h" | "H" => Some("\x08"),
        "i" | "I" => Some("\x09"),
        "j" | "J" => Some("\x0a"),
        "k" | "K" => Some("\x0b"),
        "l" | "L" => Some("\x0c"),
        "m" | "M" => Some("\x0d"),
        "n" | "N" => Some("\x0e"),
        "o" | "O" => Some("\x0f"),
        "p" | "P" => Some("\x10"),
        "q" | "Q" => Some("\x11"),
        "r" | "R" => Some("\x12"),
        "s" | "S" => Some("\x13"),
        "t" | "T" => Some("\x14"),
        "u" | "U" => Some("\x15"),
        "v" | "V" => Some("\x16"),
        "w" | "W" => Some("\x17"),
        "x" | "X" => Some("\x18"),
        "y" | "Y" => Some("\x19"),
        "z" | "Z" => Some("\x1a"),
        "@" => Some("\x00"),
        "[" => Some("\x1b"),
        "\\" => Some("\x1c"),
        "]" => Some("\x1d"),
        "^" => Some("\x1e"),
        "_" => Some("\x1f"),
        "?" => Some("\x7f"),
        _ => None,
    }
}

fn terminal_modifier_code(keystroke: &Keystroke) -> u32 {
    let mut code = 0;
    if keystroke.modifiers.shift {
        code |= 1;
    }
    if keystroke.modifiers.alt {
        code |= 1 << 1;
    }
    if keystroke.modifiers.control {
        code |= 1 << 2;
    }
    code + 1
}

fn is_copy_keystroke(keystroke: &Keystroke) -> bool {
    normalize_terminal_key(&keystroke.key) == "c"
        && keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
}

fn is_paste_keystroke(keystroke: &Keystroke) -> bool {
    normalize_terminal_key(&keystroke.key) == "v"
        && keystroke.modifiers.platform
        && !keystroke.modifiers.control
        && !keystroke.modifiers.alt
}

fn terminal_clipboard_paste_text(cx: &mut App, paste_images_as_paths: bool) -> Option<String> {
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
