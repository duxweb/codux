fn directory_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut files = entries
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|value| value.to_str()) == Some(extension))
        .collect::<Vec<_>>();
    files.sort();
    files
}

fn recursive_files(dir: &Path, extension: &str) -> Vec<PathBuf> {
    let mut files = Vec::new();
    collect_recursive_files(dir, extension, &mut files);
    files.sort();
    files
}

fn collect_recursive_files(dir: &Path, extension: &str, files: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_recursive_files(&path, extension, files);
        } else if path.extension().and_then(|value| value.to_str()) == Some(extension) {
            files.push(path);
        }
    }
}

fn for_each_jsonl_line<F>(file_path: &Path, starting_at: i64, mut body: F) -> std::io::Result<()>
where
    F: FnMut(&str, i64) -> bool,
{
    let mut file = fs::File::open(file_path)?;
    let offset = starting_at.max(0) as u64;
    file.seek(SeekFrom::Start(offset))?;
    let mut reader = BufReader::new(file);
    let mut current_offset = offset;
    loop {
        let mut line = String::new();
        let byte_count = reader.read_line(&mut line)?;
        if byte_count == 0 {
            break;
        }
        current_offset = current_offset.saturating_add(byte_count as u64);
        let line = line.trim_end_matches(['\n', '\r']);
        if line.is_empty() {
            continue;
        }
        if !body(line, current_offset.min(i64::MAX as u64) as i64) {
            break;
        }
    }
    Ok(())
}

fn file_modified_millis(path: &Path) -> Option<u128> {
    fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(std::time::UNIX_EPOCH)
        .ok()
        .map(|duration| duration.as_millis())
}

fn paths_equivalent(left: Option<&str>, right: &str) -> bool {
    let Some(left) = left.and_then(normalized_history_path) else {
        return false;
    };
    let Some(right) = normalized_history_path(right) else {
        return false;
    };
    left == right
}

fn normalized_history_path(value: &str) -> Option<String> {
    let mut value = value.trim();
    if value.is_empty() {
        return None;
    }
    value = value
        .strip_prefix(r"\\?\")
        .or_else(|| value.strip_prefix(r"//?/"))
        .unwrap_or(value);
    let mut normalized = value.replace('\\', "/");
    while normalized.ends_with('/') && !is_path_root(&normalized) {
        normalized.pop();
    }
    if has_windows_drive_prefix(&normalized) {
        normalized = normalized.to_ascii_lowercase();
    }
    Some(normalized)
}

fn has_windows_drive_prefix(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() >= 2 && bytes[1] == b':' && bytes[0].is_ascii_alphabetic()
}

fn is_path_root(value: &str) -> bool {
    value == "/" || (value.len() == 3 && has_windows_drive_prefix(value) && value.ends_with('/'))
}

fn normalized_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn displayable_model_name(value: Option<&str>) -> Option<&str> {
    let value = value?.trim();
    if value.is_empty() || value.eq_ignore_ascii_case("unknown") {
        return None;
    }
    Some(value)
}

fn json_i64(value: Option<&Value>) -> i64 {
    value
        .and_then(|value| {
            value
                .as_i64()
                .or_else(|| value.as_f64().map(|value| value as i64))
                .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
        })
        .unwrap_or(0)
}
