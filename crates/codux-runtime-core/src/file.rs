use serde_json::{Value, json};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::UNIX_EPOCH,
};

pub const MOBILE_TEXT_FILE_LIMIT_BYTES: u64 = 2 * 1024 * 1024;

// Cross-platform path primitives live in `crate::path`; re-export the sentinel
// from here too so existing `file::FILE_LIST_DRIVES_SENTINEL` references keep
// resolving.
pub use crate::path::FILE_LIST_DRIVES_SENTINEL;
use crate::path::{display_path, drive_root_parent};

pub fn file_list_payload(path: Option<&str>, purpose: Option<&str>) -> Value {
    let show_hidden = purpose == Some("sshKey");
    let requested = path.map(str::trim).filter(|value| !value.is_empty());
    if requested == Some(FILE_LIST_DRIVES_SENTINEL) {
        return drive_list_payload(purpose);
    }
    let home = std::env::var("HOME")
        .or_else(|_| std::env::var("USERPROFILE"))
        .unwrap_or_else(|_| ".".to_string());
    let requested = requested.unwrap_or(home.as_str());
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
            if !show_hidden && name.starts_with('.') {
                return None;
            }
            // symlink_metadata so symlinks are reported as such, not followed.
            let metadata = fs::symlink_metadata(&path).ok();
            let is_symlink = metadata
                .as_ref()
                .map(|metadata| metadata.file_type().is_symlink())
                .unwrap_or(false);
            let size = metadata
                .as_ref()
                .map(|metadata| metadata.len())
                .unwrap_or(0);
            let modified_at = metadata
                .as_ref()
                .and_then(|metadata| metadata.modified().ok())
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_secs() as i64)
                .unwrap_or(0);
            Some(json!({
                "name": name,
                "path": display_path(&path.to_string_lossy()),
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
    let parent = match directory.parent() {
        Some(parent) => display_path(&parent.to_string_lossy()),
        // A volume root (`C:\`, `/`) has no parent: step up to the drive list on
        // Windows so other volumes stay reachable; on POSIX there's nowhere up.
        None => drive_root_parent(),
    };
    let mut payload = json!({
        "path": display_path(&directory.to_string_lossy()),
        "parent": parent,
        "entries": entries,
    });
    if let Some(purpose) = purpose {
        payload["purpose"] = Value::String(purpose.to_string());
    }
    payload
}

fn drive_list_payload(purpose: Option<&str>) -> Value {
    let mut payload = json!({
        "path": FILE_LIST_DRIVES_SENTINEL,
        "parent": "",
        "entries": drive_entries(),
    });
    if let Some(purpose) = purpose {
        payload["purpose"] = Value::String(purpose.to_string());
    }
    payload
}

#[cfg(target_os = "windows")]
fn drive_entries() -> Vec<Value> {
    (b'A'..=b'Z')
        .filter_map(|letter| {
            let letter = letter as char;
            let root = format!("{letter}:\\");
            Path::new(&root).is_dir().then(|| {
                json!({
                    "name": format!("{letter}:"),
                    "path": root,
                    "isDirectory": true,
                    "isSymbolicLink": false,
                    "size": 0,
                    "modifiedAt": 0,
                })
            })
        })
        .collect()
}

#[cfg(not(target_os = "windows"))]
fn drive_entries() -> Vec<Value> {
    vec![json!({
        "name": "/",
        "path": "/",
        "isDirectory": true,
        "isSymbolicLink": false,
        "size": 0,
        "modifiedAt": 0,
    })]
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

pub fn file_read_blob_bytes(path: &str) -> Result<Vec<u8>, String> {
    let path = path.trim();
    if path.is_empty() {
        return Err("File path is required.".to_string());
    }
    let file = fs::File::open(path).map_err(|error| error.to_string())?;
    let metadata = file.metadata().map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("Cannot open a directory as a file.".to_string());
    }
    if metadata.len() > codux_protocol::REMOTE_BLOB_MAX_BYTES as u64 {
        return Err("Blob size is not supported.".to_string());
    }
    let mut bytes = Vec::with_capacity(metadata.len() as usize);
    file.take(codux_protocol::REMOTE_BLOB_MAX_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    if bytes.len() > codux_protocol::REMOTE_BLOB_MAX_BYTES {
        return Err("Blob size is not supported.".to_string());
    }
    Ok(bytes)
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
    if destination.exists() && !crate::path::local_paths_equal(&source, &destination) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn drives_sentinel_returns_volume_listing() {
        let payload = file_list_payload(Some(FILE_LIST_DRIVES_SENTINEL), Some("projectFiles"));
        assert_eq!(payload["path"], FILE_LIST_DRIVES_SENTINEL);
        assert_eq!(payload["purpose"], "projectFiles");
        let entries = payload["entries"].as_array().expect("entries array");
        assert!(!entries.is_empty(), "expected at least one volume entry");
        for entry in entries {
            assert_eq!(entry["isDirectory"], Value::Bool(true));
        }
    }

    #[test]
    fn ssh_key_listing_includes_hidden_entries() {
        let dir = std::env::temp_dir().join(format!(
            "codux-runtime-core-hidden-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        let ssh_dir = dir.join(".ssh");
        fs::create_dir_all(&ssh_dir).expect("ssh dir");

        let project_payload = file_list_payload(dir.to_str(), Some("projectFiles"));
        let project_entries = project_payload["entries"]
            .as_array()
            .expect("project entries");
        assert!(
            !project_entries
                .iter()
                .any(|entry| entry["name"].as_str() == Some(".ssh"))
        );

        let ssh_payload = file_list_payload(dir.to_str(), Some("sshKey"));
        let ssh_entries = ssh_payload["entries"].as_array().expect("ssh entries");
        assert!(
            ssh_entries
                .iter()
                .any(|entry| entry["name"].as_str() == Some(".ssh"))
        );

        fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn blob_read_rejects_missing_directory_and_oversized_files() {
        let dir = std::env::temp_dir().join(format!(
            "codux-runtime-core-blob-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .expect("clock")
                .as_nanos()
        ));
        fs::create_dir_all(&dir).expect("blob test dir");
        let oversized = dir.join("oversized.bin");
        let file = fs::File::create(&oversized).expect("oversized file");
        file.set_len(codux_protocol::REMOTE_BLOB_MAX_BYTES as u64 + 1)
            .expect("oversized length");

        assert_eq!(
            file_read_blob_bytes("").unwrap_err(),
            "File path is required."
        );
        assert_eq!(
            file_read_blob_bytes(dir.to_str().unwrap()).unwrap_err(),
            "Cannot open a directory as a file."
        );
        assert_eq!(
            file_read_blob_bytes(oversized.to_str().unwrap()).unwrap_err(),
            "Blob size is not supported."
        );

        fs::remove_dir_all(dir).ok();
    }
}
