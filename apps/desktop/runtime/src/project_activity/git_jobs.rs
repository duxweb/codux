use super::types::{ProjectActivityEvent, TrackedProject};
use crate::background_queue::{SerialJob, SerialJobQueue};
use crate::git::GitService;
use crate::project_store::ProjectSummary;
use crate::runtime_trace::{runtime_trace, runtime_trace_elapsed};
use crate::worktree::WorktreeService;
use std::collections::VecDeque;
use std::path::Path;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

#[derive(Clone)]
pub(super) struct GitJobQueue {
    queue: SerialJobQueue<GitJob>,
}

impl GitJobQueue {
    pub(super) fn new(
        support_dir: PathBuf,
        events: Arc<Mutex<VecDeque<ProjectActivityEvent>>>,
    ) -> Self {
        Self {
            queue: SerialJobQueue::new("codux-git-job-worker", move |job| {
                run_git_job(job, &support_dir, Arc::clone(&events))
            }),
        }
    }

    pub(super) fn submit(&self, job: GitJob) {
        self.queue.submit(job);
    }
}

pub(super) enum GitJob {
    Refresh {
        project: TrackedProject,
        fetch_remote: bool,
    },
    Worktree {
        support_dir: std::path::PathBuf,
        project: ProjectSummary,
    },
}

impl SerialJob for GitJob {
    fn queue_key(&self) -> String {
        match self {
            Self::Refresh {
                project,
                fetch_remote,
            } => git_job_key(
                if *fetch_remote {
                    "refresh-remote"
                } else {
                    "refresh-local"
                },
                &project.path,
            ),
            Self::Worktree { project, .. } => git_job_key("worktree", &project.path),
        }
    }
}

fn run_git_job(
    job: GitJob,
    support_dir: &Path,
    events: Arc<Mutex<VecDeque<ProjectActivityEvent>>>,
) {
    match job {
        GitJob::Refresh {
            project,
            fetch_remote,
        } => run_git_refresh_job(&events, support_dir, project, fetch_remote),
        GitJob::Worktree {
            support_dir,
            project,
        } => refresh_worktree_project_now(&events, support_dir, &project),
    }
}

fn run_git_refresh_job(
    events: &Arc<Mutex<VecDeque<ProjectActivityEvent>>>,
    support_dir: &Path,
    project: TrackedProject,
    fetch_remote: bool,
) {
    let started_at = Instant::now();
    let project_id = project.id.clone();
    let project_name = project.name.clone();
    let project_path = project.path.clone();
    let mut workspace = GitService::workspace_snapshot(&project_path);
    let mut remote_refresh = if fetch_remote { "skipped" } else { "disabled" };
    if fetch_remote && workspace.status.is_repository && !workspace.status.remotes.is_empty() {
        match GitService::fetch(&project_path) {
            Ok(()) => {
                workspace = GitService::workspace_snapshot(&project_path);
                remote_refresh = "ok";
            }
            Err(error) => {
                remote_refresh = "failed";
                runtime_trace(
                    "git",
                    &format!(
                        "refresh_fetch failed project={} path={} error={}",
                        project_id, project_path, error
                    ),
                );
            }
        }
    }
    let is_repository = workspace.status.is_repository;
    let ahead = workspace.status.ahead;
    let behind = workspace.status.behind;
    let staged_count = workspace.status.staged;
    let unstaged_count = workspace.status.unstaged;
    let untracked_count = workspace.status.untracked;
    if workspace.status.is_repository
        || workspace.status.error.is_none()
        || Path::new(&project_path).exists()
    {
        crate::runtime_cache::save_git_workspace(support_dir, &project_path, &workspace);
        push_event(
            events,
            ProjectActivityEvent::GitStatus {
                project_id: project_id.clone(),
                project_name: project_name.clone(),
                project_path: project_path.clone(),
                snapshot: workspace.status,
                review: workspace.review,
            },
        );
    }
    runtime_trace_elapsed(
        "git",
        "refresh_status",
        started_at,
        &format!(
            "project={} path={} repo={} remote_refresh={} ahead={} behind={} staged={} unstaged={} untracked={}",
            project_id,
            project_path,
            is_repository,
            remote_refresh,
            ahead,
            behind,
            staged_count,
            unstaged_count,
            untracked_count
        ),
    );
}

fn refresh_worktree_project_now(
    events: &Arc<Mutex<VecDeque<ProjectActivityEvent>>>,
    support_dir: std::path::PathBuf,
    project: &ProjectSummary,
) {
    let started_at = Instant::now();
    let snapshot = match WorktreeService::new(support_dir).sync_from_git(&project.id, &project.path)
    {
        Ok(snapshot) => snapshot,
        Err(error) => {
            runtime_trace(
                "project-activity",
                &format!("worktree refresh snapshot failed: {error}"),
            );
            return;
        }
    };
    push_event(
        events,
        ProjectActivityEvent::WorktreeSnapshot {
            project_id: project.id.clone(),
            project_path: project.path.clone(),
            snapshot,
        },
    );
    runtime_trace_elapsed(
        "worktree",
        "refresh_snapshot",
        started_at,
        &format!("project={} path={}", project.id, project.path),
    );
}

fn push_event(events: &Arc<Mutex<VecDeque<ProjectActivityEvent>>>, event: ProjectActivityEvent) {
    if let Ok(mut events) = events.lock() {
        events.push_back(event);
        while events.len() > 128 {
            events.pop_front();
        }
    }
}

fn git_job_key(kind: &str, path: &str) -> String {
    format!("{kind}:{}", crate::git::repository_path_key(path))
}
