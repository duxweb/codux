use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};
use serde::Deserialize;
use serde::Serialize;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
use tauri::AppHandle;

const REVIEW_UNTRACKED_LINE_COUNT_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
const GIT_WATCH_DEBOUNCE_MS: u64 = 250;
const COMMIT_CONTEXT_MAX_CHARS: usize = 24_000;
const COMMIT_CONTEXT_MAX_FILES: usize = 80;
const COMMIT_CONTEXT_MAX_LINES_PER_FILE: usize = 80;
const CODUX_MANAGED_MEMORY_ENTRYPOINT_MARKER: &str = "<!-- CODUX_MANAGED_MEMORY_ENTRYPOINT -->";

type GitRepository = git2::Repository;
pub type GitCancelToken = Arc<AtomicBool>;

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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitCommitMessageContextSnapshot {
    pub diff: String,
    pub truncated: bool,
    pub is_repository: bool,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewContentSnapshot {
    pub path: String,
    pub head_content: String,
    pub base_content: Option<String>,
    pub index_content: Option<String>,
    pub worktree_content: String,
    pub added_lines: Vec<i64>,
    pub deleted_lines: Vec<i64>,
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
pub struct GitReviewContentRequest {
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
    project_paths: Arc<Mutex<HashSet<String>>>,
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
        _app: AppHandle,
        project_path: String,
        on_changed: impl Fn(GitRepositoryChangeEvent) + Send + Sync + 'static,
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
        if let Some(existing) = watchers.get(&key) {
            if let Ok(mut paths) = existing.project_paths.lock() {
                paths.insert(watch_target.project_path.clone());
            }
            return Ok(registration);
        }

        let project_paths_for_event = Arc::new(Mutex::new(HashSet::from([watch_target
            .project_path
            .clone()])));
        let repository_path_for_event = watch_target.repository_path.clone();
        let repository_key = watch_target.repository_key.clone();
        let git_dir_keys = watch_target.git_dir_keys.clone();
        let on_changed = Arc::new(on_changed);
        let (change_tx, change_rx) = mpsc::channel::<Vec<String>>();
        let debounced_paths = Arc::clone(&project_paths_for_event);
        let debounced_repository_path = repository_path_for_event.clone();
        let debounced_on_changed = Arc::clone(&on_changed);
        thread::Builder::new()
            .name("codux-git-watch-debounce".to_string())
            .spawn(move || {
                run_git_watch_debounce(
                    change_rx,
                    debounced_paths,
                    debounced_repository_path,
                    debounced_on_changed,
                );
            })
            .map_err(|error| error.to_string())?;
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
            let _ = change_tx.send(changed_paths);
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
                project_paths: project_paths_for_event,
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
        if let Some(watcher) = watchers.get(&repository_key) {
            let mut should_remove = false;
            if let Ok(mut paths) = watcher.project_paths.lock() {
                should_remove = remove_watched_project_path(&mut paths, &requested_key);
            }
            if should_remove {
                watchers.remove(&repository_key);
            }
            return Ok(());
        }
        watchers.retain(|_, watcher| {
            let mut should_remove = false;
            if let Ok(mut paths) = watcher.project_paths.lock() {
                should_remove = remove_watched_project_path(&mut paths, &requested_key);
            }
            !should_remove
        });
        Ok(())
    }
}

fn run_git_watch_debounce(
    rx: mpsc::Receiver<Vec<String>>,
    watched_project_paths: Arc<Mutex<HashSet<String>>>,
    repository_path: String,
    on_changed: Arc<impl Fn(GitRepositoryChangeEvent) + Send + Sync + 'static>,
) {
    while let Ok(paths) = rx.recv() {
        let mut changed_paths = paths;
        loop {
            match rx.recv_timeout(Duration::from_millis(GIT_WATCH_DEBOUNCE_MS)) {
                Ok(next_paths) => push_unique_strings(&mut changed_paths, next_paths),
                Err(RecvTimeoutError::Timeout) => break,
                Err(RecvTimeoutError::Disconnected) => return,
            }
        }
        let project_paths = watched_project_paths
            .lock()
            .map(|paths| paths.iter().cloned().collect::<Vec<_>>())
            .unwrap_or_default();
        for project_path in project_paths {
            on_changed(GitRepositoryChangeEvent {
                project_path,
                repository_path: repository_path.clone(),
                changed_paths: changed_paths.clone(),
            });
        }
    }
}

fn push_unique_strings(target: &mut Vec<String>, values: Vec<String>) {
    for value in values {
        if !target.iter().any(|existing| existing == &value) {
            target.push(value);
        }
    }
}

fn remove_watched_project_path(paths: &mut HashSet<String>, requested_key: &str) -> bool {
    paths.retain(|path| normalized_path_key(Path::new(path)) != requested_key);
    paths.is_empty()
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
    if let Ok(repo) = GitRepository::discover(root) {
        push_unique_path(&mut dirs, repo.path().to_path_buf());
        push_unique_path(&mut dirs, repo.commondir().to_path_buf());
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
        match relative.as_str() {
            "head" | "index" | "fetch_head" | "orig_head" | "packed-refs" => true,
            _ => relative.starts_with("refs/") || relative.starts_with("logs/head"),
        }
    }

    #[cfg(not(windows))]
    {
        match relative {
            "HEAD" | "index" | "FETCH_HEAD" | "ORIG_HEAD" | "packed-refs" => true,
            _ => relative.starts_with("refs/") || relative.starts_with("logs/HEAD"),
        }
    }
}

pub fn git_brief_status(project_path: String) -> GitBriefStatus {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
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
    let status = git_status_from_repo(&repo);
    GitBriefStatus {
        branch: status.branch,
        ahead: status.ahead,
        behind: status.behind,
        changes: status.staged.len() + status.unstaged.len() + status.untracked.len(),
        is_repository: true,
        error: None,
    }
}

pub fn git_status(project_path: String) -> GitStatusSnapshot {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
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
    git_status_from_repo(&repo)
}

pub fn git_stage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    stage_paths_git2(&repo, &request.paths)?;
    Ok(git_status(root))
}

pub fn git_unstage(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    unstage_paths_git2(&repo, &request.paths)?;
    Ok(git_status(root))
}

pub fn git_commit(request: GitCommitRequest) -> Result<GitStatusSnapshot, String> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err("Commit message cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    create_commit_git2(&repo, message, false)?;
    Ok(git_status(root))
}

