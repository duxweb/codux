use super::entry::file_entry;
use super::path::ensure_within_root;
use super::types::FileEntry;
use std::{
    fs,
    path::{Path, PathBuf},
};

pub(super) fn copy_entry_to_directory(
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
    copy_entry(source, &destination)?;
    file_entry(root, destination)
}

pub(super) fn copy_external_entry_to_directory(
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
    let destination = next_available_child_path(target_directory, file_name);
    ensure_within_root(root, &destination)?;
    copy_entry(source, &destination)?;
    file_entry(root, destination)
}

pub(super) fn write_bytes_to_directory(
    root: &Path,
    target_directory: &Path,
    file_name: &str,
    bytes: &[u8],
) -> Result<FileEntry, String> {
    ensure_within_root(root, target_directory)?;
    if !target_directory.is_dir() {
        return Err("Target path is not a directory.".to_string());
    }
    let destination = next_available_child_path(target_directory, file_name);
    ensure_within_root(root, &destination)?;
    fs::write(&destination, bytes).map_err(|error| error.to_string())?;
    file_entry(root, destination)
}

fn copy_entry(source: &Path, destination: &Path) -> Result<(), String> {
    if source.is_dir() {
        copy_directory_recursive(source, destination)
    } else {
        fs::copy(source, destination)
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
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
