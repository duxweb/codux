//! Shared remote-host git dispatch. Both the desktop and headless hosts route
//! `git.status` / `git.invoke` / `git.read` through these functions so the op
//! table and the rich status summary (ahead/behind/upstream/remotes/commits)
//! stay identical across transports instead of being reimplemented per host.

use serde_json::{Value, json};

use crate::{GitBranchSummary, GitService, GitSummary};
use codux_runtime_core::git::{
    GitBranchSummary as WireBranchSummary, GitStatusSummary as WireStatusSummary,
};

fn arg<'a>(args: &'a Value, key: &str) -> &'a str {
    args.get(key).and_then(Value::as_str).unwrap_or_default()
}

fn flag(args: &Value, key: &str) -> bool {
    args.get(key).and_then(Value::as_bool).unwrap_or(false)
}

fn index(args: &Value) -> usize {
    args.get("index").and_then(Value::as_u64).unwrap_or(0) as usize
}

fn paths(args: &Value) -> Vec<String> {
    args.get("paths")
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Convert the rich engine `GitSummary` into the wire `GitStatusSummary` that
/// `codux_runtime_core::git::git_status_payload` consumes.
pub fn wire_status_summary(summary: GitSummary) -> WireStatusSummary {
    WireStatusSummary {
        branch: summary.branch,
        upstream: summary.upstream,
        ahead: summary.ahead,
        behind: summary.behind,
        staged: summary.staged,
        unstaged: summary.unstaged,
        untracked: summary.untracked,
        additions: summary.additions,
        deletions: summary.deletions,
        is_repository: summary.is_repository,
        error: summary.error,
        changed_files: summary
            .changed_files
            .into_iter()
            .map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
            .collect(),
        branches: wire_branches(&summary.branches),
        remote_branches: summary.remote_branches,
        remotes: summary
            .remotes
            .into_iter()
            .map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
            .collect(),
        commits: summary
            .commits
            .into_iter()
            .map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
            .collect(),
        stashes: summary
            .stashes
            .into_iter()
            .map(|value| serde_json::to_value(value).unwrap_or(Value::Null))
            .collect(),
        tags: summary.tags,
    }
}

/// Convert engine branch summaries into the wire form (also used for the
/// worktree base-branch helpers).
pub fn wire_branches(branches: &[GitBranchSummary]) -> Vec<WireBranchSummary> {
    branches
        .iter()
        .map(|branch| WireBranchSummary {
            name: branch.name.clone(),
            is_current: branch.is_current,
        })
        .collect()
}

/// Run `GitService::status` and return the wire-ready status summary.
pub fn status(repo: &str) -> WireStatusSummary {
    wire_status_summary(GitService::status(repo))
}

/// Generic git mutation (`git.invoke`). On success the caller replies with a
/// refreshed status. Mirrors every op the desktop UI can issue.
pub fn invoke(repo: &str, op: &str, args: &Value) -> Result<(), String> {
    let s = |key: &str| arg(args, key);
    let b = |key: &str| flag(args, key);
    let path = repo;
    match op {
        "stage" => GitService::stage_paths(path, &paths(args)),
        "unstage" => GitService::unstage_paths(path, &paths(args)),
        "discard" => GitService::discard_paths(path, &paths(args)),
        "commit" => GitService::commit_staged(path, s("message")),
        "commit_push" => GitService::commit_action(path, s("message"), "commitAndPush"),
        "commit_sync" => GitService::commit_action(path, s("message"), "commitAndSync"),
        "commit_merge" => GitService::commit_merge(path, s("message"), s("target")),
        "init" => GitService::init(path),
        "trust_directory" => GitService::trust_project_directory(path),
        "clone" => {
            let credentials = args
                .get("credentials")
                .cloned()
                .and_then(|value| serde_json::from_value(value).ok());
            match credentials {
                Some(credentials) => {
                    GitService::clone_repository_with_credentials(path, s("remoteUrl"), credentials)
                }
                None => GitService::clone_repository(path, s("remoteUrl")),
            }
        }
        "checkout_branch" => GitService::checkout_branch(path, s("branch")),
        "checkout_remote_branch" => GitService::checkout_remote_branch(path, s("remoteBranch")),
        "checkout_commit" => GitService::checkout_commit(path, s("commit")),
        "create_branch" => GitService::create_branch(path, s("branch"), None, b("checkout")),
        "create_branch_from" => {
            let from = s("from");
            GitService::create_branch(
                path,
                s("branch"),
                (!from.is_empty()).then_some(from),
                b("checkout"),
            )
        }
        "merge_branch" => GitService::merge_branch(path, s("branch"), b("squash")),
        "delete_branch" => GitService::delete_branch(path, s("branch"), b("force")),
        "rename_branch" => GitService::rename_branch(path, s("branch"), s("newName")),
        "rebase_branch" => GitService::rebase_branch(path, s("branch")),
        "delete_remote_branch" => GitService::delete_remote_branch(path, s("remoteBranch")),
        "stash_push" => {
            let message = s("message");
            GitService::stash_push(
                path,
                (!message.is_empty()).then_some(message),
                b("includeUntracked"),
            )
        }
        "stash_apply" => GitService::stash_apply(path, index(args)),
        "stash_pop" => GitService::stash_pop(path, index(args)),
        "stash_drop" => GitService::stash_drop(path, index(args)),
        "stash_drop_all" => GitService::stash_drop_all(path),
        "create_tag" => {
            let message = s("message");
            GitService::create_tag(path, s("name"), (!message.is_empty()).then_some(message))
        }
        "delete_tag" => GitService::delete_tag(path, s("name")),
        "push_tags" => {
            let remote = s("remote");
            GitService::push_tags(path, (!remote.is_empty()).then_some(remote))
        }
        "delete_remote_tag" => {
            let remote = s("remote");
            GitService::delete_remote_tag(path, (!remote.is_empty()).then_some(remote), s("name"))
        }
        "fetch_prune" => GitService::fetch_prune(path),
        "amend" => GitService::amend_last_commit_message(path, s("message")),
        "undo_last_commit" => GitService::undo_last_commit(path),
        "revert_commit" => GitService::revert_commit(path, s("commit")),
        "restore_commit" => GitService::restore_commit(path, s("commit"), b("forceRemote")),
        "add_remote" => GitService::add_remote(path, s("name"), s("url")),
        "remove_remote" => GitService::remove_remote(path, s("name")),
        "append_gitignore" => GitService::append_gitignore(path, &paths(args)),
        "fetch" => GitService::fetch(path),
        "pull" => GitService::pull(path),
        "push" => GitService::push(path),
        "sync" => GitService::sync(path),
        "force_push" => GitService::force_push(path),
        "push_remote" => GitService::push_remote(path, s("remote")),
        "push_remote_branch" => {
            let local = s("localBranch");
            GitService::push_remote_branch(
                path,
                s("remoteBranch"),
                (!local.is_empty()).then_some(local),
            )
        }
        other => Err(format!("git op '{other}' is not supported.")),
    }
}

/// Generic git read (`git.read`) → the inner `result` value for `{op, result}`.
pub fn read(repo: &str, op: &str, args: &Value) -> Result<Value, String> {
    let s = |key: &str| arg(args, key);
    let base = || {
        let value = s("baseBranch");
        (!value.is_empty()).then(|| value.to_string())
    };
    let path = repo;
    match op {
        "diff" => GitService::file_diff(path, s("filePath")).map(|diff| json!({ "diff": diff })),
        "review_diff" => GitService::review_file_diff(path, s("filePath"), base().as_deref())
            .map(|diff| json!({ "diff": diff })),
        "review" => serde_json::to_value(GitService::review(path, base().as_deref()))
            .map_err(|error| error.to_string()),
        "review_file_content" => serde_json::to_value(GitService::review_file_content(
            path,
            s("filePath"),
            base().as_deref(),
        ))
        .map_err(|error| error.to_string()),
        "path_status" => GitService::path_status(path, s("directoryPath"))
            .and_then(|entries| serde_json::to_value(entries).map_err(|error| error.to_string()))
            .map(|entries| json!({ "entries": entries })),
        "commit_context" => serde_json::to_value(GitService::commit_message_context(path))
            .map_err(|error| error.to_string()),
        "last_commit_message" => {
            GitService::last_commit_message(path).map(|message| json!({ "message": message }))
        }
        "head_pushed" => {
            GitService::head_commit_pushed(path).map(|pushed| json!({ "pushed": pushed }))
        }
        other => Err(format!("git read '{other}' is not supported.")),
    }
}
