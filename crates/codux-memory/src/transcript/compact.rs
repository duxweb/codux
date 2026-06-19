fn compact_transcript_for_memory(text: &str, token_limit: i32) -> Option<String> {
    let mut output = Vec::new();
    let mut omitted_low_signal = 0usize;
    let mut in_code_block = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if trimmed.starts_with("```") {
            in_code_block = !in_code_block;
            omitted_low_signal += 1;
            continue;
        }
        if in_code_block || looks_like_tool_or_log_line(trimmed) {
            omitted_low_signal += 1;
            continue;
        }
        let char_count = trimmed.chars().count();
        if char_count > 700 {
            output.push(format!(
                "{} ... {} [omitted long pasted content, {} chars]",
                trimmed.chars().take(160).collect::<String>().trim(),
                tail_chars(trimmed, 80),
                char_count
            ));
            continue;
        }
        output.push(trimmed.to_string());
    }
    if omitted_low_signal > 0 {
        output.push(format!(
            "[omitted {} low-signal code/log/tool-output lines before memory extraction]",
            omitted_low_signal
        ));
    }
    normalized_string(Some(&trim_memory_text(&output.join("\n"), token_limit)))
}

fn looks_like_tool_or_log_line(line: &str) -> bool {
    let lower = line.to_lowercase();
    let prefixes = [
        "stdout:",
        "stderr:",
        "tool:",
        "assistant.tool",
        "user.tool",
        "[tool]",
        "[stdout]",
        "[stderr]",
        "trace:",
        "debug:",
    ];
    prefixes.iter().any(|prefix| lower.starts_with(prefix))
        || (line.len() > 260 && line.chars().filter(|ch| ch.is_ascii_punctuation()).count() > 60)
}
