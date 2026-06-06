static POWER_MANAGER: OnceLock<Arc<PowerManager>> = OnceLock::new();
static AI_RUNTIME_BRIDGE: OnceLock<Arc<AIRuntimeBridge>> = OnceLock::new();
static TERMINAL_MANAGER: OnceLock<Arc<TerminalManager>> = OnceLock::new();

fn shared_power_manager() -> Arc<PowerManager> {
    Arc::clone(POWER_MANAGER.get_or_init(|| Arc::new(PowerManager::default())))
}

fn shared_ai_runtime_bridge() -> Arc<AIRuntimeBridge> {
    Arc::clone(AI_RUNTIME_BRIDGE.get_or_init(|| Arc::new(AIRuntimeBridge::new())))
}

fn shared_terminal_manager(ai_runtime: Arc<AIRuntimeBridge>) -> Arc<TerminalManager> {
    Arc::clone(
        TERMINAL_MANAGER.get_or_init(|| Arc::new(TerminalManager::with_ai_runtime(ai_runtime))),
    )
}

fn new_remote_host_runtime(
    support_dir: PathBuf,
    ai_history: AIHistoryIndexer,
    terminals: Arc<TerminalManager>,
) -> Arc<RemoteHostRuntime> {
    Arc::new(RemoteHostRuntime::new_with_ai_history_and_terminals(
        support_dir,
        ai_history,
        terminals,
    ))
}

impl RuntimeService {
    pub fn new(support_dir: PathBuf) -> Self {
        let ai_history_indexer =
            AIHistoryIndexer::with_database_path(support_dir.join("ai-usage.sqlite3"));
        let ai_runtime = shared_ai_runtime_bridge();
        let terminal_manager = shared_terminal_manager(Arc::clone(&ai_runtime));
        let project_activity = Arc::new(ProjectActivityCoordinator::new(
            support_dir.clone(),
            ai_history_indexer.clone(),
        ));
        project_activity.seed_projects(ProjectStore::new(support_dir.clone()).projects_snapshot());
        let remote_ai_history_indexer = ai_history_indexer.clone();
        let remote_host = new_remote_host_runtime(
            support_dir.clone(),
            remote_ai_history_indexer,
            terminal_manager,
        );
        Self {
            support_dir: support_dir.clone(),
            ai_history_indexer,
            project_activity,
            ai_runtime,
            file_watch_manager: Arc::new(FileWatchManager::default()),
            git_watch_manager: Arc::new(git::GitWatchManager::default()),
            file_watch_events: Arc::new(Mutex::new(VecDeque::new())),
            active_file_watch_path: Arc::new(Mutex::new(None)),
            ai_history_activation_keys: Arc::new(Mutex::new(HashSet::new())),
            git_cancels: Arc::new(Mutex::new(HashMap::new())),
            power_manager: shared_power_manager(),
            remote_host,
        }
    }

    pub fn reload_state(&self) -> RuntimeState {
        RuntimeState::load_from_support_dir(self.support_dir.clone())
    }

