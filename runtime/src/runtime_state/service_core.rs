static POWER_MANAGER: OnceLock<Arc<PowerManager>> = OnceLock::new();
static AI_HISTORY_INDEXER: OnceLock<AIHistoryIndexer> = OnceLock::new();
static REMOTE_HOST_RUNTIME: OnceLock<Arc<RemoteHostRuntime>> = OnceLock::new();

fn shared_power_manager() -> Arc<PowerManager> {
    Arc::clone(POWER_MANAGER.get_or_init(|| Arc::new(PowerManager::default())))
}

fn shared_ai_history_indexer() -> AIHistoryIndexer {
    AI_HISTORY_INDEXER.get_or_init(AIHistoryIndexer::new).clone()
}

fn shared_remote_host_runtime(support_dir: PathBuf, ai_history: AIHistoryIndexer) -> Arc<RemoteHostRuntime> {
    Arc::clone(REMOTE_HOST_RUNTIME.get_or_init(|| {
        Arc::new(RemoteHostRuntime::new_with_ai_history(support_dir, ai_history))
    }))
}

impl RuntimeService {
    pub fn new(support_dir: PathBuf) -> Self {
        let ai_history_indexer = shared_ai_history_indexer();
        let project_activity = Arc::new(ProjectActivityCoordinator::new(
            support_dir.clone(),
            ai_history_indexer.clone(),
        ));
        project_activity.seed_projects(ProjectStore::new(support_dir.clone()).projects_snapshot());
        let remote_ai_history_indexer = ai_history_indexer.clone();
        let remote_host =
            shared_remote_host_runtime(support_dir.clone(), remote_ai_history_indexer);
        Self {
            support_dir: support_dir.clone(),
            ai_history_indexer,
            project_activity,
            ai_runtime: Arc::new(AIRuntimeBridge::new()),
            file_watch_manager: Arc::new(FileWatchManager::default()),
            git_watch_manager: Arc::new(git::GitWatchManager::default()),
            file_watch_events: Arc::new(Mutex::new(VecDeque::new())),
            active_file_watch_path: Arc::new(Mutex::new(None)),
            git_cancels: Arc::new(Mutex::new(HashMap::new())),
            power_manager: shared_power_manager(),
            remote_host,
        }
    }

    pub fn reload_state(&self) -> RuntimeState {
        RuntimeState::load_from_support_dir(self.support_dir.clone())
    }

    pub fn runtime_trace_frontend(&self, category: &str, message: &str) {
        crate::runtime_trace::runtime_trace(category, message);
    }

    pub fn ai_runtime_probe(
        &self,
        request: AIRuntimeProbeRequest,
    ) -> Option<AIRuntimeContextSnapshot> {
        crate::ai_runtime::probe_runtime(&request)
    }

    pub fn app_runtime_ready(&self, visible: bool, focused: bool) -> AppRuntimeReadySnapshot {
        let started_at = std::time::Instant::now();
        let project_store = ProjectStore::new(self.support_dir.clone());
        let projects = project_store.list_snapshot();
        let selected_project_id = projects
            .selected_project_id
            .as_deref()
            .unwrap_or("none")
            .to_string();

        crate::runtime_trace::runtime_trace(
            "startup",
            &format!(
                "app_runtime_ready start projects={} selected={selected_project_id}",
                projects.projects.len()
            ),
        );

        self.project_activity
            .seed_projects(project_store.projects_snapshot());
        self.project_activity.mark_main_window_visible(visible);
        self.project_activity.mark_main_window_focused(focused);

        if let Some(project) = projects
            .selected_project_id
            .as_ref()
            .and_then(|id| projects.projects.iter().find(|project| &project.id == id))
            .cloned()
        {
            let _ = self.mark_project_active_with_watch(&project.id);
        }

        let ai_runtime_state = self.ai_runtime.runtime_state_snapshot();
        let project_activity = self.project_activity.snapshot();
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        let window_state = RuntimeWindowStateSnapshot {
            project_activity: project_activity.clone(),
            shows_dock_badge: settings.shows_dock_badge,
            attention_count: runtime_attention_count(&ai_runtime_state),
            dock_badge_count: runtime_dock_badge_count(
                settings.shows_dock_badge,
                &ai_runtime_state,
            ),
        };

        let snapshot = AppRuntimeReadySnapshot {
            projects,
            terminal_layouts: project_store.terminal_layouts_snapshot(),
            remote: self.remote_host.start(),
            ai_runtime_state,
            project_activity,
            window_state,
        };

        crate::runtime_trace::runtime_trace_elapsed(
            "startup",
            "app_runtime_ready finish",
            started_at,
            &format!(
                "projects={} selected={selected_project_id}",
                snapshot.projects.projects.len()
            ),
        );

        snapshot
    }

