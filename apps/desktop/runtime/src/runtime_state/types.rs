#[derive(Clone, Debug)]
pub struct RuntimeState {
    pub support_dir: PathBuf,
    pub settings: SettingsSummary,
    pub projects: Vec<ProjectInfo>,
    pub selected_project: Option<ProjectInfo>,
    pub git: git::GitSummary,
    pub git_review: git::GitReviewSummary,
    pub files: Vec<FileEntry>,
    pub ai_global_history: AIGlobalHistorySummary,
    pub daily_level: AIHistoryDailyLevelView,
    pub ai_history: AIHistorySummary,
    pub ai_history_stats: AIHistoryStatsView,
    /// Fingerprint of the inputs to the history-derived geometry in
    /// `ai_history_stats` (buckets/heatmap/breakdowns). Lets a refresh reuse the
    /// existing geometry and recompute only the live current-session rows when
    /// the indexed history (and the local day / cache mode) are unchanged.
    pub ai_history_stats_fingerprint: u64,
    pub ai_session_detail: Option<AISessionDetail>,
    pub memory: MemorySummary,
    pub memory_manager: MemoryManagerSnapshot,
    pub notifications: NotificationSummary,
    pub ssh: SSHSummary,
    pub db: DBSummary,
    pub worktrees: WorktreeSummary,
    pub terminal_layout: TerminalLayoutSummary,
    pub terminal_runtime: TerminalRuntimeSummary,
    pub update: UpdateSummary,
    pub runtime_activity: RuntimeActivitySummary,
    pub runtime_events: RuntimeEventSummary,
    pub ai_runtime_state: AIRuntimeStateSummary,
    pub remote_ai_current_sessions: Vec<AIHistoryCurrentSessionView>,
    pub remote: RemoteSummary,
    pub pet: PetSummary,
    pub power: PowerSummary,
    pub performance: PerformanceSummary,
    pub tool_permissions: ToolPermissionsSummary,
}

#[derive(Clone, Debug)]
pub struct AppRuntimeReadySnapshot {
    pub projects: ProjectListSnapshot,
    pub terminal_layouts: TerminalLayoutsSnapshot,
    pub remote: RemoteSummary,
    pub ai_runtime_state: AIRuntimeStateSnapshot,
    pub project_activity: ProjectActivitySnapshot,
    pub window_state: RuntimeWindowStateSnapshot,
}

#[derive(Clone, Debug)]
pub struct RuntimeWindowStateSnapshot {
    pub project_activity: ProjectActivitySnapshot,
    pub shows_dock_badge: bool,
    pub attention_count: usize,
    pub dock_badge_count: Option<i64>,
}

#[derive(Clone, Debug)]
pub struct ProjectInfo {
    pub id: String,
    pub name: String,
    pub path: String,
    pub exists: bool,
    pub badge: String,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
    pub git_default_push_remote_name: Option<String>,
    pub runtime_target: ProjectRuntimeTarget,
}

impl ProjectInfo {
    pub fn remote_device_id(&self) -> Option<&str> {
        self.runtime_target.remote_device_id()
    }

    pub fn is_remote(&self) -> bool {
        self.remote_device_id().is_some()
    }
}

#[derive(Clone)]
pub struct RuntimeService {
    support_dir: PathBuf,
    ai_history_indexer: AIHistoryIndexer,
    project_activity: Arc<ProjectActivityCoordinator>,
    ai_runtime: Arc<AIRuntimeBridge>,
    file_watch_manager: Arc<FileWatchManager>,
    git_watch_manager: Arc<git::GitWatchManager>,
    file_watch_events: Arc<Mutex<VecDeque<FileChangeEvent>>>,
    active_project_watches: Arc<Mutex<ActiveProjectWatches>>,
    project_watch_registration: Arc<Mutex<()>>,
    ai_history_activation_keys: Arc<Mutex<HashSet<String>>>,
    hosted_ai_history_events: Arc<Mutex<VecDeque<AIHistoryEvent>>>,
    git_cancels: Arc<Mutex<HashMap<String, git::GitCancelToken>>>,
    power_manager: Arc<PowerManager>,
    remote_host: Arc<RemoteHostRuntime>,
    remote_controllers: Arc<crate::remote::RemoteControllerManager>,
    wsl_runtimes: Arc<crate::wsl::WslRuntimeManager>,
    host_browser_proxy: Arc<crate::host_browser::HostBrowserProxy>,
}

#[derive(Default)]
struct ActiveProjectWatches {
    generation: u64,
    file_path: Option<String>,
    git_path: Option<String>,
    pending_file_unwatches: Vec<String>,
    pending_git_unwatches: Vec<String>,
}

impl RuntimeService {
    pub fn terminal_manager(&self) -> Arc<TerminalManager> {
        self.remote_host.terminal_manager()
    }

    pub fn ai_runtime_bridge(&self) -> Arc<AIRuntimeBridge> {
        Arc::clone(&self.ai_runtime)
    }

    /// Register the desktop theme's OSC 10/11 payloads as the host-side seed
    /// fallback for remote terminal spawns that carry no viewer colors.
    pub fn set_remote_terminal_osc_colors(&self, foreground: String, background: String) {
        self.remote_host
            .set_terminal_osc_colors(foreground, background);
    }
}

#[derive(Clone, Debug)]
pub struct AIRuntimeDrainResult {
    pub events: Vec<AIRuntimeSupervisorEvent>,
    pub memory: Vec<MemoryEnqueueResult>,
}

#[derive(Clone, Debug, Default)]
pub struct AIHistoryDrainResult {
    pub events: Vec<AIHistoryEvent>,
    pub pet: Option<PetSummary>,
    pub pet_snapshot: Option<PetSnapshot>,
    pub pet_error: Option<String>,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub relative_path: String,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Clone, Debug, serde::Serialize, serde::Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub enum FileKind {
    Directory,
    File,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct StateFile {
    #[serde(default)]
    projects: Vec<ProjectRecord>,
    selected_project_id: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProjectRecord {
    id: String,
    name: String,
    path: String,
    #[serde(default)]
    badge_text: Option<String>,
    #[serde(default)]
    badge_symbol: Option<String>,
    #[serde(default)]
    badge_color_hex: Option<String>,
    #[serde(default)]
    git_default_push_remote_name: Option<String>,
    #[serde(default)]
    runtime_target: Option<ProjectRuntimeTarget>,
    #[serde(default)]
    host_device_id: Option<String>,
}

impl ProjectRecord {
    fn resolved_runtime_target(&self) -> ProjectRuntimeTarget {
        self.runtime_target.clone().unwrap_or_else(|| {
            self.host_device_id
                .clone()
                .map(|device_id| ProjectRuntimeTarget::Remote { device_id })
                .unwrap_or_default()
        })
    }
}