    pub fn reload_settings(&self) -> SettingsSummary {
        SettingsService::new(self.support_dir.clone()).summary()
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
            self.project_activity.mark_project_active(project.clone());
            let _ = self.mark_active_project_file_path(&project.path);
            self.watch_project_background(project.path);
            self.refresh_active_ai_history_background();
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
        let (registration, previous) = self.mark_active_project_file_path(&project_path)?;
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

    pub(crate) fn mark_active_project_file_path(
        &self,
        project_path: &str,
    ) -> Result<(FileWatchRegistration, Option<String>), String> {
        let registration = self.file_watch_manager.registration(project_path)?;
        let mut active = self
            .active_file_watch_path
            .lock()
            .map_err(|_| "Active file watcher lock is poisoned.".to_string())?;
        let previous = active.clone();
        if previous.as_deref() != Some(registration.project_path.as_str()) {
            *active = Some(registration.project_path.clone());
        }
        Ok((registration, previous))
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
        refresh_git_summary(&self.support_dir, project_path)
    }

    pub fn stored_project_git_state(
        &self,
        project_path: &str,
        base_branch: Option<&str>,
    ) -> (git::GitSummary, git::GitReviewSummary) {
        (
            crate::runtime_cache::cached_git_summary(&self.support_dir, project_path)
                .unwrap_or_default(),
            crate::runtime_cache::cached_git_review(&self.support_dir, project_path, base_branch)
                .unwrap_or_default(),
        )
    }

    pub fn reload_project_git_review(
        &self,
        project_path: &str,
        base_branch: Option<&str>,
    ) -> git::GitReviewSummary {
        refresh_git_review(&self.support_dir, project_path, base_branch)
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

    pub fn ai_runtime_dock_badge_count(&self) -> Option<i64> {
        let settings = SettingsService::new(self.support_dir.clone()).summary();
        let ai_runtime_state = self.ai_runtime.runtime_state_snapshot();
        runtime_dock_badge_count(settings.shows_dock_badge, &ai_runtime_state)
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
        let _ = self.mark_active_project_file_path(&project.path);

        self.watch_project_background(project.path);
        self.refresh_active_ai_history_background();

        Ok(self.project_activity.snapshot())
    }

    fn refresh_active_ai_history_background(&self) {
        let Some(request) = self.active_ai_history_project_request() else {
            return;
        };
        let activation_key = format!("{}:{}", request.id, request.path);
        let should_refresh = self
            .ai_history_activation_keys
            .lock()
            .map(|mut keys| keys.insert(activation_key))
            .unwrap_or(false);
        if !should_refresh {
            return;
        }
        let ai_history = self.ai_history_indexer.clone();
        let _ = std::thread::Builder::new()
            .name("codux-ai-history-activation".to_string())
            .spawn(move || {
                let _ = ai_history.refresh_project(request);
            });
    }

    fn active_ai_history_project_request(&self) -> Option<AIHistoryProjectRequest> {
        let store = ProjectStore::new(self.support_dir.clone());
        let snapshot = store.snapshot();
        let project = snapshot
            .selected_project_id
            .as_ref()
            .and_then(|id| snapshot.projects.iter().find(|project| &project.id == id))
            .or_else(|| snapshot.projects.first())?;
        let selected_worktree_id = snapshot
            .selected_worktree_id_by_project
            .get(&project.id)
            .map(String::as_str)
            .unwrap_or(project.id.as_str());
        if selected_worktree_id != project.id
            && let Some(worktree) = snapshot
                .worktrees
                .iter()
                .find(|worktree| worktree.id == selected_worktree_id)
        {
            return Some(AIHistoryProjectRequest {
                id: worktree.id.clone(),
                name: worktree.name.clone(),
                path: worktree.path.clone(),
            });
        }
        Some(AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        })
    }

    pub fn watch_project_background(&self, project_path: String) {
        let service = self.clone();
        let _ = std::thread::Builder::new()
            .name("codux-project-watch-switch".to_string())
            .spawn(move || {
                let _ = service.watch_active_project_files(project_path.clone());
                let _ = service.git_watch(project_path);
            });
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
        global_today_normalized_tokens_at(self.support_dir.join("ai-usage.sqlite3"))
            .map_err(|error| error.to_string())
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

    pub fn drain_ai_history_events(&self) -> AIHistoryDrainResult {
        let events = self.ai_history_indexer.drain_events();
        let should_refresh_pet = events.iter().any(|event| {
            matches!(
                event,
                AIHistoryEvent::Project { .. }
                    | AIHistoryEvent::ProjectState {
                        state: AIHistoryProjectState {
                            is_loading: false,
                            queued: false,
                            error: None,
                            snapshot: Some(_),
                            ..
                        },
                    }
            )
        });
        if !should_refresh_pet {
            return AIHistoryDrainResult {
                events,
                ..Default::default()
            };
        }
        match self.refresh_pet_from_indexed_history() {
            Ok(pet) => {
                let pet_snapshot = self.pet_snapshot().ok();
                AIHistoryDrainResult {
                    events,
                    pet: Some(pet),
                    pet_snapshot,
                    pet_error: None,
                }
            }
            Err(error) => AIHistoryDrainResult {
                events,
                pet: None,
                pet_snapshot: None,
                pet_error: Some(error),
            },
        }
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

    pub fn summarize_ai_runtime_state_snapshot(
        &self,
        snapshot: &AIRuntimeStateSnapshot,
    ) -> AIRuntimeStateSummary {
        AIRuntimeStateService::new(self.support_dir.clone()).summary_from_runtime_snapshot(snapshot)
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
    use crate::terminal_layout::{TerminalPaneSummary, TerminalTabSummary, terminal_layout_storage_key};
    use crate::terminal_pty::TerminalLaunchContext;
    use serde_json::json;
    use std::{fs, path::PathBuf, sync::Arc, thread, time::{Duration, Instant}};

    fn wait_for_active_watch_path(service: &RuntimeService, expected: &str) {
        for _ in 0..50 {
            let current = service
                .active_file_watch_path
                .lock()
                .expect("active file watch")
                .clone();
            if current.as_deref() == Some(expected) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(
            service
                .active_file_watch_path
                .lock()
                .expect("active file watch")
                .as_deref(),
            Some(expected)
        );
    }

    fn wait_for_ai_history_loading_event(
        service: &RuntimeService,
        project_id: &str,
        project_path: &str,
    ) {
        for _ in 0..80 {
            let result = service.drain_ai_history_events();
            if result.events.iter().any(|event| {
                matches!(
                    event,
                    AIHistoryEvent::ProjectState { state }
                        if state.project_id == project_id
                            && state.project_path == project_path
                            && state.is_loading
                )
            }) {
                return;
            }
            thread::sleep(Duration::from_millis(10));
        }
        let result = service.drain_ai_history_events();
        assert!(
            result.events.iter().any(|event| {
                matches!(
                    event,
                    AIHistoryEvent::ProjectState { state }
                        if state.project_id == project_id
                            && state.project_path == project_path
                            && state.is_loading
                )
            }),
            "expected AI history loading event for {project_id} at {project_path}, got {:?}",
            result.events
        );
    }

    fn assert_tracked_project_has_git_refresh(service: &RuntimeService, project_id: &str) {
        let activity = service.project_activity_snapshot();
        let tracked = activity
            .tracked_projects
            .iter()
            .find(|project| project.id == project_id)
            .unwrap_or_else(|| panic!("missing tracked project {project_id}: {activity:?}"));
        assert!(
            tracked.has_git_refresh,
            "expected git refresh marker for {project_id}: {activity:?}"
        );
        assert!(
            activity.activated_git_count > 0,
            "expected activated git count after project activation: {activity:?}"
        );
    }

    #[cfg(unix)]
    fn recv_until_contains(
        rx: &flume::Receiver<Vec<u8>>,
        needle: &str,
        timeout: Duration,
    ) -> String {
        let deadline = Instant::now() + timeout;
        let mut output = String::new();
        while Instant::now() < deadline {
            let remaining = deadline.saturating_duration_since(Instant::now());
            match rx.recv_timeout(remaining.min(Duration::from_millis(50))) {
                Ok(bytes) => {
                    output.push_str(&String::from_utf8_lossy(&bytes));
                    if output.contains(needle) {
                        return output;
                    }
                }
                Err(flume::RecvTimeoutError::Timeout) => {}
                Err(flume::RecvTimeoutError::Disconnected) => break,
            }
        }
        output
    }

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
        wait_for_active_watch_path(&service, &expected_watch_path);
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
        wait_for_active_watch_path(&service, &expected_watch_path);
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
    fn project_and_worktree_switch_loads_terminal_layout_for_selected_worktree() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-project-worktree-terminal-layout-{}",
            uuid::Uuid::new_v4()
        ));
        let project_dir = support_dir.join("project");
        let worktree_a_dir = support_dir.join("worktree-a");
        let worktree_b_dir = support_dir.join("worktree-b");
        fs::create_dir_all(&project_dir).expect("create project dir");
        fs::create_dir_all(&worktree_a_dir).expect("create worktree a dir");
        fs::create_dir_all(&worktree_b_dir).expect("create worktree b dir");
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
                        "id": "worktree-a",
                        "projectId": "project-1",
                        "name": "Task A",
                        "branch": "task-a",
                        "path": worktree_a_dir.to_string_lossy(),
                        "status": "active",
                        "isDefault": false,
                        "createdAt": 1,
                        "updatedAt": 1
                    },
                    {
                        "id": "worktree-b",
                        "projectId": "project-1",
                        "name": "Task B",
                        "branch": "task-b",
                        "path": worktree_b_dir.to_string_lossy(),
                        "status": "active",
                        "isDefault": false,
                        "createdAt": 1,
                        "updatedAt": 1
                    }
                ],
                "worktreeTasks": [
                    {
                        "worktreeId": "worktree-a",
                        "title": "Task A",
                        "baseBranch": "main",
                        "baseCommit": null,
                        "status": "active",
                        "createdAt": 1,
                        "updatedAt": 1,
                        "startedAt": null,
                        "completedAt": null
                    },
                    {
                        "worktreeId": "worktree-b",
                        "title": "Task B",
                        "baseBranch": "main",
                        "baseCommit": null,
                        "status": "active",
                        "createdAt": 1,
                        "updatedAt": 1,
                        "startedAt": null,
                        "completedAt": null
                    }
                ],
                "selectedProjectId": "project-1",
                "selectedWorktreeIdByProject": {
                    "project-1": "worktree-a"
                }
            })
            .to_string(),
        )
        .expect("write state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));
        service
            .save_terminal_layout(
                &crate::terminal_layout::terminal_layout_storage_key("project-1", "worktree-a"),
                Vec::new(),
                "terminal-a".to_string(),
                vec![TerminalPaneSummary {
                    title: "Task A".to_string(),
                    terminal_id: "terminal-a".to_string(),
                }],
                vec![1.0],
                0.18,
            )
            .expect("save worktree a terminal layout");
        service
            .save_terminal_layout(
                &crate::terminal_layout::terminal_layout_storage_key("project-1", "worktree-b"),
                Vec::new(),
                "terminal-b".to_string(),
                vec![TerminalPaneSummary {
                    title: "Task B".to_string(),
                    terminal_id: "terminal-b".to_string(),
                }],
                vec![1.0],
                0.52,
            )
            .expect("save worktree b terminal layout");

        let state = RuntimeState::load_from_support_dir(support_dir.clone());
        assert_eq!(
            state.worktrees.selected_worktree_id.as_deref(),
            Some("worktree-a")
        );
        assert_eq!(state.terminal_layout.active_terminal_id, "terminal-a");
        assert_eq!(state.terminal_layout.top_panes[0].terminal_id, "terminal-a");
        assert_eq!(state.terminal_layout.bottom_ratio, 0.18);

        service
            .project_select_worktree(crate::project_store::ProjectSelectWorktreeRequest {
                project_id: "project-1".to_string(),
                worktree_id: "worktree-b".to_string(),
            })
            .expect("select worktree b");
        let state = RuntimeState::load_from_support_dir(support_dir.clone());
        assert_eq!(
            state.worktrees.selected_worktree_id.as_deref(),
            Some("worktree-b")
        );
        assert_eq!(state.terminal_layout.active_terminal_id, "terminal-b");
        assert_eq!(state.terminal_layout.top_panes[0].terminal_id, "terminal-b");
        assert_eq!(state.terminal_layout.bottom_ratio, 0.52);

        service
            .project_select_worktree(crate::project_store::ProjectSelectWorktreeRequest {
                project_id: "project-1".to_string(),
                worktree_id: "worktree-a".to_string(),
            })
            .expect("select worktree a");
        let state = RuntimeState::load_from_support_dir(support_dir.clone());
        assert_eq!(
            state.worktrees.selected_worktree_id.as_deref(),
            Some("worktree-a")
        );
        assert_eq!(state.terminal_layout.active_terminal_id, "terminal-a");
        assert_eq!(state.terminal_layout.top_panes[0].terminal_id, "terminal-a");
        assert_eq!(state.terminal_layout.bottom_ratio, 0.18);

        let _ = fs::remove_dir_all(support_dir);
    }

    #[cfg(unix)]
    #[test]
    fn project_and_worktree_switch_runs_runtime_activation_layout_pty_ai_and_git_flow() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-runtime-switch-full-flow-{}",
            uuid::Uuid::new_v4()
        ));
        let project_a_dir = support_dir.join("project-a");
        let project_b_dir = support_dir.join("project-b");
        let worktree_a_dir = support_dir.join("worktree-a");
        let worktree_b_dir = support_dir.join("worktree-b");
        fs::create_dir_all(&project_a_dir).expect("create project a dir");
        fs::create_dir_all(&project_b_dir).expect("create project b dir");
        fs::create_dir_all(&worktree_a_dir).expect("create worktree a dir");
        fs::create_dir_all(&worktree_b_dir).expect("create worktree b dir");
        fs::write(
            support_dir.join("state.json"),
            json!({
                "projects": [
                    {
                        "id": "project-a",
                        "name": "Project A",
                        "path": project_a_dir.to_string_lossy()
                    },
                    {
                        "id": "project-b",
                        "name": "Project B",
                        "path": project_b_dir.to_string_lossy()
                    }
                ],
                "worktrees": [
                    {
                        "id": "worktree-a",
                        "projectId": "project-a",
                        "name": "Task A",
                        "branch": "task-a",
                        "path": worktree_a_dir.to_string_lossy(),
                        "status": "active",
                        "isDefault": false,
                        "createdAt": 1,
                        "updatedAt": 1
                    },
                    {
                        "id": "worktree-b",
                        "projectId": "project-a",
                        "name": "Task B",
                        "branch": "task-b",
                        "path": worktree_b_dir.to_string_lossy(),
                        "status": "active",
                        "isDefault": false,
                        "createdAt": 1,
                        "updatedAt": 1
                    }
                ],
                "worktreeTasks": [
                    {
                        "worktreeId": "worktree-a",
                        "title": "Task A",
                        "baseBranch": "main",
                        "baseCommit": null,
                        "status": "active",
                        "createdAt": 1,
                        "updatedAt": 1,
                        "startedAt": null,
                        "completedAt": null
                    },
                    {
                        "worktreeId": "worktree-b",
                        "title": "Task B",
                        "baseBranch": "main",
                        "baseCommit": null,
                        "status": "active",
                        "createdAt": 1,
                        "updatedAt": 1,
                        "startedAt": null,
                        "completedAt": null
                    }
                ],
                "selectedProjectId": "project-a",
                "selectedWorktreeIdByProject": {
                    "project-a": "worktree-a"
                }
            })
            .to_string(),
        )
        .expect("write state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));
        let layout_a_key = terminal_layout_storage_key("project-a", "worktree-a");
        let layout_b_key = terminal_layout_storage_key("project-a", "worktree-b");
        let terminal_a_top = format!("terminal-a-top-{}", uuid::Uuid::new_v4());
        let terminal_a_tab = format!("terminal-a-tab-{}", uuid::Uuid::new_v4());
        let terminal_b_top = format!("terminal-b-top-{}", uuid::Uuid::new_v4());
        let terminal_project_b = format!("terminal-project-b-{}", uuid::Uuid::new_v4());
        service
            .save_terminal_layout(
                &layout_a_key,
                vec![TerminalTabSummary {
                    label: "Task A Tab".to_string(),
                    terminal_id: terminal_a_tab.clone(),
                }],
                terminal_a_top.clone(),
                vec![TerminalPaneSummary {
                    title: "Task A Top".to_string(),
                    terminal_id: terminal_a_top.clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save task a layout");
        service
            .save_terminal_layout(
                &layout_b_key,
                Vec::new(),
                terminal_b_top.clone(),
                vec![TerminalPaneSummary {
                    title: "Task B Top".to_string(),
                    terminal_id: terminal_b_top.clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save task b layout");
        service
            .save_terminal_layout(
                &terminal_layout_storage_key("project-b", "project-b"),
                Vec::new(),
                terminal_project_b.clone(),
                vec![TerminalPaneSummary {
                    title: "Project B".to_string(),
                    terminal_id: terminal_project_b.clone(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save project b layout");

        let ready = service.app_runtime_ready(true, true);
        assert_eq!(
            ready.projects.selected_project_id.as_deref(),
            Some("project-a")
        );
        assert_eq!(
            ready
                .projects
                .selected_worktree_id_by_project
                .get("project-a")
                .map(String::as_str),
            Some("worktree-a")
        );
        assert_tracked_project_has_git_refresh(&service, "project-a");
        wait_for_ai_history_loading_event(
            &service,
            "worktree-a",
            &worktree_a_dir.to_string_lossy(),
        );
        let layout = service.reload_terminal_layout(Some(&layout_a_key));
        assert_eq!(layout.active_terminal_id, terminal_a_top);
        assert_eq!(layout.top_panes[0].terminal_id, terminal_a_top);
        assert_eq!(layout.tabs[0].terminal_id, terminal_a_tab);

        let terminal_manager = service.terminal_manager();
        let launch_context = TerminalLaunchContext {
            project_id: "worktree-a".to_string(),
            project_name: "Task A".to_string(),
            project_path: worktree_a_dir.clone(),
            support_dir: support_dir.clone(),
            runtime_root: support_dir.join("runtime-root"),
            terminal_id: Some(layout.active_terminal_id.clone()),
            slot_id: None,
            session_key: None,
            session_title: Some("Task A Top".to_string()),
            session_cwd: Some(worktree_a_dir.clone()),
            session_instance_id: None,
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
        };
        let mut config = launch_context.to_config();
        config.shell = Some("/bin/cat".to_string());
        config.cols = Some(80);
        config.rows = Some(24);
        let ensured = terminal_manager
            .ensure_session_with_context(config.clone(), Some(&launch_context))
            .expect("ensure task a terminal pty");
        assert_eq!(ensured, terminal_a_top);
        let (attached, rx) = terminal_manager
            .attach_or_create_with_context(config, Some(&launch_context), Arc::new(|_| {}))
            .expect("attach task a terminal pty");
        assert_eq!(attached.id(), terminal_a_top);
        attached
            .write(b"task-a-shared-pty\n")
            .expect("write task a terminal");
        assert!(
            recv_until_contains(&rx, "task-a-shared-pty", Duration::from_secs(2))
                .contains("task-a-shared-pty")
        );

        service
            .project_select_worktree(ProjectSelectWorktreeRequest {
                project_id: "project-a".to_string(),
                worktree_id: "worktree-b".to_string(),
            })
            .expect("select worktree b");
        assert_eq!(
            service
                .project_list()
                .selected_worktree_id_by_project
                .get("project-a")
                .map(String::as_str),
            Some("worktree-b")
        );
        assert_tracked_project_has_git_refresh(&service, "project-a");
        wait_for_ai_history_loading_event(
            &service,
            "worktree-b",
            &worktree_b_dir.to_string_lossy(),
        );
        let layout = service.reload_terminal_layout(Some(&layout_b_key));
        assert_eq!(layout.active_terminal_id, terminal_b_top);
        assert_eq!(layout.top_panes[0].terminal_id, terminal_b_top);

        service.select_project("project-b").expect("select project b");
        assert_eq!(
            service.project_list().selected_project_id.as_deref(),
            Some("project-b")
        );
        assert_tracked_project_has_git_refresh(&service, "project-b");
        wait_for_ai_history_loading_event(
            &service,
            "project-b",
            &project_b_dir.to_string_lossy(),
        );
        let layout = service.reload_terminal_layout(Some(&terminal_layout_storage_key(
            "project-b",
            "project-b",
        )));
        assert_eq!(layout.active_terminal_id, terminal_project_b);
        assert_eq!(layout.top_panes[0].terminal_id, terminal_project_b);

        let _ = terminal_manager.kill(&terminal_a_top);
        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_close_keeps_pet_baseline() {
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
        assert_eq!(
            pet.project_normalized_token_watermarks.get("project-1"),
            Some(&10)
        );
        assert_eq!(
            pet.project_normalized_token_watermarks.get("project-2"),
            Some(&20)
        );
        assert_eq!(pet.global_normalized_total_watermark, Some(30));

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn project_close_cleans_workspace_cache_for_root_and_worktrees() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-project-close-workspace-cache-{}",
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
                        "name": "Task",
                        "branch": "task",
                        "path": worktree_dir.to_string_lossy(),
                        "status": "active",
                        "isDefault": false,
                        "createdAt": 1,
                        "updatedAt": 1
                    }
                ],
                "worktreeTasks": [
                    {
                        "worktreeId": "worktree-1",
                        "title": "Task",
                        "baseBranch": "main",
                        "status": "active",
                        "createdAt": 1,
                        "updatedAt": 1
                    }
                ],
                "selectedProjectId": "project-1",
                "selectedWorktreeIdByProject": {
                    "project-1": "worktree-1"
                }
            })
            .to_string(),
        )
        .expect("write state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));
        service
            .save_terminal_layout(
                "project-1",
                Vec::new(),
                "terminal-1".to_string(),
                vec![TerminalPaneSummary {
                    title: "Shell".to_string(),
                    terminal_id: "terminal-1".to_string(),
                }],
                vec![1.0],
                0.24,
            )
            .expect("save project terminal layout");
        service
            .save_file_editor_layout(
                "worktree-1",
                vec![FileEditorTabSummary {
                    path: "src/main.rs".to_string(),
                    label: "main.rs".to_string(),
                    language: "rust".to_string(),
                }],
                Some("src/main.rs".to_string()),
            )
            .expect("save worktree file editor layout");
        let obsolete_cache =
            crate::persistent_cache::PersistentCacheStore::for_support_dir(support_dir.clone())
                .expect("obsolete cache");
        obsolete_cache
            .put_json(
                "file-tree-state",
                "worktree-1",
                &serde_json::json!({
                    "fileDirectory": "src",
                    "selectedFileEntry": "src/main.rs"
                }),
            )
            .expect("save obsolete file tree state");
        obsolete_cache
            .put_json(
                "git-ui-state",
                "worktree-1",
                &serde_json::json!({
                    "selectedGitFile": "src/main.rs"
                }),
            )
            .expect("save obsolete git ui state");

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
            .insert("worktree-1".to_string(), 20);
        fs::write(
            support_dir.join("pet-state.json"),
            serde_json::to_vec(&pet_snapshot).expect("encode pet"),
        )
        .expect("write pet state");

        service
            .project_close(ProjectCloseRequest {
                project_id: "project-1".to_string(),
            })
            .expect("close project");

        assert!(service.project_list().projects.is_empty());
        assert!(
            service
                .project_list()
                .selected_worktree_id_by_project
                .is_empty()
        );
        assert!(service.terminal_layout_record("project-1").is_none());
        assert!(service.reload_file_editor_layout(Some("worktree-1")).tabs.is_empty());
        assert_eq!(
            obsolete_cache
                .get_json::<serde_json::Value>("file-tree-state", "worktree-1")
                .expect("load obsolete file tree state"),
            None
        );
        assert_eq!(
            obsolete_cache
                .get_json::<serde_json::Value>("git-ui-state", "worktree-1")
                .expect("load obsolete git ui state"),
            None
        );
        let pet = service.pet_snapshot().expect("pet snapshot");
        assert_eq!(
            pet.project_normalized_token_watermarks.get("project-1"),
            Some(&10)
        );
        assert_eq!(
            pet.project_normalized_token_watermarks.get("worktree-1"),
            Some(&20)
        );
        assert_eq!(pet.global_normalized_total_watermark, Some(30));

        let _ = fs::remove_dir_all(support_dir);
    }

    #[test]
    fn indexed_pet_totals_are_filtered_to_active_project_workspaces() {
        let mut active = HashSet::new();
        active.insert("project-1".to_string());
        active.insert("worktree-1".to_string());

        let filtered = filter_active_indexed_project_totals(
            vec![
                crate::ai_usage_store::AIUsageProjectTotal {
                    project_id: "project-1".to_string(),
                    total_tokens: 10,
                },
                crate::ai_usage_store::AIUsageProjectTotal {
                    project_id: "removed-project".to_string(),
                    total_tokens: 9_999,
                },
                crate::ai_usage_store::AIUsageProjectTotal {
                    project_id: "worktree-1".to_string(),
                    total_tokens: 20,
                },
            ],
            &active,
        );

        assert_eq!(filtered.len(), 2);
        assert_eq!(filtered[0].project_id, "project-1");
        assert_eq!(filtered[0].total_tokens, 10);
        assert_eq!(filtered[1].project_id, "worktree-1");
        assert_eq!(filtered[1].total_tokens, 20);
    }

    #[test]
    fn pet_refresh_uses_runtime_support_dir_history_store() {
        let support_dir = std::env::temp_dir().join(format!(
            "codux-pet-runtime-history-store-{}",
            uuid::Uuid::new_v4()
        ));
        let _ = fs::remove_dir_all(&support_dir);
        let project_dir = support_dir.join("project");
        fs::create_dir_all(&project_dir).expect("create project dir");
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
                "selectedProjectId": "project-1"
            })
            .to_string(),
        )
        .expect("write state");

        let mut pet_snapshot = crate::pet::PetSnapshot {
            claimed_at: Some(1),
            species: "codux".to_string(),
            global_normalized_total_watermark: Some(100),
            total_normalized_tokens: 100,
            ..crate::pet::PetSnapshot::default()
        };
        pet_snapshot
            .project_normalized_token_watermarks
            .insert("project-1".to_string(), 100);
        fs::write(
            support_dir.join("pet-state.json"),
            serde_json::to_vec(&pet_snapshot).expect("encode pet state"),
        )
        .expect("write pet state");

        let service = RuntimeService::new(PathBuf::from(&support_dir));
        write_usage_bucket(
            &support_dir,
            &project_dir,
            "project-1",
            "Project",
            "before-claim",
            100,
            1.0,
        );
        write_usage_bucket(
            &support_dir,
            &project_dir,
            "project-1",
            "Project",
            "after-claim",
            30,
            10.0,
        );

        let summary = service
            .refresh_pet_from_indexed_history()
            .expect("refresh pet from indexed history");
        assert_eq!(summary.total_xp, 30);
        assert_eq!(summary.daily_xp, 30);
        let snapshot = service.pet_snapshot().expect("pet snapshot");
        assert_eq!(
            snapshot.project_normalized_token_watermarks.get("project-1"),
            Some(&130)
        );

        let summary = service
            .refresh_pet_from_indexed_history()
            .expect("refresh pet from indexed history again");
        assert_eq!(summary.total_xp, 30);
        assert_eq!(summary.daily_xp, 30);

        let second_dir = support_dir.join("second");
        fs::create_dir_all(&second_dir).expect("create second project dir");
        fs::write(
            support_dir.join("state.json"),
            json!({
                "projects": [
                    {
                        "id": "project-1",
                        "name": "Project",
                        "path": project_dir.to_string_lossy()
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
        .expect("write updated state");
        write_usage_bucket(
            &support_dir,
            &second_dir,
            "project-2",
            "Second",
            "existing-second",
            10_000,
            1.0,
        );

        let summary = service
            .refresh_pet_from_indexed_history()
            .expect("refresh pet after adding project");
        assert_eq!(summary.total_xp, 30);
        assert_eq!(summary.daily_xp, 30);

        service
            .project_close(ProjectCloseRequest {
                project_id: "project-1".to_string(),
            })
            .expect("close first project");
        let summary = service
            .refresh_pet_from_indexed_history()
            .expect("refresh pet after removing project");
        assert_eq!(summary.total_xp, 30);
        assert_eq!(summary.daily_xp, 30);
        let snapshot = service.pet_snapshot().expect("pet snapshot after remove");
        assert_eq!(
            snapshot.project_normalized_token_watermarks.get("project-1"),
            Some(&130)
        );

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

    fn write_usage_bucket(
        support_dir: &Path,
        project_dir: &Path,
        project_id: &str,
        project_name: &str,
        session_key: &str,
        total_tokens: i64,
        bucket_start: f64,
    ) {
        let store =
            crate::ai_usage_store::AIUsageStore::at_path(support_dir.join("ai-usage.sqlite3"));
        let conn = store.connect().expect("connect ai usage store");
        let project_path = project_dir.to_string_lossy().to_string();
        conn.execute(
            r#"
            INSERT INTO ai_history_file_session_link (
                source, file_path, project_path, session_key, external_session_id, project_id,
                project_name, session_title, first_seen_at, last_seen_at, last_model,
                active_duration_seconds
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            rusqlite::params![
                "codex",
                "session.jsonl",
                project_path,
                session_key,
                session_key,
                project_id,
                project_name,
                "Session",
                bucket_start,
                bucket_start + 1_800.0,
                "gpt-5",
                60_i64
            ],
        )
        .expect("insert session link");
        conn.execute(
            r#"
            INSERT INTO ai_history_file_usage_bucket (
                source, file_path, project_path, session_key, model, bucket_start, bucket_end,
                input_tokens, output_tokens, total_tokens, cached_input_tokens, request_count
            )
            VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12)
            "#,
            rusqlite::params![
                "codex",
                "session.jsonl",
                project_dir.to_string_lossy().to_string(),
                session_key,
                "gpt-5",
                bucket_start,
                bucket_start + 1_800.0,
                total_tokens / 2,
                total_tokens - (total_tokens / 2),
                total_tokens,
                0_i64,
                1_i64
            ],
        )
        .expect("insert usage bucket");
    }
}
