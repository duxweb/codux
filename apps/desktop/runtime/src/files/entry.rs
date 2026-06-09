use super::MAX_TEXT_READ_BYTES;
use super::path::{modified_at, normalized_path_display, relative_path};
use super::types::{FileEntry, FileKind, FileReadResult};
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn file_entry(root: &Path, path: PathBuf) -> Result<FileEntry, String> {
    let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
    let file_type = metadata.file_type();
    let kind = if file_type.is_symlink() {
        FileKind::Symlink
    } else if file_type.is_dir() {
        FileKind::Directory
    } else {
        FileKind::File
    };
    Ok(FileEntry {
        path: normalized_path_display(&path),
        relative_path: relative_path(root, &path),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| normalized_path_display(&path)),
        is_directory: matches!(kind, FileKind::Directory),
        is_symbolic_link: matches!(kind, FileKind::Symlink),
        kind,
        size: metadata.len(),
        modified_at: modified_at(&metadata),
    })
}

pub(super) fn file_read_result(
    root: &Path,
    path: &Path,
    content: String,
    is_binary: bool,
    is_truncated: bool,
    message: Option<String>,
) -> FileReadResult {
    let metadata = fs::metadata(path).ok();
    FileReadResult {
        path: normalized_path_display(path),
        relative_path: relative_path(root, path),
        name: path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| normalized_path_display(path)),
        content,
        size: metadata
            .as_ref()
            .map(|metadata| metadata.len())
            .unwrap_or(0),
        modified_at: metadata.as_ref().map(modified_at).unwrap_or(0),
        is_binary,
        is_large: metadata
            .as_ref()
            .map(|metadata| metadata.len() > MAX_TEXT_READ_BYTES)
            .unwrap_or(false),
        is_truncated,
        read_only: metadata
            .as_ref()
            .map(|metadata| metadata.permissions().readonly())
            .unwrap_or(false),
        message,
    }
}
