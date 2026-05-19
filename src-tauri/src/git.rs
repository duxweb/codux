use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;
use tauri::{AppHandle, Emitter};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusSnapshot {
    pub branch: String,
    pub upstream: Option<String>,
    pub ahead: i64,
    pub behind: i64,
    pub staged: Vec<GitFileStatus>,
    pub unstaged: Vec<GitFileStatus>,
    pub untracked: Vec<GitFileStatus>,
    pub commits: Vec<GitCommitSummary>,
    pub branches: Vec<GitBranchSummary>,
    pub remote_branches: Vec<String>,
    pub remotes: Vec<GitRemoteSummary>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBriefStatus {
    pub branch: String,
    pub ahead: i64,
    pub behind: i64,
    pub changes: usize,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitFileStatus {
    pub path: String,
    pub index_status: String,
    pub worktree_status: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitSummary {
    pub hash: String,
    pub title: String,
    pub relative_time: String,
    pub decorations: Option<String>,
    pub graph_prefix: String,
    pub author: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteSummary {
    pub name: String,
    pub url: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewSnapshot {
    pub mode: String,
    pub title: String,
    pub base_branch: Option<String>,
    pub diff_stat: String,
    pub files: Vec<GitReviewFile>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewFile {
    pub path: String,
    pub status: String,
    pub additions: i64,
    pub deletions: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchesSnapshot {
    pub current: String,
    pub local: Vec<GitBranchSummary>,
    pub remote: Vec<GitBranchSummary>,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchSummary {
    pub name: String,
    pub upstream: Option<String>,
    pub hash: String,
    pub is_current: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffSnapshot {
    pub path: String,
    pub diff: String,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPathsRequest {
    pub project_path: String,
    pub paths: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRequest {
    pub project_path: String,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitBranchRequest {
    pub project_path: String,
    pub branch: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCreateBranchRequest {
    pub project_path: String,
    pub branch: String,
    pub checkout: bool,
    pub from: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDiffRequest {
    pub project_path: String,
    pub path: String,
    pub staged: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewDiffRequest {
    pub project_path: String,
    pub path: String,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCloneRequest {
    pub project_path: String,
    pub remote_url: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitActionRequest {
    pub project_path: String,
    pub message: String,
    pub action: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRemoteRequest {
    pub project_path: String,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitDeleteBranchRequest {
    pub project_path: String,
    pub branch: String,
    pub force: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitRefRequest {
    pub project_path: String,
    pub commit: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRestoreCommitRequest {
    pub project_path: String,
    pub commit: String,
    pub force_remote: bool,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPushRemoteRequest {
    pub project_path: String,
    pub remote: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GitPushRemoteBranchRequest {
    pub project_path: String,
    pub local_branch: Option<String>,
    pub remote_branch: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitWatchRegistration {
    pub project_path: String,
    pub repository_path: String,
    pub is_repository: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitRepositoryChangeEvent {
    pub project_path: String,
    pub repository_path: String,
    pub changed_paths: Vec<String>,
}

pub struct GitWatchManager {
    watchers: Mutex<HashMap<String, GitRepositoryWatcher>>,
}

struct GitRepositoryWatcher {
    _watcher: RecommendedWatcher,
    project_path: String,
    _repository_path: String,
    _watch_paths: Vec<PathBuf>,
}

impl Default for GitWatchManager {
    fn default() -> Self {
        Self {
            watchers: Mutex::new(HashMap::new()),
        }
    }
}

impl GitWatchManager {
    pub fn watch(
        &self,
        app: AppHandle,
        project_path: String,
    ) -> Result<GitWatchRegistration, String> {
        let watch_target = resolve_watch_target(&project_path)?;
        let key = watch_target.repository_key.clone();
        let registration = GitWatchRegistration {
            project_path: watch_target.project_path.clone(),
            repository_path: watch_target.repository_path.clone(),
            is_repository: watch_target.is_repository,
        };

        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "Git watcher lock is poisoned.".to_string())?;
        if watchers.contains_key(&key) {
            return Ok(registration);
        }

        let project_path_for_event = watch_target.project_path.clone();
        let repository_path_for_event = watch_target.repository_path.clone();
        let repository_key = watch_target.repository_key.clone();
        let git_dir_keys = watch_target.git_dir_keys.clone();
        let app_handle = app.clone();
        let mut watcher = notify::recommended_watcher(move |event: notify::Result<Event>| {
            let Ok(event) = event else {
                return;
            };
            let changed_paths = event
                .paths
                .iter()
                .filter_map(|path| {
                    let key = normalized_path_key(path);
                    should_forward_git_watch_path(&repository_key, &git_dir_keys, &key)
                        .then(|| normalized_path_display(path))
                })
                .collect::<Vec<_>>();
            if changed_paths.is_empty() {
                return;
            }
            let _ = app_handle.emit(
                "git:changed",
                GitRepositoryChangeEvent {
                    project_path: project_path_for_event.clone(),
                    repository_path: repository_path_for_event.clone(),
                    changed_paths,
                },
            );
        })
        .map_err(|error| error.to_string())?;

        for path in &watch_target.watch_paths {
            watcher
                .watch(path, RecursiveMode::Recursive)
                .map_err(|error| error.to_string())?;
        }

        watchers.insert(
            key,
            GitRepositoryWatcher {
                _watcher: watcher,
                project_path: watch_target.project_path,
                _repository_path: watch_target.repository_path,
                _watch_paths: watch_target.watch_paths,
            },
        );
        Ok(registration)
    }

    pub fn unwatch(&self, project_path: String) -> Result<(), String> {
        let requested_key = normalized_path_key(Path::new(project_path.trim()));
        let repository_key = resolve_watch_target(&project_path)
            .map(|target| target.repository_key)
            .unwrap_or_else(|_| requested_key.clone());
        let mut watchers = self
            .watchers
            .lock()
            .map_err(|_| "Git watcher lock is poisoned.".to_string())?;
        if watchers.remove(&repository_key).is_none() {
            watchers.retain(|_, watcher| {
                normalized_path_key(Path::new(&watcher.project_path)) != requested_key
            });
        }
        Ok(())
    }
}

struct GitWatchTarget {
    project_path: String,
    repository_path: String,
    repository_key: String,
    git_dir_keys: Vec<String>,
    watch_paths: Vec<PathBuf>,
    is_repository: bool,
}

fn resolve_watch_target(project_path: &str) -> Result<GitWatchTarget, String> {
    let project = PathBuf::from(project_path.trim());
    if project.as_os_str().is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    if !project.exists() {
        return Err(format!(
            "Project path does not exist: {}",
            project.display()
        ));
    }

    let project_path = normalized_path_display(&project);
    let root = repository_root(project_path.as_str()).ok();
    let is_repository = root.is_some();
    let repository_path = root.unwrap_or_else(|| project_path.clone());
    let repository_path_buf = PathBuf::from(&repository_path);
    let repository_key = normalized_path_key(&repository_path_buf);
    let git_dirs = if is_repository {
        repository_git_dirs(&repository_path_buf)
    } else {
        vec![repository_path_buf.join(".git")]
    };
    let git_dir_keys = git_dirs
        .iter()
        .map(|path| normalized_path_key(path))
        .collect::<Vec<_>>();

    let mut watch_paths = Vec::new();
    push_unique_path(&mut watch_paths, repository_path_buf);
    for git_dir in git_dirs {
        if git_dir.exists() {
            push_unique_path(&mut watch_paths, git_dir);
        }
    }

    Ok(GitWatchTarget {
        project_path,
        repository_path,
        repository_key,
        git_dir_keys,
        watch_paths,
        is_repository,
    })
}

fn repository_git_dirs(root: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    for args in [
        ["rev-parse", "--absolute-git-dir"].as_slice(),
        ["rev-parse", "--git-common-dir"].as_slice(),
    ] {
        if let Ok(value) = git_output(root, args) {
            let trimmed = value.trim();
            if trimmed.is_empty() {
                continue;
            }
            let path = PathBuf::from(trimmed);
            let path = if path.is_absolute() {
                path
            } else {
                root.join(path)
            };
            push_unique_path(&mut dirs, path);
        }
    }
    if dirs.is_empty() {
        dirs.push(root.join(".git"));
    }
    dirs
}

fn push_unique_path(paths: &mut Vec<PathBuf>, path: PathBuf) {
    let key = normalized_path_key(&path);
    if paths
        .iter()
        .any(|existing| normalized_path_key(existing) == key)
    {
        return;
    }
    paths.push(path);
}

fn normalized_path_key(path: &Path) -> String {
    let normalized_path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut key = normalized_path.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key = key.to_ascii_lowercase();
    }
    key
}

fn normalized_path_display(path: &Path) -> String {
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .to_string_lossy()
        .to_string()
}

fn should_forward_git_watch_path(
    repository_key: &str,
    git_dir_keys: &[String],
    path_key: &str,
) -> bool {
    for git_dir_key in git_dir_keys {
        let is_git_path = path_key == git_dir_key
            || path_key
                .strip_prefix(git_dir_key)
                .is_some_and(|suffix| suffix.starts_with('/'));
        if !is_git_path {
            continue;
        }

        let relative = path_key
            .strip_prefix(git_dir_key)
            .unwrap_or("")
            .trim_start_matches('/');
        return is_allowed_git_metadata_path(relative);
    }

    let repository_git_key = format!("{repository_key}/.git");
    if path_key == repository_git_key
        || path_key
            .strip_prefix(&repository_git_key)
            .is_some_and(|suffix| suffix.starts_with('/'))
    {
        let relative = path_key
            .strip_prefix(&repository_git_key)
            .unwrap_or("")
            .trim_start_matches('/');
        return is_allowed_git_metadata_path(relative);
    }

    true
}

fn is_allowed_git_metadata_path(relative: &str) -> bool {
    let relative = relative.trim_start_matches('/');
    if relative.is_empty() {
        return false;
    }

    #[cfg(windows)]
    {
        let relative = relative.to_ascii_lowercase();
        return match relative.as_str() {
            "head" | "index" | "fetch_head" | "orig_head" | "packed-refs" => true,
            _ => relative.starts_with("refs/") || relative.starts_with("logs/head"),
        };
    }

    match relative {
        "HEAD" | "index" | "FETCH_HEAD" | "ORIG_HEAD" | "packed-refs" => true,
        _ => relative.starts_with("refs/") || relative.starts_with("logs/HEAD"),
    }
}

pub fn git_brief_status(project_path: String) -> GitBriefStatus {
    let project_path = Path::new(&project_path);
    let output = match git_output(
        project_path,
        &["status", "--porcelain=v1", "--branch", "-z"],
    ) {
        Ok(value) => value,
        Err(error) => {
            return GitBriefStatus {
                branch: "uninitialized".to_string(),
                ahead: 0,
                behind: 0,
                changes: 0,
                is_repository: false,
                error: Some(error),
            };
        }
    };
    parse_brief_status(&output)
}

pub fn git_status(project_path: String) -> GitStatusSnapshot {
    let project_path = Path::new(&project_path);
    let root = match git_output(project_path, &["rev-parse", "--show-toplevel"]) {
        Ok(value) => value.trim().to_string(),
        Err(error) => {
            return GitStatusSnapshot {
                branch: "uninitialized".to_string(),
                upstream: None,
                ahead: 0,
                behind: 0,
                staged: Vec::new(),
                unstaged: Vec::new(),
                untracked: Vec::new(),
                commits: Vec::new(),
                branches: Vec::new(),
                remote_branches: Vec::new(),
                remotes: Vec::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let root_path = Path::new(&root);
    let branch = git_output(root_path, &["branch", "--show-current"])
        .ok()
        .and_then(|value| normalized(value.trim()))
        .or_else(|| {
            git_output(root_path, &["rev-parse", "--short", "HEAD"])
                .ok()
                .and_then(|value| normalized(value.trim()))
        })
        .unwrap_or_else(|| "HEAD".to_string());
    let upstream = git_output(
        root_path,
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .ok()
    .and_then(|value| normalized(value.trim()));
    let (ahead, behind) = if upstream.is_some() {
        git_output(
            root_path,
            &["rev-list", "--left-right", "--count", "HEAD...@{u}"],
        )
        .ok()
        .and_then(|value| {
            let mut parts = value.split_whitespace();
            let ahead = parts.next()?.parse().ok()?;
            let behind = parts.next()?.parse().ok()?;
            Some((ahead, behind))
        })
        .unwrap_or((0, 0))
    } else {
        (0, 0)
    };
    let (staged, unstaged, untracked) = parse_porcelain_status(
        &git_output(root_path, &["status", "--porcelain=v1", "-z"]).unwrap_or_default(),
    );
    let commits = parse_git_log(
        &git_output(
            root_path,
            &[
                "log",
                "--graph",
                "--date=relative",
                "--decorate=short",
                "--pretty=format:%x09%H%x1f%D%x1f%cr%x1f%s%x1f%an%x1e",
                "-n",
                "20",
            ],
        )
        .unwrap_or_default(),
    );
    let branches = local_branches(root_path, &branch);
    let remote_branches = remote_branch_names(root_path);
    let remotes = git_remotes(root_path);

    GitStatusSnapshot {
        branch,
        upstream,
        ahead,
        behind,
        staged,
        unstaged,
        untracked,
        commits,
        branches,
        remote_branches,
        remotes,
        is_repository: true,
        error: None,
    }
}

fn parse_brief_status(value: &str) -> GitBriefStatus {
    let parts = value
        .split('\0')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let branch_line = parts
        .first()
        .copied()
        .filter(|line| line.starts_with("## "))
        .unwrap_or("## HEAD");
    let (branch, ahead, behind) = parse_branch_line(branch_line);
    let mut changes = 0;
    let mut index = if branch_line.starts_with("## ") { 1 } else { 0 };
    while index < parts.len() {
        let item = parts[index];
        let bytes = item.as_bytes();
        if bytes.len() >= 4 {
            changes += 1;
            let index_status = item[0..1].to_string();
            if index_status == "R" || index_status == "C" {
                index += 1;
            }
        }
        index += 1;
    }

    GitBriefStatus {
        branch,
        ahead,
        behind,
        changes,
        is_repository: true,
        error: None,
    }
}

fn parse_branch_line(value: &str) -> (String, i64, i64) {
    let value = value.trim().strip_prefix("## ").unwrap_or(value.trim());
    let (head, meta) = value
        .split_once(" [")
        .map(|(head, meta)| (head, Some(meta.trim_end_matches(']'))))
        .unwrap_or((value, None));
    let branch = if let Some(branch) = head.strip_prefix("No commits yet on ") {
        branch.to_string()
    } else if head == "HEAD (no branch)" {
        "HEAD".to_string()
    } else {
        head.split("...").next().unwrap_or(head).trim().to_string()
    };
    let mut ahead = 0;
    let mut behind = 0;
    if let Some(meta) = meta {
        for part in meta.split(',') {
            let part = part.trim();
            if let Some(value) = part.strip_prefix("ahead ") {
                ahead = value.parse().unwrap_or(0);
            } else if let Some(value) = part.strip_prefix("behind ") {
                behind = value.parse().unwrap_or(0);
            }
        }
    }
    (
        normalized(&branch).unwrap_or_else(|| "HEAD".to_string()),
        ahead,
        behind,
    )
}

pub fn git_stage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&request.project_path)?;
    if request.paths.is_empty() {
        git_output(Path::new(&root), &["add", "-A"])?;
    } else {
        let mut args = vec!["add", "--"];
        let paths = request.paths.iter().map(String::as_str).collect::<Vec<_>>();
        args.extend(paths);
        git_output(Path::new(&root), &args)?;
    }
    Ok(git_status(root))
}

pub fn git_unstage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&request.project_path)?;
    if has_resolvable_head(&root) {
        if request.paths.is_empty() {
            git_output(Path::new(&root), &["reset", "HEAD", "--", "."])?;
        } else {
            let mut args = vec!["reset", "HEAD", "--"];
            let paths = request.paths.iter().map(String::as_str).collect::<Vec<_>>();
            args.extend(paths);
            git_output(Path::new(&root), &args)?;
        }
    } else if request.paths.is_empty() {
        git_output(Path::new(&root), &["rm", "--cached", "-r", "."])?;
    } else {
        let mut args = vec!["rm", "--cached", "-r", "--"];
        let paths = request.paths.iter().map(String::as_str).collect::<Vec<_>>();
        args.extend(paths);
        git_output(Path::new(&root), &args)?;
    }
    Ok(git_status(root))
}

pub fn git_commit(request: GitCommitRequest) -> Result<GitStatusSnapshot, String> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err("Commit message cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["commit", "-m", message])?;
    Ok(git_status(root))
}

pub fn git_commit_action(request: GitCommitActionRequest) -> Result<GitStatusSnapshot, String> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err("Commit message cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["commit", "-m", message])?;
    match request.action.as_str() {
        "commit" => {}
        "commitAndPush" => {
            git_output(Path::new(&root), &["push"])?;
        }
        "commitAndSync" => {
            git_output(Path::new(&root), &["pull", "--rebase"])?;
            git_output(Path::new(&root), &["push"])?;
        }
        _ => return Err(format!("Unknown commit action: {}", request.action)),
    }
    Ok(git_status(root))
}

pub fn git_amend_last_commit_message(
    request: GitCommitRequest,
) -> Result<GitStatusSnapshot, String> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err("Commit message cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["commit", "--amend", "-m", message])?;
    Ok(git_status(root))
}

pub fn git_last_commit_message(project_path: String) -> Result<String, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["log", "-1", "--pretty=%s"])
        .map(|value| value.trim().to_string())
}

pub fn git_undo_last_commit(project_path: String) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["reset", "--soft", "HEAD~1"])?;
    Ok(git_status(root))
}

pub fn git_head_commit_pushed(project_path: String) -> Result<bool, String> {
    let root = repository_root(&project_path)?;
    let upstream = git_output(
        Path::new(&root),
        &["rev-parse", "--abbrev-ref", "--symbolic-full-name", "@{u}"],
    )
    .unwrap_or_default()
    .trim()
    .to_string();
    if upstream.is_empty() || upstream.contains("fatal:") {
        return Ok(false);
    }
    let output =
        git_output(Path::new(&root), &["branch", "-r", "--contains", "HEAD"]).unwrap_or_default();
    Ok(output.lines().map(str::trim).any(|line| line == upstream))
}

pub fn git_init(project_path: String) -> Result<GitStatusSnapshot, String> {
    let path = Path::new(project_path.trim());
    if !path.exists() {
        return Err(format!("Project path does not exist: {}", path.display()));
    }
    git_output(path, &["init"])?;
    Ok(git_status(path.display().to_string()))
}

pub fn git_clone(request: GitCloneRequest) -> Result<GitStatusSnapshot, String> {
    let remote_url = request.remote_url.trim();
    if remote_url.is_empty() {
        return Err("Remote URL cannot be empty.".to_string());
    }
    let project_path = Path::new(request.project_path.trim());
    let parent = project_path
        .parent()
        .ok_or_else(|| "Project path has no parent directory.".to_string())?;
    let folder_name = project_path
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| "Project folder name is invalid.".to_string())?;
    git_output(parent, &["clone", "--progress", remote_url, folder_name])?;
    Ok(git_status(project_path.display().to_string()))
}

pub fn git_discard(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&request.project_path)?;
    if request.paths.is_empty() {
        let _ = git_output_permissive(
            Path::new(&root),
            &["restore", "--staged", "--worktree", "--", "."],
        );
        let _ = git_output_permissive(Path::new(&root), &["clean", "-fd"]);
    } else {
        let mut restore_args = vec!["restore", "--staged", "--worktree", "--"];
        let paths = request.paths.iter().map(String::as_str).collect::<Vec<_>>();
        restore_args.extend(paths.iter().copied());
        let _ = git_output_permissive(Path::new(&root), &restore_args);

        let mut clean_args = vec!["clean", "-fd", "--"];
        clean_args.extend(paths);
        let _ = git_output_permissive(Path::new(&root), &clean_args);
    }
    Ok(git_status(root))
}

pub fn git_branches(project_path: String) -> GitBranchesSnapshot {
    let root = match repository_root(&project_path) {
        Ok(root) => root,
        Err(error) => {
            return GitBranchesSnapshot {
                current: String::new(),
                local: Vec::new(),
                remote: Vec::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let current = git_output(Path::new(&root), &["branch", "--show-current"])
        .ok()
        .and_then(|value| normalized(value.trim()))
        .unwrap_or_else(|| "HEAD".to_string());
    let local = parse_branch_refs(
        &git_output(
            Path::new(&root),
            &[
                "for-each-ref",
                "--format=%(refname:short)%x1f%(upstream:short)%x1f%(objectname:short)",
                "refs/heads",
            ],
        )
        .unwrap_or_default(),
        &current,
    );
    let remote = parse_branch_refs(
        &git_output(
            Path::new(&root),
            &[
                "for-each-ref",
                "--format=%(refname:short)%x1f%(upstream:short)%x1f%(objectname:short)",
                "refs/remotes",
            ],
        )
        .unwrap_or_default(),
        &current,
    )
    .into_iter()
    .filter(|branch| !branch.name.ends_with("/HEAD"))
    .collect();
    GitBranchesSnapshot {
        current,
        local,
        remote,
        is_repository: true,
        error: None,
    }
}

pub fn git_checkout_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["checkout", branch])?;
    Ok(git_status(root))
}

pub fn git_create_branch(request: GitCreateBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    let from = request
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if request.checkout {
        if let Some(from) = from {
            git_output(Path::new(&root), &["checkout", "-b", branch, from])?;
        } else {
            git_output(Path::new(&root), &["checkout", "-b", branch])?;
        }
    } else if let Some(from) = from {
        git_output(Path::new(&root), &["branch", branch, from])?;
    } else {
        git_output(Path::new(&root), &["branch", branch])?;
    }
    Ok(git_status(root))
}

pub fn git_checkout_remote_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    let remote_branch = request.branch.trim();
    if remote_branch.is_empty() {
        return Err("Remote branch name cannot be empty.".to_string());
    }
    let local_name = remote_branch
        .split_once('/')
        .map(|(_, branch)| branch)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(remote_branch);
    let root = repository_root(&request.project_path)?;
    git_output(
        Path::new(&root),
        &["checkout", "-b", local_name, "--track", remote_branch],
    )?;
    Ok(git_status(root))
}

pub fn git_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["merge", branch])?;
    Ok(git_status(root))
}

pub fn git_squash_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["merge", "--squash", branch])?;
    Ok(git_status(root))
}

pub fn git_delete_branch(request: GitDeleteBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(
        Path::new(&root),
        &["branch", if request.force { "-D" } else { "-d" }, branch],
    )?;
    Ok(git_status(root))
}

pub fn git_checkout_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    let commit = request.commit.trim();
    if commit.is_empty() {
        return Err("Commit cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["checkout", commit])?;
    Ok(git_status(root))
}

pub fn git_revert_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    let commit = request.commit.trim();
    if commit.is_empty() {
        return Err("Commit cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["revert", "--no-edit", commit])?;
    Ok(git_status(root))
}

pub fn git_restore_commit(request: GitRestoreCommitRequest) -> Result<GitStatusSnapshot, String> {
    let commit = request.commit.trim();
    if commit.is_empty() {
        return Err("Commit cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["reset", "--hard", commit])?;
    if request.force_remote {
        git_output(Path::new(&root), &["push", "--force-with-lease"])?;
    }
    Ok(git_status(root))
}

pub fn git_add_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    let name = request.name.trim();
    let url = request.url.as_deref().map(str::trim).unwrap_or("");
    if name.is_empty() {
        return Err("Remote name cannot be empty.".to_string());
    }
    if url.is_empty() {
        return Err("Remote URL cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["remote", "add", name, url])?;
    Ok(git_status(root))
}

pub fn git_remove_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err("Remote name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    git_output(Path::new(&root), &["remote", "remove", name])?;
    Ok(git_status(root))
}

pub fn git_append_gitignore(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&request.project_path)?;
    let additions = request
        .paths
        .iter()
        .map(|path| path.trim())
        .filter(|path| !path.is_empty())
        .collect::<Vec<_>>();
    if additions.is_empty() {
        return Ok(git_status(root));
    }
    let gitignore_path = Path::new(&root).join(".gitignore");
    let existing = std::fs::read_to_string(&gitignore_path).unwrap_or_default();
    let existing_lines = existing
        .lines()
        .map(str::trim)
        .collect::<std::collections::HashSet<_>>();
    let next = additions
        .into_iter()
        .filter(|path| !existing_lines.contains(path))
        .collect::<Vec<_>>();
    if next.is_empty() {
        return Ok(git_status(root));
    }
    let mut content = existing;
    if !content.is_empty() && !content.ends_with('\n') {
        content.push('\n');
    }
    content.push_str(&next.join("\n"));
    content.push('\n');
    std::fs::write(gitignore_path, content).map_err(|error| error.to_string())?;
    Ok(git_status(root))
}

pub fn git_fetch(project_path: String) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["fetch"])?;
    Ok(git_status(root))
}

pub fn git_sync(project_path: String) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["pull", "--rebase"])?;
    git_output(Path::new(&root), &["push"])?;
    Ok(git_status(root))
}

pub fn git_push_remote(request: GitPushRemoteRequest) -> Result<GitStatusSnapshot, String> {
    let remote = request.remote.trim();
    if remote.is_empty() {
        return Err("Remote name cannot be empty.".to_string());
    }
    let root = repository_root(&request.project_path)?;
    let branch = git_output(Path::new(&root), &["branch", "--show-current"])?
        .trim()
        .to_string();
    if branch.is_empty() {
        return Err("Cannot push detached HEAD to a remote.".to_string());
    }
    git_output(Path::new(&root), &["push", "-u", remote, branch.as_str()])?;
    Ok(git_status(root))
}

pub fn git_push_remote_branch(
    request: GitPushRemoteBranchRequest,
) -> Result<GitStatusSnapshot, String> {
    let remote_branch = request.remote_branch.trim();
    if remote_branch.is_empty() {
        return Err("Remote branch cannot be empty.".to_string());
    }
    let (remote, branch_name) = remote_branch
        .split_once('/')
        .ok_or_else(|| "Remote branch must include a remote name.".to_string())?;
    let root = repository_root(&request.project_path)?;
    let local_branch = match request
        .local_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch.to_string(),
        None => git_output(Path::new(&root), &["branch", "--show-current"])?
            .trim()
            .to_string(),
    };
    if local_branch.is_empty() {
        return Err("Cannot push detached HEAD to a remote branch.".to_string());
    }
    let refspec = format!("{local_branch}:{branch_name}");
    git_output(Path::new(&root), &["push", remote, refspec.as_str()])?;
    Ok(git_status(root))
}

pub fn git_pull(project_path: String) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["pull", "--rebase"])?;
    Ok(git_status(root))
}

pub fn git_push(project_path: String) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["push"])?;
    Ok(git_status(root))
}

pub fn git_force_push(project_path: String) -> Result<GitStatusSnapshot, String> {
    let root = repository_root(&project_path)?;
    git_output(Path::new(&root), &["push", "--force-with-lease"])?;
    Ok(git_status(root))
}

pub fn git_diff_file(request: GitDiffRequest) -> GitDiffSnapshot {
    let root = match repository_root(&request.project_path) {
        Ok(root) => root,
        Err(error) => {
            return GitDiffSnapshot {
                path: request.path,
                diff: String::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let path = request.path.trim();
    if path.is_empty() {
        return GitDiffSnapshot {
            path: String::new(),
            diff: String::new(),
            is_repository: true,
            error: Some("File path cannot be empty.".to_string()),
        };
    }
    let diff = if request.staged {
        git_output_permissive(Path::new(&root), &["diff", "--cached", "--", path])
    } else {
        git_output_permissive(Path::new(&root), &["diff", "--", path])
    }
    .unwrap_or_default();
    let diff = if diff.trim().is_empty() {
        if !request.staged && is_untracked_path(&root, path) {
            format!("Untracked file: {path}\n\nStage the file to include it in the next commit.")
        } else {
            diff
        }
    } else {
        diff
    };
    GitDiffSnapshot {
        path: path.to_string(),
        diff,
        is_repository: true,
        error: None,
    }
}

pub fn git_review_diff_file(request: GitReviewDiffRequest) -> GitDiffSnapshot {
    let root = match repository_root(&request.project_path) {
        Ok(root) => root,
        Err(error) => {
            return GitDiffSnapshot {
                path: request.path,
                diff: String::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let path = request.path.trim();
    if path.is_empty() {
        return GitDiffSnapshot {
            path: String::new(),
            diff: String::new(),
            is_repository: true,
            error: Some("File path cannot be empty.".to_string()),
        };
    }
    let base = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current branch");

    let diff = if let Some(base) = base {
        let range = format!("{base}...HEAD");
        git_output_permissive(Path::new(&root), &["diff", range.as_str(), "--", path])
            .or_else(|_| {
                git_output_permissive(Path::new(&root), &["diff", base, "HEAD", "--", path])
            })
            .unwrap_or_default()
    } else {
        let unstaged =
            git_output_permissive(Path::new(&root), &["diff", "--", path]).unwrap_or_default();
        let staged = git_output_permissive(Path::new(&root), &["diff", "--cached", "--", path])
            .unwrap_or_default();
        match (staged.trim().is_empty(), unstaged.trim().is_empty()) {
            (true, true) if is_untracked_path(&root, path) => {
                format!(
                    "Untracked file: {path}\n\nStage the file to include it in the next commit."
                )
            }
            (true, _) => unstaged,
            (_, true) => staged,
            _ => format!("{staged}\n{unstaged}"),
        }
    };

    GitDiffSnapshot {
        path: path.to_string(),
        diff,
        is_repository: true,
        error: None,
    }
}

pub fn git_review(project_path: String, base_branch: Option<String>) -> GitReviewSnapshot {
    let root = match repository_root(&project_path) {
        Ok(root) => root,
        Err(error) => {
            return GitReviewSnapshot {
                mode: "workingTreeAudit".to_string(),
                title: "Uncommitted Audit".to_string(),
                base_branch,
                diff_stat: String::new(),
                files: Vec::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let base = base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current branch")
        .map(str::to_string);

    if let Some(base) = base {
        let range = format!("{base}...HEAD");
        let name_status = git_output(Path::new(&root), &["diff", "--name-status", range.as_str()])
            .or_else(|_| {
                git_output(
                    Path::new(&root),
                    &["diff", "--name-status", base.as_str(), "HEAD"],
                )
            })
            .unwrap_or_default();
        let numstat = git_output(Path::new(&root), &["diff", "--numstat", range.as_str()])
            .or_else(|_| {
                git_output(
                    Path::new(&root),
                    &["diff", "--numstat", base.as_str(), "HEAD"],
                )
            })
            .unwrap_or_default();
        let diff_stat = git_output(Path::new(&root), &["diff", "--shortstat", range.as_str()])
            .or_else(|_| {
                git_output(
                    Path::new(&root),
                    &["diff", "--shortstat", base.as_str(), "HEAD"],
                )
            })
            .unwrap_or_default()
            .trim()
            .to_string();
        return GitReviewSnapshot {
            mode: "taskBranch".to_string(),
            title: "Worktree Review".to_string(),
            base_branch: Some(base),
            diff_stat,
            files: merge_review_files(&name_status, &numstat),
            is_repository: true,
            error: None,
        };
    }

    let status = git_status(root);
    let files = status
        .staged
        .iter()
        .map(|file| review_file_from_status(file, "staged"))
        .chain(
            status
                .unstaged
                .iter()
                .map(|file| review_file_from_status(file, "modified")),
        )
        .chain(
            status
                .untracked
                .iter()
                .map(|file| review_file_from_status(file, "added")),
        )
        .collect::<Vec<_>>();
    GitReviewSnapshot {
        mode: "workingTreeAudit".to_string(),
        title: "Uncommitted Audit".to_string(),
        base_branch: None,
        diff_stat: if files.is_empty() {
            String::new()
        } else {
            format!("{} changed files", files.len())
        },
        files,
        is_repository: true,
        error: None,
    }
}

fn parse_porcelain_status(
    value: &str,
) -> (Vec<GitFileStatus>, Vec<GitFileStatus>, Vec<GitFileStatus>) {
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    let parts = value
        .split('\0')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let mut index = 0;
    while index < parts.len() {
        let item = parts[index];
        let bytes = item.as_bytes();
        if bytes.len() < 4 {
            index += 1;
            continue;
        }
        let index_status = item[0..1].to_string();
        let worktree_status = item[1..2].to_string();
        let path = item[3..].to_string();
        let status = GitFileStatus {
            path,
            index_status: index_status.clone(),
            worktree_status: worktree_status.clone(),
        };
        if index_status == "?" && worktree_status == "?" {
            untracked.push(status);
        } else {
            if index_status.trim().is_empty() == false {
                staged.push(status.clone());
            }
            if worktree_status.trim().is_empty() == false {
                unstaged.push(status);
            }
        }
        if index_status == "R" || index_status == "C" {
            index += 1;
        }
        index += 1;
    }
    (staged, unstaged, untracked)
}

fn parse_git_log(value: &str) -> Vec<GitCommitSummary> {
    value
        .split('\u{1e}')
        .filter_map(|row| {
            let row = row.trim_matches('\n');
            if row.is_empty() {
                return None;
            }
            let (graph_prefix, data) = row
                .split_once('\t')
                .map(|(graph, data)| (graph.trim_end().to_string(), data))
                .unwrap_or_else(|| (String::new(), row));
            let mut parts = data.split('\u{1f}');
            Some(GitCommitSummary {
                hash: parts.next()?.trim().to_string(),
                decorations: normalized(parts.next()?.trim()),
                relative_time: parts.next()?.trim().to_string(),
                title: parts.next()?.trim().to_string(),
                author: parts.next().unwrap_or("").trim().to_string(),
                graph_prefix,
            })
        })
        .collect()
}

fn local_branches(root: &Path, current: &str) -> Vec<GitBranchSummary> {
    parse_branch_refs(
        &git_output(
            root,
            &[
                "for-each-ref",
                "--format=%(refname:short)%x1f%(upstream:short)%x1f%(objectname:short)",
                "refs/heads",
            ],
        )
        .unwrap_or_default(),
        current,
    )
}

fn remote_branch_names(root: &Path) -> Vec<String> {
    git_output(
        root,
        &["for-each-ref", "--format=%(refname:short)", "refs/remotes"],
    )
    .unwrap_or_default()
    .lines()
    .map(str::trim)
    .filter(|value| !value.is_empty() && !value.ends_with("/HEAD") && value.contains('/'))
    .map(str::to_string)
    .collect()
}

fn git_remotes(root: &Path) -> Vec<GitRemoteSummary> {
    let mut remotes = Vec::new();
    let mut seen = std::collections::HashSet::new();
    for line in git_output(root, &["remote", "-v"])
        .unwrap_or_default()
        .lines()
    {
        let parts = line.split_whitespace().collect::<Vec<_>>();
        let Some(name) = parts.first().copied() else {
            continue;
        };
        let Some(url) = parts.get(1).copied() else {
            continue;
        };
        if !seen.insert(name.to_string()) {
            continue;
        }
        remotes.push(GitRemoteSummary {
            name: name.to_string(),
            url: url.to_string(),
        });
    }
    remotes.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    remotes
}

fn parse_branch_refs(value: &str, current: &str) -> Vec<GitBranchSummary> {
    value
        .lines()
        .filter_map(|line| {
            let mut parts = line.split('\u{1f}');
            let name = parts.next()?.trim().to_string();
            if name.is_empty() {
                return None;
            }
            let upstream = parts.next().and_then(normalized);
            let hash = parts.next().unwrap_or("").trim().to_string();
            Some(GitBranchSummary {
                is_current: name == current,
                name,
                upstream,
                hash,
            })
        })
        .collect()
}

fn merge_review_files(name_status: &str, numstat: &str) -> Vec<GitReviewFile> {
    let stats = parse_numstat(numstat);
    name_status
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            let raw_status = parts.first()?.trim();
            let path = if raw_status.starts_with('R') || raw_status.starts_with('C') {
                parts.get(2).or_else(|| parts.get(1))?
            } else {
                parts.get(1)?
            }
            .trim();
            if path.is_empty() {
                return None;
            }
            let (additions, deletions) = stats.get(path).copied().unwrap_or((0, 0));
            Some(GitReviewFile {
                path: path.to_string(),
                status: review_status(raw_status),
                additions,
                deletions,
            })
        })
        .collect()
}

fn parse_numstat(value: &str) -> std::collections::HashMap<String, (i64, i64)> {
    value
        .lines()
        .filter_map(|line| {
            let parts = line.split('\t').collect::<Vec<_>>();
            let additions = parts.first()?.parse().unwrap_or(0);
            let deletions = parts.get(1)?.parse().unwrap_or(0);
            let path = if parts.len() >= 4 {
                parts.get(3)?
            } else {
                parts.get(2)?
            };
            Some((path.to_string(), (additions, deletions)))
        })
        .collect()
}

fn review_file_from_status(file: &GitFileStatus, fallback: &str) -> GitReviewFile {
    let status = if file.index_status == "?" && file.worktree_status == "?" {
        "added".to_string()
    } else {
        review_status(
            file.worktree_status
                .trim()
                .chars()
                .next()
                .or_else(|| file.index_status.trim().chars().next())
                .map(|value| value.to_string())
                .as_deref()
                .unwrap_or(fallback),
        )
    };
    GitReviewFile {
        path: file.path.clone(),
        status,
        additions: 0,
        deletions: 0,
    }
}

fn review_status(value: &str) -> String {
    match value.chars().next().unwrap_or('M') {
        'A' => "added",
        'D' => "deleted",
        'R' => "renamed",
        'C' => "copied",
        'T' => "typeChanged",
        '?' => "added",
        _ => "modified",
    }
    .to_string()
}

fn git_output(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| error.to_string())?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git {:?} failed", args)
        } else {
            stderr
        });
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

fn git_output_permissive(cwd: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .map_err(|error| error.to_string())?;
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    if output.status.success() || !stdout.trim().is_empty() {
        return Ok(stdout);
    }
    Ok(String::new())
}

fn repository_root(path: &str) -> Result<String, String> {
    git_output(Path::new(path), &["rev-parse", "--show-toplevel"])
        .map(|value| value.trim().to_string())
}

fn has_resolvable_head(path: &str) -> bool {
    git_output(Path::new(path), &["rev-parse", "--verify", "HEAD"]).is_ok()
}

fn is_untracked_path(root: &str, path: &str) -> bool {
    git_output(
        Path::new(root),
        &["ls-files", "--others", "--exclude-standard", "--", path],
    )
    .map(|output| output.lines().map(str::trim).any(|line| line == path))
    .unwrap_or(false)
}

fn normalized(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_watch_filter_allows_worktree_and_known_metadata() {
        let repository = "/repo/app";
        let git_dirs = vec!["/repo/app/.git".to_string()];

        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/src/main.rs"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/HEAD"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/index"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/refs/heads/main"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/logs/HEAD"
        ));
        assert!(should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/FETCH_HEAD"
        ));
    }

    #[test]
    fn git_watch_filter_ignores_git_object_churn() {
        let repository = "/repo/app";
        let git_dirs = vec!["/repo/app/.git".to_string()];

        assert!(!should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git"
        ));
        assert!(!should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/objects/ab/cdef"
        ));
        assert!(!should_forward_git_watch_path(
            repository,
            &git_dirs,
            "/repo/app/.git/modules/dependency/config"
        ));
    }

    #[test]
    fn parse_git_log_keeps_graph_author_and_decorations() {
        let log = "* \t0123456789abcdef\u{1f}HEAD -> main, tag: v1\u{1f}2 hours ago\u{1f}Add git watcher\u{1f}Alice\u{1e}";
        let commits = parse_git_log(log);

        assert_eq!(commits.len(), 1);
        assert_eq!(commits[0].hash, "0123456789abcdef");
        assert_eq!(commits[0].graph_prefix, "*");
        assert_eq!(
            commits[0].decorations.as_deref(),
            Some("HEAD -> main, tag: v1")
        );
        assert_eq!(commits[0].relative_time, "2 hours ago");
        assert_eq!(commits[0].title, "Add git watcher");
        assert_eq!(commits[0].author, "Alice");
    }
}
