use super::read_limited_file;
use std::{collections::HashSet, fs, path::Path};

#[derive(Debug, Clone)]
pub(super) struct ProjectSourceSample {
    path: String,
    signals: Vec<String>,
}

pub(super) fn collect_project_source_samples(root: &Path) -> Vec<ProjectSourceSample> {
    let mut candidates = Vec::new();
    for relative in [
        "src/main.tsx",
        "src/main.ts",
        "src/App.tsx",
        "src/App.ts",
        "src/index.tsx",
        "src/index.ts",
        "src/router.ts",
        "src/routes.ts",
        "src-tauri/src/lib.rs",
        "src-tauri/src/main.rs",
        "routes/web.php",
        "routes/api.php",
        "app/Http/Controllers",
        "cmd",
        "internal",
        "pkg",
        "src",
    ] {
        collect_source_sample_candidates(root, relative, &mut candidates);
    }

    let mut seen = HashSet::new();
    candidates
        .into_iter()
        .filter(|path| seen.insert(path.clone()))
        .filter_map(|path| source_sample_for_path(root, &path))
        .take(10)
        .collect()
}

fn collect_source_sample_candidates(root: &Path, relative: &str, output: &mut Vec<String>) {
    let path = root.join(relative);
    if path.is_file() {
        output.push(relative.to_string());
        return;
    }
    if !path.is_dir() {
        return;
    }
    let Ok(entries) = fs::read_dir(&path) else {
        return;
    };
    let mut files = entries
        .flatten()
        .filter_map(|entry| {
            let entry_path = entry.path();
            if entry_path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                let nested = entry_path.join("mod.rs");
                if nested.is_file() {
                    return path_relative_to(root, &nested);
                }
                if name.ends_with("Controller") || matches!(name.as_str(), "routes" | "pages") {
                    return path_relative_to(root, &entry_path);
                }
                return None;
            }
            source_sample_supported_file(&entry_path)
                .then(|| path_relative_to(root, &entry_path))?
        })
        .collect::<Vec<_>>();
    files.sort_by_key(|name| source_sample_priority(name));
    output.extend(files.into_iter().take(4));
}

fn source_sample_for_path(root: &Path, relative: &str) -> Option<ProjectSourceSample> {
    let path = root.join(relative);
    if path.is_dir() {
        return source_sample_for_directory(relative, &path);
    }
    if !source_sample_supported_file(&path) {
        return None;
    }
    let metadata = fs::metadata(&path).ok()?;
    if metadata.len() > 180_000 {
        return None;
    }
    let text = read_limited_file(&path, 24_000)?;
    let signals = extract_source_signals(&text);
    if signals.is_empty() {
        return None;
    }
    Some(ProjectSourceSample {
        path: relative.to_string(),
        signals,
    })
}

fn source_sample_for_directory(relative: &str, path: &Path) -> Option<ProjectSourceSample> {
    let names = source_module_names(path)
        .into_iter()
        .take(8)
        .collect::<Vec<_>>();
    if names.is_empty() {
        return None;
    }
    Some(ProjectSourceSample {
        path: relative.to_string(),
        signals: vec![format!("contains {}", names.join(", "))],
    })
}

fn source_sample_supported_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            matches!(
                extension,
                "ts" | "tsx" | "js" | "jsx" | "rs" | "php" | "py" | "go" | "java" | "kt" | "rb"
            )
        })
}

fn source_sample_priority(path: &str) -> (usize, String) {
    let lower = path.to_lowercase();
    let rank = if lower.contains("main.") || lower.contains("app.") || lower.contains("lib.rs") {
        0
    } else if lower.contains("route") || lower.contains("controller") {
        1
    } else if lower.contains("window") || lower.contains("manager") {
        2
    } else {
        3
    };
    (rank, lower)
}

fn path_relative_to(root: &Path, path: &Path) -> Option<String> {
    path.strip_prefix(root)
        .ok()
        .map(|path| path.to_string_lossy().replace('\\', "/"))
}

fn extract_source_signals(text: &str) -> Vec<String> {
    let lines = text.lines().collect::<Vec<_>>();
    let mut signals = Vec::new();
    for (index, line) in lines.iter().enumerate() {
        let trimmed = compact_source_line(line);
        if trimmed.is_empty() {
            continue;
        }
        if is_source_import_export_signal(&trimmed)
            || is_source_declaration_signal(&trimmed)
            || is_source_route_signal(&trimmed)
            || is_source_runtime_signal(&trimmed)
        {
            push_source_signal(&mut signals, trimmed.clone());
        }
        if trimmed == "#[tauri::command]"
            && let Some(next) = lines.get(index + 1).map(|line| compact_source_line(line))
        {
            push_source_signal(&mut signals, format!("tauri command {}", next));
        }
        if signals.len() >= 8 {
            break;
        }
    }
    signals
}

fn compact_source_line(line: &str) -> String {
    line.trim()
        .trim_start_matches("pub ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(180)
        .collect()
}

fn is_source_import_export_signal(line: &str) -> bool {
    line.starts_with("import ")
        || line.starts_with("export ")
        || line.starts_with("use crate::")
        || line.starts_with("use ")
        || line.starts_with("mod ")
}

fn is_source_declaration_signal(line: &str) -> bool {
    line.starts_with("fn ")
        || line.starts_with("async fn ")
        || line.starts_with("function ")
        || line.starts_with("const ")
        || line.starts_with("class ")
        || line.starts_with("interface ")
        || line.starts_with("type ")
        || line.starts_with("struct ")
        || line.starts_with("enum ")
        || line.starts_with("impl ")
}

fn is_source_route_signal(line: &str) -> bool {
    line.contains("Route::")
        || line.contains("router.")
        || line.contains("app.get(")
        || line.contains("app.post(")
        || line.contains("<Route")
        || line.contains("createBrowserRouter")
}

fn is_source_runtime_signal(line: &str) -> bool {
    line.contains("createRoot(")
        || line.contains("invoke_handler")
        || line.contains("tauri::generate_handler")
        || line.contains("register_plugin")
        || line.contains("createApp(")
}

fn push_source_signal(signals: &mut Vec<String>, signal: String) {
    if signal.chars().count() < 8 || signals.iter().any(|existing| existing == &signal) {
        return;
    }
    signals.push(signal);
}

pub(super) fn render_source_sample_signals(samples: &[ProjectSourceSample]) -> Vec<String> {
    samples
        .iter()
        .map(|sample| format!("{}: {}", sample.path, sample.signals.join("; ")))
        .collect()
}

pub(super) fn source_module_names(src: &Path) -> Vec<String> {
    let Ok(entries) = fs::read_dir(src) else {
        return Vec::new();
    };
    let mut modules = entries
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().to_string();
            if path.is_dir() {
                return Some(name);
            }
            path.extension()
                .and_then(|extension| extension.to_str())
                .filter(|extension| {
                    matches!(
                        *extension,
                        "ts" | "tsx" | "rs" | "php" | "py" | "go" | "java" | "kt" | "rb"
                    )
                })
                .map(|_| name)
        })
        .collect::<Vec<_>>();
    modules.sort();
    modules.truncate(24);
    modules
}