    pub fn reload_project_files(
        &self,
        project_path: &str,
        directory_path: Option<&str>,
    ) -> Vec<FileEntry> {
        load_file_entries(project_path, directory_path)
    }

    pub fn watch_project_files(
        &self,
        project_path: String,
        on_change: impl Fn(FileChangeEvent) + Send + 'static,
    ) -> Result<FileWatchRegistration, String> {
        self.file_watch_manager.watch(project_path, on_change)
    }

    pub fn unwatch_project_files(&self, project_path: String) -> Result<(), String> {
        self.file_watch_manager.unwatch(project_path)
    }

    pub fn file_watch(&self, project_path: String) -> Result<FileWatchRegistration, String> {
        let events = Arc::clone(&self.file_watch_events);
        self.watch_project_files(project_path, move |event| {
            if let Ok(mut events) = events.lock() {
                events.push_back(event);
                while events.len() > 128 {
                    events.pop_front();
                }
            }
        })
    }

    pub fn file_unwatch(&self, project_path: String) -> Result<(), String> {
        self.unwatch_project_files(project_path)
    }

    pub fn drain_file_change_events(&self) -> Vec<FileChangeEvent> {
        self.file_watch_events
            .lock()
            .map(|mut events| events.drain(..).collect())
            .unwrap_or_default()
    }

    pub fn watch_project_git(
        &self,
        project_path: String,
        on_change: impl Fn(git::GitRepositoryChangeEvent) + Send + Sync + 'static,
    ) -> Result<git::GitWatchRegistration, String> {
        self.git_watch_manager.watch(project_path, on_change)
    }

    pub fn unwatch_project_git(&self, project_path: String) -> Result<(), String> {
        self.git_watch_manager.unwatch(project_path)
    }

    pub fn git_watch(&self, project_path: String) -> Result<git::GitWatchRegistration, String> {
        let activity = Arc::clone(&self.project_activity);
        let support_dir = self.support_dir.clone();
        self.watch_project_git(project_path, move |event| {
            let project_store = ProjectStore::new(support_dir.clone());
            activity.refresh_git_changed(
                &project_store,
                event.project_path,
                event.repository_path,
                event.changed_paths,
            );
        })
    }

    pub fn git_unwatch(&self, project_path: String) -> Result<(), String> {
        self.unwatch_project_git(project_path)
    }

    fn watch_active_project_files(
        &self,
        project_path: String,
    ) -> Result<FileWatchRegistration, String> {
        let registration = self.file_watch_manager.registration(&project_path)?;
        let previous = self
            .active_file_watch_path
            .lock()
            .map_err(|_| "Active file watcher lock is poisoned.".to_string())?
            .clone();

        if previous.as_deref() == Some(registration.project_path.as_str()) {
            return Ok(registration);
        }

        if let Some(previous) = previous {
            let _ = self.file_unwatch(previous);
        }

        let registration = self.file_watch(project_path)?;
        if let Ok(mut active) = self.active_file_watch_path.lock() {
            *active = Some(registration.project_path.clone());
        }
        Ok(registration)
    }

