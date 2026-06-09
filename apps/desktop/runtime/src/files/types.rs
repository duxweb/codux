use serde::{Deserialize, Serialize};

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
pub struct FileMoveRequest {
    pub root_path: String,
    pub source_path: String,
    pub target_directory_path: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileExternalCopyRequest {
    pub root_path: String,
    pub source_paths: Vec<String>,
    pub target_directory_path: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileBytesWriteRequest {
    pub root_path: String,
    pub target_directory_path: Option<String>,
    pub file_name: String,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub path: String,
    pub relative_path: String,
    pub name: String,
    pub kind: FileKind,
    pub is_directory: bool,
    pub is_symbolic_link: bool,
    pub size: u64,
    pub modified_at: i64,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Directory,
    File,
    Symlink,
}

#[derive(Clone, Debug, Serialize)]
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