pub fn git_commit_action(request: GitCommitActionRequest) -> Result<GitStatusSnapshot, String> {
    let message = request.message.trim();
    if message.is_empty() {
        return Err("Commit message cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    create_commit_git2(&repo, message, false)?;
    match request.action.as_str() {
        "commit" => {}
        "commitAndPush" => {
            push_current_branch_git2(&repo, None, false, None)?;
        }
        "commitAndSync" => {
            pull_current_branch_git2(&repo, None)?;
            push_current_branch_git2(&repo, None, false, None)?;
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
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    create_commit_git2(&repo, message, true)?;
    Ok(git_status(root))
}

pub fn git_last_commit_message(project_path: String) -> Result<String, String> {
    let repo = open_git_repository(&project_path)?;
    let commit = repo
        .head()
        .map_err(|error| error.message().to_string())?
        .peel_to_commit()
        .map_err(|error| error.message().to_string())?;
    Ok(commit.summary().ok().flatten().unwrap_or("").to_string())
}

pub fn git_undo_last_commit(project_path: String) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    soft_reset_to_parent_git2(&repo)?;
    Ok(git_status(root))
}

pub fn git_head_commit_pushed(project_path: String) -> Result<bool, String> {
    let repo = open_git_repository(&project_path)?;
    let Some(head) = repo.head().ok().and_then(|head| head.target()) else {
        return Ok(false);
    };
    let Some(upstream) = upstream_branch_name(&repo) else {
        return Ok(false);
    };
    let upstream_ref = format!("refs/remotes/{upstream}");
    let Some(upstream_target) = repo
        .find_reference(&upstream_ref)
        .ok()
        .and_then(|reference| reference.target())
    else {
        return Ok(false);
    };
    Ok(repo
        .graph_descendant_of(upstream_target, head)
        .unwrap_or(false))
}

pub fn git_init(project_path: String) -> Result<GitStatusSnapshot, String> {
    let path = Path::new(project_path.trim());
    if !path.exists() {
        return Err(format!("Project path does not exist: {}", path.display()));
    }
    GitRepository::init(path).map_err(|error| error.message().to_string())?;
    Ok(git_status(path.display().to_string()))
}

pub fn git_clone(request: GitCloneRequest) -> Result<GitStatusSnapshot, String> {
    let remote_url = request.remote_url.trim();
    if remote_url.is_empty() {
        return Err("Remote URL cannot be empty.".to_string());
    }
    let project_path = Path::new(request.project_path.trim());
    clone_repository_git2(remote_url, project_path)?;
    Ok(git_status(project_path.display().to_string()))
}

pub fn git_discard(request: GitPathsRequest) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    discard_paths_git2(&repo, &request.paths)?;
    Ok(git_status(root))
}

pub fn git_branches(project_path: String) -> GitBranchesSnapshot {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
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
    let current = current_branch_name(&repo);
    let local = git2_branches(&repo, git2::BranchType::Local, &current);
    let remote = git2_branches(&repo, git2::BranchType::Remote, &current)
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
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    checkout_branch_git2(&repo, branch)?;
    Ok(git_status(root))
}

pub fn git_create_branch(request: GitCreateBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    let from = request
        .from
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    create_branch_git2(&repo, branch, from, request.checkout)?;
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
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    checkout_remote_branch_git2(&repo, remote_branch, local_name)?;
    Ok(git_status(root))
}

pub fn git_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    merge_branch_git2(&repo, branch, false)?;
    Ok(git_status(root))
}

pub fn git_squash_merge_branch(request: GitBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    merge_branch_git2(&repo, branch, true)?;
    Ok(git_status(root))
}

pub fn git_delete_branch(request: GitDeleteBranchRequest) -> Result<GitStatusSnapshot, String> {
    let branch = request.branch.trim();
    if branch.is_empty() {
        return Err("Branch name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    delete_branch_git2(&repo, branch, request.force)?;
    Ok(git_status(root))
}

pub fn git_checkout_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    let commit = request.commit.trim();
    if commit.is_empty() {
        return Err("Commit cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    checkout_commit_git2(&repo, commit)?;
    Ok(git_status(root))
}

pub fn git_revert_commit(request: GitCommitRefRequest) -> Result<GitStatusSnapshot, String> {
    let commit = request.commit.trim();
    if commit.is_empty() {
        return Err("Commit cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    revert_commit_git2(&repo, commit)?;
    Ok(git_status(root))
}

pub fn git_restore_commit(request: GitRestoreCommitRequest) -> Result<GitStatusSnapshot, String> {
    let commit = request.commit.trim();
    if commit.is_empty() {
        return Err("Commit cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    hard_reset_git2(&repo, commit)?;
    if request.force_remote {
        push_current_branch_git2(&repo, None, true, None)?;
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
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    repo.remote(name, url)
        .map_err(|error| error.message().to_string())?;
    Ok(git_status(root))
}

pub fn git_remove_remote(request: GitRemoteRequest) -> Result<GitStatusSnapshot, String> {
    let name = request.name.trim();
    if name.is_empty() {
        return Err("Remote name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    repo.remote_delete(name)
        .map_err(|error| error.message().to_string())?;
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

pub fn git_fetch_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    fetch_all_remotes_git2(&repo, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_sync_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    pull_current_branch_git2(&repo, cancel.as_ref())?;
    push_current_branch_git2(&repo, None, false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_push_remote_with_cancel(
    request: GitPushRemoteRequest,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let remote = request.remote.trim();
    if remote.is_empty() {
        return Err("Remote name cannot be empty.".to_string());
    }
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    let branch = current_local_branch_name(&root)?;
    if branch.is_empty() {
        return Err("Cannot push detached HEAD to a remote.".to_string());
    }
    push_current_branch_git2(&repo, Some(remote), false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_push_remote_branch_with_cancel(
    request: GitPushRemoteBranchRequest,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let remote_branch = request.remote_branch.trim();
    if remote_branch.is_empty() {
        return Err("Remote branch cannot be empty.".to_string());
    }
    let (remote, branch_name) = remote_branch
        .split_once('/')
        .ok_or_else(|| "Remote branch must include a remote name.".to_string())?;
    let repo = open_git_repository(&request.project_path)?;
    let root = repo_root(&repo).display().to_string();
    let local_branch = match request
        .local_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        Some(branch) => branch.to_string(),
        None => current_local_branch_name(&root)?,
    };
    if local_branch.is_empty() {
        return Err("Cannot push detached HEAD to a remote branch.".to_string());
    }
    let refspec = format!("{local_branch}:{branch_name}");
    push_refspec_git2(&repo, remote, &refspec, false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_pull_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    pull_current_branch_git2(&repo, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_push_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    push_current_branch_git2(&repo, None, false, cancel.as_ref())?;
    Ok(git_status(root))
}

pub fn git_force_push_with_cancel(
    project_path: String,
    cancel: Option<GitCancelToken>,
) -> Result<GitStatusSnapshot, String> {
    let repo = open_git_repository(&project_path)?;
    let root = repo_root(&repo).display().to_string();
    push_current_branch_git2(&repo, None, true, cancel.as_ref())?;
    Ok(git_status(root))
}

#[cfg(test)]
fn git_push_remote(request: GitPushRemoteRequest) -> Result<GitStatusSnapshot, String> {
    git_push_remote_with_cancel(request, None)
}

#[cfg(test)]
fn git_pull(project_path: String) -> Result<GitStatusSnapshot, String> {
    git_pull_with_cancel(project_path, None)
}

#[cfg(test)]
fn git_push(project_path: String) -> Result<GitStatusSnapshot, String> {
    git_push_with_cancel(project_path, None)
}

pub fn git_diff_file(request: GitDiffRequest) -> GitDiffSnapshot {
    let repo = match open_git_repository(&request.project_path) {
        Ok(repo) => repo,
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
        git2_diff_to_string(&repo, DiffTarget::Index, Some(path), 3)
    } else {
        git2_diff_to_string(&repo, DiffTarget::Worktree, Some(path), 3)
    }
    .unwrap_or_default();
    let diff = if diff.trim().is_empty() {
        if !request.staged && is_untracked_path_git2(&repo, path) {
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

pub fn git_commit_message_context(project_path: String) -> GitCommitMessageContextSnapshot {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
        Err(error) => {
            return GitCommitMessageContextSnapshot {
                diff: String::new(),
                truncated: false,
                is_repository: false,
                error: Some(error),
            };
        }
    };
    match git2_diff_to_string(&repo, DiffTarget::Index, None, 1) {
        Ok(diff) => {
            let (diff, truncated) = compact_commit_message_diff(&diff);
            GitCommitMessageContextSnapshot {
                diff,
                truncated,
                is_repository: true,
                error: None,
            }
        }
        Err(error) => GitCommitMessageContextSnapshot {
            diff: String::new(),
            truncated: false,
            is_repository: true,
            error: Some(error),
        },
    }
}

pub fn git_review_diff_file(request: GitReviewDiffRequest) -> GitDiffSnapshot {
    let repo = match open_git_repository(&request.project_path) {
        Ok(repo) => repo,
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
        git2_commit_diff_to_string(&repo, base, Some(path), 3).unwrap_or_default()
    } else {
        let unstaged =
            git2_diff_to_string(&repo, DiffTarget::Worktree, Some(path), 3).unwrap_or_default();
        let staged =
            git2_diff_to_string(&repo, DiffTarget::Index, Some(path), 3).unwrap_or_default();
        match (staged.trim().is_empty(), unstaged.trim().is_empty()) {
            (true, true) if is_untracked_path_git2(&repo, path) => {
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

pub fn git_review_file_content(request: GitReviewContentRequest) -> GitReviewContentSnapshot {
    let repo = match open_git_repository(&request.project_path) {
        Ok(repo) => repo,
        Err(error) => {
            return GitReviewContentSnapshot {
                path: request.path,
                head_content: String::new(),
                base_content: None,
                index_content: None,
                worktree_content: String::new(),
                added_lines: Vec::new(),
                deleted_lines: Vec::new(),
                is_repository: false,
                error: Some(error),
            };
        }
    };
    let path = request.path.trim();
    if path.is_empty() {
        return GitReviewContentSnapshot {
            path: String::new(),
            head_content: String::new(),
            base_content: None,
            index_content: None,
            worktree_content: String::new(),
            added_lines: Vec::new(),
            deleted_lines: Vec::new(),
            is_repository: true,
            error: Some("File path cannot be empty.".to_string()),
        };
    }

    let base = request
        .base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current branch");
    let head_content = git2_blob_or_empty(&repo, "HEAD", path);
    let base_content = base.map(|reference| git2_blob_or_empty(&repo, reference, path));
    let index_content = git2_index_blob(&repo, path).ok();
    let worktree_content = read_worktree_file(repo_root(&repo), path).unwrap_or_default();
    let diff = if let Some(base) = base {
        git2_commit_diff_to_string(&repo, base, Some(path), 0).unwrap_or_default()
    } else {
        let unstaged =
            git2_diff_to_string(&repo, DiffTarget::Worktree, Some(path), 0).unwrap_or_default();
        let staged =
            git2_diff_to_string(&repo, DiffTarget::Index, Some(path), 0).unwrap_or_default();
        match (staged.trim().is_empty(), unstaged.trim().is_empty()) {
            (true, _) => unstaged,
            (_, true) => staged,
            _ => format!("{staged}\n{unstaged}"),
        }
    };
    let (deleted_lines, added_lines) = parse_diff_line_numbers(&diff);

    GitReviewContentSnapshot {
        path: path.to_string(),
        head_content,
        base_content,
        index_content,
        worktree_content,
        added_lines,
        deleted_lines,
        is_repository: true,
        error: None,
    }
}

pub fn git_review(project_path: String, base_branch: Option<String>) -> GitReviewSnapshot {
    let repo = match open_git_repository(&project_path) {
        Ok(repo) => repo,
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
    let root = repo_root(&repo);
    let base = base_branch
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty() && *value != "current branch")
        .map(str::to_string);

    if let Some(base) = base {
        let files = git2_commit_review_files(&repo, &base).unwrap_or_default();
        let diff_stat = review_diff_stat(&files);
        return GitReviewSnapshot {
            mode: "taskBranch".to_string(),
            title: "Worktree Review".to_string(),
            base_branch: Some(base),
            diff_stat,
            files,
            is_repository: true,
            error: None,
        };
    }

    let status = git_status_from_repo(&repo);
    let stats = working_tree_review_stats_git2(&repo);
    let mut seen_paths = HashSet::new();
    let mut files = Vec::new();
    for file in &status.staged {
        push_review_file_from_status(&mut files, &mut seen_paths, file, "staged", &stats, root);
    }
    for file in &status.unstaged {
        push_review_file_from_status(&mut files, &mut seen_paths, file, "modified", &stats, root);
    }
    for file in &status.untracked {
        push_review_file_from_status(&mut files, &mut seen_paths, file, "added", &stats, root);
    }
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

fn open_git_repository(path: &str) -> Result<GitRepository, String> {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return Err("Project path cannot be empty.".to_string());
    }
    GitRepository::discover(path).map_err(|error| error.message().to_string())
}

fn repo_root(repo: &GitRepository) -> &Path {
    repo.workdir()
        .or_else(|| repo.path().parent())
        .unwrap_or_else(|| Path::new(""))
}

fn git_status_from_repo(repo: &GitRepository) -> GitStatusSnapshot {
    let branch = current_branch_name(repo);
    let upstream = upstream_branch_name(repo);
    let (ahead, behind) = ahead_behind(repo).unwrap_or((0, 0));
    let (staged, unstaged, untracked) = git2_status_files(repo);
    let commits = git2_commit_log(repo, 20);
    let branches = git2_branches(repo, git2::BranchType::Local, &branch);
    let remote_branches = git2_branches(repo, git2::BranchType::Remote, &branch)
        .into_iter()
        .map(|branch| branch.name)
        .filter(|name| !name.ends_with("/HEAD") && name.contains('/'))
        .collect();
    let remotes = git2_remotes(repo);
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

fn stage_paths_git2(repo: &GitRepository, paths: &[String]) -> Result<(), String> {
    let mut index = repo.index().map_err(|error| error.message().to_string())?;
    if paths.is_empty() {
        index
            .add_all(
                ["*"].iter(),
                git2::IndexAddOption::DEFAULT,
                Some(&mut |path, _| {
                    if is_codux_managed_memory_entrypoint_path(repo, path) {
                        1
                    } else {
                        0
                    }
                }),
            )
            .map_err(|error| error.message().to_string())?;
        remove_codux_managed_memory_entrypoint_from_index(repo, &mut index);
    } else {
        for path in normalized_pathspecs(paths) {
            if is_codux_managed_memory_entrypoint(repo, &path) {
                let _ = index.remove_path(Path::new(&path));
                continue;
            }
            if repo_root(repo).join(&path).exists() {
                index
                    .add_path(Path::new(&path))
                    .map_err(|error| error.message().to_string())?;
            } else {
                let _ = index.remove_path(Path::new(&path));
            }
        }
    }
    index.write().map_err(|error| error.message().to_string())
}

fn unstage_paths_git2(repo: &GitRepository, paths: &[String]) -> Result<(), String> {
    if let Ok(head) = repo.head() {
        let target = head
            .peel(git2::ObjectType::Commit)
            .map_err(|error| error.message().to_string())?;
        let pathspecs = if paths.is_empty() {
            vec![".".to_string()]
        } else {
            normalized_pathspecs(paths)
        };
        repo.reset_default(Some(&target), pathspecs.iter().map(String::as_str))
            .map_err(|error| error.message().to_string())
    } else {
        let mut index = repo.index().map_err(|error| error.message().to_string())?;
        if paths.is_empty() {
            index.clear().map_err(|error| error.message().to_string())?;
        } else {
            for path in normalized_pathspecs(paths) {
                let _ = index.remove_path(Path::new(&path));
            }
        }
        index.write().map_err(|error| error.message().to_string())
    }
}

fn discard_paths_git2(repo: &GitRepository, paths: &[String]) -> Result<(), String> {
    unstage_paths_git2(repo, paths)?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout
        .force()
        .remove_untracked(true)
        .recreate_missing(true);
    if !paths.is_empty() {
        checkout.disable_pathspec_match(true);
        for path in normalized_pathspecs(paths) {
            checkout.path(path);
        }
    }
    repo.checkout_head(Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}

fn create_commit_git2(
    repo: &GitRepository,
    message: &str,
    amend: bool,
) -> Result<git2::Oid, String> {
    let mut index = repo.index().map_err(|error| error.message().to_string())?;
    if index.has_conflicts() {
        return Err("Cannot commit while the index has conflicts.".to_string());
    }
    let tree_id = index
        .write_tree()
        .map_err(|error| error.message().to_string())?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|error| error.message().to_string())?;
    if !amend && !commit_tree_has_changes(repo, &tree) {
        return Err("No staged changes to commit.".to_string());
    }
    let signature = repo_signature(repo)?;
    if amend {
        let parent = repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|error| error.message().to_string())?;
        return parent
            .amend(
                Some("HEAD"),
                Some(&signature),
                Some(&signature),
                None,
                Some(message),
                Some(&tree),
            )
            .map_err(|error| error.message().to_string());
    }
    let parents = repo
        .head()
        .ok()
        .and_then(|head| head.peel_to_commit().ok())
        .into_iter()
        .collect::<Vec<_>>();
    let parent_refs = parents.iter().collect::<Vec<_>>();
    repo.commit(
        Some("HEAD"),
        &signature,
        &signature,
        message,
        &tree,
        &parent_refs,
    )
    .map_err(|error| error.message().to_string())
}

fn commit_tree_has_changes(repo: &GitRepository, tree: &git2::Tree<'_>) -> bool {
    if let Ok(head) = repo.head().and_then(|head| head.peel_to_commit()) {
        return head.tree_id() != tree.id();
    }
    !tree.is_empty()
}

fn repo_signature(repo: &GitRepository) -> Result<git2::Signature<'_>, String> {
    repo.signature()
        .or_else(|_| git2::Signature::now("Codux", "codux@example.local"))
        .map_err(|error| error.message().to_string())
}

fn soft_reset_to_parent_git2(repo: &GitRepository) -> Result<(), String> {
    let head = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let parent = head
        .parent(0)
        .map_err(|error| error.message().to_string())?;
    let target = parent.as_object();
    repo.reset(target, git2::ResetType::Soft, None)
        .map_err(|error| error.message().to_string())
}

fn checkout_branch_git2(repo: &GitRepository, branch: &str) -> Result<(), String> {
    let reference_name = if branch.starts_with("refs/") {
        branch.to_string()
    } else {
        format!("refs/heads/{branch}")
    };
    let reference = repo
        .find_reference(&reference_name)
        .map_err(|error| error.message().to_string())?;
    let object = reference
        .peel(git2::ObjectType::Commit)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&object, Some(&mut checkout))
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&reference_name)
        .map_err(|error| error.message().to_string())
}

fn create_branch_git2(
    repo: &GitRepository,
    branch: &str,
    from: Option<&str>,
    checkout: bool,
) -> Result<(), String> {
    let commit = match from {
        Some(from) => repo
            .revparse_single(from)
            .and_then(|object| object.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
        None => repo
            .head()
            .and_then(|head| head.peel_to_commit())
            .map_err(|error| error.message().to_string())?,
    };
    repo.branch(branch, &commit, false)
        .map_err(|error| error.message().to_string())?;
    if checkout {
        checkout_branch_git2(repo, branch)?;
    }
    Ok(())
}

fn checkout_remote_branch_git2(
    repo: &GitRepository,
    remote_branch: &str,
    local_name: &str,
) -> Result<(), String> {
    let remote_ref = format!("refs/remotes/{remote_branch}");
    let commit = repo
        .find_reference(&remote_ref)
        .and_then(|reference| reference.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let mut branch = repo
        .branch(local_name, &commit, false)
        .map_err(|error| error.message().to_string())?;
    branch
        .set_upstream(Some(remote_branch))
        .map_err(|error| error.message().to_string())?;
    checkout_branch_git2(repo, local_name)
}

fn checkout_commit_git2(repo: &GitRepository, reference: &str) -> Result<(), String> {
    let object = repo
        .revparse_single(reference)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.safe();
    repo.checkout_tree(&object, Some(&mut checkout))
        .map_err(|error| error.message().to_string())?;
    repo.set_head_detached(object.id())
        .map_err(|error| error.message().to_string())
}

fn hard_reset_git2(repo: &GitRepository, reference: &str) -> Result<(), String> {
    let object = repo
        .revparse_single(reference)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force().remove_untracked(true);
    repo.reset(&object, git2::ResetType::Hard, Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}

fn delete_branch_git2(repo: &GitRepository, branch: &str, force: bool) -> Result<(), String> {
    if current_branch_name(repo) == branch {
        return Err(format!("Cannot delete the checked out branch: {branch}"));
    }
    let mut local_branch = repo
        .find_branch(branch, git2::BranchType::Local)
        .map_err(|error| error.message().to_string())?;
    if !force {
        let head = repo.head().ok().and_then(|head| head.target());
        let target = local_branch.get().target();
        if let (Some(head), Some(target)) = (head, target) {
            if !repo.graph_descendant_of(head, target).unwrap_or(false) {
                return Err(format!("Branch {branch} is not fully merged."));
            }
        }
    }
    local_branch
        .delete()
        .map_err(|error| error.message().to_string())
}

fn revert_commit_git2(repo: &GitRepository, reference: &str) -> Result<(), String> {
    let commit = repo
        .revparse_single(reference)
        .and_then(|object| object.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    repo.revert(&commit, None)
        .map_err(|error| error.message().to_string())?;
    if repo
        .index()
        .map_err(|error| error.message().to_string())?
        .has_conflicts()
    {
        return Err("Revert produced conflicts. Resolve them manually.".to_string());
    }
    let summary = commit.summary().ok().flatten().unwrap_or(reference);
    create_commit_git2(repo, &format!("Revert \"{summary}\""), false)?;
    repo.cleanup_state()
        .map_err(|error| error.message().to_string())
}

fn merge_branch_git2(repo: &GitRepository, branch: &str, squash: bool) -> Result<(), String> {
    let annotated = annotated_commit_for_branch(repo, branch)?;
    let (analysis, _) = repo
        .merge_analysis(&[&annotated])
        .map_err(|error| error.message().to_string())?;
    if analysis.is_up_to_date() {
        return Ok(());
    }
    if analysis.is_fast_forward() && !squash {
        let target = annotated.id();
        fast_forward_head(repo, target)?;
        return Ok(());
    }
    let head_commit = repo
        .head()
        .and_then(|head| head.peel_to_commit())
        .map_err(|error| error.message().to_string())?;
    let their_commit = repo
        .find_commit(annotated.id())
        .map_err(|error| error.message().to_string())?;
    let mut index = repo
        .merge_commits(&head_commit, &their_commit, None)
        .map_err(|error| error.message().to_string())?;
    if index.has_conflicts() {
        repo.checkout_index(Some(&mut index), None)
            .map_err(|error| error.message().to_string())?;
        return Err("Merge produced conflicts. Resolve them manually.".to_string());
    }
    let tree_id = index
        .write_tree_to(repo)
        .map_err(|error| error.message().to_string())?;
    let tree = repo
        .find_tree(tree_id)
        .map_err(|error| error.message().to_string())?;
    repo.checkout_tree(
        tree.as_object(),
        Some(git2::build::CheckoutBuilder::new().safe()),
    )
    .map_err(|error| error.message().to_string())?;
    repo.index()
        .and_then(|mut repo_index| {
            repo_index.read_tree(&tree)?;
            repo_index.write()
        })
        .map_err(|error| error.message().to_string())?;
    if !squash {
        let signature = repo_signature(repo)?;
        let message = format!("Merge branch '{branch}'");
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            &message,
            &tree,
            &[&head_commit, &their_commit],
        )
        .map_err(|error| error.message().to_string())?;
    }
    repo.cleanup_state()
        .map_err(|error| error.message().to_string())
}

fn annotated_commit_for_branch<'repo>(
    repo: &'repo GitRepository,
    branch: &str,
) -> Result<git2::AnnotatedCommit<'repo>, String> {
    let object = repo
        .revparse_single(branch)
        .or_else(|_| repo.revparse_single(&format!("refs/heads/{branch}")))
        .map_err(|error| error.message().to_string())?;
    repo.find_annotated_commit(object.id())
        .map_err(|error| error.message().to_string())
}

fn fast_forward_head(repo: &GitRepository, target: git2::Oid) -> Result<(), String> {
    let head_name = repo
        .head()
        .ok()
        .and_then(|head| head.name().ok().map(str::to_string))
        .ok_or_else(|| "Cannot fast-forward detached HEAD.".to_string())?;
    let mut reference = repo
        .find_reference(&head_name)
        .map_err(|error| error.message().to_string())?;
    reference
        .set_target(target, "Fast-forward")
        .map_err(|error| error.message().to_string())?;
    repo.set_head(&head_name)
        .map_err(|error| error.message().to_string())?;
    let mut checkout = git2::build::CheckoutBuilder::new();
    checkout.force();
    repo.checkout_head(Some(&mut checkout))
        .map_err(|error| error.message().to_string())
}

fn clone_repository_git2(remote_url: &str, project_path: &Path) -> Result<(), String> {
    let mut fetch_options = git2::FetchOptions::new();
    fetch_options.remote_callbacks(git_remote_callbacks(None));
    let mut builder = git2::build::RepoBuilder::new();
    builder.fetch_options(fetch_options);
    builder
        .clone(remote_url, project_path)
        .map(|_| ())
        .map_err(|error| error.message().to_string())
}

fn fetch_all_remotes_git2(
    repo: &GitRepository,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    let names = repo
        .remotes()
        .map_err(|error| error.message().to_string())?;
    for name in names.iter().flatten().flatten() {
        check_git_cancelled(cancel)?;
        fetch_remote_git2(repo, name, cancel)?;
    }
    Ok(())
}

fn fetch_remote_git2(
    repo: &GitRepository,
    remote_name: &str,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|error| error.message().to_string())?;
    let mut options = git2::FetchOptions::new();
    options.remote_callbacks(git_remote_callbacks(cancel.cloned()));
    remote
        .fetch(&[] as &[&str], Some(&mut options), None)
        .map_err(git_error_message)
}

fn pull_current_branch_git2(
    repo: &GitRepository,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    let branch_name = current_branch_name(repo);
    if branch_name == "HEAD" || branch_name == "uninitialized" {
        return Err("Cannot pull detached HEAD.".to_string());
    }
    let mut branch = repo
        .find_branch(&branch_name, git2::BranchType::Local)
        .map_err(|error| error.message().to_string())?;
    let upstream = branch
        .upstream()
        .map_err(|_| "The current branch does not have an upstream.".to_string())?;
    let upstream_name = upstream
        .name()
        .ok()
        .flatten()
        .ok_or_else(|| "The upstream branch name is invalid.".to_string())?
        .to_string();
    let remote_name = upstream_name
        .split_once('/')
        .map(|(remote, _)| remote)
        .ok_or_else(|| "The upstream branch is missing a remote name.".to_string())?;
    fetch_remote_git2(repo, remote_name, cancel)?;
    check_git_cancelled(cancel)?;
    branch = repo
        .find_branch(&branch_name, git2::BranchType::Local)
        .map_err(|error| error.message().to_string())?;
    let local_oid = branch
        .get()
        .target()
        .ok_or_else(|| "The current branch target is invalid.".to_string())?;
    let upstream_ref = repo
        .find_reference(&format!("refs/remotes/{upstream_name}"))
        .map_err(|error| error.message().to_string())?;
    let upstream_oid = upstream_ref
        .target()
        .ok_or_else(|| "The upstream target is invalid.".to_string())?;
    let (ahead, behind) = repo
        .graph_ahead_behind(local_oid, upstream_oid)
        .map_err(|error| error.message().to_string())?;
    if behind == 0 {
        return Ok(());
    }
    if ahead > 0 {
        return rebase_current_branch_git2(repo, upstream_oid, cancel);
    }
    fast_forward_head(repo, upstream_oid)
}

fn rebase_current_branch_git2(
    repo: &GitRepository,
    upstream_oid: git2::Oid,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    let upstream = repo
        .find_annotated_commit(upstream_oid)
        .map_err(|error| error.message().to_string())?;
    let mut options = git2::RebaseOptions::new();
    let mut rebase = repo
        .rebase(None, Some(&upstream), None, Some(&mut options))
        .map_err(|error| error.message().to_string())?;
    let signature = repo_signature(repo)?;
    while let Some(operation) = rebase.next() {
        check_git_cancelled(cancel)?;
        operation.map_err(|error| error.message().to_string())?;
        if repo
            .index()
            .map_err(|error| error.message().to_string())?
            .has_conflicts()
        {
            return Err("Pull rebase produced conflicts. Resolve them manually.".to_string());
        }
        rebase
            .commit(None, &signature, None)
            .map_err(|error| error.message().to_string())?;
    }
    rebase
        .finish(Some(&signature))
        .map_err(|error| error.message().to_string())
}

fn push_current_branch_git2(
    repo: &GitRepository,
    remote_override: Option<&str>,
    force: bool,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    let branch = current_branch_name(repo);
    if branch == "HEAD" || branch == "uninitialized" {
        return Err("Cannot push detached HEAD.".to_string());
    }
    let remote = remote_override
        .map(str::to_string)
        .or_else(|| upstream_remote_for_branch(repo, &branch))
        .or_else(|| first_remote_name(repo))
        .ok_or_else(|| "No Git remote is configured.".to_string())?;
    let refspec = if force {
        format!("+refs/heads/{branch}:refs/heads/{branch}")
    } else {
        format!("refs/heads/{branch}:refs/heads/{branch}")
    };
    push_refspec_git2(repo, &remote, &refspec, force, cancel)?;
    if let Ok(mut branch_ref) = repo.find_branch(&branch, git2::BranchType::Local) {
        let _ = branch_ref.set_upstream(Some(&format!("{remote}/{branch}")));
    }
    Ok(())
}

fn push_refspec_git2(
    repo: &GitRepository,
    remote_name: &str,
    refspec: &str,
    _force: bool,
    cancel: Option<&GitCancelToken>,
) -> Result<(), String> {
    check_git_cancelled(cancel)?;
    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|error| error.message().to_string())?;
    let mut options = git2::PushOptions::new();
    options.remote_callbacks(git_remote_callbacks(cancel.cloned()));
    remote
        .push(&[refspec], Some(&mut options))
        .map_err(git_error_message)?;
    check_git_cancelled(cancel)
}

fn upstream_remote_for_branch(repo: &GitRepository, branch: &str) -> Option<String> {
    let local = repo.find_branch(branch, git2::BranchType::Local).ok()?;
    let upstream = local.upstream().ok()?;
    let name = upstream.name().ok().flatten()?;
    name.split_once('/').map(|(remote, _)| remote.to_string())
}

fn first_remote_name(repo: &GitRepository) -> Option<String> {
    repo.remotes()
        .ok()?
        .iter()
        .flatten()
        .flatten()
        .next()
        .map(str::to_string)
}

fn git_remote_callbacks<'a>(cancel: Option<GitCancelToken>) -> git2::RemoteCallbacks<'a> {
    let mut callbacks = git2::RemoteCallbacks::new();
    let transfer_cancel = cancel.clone();
    callbacks.transfer_progress(move |_| !is_git_cancelled(transfer_cancel.as_ref()));
    let sideband_cancel = cancel.clone();
    callbacks.sideband_progress(move |_| !is_git_cancelled(sideband_cancel.as_ref()));
    let push_negotiation_cancel = cancel.clone();
    callbacks.push_negotiation(move |_| {
        check_git_cancelled(push_negotiation_cancel.as_ref())
            .map_err(|error| git2::Error::from_str(&error))
    });
    callbacks.credentials(|url, username_from_url, allowed| {
        if allowed.is_ssh_key() || allowed.is_ssh_memory() {
            let username = username_from_url.unwrap_or("git");
            if let Ok(cred) = git2::Cred::ssh_key_from_agent(username) {
                return Ok(cred);
            }
            for key in default_ssh_key_paths() {
                if key.exists() {
                    if let Ok(cred) = git2::Cred::ssh_key(username, None, &key, None) {
                        return Ok(cred);
                    }
                }
            }
        }
        if allowed.is_user_pass_plaintext() {
            if let Ok(config) = git2::Config::open_default() {
                if let Some((username, password)) = git2::CredentialHelper::new(url)
                    .config(&config)
                    .username(username_from_url)
                    .execute()
                {
                    return git2::Cred::userpass_plaintext(&username, &password);
                }
            }
        }
        if allowed.is_username() {
            return git2::Cred::username(username_from_url.unwrap_or("git"));
        }
        if allowed.is_default() {
            return git2::Cred::default();
        }
        Err(git2::Error::from_str(
            "No compatible Git credential was found.",
        ))
    });
    callbacks
}

fn check_git_cancelled(cancel: Option<&GitCancelToken>) -> Result<(), String> {
    if is_git_cancelled(cancel) {
        Err("Git operation cancelled.".to_string())
    } else {
        Ok(())
    }
}

fn is_git_cancelled(cancel: Option<&GitCancelToken>) -> bool {
    cancel
        .map(|token| token.load(Ordering::Relaxed))
        .unwrap_or(false)
}

fn git_error_message(error: git2::Error) -> String {
    if error.code() == git2::ErrorCode::User {
        "Git operation cancelled.".to_string()
    } else {
        normalize_git_error_message(error.message())
    }
}

fn normalize_git_error_message(message: &str) -> String {
    if message
        .to_lowercase()
        .contains("cannot push because a reference that you are trying to update on the remote contains commits that are not present locally")
    {
        return "Push rejected because the remote branch has commits that are not present locally. Pull or sync first, then push again.".to_string();
    }
    message.to_string()
}

fn default_ssh_key_paths() -> Vec<PathBuf> {
    let Some(home) = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE")) else {
        return Vec::new();
    };
    let ssh = PathBuf::from(home).join(".ssh");
    ["id_ed25519", "id_rsa", "id_ecdsa"]
        .into_iter()
        .map(|name| ssh.join(name))
        .collect()
}

fn normalized_pathspecs(paths: &[String]) -> Vec<String> {
    paths
        .iter()
        .map(|path| path.trim().replace('\\', "/"))
        .filter(|path| !path.is_empty())
        .collect()
}

fn current_branch_name(repo: &GitRepository) -> String {
    repo.head()
        .ok()
        .and_then(|head| {
            if head.is_branch() {
                head.shorthand().ok().map(str::to_string)
            } else {
                head.target().map(|oid| short_oid(oid))
            }
        })
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "HEAD".to_string())
}

fn upstream_branch_name(repo: &GitRepository) -> Option<String> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None;
    }
    let name = head.shorthand().ok()?;
    repo.find_branch(name, git2::BranchType::Local)
        .ok()
        .and_then(|branch| branch.upstream().ok())
        .and_then(|branch| branch.name().ok().flatten().map(str::to_string))
}

fn ahead_behind(repo: &GitRepository) -> Option<(i64, i64)> {
    let head = repo.head().ok()?.target()?;
    let upstream = {
        let head_ref = repo.head().ok()?;
        if !head_ref.is_branch() {
            return Some((0, 0));
        }
        let name = head_ref.shorthand().ok()?;
        repo.find_branch(name, git2::BranchType::Local)
            .ok()?
            .upstream()
            .ok()?
            .get()
            .target()?
    };
    repo.graph_ahead_behind(head, upstream)
        .ok()
        .map(|(ahead, behind)| (ahead as i64, behind as i64))
}

fn git2_status_files(
    repo: &GitRepository,
) -> (Vec<GitFileStatus>, Vec<GitFileStatus>, Vec<GitFileStatus>) {
    let mut options = git2::StatusOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .renames_head_to_index(true)
        .renames_index_to_workdir(true);
    let statuses = match repo.statuses(Some(&mut options)) {
        Ok(statuses) => statuses,
        Err(_) => return (Vec::new(), Vec::new(), Vec::new()),
    };
    let mut staged = Vec::new();
    let mut unstaged = Vec::new();
    let mut untracked = Vec::new();
    for entry in statuses.iter() {
        let status = entry.status();
        let Some(path) = entry.path().ok().map(normalize_git_path) else {
            continue;
        };
        if is_codux_managed_memory_entrypoint(repo, &path) {
            continue;
        }
        let index_status = git2_index_status_code(status);
        let worktree_status = git2_worktree_status_code(status);
        let file = GitFileStatus {
            path,
            index_status: index_status.clone(),
            worktree_status: worktree_status.clone(),
        };
        if status.contains(git2::Status::WT_NEW) && index_status.trim().is_empty() {
            untracked.push(file);
            continue;
        }
        if !index_status.trim().is_empty() {
            staged.push(file.clone());
        }
        if !worktree_status.trim().is_empty() {
            unstaged.push(file);
        }
    }
    (staged, unstaged, untracked)
}

fn remove_codux_managed_memory_entrypoint_from_index(
    repo: &GitRepository,
    index: &mut git2::Index,
) {
    let path = "AGENTS.md";
    if is_codux_managed_memory_entrypoint(repo, path) {
        let _ = index.remove_path(Path::new(path));
    }
}

fn is_codux_managed_memory_entrypoint_path(repo: &GitRepository, path: &Path) -> bool {
    path.to_str()
        .map(normalize_git_path)
        .is_some_and(|path| is_codux_managed_memory_entrypoint(repo, &path))
}

fn is_codux_managed_memory_entrypoint(repo: &GitRepository, path: &str) -> bool {
    if path != "AGENTS.md" {
        return false;
    }
    if head_contains_path(repo, path) {
        return false;
    }
    let full_path = repo_root(repo).join(path);
    is_codux_managed_memory_entrypoint_file(&full_path)
}

fn head_contains_path(repo: &GitRepository, path: &str) -> bool {
    head_tree(repo)
        .ok()
        .and_then(|tree| tree.get_path(Path::new(path)).ok())
        .is_some()
}

fn is_codux_managed_memory_entrypoint_file(path: &Path) -> bool {
    match std::fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => std::fs::read_link(path)
            .ok()
            .and_then(|target| target.to_str().map(str::to_string))
            .is_some_and(|target| {
                let normalized = target.replace('\\', "/");
                normalized.ends_with("/AGENTS.md")
                    && normalized.contains("/runtime-root/memory-workspaces/")
            }),
        Ok(metadata) if metadata.is_file() => std::fs::read_to_string(path)
            .ok()
            .and_then(|text| text.lines().next().map(str::to_string))
            .is_some_and(|line| line.trim() == CODUX_MANAGED_MEMORY_ENTRYPOINT_MARKER),
        _ => false,
    }
}

fn git2_index_status_code(status: git2::Status) -> String {
    if status.contains(git2::Status::INDEX_NEW) {
        "A"
    } else if status.contains(git2::Status::INDEX_MODIFIED) {
        "M"
    } else if status.contains(git2::Status::INDEX_DELETED) {
        "D"
    } else if status.contains(git2::Status::INDEX_RENAMED) {
        "R"
    } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
        "T"
    } else {
        " "
    }
    .to_string()
}

fn git2_worktree_status_code(status: git2::Status) -> String {
    if status.contains(git2::Status::WT_NEW) {
        "?"
    } else if status.contains(git2::Status::WT_MODIFIED) {
        "M"
    } else if status.contains(git2::Status::WT_DELETED) {
        "D"
    } else if status.contains(git2::Status::WT_RENAMED) {
        "R"
    } else if status.contains(git2::Status::WT_TYPECHANGE) {
        "T"
    } else {
        " "
    }
    .to_string()
}

fn git2_commit_log(repo: &GitRepository, limit: usize) -> Vec<GitCommitSummary> {
    let mut revwalk = match repo.revwalk() {
        Ok(revwalk) => revwalk,
        Err(_) => return Vec::new(),
    };
    let _ = revwalk.set_sorting(git2::Sort::TIME);
    if revwalk.push_head().is_err() {
        return Vec::new();
    }
    revwalk
        .take(limit)
        .filter_map(Result::ok)
        .filter_map(|oid| {
            let commit = repo.find_commit(oid).ok()?;
            let author = commit.author().name().unwrap_or("").to_string();
            Some(GitCommitSummary {
                hash: oid.to_string(),
                title: commit.summary().ok().flatten().unwrap_or("").to_string(),
                relative_time: relative_git_time(commit.time().seconds()),
                decorations: commit_decorations(repo, oid),
                graph_prefix: String::new(),
                author,
            })
        })
        .collect()
}

fn commit_decorations(repo: &GitRepository, oid: git2::Oid) -> Option<String> {
    let mut labels = Vec::new();
    if let Ok(head) = repo.head() {
        if head.target() == Some(oid) {
            if let Ok(name) = head.shorthand() {
                labels.push(format!("HEAD -> {name}"));
            } else {
                labels.push("HEAD".to_string());
            }
        }
    }
    if let Ok(mut refs) = repo.references() {
        while let Some(Ok(reference)) = refs.next() {
            if reference.target() != Some(oid) {
                continue;
            }
            let Ok(name) = reference.shorthand() else {
                continue;
            };
            if reference.is_tag() {
                labels.push(format!("tag: {name}"));
            } else if reference.is_branch() && !labels.iter().any(|item| item.contains(name)) {
                labels.push(name.to_string());
            } else if reference.is_remote() {
                labels.push(name.to_string());
            }
        }
    }
    (!labels.is_empty()).then(|| labels.join(", "))
}

fn git2_branches(
    repo: &GitRepository,
    branch_type: git2::BranchType,
    current: &str,
) -> Vec<GitBranchSummary> {
    let mut branches = Vec::new();
    let Ok(iter) = repo.branches(Some(branch_type)) else {
        return branches;
    };
    for item in iter.filter_map(Result::ok) {
        let branch = item.0;
        let name = branch
            .name()
            .ok()
            .flatten()
            .map(str::to_string)
            .unwrap_or_default();
        if name.is_empty() {
            continue;
        }
        let upstream = branch
            .upstream()
            .ok()
            .and_then(|branch| branch.name().ok().flatten().map(str::to_string));
        let hash = branch.get().target().map(short_oid).unwrap_or_default();
        branches.push(GitBranchSummary {
            is_current: branch_type == git2::BranchType::Local && name == current,
            name,
            upstream,
            hash,
        });
    }
    branches.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    if branch_type == git2::BranchType::Local {
        ensure_current_local_branch(branches, current)
    } else {
        branches
    }
}

fn git2_remotes(repo: &GitRepository) -> Vec<GitRemoteSummary> {
    let mut remotes = Vec::new();
    let Ok(names) = repo.remotes() else {
        return remotes;
    };
    for name in names.iter().flatten().flatten() {
        if let Ok(remote) = repo.find_remote(name) {
            remotes.push(GitRemoteSummary {
                name: name.to_string(),
                url: remote.url().unwrap_or("").to_string(),
            });
        }
    }
    remotes.sort_by(|left, right| left.name.to_lowercase().cmp(&right.name.to_lowercase()));
    remotes
}

#[derive(Clone, Copy)]
enum DiffTarget {
    Index,
    Worktree,
}

fn git2_diff_to_string(
    repo: &GitRepository,
    target: DiffTarget,
    path: Option<&str>,
    context_lines: u32,
) -> Result<String, String> {
    let tree = head_tree(repo).ok();
    let mut options = git2_diff_options(path, context_lines);
    let diff = match target {
        DiffTarget::Index => repo.diff_tree_to_index(tree.as_ref(), None, Some(&mut options)),
        DiffTarget::Worktree => repo.diff_index_to_workdir(None, Some(&mut options)),
    }
    .map_err(|error| error.message().to_string())?;
    diff_to_string(&diff)
}

fn git2_commit_diff_to_string(
    repo: &GitRepository,
    base: &str,
    path: Option<&str>,
    context_lines: u32,
) -> Result<String, String> {
    let base_tree = resolve_commit_tree(repo, base)?;
    let head_tree = head_tree(repo)?;
    let mut options = git2_diff_options(path, context_lines);
    let diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), Some(&mut options))
        .map_err(|error| error.message().to_string())?;
    diff_to_string(&diff)
}

fn git2_commit_review_files(
    repo: &GitRepository,
    base: &str,
) -> Result<Vec<GitReviewFile>, String> {
    let base_tree = resolve_commit_tree(repo, base)?;
    let head_tree = head_tree(repo)?;
    let mut diff = repo
        .diff_tree_to_tree(Some(&base_tree), Some(&head_tree), None)
        .map_err(|error| error.message().to_string())?;
    let _ = diff.find_similar(None);
    review_files_from_diff(&diff)
}

fn working_tree_review_stats_git2(repo: &GitRepository) -> HashMap<String, (i64, i64)> {
    let mut stats = HashMap::new();
    if let Ok(diff) = diff_for_review_stats(repo, DiffTarget::Index) {
        merge_review_stats_from_diff(&mut stats, &diff);
    }
    if let Ok(diff) = diff_for_review_stats(repo, DiffTarget::Worktree) {
        merge_review_stats_from_diff(&mut stats, &diff);
    }
    stats
}

fn diff_for_review_stats(
    repo: &GitRepository,
    target: DiffTarget,
) -> Result<git2::Diff<'_>, String> {
    let tree = head_tree(repo).ok();
    let diff = match target {
        DiffTarget::Index => repo.diff_tree_to_index(tree.as_ref(), None, None),
        DiffTarget::Worktree => repo.diff_index_to_workdir(None, None),
    }
    .map_err(|error| error.message().to_string())?;
    Ok(diff)
}

fn merge_review_stats_from_diff(target: &mut HashMap<String, (i64, i64)>, diff: &git2::Diff<'_>) {
    for file in review_files_from_diff(diff).unwrap_or_default() {
        let entry = target.entry(file.path).or_insert((0, 0));
        entry.0 += file.additions;
        entry.1 += file.deletions;
    }
}

fn review_files_from_diff(diff: &git2::Diff<'_>) -> Result<Vec<GitReviewFile>, String> {
    let mut files = Vec::new();
    for index in 0..diff.deltas().len() {
        let Some(delta) = diff.get_delta(index) else {
            continue;
        };
        let Some(path) = delta
            .new_file()
            .path()
            .or_else(|| delta.old_file().path())
            .map(normalize_git_path_path)
        else {
            continue;
        };
        let (additions, deletions) = patch_line_stats(diff, index);
        files.push(GitReviewFile {
            path,
            status: review_status_from_delta(delta.status()),
            additions,
            deletions,
        });
    }
    Ok(files)
}

fn patch_line_stats(diff: &git2::Diff<'_>, index: usize) -> (i64, i64) {
    let Some(delta) = diff.get_delta(index) else {
        return (0, 0);
    };
    let Ok(Some(patch)) = git2::Patch::from_diff(diff, index) else {
        return (0, 0);
    };
    let mut additions = 0;
    let mut deletions = 0;
    for hunk_index in 0..patch.num_hunks() {
        let Ok((_hunk, line_count)) = patch.hunk(hunk_index) else {
            continue;
        };
        for line_index in 0..line_count {
            let Ok(line) = patch.line_in_hunk(hunk_index, line_index) else {
                continue;
            };
            match line.origin() {
                '+' => additions += 1,
                '-' => deletions += 1,
                _ => {}
            }
        }
    }
    if additions == 0 && deletions == 0 {
        match delta.status() {
            git2::Delta::Added => additions = 1,
            git2::Delta::Deleted => deletions = 1,
            _ => {}
        }
    }
    (additions, deletions)
}

fn diff_to_string(diff: &git2::Diff<'_>) -> Result<String, String> {
    let mut output = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        output.extend_from_slice(line.content());
        true
    })
    .map_err(|error| error.message().to_string())?;
    Ok(String::from_utf8_lossy(&output).to_string())
}

fn compact_commit_message_diff(diff: &str) -> (String, bool) {
    let mut output = String::new();
    let mut truncated = false;
    let mut file_count = 0usize;
    let mut file_line_count = 0usize;
    let mut include_current_file = true;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            file_count += 1;
            if file_count > COMMIT_CONTEXT_MAX_FILES {
                truncated = true;
                break;
            }
            file_line_count = 0;
            include_current_file = true;
        }

        let is_header = line.starts_with("diff --git ")
            || line.starts_with("index ")
            || line.starts_with("--- ")
            || line.starts_with("+++ ")
            || line.starts_with("@@ ")
            || line.starts_with("new file mode ")
            || line.starts_with("deleted file mode ")
            || line.starts_with("rename from ")
            || line.starts_with("rename to ")
            || line.starts_with("Binary files ");

        if !is_header {
            file_line_count += 1;
            if file_line_count > COMMIT_CONTEXT_MAX_LINES_PER_FILE {
                if include_current_file {
                    push_commit_context_line(&mut output, "... file diff truncated ...");
                    include_current_file = false;
                    truncated = true;
                }
                continue;
            }
        }

        if output.len() + line.len() + 1 > COMMIT_CONTEXT_MAX_CHARS {
            truncated = true;
            break;
        }
        push_commit_context_line(&mut output, line);
    }

    if truncated {
        push_commit_context_line(&mut output, "... diff truncated for token budget ...");
    }
    (output, truncated)
}

fn push_commit_context_line(output: &mut String, line: &str) {
    if !output.is_empty() {
        output.push('\n');
    }
    output.push_str(line);
}

fn git2_diff_options(path: Option<&str>, context_lines: u32) -> git2::DiffOptions {
    let mut options = git2::DiffOptions::new();
    options
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .context_lines(context_lines);
    if let Some(path) = path.filter(|path| !path.trim().is_empty()) {
        options.pathspec(path);
    }
    options
}

fn head_tree(repo: &GitRepository) -> Result<git2::Tree<'_>, String> {
    let head = repo.head().map_err(|error| error.message().to_string())?;
    let commit = head
        .peel_to_commit()
        .map_err(|error| error.message().to_string())?;
    commit.tree().map_err(|error| error.message().to_string())
}

fn resolve_commit_tree<'repo>(
    repo: &'repo GitRepository,
    reference: &str,
) -> Result<git2::Tree<'repo>, String> {
    let object = repo
        .revparse_single(reference)
        .map_err(|_| format!("Cannot resolve git reference: {reference}"))?;
    let commit = object
        .peel_to_commit()
        .map_err(|error| error.message().to_string())?;
    commit.tree().map_err(|error| error.message().to_string())
}

fn git2_blob_or_empty(repo: &GitRepository, reference: &str, path: &str) -> String {
    git2_blob(repo, reference, path).unwrap_or_default()
}

fn git2_blob(repo: &GitRepository, reference: &str, path: &str) -> Result<String, String> {
    let tree = resolve_commit_tree(repo, reference)?;
    let entry = tree
        .get_path(Path::new(path))
        .map_err(|error| error.message().to_string())?;
    let blob = repo
        .find_blob(entry.id())
        .map_err(|error| error.message().to_string())?;
    Ok(String::from_utf8_lossy(blob.content()).to_string())
}

fn git2_index_blob(repo: &GitRepository, path: &str) -> Result<String, String> {
    let index = repo.index().map_err(|error| error.message().to_string())?;
    let entry = index
        .get_path(Path::new(path), 0)
        .ok_or_else(|| "Index entry not found.".to_string())?;
    let blob = repo
        .find_blob(entry.id)
        .map_err(|error| error.message().to_string())?;
    Ok(String::from_utf8_lossy(blob.content()).to_string())
}

fn is_untracked_path_git2(repo: &GitRepository, path: &str) -> bool {
    let (.., untracked) = git2_status_files(repo);
    untracked.iter().any(|file| file.path == path)
}

fn review_status_from_delta(delta: git2::Delta) -> String {
    match delta {
        git2::Delta::Added => "added",
        git2::Delta::Deleted => "deleted",
        git2::Delta::Renamed => "renamed",
        git2::Delta::Copied => "copied",
        git2::Delta::Typechange => "typeChanged",
        _ => "modified",
    }
    .to_string()
}

fn review_diff_stat(files: &[GitReviewFile]) -> String {
    if files.is_empty() {
        return String::new();
    }
    let additions: i64 = files.iter().map(|file| file.additions).sum();
    let deletions: i64 = files.iter().map(|file| file.deletions).sum();
    format!(
        "{} changed files, {} insertions(+), {} deletions(-)",
        files.len(),
        additions,
        deletions
    )
}

fn short_oid(oid: git2::Oid) -> String {
    oid.to_string().chars().take(7).collect()
}

fn relative_git_time(seconds: i64) -> String {
    let now = chrono::Utc::now().timestamp();
    let elapsed = now.saturating_sub(seconds).max(0);
    if elapsed < 60 {
        "just now".to_string()
    } else if elapsed < 3_600 {
        format!("{} minutes ago", elapsed / 60)
    } else if elapsed < 86_400 {
        format!("{} hours ago", elapsed / 3_600)
    } else if elapsed < 2_592_000 {
        format!("{} days ago", elapsed / 86_400)
    } else if elapsed < 31_536_000 {
        format!("{} months ago", elapsed / 2_592_000)
    } else {
        format!("{} years ago", elapsed / 31_536_000)
    }
}

fn normalize_git_path(value: &str) -> String {
    value.replace('\\', "/")
}

fn normalize_git_path_path(path: &Path) -> String {
    normalize_git_path(&path.to_string_lossy())
}

fn read_worktree_file(root: &Path, path: &str) -> Result<String, String> {
    let full_path = root.join(path);
    let root = root.canonicalize().map_err(|error| error.to_string())?;
    let full_path = full_path
        .canonicalize()
        .map_err(|error| error.to_string())?;
    if !full_path.starts_with(&root) || !full_path.is_file() {
        return Ok(String::new());
    }
    std::fs::read_to_string(full_path).map_err(|error| error.to_string())
}

fn parse_diff_line_numbers(diff: &str) -> (Vec<i64>, Vec<i64>) {
    let mut deleted = Vec::new();
    let mut added = Vec::new();
    for line in diff.lines() {
        let Some(header) = line.strip_prefix("@@ ") else {
            continue;
        };
        let Some(end) = header.find(" @@") else {
            continue;
        };
        let hunk = &header[..end];
        let mut parts = hunk.split_whitespace();
        if let Some(old_range) = parts.next() {
            deleted.extend(diff_range_lines(old_range.trim_start_matches('-')));
        }
        if let Some(new_range) = parts.next() {
            added.extend(diff_range_lines(new_range.trim_start_matches('+')));
        }
    }
    (deleted, added)
}

fn diff_range_lines(range: &str) -> Vec<i64> {
    let mut parts = range.split(',');
    let start = parts
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(0);
    let count = parts
        .next()
        .and_then(|value| value.parse::<i64>().ok())
        .unwrap_or(1);
    if start <= 0 || count <= 0 {
        return Vec::new();
    }
    (start..start + count).collect()
}

fn ensure_current_local_branch(
    mut branches: Vec<GitBranchSummary>,
    current: &str,
) -> Vec<GitBranchSummary> {
    let current = current.trim();
    if current.is_empty() || current == "HEAD" || current == "uninitialized" {
        return branches;
    }
    for branch in &mut branches {
        branch.is_current = branch.name == current;
    }
    branches
}

fn push_review_file_from_status(
    files: &mut Vec<GitReviewFile>,
    seen_paths: &mut HashSet<String>,
    file: &GitFileStatus,
    fallback: &str,
    stats: &HashMap<String, (i64, i64)>,
    root: &Path,
) {
    if !seen_paths.insert(file.path.clone()) {
        return;
    }
    let mut review_file = review_file_from_status(file, fallback, stats);
    if file.index_status == "?" && file.worktree_status == "?" && review_file.additions == 0 {
        review_file.additions = count_untracked_file_lines(root, &file.path).unwrap_or(0);
    }
    files.push(review_file);
}

fn count_untracked_file_lines(root: &Path, path: &str) -> Option<i64> {
    let root = root.canonicalize().ok()?;
    let full_path = root.join(path).canonicalize().ok()?;
    if !full_path.starts_with(&root) || !full_path.is_file() {
        return None;
    }
    let metadata = std::fs::metadata(&full_path).ok()?;
    if metadata.len() > REVIEW_UNTRACKED_LINE_COUNT_LIMIT_BYTES {
        return None;
    }
    let data = std::fs::read(full_path).ok()?;
    if data.contains(&0) {
        return None;
    }
    let text = String::from_utf8_lossy(&data);
    Some(text.lines().count() as i64)
}

fn review_file_from_status(
    file: &GitFileStatus,
    fallback: &str,
    stats: &HashMap<String, (i64, i64)>,
) -> GitReviewFile {
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
    let (additions, deletions) = stats.get(&file.path).copied().unwrap_or((0, 0));
    GitReviewFile {
        path: file.path.clone(),
        status,
        additions,
        deletions,
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

fn repository_root(path: &str) -> Result<String, String> {
    open_git_repository(path).map(|repo| repo_root(&repo).display().to_string())
}

fn current_local_branch_name(path: &str) -> Result<String, String> {
    let repo = open_git_repository(path)?;
    let head = repo.head().map_err(|error| error.message().to_string())?;
    if !head.is_branch() {
        return Ok(String::new());
    }
    head.shorthand()
        .map(str::to_string)
        .map_err(|error| error.message().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    struct TempDir {
        path: PathBuf,
    }

    impl TempDir {
        fn new(name: &str) -> Self {
            let path = std::env::temp_dir()
                .join(format!("codux-git-test-{name}-{}", uuid::Uuid::new_v4()));
            fs::create_dir_all(&path).expect("create temp dir");
            Self { path }
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

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
    fn git_watcher_path_set_keeps_other_worktrees_when_one_is_removed() {
        let mut paths = HashSet::from([
            "/repo/app".to_string(),
            "/repo/app/.codux/worktrees/task-a".to_string(),
        ]);

        let empty = remove_watched_project_path(
            &mut paths,
            &normalized_path_key(Path::new("/repo/app/.codux/worktrees/task-a")),
        );

        assert!(!empty);
        assert_eq!(paths, HashSet::from(["/repo/app".to_string()]));
    }

    #[test]
    fn git_watcher_path_set_reports_empty_after_last_path_is_removed() {
        let mut paths = HashSet::from(["/repo/app".to_string()]);

        let empty =
            remove_watched_project_path(&mut paths, &normalized_path_key(Path::new("/repo/app")));

        assert!(empty);
        assert!(paths.is_empty());
    }

    #[test]
    fn stage_commit_branch_and_discard_use_git2() {
        let temp = TempDir::new("local-ops");
        let root = create_repo(&temp.path);

        fs::write(temp.path.join("README.md"), "hello\n").expect("write file");
        git_stage(GitPathsRequest {
            project_path: root.clone(),
            paths: vec!["README.md".to_string()],
        })
        .expect("stage");
        git_commit(GitCommitRequest {
            project_path: root.clone(),
            message: "initial".to_string(),
        })
        .expect("commit");

        git_create_branch(GitCreateBranchRequest {
            project_path: root.clone(),
            branch: "feature/test".to_string(),
            from: None,
            checkout: true,
        })
        .expect("create branch");
        assert_eq!(current_local_branch_name(&root).unwrap(), "feature/test");

        fs::write(temp.path.join("README.md"), "changed\n").expect("modify");
        git_stage(GitPathsRequest {
            project_path: root.clone(),
            paths: vec!["README.md".to_string()],
        })
        .expect("stage modified");
        git_discard(GitPathsRequest {
            project_path: root.clone(),
            paths: vec!["README.md".to_string()],
        })
        .expect("discard");

        let status = git_status(root);
        assert!(status.staged.is_empty());
        assert!(status.unstaged.is_empty());
    }

    #[test]
    fn commit_rejects_empty_index_like_git_cli() {
        let temp = TempDir::new("empty-commit");
        let root = create_repo(&temp.path);

        let result = git_commit(GitCommitRequest {
            project_path: root.clone(),
            message: "empty".to_string(),
        });
        assert!(result.is_err());

        fs::write(temp.path.join("README.md"), "hello\n").expect("write file");
        git_stage(GitPathsRequest {
            project_path: root.clone(),
            paths: vec![],
        })
        .expect("stage all");
        git_commit(GitCommitRequest {
            project_path: root.clone(),
            message: "initial".to_string(),
        })
        .expect("initial commit");

        let result = git_commit(GitCommitRequest {
            project_path: root,
            message: "empty after head".to_string(),
        });
        assert!(result.is_err());
    }

    #[test]
    fn amend_allows_message_only_changes() {
        let temp = TempDir::new("amend-message");
        let root = create_repo(&temp.path);
        fs::write(temp.path.join("README.md"), "hello\n").expect("write file");
        git_stage(GitPathsRequest {
            project_path: root.clone(),
            paths: vec![],
        })
        .expect("stage all");
        git_commit(GitCommitRequest {
            project_path: root.clone(),
            message: "initial".to_string(),
        })
        .expect("initial commit");

        git_amend_last_commit_message(GitCommitRequest {
            project_path: root.clone(),
            message: "renamed initial".to_string(),
        })
        .expect("amend message");

        assert_eq!(git_last_commit_message(root).unwrap(), "renamed initial");
    }

    #[test]
    fn fetch_push_and_pull_use_git2_with_file_remote() {
        let temp = TempDir::new("remote-ops");
        let bare_path = temp.path.join("origin.git");
        let local_path = temp.path.join("local");
        let peer_path = temp.path.join("peer");
        fs::create_dir_all(&local_path).expect("create local");
        fs::create_dir_all(&peer_path).expect("create peer");
        GitRepository::init_bare(&bare_path).expect("init bare remote");

        let local = create_repo(&local_path);
        let repo = GitRepository::discover(&local).expect("open local");
        repo.remote("origin", &bare_path.to_string_lossy())
            .expect("add remote");
        drop(repo);
        write_and_commit(&local_path, "README.md", "local\n", "initial");
        git_push_remote(GitPushRemoteRequest {
            project_path: local.clone(),
            remote: "origin".to_string(),
        })
        .expect("push");

        clone_repository_git2(&bare_path.to_string_lossy(), &peer_path).expect("clone peer");
        write_and_commit(&peer_path, "peer.txt", "peer\n", "peer change");
        let peer = peer_path.to_string_lossy().to_string();
        git_push(peer).expect("push peer");

        git_pull(local.clone()).expect("pull local");
        assert!(local_path.join("peer.txt").exists());

        let status = git_status(local);
        assert!(status.remotes.iter().any(|remote| remote.name == "origin"));
    }

    #[test]
    fn normalizes_non_fast_forward_push_rejection_message() {
        let message = normalize_git_error_message(
            "cannot push because a reference that you are trying to update on the remote contains commits that are not present locally.",
        );
        assert_eq!(
            message,
            "Push rejected because the remote branch has commits that are not present locally. Pull or sync first, then push again."
        );
    }

    #[test]
    fn hides_and_skips_untracked_codux_managed_agents_entrypoint() {
        let temp = TempDir::new("codux-agents-entrypoint");
        let root = create_repo(&temp.path);
        write_and_commit(&temp.path, "README.md", "hello\n", "initial");

        let memory_root = temp
            .path
            .join("runtime-root")
            .join("memory-workspaces")
            .join("p");
        fs::create_dir_all(&memory_root).expect("memory root");
        let memory_agents = memory_root.join("AGENTS.md");
        fs::write(&memory_agents, "memory\n").expect("memory agents");
        let project_agents = temp.path.join("AGENTS.md");
        #[cfg(unix)]
        std::os::unix::fs::symlink(&memory_agents, &project_agents).expect("agents symlink");
        #[cfg(not(unix))]
        fs::write(
            &project_agents,
            format!("{CODUX_MANAGED_MEMORY_ENTRYPOINT_MARKER}\r\nmemory\n"),
        )
        .expect("agents marker copy");

        let status = git_status(root.clone());
        assert!(status.untracked.iter().all(|file| file.path != "AGENTS.md"));

        git_stage(GitPathsRequest {
            project_path: root.clone(),
            paths: vec![],
        })
        .expect("stage all");
        let repo = GitRepository::discover(root).expect("open repo");
        let index = repo.index().expect("index");
        assert!(index.get_path(Path::new("AGENTS.md"), 0).is_none());
    }

    #[test]
    fn pull_rebases_local_commits_with_git2() {
        let temp = TempDir::new("pull-rebase");
        let bare_path = temp.path.join("origin.git");
        let local_path = temp.path.join("local");
        let peer_path = temp.path.join("peer");
        fs::create_dir_all(&local_path).expect("create local");
        fs::create_dir_all(&peer_path).expect("create peer");
        GitRepository::init_bare(&bare_path).expect("init bare remote");

        let local = create_repo(&local_path);
        let repo = GitRepository::discover(&local).expect("open local");
        repo.remote("origin", &bare_path.to_string_lossy())
            .expect("add remote");
        drop(repo);
        write_and_commit(&local_path, "README.md", "initial\n", "initial");
        git_push_remote(GitPushRemoteRequest {
            project_path: local.clone(),
            remote: "origin".to_string(),
        })
        .expect("push initial");

        clone_repository_git2(&bare_path.to_string_lossy(), &peer_path).expect("clone peer");
        write_and_commit(&peer_path, "peer.txt", "peer\n", "peer change");
        git_push(peer_path.to_string_lossy().to_string()).expect("push peer");

        write_and_commit(&local_path, "local.txt", "local\n", "local change");
        git_pull(local.clone()).expect("pull with rebase");

        assert!(local_path.join("peer.txt").exists());
        assert!(local_path.join("local.txt").exists());
        let repo = GitRepository::discover(local_path).expect("open local");
        let branch = repo
            .find_branch("master", git2::BranchType::Local)
            .or_else(|_| repo.find_branch("main", git2::BranchType::Local))
            .expect("current branch");
        let upstream = branch.upstream().expect("upstream");
        let local_oid = branch.get().target().expect("local oid");
        let upstream_oid = upstream.get().target().expect("upstream oid");
        let (ahead, behind) = repo
            .graph_ahead_behind(local_oid, upstream_oid)
            .expect("ahead behind");
        assert_eq!((ahead, behind), (1, 0));
    }

    fn create_repo(path: &Path) -> String {
        let repo = GitRepository::init(path).expect("init repo");
        let mut config = repo.config().expect("config");
        config.set_str("user.name", "Codux").expect("user name");
        config
            .set_str("user.email", "codux@example.test")
            .expect("user email");
        path.to_string_lossy().to_string()
    }

    fn write_and_commit(repo_path: &Path, relative_path: &str, content: &str, message: &str) {
        let repo = GitRepository::discover(repo_path).expect("open repo");
        fs::write(repo_path.join(relative_path), content).expect("write file");
        let mut index = repo.index().expect("index");
        index.add_path(Path::new(relative_path)).expect("add path");
        index.write().expect("write index");
        let tree_id = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_id).expect("tree");
        let signature = repo_signature(&repo).expect("signature");
        let parents = repo
            .head()
            .ok()
            .and_then(|head| head.peel_to_commit().ok())
            .into_iter()
            .collect::<Vec<_>>();
        let parent_refs = parents.iter().collect::<Vec<_>>();
        repo.commit(
            Some("HEAD"),
            &signature,
            &signature,
            message,
            &tree,
            &parent_refs,
        )
        .expect("commit");
    }
}
