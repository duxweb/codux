//! Worktree listing for the headless host via the `git worktree` CLI. The
//! desktop has a richer WorktreeService (tasks, default/selected state in
//! state.json); the agent reports the real git worktrees so a controller's
//! worktree panel populates. Mutations (create/merge/remove) route through the
//! shared `codux_git::worktree` engine — the same git2 implementation + managed
//! path convention the desktop uses — so the two hosts can't drift on them.

use codux_runtime_core::worktree::{
    RuntimeWorktreeItem, WorktreeSummaryPayload, default_worktree_base_branch,
    selected_runtime_worktree_id, worktree_base_branches, worktree_display_name,
    worktree_summary_payload, worktree_uuid,
};
use serde_json::{Value, json};
use std::path::Path;
use std::process::Command;

/// A scanned git worktree before it is mapped to the wire shape.
struct ScannedEntry {
    path: String,
    branch: String,
    is_default: bool,
}

/// A `worktree.list` reply: the project's real git worktrees, mapped through the
/// shared `worktree_summary_payload` so the wire shape (ids, base branches,
/// selection) matches the desktop host exactly.
pub fn worktree_list_payload(project_id: &str, project_path: &str) -> Value {
    let scanned = scan_worktrees(project_path);
    let entries: Vec<Value> = scanned
        .iter()
        .map(|entry| worktree_entry(project_id, entry))
        .collect();
    let items: Vec<RuntimeWorktreeItem> = scanned
        .iter()
        .map(|entry| RuntimeWorktreeItem {
            id: entry_id(project_id, entry),
            project_id: project_id.to_string(),
            path: entry.path.clone(),
            is_default: entry.is_default,
            exists: Path::new(&entry.path).exists(),
        })
        .collect();
    let selected = selected_runtime_worktree_id(project_id, None, &items);
    // Base branches come from the project's git branches (same source the
    // desktop host uses), so the worktree create dialog offers real options.
    let status = codux_git::wire::status(project_path);
    let base_branches = worktree_base_branches(&status.branch, &status.branches);
    let default_base_branch = default_worktree_base_branch(&status.branch, &status.branches);
    worktree_summary_payload(WorktreeSummaryPayload {
        project_id: project_id.to_string(),
        selected_worktree_id: selected,
        worktrees: Value::Array(entries),
        tasks: json!([]),
        available: true,
        base_branches,
        default_base_branch,
        error: None,
    })
}

/// Scan the project's real git worktrees via `git worktree list --porcelain`.
fn scan_worktrees(project_path: &str) -> Vec<ScannedEntry> {
    let mut worktrees = Vec::new();
    let Ok(output) = Command::new("git")
        .arg("-C")
        .arg(project_path)
        .args(["worktree", "list", "--porcelain"])
        .output()
    else {
        return worktrees;
    };
    if !output.status.success() {
        return worktrees;
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut path: Option<String> = None;
    let mut branch = String::new();
    let flush = |worktrees: &mut Vec<ScannedEntry>, path: Option<String>, branch: &str| {
        if let Some(path) = path {
            let is_default = worktrees.is_empty();
            worktrees.push(ScannedEntry {
                path,
                branch: branch.to_string(),
                is_default,
            });
        }
    };
    for line in text.lines() {
        if let Some(value) = line.strip_prefix("worktree ") {
            path = Some(value.to_string());
            branch = String::new();
        } else if let Some(value) = line.strip_prefix("branch ") {
            branch = value.trim_start_matches("refs/heads/").to_string();
        } else if line.trim().is_empty() {
            flush(&mut worktrees, path.take(), &branch);
        }
    }
    flush(&mut worktrees, path.take(), &branch);
    worktrees
}

/// The default/main worktree uses the project id; others the shared v5 UUID.
fn entry_id(project_id: &str, entry: &ScannedEntry) -> String {
    if entry.is_default {
        project_id.to_string()
    } else {
        worktree_uuid(project_id, &entry.path)
    }
}

/// Create a worktree via the shared `codux-git` engine — the same git2 backend
/// and managed `.codux/worktrees/<slug>` path the desktop host uses, so the two
/// hosts never diverge on path layout or branch setup.
pub fn worktree_create(
    project_path: &str,
    branch_name: &str,
    base_branch: Option<&str>,
) -> Result<(), String> {
    codux_git::worktree::create_worktree(project_path, branch_name, base_branch).map(|_| ())
}

/// Remove a worktree (and optionally its branch) via the shared `codux-git`
/// engine.
pub fn worktree_remove(
    project_path: &str,
    worktree_path: &str,
    remove_branch: bool,
) -> Result<(), String> {
    codux_git::worktree::remove_worktree(project_path, worktree_path, remove_branch)
}

/// Merge a worktree's branch into `base_branch` (in the main worktree),
/// optionally removing the worktree + branch afterwards, via the shared
/// `codux-git` engine.
pub fn worktree_merge(
    project_path: &str,
    worktree_path: &str,
    base_branch: Option<&str>,
    remove_branch: bool,
) -> Result<(), String> {
    codux_git::worktree::merge_worktree(project_path, worktree_path, base_branch, remove_branch)
}

fn worktree_entry(project_id: &str, entry: &ScannedEntry) -> Value {
    let path = entry.path.as_str();
    json!({
        "id": entry_id(project_id, entry),
        "projectId": project_id,
        "name": worktree_display_name(&entry.branch, path),
        "branch": entry.branch,
        "path": path,
        "status": "active",
        "isDefault": entry.is_default,
        "exists": Path::new(path).exists(),
        "gitSummary": {
            "changes": changed_file_count(path),
            "incoming": 0,
            "outgoing": 0,
            "additions": 0,
            "deletions": 0,
        },
    })
}

fn changed_file_count(path: &str) -> usize {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["status", "--porcelain"])
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| {
            String::from_utf8_lossy(&output.stdout)
                .lines()
                .filter(|line| !line.trim().is_empty())
                .count()
        })
        .unwrap_or(0)
}
