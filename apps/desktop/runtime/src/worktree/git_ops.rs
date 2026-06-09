use std::{
    fs,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use uuid::Uuid;

use super::GitRepository;

#[derive(Clone, Debug)]
pub(super) struct GitWorktreeEntry {
    pub path: String,
    pub branch: String,
    pub head: String,
    pub detached: bool,
    pub bare: bool,
}

include!("git_ops/discovery.rs");
include!("git_ops/worktrees.rs");
include!("git_ops/branches.rs");
include!("git_ops/helpers.rs");
