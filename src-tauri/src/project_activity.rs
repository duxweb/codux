use crate::ai_history::AIHistoryProjectRequest;
use crate::ai_history_indexer::AIHistoryIndexer;
use crate::app_settings::AppSettingsStore;
use crate::git::{git_status, GitStatusSnapshot};
use crate::project_store::{ProjectRecord, ProjectSummary};
use serde::Serialize;
use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};
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

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct GitStatusEvent {
    project_id: String,
    project_name: String,
    project_path: String,
    snapshot: GitStatusSnapshot,
}

#[derive(Default)]
pub struct ProjectActivityCoordinator {
    projects: Mutex<HashMap<String, TrackedProject>>,
    last_global_ai_refresh: Mutex<Option<Instant>>,
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

    pub fn mark_project_summary(&self, project: &ProjectSummary) {
        if let Ok(mut guard) = self.projects.lock() {
            upsert_project(
                &mut guard,
                project.id.clone(),
                project.name.clone(),
                project.path.clone(),
            );
        }
    }

    pub fn remove_project(&self, project_id: &str) {
        if let Ok(mut guard) = self.projects.lock() {
            guard.remove(project_id);
        }
    }

    pub fn clear(&self) {
        if let Ok(mut guard) = self.projects.lock() {
            guard.clear();
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
    ) {
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

    pub fn refresh_ai_once(&self, project: ProjectSummary, ai_history: Arc<AIHistoryIndexer>) {
        self.mark_project_summary(&project);
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

    fn projects_due_for_git(&self, interval: Duration) -> Vec<TrackedProject> {
        projects_due(&self.projects, interval, |project| {
            &mut project.last_git_refresh
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
        for project in coordinator.projects_due_for_git(interval) {
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
) {
    if id.trim().is_empty() || path.trim().is_empty() {
        return;
    }
    projects
        .entry(id.clone())
        .and_modify(|project| {
            project.name = name.clone();
            project.path = path.clone();
        })
        .or_insert_with(|| TrackedProject {
            id,
            name,
            path,
            last_git_refresh: None,
            last_ai_refresh: Some(Instant::now()),
        });
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

fn refresh_git_project(app: AppHandle, project: TrackedProject) {
    thread::spawn(move || {
        let snapshot = git_status(project.path.clone());
        if snapshot.is_repository || snapshot.error.is_none() || Path::new(&project.path).exists() {
            let _ = app.emit(
                "git:status",
                GitStatusEvent {
                    project_id: project.id,
                    project_name: project.name,
                    project_path: project.path,
                    snapshot,
                },
            );
        }
    });
}

fn configured_interval_seconds(value: &str, minimum: u64) -> Option<Duration> {
    let seconds = value.trim().parse::<u64>().ok()?;
    (seconds > 0).then(|| Duration::from_secs(seconds.max(minimum)))
}