    fn stop_active_project_files(&self) {
        let previous = self
            .active_file_watch_path
            .lock()
            .ok()
            .and_then(|mut active| active.take());
        if let Some(previous) = previous {
            let _ = self.file_unwatch(previous);
        }
    }

    pub fn reload_project_git(&self, project_path: &str) -> git::GitSummary {
        load_git_summary(project_path)
    }

    pub fn reload_project_git_review(
        &self,
        project_path: &str,
        base_branch: Option<&str>,
    ) -> git::GitReviewSummary {
        git::GitService::review(project_path, base_branch)
    }

    pub fn project_activity_snapshot(&self) -> ProjectActivitySnapshot {
        self.project_activity.snapshot()
    }

    pub fn mark_main_window_state(&self, visible: bool, focused: bool) -> ProjectActivitySnapshot {
        self.project_activity.mark_main_window_visible(visible);
        self.project_activity.mark_main_window_focused(focused);
        self.project_activity.snapshot()
    }

    pub fn app_window_state(&self, visible: bool, focused: bool) -> RuntimeWindowStateSnapshot {
        self.project_activity.mark_main_window_visible(visible);
        self.project_activity.mark_main_window_focused(focused);
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        let ai_runtime_state = self.ai_runtime.runtime_state_snapshot();
        RuntimeWindowStateSnapshot {
            project_activity: self.project_activity.snapshot(),
            shows_dock_badge: settings.shows_dock_badge,
            attention_count: runtime_attention_count(&ai_runtime_state),
            dock_badge_count: runtime_dock_badge_count(
                settings.shows_dock_badge,
                &ai_runtime_state,
            ),
        }
    }

    pub fn mark_project_active(&self, project_id: &str) -> Result<ProjectActivitySnapshot, String> {
        let project = ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        self.project_activity.mark_project_active(project);
        Ok(self.project_activity.snapshot())
    }

    pub fn mark_project_active_with_watch(
        &self,
        project_id: &str,
    ) -> Result<ProjectActivitySnapshot, String> {
        let project = ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        self.project_activity.mark_project_active(project.clone());

        let _ = self.git_watch(project.path.clone());
        let _ = self.watch_active_project_files(project.path);

        Ok(self.project_activity.snapshot())
    }

