fn read_limited_file(path: &Path, max_bytes: usize) -> Option<String> {
    let metadata = fs::metadata(path).ok()?;
    if !metadata.is_file() || metadata.len() == 0 {
        return None;
    }
    let text = fs::read_to_string(path).ok()?;
    Some(text.chars().take(max_bytes).collect::<String>())
}

fn read_json_file(path: &Path) -> Option<Value> {
    read_limited_file(path, 80_000).and_then(|text| serde_json::from_str(&text).ok())
}

fn read_first_existing_file(root: &Path, names: &[&str], max_bytes: usize) -> Option<String> {
    names
        .iter()
        .find_map(|name| read_limited_file(&root.join(name), max_bytes))
}

fn root_file_markers(root: &Path) -> HashSet<String> {
    [
        "artisan",
        "bin/console",
        "manage.py",
        "requirements.txt",
        "Dockerfile",
        ".env.example",
    ]
    .into_iter()
    .filter(|name| root.join(name).exists())
    .map(str::to_string)
    .collect()
}

fn detect_package_manager(root: &Path) -> &'static str {
    if root.join("pnpm-lock.yaml").exists() || root.join("pnpm-workspace.yaml").exists() {
        "pnpm"
    } else if root.join("bun.lock").exists() || root.join("bun.lockb").exists() {
        "bun"
    } else if root.join("yarn.lock").exists() {
        "yarn"
    } else {
        "npm"
    }
}

fn top_level_directories(root: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(root) else {
        return Vec::new();
    };
    let mut dirs = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if !path.is_dir() {
                return None;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with('.') || matches!(name.as_str(), "node_modules" | "target" | "dist")
            {
                return None;
            }
            Some(name)
        })
        .collect::<Vec<_>>();
    dirs.sort();
    dirs.truncate(16);
    dirs
}
