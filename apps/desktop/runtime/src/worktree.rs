mod git_ops;
mod scan;
mod snapshot;
mod state;
#[cfg(test)]
mod tests;
mod types;

pub use types::*;

use crate::git::GitService;
use git_ops::*;
use scan::{ScannedWorktreeSnapshot, scan_git_worktrees};
use serde_json::{Map, Value};
use snapshot::{
    project_worktree_git_summary, project_worktree_snapshot, scanned_task_to_snapshot,
    scanned_worktree_to_snapshot,
};
use state::{
    StateFile, WorktreeRecord, WorktreeTaskRecord, enrich_scanned_snapshot_from_state,
    merge_worktree_snapshot, raw_snapshot, save_raw_snapshot, selected_worktree_id_from_state,
};
use std::{
    fs,
    path::{Path, PathBuf},
};

type GitRepository = git2::Repository;
const WORKTREE_GIT_SUMMARY_NAMESPACE: &str = "worktree-git-summary";

pub struct WorktreeService {
    support_dir: PathBuf,
    state_file: PathBuf,
}

impl WorktreeService {
    pub fn new(support_dir: PathBuf) -> Self {
        Self {
            state_file: crate::config::state_file_path(&support_dir),
            support_dir,
        }
    }
}

include!("worktree/service_summary.rs");
include!("worktree/service_state.rs");
include!("worktree/service_snapshot.rs");
include!("worktree/service_operations.rs");
