pub(super) fn normalized_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

pub(super) fn normalize_path(path: &str) -> String {
    crate::git::normalize_repository_path(path)
}

pub(super) fn short_hash(value: &str) -> String {
    value.chars().take(7).collect()
}

pub(super) fn now_seconds() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs() as i64)
        .unwrap_or(0)
}
