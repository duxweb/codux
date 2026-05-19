use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use std::time::UNIX_EPOCH;
use tauri::{AppHandle, Emitter};

const MAX_TEXT_READ_BYTES: u64 = 100 * 1024 * 1024;
const LARGE_TEXT_SAMPLE_BYTES: usize = 96 * 1024;

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChildrenRequest {
    pub root_path: String,
    pub directory_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePathRequest {
    pub root_path: String,
    pub path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWriteRequest {
    pub root_path: String,
    pub path: String,
    pub content: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCreateRequest {
    pub root_path: String,
    pub parent_path: Option<String>,
    pub name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRenameRequest {
    pub root_path: String,
    pub path: String,
    pub new_name: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileCopyRequest {
    pub root_path: String,
    pub source_path: String,
    pub target_directory_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileExternalCopyRequest {
    pub root_path: String,
    pub source_paths: Vec<String>,
    pub target_directory_path: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub relative_path: String,
    pub name: String,
    pub is_directory: bool,
    pub is_symbolic_link: bool,
    pub size: u64,
    pub modified_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileReadResult {
    pub path: String,
    pub relative_path: String,
    pub name: String,
    pub content: String,
    pub size: u64,
    pub modified_at: i64,
    pub is_binary: bool,
    pub is_large: bool,
    pub is_truncated: bool,
    pub read_only: bool,
    pub message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileWatchRegistration {
    pub project_path: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileChangeEvent {
    pub project_path: String,
    pub changed_paths: Vec<String>,
}

pub struct FileWatchManager {
    watchers: Mutex<HashMap<String, FileProjectWatcher>>,
}

struct FileProjectWatcher {
    _watcher: RecommendedWatcher,
    _project_path: String,
    ref_count: usize,
}

impl Default for FileWatchManager {
    fn default() -> Self {
        Self {
            watchers: Mutex::new(HashMap::new()),
        }
    }
}

impl FileWatchManager {
    pub fn watch(
        &self,
        app: AppHandle,
        project_path: String,
    ) -> Result<FileWatchRegistration, String> {
        let root = canonical_root(&project_path)?;
        let root_key = normalized_path_key(&root);
        let normalized_project_path = normalized_path_display(&root);
        let registration = FileWatchRegistration {
            project_path: normalized_project_path.clone(),
        };

        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "File watcher lock is poisoned.".to_string())?;
        if watchers.contains_key(&root_key) {
            if let Some(existing) = watchers.get_mut(&root_key) {
                existing.ref_count = existing.ref_count.saturating_add(1);
            }
            return Ok(registration);
        }

        let app_handle = app.clone();
        let root_key_for_event = root_key.clone();
        let project_path_for_event = normalized_project_path.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else {
                return;
            };
            let changed_paths = event
                .paths
                .iter()
                .filter_map(|path| {
                    let key = normalized_path_key(path);
                    should_forward_file_watch_path(&root_key_for_event, &key)
                        .then(|| normalized_path_display(path))
                })
                .collect::<Vec<_>>();
            if changed_paths.is_empty() {
                return;
            }
            let _ = app_handle.emit(
                "file:changed",
                FileChangeEvent {
                    project_path: project_path_for_event.clone(),
                    changed_paths,
                },
            );
        })
        .map_err(|error| error.to_string())?;

        watcher
            .watch(&root, RecursiveMode::Recursive)
            .map_err(|error| error.to_string())?;
        watchers.insert(
            root_key,
            FileProjectWatcher {
                _watcher: watcher,
                _project_path: normalized_project_path,
                ref_count: 1,
            },
        );
        Ok(registration)
    }

    pub fn unwatch(&self, project_path: String) -> Result<(), String> {
        let key = canonical_root(&project_path)
            .map(|root| normalized_path_key(&root))
            .unwrap_or_else(|_| normalized_path_key(Path::new(project_path.trim())));
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "File watcher lock is poisoned.".to_string())?;
        if let Some(existing) = watchers.get_mut(&key) {
            if existing.ref_count > 1 {
                existing.ref_count -= 1;
                return Ok(());
            }
        }
        watchers.remove(&key);
        Ok(())
    }
}

pub fn file_list_children(request: FileChildrenRequest) -> Result<Vec<FileEntry>, String> {
    let root = canonical_root(&request.root_path)?;
    let directory = match request.directory_path.as_deref().and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    if !directory.is_dir() {
        return Err("Selected path is not a directory.".to_string());
    }

    let mut entries = fs::read_dir(&directory)
        .map_err(|error| error.to_string())?
        .filter_map(|entry| entry.ok())
        .filter(|entry| entry.file_name().to_string_lossy() != ".git")
        .filter_map(|entry| file_entry(&root, entry.path()).ok())
        .collect::<Vec<_>>();
    entries.sort_by(compare_entries);
    Ok(entries)
}

pub fn file_read(request: FilePathRequest) -> Result<FileReadResult, String> {
    let root = canonical_root(&request.root_path)?;
    let path = resolve_existing_path(&root, &request.path)?;
    if path.is_dir() {
        return Err("Folders cannot be opened in the text editor.".to_string());
    }

    let metadata = fs::metadata(&path).map_err(|error| error.to_string())?;
    let size = metadata.len();
    let is_large = size > MAX_TEXT_READ_BYTES;
    let read_limit = if is_large {
        LARGE_TEXT_SAMPLE_BYTES
    } else {
        size.min(MAX_TEXT_READ_BYTES) as usize
    };
    let mut data = fs::read(&path).map_err(|error| error.to_string())?;
    if data.len() > read_limit {
        data.truncate(read_limit);
    }

    let relative_path = relative_path(&root, &path);
    let name = path_name(&path);
    if data.contains(&0) {
        return Ok(FileReadResult {
            path: path.display().to_string(),
            relative_path,
            name,
            content: String::new(),
            size,
            modified_at: modified_at(&metadata),
            is_binary: true,
            is_large,
            is_truncated: is_large,
            read_only: true,
            message: Some("Binary files cannot be edited here.".to_string()),
        });
    }

    let Some(content) = decode_text(&data) else {
        return Ok(FileReadResult {
            path: path.display().to_string(),
            relative_path,
            name,
            content: String::new(),
            size,
            modified_at: modified_at(&metadata),
            is_binary: true,
            is_large,
            is_truncated: is_large,
            read_only: true,
            message: Some("This file is not valid UTF text.".to_string()),
        });
    };

    Ok(FileReadResult {
        path: path.display().to_string(),
        relative_path,
        name,
        content,
        size,
        modified_at: modified_at(&metadata),
        is_binary: false,
        is_large,
        is_truncated: is_large,
        read_only: is_large,
        message: is_large.then(|| "Large file preview is read-only.".to_string()),
    })
}

pub fn file_write(request: FileWriteRequest) -> Result<FileReadResult, String> {
    let root = canonical_root(&request.root_path)?;
    let path = resolve_existing_path(&root, &request.path)?;
    if path.is_dir() {
        return Err("Folders cannot be saved as text files.".to_string());
    }
    fs::write(&path, request.content).map_err(|error| error.to_string())?;
    file_read(FilePathRequest {
        root_path: root.display().to_string(),
        path: path.display().to_string(),
    })
}

pub fn file_create_file(request: FileCreateRequest) -> Result<FileEntry, String> {
    let path = resolve_new_child_path(
        &request.root_path,
        request.parent_path.as_deref(),
        &request.name,
    )?;
    fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&path)
        .map_err(|error| error.to_string())?;
    let root = canonical_root(&request.root_path)?;
    file_entry(&root, path)
}

pub fn file_create_dir(request: FileCreateRequest) -> Result<FileEntry, String> {
    let path = resolve_new_child_path(
        &request.root_path,
        request.parent_path.as_deref(),
        &request.name,
    )?;
    fs::create_dir(&path).map_err(|error| error.to_string())?;
    let root = canonical_root(&request.root_path)?;
    file_entry(&root, path)
}

pub fn file_rename(request: FileRenameRequest) -> Result<FileEntry, String> {
    let root = canonical_root(&request.root_path)?;
    let source = resolve_existing_path(&root, &request.path)?;
    let name = clean_child_name(&request.new_name)?;
    let destination = source
        .parent()
        .ok_or_else(|| "Cannot rename project root.".to_string())?
        .join(name);
    if destination.exists() {
        return Err("A file with this name already exists.".to_string());
    }
    ensure_within_root(&root, &destination)?;
    fs::rename(&source, &destination).map_err(|error| error.to_string())?;
    file_entry(&root, destination)
}

pub fn file_delete(request: FilePathRequest) -> Result<(), String> {
    let root = canonical_root(&request.root_path)?;
    let path = resolve_existing_path(&root, &request.path)?;
    if path == root {
        return Err("Project root cannot be deleted.".to_string());
    }
    move_to_trash(&path)
}

pub fn file_copy(request: FileCopyRequest) -> Result<FileEntry, String> {
    let root = canonical_root(&request.root_path)?;
    let source = resolve_existing_path(&root, &request.source_path)?;
    let target_directory = match request.target_directory_path.as_deref().and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    copy_entry_to_directory(&root, &source, &target_directory)
}

pub fn file_import_external(request: FileExternalCopyRequest) -> Result<Vec<FileEntry>, String> {
    let root = canonical_root(&request.root_path)?;
    let target_directory = match request.target_directory_path.as_deref().and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    let mut entries = Vec::new();
    for source in request.source_paths {
        let source = PathBuf::from(source.trim());
        if !source.exists() {
            return Err(format!("Source path does not exist: {}", source.display()));
        }
        entries.push(copy_entry_to_directory(&root, &source, &target_directory)?);
    }
    Ok(entries)
}

pub fn file_reveal(request: FilePathRequest) -> Result<(), String> {
    let root = canonical_root(&request.root_path)?;
    let path = resolve_existing_path(&root, &request.path)?;
    reveal_path(&path)
}

pub fn file_open(request: FilePathRequest) -> Result<(), String> {
    let root = canonical_root(&request.root_path)?;
    let path = resolve_existing_path(&root, &request.path)?;
    tauri_plugin_opener::open_path(path, None::<&str>).map_err(|error| error.to_string())
}

fn copy_entry_to_directory(
    root: &Path,
    source: &Path,
    target_directory: &Path,
) -> Result<FileEntry, String> {
    ensure_within_root(root, target_directory)?;
    if !target_directory.is_dir() {
        return Err("Target path is not a directory.".to_string());
    }
    let file_name = source
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Source path has no file name.".to_string())?;
    if source.is_dir() && target_directory.starts_with(source) {
        return Err("Cannot copy a folder into itself.".to_string());
    }
    let destination = next_available_child_path(target_directory, file_name);
    ensure_within_root(root, &destination)?;
    if source.is_dir() {
        copy_directory_recursive(source, &destination)?;
    } else {
        fs::copy(source, &destination).map_err(|error| error.to_string())?;
    }
    file_entry(root, destination)
}

fn copy_directory_recursive(source: &Path, destination: &Path) -> Result<(), String> {
    fs::create_dir(destination).map_err(|error| error.to_string())?;
    for entry in fs::read_dir(source).map_err(|error| error.to_string())? {
        let entry = entry.map_err(|error| error.to_string())?;
        let source_child = entry.path();
        let destination_child = destination.join(entry.file_name());
        if source_child.is_dir() {
            copy_directory_recursive(&source_child, &destination_child)?;
        } else {
            fs::copy(&source_child, &destination_child).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

fn next_available_child_path(directory: &Path, name: &str) -> PathBuf {
    let candidate = directory.join(name);
    if !candidate.exists() {
        return candidate;
    }
    let source = Path::new(name);
    let stem = source
        .file_stem()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .unwrap_or(name);
    let extension = source.extension().and_then(|value| value.to_str());
    for index in 1.. {
        let next_name = match extension {
            Some(extension) => format!("{stem} copy {index}.{extension}"),
            None => format!("{stem} copy {index}"),
        };
        let next = directory.join(next_name);
        if !next.exists() {
            return next;
        }
    }
    candidate
}

fn file_entry(root: &Path, path: PathBuf) -> Result<FileEntry, String> {
    ensure_within_root(root, &path)?;
    let symlink_metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    let metadata = fs::metadata(&path).unwrap_or_else(|_| symlink_metadata.clone());
    Ok(FileEntry {
        name: path_name(&path),
        relative_path: relative_path(root, &path),
        path: path.display().to_string(),
        is_directory: metadata.is_dir(),
        is_symbolic_link: symlink_metadata.file_type().is_symlink(),
        size: metadata.len(),
        modified_at: modified_at(&metadata),
    })
}

fn modified_at(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

fn compare_entries(left: &FileEntry, right: &FileEntry) -> Ordering {
    right
        .is_directory
        .cmp(&left.is_directory)
        .then_with(|| left.name.to_lowercase().cmp(&right.name.to_lowercase()))
        .then_with(|| left.name.cmp(&right.name))
}

fn canonical_root(value: &str) -> Result<PathBuf, String> {
    let root = PathBuf::from(value.trim());
    if !root.exists() {
        return Err(format!("Project path does not exist: {}", root.display()));
    }
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    if !root.is_dir() {
        return Err("Project path is not a directory.".to_string());
    }
    Ok(root)
}

fn resolve_existing_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    let raw = raw_path(root, value);
    let path = raw.canonicalize().map_err(|error| error.to_string())?;
    ensure_within_root(root, &path)?;
    Ok(path)
}

fn resolve_new_child_path(
    root_path: &str,
    parent_path: Option<&str>,
    name: &str,
) -> Result<PathBuf, String> {
    let root = canonical_root(root_path)?;
    let parent = match parent_path.and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    if !parent.is_dir() {
        return Err("Target path is not a directory.".to_string());
    }
    let child = parent.join(clean_child_name(name)?);
    ensure_within_root(&root, &child)?;
    if child.exists() {
        return Err("A file with this name already exists.".to_string());
    }
    Ok(child)
}

fn raw_path(root: &Path, value: &str) -> PathBuf {
    let path = PathBuf::from(value.trim());
    if path.is_absolute() {
        path
    } else if value.trim().is_empty() {
        root.to_path_buf()
    } else {
        root.join(path)
    }
}

fn ensure_within_root(root: &Path, path: &Path) -> Result<(), String> {
    if path == root || path.starts_with(root) {
        return Ok(());
    }
    Err("Path is outside the current project.".to_string())
}

fn clean_child_name(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value.contains('/') || value.contains('\\') {
        return Err("Enter a valid file name.".to_string());
    }
    Ok(value.to_string())
}

fn relative_path(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .ok()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(|value| value.to_string())
        .unwrap_or_else(|| path_name(path))
}

fn path_name(path: &Path) -> String {
    path.file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .unwrap_or("Untitled")
        .to_string()
}

fn non_empty(value: &str) -> Option<&str> {
    let value = value.trim();
    (!value.is_empty()).then_some(value)
}

fn normalized_path_display(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalized_path_key(path: &Path) -> String {
    normalized_path_display(path)
        .trim_end_matches('/')
        .to_lowercase()
}

fn should_forward_file_watch_path(root_key: &str, path_key: &str) -> bool {
    if path_key.is_empty()
        || path_key.contains("/.git/")
        || path_key.ends_with("/.git")
        || path_key.contains("/node_modules/")
        || path_key.ends_with("/node_modules")
        || path_key.contains("/target/")
        || path_key.ends_with("/target")
    {
        return false;
    }
    path_key == root_key || path_key.starts_with(&format!("{root_key}/"))
}

fn decode_text(data: &[u8]) -> Option<String> {
    if data.is_empty() {
        return Some(String::new());
    }
    if let Ok(value) = String::from_utf8(data.to_vec()) {
        return Some(value);
    }
    if data.len() % 2 != 0 {
        return None;
    }
    let units = data
        .chunks_exact(2)
        .map(|chunk| u16::from_le_bytes([chunk[0], chunk[1]]))
        .collect::<Vec<_>>();
    String::from_utf16(&units).ok()
}

fn reveal_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return run_reveal_command("open", &["-R", &path.display().to_string()]);
    }
    #[cfg(target_os = "windows")]
    {
        return run_reveal_command("explorer", &["/select,", &path.display().to_string()]);
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let parent = path.parent().unwrap_or(path);
        return run_reveal_command("xdg-open", &[&parent.display().to_string()]);
    }
}

fn move_to_trash(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            apple_script_string(path)
        );
        return run_command_status("osascript", &["-e", &script], "move item to Trash");
    }

    #[cfg(target_os = "windows")]
    {
        let target = powershell_string(path);
        let action = if path.is_dir() {
            format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory({target}, 'OnlyErrorDialogs', 'SendToRecycleBin')"
            )
        } else {
            format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile({target}, 'OnlyErrorDialogs', 'SendToRecycleBin')"
            )
        };
        return run_command_status(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", &action],
            "move item to Recycle Bin",
        );
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        if run_command_status(
            "gio",
            &["trash", &path.display().to_string()],
            "move item to Trash",
        )
        .is_ok()
        {
            return Ok(());
        }
        if run_command_status(
            "kioclient6",
            &["move", &path.display().to_string(), "trash:/"],
            "move item to Trash",
        )
        .is_ok()
        {
            return Ok(());
        }
        return run_command_status(
            "kioclient5",
            &["move", &path.display().to_string(), "trash:/"],
            "move item to Trash",
        );
    }
}

#[cfg(target_os = "macos")]
fn apple_script_string(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn powershell_string(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn run_command_status(program: &str, args: &[&str], action: &str) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("Unable to {action}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("Unable to {action}."))
    } else {
        Err(stderr)
    }
}

fn run_reveal_command(program: &str, args: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
