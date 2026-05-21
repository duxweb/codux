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
    git_refreshes: Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    git_reviews: Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    worktree_refreshes: Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    git_work_lane: Arc<Mutex<()>>,
}

#[derive(Debug, Clone, Default)]
struct CoalescedRefreshState {
    running: bool,
    pending: bool,
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
        refresh_git_project_with_lane(
            app,
            Arc::clone(&self.git_refreshes),
            Arc::clone(&self.git_work_lane),
            tracked_project,
        );
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
        let git_refreshes = Arc::clone(&self.git_refreshes);
        let worktree_refreshes = Arc::clone(&self.worktree_refreshes);
        let git_work_lane = Arc::clone(&self.git_work_lane);
        thread::spawn(move || {
            let _ = app.emit(
                "git:changed",
                GitProjectChangedEvent {
                    project_path: project_path.clone(),
                    repository_path,
                    changed_paths,
                },
            );
            refresh_worktree_project(
                app.clone(),
                Arc::clone(&project_store),
                Arc::clone(&worktree_refreshes),
                Arc::clone(&git_work_lane),
                &project,
            );
            refresh_git_project_with_lane(
                app,
                git_refreshes,
                git_work_lane,
                TrackedProject::from(project),
            );
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
        let git_reviews = Arc::clone(&self.git_reviews);
        let worktree_refreshes = Arc::clone(&self.worktree_refreshes);
        let git_work_lane = Arc::clone(&self.git_work_lane);
        thread::spawn(move || {
            refresh_worktree_project(
                app.clone(),
                Arc::clone(&project_store),
                worktree_refreshes,
                Arc::clone(&git_work_lane),
                &project,
            );
            refresh_git_review_project(
                app,
                git_reviews,
                git_work_lane,
                TrackedProject::from(project),
            );
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
                refresh_worktree_project_without_coalescing(
                    app.clone(),
                    Arc::clone(&project_store),
                    &project,
                );
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
            refresh_worktree_project(
                app.clone(),
                Arc::clone(&project_store),
                Arc::clone(&self.worktree_refreshes),
                Arc::clone(&self.git_work_lane),
                &project,
            );
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
            refresh_git_project_with_lane(
                app.clone(),
                Arc::clone(&coordinator.git_refreshes),
                Arc::clone(&coordinator.git_work_lane),
                project,
            );
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

fn refresh_git_project_with_lane(
    app: AppHandle,
    states: Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    lane: Arc<Mutex<()>>,
    project: TrackedProject,
) {
    let refresh_key = coalesced_refresh_key(&project.path);
    if !claim_coalesced_refresh(&states, &refresh_key) {
        return;
    }
    thread::spawn(move || loop {
        with_git_lane(&lane, || {
            let project_id = project.id.clone();
            let project_name = project.name.clone();
            let project_path = project.path.clone();
            let snapshot = git_status(project_path.clone());
            if snapshot.is_repository
                || snapshot.error.is_none()
                || Path::new(&project_path).exists()
            {
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
            emit_git_review(app.clone(), project_id, project_name, project_path);
        });
        if finish_coalesced_refresh(&states, &refresh_key) {
            continue;
        }
        break;
    });
}

fn refresh_git_review_project(
    app: AppHandle,
    states: Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    lane: Arc<Mutex<()>>,
    project: TrackedProject,
) {
    let refresh_key = coalesced_refresh_key(&project.path);
    if !claim_coalesced_refresh(&states, &refresh_key) {
        return;
    }
    thread::spawn(move || loop {
        with_git_lane(&lane, || {
            emit_git_review(
                app.clone(),
                project.id.clone(),
                project.name.clone(),
                project.path.clone(),
            );
        });
        if finish_coalesced_refresh(&states, &refresh_key) {
            continue;
        }
        break;
    });
}

fn claim_coalesced_refresh(
    states: &Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    project_id: &str,
) -> bool {
    let Ok(mut guard) = states.lock() else {
        return true;
    };
    let state = guard.entry(project_id.to_string()).or_default();
    if state.running {
        state.pending = true;
        return false;
    }
    state.running = true;
    state.pending = false;
    true
}

fn finish_coalesced_refresh(
    states: &Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    project_id: &str,
) -> bool {
    let Ok(mut guard) = states.lock() else {
        return false;
    };
    let Some(state) = guard.get_mut(project_id) else {
        return false;
    };
    if state.pending {
        state.pending = false;
        return true;
    }
    guard.remove(project_id);
    false
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
    states: Arc<Mutex<HashMap<String, CoalescedRefreshState>>>,
    lane: Arc<Mutex<()>>,
    project: &ProjectSummary,
) {
    let refresh_key = coalesced_refresh_key(&project.path);
    if !claim_coalesced_refresh(&states, &refresh_key) {
        return;
    }
    let project = project.clone();
    thread::spawn(move || loop {
        with_git_lane(&lane, || {
            refresh_worktree_project_now(app.clone(), Arc::clone(&project_store), &project);
        });
        if finish_coalesced_refresh(&states, &refresh_key) {
            continue;
        }
        break;
    });
}

fn refresh_worktree_project_without_coalescing(
    app: AppHandle,
    project_store: Arc<ProjectStore>,
    project: &ProjectSummary,
) {
    refresh_worktree_project_now(app, project_store, project);
}

fn refresh_worktree_project_now(
    app: AppHandle,
    project_store: Arc<ProjectStore>,
    project: &ProjectSummary,
) {
    let root_project = project_store
        .root_project_summary_for_workspace_id(&project.id)
        .unwrap_or_else(|| project.clone());
    let project_id = root_project.id.clone();
    let project_path = root_project.path.clone();
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
}

fn configured_interval_seconds(value: &str, minimum: u64) -> Option<Duration> {
    let seconds = value.trim().parse::<u64>().ok()?;
    (seconds > 0).then(|| Duration::from_secs(seconds.max(minimum)))
}

fn coalesced_refresh_key(path: &str) -> String {
    let path = Path::new(path.trim());
    if path.as_os_str().is_empty() {
        return String::new();
    }
    let normalized = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut key = normalized.to_string_lossy().replace('\\', "/");
    while key.len() > 1 && key.ends_with('/') {
        key.pop();
    }
    #[cfg(windows)]
    {
        key = key.to_ascii_lowercase();
    }
    key
}

fn with_git_lane<R>(lane: &Arc<Mutex<()>>, work: impl FnOnce() -> R) -> R {
    let Ok(_guard) = lane.lock() else {
        return work();
    };
    work()
}
