use crate::paths::app_support_dir;
use crate::worktree::{ProjectWorktreeSnapshot, WorktreeSnapshot, WorktreeTaskSnapshot};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRecord {
    pub id: String,
    pub name: String,
    pub path: String,
    pub badge_text: Option<String>,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
    pub git_default_push_remote_name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct AppSnapshot {
    #[serde(default)]
    pub projects: Vec<ProjectRecord>,
    #[serde(default)]
    pub worktrees: Vec<ProjectWorktreeRecord>,
    #[serde(default)]
    pub worktree_tasks: Vec<WorktreeTaskRecord>,
    #[serde(default)]
    pub terminal_layouts: HashMap<String, TerminalLayoutRecord>,
    pub selected_project_id: Option<String>,
    #[serde(default)]
    pub selected_worktree_id_by_project: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutRecord {
    #[serde(default)]
    pub tabs: Vec<TerminalBottomTabRecord>,
    #[serde(default)]
    pub active_tab_id: String,
    #[serde(default)]
    pub top_panes: Vec<TerminalTopPaneRecord>,
    #[serde(default)]
    pub top_ratios: Vec<f64>,
    #[serde(default = "default_bottom_ratio")]
    pub bottom_ratio: f64,
    #[serde(default)]
    pub active_slot_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalBottomTabRecord {
    pub id: String,
    pub label: String,
    pub terminal_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct TerminalTopPaneRecord {
    pub id: String,
    pub title: String,
    pub terminal_id: String,
    #[serde(default, skip_serializing_if = "is_false")]
    pub detached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectWorktreeRecord {
    pub id: String,
    pub project_id: String,
    pub name: String,
    pub branch: String,
    pub path: String,
    pub status: String,
    pub is_default: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct WorktreeTaskRecord {
    pub worktree_id: String,
    pub title: String,
    pub base_branch: String,
    pub base_commit: Option<String>,
    pub status: String,
    pub created_at: i64,
    pub updated_at: i64,
    pub started_at: Option<i64>,
    pub completed_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectListSnapshot {
    pub projects: Vec<ProjectSummary>,
    pub selected_project_id: Option<String>,
    pub selected_worktree_id_by_project: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalLayoutsSnapshot {
    pub layouts: HashMap<String, TerminalLayoutRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSummary {
    pub id: String,
    pub name: String,
    pub path: String,
    pub badge: String,
    pub status: String,
    pub branch: String,
    pub changes: usize,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
    pub git_default_push_remote_name: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ProjectWorkspaceRecord {
    pub id: String,
    pub root_project_id: String,
    pub root_project_name: String,
    pub root_project_path: String,
    pub workspace_path: String,
    pub git_default_push_remote_name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCreateRequest {
    pub name: String,
    pub path: String,
    pub badge_text: Option<String>,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdateRequest {
    pub project_id: String,
    pub name: String,
    pub path: String,
    pub badge_text: Option<String>,
    pub badge_symbol: Option<String>,
    pub badge_color_hex: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectCloseRequest {
    pub project_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectSelectWorktreeRequest {
    pub project_id: String,
    pub worktree_id: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectReorderRequest {
    pub project_ids: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDefaultPushRemoteRequest {
    pub project_id: String,
    pub remote_name: Option<String>,
}

pub struct ProjectStore {
    snapshot: Mutex<AppSnapshot>,
    state_file: PathBuf,
}

impl ProjectStore {
    pub fn load_or_default() -> Self {
        let state_file = state_file_path();
        let snapshot = load_snapshot(&state_file).unwrap_or_else(default_snapshot);
        let snapshot = sanitize_snapshot(snapshot);
        let store = Self {
            snapshot: Mutex::new(snapshot),
            state_file,
        };
        let _ = store.save();
        store
    }

    pub fn list_snapshot(&self) -> ProjectListSnapshot {
        let snapshot = self
            .snapshot
            .lock()
            .map(|value| value.clone())
            .unwrap_or_default();
        let selected_project_id = snapshot
            .selected_project_id
            .clone()
            .filter(|id| snapshot.projects.iter().any(|project| &project.id == id))
            .or_else(|| snapshot.projects.first().map(|project| project.id.clone()));
        ProjectListSnapshot {
            projects: snapshot
                .projects
                .iter()
                .map(project_summary)
                .collect::<Vec<_>>(),
            selected_project_id,
            selected_worktree_id_by_project: snapshot.selected_worktree_id_by_project,
        }
    }

    pub fn projects_snapshot(&self) -> Vec<ProjectRecord> {
        self.snapshot
            .lock()
            .map(|value| value.projects.clone())
            .unwrap_or_default()
    }

    pub fn project_workspaces_snapshot(&self) -> Vec<ProjectWorkspaceRecord> {
        let Ok(snapshot) = self.snapshot.lock() else {
            return Vec::new();
        };
        let mut rows = Vec::new();
        for project in &snapshot.projects {
            rows.push(ProjectWorkspaceRecord {
                id: project.id.clone(),
                root_project_id: project.id.clone(),
                root_project_name: project.name.clone(),
                root_project_path: project.path.clone(),
                workspace_path: project.path.clone(),
                git_default_push_remote_name: project.git_default_push_remote_name.clone(),
            });
            for worktree in snapshot
                .worktrees
                .iter()
                .filter(|worktree| worktree.project_id == project.id)
            {
                rows.push(ProjectWorkspaceRecord {
                    id: worktree.id.clone(),
                    root_project_id: project.id.clone(),
                    root_project_name: project.name.clone(),
                    root_project_path: project.path.clone(),
                    workspace_path: worktree.path.clone(),
                    git_default_push_remote_name: project.git_default_push_remote_name.clone(),
                });
            }
        }
        rows
    }

    pub fn workspace_summary_by_path(&self, path: &str) -> Option<ProjectSummary> {
        let normalized = normalize_path(path);
        let snapshot = self.snapshot.lock().ok()?;
        snapshot
            .worktrees
            .iter()
            .find(|worktree| normalize_path(&worktree.path) == normalized)
            .map(worktree_summary)
            .or_else(|| {
                snapshot
                    .projects
                    .iter()
                    .find(|project| normalize_path(&project.path) == normalized)
                    .map(project_summary)
            })
    }

    pub fn root_project_summary_for_workspace_id(&self, id: &str) -> Option<ProjectSummary> {
        let snapshot = self.snapshot.lock().ok()?;
        if let Some(project) = snapshot.projects.iter().find(|project| project.id == id) {
            return Some(project_summary(project));
        }
        let worktree = snapshot
            .worktrees
            .iter()
            .find(|worktree| worktree.id == id)?;
        snapshot
            .projects
            .iter()
            .find(|project| project.id == worktree.project_id)
            .map(project_summary)
    }

    pub fn create_project(
        &self,
        request: ProjectCreateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let path = normalize_path(&request.path);
        if path.trim().is_empty() {
            return Err("Project path cannot be empty.".to_string());
        }
        if !Path::new(&path).exists() {
            return Err(format!("Project path does not exist: {path}"));
        }
        let name = normalized_string(&request.name).unwrap_or_else(|| {
            Path::new(&path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Project")
                .to_string()
        });
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        if let Some(existing) = snapshot
            .projects
            .iter()
            .find(|project| normalize_path(&project.path) == path)
        {
            snapshot.selected_project_id = Some(existing.id.clone());
            drop(snapshot);
            self.save()?;
            return Ok(self.list_snapshot());
        }
        let record = ProjectRecord {
            id: project_uuid(&name, &path),
            name,
            path,
            badge_text: request
                .badge_text
                .and_then(|value| normalized_string(&value)),
            badge_symbol: request
                .badge_symbol
                .and_then(|value| normalized_string(&value)),
            badge_color_hex: request
                .badge_color_hex
                .and_then(|value| normalized_string(&value)),
            git_default_push_remote_name: None,
        };
        snapshot.selected_project_id = Some(record.id.clone());
        snapshot.projects.push(record);
        drop(snapshot);
        self.save()?;
        Ok(self.list_snapshot())
    }

    pub fn select_project(&self, project_id: String) -> Result<(), String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        if snapshot
            .projects
            .iter()
            .any(|project| project.id == project_id)
        {
            snapshot.selected_project_id = Some(project_id);
            drop(snapshot);
            self.save()
        } else {
            Err("Project not found.".to_string())
        }
    }

    pub fn reorder_projects(
        &self,
        request: ProjectReorderRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        let ordered_project_ids = request.project_ids;
        let project_ids = ordered_project_ids.iter().cloned().collect::<HashSet<_>>();
        if ordered_project_ids.len() != snapshot.projects.len()
            || project_ids.len() != snapshot.projects.len()
            || snapshot
                .projects
                .iter()
                .any(|project| !project_ids.contains(&project.id))
        {
            return Err("Project order does not match current projects.".to_string());
        }
        let mut by_id = snapshot
            .projects
            .drain(..)
            .map(|project| (project.id.clone(), project))
            .collect::<HashMap<_, _>>();
        snapshot.projects = ordered_project_ids
            .iter()
            .filter_map(|id| by_id.remove(id))
            .collect::<Vec<_>>();
        drop(snapshot);
        self.save()?;
        Ok(self.list_snapshot())
    }

    pub fn close_project(&self, project_id: String) -> Result<ProjectListSnapshot, String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        let index = snapshot
            .projects
            .iter()
            .position(|project| project.id == project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        snapshot.projects.remove(index);
        prune_project_state(&mut snapshot, &project_id);
        snapshot.selected_project_id = select_project_after_removal(&snapshot, index);
        drop(snapshot);
        self.save()?;
        Ok(self.list_snapshot())
    }

    pub fn update_project(
        &self,
        request: ProjectUpdateRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let path = normalize_path(&request.path);
        if path.trim().is_empty() {
            return Err("Project path cannot be empty.".to_string());
        }
        if !Path::new(&path).exists() {
            return Err(format!("Project path does not exist: {path}"));
        }
        let name = normalized_string(&request.name).unwrap_or_else(|| {
            Path::new(&path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Project")
                .to_string()
        });
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        let project = snapshot
            .projects
            .iter_mut()
            .find(|project| project.id == request.project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        project.name = name.clone();
        project.path = path.clone();
        project.badge_text = request
            .badge_text
            .and_then(|value| normalized_string(&value));
        project.badge_symbol = request
            .badge_symbol
            .and_then(|value| normalized_string(&value));
        project.badge_color_hex = request
            .badge_color_hex
            .and_then(|value| normalized_string(&value));
        for worktree in snapshot
            .worktrees
            .iter_mut()
            .filter(|worktree| worktree.project_id == request.project_id && worktree.is_default)
        {
            worktree.name = name.clone();
            worktree.path = path.clone();
            worktree.updated_at = chrono::Utc::now().timestamp_millis();
        }
        drop(snapshot);
        self.save()?;
        Ok(self.list_snapshot())
    }

    pub fn close_all_projects(&self) -> Result<ProjectListSnapshot, String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        snapshot.projects.clear();
        snapshot.worktrees.clear();
        snapshot.worktree_tasks.clear();
        snapshot.terminal_layouts.clear();
        snapshot.selected_project_id = None;
        snapshot.selected_worktree_id_by_project.clear();
        drop(snapshot);
        self.save()?;
        Ok(self.list_snapshot())
    }

    pub fn select_worktree(&self, request: ProjectSelectWorktreeRequest) -> Result<(), String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        if snapshot
            .projects
            .iter()
            .any(|project| project.id == request.project_id)
        {
            snapshot
                .selected_worktree_id_by_project
                .insert(request.project_id, request.worktree_id);
            drop(snapshot);
            self.save()
        } else {
            Err("Project not found.".to_string())
        }
    }

    pub fn set_default_push_remote(
        &self,
        request: ProjectDefaultPushRemoteRequest,
    ) -> Result<ProjectListSnapshot, String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        let project = snapshot
            .projects
            .iter_mut()
            .find(|project| project.id == request.project_id)
            .ok_or_else(|| "Project not found.".to_string())?;
        project.git_default_push_remote_name = request
            .remote_name
            .and_then(|value| normalized_string(&value));
        drop(snapshot);
        self.save()?;
        Ok(self.list_snapshot())
    }

    pub fn merge_worktree_snapshot(
        &self,
        incoming: WorktreeSnapshot,
    ) -> Result<WorktreeSnapshot, String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        let result = merge_worktree_state(&mut snapshot, incoming)?;
        drop(snapshot);
        self.save()?;
        Ok(result)
    }

    pub fn terminal_layout(&self, project_id: &str) -> Option<TerminalLayoutRecord> {
        self.snapshot
            .lock()
            .ok()
            .and_then(|snapshot| snapshot.terminal_layouts.get(project_id).cloned())
    }

    pub fn terminal_layouts_snapshot(&self) -> TerminalLayoutsSnapshot {
        let layouts = self
            .snapshot
            .lock()
            .map(|snapshot| snapshot.terminal_layouts.clone())
            .unwrap_or_default();
        TerminalLayoutsSnapshot { layouts }
    }

    pub fn save_terminal_layout(
        &self,
        project_id: String,
        layout: TerminalLayoutRecord,
    ) -> Result<TerminalLayoutRecord, String> {
        let mut snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?;
        if !is_known_workspace_id(&snapshot, &project_id) {
            return Err("Project workspace not found.".to_string());
        }
        let layout = sanitize_terminal_layout(layout)
            .ok_or_else(|| "Terminal layout is empty.".to_string())?;
        snapshot.terminal_layouts.insert(project_id, layout.clone());
        drop(snapshot);
        self.save()?;
        Ok(layout)
    }

    fn save(&self) -> Result<(), String> {
        let snapshot = self
            .snapshot
            .lock()
            .map_err(|_| "Project store lock poisoned.".to_string())?
            .clone();
        if let Some(parent) = self.state_file.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let data = serde_json::to_vec_pretty(&snapshot).map_err(|error| error.to_string())?;
        fs::write(&self.state_file, data).map_err(|error| error.to_string())
    }
}

fn load_snapshot(path: &Path) -> Option<AppSnapshot> {
    let data = fs::read(path).ok()?;
    if data.is_empty() {
        return None;
    }
    serde_json::from_slice(&data).ok()
}

fn sanitize_snapshot(snapshot: AppSnapshot) -> AppSnapshot {
    let mut seen_ids = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut projects = Vec::new();
    for mut project in snapshot.projects {
        project.path = normalize_path(&project.path);
        if project.path.trim().is_empty()
            || !seen_ids.insert(project.id.clone())
            || !seen_paths.insert(project.path.clone())
        {
            continue;
        }
        if project.name.trim().is_empty() {
            project.name = Path::new(&project.path)
                .file_name()
                .and_then(|value| value.to_str())
                .unwrap_or("Project")
                .to_string();
        }
        projects.push(project);
    }
    let selected_project_id = snapshot
        .selected_project_id
        .filter(|id| projects.iter().any(|project| &project.id == id))
        .or_else(|| projects.first().map(|project| project.id.clone()));
    if projects.is_empty() {
        return AppSnapshot {
            projects,
            worktrees: Vec::new(),
            worktree_tasks: Vec::new(),
            terminal_layouts: HashMap::new(),
            selected_project_id,
            selected_worktree_id_by_project: HashMap::new(),
        };
    }
    let project_ids = projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<HashSet<_>>();
    let worktrees = sanitize_worktrees(snapshot.worktrees, &projects);
    let worktree_ids = worktrees
        .iter()
        .map(|worktree| worktree.id.clone())
        .collect::<HashSet<_>>();
    let worktree_tasks = sanitize_worktree_tasks(snapshot.worktree_tasks, &worktree_ids);
    let selected_worktree_id_by_project = snapshot
        .selected_worktree_id_by_project
        .into_iter()
        .filter(|(project_id, worktree_id)| {
            project_ids.contains(project_id) && worktree_ids.contains(worktree_id)
        })
        .collect();
    let mut known_workspace_ids = project_ids;
    known_workspace_ids.extend(worktree_ids);
    let terminal_layouts = snapshot
        .terminal_layouts
        .into_iter()
        .filter_map(|(project_id, layout)| {
            if known_workspace_ids.contains(&project_id) {
                sanitize_terminal_layout(layout).map(|layout| (project_id, layout))
            } else {
                None
            }
        })
        .collect();
    AppSnapshot {
        projects,
        worktrees,
        worktree_tasks,
        terminal_layouts,
        selected_project_id,
        selected_worktree_id_by_project,
    }
}

fn default_snapshot() -> AppSnapshot {
    AppSnapshot::default()
}

fn sanitize_terminal_layout(layout: TerminalLayoutRecord) -> Option<TerminalLayoutRecord> {
    let tabs = sanitize_bottom_tabs(layout.tabs);
    let (top_panes, top_ratios) =
        sanitize_top_pane_ratio_entries(layout.top_panes, layout.top_ratios);
    if tabs.is_empty() && top_panes.is_empty() {
        return None;
    }
    let active_tab_id = if tabs.iter().any(|tab| tab.id == layout.active_tab_id) {
        layout.active_tab_id
    } else {
        tabs.first().map(|tab| tab.id.clone()).unwrap_or_default()
    };
    let active_slot_id = if top_panes
        .iter()
        .any(|pane| pane.id == layout.active_slot_id)
        || tabs.iter().any(|tab| tab.id == layout.active_slot_id)
    {
        layout.active_slot_id
    } else {
        top_panes
            .first()
            .map(|pane| pane.id.clone())
            .or_else(|| tabs.first().map(|tab| tab.id.clone()))
            .unwrap_or_default()
    };

    Some(TerminalLayoutRecord {
        tabs,
        active_tab_id,
        top_panes,
        top_ratios,
        bottom_ratio: clamp_ratio(layout.bottom_ratio, 0.18, 0.72, default_bottom_ratio()),
        active_slot_id,
    })
}

fn sanitize_bottom_tabs(tabs: Vec<TerminalBottomTabRecord>) -> Vec<TerminalBottomTabRecord> {
    let mut seen = HashSet::new();
    let mut next = tabs
        .into_iter()
        .filter_map(|tab| {
            let id = normalized_string(&tab.id)?;
            if !seen.insert(id.clone()) {
                return None;
            }
            Some(TerminalBottomTabRecord {
                id,
                label: normalized_string(&tab.label).unwrap_or_else(|| "Tab".to_string()),
                terminal_id: normalized_string(&tab.terminal_id).unwrap_or_default(),
            })
        })
        .collect::<Vec<_>>();
    next.sort_by(|left, right| compare_slot_id(&left.id, &right.id));
    next
}

fn sanitize_top_pane_ratio_entries(
    panes: Vec<TerminalTopPaneRecord>,
    ratios: Vec<f64>,
) -> (Vec<TerminalTopPaneRecord>, Vec<f64>) {
    let mut seen = HashSet::new();
    let mut next = panes
        .into_iter()
        .enumerate()
        .filter_map(|pane| {
            let (index, pane) = pane;
            let id = normalized_string(&pane.id)?;
            if !seen.insert(id.clone()) {
                return None;
            }
            Some((
                TerminalTopPaneRecord {
                    id,
                    title: normalized_string(&pane.title).unwrap_or_else(|| "Split".to_string()),
                    terminal_id: normalized_string(&pane.terminal_id).unwrap_or_default(),
                    detached: false,
                },
                ratios.get(index).copied().unwrap_or(0.0),
            ))
        })
        .collect::<Vec<_>>();
    next.sort_by(|left, right| compare_slot_id(&left.0.id, &right.0.id));
    let top_panes = next
        .iter()
        .map(|(pane, _)| pane.clone())
        .collect::<Vec<_>>();
    let top_ratios = normalize_ratios(
        next.into_iter().map(|(_, ratio)| ratio).collect(),
        top_panes.len(),
    );
    (top_panes, top_ratios)
}

fn normalize_ratios(ratios: Vec<f64>, count: usize) -> Vec<f64> {
    if count == 0 {
        return Vec::new();
    }
    let mut values = ratios
        .into_iter()
        .take(count)
        .map(|value| {
            if value.is_finite() {
                value.max(0.0)
            } else {
                0.0
            }
        })
        .collect::<Vec<_>>();
    while values.len() < count {
        values.push(1.0 / count as f64);
    }
    let total = values.iter().sum::<f64>();
    if total <= 0.0 {
        return vec![1.0 / count as f64; count];
    }
    values.into_iter().map(|value| value / total).collect()
}

fn clamp_ratio(value: f64, min: f64, max: f64, fallback: f64) -> f64 {
    if !value.is_finite() {
        return fallback;
    }
    value.clamp(min, max)
}

fn compare_slot_id(left: &str, right: &str) -> std::cmp::Ordering {
    let (left_prefix, left_index) = parse_slot_id(left);
    let (right_prefix, right_index) = parse_slot_id(right);
    left_prefix
        .cmp(&right_prefix)
        .then_with(|| left_index.cmp(&right_index))
}

fn parse_slot_id(id: &str) -> (String, usize) {
    let Some((prefix, index)) = id.rsplit_once('-') else {
        return (id.to_string(), usize::MAX);
    };
    let index = index.parse::<usize>().unwrap_or(usize::MAX);
    (prefix.to_string(), index)
}

fn default_bottom_ratio() -> f64 {
    0.32
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_known_workspace_id(snapshot: &AppSnapshot, project_id: &str) -> bool {
    snapshot
        .projects
        .iter()
        .any(|project| project.id == project_id)
        || snapshot
            .worktrees
            .iter()
            .any(|worktree| worktree.id == project_id)
}

fn prune_project_state(snapshot: &mut AppSnapshot, project_id: &str) {
    let removed_worktree_ids = snapshot
        .worktrees
        .iter()
        .filter(|worktree| worktree.project_id == project_id)
        .map(|worktree| worktree.id.clone())
        .collect::<HashSet<_>>();
    snapshot
        .worktrees
        .retain(|worktree| worktree.project_id != project_id);
    snapshot
        .worktree_tasks
        .retain(|task| !removed_worktree_ids.contains(&task.worktree_id));
    snapshot.terminal_layouts.remove(project_id);
    for worktree_id in removed_worktree_ids {
        snapshot.terminal_layouts.remove(&worktree_id);
    }
    snapshot.selected_worktree_id_by_project.remove(project_id);
}

fn select_project_after_removal(snapshot: &AppSnapshot, removed_index: usize) -> Option<String> {
    if snapshot.projects.is_empty() {
        return None;
    }
    snapshot
        .projects
        .get(removed_index)
        .or_else(|| {
            removed_index
                .checked_sub(1)
                .and_then(|index| snapshot.projects.get(index))
        })
        .or_else(|| snapshot.projects.first())
        .map(|project| project.id.clone())
}

fn merge_worktree_state(
    snapshot: &mut AppSnapshot,
    incoming: WorktreeSnapshot,
) -> Result<WorktreeSnapshot, String> {
    if !snapshot
        .projects
        .iter()
        .any(|project| project.id == incoming.project_id)
    {
        return Err("Project not found.".to_string());
    }

    let project_id = incoming.project_id.clone();
    let incoming_ids = incoming
        .worktrees
        .iter()
        .map(|worktree| worktree.id.clone())
        .collect::<HashSet<_>>();
    let mut existing_by_id = snapshot
        .worktrees
        .iter()
        .filter(|worktree| worktree.project_id == project_id)
        .map(|worktree| (worktree.id.clone(), worktree.clone()))
        .collect::<HashMap<_, _>>();
    let incoming_task_by_id = incoming
        .tasks
        .iter()
        .map(|task| (task.worktree_id.clone(), task.clone()))
        .collect::<HashMap<_, _>>();
    let existing_project_worktree_ids = snapshot
        .worktrees
        .iter()
        .filter(|worktree| worktree.project_id == project_id)
        .map(|worktree| worktree.id.clone())
        .collect::<HashSet<_>>();
    let mut stored_tasks_by_id = snapshot
        .worktree_tasks
        .iter()
        .filter(|task| incoming_ids.contains(&task.worktree_id))
        .map(|task| (task.worktree_id.clone(), task.clone()))
        .collect::<HashMap<_, _>>();

    let mut merged_records = Vec::new();
    let mut merged_worktrees = Vec::new();
    for incoming_worktree in incoming.worktrees {
        let mut record = existing_by_id
            .remove(&incoming_worktree.id)
            .unwrap_or_else(|| ProjectWorktreeRecord::from_snapshot(&incoming_worktree));
        let normalized_path = normalize_path(&incoming_worktree.path);
        let changed = record.branch != incoming_worktree.branch
            || record.path != normalized_path
            || record.is_default != incoming_worktree.is_default;
        record.project_id = project_id.clone();
        record.name = if incoming_worktree.is_default {
            incoming_worktree.name.clone()
        } else {
            normalized_string(&record.name).unwrap_or_else(|| incoming_worktree.name.clone())
        };
        record.branch = incoming_worktree.branch.clone();
        record.path = normalized_path;
        record.is_default = incoming_worktree.is_default;
        if record.status.trim().is_empty() {
            record.status = incoming_worktree.status.clone();
        }
        if changed {
            record.updated_at = incoming_worktree.updated_at;
        }
        merged_worktrees.push(record.apply_to_snapshot(incoming_worktree));
        merged_records.push(record);
    }

    let merged_ids = merged_records
        .iter()
        .map(|worktree| worktree.id.clone())
        .collect::<HashSet<_>>();
    let mut merged_tasks = Vec::new();
    for record in &merged_records {
        if record.is_default {
            continue;
        }
        let task = stored_tasks_by_id.remove(&record.id).unwrap_or_else(|| {
            incoming_task_by_id
                .get(&record.id)
                .cloned()
                .map(WorktreeTaskRecord::from_snapshot)
                .unwrap_or_else(|| WorktreeTaskRecord::from_worktree(record))
        });
        merged_tasks.push(task);
    }

    snapshot
        .worktrees
        .retain(|worktree| worktree.project_id != project_id);
    snapshot.worktrees.extend(merged_records);
    snapshot.worktree_tasks.retain(|task| {
        !merged_ids.contains(&task.worktree_id)
            && !existing_project_worktree_ids.contains(&task.worktree_id)
    });
    snapshot.worktree_tasks.extend(merged_tasks.clone());

    let selected = snapshot
        .selected_worktree_id_by_project
        .get(&project_id)
        .cloned()
        .filter(|id| merged_ids.contains(id))
        .or_else(|| {
            merged_worktrees
                .iter()
                .find(|worktree| worktree.is_default)
                .map(|worktree| worktree.id.clone())
        })
        .or_else(|| merged_worktrees.first().map(|worktree| worktree.id.clone()))
        .unwrap_or_default();
    if selected.is_empty() {
        snapshot.selected_worktree_id_by_project.remove(&project_id);
    } else {
        snapshot
            .selected_worktree_id_by_project
            .insert(project_id.clone(), selected.clone());
    }

    Ok(WorktreeSnapshot {
        project_id,
        selected_worktree_id: selected,
        worktrees: merged_worktrees,
        tasks: merged_tasks
            .into_iter()
            .map(WorktreeTaskRecord::into_snapshot)
            .collect(),
        error: incoming.error,
    })
}

impl ProjectWorktreeRecord {
    fn from_snapshot(snapshot: &ProjectWorktreeSnapshot) -> Self {
        Self {
            id: snapshot.id.clone(),
            project_id: snapshot.project_id.clone(),
            name: snapshot.name.clone(),
            branch: snapshot.branch.clone(),
            path: normalize_path(&snapshot.path),
            status: snapshot.status.clone(),
            is_default: snapshot.is_default,
            created_at: snapshot.created_at,
            updated_at: snapshot.updated_at,
        }
    }

    fn apply_to_snapshot(&self, mut snapshot: ProjectWorktreeSnapshot) -> ProjectWorktreeSnapshot {
        snapshot.name = self.name.clone();
        snapshot.branch = self.branch.clone();
        snapshot.path = self.path.clone();
        snapshot.status = self.status.clone();
        snapshot.is_default = self.is_default;
        snapshot.created_at = self.created_at;
        snapshot.updated_at = self.updated_at;
        snapshot
    }
}

impl WorktreeTaskRecord {
    fn from_snapshot(snapshot: WorktreeTaskSnapshot) -> Self {
        Self {
            worktree_id: snapshot.worktree_id,
            title: snapshot.title,
            base_branch: snapshot.base_branch,
            base_commit: snapshot.base_commit,
            status: snapshot.status,
            created_at: snapshot.created_at,
            updated_at: snapshot.updated_at,
            started_at: snapshot.started_at,
            completed_at: snapshot.completed_at,
        }
    }

    fn from_worktree(worktree: &ProjectWorktreeRecord) -> Self {
        Self {
            worktree_id: worktree.id.clone(),
            title: worktree.name.clone(),
            base_branch: String::new(),
            base_commit: None,
            status: worktree.status.clone(),
            created_at: worktree.created_at,
            updated_at: worktree.updated_at,
            started_at: None,
            completed_at: None,
        }
    }

    fn into_snapshot(self) -> WorktreeTaskSnapshot {
        WorktreeTaskSnapshot {
            worktree_id: self.worktree_id,
            title: self.title,
            base_branch: self.base_branch,
            base_commit: self.base_commit,
            status: self.status,
            created_at: self.created_at,
            updated_at: self.updated_at,
            started_at: self.started_at,
            completed_at: self.completed_at,
        }
    }
}

fn sanitize_worktrees(
    worktrees: Vec<ProjectWorktreeRecord>,
    projects: &[ProjectRecord],
) -> Vec<ProjectWorktreeRecord> {
    let project_ids = projects
        .iter()
        .map(|project| project.id.clone())
        .collect::<HashSet<_>>();
    let mut seen_ids = HashSet::new();
    let mut seen_paths = HashSet::new();
    worktrees
        .into_iter()
        .filter_map(|mut worktree| {
            worktree.path = normalize_path(&worktree.path);
            if !project_ids.contains(&worktree.project_id)
                || worktree.path.trim().is_empty()
                || !seen_ids.insert(worktree.id.clone())
                || !seen_paths.insert(worktree.path.clone())
            {
                return None;
            }
            if worktree.name.trim().is_empty() {
                worktree.name = Path::new(&worktree.path)
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or("Worktree")
                    .to_string();
            }
            if worktree.status.trim().is_empty() {
                worktree.status = "todo".to_string();
            }
            Some(worktree)
        })
        .collect()
}

fn sanitize_worktree_tasks(
    tasks: Vec<WorktreeTaskRecord>,
    valid_worktree_ids: &HashSet<String>,
) -> Vec<WorktreeTaskRecord> {
    let mut seen_ids = HashSet::new();
    tasks
        .into_iter()
        .filter_map(|mut task| {
            if !valid_worktree_ids.contains(&task.worktree_id)
                || !seen_ids.insert(task.worktree_id.clone())
            {
                return None;
            }
            task.title =
                normalized_string(&task.title).unwrap_or_else(|| "Untitled Task".to_string());
            task.base_branch = task.base_branch.trim().to_string();
            if task.status.trim().is_empty() {
                task.status = "todo".to_string();
            }
            Some(task)
        })
        .collect()
}

fn project_summary(project: &ProjectRecord) -> ProjectSummary {
    ProjectSummary {
        id: project.id.clone(),
        name: project.name.clone(),
        path: project.path.clone(),
        badge: project
            .badge_text
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| badge_from_name(&project.name)),
        status: "active".to_string(),
        branch: "master".to_string(),
        changes: 0,
        badge_symbol: project.badge_symbol.clone(),
        badge_color_hex: project.badge_color_hex.clone(),
        git_default_push_remote_name: project.git_default_push_remote_name.clone(),
    }
}

fn worktree_summary(worktree: &ProjectWorktreeRecord) -> ProjectSummary {
    ProjectSummary {
        id: worktree.id.clone(),
        name: worktree.name.clone(),
        path: worktree.path.clone(),
        badge: badge_from_name(&worktree.name),
        status: worktree.status.clone(),
        branch: worktree.branch.clone(),
        changes: 0,
        badge_symbol: None,
        badge_color_hex: None,
        git_default_push_remote_name: None,
    }
}

fn project_uuid(name: &str, path: &str) -> String {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("codux:project:{name}:{path}").as_bytes(),
    )
    .to_string()
}

fn badge_from_name(name: &str) -> String {
    let letters = name
        .split(|ch: char| !ch.is_ascii_alphanumeric())
        .filter_map(|part| part.chars().next())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    if letters.is_empty() {
        "PR".to_string()
    } else {
        letters
    }
}

fn normalized_string(value: &str) -> Option<String> {
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn normalize_path(path: &str) -> String {
    let path = Path::new(path.trim());
    path.canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .display()
        .to_string()
}

fn state_file_path() -> PathBuf {
    app_support_dir().join("state.json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::worktree::ProjectWorktreeGitSummary;

    #[test]
    fn merge_worktree_state_preserves_stored_task_and_selection() {
        let mut selected = HashMap::new();
        selected.insert("project-a".to_string(), "worktree-a".to_string());
        let mut state = AppSnapshot {
            projects: vec![test_project("project-a")],
            worktrees: vec![ProjectWorktreeRecord {
                id: "worktree-a".to_string(),
                project_id: "project-a".to_string(),
                name: "Stored task".to_string(),
                branch: "task/a".to_string(),
                path: "/tmp/project-a-worktree".to_string(),
                status: "review".to_string(),
                is_default: false,
                created_at: 1,
                updated_at: 2,
            }],
            worktree_tasks: vec![WorktreeTaskRecord {
                worktree_id: "worktree-a".to_string(),
                title: "Stored title".to_string(),
                base_branch: "main".to_string(),
                base_commit: Some("abc".to_string()),
                status: "review".to_string(),
                created_at: 1,
                updated_at: 2,
                started_at: Some(3),
                completed_at: None,
            }],
            terminal_layouts: HashMap::new(),
            selected_project_id: Some("project-a".to_string()),
            selected_worktree_id_by_project: selected,
        };

        let result = merge_worktree_state(&mut state, incoming_snapshot()).unwrap();

        assert_eq!(result.selected_worktree_id, "worktree-a");
        let worktree = result
            .worktrees
            .iter()
            .find(|worktree| worktree.id == "worktree-a")
            .unwrap();
        assert_eq!(worktree.name, "Stored task");
        assert_eq!(worktree.status, "review");
        assert_eq!(worktree.created_at, 1);
        assert_eq!(result.tasks[0].title, "Stored title");
        assert_eq!(result.tasks[0].status, "review");
        assert_eq!(state.worktree_tasks.len(), 1);
    }

    #[test]
    fn merge_worktree_state_falls_back_to_default_when_selection_is_stale() {
        let mut selected = HashMap::new();
        selected.insert("project-a".to_string(), "missing".to_string());
        let mut state = AppSnapshot {
            projects: vec![test_project("project-a")],
            worktrees: Vec::new(),
            worktree_tasks: Vec::new(),
            terminal_layouts: HashMap::new(),
            selected_project_id: Some("project-a".to_string()),
            selected_worktree_id_by_project: selected,
        };

        let result = merge_worktree_state(&mut state, incoming_snapshot()).unwrap();

        assert_eq!(result.selected_worktree_id, "project-a");
        assert_eq!(
            state
                .selected_worktree_id_by_project
                .get("project-a")
                .map(String::as_str),
            Some("project-a")
        );
    }

    #[test]
    fn merge_worktree_state_removes_worktrees_missing_from_incoming_snapshot() {
        let mut selected = HashMap::new();
        selected.insert("project-a".to_string(), "worktree-a".to_string());
        let mut state = AppSnapshot {
            projects: vec![test_project("project-a")],
            worktrees: vec![
                ProjectWorktreeRecord {
                    id: "project-a".to_string(),
                    project_id: "project-a".to_string(),
                    name: "Default".to_string(),
                    branch: "main".to_string(),
                    path: "/tmp/project-a".to_string(),
                    status: "todo".to_string(),
                    is_default: true,
                    created_at: 1,
                    updated_at: 1,
                },
                ProjectWorktreeRecord {
                    id: "worktree-a".to_string(),
                    project_id: "project-a".to_string(),
                    name: "Removed task".to_string(),
                    branch: "task/a".to_string(),
                    path: "/tmp/project-a-worktree".to_string(),
                    status: "review".to_string(),
                    is_default: false,
                    created_at: 1,
                    updated_at: 2,
                },
            ],
            worktree_tasks: vec![WorktreeTaskRecord {
                worktree_id: "worktree-a".to_string(),
                title: "Removed task".to_string(),
                base_branch: "main".to_string(),
                base_commit: None,
                status: "review".to_string(),
                created_at: 1,
                updated_at: 2,
                started_at: None,
                completed_at: None,
            }],
            terminal_layouts: HashMap::new(),
            selected_project_id: Some("project-a".to_string()),
            selected_worktree_id_by_project: selected,
        };
        let incoming = WorktreeSnapshot {
            project_id: "project-a".to_string(),
            selected_worktree_id: "project-a".to_string(),
            worktrees: vec![test_worktree(
                "project-a",
                "project-a",
                "Default",
                "/tmp/project-a",
                true,
            )],
            tasks: Vec::new(),
            error: None,
        };

        let result = merge_worktree_state(&mut state, incoming).unwrap();

        assert_eq!(result.selected_worktree_id, "project-a");
        assert_eq!(result.worktrees.len(), 1);
        assert!(result
            .worktrees
            .iter()
            .all(|worktree| worktree.id != "worktree-a"));
        assert!(state
            .worktrees
            .iter()
            .all(|worktree| worktree.id != "worktree-a"));
        assert!(state.worktree_tasks.is_empty());
    }

    #[test]
    fn sanitize_snapshot_preserves_valid_terminal_layouts() {
        let state = sanitize_snapshot(AppSnapshot {
            projects: vec![test_project("project-a")],
            worktrees: vec![ProjectWorktreeRecord {
                id: "worktree-a".to_string(),
                project_id: "project-a".to_string(),
                name: "Worktree A".to_string(),
                branch: "task/a".to_string(),
                path: "/tmp/project-a-worktree".to_string(),
                status: "running".to_string(),
                is_default: false,
                created_at: 1,
                updated_at: 1,
            }],
            worktree_tasks: Vec::new(),
            terminal_layouts: HashMap::from([
                (
                    "worktree-a".to_string(),
                    TerminalLayoutRecord {
                        tabs: vec![TerminalBottomTabRecord {
                            id: "bottom-2".to_string(),
                            label: "Tab 2".to_string(),
                            terminal_id: "old:bottom-2".to_string(),
                        }],
                        active_tab_id: "missing".to_string(),
                        top_panes: vec![
                            TerminalTopPaneRecord {
                                id: "top-2".to_string(),
                                title: "Split 2".to_string(),
                                terminal_id: "old:top-2".to_string(),
                                detached: true,
                            },
                            TerminalTopPaneRecord {
                                id: "top-1".to_string(),
                                title: "Split 1".to_string(),
                                terminal_id: "old:top-1".to_string(),
                                detached: false,
                            },
                        ],
                        top_ratios: vec![0.25, 0.75],
                        bottom_ratio: 0.9,
                        active_slot_id: "missing".to_string(),
                    },
                ),
                (
                    "missing".to_string(),
                    TerminalLayoutRecord {
                        tabs: Vec::new(),
                        active_tab_id: String::new(),
                        top_panes: Vec::new(),
                        top_ratios: Vec::new(),
                        bottom_ratio: 0.32,
                        active_slot_id: String::new(),
                    },
                ),
            ]),
            selected_project_id: Some("project-a".to_string()),
            selected_worktree_id_by_project: HashMap::new(),
        });

        assert_eq!(state.terminal_layouts.len(), 1);
        let layout = state.terminal_layouts.get("worktree-a").unwrap();
        assert_eq!(
            layout
                .top_panes
                .iter()
                .map(|pane| pane.id.as_str())
                .collect::<Vec<_>>(),
            vec!["top-1", "top-2"]
        );
        assert_eq!(layout.top_ratios, vec![0.75, 0.25]);
        assert_eq!(layout.bottom_ratio, 0.72);
        assert_eq!(layout.active_tab_id, "bottom-2");
        assert_eq!(layout.active_slot_id, "top-1");
        assert!(layout.top_panes.iter().all(|pane| !pane.detached));
    }

    #[test]
    fn sanitize_terminal_layout_allows_empty_bottom_tabs() {
        let layout = sanitize_terminal_layout(TerminalLayoutRecord {
            tabs: Vec::new(),
            active_tab_id: "bottom-1".to_string(),
            top_panes: vec![TerminalTopPaneRecord {
                id: "top-1".to_string(),
                title: "Split 1".to_string(),
                terminal_id: "old:top-1".to_string(),
                detached: false,
            }],
            top_ratios: Vec::new(),
            bottom_ratio: 0.1,
            active_slot_id: "bottom-1".to_string(),
        })
        .unwrap();

        assert!(layout.tabs.is_empty());
        assert_eq!(layout.top_ratios, vec![1.0]);
        assert_eq!(layout.bottom_ratio, 0.18);
        assert_eq!(layout.active_tab_id, "");
        assert_eq!(layout.active_slot_id, "top-1");
    }

    #[test]
    fn project_summary_is_fast_and_does_not_require_git_repository() {
        let summary = project_summary(&test_project("project-a"));

        assert_eq!(summary.id, "project-a");
        assert_eq!(summary.branch, "master");
        assert_eq!(summary.changes, 0);
    }

    #[test]
    fn sanitize_snapshot_allows_empty_project_list() {
        let state = sanitize_snapshot(AppSnapshot::default());

        assert!(state.projects.is_empty());
        assert_eq!(state.selected_project_id, None);
    }

    #[test]
    fn close_project_prunes_worktrees_tasks_layouts_and_selects_next_project() {
        let mut selected = HashMap::new();
        selected.insert("project-a".to_string(), "worktree-a".to_string());
        let mut state = AppSnapshot {
            projects: vec![test_project("project-a"), test_project("project-b")],
            worktrees: vec![ProjectWorktreeRecord {
                id: "worktree-a".to_string(),
                project_id: "project-a".to_string(),
                name: "Task A".to_string(),
                branch: "task/a".to_string(),
                path: "/tmp/project-a-worktree".to_string(),
                status: "todo".to_string(),
                is_default: false,
                created_at: 1,
                updated_at: 1,
            }],
            worktree_tasks: vec![WorktreeTaskRecord {
                worktree_id: "worktree-a".to_string(),
                title: "Task A".to_string(),
                base_branch: "main".to_string(),
                base_commit: None,
                status: "todo".to_string(),
                created_at: 1,
                updated_at: 1,
                started_at: None,
                completed_at: None,
            }],
            terminal_layouts: HashMap::from([
                ("project-a".to_string(), valid_layout()),
                ("worktree-a".to_string(), valid_layout()),
                ("project-b".to_string(), valid_layout()),
            ]),
            selected_project_id: Some("project-a".to_string()),
            selected_worktree_id_by_project: selected,
        };

        prune_project_state(&mut state, "project-a");
        state.projects.remove(0);
        state.selected_project_id = select_project_after_removal(&state, 0);

        assert_eq!(state.projects.len(), 1);
        assert_eq!(state.selected_project_id.as_deref(), Some("project-b"));
        assert!(state.worktrees.is_empty());
        assert!(state.worktree_tasks.is_empty());
        assert!(!state.terminal_layouts.contains_key("project-a"));
        assert!(!state.terminal_layouts.contains_key("worktree-a"));
        assert!(state.terminal_layouts.contains_key("project-b"));
        assert!(state.selected_worktree_id_by_project.is_empty());
    }

    fn test_project(id: &str) -> ProjectRecord {
        ProjectRecord {
            id: id.to_string(),
            name: "Project A".to_string(),
            path: "/tmp/project-a".to_string(),
            badge_text: None,
            badge_symbol: None,
            badge_color_hex: None,
            git_default_push_remote_name: None,
        }
    }

    fn valid_layout() -> TerminalLayoutRecord {
        TerminalLayoutRecord {
            tabs: Vec::new(),
            active_tab_id: String::new(),
            top_panes: vec![TerminalTopPaneRecord {
                id: "top-1".to_string(),
                title: "Split 1".to_string(),
                terminal_id: "terminal-1".to_string(),
                detached: false,
            }],
            top_ratios: vec![1.0],
            bottom_ratio: 0.32,
            active_slot_id: "top-1".to_string(),
        }
    }

    fn incoming_snapshot() -> WorktreeSnapshot {
        WorktreeSnapshot {
            project_id: "project-a".to_string(),
            selected_worktree_id: "project-a".to_string(),
            worktrees: vec![
                test_worktree("project-a", "project-a", "Default", "/tmp/project-a", true),
                test_worktree(
                    "worktree-a",
                    "project-a",
                    "Synthetic task",
                    "/tmp/project-a-worktree",
                    false,
                ),
            ],
            tasks: vec![WorktreeTaskSnapshot {
                worktree_id: "worktree-a".to_string(),
                title: "Synthetic title".to_string(),
                base_branch: "main".to_string(),
                base_commit: None,
                status: "todo".to_string(),
                created_at: 10,
                updated_at: 10,
                started_at: None,
                completed_at: None,
            }],
            error: None,
        }
    }

    fn test_worktree(
        id: &str,
        project_id: &str,
        name: &str,
        path: &str,
        is_default: bool,
    ) -> ProjectWorktreeSnapshot {
        ProjectWorktreeSnapshot {
            id: id.to_string(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            branch: "main".to_string(),
            path: path.to_string(),
            status: "todo".to_string(),
            is_default,
            created_at: 10,
            updated_at: 10,
            git_summary: ProjectWorktreeGitSummary {
                changes: 0,
                incoming: 0,
                outgoing: 0,
                additions: 0,
                deletions: 0,
            },
        }
    }
}
