use std::{
    fs,
    path::{Path, PathBuf},
};

mod copy;
mod entry;
mod path;
mod system;
#[cfg(test)]
mod tests;
mod types;
mod watch;
use copy::*;
use entry::*;
use path::*;
use system::*;
pub use types::*;
pub use watch::*;

const LARGE_TEXT_SAMPLE_BYTES: usize = 96 * 1024;
const MAX_TEXT_READ_BYTES: u64 = 100 * 1024 * 1024;

pub fn file_list_children(request: FileChildrenRequest) -> Result<Vec<FileEntry>, String> {
    FilesService::list_children(&request.root_path, request.directory_path.as_deref())
}

pub fn file_read(request: FilePathRequest) -> Result<FileReadResult, String> {
    FilesService::read_text(&request.root_path, &request.path)
}

pub fn file_write(request: FileWriteRequest) -> Result<FileReadResult, String> {
    FilesService::write_text(&request.root_path, &request.path, &request.content)
}

pub fn file_create_file(request: FileCreateRequest) -> Result<FileEntry, String> {
    FilesService::create_file(
        &request.root_path,
        request.parent_path.as_deref(),
        &request.name,
    )
}

pub fn file_create_dir(request: FileCreateRequest) -> Result<FileEntry, String> {
    FilesService::create_dir(
        &request.root_path,
        request.parent_path.as_deref(),
        &request.name,
    )
}

pub fn file_rename(request: FileRenameRequest) -> Result<FileEntry, String> {
    FilesService::rename(&request.root_path, &request.path, &request.new_name)
}

pub fn file_delete(request: FilePathRequest) -> Result<(), String> {
    FilesService::delete(&request.root_path, &request.path)
}

pub fn file_copy(request: FileCopyRequest) -> Result<FileEntry, String> {
    FilesService::copy_to_directory(
        &request.root_path,
        &request.source_path,
        request.target_directory_path.as_deref(),
    )
}

pub fn file_move(request: FileMoveRequest) -> Result<FileEntry, String> {
    FilesService::move_to_directory(
        &request.root_path,
        &request.source_path,
        &request.target_directory_path,
    )
}

pub fn file_import_external(request: FileExternalCopyRequest) -> Result<Vec<FileEntry>, String> {
    let root = canonical_root(&request.root_path)?;
    let target_directory = match request.target_directory_path.as_deref().and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    request
        .source_paths
        .iter()
        .map(|source| {
            let source = PathBuf::from(source);
            if !source.exists() {
                return Err(format!("Source path does not exist: {}", source.display()));
            }
            copy_external_entry_to_directory(&root, &source, &target_directory)
        })
        .collect()
}

pub fn file_write_bytes(request: FileBytesWriteRequest) -> Result<FileEntry, String> {
    let root = canonical_root(&request.root_path)?;
    let target_directory = match request.target_directory_path.as_deref().and_then(non_empty) {
        Some(path) => resolve_existing_path(&root, path)?,
        None => root.clone(),
    };
    write_bytes_to_directory(
        &root,
        &target_directory,
        &clean_child_name(&request.file_name)?,
        &request.bytes,
    )
}

pub fn file_reveal(request: FilePathRequest) -> Result<(), String> {
    FilesService::reveal(&request.root_path, &request.path)
}

pub fn file_open(request: FilePathRequest) -> Result<(), String> {
    FilesService::open_path(&request.root_path, &request.path)
}

pub struct FilesService;

impl FilesService {
    pub fn list_children(
        root_path: &str,
        directory_path: Option<&str>,
    ) -> Result<Vec<FileEntry>, String> {
        let root = canonical_root(root_path)?;
        let directory = match directory_path.and_then(non_empty) {
            Some(path) => resolve_existing_path(&root, path)?,
            None => root.clone(),
        };
        if !directory.is_dir() {
            return Err("Path is not a directory.".to_string());
        }

        let mut entries = fs::read_dir(&directory)
            .map_err(|error| error.to_string())?
            .filter_map(Result::ok)
            .filter_map(|entry| file_entry(&root, entry.path()).ok())
            .filter(|entry| !should_hide_entry(&entry.name))
            .collect::<Vec<_>>();

        entries.sort_by(|a, b| match (&a.kind, &b.kind) {
            (FileKind::Directory, FileKind::File | FileKind::Symlink) => std::cmp::Ordering::Less,
            (FileKind::File | FileKind::Symlink, FileKind::Directory) => {
                std::cmp::Ordering::Greater
            }
            _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
        });
        Ok(entries)
    }

    pub fn read_text(root_path: &str, path: &str) -> Result<FileReadResult, String> {
        let root = canonical_root(root_path)?;
        let target = resolve_existing_project_entry(&root, path)?;
        if !target.is_file() {
            return Err("Path is not a file.".to_string());
        }

        let metadata = fs::metadata(&target).map_err(|error| error.to_string())?;
        let size = metadata.len();
        if size > MAX_TEXT_READ_BYTES {
            return Ok(file_read_result(
                &root,
                &target,
                String::new(),
                true,
                true,
                Some("File is too large to read in the GPUI preview.".to_string()),
            ));
        }

        let bytes = fs::read(&target).map_err(|error| error.to_string())?;
        let is_binary = bytes.contains(&0);
        if is_binary {
            return Ok(file_read_result(
                &root,
                &target,
                String::new(),
                true,
                false,
                Some("Binary file preview is not supported yet.".to_string()),
            ));
        }

        let is_truncated = bytes.len() > LARGE_TEXT_SAMPLE_BYTES;
        let sample = if is_truncated {
            &bytes[..LARGE_TEXT_SAMPLE_BYTES]
        } else {
            &bytes
        };
        let content = String::from_utf8_lossy(sample).to_string();
        Ok(file_read_result(
            &root,
            &target,
            content,
            false,
            is_truncated,
            None,
        ))
    }

