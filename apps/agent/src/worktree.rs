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
        let path = codux_git::normalize_repository_path(&entry.path);
        worktree_uuid(project_id, &path)
    }
}

/// Create a worktree via the shared `codux-git` engine — the same git2 backend
/// and managed `.codux/worktrees/<slug>` path the desktop host uses, so the two
/// hosts never diverge on path layout or branch setup.
pub fn worktree_create_payload(
    project_id: &str,
    project_path: &str,
    branch_name: &str,
    base_branch: Option<&str>,
) -> Result<Value, String> {
    let path = codux_git::worktree::create_worktree(project_path, branch_name, base_branch)?;
    let mut payload = worktree_list_payload(project_id, project_path);
    let created_path = codux_git::normalize_repository_path(&path.to_string_lossy());
    payload["selectedWorktreeId"] = Value::String(worktree_uuid(project_id, &created_path));
    Ok(payload)
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn create_payload_selects_the_created_worktree() {
        let repo = temp_dir("create-payload-selection");
        init_repo(&repo);

        let payload = worktree_create_payload(
            "project-1",
            repo.to_string_lossy().as_ref(),
            "feature/selected",
            None,
        )
        .expect("create worktree");
        let created = payload["worktrees"]
            .as_array()
            .and_then(|worktrees| {
                worktrees
                    .iter()
                    .find(|worktree| !worktree["isDefault"].as_bool().unwrap_or(false))
            })
            .expect("created worktree");

        assert_eq!(payload["selectedWorktreeId"], created["id"]);
        assert_eq!(created["branch"], "feature/selected");

        fs::remove_dir_all(repo).ok();
    }

    fn init_repo(path: &Path) {
        fs::create_dir_all(path).expect("create repository directory");
        run_git(path, &["init"]);
        run_git(path, &["config", "user.email", "codux@example.test"]);
        run_git(path, &["config", "user.name", "Codux"]);
        fs::write(path.join("README.md"), "test\n").expect("write repository file");
        run_git(path, &["add", "README.md"]);
        run_git(path, &["commit", "-m", "initial"]);
    }

    fn run_git(path: &Path, args: &[&str]) {
        let output = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .expect("run git");
        assert!(
            output.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn temp_dir(label: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time")
            .as_nanos();
        std::env::temp_dir().join(format!("codux-agent-worktree-{label}-{nanos}"))
    }
}
