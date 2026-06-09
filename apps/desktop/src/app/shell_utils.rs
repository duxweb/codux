pub(in crate::app) fn shell_quote(value: &str) -> String {
    if value
        .chars()
        .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.' | '/' | ':' | '='))
    {
        value.to_string()
    } else {
        format!("'{}'", value.replace('\'', "'\\''"))
    }
}

pub(in crate::app) fn shell_read_file_arg(path: &str) -> String {
    if cfg!(windows) {
        format!("(Get-Content -Raw -LiteralPath {})", powershell_quote(path))
    } else {
        format!("\"$(cat {})\"", shell_quote(path))
    }
}

fn powershell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "''"))
}

pub(in crate::app) fn terminal_command_text(command: &str) -> String {
    if cfg!(windows) {
        format!("{command}\r")
    } else {
        format!("{command}\n")
    }
}

#[cfg(test)]
pub(in crate::app) fn shell_join(parts: Vec<String>) -> String {
    parts
        .into_iter()
        .map(|part| shell_quote(&part))
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn terminal_command_text_uses_platform_enter() {
        let text = terminal_command_text("codex resume session-1");
        if cfg!(windows) {
            assert_eq!(text, "codex resume session-1\r");
        } else {
            assert_eq!(text, "codex resume session-1\n");
        }
    }
}