    pub fn write_text(
        root_path: &str,
        path: &str,
        content: &str,
    ) -> Result<FileReadResult, String> {
        let root = canonical_root(root_path)?;
        let target = resolve_existing_path(&root, path)?;
        if target.is_dir() {
            return Err("Folders cannot be saved as text files.".to_string());
        }
        fs::write(&target, content).map_err(|error| error.to_string())?;
        Self::read_text(root_path, path)
    }

    pub fn create_file(
        root_path: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<FileEntry, String> {
        let path = resolve_new_child_path(root_path, parent_path, name)?;
        fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&path)
            .map_err(|error| error.to_string())?;
        let root = canonical_root(root_path)?;
        file_entry(&root, path)
    }

    pub fn create_dir(
        root_path: &str,
        parent_path: Option<&str>,
        name: &str,
    ) -> Result<FileEntry, String> {
        let path = resolve_new_child_path(root_path, parent_path, name)?;
        fs::create_dir(&path).map_err(|error| error.to_string())?;
        let root = canonical_root(root_path)?;
        file_entry(&root, path)
    }

    pub fn delete(root_path: &str, path: &str) -> Result<(), String> {
        let root = canonical_root(root_path)?;
        let target = resolve_existing_path(&root, path)?;
        if target == root {
            return Err("Project root cannot be deleted.".to_string());
        }
        move_to_trash(&target)
    }

    pub fn rename(root_path: &str, path: &str, new_name: &str) -> Result<FileEntry, String> {
        let root = canonical_root(root_path)?;
        let source = resolve_existing_path(&root, path)?;
        let destination = source
            .parent()
            .ok_or_else(|| "Cannot rename project root.".to_string())?
            .join(clean_child_name(new_name)?);
        ensure_within_root(&root, &destination)?;
        if destination.exists() {
            return Err("A file with this name already exists.".to_string());
        }
        fs::rename(&source, &destination).map_err(|error| error.to_string())?;
        file_entry(&root, destination)
    }

    pub fn move_to_directory(
        root_path: &str,
        path: &str,
        target_directory_path: &str,
    ) -> Result<FileEntry, String> {
        Self::move_to_directory_with_options(root_path, path, target_directory_path, false)
    }

    pub fn move_to_directory_overwrite(
        root_path: &str,
        path: &str,
        target_directory_path: &str,
    ) -> Result<FileEntry, String> {
        Self::move_to_directory_with_options(root_path, path, target_directory_path, true)
    }

    fn move_to_directory_with_options(
        root_path: &str,
        path: &str,
        target_directory_path: &str,
        overwrite: bool,
    ) -> Result<FileEntry, String> {
        let root = canonical_root(root_path)?;
        let source = resolve_existing_path(&root, path)?;
        let target_directory = resolve_existing_path(&root, target_directory_path)?;
        if source == root {
            return Err("Project root cannot be moved.".to_string());
        }
        if !target_directory.is_dir() {
            return Err("Target path is not a directory.".to_string());
        }
        if source.is_dir() && target_directory.starts_with(&source) {
            return Err("Cannot move a directory into itself.".to_string());
        }
        let file_name = source
            .file_name()
            .ok_or_else(|| "Cannot move project root.".to_string())?;
        let destination = target_directory.join(file_name);
        ensure_within_root(&root, &destination)?;
        if source == destination {
            return Ok(file_entry(&root, source)?);
        }
        if destination.exists() {
            if !overwrite {
                return Err("A file with this name already exists.".to_string());
            }
            if destination.is_dir() {
                fs::remove_dir_all(&destination).map_err(|error| error.to_string())?;
            } else {
                fs::remove_file(&destination).map_err(|error| error.to_string())?;
            }
        }
        fs::rename(&source, &destination).map_err(|error| error.to_string())?;
        file_entry(&root, destination)
    }

    pub fn copy_to_directory(
        root_path: &str,
        source_path: &str,
        target_directory_path: Option<&str>,
    ) -> Result<FileEntry, String> {
        let root = canonical_root(root_path)?;
        let source = resolve_existing_path(&root, source_path)?;
        let target_directory = match target_directory_path.and_then(non_empty) {
            Some(path) => resolve_existing_path(&root, path)?,
            None => source
                .parent()
                .map(Path::to_path_buf)
                .unwrap_or_else(|| root.clone()),
        };
        copy_entry_to_directory(&root, &source, &target_directory)
    }

    pub fn write_bytes_to_directory(
        root_path: &str,
        target_directory_path: Option<&str>,
        file_name: &str,
        bytes: &[u8],
    ) -> Result<FileEntry, String> {
        file_write_bytes(FileBytesWriteRequest {
            root_path: root_path.to_string(),
            target_directory_path: target_directory_path.map(str::to_string),
            file_name: file_name.to_string(),
            bytes: bytes.to_vec(),
        })
    }

    pub fn reveal(root_path: &str, path: &str) -> Result<(), String> {
        let root = canonical_root(root_path)?;
        let target = resolve_existing_path(&root, path)?;
        reveal_path(&target)
    }

    pub fn open_path(root_path: &str, path: &str) -> Result<(), String> {
        let root = canonical_root(root_path)?;
        let target = resolve_existing_path(&root, path)?;
        open_path(&target)
    }
}
