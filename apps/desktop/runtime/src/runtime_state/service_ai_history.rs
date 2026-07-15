impl RuntimeService {
    pub fn indexed_project_ai_history_summary(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        if let Some(runtime) = self.hosted_ai_history_runtime(&project.path)? {
            return runtime.state(&project, false);
        }
        self.ai_history_indexer.project_summary(project)
    }

    pub fn refresh_indexed_project_ai_history(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<(), String> {
        let target = ProjectStore::new(self.support_dir.clone())
            .runtime_target_for_workspace_path(&project.path)?;
        if target.is_hosted() {
            let service = self.clone();
            std::thread::Builder::new()
                .name("codux-hosted-ai-history-refresh".to_string())
                .spawn(move || {
                    let state = service
                        .hosted_ai_history_runtime(&project.path)
                        .and_then(|runtime| {
                            runtime
                                .ok_or_else(|| "Hosted AI history runtime is unavailable".to_string())
                        })
                        .and_then(|runtime| runtime.refresh_state(&project))
                        .unwrap_or_else(|error| AIHistoryProjectState {
                            project_id: project.id.clone(),
                            project_name: project.name.clone(),
                            project_path: project.path.clone(),
                            snapshot: None,
                            is_loading: false,
                            queued: false,
                            progress: None,
                            detail: "failed".to_string(),
                            error: Some(error),
                            version: 0,
                        });
                    if let Ok(mut events) = service.hosted_ai_history_events.lock() {
                        events.push_back(AIHistoryEvent::ProjectState { state });
                    }
                })
                .map_err(|error| error.to_string())?;
            return Ok(());
        }
        self.ai_history_indexer.refresh_project(project)
    }

    pub fn active_ai_history_index_count(&self) -> usize {
        self.ai_history_indexer.active_project_count()
    }

    pub fn indexed_project_ai_history_state(
        &self,
        project: AIHistoryProjectRequest,
    ) -> Result<AIHistoryProjectState, String> {
        if let Some(runtime) = self.hosted_ai_history_runtime(&project.path)? {
            return runtime.state(&project, false);
        }
        self.ai_history_indexer.project_state(project)
    }

    pub fn indexed_global_ai_history_summary(
        &self,
    ) -> Result<AIGlobalHistorySnapshot, String> {
        self.ai_history_indexer
            .global_summary(self.ai_history_workspace_requests())
    }

    pub fn indexed_global_ai_history_state(
        &self,
    ) -> Result<Option<AIGlobalHistorySnapshot>, String> {
        self.ai_history_indexer
            .global_state(self.ai_history_workspace_requests())
    }

    pub fn refresh_indexed_global_ai_history(&self) -> Result<(), String> {
        self.ai_history_indexer
            .refresh_global(self.ai_history_workspace_requests())
    }

    pub fn rename_indexed_ai_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
        title: String,
    ) -> Result<AIHistoryProjectState, String> {
        if let Some(runtime) = self.hosted_ai_history_runtime(&project.path)? {
            return runtime.session_state(&project, "indexedRename", session_id, Some(title));
        }
        self.ai_history_indexer
            .rename_session(project, session_id, title)
    }

    pub fn remove_indexed_ai_session(
        &self,
        project: AIHistoryProjectRequest,
        session_id: String,
    ) -> Result<AIHistoryProjectState, String> {
        if let Some(runtime) = self.hosted_ai_history_runtime(&project.path)? {
            return runtime.session_state(&project, "indexedRemove", session_id, None);
        }
        self.ai_history_indexer.remove_session(project, session_id)
    }

    pub fn drain_ai_history_events(&self) -> AIHistoryDrainResult {
        let mut events = self.ai_history_indexer.drain_events();
        if let Ok(mut hosted_events) = self.hosted_ai_history_events.lock() {
            events.extend(hosted_events.drain(..));
        }
        if events
            .iter()
            .any(|event| matches!(event, AIHistoryEvent::Project { .. }))
        {
            let _ = self.refresh_indexed_global_ai_history();
        }
        let should_refresh_pet = events.iter().any(|event| {
            matches!(
                event,
                AIHistoryEvent::Global { .. }
                    | AIHistoryEvent::Project { .. }
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

    pub(super) fn ai_history_workspace_requests(&self) -> Vec<AIHistoryProjectRequest> {
        ai_history_workspace_requests_from_support_dir(&self.support_dir)
    }
}

fn ai_history_workspace_requests_from_support_dir(
    support_dir: &Path,
) -> Vec<AIHistoryProjectRequest> {
    let snapshot = ProjectStore::new(support_dir.to_path_buf()).snapshot();
    let mut requests = snapshot
        .projects
        .iter()
        .filter(|project| project.runtime_target.is_local())
        .map(|project| AIHistoryProjectRequest {
            id: project.id.clone(),
            name: project.name.clone(),
            path: project.path.clone(),
        })
        .collect::<Vec<_>>();
    requests.extend(snapshot.worktrees.iter().filter_map(|worktree| {
        let project = snapshot
            .projects
            .iter()
            .find(|project| project.id == worktree.project_id)?;
        if !project.runtime_target.is_local() {
            return None;
        }
        Some(AIHistoryProjectRequest {
            id: worktree.id.clone(),
            name: worktree.name.clone(),
            path: worktree.path.clone(),
        })
    }));
    let mut paths = HashSet::new();
    requests.retain(|request| {
        normalized_history_path(&request.path).is_some_and(|path| paths.insert(path))
    });
    requests
}