    pub fn refresh_project_activity(
        &self,
        project_id: &str,
    ) -> Result<ProjectActivitySnapshot, String> {
        let project = ProjectStore::new(self.support_dir.clone())
            .project_summaries()
            .into_iter()
            .find(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        self.project_activity.refresh_project_now(project);
        Ok(self.project_activity.snapshot())
    }

    pub fn tick_project_activity(&self) -> ProjectActivitySnapshot {
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        self.project_activity.run_tick(&settings);
        self.project_activity.snapshot()
    }

    pub fn drain_project_activity_events(&self) -> Vec<ProjectActivityEvent> {
        self.project_activity.drain_events()
    }

    pub fn indexed_project_ai_history_summary(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        self.ai_history_indexer.project_summary(project)
    }

    pub fn refresh_indexed_project_ai_history(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<(), String> {
        self.ai_history_indexer.refresh_project(project)
    }

    pub fn active_ai_history_index_count(&self) -> usize {
        self.ai_history_indexer.active_project_count()
    }

    pub fn indexed_project_ai_history_state(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        self.ai_history_indexer.project_state(project)
    }

    pub fn indexed_global_ai_history_summary(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<AIGlobalHistorySnapshot, String> {
        self.ai_history_indexer.global_summary(projects)
    }

    pub fn indexed_global_ai_history_state(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<Option<AIGlobalHistorySnapshot>, String> {
        self.ai_history_indexer.global_state(projects)
    }

    pub fn refresh_indexed_global_ai_history(
        &self,
        projects: Vec<AIHistoryProjectRequest>,
    ) -> Result<(), String> {
        self.ai_history_indexer.refresh_global(projects)
    }

    pub fn global_today_normalized_ai_tokens(&self) -> Result<i64, String> {
        global_today_normalized_tokens().map_err(|error| error.to_string())
    }

    pub fn rename_indexed_ai_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
        title: String,
    ) -> Result<AIHistoryProjectState, String> {
        self.ai_history_indexer
            .rename_session(project, session_id, title)
    }

    pub fn remove_indexed_ai_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
    ) -> Result<AIHistoryProjectState, String> {
        self.ai_history_indexer.remove_session(project, session_id)
    }

    pub fn drain_ai_history_events(&self) -> Vec<AIHistoryEvent> {
        self.ai_history_indexer.drain_events()
    }

    pub fn prepare_ai_runtime_bridge(&self) -> Result<AIRuntimeBridgeSnapshot, String> {
        self.ai_runtime.prepare()?;
        Ok(self.ai_runtime.snapshot())
    }

    pub fn start_ai_runtime_event_processing(&self) -> Result<AIRuntimeBridgeSnapshot, String> {
        self.ai_runtime.start_event_processing_background()?;
        Ok(self.ai_runtime.snapshot())
    }

    pub fn ai_runtime_bridge_snapshot(&self) -> AIRuntimeBridgeSnapshot {
        self.ai_runtime.snapshot()
    }

    pub fn ai_runtime_state_snapshot(&self) -> AIRuntimeStateSnapshot {
        self.ai_runtime.runtime_state_snapshot()
    }

    pub fn save_ai_runtime_state_snapshot(
        &self,
        snapshot: &AIRuntimeStateSnapshot,
    ) -> Result<AIRuntimeStateSummary, String> {
        AIRuntimeStateService::new(self.support_dir.clone()).save_from_runtime_snapshot(snapshot)
    }

    pub fn poll_ai_runtime_state(&self) -> Result<AIRuntimeStateSnapshot, String> {
        self.ai_runtime.poll_runtime_state()?;
        Ok(self.ai_runtime.runtime_state_snapshot())
    }

    pub fn ai_runtime_dismiss_completion(&self, project_id: &str) -> bool {
        self.ai_runtime.dismiss_completion(project_id)
    }

    pub fn dismiss_ai_runtime_completion(&self, project_id: &str) -> AIRuntimeStateSnapshot {
        let _ = self.ai_runtime_dismiss_completion(project_id);
        self.ai_runtime.runtime_state_snapshot()
    }

    pub fn drain_ai_runtime_events(&self) -> Vec<AIRuntimeSupervisorEvent> {
        self.ai_runtime.drain_supervisor_events()
    }

    pub fn drain_ai_runtime_events_and_enqueue_memory(&self) -> AIRuntimeDrainResult {
        let events = self.ai_runtime.drain_supervisor_events();
        let memory = events
            .iter()
            .filter_map(|event| match event {
                AIRuntimeSupervisorEvent::Completion { completion } => completion.session.as_ref(),
                _ => None,
            })
            .filter_map(|session| self.enqueue_completed_session_memory(session).ok())
            .collect::<Vec<_>>();
        AIRuntimeDrainResult { events, memory }
    }
}

fn runtime_attention_count(snapshot: &AIRuntimeStateSnapshot) -> usize {
    snapshot.needs_input_count + snapshot.completion_count
}

fn runtime_dock_badge_count(
    shows_dock_badge: bool,
    snapshot: &AIRuntimeStateSnapshot,
) -> Option<i64> {
    let attention_count = runtime_attention_count(snapshot);
    if shows_dock_badge && attention_count > 0 {
        Some(attention_count as i64)
    } else {
        None
    }
}

#[cfg(test)]
mod app_runtime_ready_tests {
    use super::*;
    use serde_json::json;
    use std::{fs, path::PathBuf};

    #[test]
    fn app_runtime_ready_marks_selected_project_active_and_returns_startup_snapshots() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-runtime-ready-{}", uuid::Uuid::new_v4()));
        let project_dir = support_dir.join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");
        fs::write(
            support_dir.join("state.json"),
            json!({
                "projects": [
                    {
                        "id": "project-1",
                        "name": "Runtime Ready",
                        "path": project_dir.to_string_lossy()
                    }
                ],
                "selectedProjectId": "project-1"
            })
            .to_string(),
        )
        .expect("write state");
        let service = RuntimeService::new(PathBuf::from(&support_dir));
        let snapshot = service.app_runtime_ready(true, true);

        assert_eq!(
            snapshot.projects.selected_project_id.as_deref(),
            Some("project-1")
        );
        assert_eq!(
            snapshot.project_activity.active_project_id.as_deref(),
            Some("project-1")
        );
        assert!(snapshot.project_activity.visible);
        assert!(snapshot.project_activity.focused);
        assert!(snapshot.window_state.project_activity.visible);
        assert!(snapshot.window_state.project_activity.focused);
        assert_eq!(snapshot.window_state.attention_count, 0);
        assert_eq!(snapshot.window_state.dock_badge_count, None);
        assert_eq!(snapshot.terminal_layouts.layouts.len(), 0);
        assert_eq!(snapshot.ai_runtime_state.sessions.len(), 0);
        let expected_watch_path = project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        assert_eq!(
            service
                .active_file_watch_path
                .lock()
                .expect("active file watch")
                .as_deref(),
            Some(expected_watch_path.as_str())
        );

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_update_marks_updated_project_active_and_rewatches_files() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-project-update-{}", uuid::Uuid::new_v4()));
        let old_project_dir = support_dir.join("old-project");
        let new_project_dir = support_dir.join("new-project");
        fs::create_dir_all(&old_project_dir).expect("create old project dir");
        fs::create_dir_all(&new_project_dir).expect("create new project dir");
        fs::write(
            support_dir.join("state.json"),
            json!({
                "projects": [
                    {
                        "id": "project-1",
                        "name": "Old Project",
                        "path": old_project_dir.to_string_lossy()
                    }
                ],
                "selectedProjectId": "project-1"
            })
            .to_string(),
        )
        .expect("write state");
        fs::write(
            support_dir.join("pet-state.json"),
            serde_json::to_vec(&crate::pet::PetSnapshot::default()).expect("encode empty pet"),
        )
        .expect("write pet state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));
        service.app_runtime_ready(true, true);

        service
            .update_project(
                "project-1",
                "New Project",
                new_project_dir.to_str().unwrap(),
            )
            .expect("update project");

        let expected_watch_path = new_project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        assert_eq!(
            service
                .active_file_watch_path
                .lock()
                .expect("active file watch")
                .as_deref(),
            Some(expected_watch_path.as_str())
        );
        assert_eq!(
            service
                .project_activity_snapshot()
                .active_project_id
                .as_deref(),
            Some("project-1")
        );

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_select_worktree_marks_root_project_active_and_keeps_file_watch() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-project-select-worktree-{}",
            uuid::Uuid::new_v4()
        ));
        let project_dir = support_dir.join("project");
        let worktree_dir = support_dir.join("worktree");
        fs::create_dir_all(&project_dir).expect("create project dir");
        fs::create_dir_all(&worktree_dir).expect("create worktree dir");
        fs::write(
            support_dir.join("state.json"),
            json!({
                "projects": [
                    {
                        "id": "project-1",
                        "name": "Project",
                        "path": project_dir.to_string_lossy()
                    }
                ],
                "worktrees": [
                    {
                        "id": "worktree-1",
                        "projectId": "project-1",
                        "name": "Feature",
                        "branch": "feature",
                        "path": worktree_dir.to_string_lossy(),
                        "status": "active",
                        "isDefault": false,
                        "createdAt": 1,
                        "updatedAt": 1
                    }
                ],
                "selectedProjectId": "project-1"
            })
            .to_string(),
        )
        .expect("write state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));
        service
            .project_select_worktree(crate::project_store::ProjectSelectWorktreeRequest {
                project_id: "project-1".to_string(),
                worktree_id: "worktree-1".to_string(),
            })
            .expect("select worktree");

        let expected_watch_path = project_dir
            .canonicalize()
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        assert_eq!(
            service
                .active_file_watch_path
                .lock()
                .expect("active file watch")
                .as_deref(),
            Some(expected_watch_path.as_str())
        );
        assert_eq!(
            service
                .project_activity_snapshot()
                .active_project_id
                .as_deref(),
            Some("project-1")
        );
        assert_eq!(
            service
                .project_list()
                .selected_worktree_id_by_project
                .get("project-1")
                .map(String::as_str),
            Some("worktree-1")
        );

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_close_forgets_pet_baseline_and_close_all_forgets_all_baselines() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-project-close-pet-baseline-{}",
            uuid::Uuid::new_v4()
        ));
        let first_dir = support_dir.join("first");
        let second_dir = support_dir.join("second");
        fs::create_dir_all(&first_dir).expect("create first project dir");
        fs::create_dir_all(&second_dir).expect("create second project dir");
        fs::write(
            support_dir.join("state.json"),
            json!({
                "projects": [
                    {
                        "id": "project-1",
                        "name": "First",
                        "path": first_dir.to_string_lossy()
                    },
                    {
                        "id": "project-2",
                        "name": "Second",
                        "path": second_dir.to_string_lossy()
                    }
                ],
                "selectedProjectId": "project-1"
            })
            .to_string(),
        )
        .expect("write state");
        let mut pet_snapshot = crate::pet::PetSnapshot {
            claimed_at: Some(1),
            species: "codux".to_string(),
            global_normalized_total_watermark: Some(30),
            ..crate::pet::PetSnapshot::default()
        };
        pet_snapshot
            .project_normalized_token_watermarks
            .insert("project-1".to_string(), 10);
        pet_snapshot
            .project_normalized_token_watermarks
            .insert("project-2".to_string(), 20);
        fs::write(
            support_dir.join("pet-state.json"),
            serde_json::to_vec(&pet_snapshot).expect("encode pet"),
        )
        .expect("write pet state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));

        service
            .project_close(crate::project_store::ProjectCloseRequest {
                project_id: "project-1".to_string(),
            })
            .expect("close first project");
        let pet = service.pet_snapshot().expect("pet snapshot after close");
        assert!(
            !pet.project_normalized_token_watermarks
                .contains_key("project-1")
        );
        assert_eq!(
            pet.project_normalized_token_watermarks.get("project-2"),
            Some(&20)
        );
        assert_eq!(pet.global_normalized_total_watermark, Some(20));

        service.project_close_all().expect("close all projects");
        let pet = service
            .pet_snapshot()
            .expect("pet snapshot after close all");
        assert!(pet.project_normalized_token_watermarks.is_empty());
        assert_eq!(pet.global_normalized_total_watermark, None);

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn file_watch_events_are_queued_and_drained_for_gpui() {
        let support_dir =
            std::env::temp_dir().join(format!("codux-file-watch-events-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&support_dir).expect("create support dir");
        let service = RuntimeService::new(PathBuf::from(&support_dir));

        service
            .file_watch_events
            .lock()
            .expect("file event queue")
            .push_back(FileChangeEvent {
                project_path: "/tmp/project".to_string(),
                changed_paths: vec!["/tmp/project/src/main.rs".to_string()],
            });

        let events = service.drain_file_change_events();
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].project_path, "/tmp/project");
        assert!(service.drain_file_change_events().is_empty());

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn runtime_dock_badge_count_matches_tauri_attention_semantics() {
        let mut snapshot = AIRuntimeStateSnapshot::default();

        assert_eq!(runtime_dock_badge_count(true, &snapshot), None);

        snapshot.needs_input_count = 2;
        snapshot.completion_count = 3;

        assert_eq!(runtime_dock_badge_count(true, &snapshot), Some(5));
        assert_eq!(runtime_dock_badge_count(false, &snapshot), None);
    }
}
