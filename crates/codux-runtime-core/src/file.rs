use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub const MOBILE_TEXT_FILE_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

pub fn file_list_payload(path: Option<&str>, purpose: Option<&str>) -> Value {
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let requested = path
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&home);
    let requested_path = PathBuf::from(requested);
    let directory = if requested_path.is_dir() {
        requested_path
    } else {
        requested_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from(&home))
    };
    let mut entries = fs::read_dir(&directory)
        .ok()
        .into_iter()
        .flat_map(|read_dir| read_dir.filter_map(Result::ok))
        .filter_map(|entry| {
            let path = entry.path();
            let name = path.file_name()?.to_str()?.to_string();
            if name.starts_with('.') {
                return None;
            }
            // symlink_metadata so symlinks are reported as such, not followed.
            let metadata = fs::symlink_metadata(&path).ok();
            let is_symlink = metadata
                .as_ref()
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false);
            let size = metadata.as_ref().map(|metadata| metadata.len()).unwrap_or(0);
            let modified_at = metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            Some(json!({
                "name": name,
                "path": path.to_string_lossy().to_string(),
                "isDirectory": path.is_dir(),
                "isSymbolicLink": is_symlink,
                "size": size,
                "modifiedAt": modified_at,
            }))
        })
        .collect::<Vec<_>>();
    entries.sort_by(|left, right| {
        let left_dir = left
            .get("isDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let right_dir = right
            .get("isDirectory")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        right_dir.cmp(&left_dir).then_with(|| {
            left.get("name")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_lowercase()
                .cmp(
                    &right
                        .get("name")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .to_lowercase(),
                )
        })
    });
    let mut payload = json!({
        "path": directory.to_string_lossy().to_string(),
        "parent": directory.parent().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
        "entries": entries,
    });
    if let Some(purpose) = purpose {
        payload["purpose"] = Value::String(purpose.to_string());
    }
    payload
}

pub fn file_read_payload(path: &str) -> Result<Value, String> {
    let path = PathBuf::from(path);
    if path.is_dir() {
        return Err("Cannot open a directory as a file.".to_string());
    }
    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    if metadata.len() > MOBILE_TEXT_FILE_LIMIT_BYTES {
        return Err("File is larger than 2MB and cannot be opened on mobile yet.".to_string());
    }
    let content = fs::read_to_string(&path)
        .map_err(|_| "Only UTF-8 text files can be edited on mobile.".to_string())?;
    Ok(json!({
        "path": path.to_string_lossy().to_string(),
        "name": path.file_name().and_then(|value| value.to_str()).unwrap_or_default(),
        "content": content,
        "size": content.len(),
    }))
}

pub fn file_write(path: &str, content: &str) -> Result<(), String> {
    fs::write(path, content).map_err(|error| error.to_string())
}

/// Write raw bytes as `name` inside `directory`; returns the new absolute path.
pub fn file_write_bytes(directory: &str, name: &str, bytes: &[u8]) -> Result<String, String> {
    let destination = unique_destination(&PathBuf::from(directory), name);
    fs::write(&destination, bytes).map_err(|error| error.to_string())?;
    Ok(destination.to_string_lossy().to_string())
}

/// Copy a file or directory into `target_directory`; returns the new path.
pub fn file_copy(path: &str, target_directory: &str) -> Result<String, String> {
    let source = PathBuf::from(path);
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid source path.".to_string())?;
    let destination = unique_destination(&PathBuf::from(target_directory), name);
    if source.is_dir() {
        copy_dir_recursive(&source, &destination)?;
    } else {
        fs::copy(&source, &destination).map_err(|error| error.to_string())?;
    }
    Ok(destination.to_string_lossy().to_string())
}

/// Move a file or directory into `target_directory`; returns the new path.
pub fn file_move(path: &str, target_directory: &str, overwrite: bool) -> Result<String, String> {
    let source = PathBuf::from(path);
    let name = source
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| "Invalid source path.".to_string())?;
    let destination = PathBuf::from(target_directory).join(name);
    let destination = if overwrite {
        if destination.exists() {
            if destination.is_dir() {
                let _ = fs::remove_dir_all(&destination);
            } else {
                let _ = fs::remove_file(&destination);
            }
        }
        destination
    } else {
        unique_destination(&PathBuf::from(target_directory), name)
    };
    fs::rename(&source, &destination).map_err(|error| error.to_string())?;
    Ok(destination.to_string_lossy().to_string())
}

fn unique_destination(directory: &Path, name: &str) -> PathBuf {
    let candidate = directory.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let (stem, ext) = match name.rsplit_once('.') {
        Some((stem, ext)) if !stem.is_empty() => (stem.to_string(), format!(".{ext}")),
        _ => (name.to_string(), String::new()),
    };
    for index in 1.. {
        let candidate = directory.join(format!("{stem} {index}{ext}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    directory.join(name)
}

fn copy_dir_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir_all(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let child = entry.path();
        let target = destination.join(entry.file_name());
        if child.is_dir() {
            copy_dir_recursive(&child, &target)?;
        } else {
            fs::copy(&child, &target).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn file_rename(path: &str, new_path: &str) -> Result<(), String> {
    let source = PathBuf::from(path);
    let destination = PathBuf::from(new_path);
    if source.parent() != destination.parent() {
        return Err("Rename must stay in the same directory.".to_string());
    }
    if destination.exists() {
        return Err("A file with this name already exists.".to_string());
    }
    fs::rename(source, destination).map_err(|error| error.to_string())
}

pub fn file_delete(path: &str) -> Result<(), String> {
    let target = PathBuf::from(path);
    if target.is_dir() {
        fs::remove_dir_all(target).map_err(|error| error.to_string())
    } else {
        fs::remove_file(target).map_err(|error| error.to_string())
    }
}

pub fn file_make_directory(path: &str) -> Result<(), String> {
    let target = PathBuf::from(path);
    if target.exists() {
        return Err("A file or directory with this name already exists.".to_string());
    }
    fs::create_dir_all(target).map_err(|error| error.to_string())
}
