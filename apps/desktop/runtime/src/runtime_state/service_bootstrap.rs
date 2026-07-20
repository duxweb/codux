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
    ai_runtime: Arc<AIRuntimeBridge>,
    terminals: Arc<TerminalManager>,
) -> Arc<RemoteHostRuntime> {
    let current_sessions: Arc<dyn codux_runtime_core::ai_stats::RemoteAICurrentSessionProvider> =
        Arc::new(DesktopAICurrentSessionProvider {
            support_dir: support_dir.clone(),
            ai_runtime,
        });
    let runtime = Arc::new(
        RemoteHostRuntime::new_with_ai_history_current_sessions_and_terminals(
            support_dir,
            ai_history,
            current_sessions,
            terminals,
        ),
    );
    if let Err(error) = runtime.enable_agent_worktree_control() {
        crate::runtime_trace::runtime_trace(
            "agent-worktree",
            &format!("failed to start desktop control: {error}"),
        );
    }
    runtime
}

struct DesktopAICurrentSessionProvider {
    support_dir: PathBuf,
    ai_runtime: Arc<AIRuntimeBridge>,
}

impl codux_runtime_core::ai_stats::RemoteAICurrentSessionProvider
    for DesktopAICurrentSessionProvider
{
    fn current_sessions(&self, project_id: &str) -> Vec<codux_protocol::RemoteAICurrentSession> {
        let snapshot = self.ai_runtime.runtime_state_snapshot();
        let summary =
            AIRuntimeStateService::new(&self.support_dir).summary_from_runtime_snapshot(&snapshot);
        crate::ai_runtime_state::remote_current_sessions_from_runtime_state(&summary, project_id)
    }
}

impl RuntimeService {
    pub fn new(support_dir: PathBuf) -> Self {
        codux_ai_history::trace::set_trace_sink(crate::runtime_trace::runtime_trace);
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
            Arc::clone(&ai_runtime),
            terminal_manager,
        );
        let hosted_runtime_events = Arc::new(Mutex::new(VecDeque::new()));
        let hosted_terminal_layout_generation =
            Arc::new(std::sync::atomic::AtomicU64::new(0));
        let wsl_runtimes = {
            let support_dir = support_dir.clone();
            let events = Arc::clone(&hosted_runtime_events);
            let generation = Arc::clone(&hosted_terminal_layout_generation);
            crate::wsl::WslRuntimeManager::with_event_sink(Arc::new(move |event| {
                let (project_id, project_path) = match event {
                    crate::wsl::WslRuntimeEvent::AgentWorktreeCreated(created) => {
                        let layout_key = crate::terminal_layout::terminal_layout_storage_key(
                            &created.project_id,
                            &created.worktree_id,
                        );
                        if let Err(error) = TerminalLayoutService::new(support_dir.clone())
                            .ensure_terminal(&layout_key, &created.terminal_id, &created.title)
                        {
                            crate::runtime_trace::runtime_trace(
                                "agent-worktree",
                                &format!("failed to persist WSL terminal layout: {error}"),
                            );
                            return;
                        }
                        (created.project_id, created.project_path)
                    }
                    crate::wsl::WslRuntimeEvent::AgentWorktreeChanged(changed) => {
                        if changed.removed {
                            let layout_key = crate::terminal_layout::terminal_layout_storage_key(
                                &changed.project_id,
                                &changed.worktree_id,
                            );
                            if let Err(error) = TerminalLayoutService::new(support_dir.clone())
                                .delete(&layout_key)
                            {
                                crate::runtime_trace::runtime_trace(
                                    "agent-worktree",
                                    &format!("failed to delete WSL terminal layout: {error}"),
                                );
                            }
                        }
                        (changed.project_id, changed.project_path)
                    }
                };
                let Ok(mut events) = events.lock() else {
                    return;
                };
                events.push_back(RemoteHostEvent::WorktreesChanged {
                    project_id,
                    project_path,
                });
                events.push_back(RemoteHostEvent::TerminalLayoutChanged(
                    crate::remote::RemoteTerminalLayoutChanged {
                        generation: generation
                            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                            + 1,
                    },
                ));
            }))
        };
        Self {
            support_dir: support_dir.clone(),
            ai_history_indexer,
            project_activity,
            ai_runtime,
            file_watch_manager: Arc::new(FileWatchManager::default()),
            git_watch_manager: Arc::new(git::GitWatchManager::default()),
            file_watch_events: Arc::new(Mutex::new(VecDeque::new())),
            active_project_watches: Arc::new(Mutex::new(ActiveProjectWatches::default())),
            project_watch_registration: Arc::new(Mutex::new(())),
            ai_history_activation_keys: Arc::new(Mutex::new(HashSet::new())),
            hosted_ai_history_events: Arc::new(Mutex::new(VecDeque::new())),
            hosted_runtime_events,
            hosted_terminal_layout_generation,
            git_cancels: Arc::new(Mutex::new(HashMap::new())),
            power_manager: shared_power_manager(),
            remote_host,
            remote_controllers: Arc::new(crate::remote::RemoteControllerManager::new(support_dir)),
            wsl_runtimes: Arc::new(wsl_runtimes),
            host_browser_proxy: Arc::new(crate::host_browser::HostBrowserProxy::new()),
        }
    }
}
