use crate::ai_history::AIHistoryProjectRequest;
use crate::ai_history_indexer::AIHistoryIndexer;
use crate::app_settings::AppSettingsStore;
use crate::git::{git_review, git_status, GitReviewSnapshot, GitStatusSnapshot};
use crate::project_store::{ProjectRecord, ProjectStore, ProjectSummary};
use crate::worktree::{worktree_snapshot, WorktreeSnapshot};
use serde::Serialize;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};
use tauri::async_runtime;
use tauri::{AppHandle, Emitter};

const TICK_SECONDS: u64 = 30;
const MIN_GIT_REFRESH_SECONDS: u64 = 15;
const MIN_AI_REFRESH_SECONDS: u64 = 120;

#[derive(Debug, Clone)]
struct TrackedProject {
    id: String,
    name: String,
    path: String,
    last_git_refresh: Option<Instant>,
    last_ai_refresh: Option<Instant>,
}

#[derive(Debug, Clone)]
struct ActivationRequest {
    project: ProjectSummary,
    refresh_ai_immediately: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusEvent {
    pub project_id: String,
    pub project_name: String,
    pub project_path: String,
    pub snapshot: GitStatusSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitReviewEvent {
    project_id: String,
    project_name: String,
    project_path: String,
    base_branch: Option<String>,
    snapshot: GitReviewSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeSnapshotEvent {
    pub project_id: String,
    pub project_path: String,
    pub snapshot: WorktreeSnapshot,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitProjectChangedEvent {
    project_path: String,
    repository_path: String,
    changed_paths: Vec<String>,
}

#[derive(Default)]
pub struct ProjectActivityCoordinator {
    projects: Mutex<HashMap<String, TrackedProject>>,
    active_project_id: Mutex<Option<String>>,
    main_window_visible: AtomicBool,
    main_window_focused: AtomicBool,
    activated_git_projects: Mutex<HashSet<String>>,
    activated_ai_projects: Mutex<HashSet<String>>,
    last_global_ai_refresh: Mutex<Option<Instant>>,
    activation_queue: Mutex<VecDeque<ActivationRequest>>,
    activation_signal: Condvar,
}

impl ProjectActivityCoordinator {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn seed_projects(&self, projects: Vec<ProjectRecord>) {
        if let Ok(mut guard) = self.projects.lock() {
            for project in projects {
                upsert_project(&mut guard, project.id, project.name, project.path);
            }
        }
    }

    pub fn mark_project_summary(&self, project: &ProjectSummary) -> bool {
        if let Ok(mut guard) = self.projects.lock() {
            return upsert_project(
                &mut guard,
                project.id.clone(),
                project.name.clone(),
                project.path.clone(),
            );
        }
        false
    }

    pub fn mark_project_active(&self, project: ProjectSummary) {
        self.mark_project_summary(&project);
        if let Ok(mut active) = self.active_project_id.lock() {
            *active = Some(project.id.clone());
        }
        if let Ok(mut queue) = self.activation_queue.lock() {
            queue.retain(|request| request.project.id != project.id);
            let refresh_ai_immediately = self.mark_ai_activation(&project.id);
            queue.push_back(ActivationRequest {
                project,
                refresh_ai_immediately,
            });
            self.activation_signal.notify_one();
        }
    }

    pub fn mark_main_window_visible(&self, visible: bool) {
        self.main_window_visible.store(visible, Ordering::Relaxed);
    }

    pub fn mark_main_window_focused(&self, focused: bool) {
        self.main_window_focused.store(focused, Ordering::Relaxed);
    }

    pub fn refresh_project_now(
        &self,
        app: AppHandle,
        project: ProjectSummary,
        ai_history: Arc<AIHistoryIndexer>,
    ) {
        self.mark_project_summary(&project);
        self.refresh_git_once(app, &project);
        if self.mark_ai_activation(&project.id) {
            self.refresh_ai_once(project, ai_history);
        }
    }

    pub fn refresh_git_once(&self, app: AppHandle, project: &ProjectSummary) {
        self.mark_project_summary(project);
        let mut tracked_project = TrackedProject::from(project.clone());
        if let Ok(mut guard) = self.projects.lock() {
            if let Some(tracked) = guard.get_mut(&project.id) {
                tracked.last_git_refresh = Some(Instant::now());
                tracked_project = tracked.clone();
            }
        }
        refresh_git_project(app, tracked_project);
    }

    pub fn refresh_git_changed(
        &self,
        app: AppHandle,
        project_store: Arc<ProjectStore>,
        project_path: String,
        repository_path: String,
        changed_paths: Vec<String>,
    ) {
        let Some(project) = project_store.workspace_summary_by_path(&project_path) else {
            return;
        };
        self.mark_project_summary(&project);
        if let Ok(mut guard) = self.projects.lock() {
            if let Some(tracked) = guard.get_mut(&project.id) {
                tracked.last_git_refresh = Some(Instant::now());
            }
        }
        thread::spawn(move || {
            let _ = app.emit(
                "git:changed",
                GitProjectChangedEvent {
                    project_path: project_path.clone(),
                    repository_path,
                    changed_paths,
                },
            );
            refresh_worktree_project(app.clone(), Arc::clone(&project_store), &project);
            refresh_git_project(app, TrackedProject::from(project));
        });
    }

    pub fn refresh_git_sidecars_by_path(
        &self,
        app: AppHandle,
        project_store: Arc<ProjectStore>,
        project_path: String,
    ) {
        let Some(project) = project_store.workspace_summary_by_path(&project_path) else {
            return;
        };
        self.mark_project_summary(&project);
        thread::spawn(move || {
            refresh_worktree_project(app.clone(), Arc::clone(&project_store), &project);
            refresh_git_review_project(app, TrackedProject::from(project));
        });
    }

    pub fn prewarm_worktrees(
        &self,
        app: AppHandle,
        project_store: Arc<ProjectStore>,
        projects: Vec<ProjectSummary>,
    ) {
        thread::spawn(move || {
            for project in projects {
                refresh_worktree_project(app.clone(), Arc::clone(&project_store), &project);
            }
        });
    }

    pub fn remove_project(&self, project_id: &str) {
        if let Ok(mut guard) = self.projects.lock() {
            guard.remove(project_id);
        }
        if let Ok(mut activated) = self.activated_git_projects.lock() {
            activated.remove(project_id);
        }
        if let Ok(mut activated) = self.activated_ai_projects.lock() {
            activated.remove(project_id);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.projects.lock() {
            guard.clear();
        }
        if let Ok(mut active) = self.active_project_id.lock() {
            *active = None;
        }
        if let Ok(mut activated) = self.activated_git_projects.lock() {
            activated.clear();
        }
        if let Ok(mut activated) = self.activated_ai_projects.lock() {
            activated.clear();
        }
        if let Ok(mut last) = self.last_global_ai_refresh.lock() {
            *last = None;
        }
    }

    pub fn start(
        self: Arc<Self>,
        app: AppHandle,
        settings: Arc<AppSettingsStore>,
        ai_history: Arc<AIHistoryIndexer>,
        project_store: Arc<ProjectStore>,
    ) {
        let activation_coordinator = Arc::clone(&self);
        let activation_app = app.clone();
        let activation_ai_history = Arc::clone(&ai_history);
        thread::spawn(move || {
            activation_coordinator.run_activation_queue(
                activation_app,
                activation_ai_history,
                project_store,
            );
        });

        thread::spawn(move || loop {
            run_activity_tick(
                &self,
                &app,
                &settings,
                &ai_history,
                MIN_GIT_REFRESH_SECONDS,
                MIN_AI_REFRESH_SECONDS,
            );

            thread::sleep(Duration::from_secs(TICK_SECONDS));
        });
    }

    fn run_activation_queue(
        &self,
        app: AppHandle,
        ai_history: Arc<AIHistoryIndexer>,
        project_store: Arc<ProjectStore>,
    ) {
        loop {
            let request = {
                let Ok(queue) = self.activation_queue.lock() else {
                    thread::sleep(Duration::from_millis(200));
                    continue;
                };
                let mut queue = self
                    .activation_signal
                    .wait_while(queue, |queue| queue.is_empty())
                    .unwrap_or_else(|error| error.into_inner());
                queue.pop_front()
            };
            let Some(request) = request else {
                continue;
            };
            let project = request.project;
            let is_first_git_activation = self.mark_git_activation(&project.id);
            refresh_worktree_project(app.clone(), Arc::clone(&project_store), &project);
            if is_first_git_activation {
                self.refresh_git_once(app.clone(), &project);
            }
            if request.refresh_ai_immediately {
                self.refresh_ai_once(project, Arc::clone(&ai_history));
            }
        }
    }

    pub fn refresh_ai_once(&self, project: ProjectSummary, ai_history: Arc<AIHistoryIndexer>) {
        self.mark_project_summary(&project);
        let _ = self.mark_ai_activation(&project.id);
        if let Ok(mut guard) = self.projects.lock() {
            if let Some(tracked) = guard.get_mut(&project.id) {
                tracked.last_ai_refresh = Some(Instant::now());
            }
        }
        if let Ok(mut last) = self.last_global_ai_refresh.lock() {
            *last = Some(Instant::now());
        }
        async_runtime::spawn(async move {
            let _ = ai_history.refresh_project(project.into()).await;
        });
    }

    fn mark_ai_activation(&self, project_id: &str) -> bool {
        self.activated_ai_projects
            .lock()
            .map(|mut activated| activated.insert(project_id.to_string()))
            .unwrap_or(false)
    }

    fn mark_git_activation(&self, project_id: &str) -> bool {
        self.activated_git_projects
            .lock()
            .map(|mut activated| activated.insert(project_id.to_string()))
            .unwrap_or(false)
    }

    fn projects_due_for_git(
        &self,
        foreground_interval: Duration,
        background_interval: Duration,
    ) -> Vec<TrackedProject> {
        let active_project_id = self
            .active_project_id
            .lock()
            .ok()
            .and_then(|value| value.clone());
        let is_foreground = self.main_window_visible.load(Ordering::Relaxed)
            || self.main_window_focused.load(Ordering::Relaxed);
        projects_due_by_interval(&self.projects, |project| {
            if is_foreground && active_project_id.as_deref() == Some(project.id.as_str()) {
                foreground_interval
            } else {
                background_interval
            }
        })
    }

    fn projects_due_for_ai(&self, interval: Duration) -> Vec<TrackedProject> {
        projects_due(&self.projects, interval, |project| {
            &mut project.last_ai_refresh
        })
    }

    fn tracked_projects(&self) -> Vec<TrackedProject> {
        self.projects
            .lock()
            .map(|projects| projects.values().cloned().collect())
            .unwrap_or_default()
    }

    fn global_ai_due(&self, interval: Duration) -> bool {
        let now = Instant::now();
        let Ok(mut last) = self.last_global_ai_refresh.lock() else {
            return false;
        };
        let Some(previous) = *last else {
            *last = Some(now);
            return false;
        };
        let is_due = now.duration_since(previous) >= interval;
        if is_due {
            *last = Some(now);
        }
        is_due
    }
}

fn run_activity_tick(
    coordinator: &ProjectActivityCoordinator,
    app: &AppHandle,
    settings: &AppSettingsStore,
    ai_history: &Arc<AIHistoryIndexer>,
    min_git_refresh_seconds: u64,
    min_ai_refresh_seconds: u64,
) {
    let configured = settings.snapshot();
    let git_interval =
        configured_interval_seconds(&configured.git_refresh, min_git_refresh_seconds);
    let ai_interval =
        configured_interval_seconds(&configured.ai_background_refresh, min_ai_refresh_seconds);

    if let Some(interval) = git_interval {
        let background_interval = interval
            .checked_mul(4)
            .unwrap_or_else(|| Duration::from_secs(min_git_refresh_seconds * 4))
            .max(Duration::from_secs(min_git_refresh_seconds * 4));
        for project in coordinator.projects_due_for_git(interval, background_interval) {
            refresh_git_project(app.clone(), project);
        }
    }

    if let Some(interval) = ai_interval {
        for project in coordinator.projects_due_for_ai(interval) {
            let ai_history = Arc::clone(ai_history);
            async_runtime::spawn(async move {
                let _ = ai_history.refresh_project(project.into()).await;
            });
        }
        if coordinator.global_ai_due(interval) {
            let projects = coordinator
                .tracked_projects()
                .into_iter()
                .map(AIHistoryProjectRequest::from)
                .collect::<Vec<_>>();
            if !projects.is_empty() {
                let ai_history = Arc::clone(ai_history);
                async_runtime::spawn(async move {
                    let _ = ai_history.refresh_global(projects).await;
                });
            }
        }
    }
}

impl From<ProjectSummary> for AIHistoryProjectRequest {
    fn from(project: ProjectSummary) -> Self {
        Self {
            id: project.id,
            name: project.name,
            path: project.path,
        }
    }
}

impl From<ProjectSummary> for TrackedProject {
    fn from(project: ProjectSummary) -> Self {
        Self {
            id: project.id,
            name: project.name,
            path: project.path,
            last_git_refresh: None,
            last_ai_refresh: Some(Instant::now()),
        }
    }
}

impl From<TrackedProject> for AIHistoryProjectRequest {
    fn from(project: TrackedProject) -> Self {
        Self {
            id: project.id,
            name: project.name,
            path: project.path,
        }
    }
}

fn upsert_project(
    projects: &mut HashMap<String, TrackedProject>,
    id: String,
    name: String,
    path: String,
) -> bool {
    if id.trim().is_empty() || path.trim().is_empty() {
        return false;
    }
    let mut inserted = false;
    projects
        .entry(id.clone())
        .and_modify(|project| {
            project.name = name.clone();
            project.path = path.clone();
        })
        .or_insert_with(|| {
            inserted = true;
            TrackedProject {
                id,
                name,
                path,
                last_git_refresh: None,
                last_ai_refresh: Some(Instant::now()),
            }
        });
    inserted
}

fn projects_due(
    projects: &Mutex<HashMap<String, TrackedProject>>,
    interval: Duration,
    last_refresh: impl Fn(&mut TrackedProject) -> &mut Option<Instant>,
) -> Vec<TrackedProject> {
    let now = Instant::now();
    let Ok(mut guard) = projects.lock() else {
        return Vec::new();
    };
    guard
        .values_mut()
        .filter_map(|project| {
            let last = last_refresh(project);
            let is_due = last
                .map(|value| now.duration_since(value) >= interval)
                .unwrap_or(true);
            if !is_due {
                return None;
            }
            *last = Some(now);
            Some(project.clone())
        })
        .collect()
}

fn projects_due_by_interval(
    projects: &Mutex<HashMap<String, TrackedProject>>,
    interval_for_project: impl Fn(&TrackedProject) -> Duration,
) -> Vec<TrackedProject> {
    let now = Instant::now();
    let Ok(mut guard) = projects.lock() else {
        return Vec::new();
    };
    guard
        .values_mut()
        .filter_map(|project| {
            let interval = interval_for_project(project);
            let is_due = project
                .last_git_refresh
                .map(|value| now.duration_since(value) >= interval)
                .unwrap_or(true);
            if !is_due {
                return None;
            }
            project.last_git_refresh = Some(now);
            Some(project.clone())
        })
        .collect()
}

fn refresh_git_project(app: AppHandle, project: TrackedProject) {
    thread::spawn(move || {
        let project_id = project.id.clone();
        let project_name = project.name.clone();
        let project_path = project.path.clone();
        let snapshot = git_status(project_path.clone());
        if snapshot.is_repository || snapshot.error.is_none() || Path::new(&project_path).exists() {
            let _ = app.emit(
                "git:status",
                GitStatusEvent {
                    project_id: project_id.clone(),
                    project_name: project_name.clone(),
                    project_path: project_path.clone(),
                    snapshot,
                },
            );
        }
        emit_git_review(app, project_id, project_name, project_path);
    });
}

fn refresh_git_review_project(app: AppHandle, project: TrackedProject) {
    thread::spawn(move || {
        emit_git_review(app, project.id, project.name, project.path);
    });
}

fn emit_git_review(app: AppHandle, project_id: String, project_name: String, project_path: String) {
    let review = git_review(project_path.clone(), None);
    if review.is_repository || review.error.is_none() || Path::new(&project_path).exists() {
        let _ = app.emit(
            "git:review",
            GitReviewEvent {
                project_id,
                project_name,
                project_path,
                base_branch: None,
                snapshot: review,
            },
        );
    }
}

fn refresh_worktree_project(
    app: AppHandle,
    project_store: Arc<ProjectStore>,
    project: &ProjectSummary,
) {
    let root_project = project_store
        .root_project_summary_for_workspace_id(&project.id)
        .unwrap_or_else(|| project.clone());
    let project_id = root_project.id.clone();
    let project_path = root_project.path.clone();
    thread::spawn(move || {
        let snapshot = match project_store
            .merge_worktree_snapshot(worktree_snapshot(project_id.clone(), project_path.clone()))
        {
            Ok(snapshot) => snapshot,
            Err(error) => {
                eprintln!("failed to refresh worktree snapshot: {error}");
                return;
            }
        };
        let _ = app.emit(
            "worktree:snapshot",
            WorktreeSnapshotEvent {
                project_id,
                project_path,
                snapshot,
            },
        );
    });
}

fn configured_interval_seconds(value: &str, minimum: u64) -> Option<Duration> {
    let seconds = value.trim().parse::<u64>().ok()?;
    (seconds > 0).then(|| Duration::from_secs(seconds.max(minimum)))
}
