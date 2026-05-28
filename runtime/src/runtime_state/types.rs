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
    pub ai_history: AIHistorySummary,
    pub ai_session_detail: Option<AISessionDetail>,
    pub memory: MemorySummary,
    pub memory_manager: MemoryManagerSnapshot,
    pub notifications: NotificationSummary,
    pub ssh: SSHSummary,
    pub worktrees: WorktreeSummary,
    pub terminal_layout: TerminalLayoutSummary,
    pub terminal_runtime: TerminalRuntimeSummary,
    pub update: UpdateSummary,
    pub runtime_activity: RuntimeActivitySummary,
    pub runtime_events: RuntimeEventSummary,
    pub ai_runtime_state: AIRuntimeStateSummary,
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
    active_file_watch_path: Arc<Mutex<Option<String>>>,
    git_cancels: Arc<Mutex<HashMap<String, git::GitCancelToken>>>,
    power_manager: Arc<PowerManager>,
    remote_host: Arc<RemoteHostRuntime>,
}

#[derive(Clone, Debug)]
pub struct AIRuntimeDrainResult {
    pub events: Vec<AIRuntimeSupervisorEvent>,
    pub memory: Vec<MemoryEnqueueResult>,
}

#[derive(Clone, Debug)]
pub struct FileEntry {
    pub name: String,
    pub relative_path: String,
    pub kind: FileKind,
    pub size: u64,
}

#[derive(Clone, Debug)]
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
}
