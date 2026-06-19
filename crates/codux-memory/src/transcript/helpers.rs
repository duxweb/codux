pub(super) fn session_identifier(session: &MemorySessionSnapshot) -> String {
    session
        .ai_session_id
        .as_deref()
        .and_then(|ai_session| normalized_string(Some(ai_session)))
        .unwrap_or_else(|| session.terminal_id.clone())
}

fn tail_chars(value: &str, max_chars: usize) -> String {
    let mut chars = value.chars().rev().take(max_chars).collect::<Vec<_>>();
    chars.reverse();
    chars.into_iter().collect::<String>()
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}
