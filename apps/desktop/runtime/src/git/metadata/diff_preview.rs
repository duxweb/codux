fn untracked_file_preview(repo: &Path, file_path: &str) -> Result<String, String> {
    let target = repo.join(file_path);
    let bytes = fs::read(&target).map_err(|error| error.to_string())?;
    if bytes.contains(&0) {
        return Ok("Untracked binary file preview is not supported.".to_string());
    }
    let is_truncated = bytes.len() > MAX_DIFF_BYTES;
    let sample = if is_truncated {
        &bytes[..MAX_DIFF_BYTES]
    } else {
        &bytes
    };
    let text = String::from_utf8_lossy(sample);
    Ok(format!(
        "--- untracked ---\n+++ {}\n{}{}",
        file_path,
        text,
        if is_truncated {
            "\n... diff preview truncated"
        } else {
            ""
        }
    ))
}

fn truncate_diff(diff: String) -> String {
    if diff.len() <= MAX_DIFF_BYTES {
        return diff;
    }
    format!("{}\n... diff preview truncated", &diff[..MAX_DIFF_BYTES])
}
