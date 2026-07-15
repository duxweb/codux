use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeProject {
    pub id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeTerminal {
    pub id: String,
    #[serde(default)]
    pub title: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(
        default,
        rename = "worktreeId",
        skip_serializing_if = "Option::is_none"
    )]
    pub worktree_id: Option<String>,
    #[serde(
        default,
        rename = "layoutOrder",
        skip_serializing_if = "Option::is_none"
    )]
    pub layout_order: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cols: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rows: Option<u16>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub buffer_characters: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeWorktree {
    pub id: String,
    #[serde(rename = "projectId")]
    pub project_id: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub branch: String,
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, rename = "isDefault")]
    pub is_default: bool,
    #[serde(default = "default_true")]
    pub exists: bool,
    #[serde(
        default,
        rename = "baseBranch",
        skip_serializing_if = "Option::is_none"
    )]
    pub base_branch: Option<String>,
    #[serde(default)]
    pub changes: i64,
    #[serde(default)]
    pub incoming: i64,
    #[serde(default)]
    pub outgoing: i64,
    #[serde(default)]
    pub additions: i64,
    #[serde(default)]
    pub deletions: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeWorktreeState {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_worktree_id: Option<String>,
    #[serde(default)]
    pub worktrees: Vec<RemoteRuntimeWorktree>,
    #[serde(default)]
    pub base_branches: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default_base_branch: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeTerminalScope {
    pub project_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktree_id: Option<String>,
}

pub type RuntimeProject = RemoteRuntimeProject;
pub type RuntimeTerminal = RemoteRuntimeTerminal;
pub type RuntimeTerminalScope = RemoteRuntimeTerminalScope;
pub type RuntimeWorktree = RemoteRuntimeWorktree;
pub type RuntimeWorktreeState = RemoteRuntimeWorktreeState;
pub type RuntimePlan = RemoteRuntimePlan;
pub type RuntimeStateSnapshot = RemoteRuntimeStateSnapshot;
pub type RuntimeModel = RemoteRuntimeModel;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimePlan {
    #[serde(default)]
    pub state_changed: bool,
    #[serde(default)]
    pub clear_terminal: bool,
    #[serde(default)]
    pub reset_terminal_input: bool,
    #[serde(default)]
    pub reset_terminal_buffer: bool,
    #[serde(default)]
    pub request_terminal_list: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_project_select_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bind_session_id: Option<String>,
    #[serde(default)]
    pub bind_full_buffer: bool,
    #[serde(default)]
    pub flush_terminal_input: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub removed_session_ids: Vec<String>,
}

// Not `Eq`: `git_status_by_project` holds opaque git-status JSON (`serde_json::Value`),
// which is `PartialEq` but not `Eq`. The runtime model does no logic on git status —
// it only stores the per-project projection so all subscription state lives in one place.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoteRuntimeStateSnapshot {
    pub projects: Vec<RemoteRuntimeProject>,
    pub terminals: Vec<RemoteRuntimeTerminal>,
    #[serde(default)]
    pub worktrees: Vec<RemoteRuntimeWorktree>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_project_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_session_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_worktree_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pending_project_select_id: Option<String>,
    #[serde(default)]
    pub pending_project_select_sent: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_select_acknowledged_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub creating_terminal_project_id: Option<String>,
    #[serde(default)]
    pub last_terminal_id_by_project: HashMap<String, String>,
    #[serde(default)]
    pub selected_worktree_id_by_project: HashMap<String, String>,
    #[serde(default)]
    pub base_branches_by_project: HashMap<String, Vec<String>>,
    #[serde(default)]
    pub default_base_branch_by_project: HashMap<String, String>,
    #[serde(default)]
    pub git_status_by_project: HashMap<String, serde_json::Value>,
}

#[derive(Debug, Clone, Default)]
struct PendingTerminalCreateRequest {
    terminal_id: String,
    project_id: String,
    worktree_id: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub struct RemoteRuntimeModel {
    projects: Vec<RemoteRuntimeProject>,
    terminals: Vec<RemoteRuntimeTerminal>,
    worktrees: Vec<RemoteRuntimeWorktree>,
    selected_project_id: Option<String>,
    active_session_id: Option<String>,
    selected_worktree_id: Option<String>,
    pending_worktree_terminal_request_key: Option<String>,
    pending_project_select_id: Option<String>,
    pending_project_select_sent: bool,
    project_select_acknowledged_id: Option<String>,
    creating_terminal_project_id: Option<String>,
    pending_terminal_create_request: Option<PendingTerminalCreateRequest>,
    pending_created_terminal: Option<RemoteRuntimeTerminal>,
    pending_created_terminal_confirmed: bool,
    last_terminal_id_by_project: HashMap<String, String>,
    selected_worktree_id_by_project: HashMap<String, String>,
    base_branches_by_project: HashMap<String, Vec<String>>,
    default_base_branch_by_project: HashMap<String, String>,
    git_status_by_project: HashMap<String, serde_json::Value>,
}

impl RemoteRuntimeModel {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn snapshot(&self) -> RemoteRuntimeStateSnapshot {
        RemoteRuntimeStateSnapshot {
            projects: self.projects.clone(),
            terminals: self.terminals.clone(),
            worktrees: self.worktrees.clone(),
            selected_project_id: self.selected_project_id.clone(),
            active_session_id: self.active_session_id.clone(),
            selected_worktree_id: self.selected_worktree_id.clone(),
            pending_project_select_id: self.pending_project_select_id.clone(),
            pending_project_select_sent: self.pending_project_select_sent,
            project_select_acknowledged_id: self.project_select_acknowledged_id.clone(),
            creating_terminal_project_id: self.creating_terminal_project_id.clone(),
            last_terminal_id_by_project: self.last_terminal_id_by_project.clone(),
            selected_worktree_id_by_project: self.selected_worktree_id_by_project.clone(),
            base_branches_by_project: self.base_branches_by_project.clone(),
            default_base_branch_by_project: self.default_base_branch_by_project.clone(),
            git_status_by_project: self.git_status_by_project.clone(),
        }
    }

    /// Store the latest git-status projection for a project. Returns a plan with
    /// `state_changed` so the UI re-renders; a status with no `projectId` is ignored.
    pub fn apply_git_status(&mut self, status: serde_json::Value) -> RemoteRuntimePlan {
        let project_id = status
            .get("projectId")
            .and_then(|value| value.as_str())
            .and_then(clean_nonempty_str);
        let Some(project_id) = project_id else {
            return RemoteRuntimePlan::default();
        };
        self.git_status_by_project.insert(project_id, status);
        RemoteRuntimePlan {
            state_changed: true,
            ..RemoteRuntimePlan::default()
        }
    }

    pub fn selected_scope_key(&self) -> Option<String> {
        let project_id = self.selected_project_id.as_deref()?;
        Some(runtime_scope_key(
            project_id,
            self.selected_worktree_id.as_deref(),
        ))
    }

    pub fn terminal_scope_for_project(
        &self,
        project_id: &str,
    ) -> Option<RemoteRuntimeTerminalScope> {
        let project_id = clean_nonempty_str(project_id)?;
        let worktree_id = if self.selected_project_id.as_deref() == Some(project_id.as_str()) {
            self.selected_worktree_id.clone()
        } else {
            None
        };
        Some(RemoteRuntimeTerminalScope {
            project_path: self.project_path(&project_id),
            worktree_id: normalize_worktree_scope(&project_id, worktree_id),
            project_id,
        })
    }

    pub fn terminal_scope_for_session(
        &self,
        session_id: &str,
        terminal: Option<RemoteRuntimeTerminal>,
    ) -> Option<RemoteRuntimeTerminalScope> {
        let session_id = clean_nonempty_str(session_id)?;
        let explicit_terminal = terminal
            .filter(|terminal| terminal.id == session_id && is_accessible_terminal(terminal));
        let terminal_ref = explicit_terminal.as_ref().or_else(|| {
            self.terminals
                .iter()
                .find(|terminal| terminal.id == session_id && is_accessible_terminal(terminal))
        });
        let project_id = terminal_ref
            .and_then(|terminal| clean_nonempty_str(&terminal.project_id))
            .or_else(|| self.selected_project_id.clone())?;
        let terminal_worktree_id = terminal_ref
            .and_then(|terminal| terminal.worktree_id.as_deref().and_then(clean_nonempty_str));
        let selected_worktree_id =
            if self.selected_project_id.as_deref() == Some(project_id.as_str()) {
                self.selected_worktree_id.clone()
            } else {
                None
            };
        Some(RemoteRuntimeTerminalScope {
            project_path: self.project_path(&project_id),
            worktree_id: normalize_worktree_scope(
                &project_id,
                terminal_worktree_id.or(selected_worktree_id),
            ),
            project_id,
        })
    }

    pub fn reset(&mut self, keep_projects: bool) {
        let projects = if keep_projects {
            self.projects.clone()
        } else {
            Vec::new()
        };
        let selected = self
            .selected_project_id
            .as_ref()
            .filter(|selected| keep_projects && projects.iter().any(|item| item.id == **selected))
            .cloned();
        *self = Self {
            projects,
            selected_project_id: selected,
            worktrees: if keep_projects {
                self.worktrees.clone()
            } else {
                Vec::new()
            },
            base_branches_by_project: if keep_projects {
                self.base_branches_by_project.clone()
            } else {
                HashMap::new()
            },
            default_base_branch_by_project: if keep_projects {
                self.default_base_branch_by_project.clone()
            } else {
                HashMap::new()
            },
            git_status_by_project: if keep_projects {
                self.git_status_by_project.clone()
            } else {
                HashMap::new()
            },
            selected_worktree_id: None,
            selected_worktree_id_by_project: if keep_projects {
                self.selected_worktree_id_by_project.clone()
            } else {
                HashMap::new()
            },
            ..Self::default()
        };
        if let Some(selected_project_id) = self.selected_project_id.clone() {
            self.selected_worktree_id = self.remembered_worktree_for_project(&selected_project_id);
        }
    }

    pub fn restore_cached_projects(&mut self, projects: Vec<RemoteRuntimeProject>) {
        if projects.is_empty() || !self.projects.is_empty() {
            return;
        }
        self.selected_project_id = projects.first().map(|item| item.id.clone());
        self.projects = projects;
    }

    pub fn apply_project_list(
        &mut self,
        projects: Vec<RemoteRuntimeProject>,
        remote_selected_project_id: Option<String>,
        remote_selected_worktree_id: Option<String>,
        terminal_visible: bool,
        terminal_list_loaded: bool,
    ) -> RemoteRuntimePlan {
        let previous_selected = self.selected_project_id.clone();
        let remote_selected_project_id = clean_optional_string(remote_selected_project_id);
        let remote_selected_worktree_id = clean_optional_string(remote_selected_worktree_id);
        let confirms_pending_project_select = self
            .pending_project_select_id
            .as_deref()
            .is_some_and(|pending| remote_selected_project_id.as_deref() == Some(pending));
        let selected = selected_project_from_list(
            &projects,
            self.pending_project_select_id.as_deref(),
            remote_selected_project_id.as_deref(),
            previous_selected.as_deref(),
            self.active_session_id.is_some(),
        );
        let project_changed = selected != previous_selected;
        if project_changed {
            self.remember_current_worktree_selection();
        }
        self.projects = projects;
        self.selected_project_id = selected;
        self.prune_worktree_selections();
        if let Some(project_id) = self.selected_project_id.clone() {
            let had_remembered = self
                .selected_worktree_id_by_project
                .contains_key(project_id.as_str());
            let remote_worktree_id = remote_selected_worktree_id.clone().and_then(|worktree_id| {
                self.valid_worktree_selection_for_project(&project_id, Some(worktree_id))
            });
            if !had_remembered && let Some(worktree_id) = remote_worktree_id.clone() {
                self.remember_worktree_selection(&project_id, Some(worktree_id));
            }
            let selected_worktree_id = self
                .remembered_worktree_for_project(&project_id)
                .or(remote_worktree_id)
                .or_else(|| self.default_worktree_selection_for_project(&project_id));
            if project_changed
                || !had_remembered
                || self.selected_worktree_id.is_none()
                || !self.selected_worktree_is_valid_for_project(&project_id)
            {
                self.selected_worktree_id = selected_worktree_id;
            }
        }
        if project_changed {
            self.active_session_id = None;
            self.pending_worktree_terminal_request_key = None;
            self.clear_pending_created_terminal();
            self.pending_terminal_create_request = None;
            self.creating_terminal_project_id = None;
        }
        if confirms_pending_project_select {
            self.project_select_acknowledged_id = self.pending_project_select_id.clone();
            self.pending_project_select_id = None;
            self.pending_project_select_sent = false;
        }
        let bind =
            self.ensure_terminal_for_selected_project(terminal_visible, terminal_list_loaded);
        RemoteRuntimePlan {
            state_changed: true,
            clear_terminal: project_changed && terminal_visible,
            reset_terminal_input: project_changed && terminal_visible,
            reset_terminal_buffer: terminal_visible
                && (project_changed || bind.reset_terminal_buffer),
            request_terminal_list: bind.request_terminal_list,
            request_project_select_id: bind.request_project_select_id,
            bind_session_id: bind.bind_session_id,
            bind_full_buffer: terminal_visible && bind.bind_full_buffer,
            flush_terminal_input: terminal_visible && bind.flush_terminal_input,
            removed_session_ids: Vec::new(),
        }
    }

    pub fn apply_terminal_list(
        &mut self,
        mut terminals: Vec<RemoteRuntimeTerminal>,
        terminal_visible: bool,
        terminal_list_loaded: bool,
    ) -> RemoteRuntimePlan {
        let unavailable_session_ids = terminals
            .iter()
            .filter(|terminal| !is_accessible_terminal(terminal))
            .map(|terminal| terminal.id.clone())
            .collect::<HashSet<_>>();
        terminals.retain(is_accessible_terminal);
        let previous_session_ids = self
            .terminals
            .iter()
            .filter(|terminal| is_accessible_terminal(terminal))
            .map(|terminal| terminal.id.clone())
            .collect::<HashSet<_>>();
        let created_from_list = terminals
            .iter()
            .find(|terminal| self.pending_terminal_create_matches(terminal))
            .cloned();
        let created_should_bind = created_from_list.is_some();
        let waiting_for_pending_create =
            self.pending_terminal_create_request.is_some() && !created_should_bind;
        if let Some(terminal) = created_from_list.as_ref() {
            self.terminals.retain(|item| item.id != terminal.id);
            self.pending_created_terminal = Some((*terminal).clone());
            self.pending_created_terminal_confirmed = true;
            self.pending_worktree_terminal_request_key = None;
            self.focus_created_terminal(terminal);
            self.pending_project_select_id = None;
            self.pending_project_select_sent = false;
            self.project_select_acknowledged_id = None;
            self.creating_terminal_project_id = None;
            self.pending_terminal_create_request = None;
        }
        if let Some(pending) = self.pending_created_terminal.clone() {
            if let Some(confirmed) = terminals
                .iter()
                .find(|item| item.id == pending.id && is_accessible_terminal(item))
                .cloned()
            {
                if self.active_session_id.as_deref() == Some(confirmed.id.as_str()) {
                    self.selected_worktree_id = normalize_terminal_worktree_scope(
                        &confirmed.project_id,
                        confirmed.worktree_id.clone(),
                    );
                    self.remember_worktree_selection(
                        &confirmed.project_id,
                        self.selected_worktree_id.clone(),
                    );
                    self.last_terminal_id_by_project.insert(
                        terminal_memory_key(
                            &confirmed.project_id,
                            self.selected_worktree_id.as_deref(),
                        ),
                        confirmed.id.clone(),
                    );
                    self.selected_project_id = Some(confirmed.project_id.clone());
                }
                self.pending_created_terminal = Some(confirmed);
                self.pending_created_terminal_confirmed = true;
            } else if is_accessible_terminal(&pending)
                && !unavailable_session_ids.contains(&pending.id)
                && (!self.pending_created_terminal_confirmed
                    || self.active_session_id.as_deref() == Some(pending.id.as_str()))
            {
                terminals.retain(|item| item.id != pending.id);
                terminals.insert(0, pending);
            } else {
                self.clear_pending_created_terminal();
            }
        }
        let active_missing = self.active_session_id.as_ref().is_some_and(|active_id| {
            !terminals
                .iter()
                .any(|item| item.id == *active_id && is_accessible_terminal(item))
        });
        let current_session_ids = terminals
            .iter()
            .filter(|terminal| is_accessible_terminal(terminal))
            .map(|terminal| terminal.id.as_str())
            .collect::<HashSet<_>>();
        let mut removed_session_ids = previous_session_ids
            .into_iter()
            .filter(|session_id| !current_session_ids.contains(session_id.as_str()))
            .collect::<Vec<_>>();
        if let Some(active_session_id) = self
            .active_session_id
            .as_ref()
            .filter(|_| active_missing)
            .cloned()
            && !removed_session_ids.contains(&active_session_id)
        {
            removed_session_ids.push(active_session_id);
        }
        removed_session_ids.sort();
        if !removed_session_ids.is_empty() {
            self.last_terminal_id_by_project
                .retain(|_, terminal_id| !removed_session_ids.contains(terminal_id));
        }
        if active_missing {
            self.active_session_id = None;
        }
        self.terminals = terminals;
        let bind = if waiting_for_pending_create {
            RemoteRuntimePlan::default()
        } else {
            self.ensure_terminal_for_selected_project(terminal_visible, terminal_list_loaded)
        };
        let created_bind_session_id = created_from_list.map(|terminal| terminal.id);
        let reset_terminal_input = terminal_visible && (active_missing || created_should_bind);
        RemoteRuntimePlan {
            state_changed: true,
            clear_terminal: terminal_visible && created_should_bind,
            reset_terminal_input,
            reset_terminal_buffer: terminal_visible
                && (active_missing || created_should_bind || bind.reset_terminal_buffer),
            removed_session_ids,
            request_terminal_list: bind.request_terminal_list,
            request_project_select_id: bind.request_project_select_id,
            bind_session_id: bind.bind_session_id.or(created_bind_session_id),
            bind_full_buffer: terminal_visible && bind.bind_full_buffer,
            flush_terminal_input: terminal_visible
                && !reset_terminal_input
                && bind.flush_terminal_input,
        }
    }

    pub fn user_select_project(
        &mut self,
        project: RemoteRuntimeProject,
        terminal_visible: bool,
    ) -> RemoteRuntimePlan {
        let project_changed = self.selected_project_id.as_deref() != Some(project.id.as_str());
        let previous_project_id = self.selected_project_id.clone();
        if project_changed {
            self.remember_current_worktree_selection();
        }
        let selected_worktree_id = if project_changed {
            self.remembered_worktree_for_project(&project.id)
                .or_else(|| self.default_worktree_selection_for_project(&project.id))
        } else {
            self.selected_worktree_id
                .clone()
                .or_else(|| self.default_worktree_selection_for_project(&project.id))
        };
        if project_changed
            && let (Some(previous_project_id), Some(active_session_id)) = (
                previous_project_id.as_ref(),
                self.active_session_id.as_ref(),
            )
            && self.terminals.iter().any(|item| {
                item.id == *active_session_id
                    && item.project_id == *previous_project_id
                    && is_accessible_terminal(item)
            })
        {
            self.last_terminal_id_by_project.insert(
                terminal_memory_key(previous_project_id, self.selected_worktree_id.as_deref()),
                active_session_id.clone(),
            );
        }
        let existing = if terminal_visible {
            accessible_terminals_for_project_and_worktree(
                &self.terminals,
                &project.id,
                selected_worktree_id.as_deref(),
            )
        } else {
            Vec::new()
        };
        let terminal = preferred_terminal_for_project(
            &self.last_terminal_id_by_project,
            &project.id,
            selected_worktree_id.as_deref(),
            &existing,
        )
        .cloned();
        if let Some(terminal) = terminal.as_ref() {
            self.last_terminal_id_by_project.insert(
                terminal_memory_key(&project.id, selected_worktree_id.as_deref()),
                terminal.id.clone(),
            );
            self.pending_worktree_terminal_request_key = None;
        }
        self.selected_project_id = Some(project.id.clone());
        if project_changed {
            self.selected_worktree_id = selected_worktree_id;
            self.pending_worktree_terminal_request_key = None;
            self.clear_pending_created_terminal();
            self.pending_terminal_create_request = None;
            self.creating_terminal_project_id = None;
        }
        self.active_session_id = terminal.as_ref().map(|item| item.id.clone()).or_else(|| {
            if project_changed && terminal_visible {
                None
            } else {
                self.active_session_id.clone()
            }
        });
        self.pending_project_select_id = Some(project.id.clone());
        self.pending_project_select_sent = false;
        self.project_select_acknowledged_id = None;
        RemoteRuntimePlan {
            state_changed: true,
            clear_terminal: project_changed && terminal_visible,
            reset_terminal_input: project_changed && terminal_visible,
            reset_terminal_buffer: project_changed && terminal_visible,
            request_terminal_list: terminal_visible && terminal.is_none(),
            request_project_select_id: Some(project.id),
            bind_session_id: terminal.as_ref().map(|item| item.id.clone()),
            bind_full_buffer: false,
            flush_terminal_input: terminal.is_some(),
            removed_session_ids: Vec::new(),
        }
    }

    pub fn project_selected(
        &mut self,
        project_id: Option<String>,
        worktree_id: Option<String>,
    ) -> RemoteRuntimePlan {
        let Some(selected) = clean_optional_string(project_id) else {
            return RemoteRuntimePlan::default();
        };
        if let Some(pending) = self.pending_project_select_id.as_deref()
            && pending != selected
        {
            return RemoteRuntimePlan::default();
        }
        if self.selected_project_id.as_deref() != Some(selected.as_str())
            && !self.projects.iter().any(|item| item.id == selected)
        {
            return RemoteRuntimePlan::default();
        }
        let project_changed = self.selected_project_id.as_deref() != Some(selected.as_str());
        if project_changed {
            self.remember_current_worktree_selection();
        }
        self.selected_project_id = Some(selected.clone());
        let selected_worktree_id = normalize_worktree_scope(&selected, worktree_id)
            .or_else(|| self.remembered_worktree_for_project(&selected));
        if project_changed {
            self.active_session_id = None;
            self.pending_worktree_terminal_request_key = None;
            self.clear_pending_created_terminal();
            self.pending_terminal_create_request = None;
            self.creating_terminal_project_id = None;
        }
        self.selected_worktree_id = selected_worktree_id;
        self.remember_current_worktree_selection();
        self.pending_project_select_id = None;
        self.pending_project_select_sent = false;
        self.project_select_acknowledged_id = Some(selected);
        RemoteRuntimePlan {
            state_changed: true,
            reset_terminal_input: true,
            reset_terminal_buffer: true,
            request_terminal_list: true,
            ..RemoteRuntimePlan::default()
        }
    }

    pub fn ensure_terminal_for_selected_project(
        &mut self,
        terminal_visible: bool,
        terminal_list_loaded: bool,
    ) -> RemoteRuntimePlan {
        let Some(project_id) = self.selected_project_id.clone() else {
            return RemoteRuntimePlan::default();
        };
        if !terminal_list_loaded {
            return RemoteRuntimePlan {
                request_terminal_list: true,
                ..RemoteRuntimePlan::default()
            };
        }
        if let Some(active_id) = self.active_session_id.as_ref()
            && self.terminals.iter().any(|item| {
                item.id == *active_id
                    && item.project_id == project_id
                    && terminal_matches_selected_worktree(
                        item,
                        self.selected_worktree_id.as_deref(),
                    )
                    && is_accessible_terminal(item)
            })
        {
            return RemoteRuntimePlan::default();
        }
        if self
            .pending_created_terminal
            .as_ref()
            .is_some_and(|terminal| {
                self.active_session_id.as_deref() == Some(terminal.id.as_str())
                    && terminal.project_id == project_id
                    && terminal_matches_selected_worktree(
                        terminal,
                        self.selected_worktree_id.as_deref(),
                    )
                    && is_accessible_terminal(terminal)
            })
        {
            return RemoteRuntimePlan::default();
        }
        let existing = accessible_terminals_for_project_and_worktree(
            &self.terminals,
            &project_id,
            self.selected_worktree_id.as_deref(),
        );
        if existing.is_empty() {
            if let Some(selected_worktree_id) = self.selected_worktree_id.as_deref()
                && selected_worktree_id != project_id
            {
                let request_key =
                    terminal_memory_key(&project_id, self.selected_worktree_id.as_deref());
                if self.pending_worktree_terminal_request_key.as_deref()
                    == Some(request_key.as_str())
                {
                    return RemoteRuntimePlan::default();
                }
                self.pending_worktree_terminal_request_key = Some(request_key);
                return RemoteRuntimePlan {
                    request_terminal_list: true,
                    ..RemoteRuntimePlan::default()
                };
            }
            if self.pending_project_select_id.as_deref() == Some(project_id.as_str()) {
                if self.pending_project_select_sent {
                    return RemoteRuntimePlan::default();
                }
                return RemoteRuntimePlan {
                    request_project_select_id: Some(project_id),
                    ..RemoteRuntimePlan::default()
                };
            }
            if self.project_select_acknowledged_id.as_deref() == Some(project_id.as_str()) {
                return RemoteRuntimePlan::default();
            }
            self.pending_project_select_id = Some(project_id.clone());
            self.pending_project_select_sent = false;
            self.project_select_acknowledged_id = None;
            return RemoteRuntimePlan {
                request_terminal_list: true,
                request_project_select_id: Some(project_id),
                ..RemoteRuntimePlan::default()
            };
        }
        let terminal = preferred_terminal_for_project(
            &self.last_terminal_id_by_project,
            &project_id,
            self.selected_worktree_id.as_deref(),
            &existing,
        )
        .expect("existing terminals are not empty");
        self.active_session_id = Some(terminal.id.clone());
        if self.pending_project_select_id.as_deref() != Some(project_id.as_str()) {
            self.pending_project_select_id = None;
            self.pending_project_select_sent = false;
            self.project_select_acknowledged_id = None;
        }
        self.creating_terminal_project_id = None;
        self.pending_worktree_terminal_request_key = None;
        self.last_terminal_id_by_project.insert(
            terminal_memory_key(&project_id, self.selected_worktree_id.as_deref()),
            terminal.id.clone(),
        );
        RemoteRuntimePlan {
            state_changed: true,
            reset_terminal_buffer: terminal_visible,
            bind_session_id: Some(terminal.id.clone()),
            bind_full_buffer: false,
            flush_terminal_input: terminal_visible,
            ..RemoteRuntimePlan::default()
        }
    }

    pub fn select_terminal(&mut self, terminal: RemoteRuntimeTerminal) -> RemoteRuntimePlan {
        if !is_accessible_terminal(&terminal) {
            return RemoteRuntimePlan::default();
        }
        self.selected_worktree_id =
            normalize_terminal_worktree_scope(&terminal.project_id, terminal.worktree_id.clone());
        self.remember_worktree_selection(&terminal.project_id, self.selected_worktree_id.clone());
        self.pending_worktree_terminal_request_key = None;
        self.last_terminal_id_by_project.insert(
            terminal_memory_key(&terminal.project_id, self.selected_worktree_id.as_deref()),
            terminal.id.clone(),
        );
        self.selected_project_id = Some(terminal.project_id.clone());
        self.active_session_id = Some(terminal.id.clone());
        if self
            .pending_created_terminal
            .as_ref()
            .is_some_and(|pending| pending.id != terminal.id)
        {
            self.clear_pending_created_terminal();
        }
        self.pending_project_select_id = None;
        self.pending_project_select_sent = false;
        self.project_select_acknowledged_id = None;
        self.creating_terminal_project_id = None;
        self.pending_terminal_create_request = None;
        RemoteRuntimePlan {
            state_changed: true,
            reset_terminal_input: true,
            reset_terminal_buffer: true,
            bind_session_id: Some(terminal.id),
            bind_full_buffer: false,
            ..RemoteRuntimePlan::default()
        }
    }

    pub fn remove_terminal(&mut self, terminal_id: &str) -> RemoteRuntimePlan {
        let closing_active = self.active_session_id.as_deref() == Some(terminal_id);
        self.cancel_terminal_create(Some(terminal_id.to_string()));
        self.terminals.retain(|item| item.id != terminal_id);
        self.last_terminal_id_by_project
            .retain(|_, id| id != terminal_id);
        if self
            .pending_created_terminal
            .as_ref()
            .is_some_and(|terminal| terminal.id == terminal_id)
        {
            self.clear_pending_created_terminal();
        }
        if closing_active {
            self.active_session_id = None;
        }
        RemoteRuntimePlan {
            state_changed: true,
            clear_terminal: closing_active,
            reset_terminal_input: closing_active,
            reset_terminal_buffer: closing_active,
            removed_session_ids: vec![terminal_id.to_string()],
            ..RemoteRuntimePlan::default()
        }
    }

    pub fn begin_terminal_create(
        &mut self,
        terminal_id: Option<String>,
        project_id: Option<String>,
        worktree_id: Option<String>,
    ) {
        let Some((terminal_id, project_id)) =
            clean_optional_string(terminal_id).zip(clean_optional_string(project_id))
        else {
            self.creating_terminal_project_id = None;
            self.pending_terminal_create_request = None;
            return;
        };
        let requested_worktree_id =
            if self.selected_project_id.as_deref() == Some(project_id.as_str()) {
                worktree_id.or_else(|| self.selected_worktree_id.clone())
            } else {
                worktree_id
            };
        let normalized_worktree_id = normalize_worktree_scope(&project_id, requested_worktree_id);
        self.creating_terminal_project_id = Some(project_id.clone());
        self.pending_created_terminal = None;
        self.pending_created_terminal_confirmed = false;
        self.pending_terminal_create_request = Some(PendingTerminalCreateRequest {
            terminal_id,
            project_id,
            worktree_id: normalized_worktree_id,
        });
    }

    pub fn cancel_terminal_create(&mut self, terminal_id: Option<String>) -> bool {
        let Some(request) = self.pending_terminal_create_request.as_ref() else {
            return false;
        };
        if terminal_id
            .as_deref()
            .is_some_and(|terminal_id| terminal_id != request.terminal_id)
        {
            return false;
        }
        self.creating_terminal_project_id = None;
        self.pending_terminal_create_request = None;
        true
    }

    pub fn terminal_created(&mut self, terminal: RemoteRuntimeTerminal) -> RemoteRuntimePlan {
        if !is_accessible_terminal(&terminal) {
            return RemoteRuntimePlan::default();
        }
        let created_by_pending_request = self.pending_terminal_create_matches(&terminal);
        let waiting_for_pending_create = self.pending_terminal_create_request.is_some();
        let worktree_id =
            normalize_terminal_worktree_scope(&terminal.project_id, terminal.worktree_id.clone());
        let created_should_bind = if waiting_for_pending_create {
            created_by_pending_request
        } else {
            !active_terminal_matches_scope_in(
                self.active_session_id.as_deref(),
                &self.terminals,
                &terminal.project_id,
                worktree_id.as_deref(),
            )
        };
        self.terminals.retain(|item| item.id != terminal.id);
        self.terminals.insert(0, terminal.clone());
        self.pending_created_terminal = Some(terminal.clone());
        self.pending_created_terminal_confirmed = false;
        self.pending_worktree_terminal_request_key = None;
        if created_should_bind {
            self.focus_created_terminal(&terminal);
        }
        self.pending_project_select_id = None;
        self.pending_project_select_sent = false;
        self.project_select_acknowledged_id = None;
        if created_by_pending_request {
            self.creating_terminal_project_id = None;
            self.pending_terminal_create_request = None;
        }
        RemoteRuntimePlan {
            state_changed: true,
            clear_terminal: created_should_bind,
            reset_terminal_input: created_should_bind,
            reset_terminal_buffer: created_should_bind,
            bind_session_id: created_should_bind.then_some(terminal.id),
            bind_full_buffer: false,
            ..RemoteRuntimePlan::default()
        }
    }

    pub fn mark_project_select_sent(&mut self, project_id: &str) {
        if self.pending_project_select_id.as_deref() == Some(project_id) {
            self.pending_project_select_sent = true;
        }
    }

    pub fn clear_project_select_sent(&mut self, project_id: &str) {
        if self.pending_project_select_id.as_deref() == Some(project_id) {
            self.pending_project_select_sent = false;
        }
    }

    pub fn pending_project_select(&self, include_sent: bool) -> Option<String> {
        let project_id = self.pending_project_select_id.as_ref()?;
        if project_id.is_empty() || (!include_sent && self.pending_project_select_sent) {
            return None;
        }
        Some(project_id.clone())
    }

    pub fn current_project_terminals(&self) -> Vec<RemoteRuntimeTerminal> {
        let Some(project_id) = self.selected_project_id.as_ref() else {
            return Vec::new();
        };
        let mut terminals = accessible_terminals_for_project_and_worktree(
            &self.terminals,
            project_id,
            self.selected_worktree_id.as_deref(),
        );
        terminals.sort_by(|left, right| compare_remote_terminals(left, right));
        terminals.into_iter().cloned().collect()
    }

    fn project_path(&self, project_id: &str) -> Option<String> {
        self.projects
            .iter()
            .find(|project| project.id == project_id)
            .and_then(|project| project.path.clone())
            .and_then(clean_nonempty_string)
    }

    pub fn apply_worktree_selected(
        &mut self,
        project_id: Option<String>,
        worktree_id: Option<String>,
        terminal_visible: bool,
        terminal_list_loaded: bool,
    ) -> RemoteRuntimePlan {
        let Some(project_id) =
            clean_optional_string(project_id).or_else(|| self.selected_project_id.clone())
        else {
            return RemoteRuntimePlan::default();
        };
        if !self.projects.is_empty() && !self.projects.iter().any(|item| item.id == project_id) {
            return RemoteRuntimePlan::default();
        }
        let next_worktree_id = self.valid_worktree_selection_for_project(&project_id, worktree_id);
        if self.selected_project_id.as_deref() != Some(project_id.as_str())
            || self.selected_worktree_id != next_worktree_id
        {
            self.pending_worktree_terminal_request_key = None;
            self.clear_pending_created_terminal();
            self.pending_terminal_create_request = None;
            self.creating_terminal_project_id = None;
        }
        self.selected_project_id = Some(project_id);
        self.selected_worktree_id = next_worktree_id;
        if let Some(project_id) = self.selected_project_id.clone() {
            self.remember_worktree_selection(&project_id, self.selected_worktree_id.clone());
        }
        self.active_session_id = None;
        self.ensure_terminal_for_selected_project(terminal_visible, terminal_list_loaded)
    }

    pub fn apply_worktree_state(
        &mut self,
        state: RemoteRuntimeWorktreeState,
        allow_runtime_selection: bool,
        terminal_visible: bool,
        terminal_list_loaded: bool,
    ) -> RemoteRuntimePlan {
        let current_project_id = self.selected_project_id.clone();
        let project_id = clean_optional_string(state.project_id.clone());
        if project_id.is_none() {
            return self.apply_all_worktree_state(
                state,
                allow_runtime_selection,
                terminal_visible,
                terminal_list_loaded,
            );
        }
        let project_id = project_id.or(current_project_id);
        let Some(project_id) = project_id else {
            self.worktrees = state.worktrees;
            return RemoteRuntimePlan {
                state_changed: true,
                ..RemoteRuntimePlan::default()
            };
        };
        self.worktrees
            .retain(|worktree| worktree.project_id != project_id);
        let scoped_worktrees = state
            .worktrees
            .into_iter()
            .filter(|worktree| worktree.project_id == project_id)
            .collect::<Vec<_>>();
        let scoped_worktrees = ensure_default_worktree_scope(project_id.as_str(), scoped_worktrees);
        self.base_branches_by_project
            .insert(project_id.clone(), state.base_branches);
        if let Some(default_base_branch) = clean_optional_string(state.default_base_branch) {
            self.default_base_branch_by_project
                .insert(project_id.clone(), default_base_branch);
        } else {
            self.default_base_branch_by_project.remove(&project_id);
        }

        let selected_worktree_id = selected_worktree_from_state(
            &project_id,
            &scoped_worktrees,
            self.selected_project_id.as_deref(),
            self.selected_worktree_id.as_deref(),
            state.selected_worktree_id.as_deref(),
            allow_runtime_selection,
        );
        self.worktrees.extend(scoped_worktrees);
        if self.selected_project_id.as_deref() == Some(project_id.as_str()) {
            self.selected_worktree_id = selected_worktree_id.clone();
            self.remember_worktree_selection(&project_id, selected_worktree_id.clone());
        } else if !self
            .selected_worktree_id_by_project
            .contains_key(project_id.as_str())
        {
            self.remember_worktree_selection(&project_id, selected_worktree_id.clone());
        }

        if !allow_runtime_selection {
            return RemoteRuntimePlan {
                state_changed: true,
                ..RemoteRuntimePlan::default()
            };
        }

        let Some(selected_worktree_id) = selected_worktree_id else {
            return RemoteRuntimePlan {
                state_changed: true,
                ..RemoteRuntimePlan::default()
            };
        };
        if !self.worktrees.iter().any(|worktree| {
            worktree.project_id == project_id && worktree.id == selected_worktree_id
        }) {
            return RemoteRuntimePlan {
                state_changed: true,
                ..RemoteRuntimePlan::default()
            };
        }
        let mut plan = self.apply_worktree_selected(
            Some(project_id),
            Some(selected_worktree_id),
            terminal_visible,
            terminal_list_loaded,
        );
        plan.state_changed = true;
        plan
    }

    fn apply_all_worktree_state(
        &mut self,
        state: RemoteRuntimeWorktreeState,
        allow_runtime_selection: bool,
        terminal_visible: bool,
        terminal_list_loaded: bool,
    ) -> RemoteRuntimePlan {
        self.worktrees = state.worktrees;
        self.base_branches_by_project.clear();
        self.default_base_branch_by_project.clear();
        let selected_project_id = self.selected_project_id.clone();
        let mut valid_project_ids = self
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        for worktree in &self.worktrees {
            valid_project_ids.insert(worktree.project_id.as_str());
        }
        self.selected_worktree_id_by_project
            .retain(|project_id, _| valid_project_ids.contains(project_id.as_str()));
        if let Some(project_id) = selected_project_id.as_deref() {
            let selected_worktree_id = selected_worktree_from_state(
                project_id,
                &self.worktrees,
                Some(project_id),
                self.selected_worktree_id.as_deref(),
                state.selected_worktree_id.as_deref(),
                allow_runtime_selection,
            )
            .or_else(|| self.default_worktree_selection_for_project(project_id));
            self.selected_worktree_id = selected_worktree_id.clone();
            self.remember_worktree_selection(project_id, selected_worktree_id.clone());
            if allow_runtime_selection {
                let mut plan = self
                    .ensure_terminal_for_selected_project(terminal_visible, terminal_list_loaded);
                plan.state_changed = true;
                return plan;
            }
        }
        RemoteRuntimePlan {
            state_changed: true,
            ..RemoteRuntimePlan::default()
        }
    }

    fn remember_current_worktree_selection(&mut self) {
        let Some(project_id) = self.selected_project_id.clone() else {
            return;
        };
        self.remember_worktree_selection(&project_id, self.selected_worktree_id.clone());
    }

    fn remember_worktree_selection(&mut self, project_id: &str, worktree_id: Option<String>) {
        if project_id.trim().is_empty() {
            return;
        }
        if let Some(worktree_id) = normalize_worktree_scope(project_id, worktree_id) {
            self.selected_worktree_id_by_project
                .insert(project_id.to_string(), worktree_id);
        } else {
            self.selected_worktree_id_by_project
                .insert(project_id.to_string(), project_id.to_string());
        }
    }

    fn remembered_worktree_for_project(&self, project_id: &str) -> Option<String> {
        let remembered = self
            .selected_worktree_id_by_project
            .get(project_id)
            .and_then(|value| clean_optional_string(Some(value.clone())))?;
        let has_worktree_list = self
            .worktrees
            .iter()
            .any(|worktree| worktree.project_id == project_id);
        if has_worktree_list
            && !self
                .worktrees
                .iter()
                .any(|worktree| worktree.project_id == project_id && worktree.id == remembered)
        {
            return None;
        }
        Some(remembered)
    }

    fn valid_worktree_selection_for_project(
        &self,
        project_id: &str,
        worktree_id: Option<String>,
    ) -> Option<String> {
        let requested = normalize_worktree_scope(project_id, worktree_id)?;
        let has_worktree_list = self
            .worktrees
            .iter()
            .any(|worktree| worktree.project_id == project_id);
        if !has_worktree_list {
            return Some(requested);
        }
        if self
            .worktrees
            .iter()
            .any(|worktree| worktree.project_id == project_id && worktree.id == requested)
        {
            return Some(requested);
        }
        selected_worktree_from_state(
            project_id,
            &self.worktrees,
            Some(project_id),
            self.selected_worktree_id.as_deref(),
            self.selected_worktree_id_by_project
                .get(project_id)
                .map(String::as_str),
            false,
        )
    }

    fn default_worktree_selection_for_project(&self, project_id: &str) -> Option<String> {
        selected_worktree_from_state(
            project_id,
            &self.worktrees,
            Some(project_id),
            None,
            None,
            false,
        )
        .or_else(|| normalize_worktree_scope(project_id, None))
    }

    fn selected_worktree_is_valid_for_project(&self, project_id: &str) -> bool {
        let Some(selected_worktree_id) =
            normalize_worktree_scope(project_id, self.selected_worktree_id.clone())
        else {
            return false;
        };
        let has_worktree_list = self
            .worktrees
            .iter()
            .any(|worktree| worktree.project_id == project_id);
        !has_worktree_list
            || self.worktrees.iter().any(|worktree| {
                worktree.project_id == project_id && worktree.id == selected_worktree_id
            })
    }

    fn prune_worktree_selections(&mut self) {
        let valid_projects = self
            .projects
            .iter()
            .map(|project| project.id.as_str())
            .collect::<std::collections::HashSet<_>>();
        self.selected_worktree_id_by_project
            .retain(|project_id, _| valid_projects.contains(project_id.as_str()));
    }

    fn pending_terminal_create_matches(&self, terminal: &RemoteRuntimeTerminal) -> bool {
        let Some(request) = self.pending_terminal_create_request.as_ref() else {
            return false;
        };
        is_accessible_terminal(terminal)
            && terminal.id == request.terminal_id
            && terminal.project_id == request.project_id
            && terminal_matches_selected_worktree(terminal, request.worktree_id.as_deref())
    }

    fn focus_created_terminal(&mut self, terminal: &RemoteRuntimeTerminal) {
        self.selected_worktree_id =
            normalize_terminal_worktree_scope(&terminal.project_id, terminal.worktree_id.clone());
        self.remember_worktree_selection(&terminal.project_id, self.selected_worktree_id.clone());
        self.last_terminal_id_by_project.insert(
            terminal_memory_key(&terminal.project_id, self.selected_worktree_id.as_deref()),
            terminal.id.clone(),
        );
        self.selected_project_id = Some(terminal.project_id.clone());
        self.active_session_id = Some(terminal.id.clone());
    }

    fn clear_pending_created_terminal(&mut self) {
        self.pending_created_terminal = None;
        self.pending_created_terminal_confirmed = false;
    }
}

fn default_true() -> bool {
    true
}

fn clean_optional_string(value: Option<String>) -> Option<String> {
    value.and_then(clean_nonempty_string)
}

fn clean_nonempty_string(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn clean_nonempty_str(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn selected_project_from_list(
    projects: &[RemoteRuntimeProject],
    pending_project_select_id: Option<&str>,
    remote_selected_project_id: Option<&str>,
    current_selected_project_id: Option<&str>,
    prefer_current_project: bool,
) -> Option<String> {
    let pending = pending_project_select_id.and_then(|value| {
        let value = value.trim();
        if !value.is_empty() && projects.iter().any(|item| item.id == value) {
            Some(value.to_string())
        } else {
            None
        }
    });
    if pending.is_some() {
        return pending;
    }
    if prefer_current_project
        && let Some(current) = current_selected_project_id
        && projects.iter().any(|item| item.id == current)
    {
        return Some(current.to_string());
    }
    let remote = remote_selected_project_id.and_then(|value| {
        let value = value.trim();
        if !value.is_empty() && projects.iter().any(|item| item.id == value) {
            Some(value.to_string())
        } else {
            None
        }
    });
    if remote.is_some() {
        return remote;
    }
    if prefer_current_project
        && let Some(current) = current_selected_project_id
        && projects.iter().any(|item| item.id == current)
    {
        return Some(current.to_string());
    }
    projects.first().map(|item| item.id.clone())
}

fn selected_worktree_from_state(
    project_id: &str,
    worktrees: &[RemoteRuntimeWorktree],
    selected_project_id: Option<&str>,
    current_selected_worktree_id: Option<&str>,
    remote_selected_worktree_id: Option<&str>,
    prefer_remote_selection: bool,
) -> Option<String> {
    let find = |candidate: Option<&str>| {
        let candidate = normalize_worktree_scope(project_id, candidate.map(str::to_string))?;
        worktrees
            .iter()
            .any(|worktree| worktree.project_id == project_id && worktree.id == candidate)
            .then_some(candidate)
    };
    if prefer_remote_selection && let Some(remote) = find(remote_selected_worktree_id) {
        return Some(remote);
    }
    if selected_project_id == Some(project_id)
        && let Some(current) = find(current_selected_worktree_id)
    {
        return Some(current);
    }
    if let Some(remote) = find(remote_selected_worktree_id) {
        return Some(remote);
    }
    worktrees
        .iter()
        .find(|worktree| worktree.project_id == project_id && worktree.is_default)
        .or_else(|| {
            worktrees
                .iter()
                .find(|worktree| worktree.project_id == project_id && worktree.id == project_id)
        })
        .or_else(|| {
            worktrees
                .iter()
                .find(|worktree| worktree.project_id == project_id)
        })
        .map(|worktree| worktree.id.clone())
}

fn is_accessible_terminal(terminal: &RemoteRuntimeTerminal) -> bool {
    !terminal.id.is_empty()
        && !terminal.project_id.is_empty()
        && !terminal.status.as_deref().is_some_and(|status| {
            matches!(
                status.trim().to_ascii_lowercase().as_str(),
                "exited" | "closed"
            )
        })
}

fn accessible_terminals_for_project_and_worktree<'a>(
    terminals: &'a [RemoteRuntimeTerminal],
    project_id: &str,
    worktree_id: Option<&str>,
) -> Vec<&'a RemoteRuntimeTerminal> {
    terminals
        .iter()
        .filter(|item| {
            item.project_id == project_id
                && terminal_matches_selected_worktree(item, worktree_id)
                && is_accessible_terminal(item)
        })
        .collect()
}

fn active_terminal_matches_scope_in(
    active_session_id: Option<&str>,
    terminals: &[RemoteRuntimeTerminal],
    project_id: &str,
    worktree_id: Option<&str>,
) -> bool {
    let Some(active_session_id) = active_session_id else {
        return false;
    };
    terminals.iter().any(|item| {
        item.id == active_session_id
            && item.project_id == project_id
            && terminal_matches_selected_worktree(item, worktree_id)
            && is_accessible_terminal(item)
    })
}

fn preferred_terminal_for_project<'a>(
    last_terminal_id_by_project: &HashMap<String, String>,
    project_id: &str,
    worktree_id: Option<&str>,
    terminals: &'a [&'a RemoteRuntimeTerminal],
) -> Option<&'a RemoteRuntimeTerminal> {
    let mut list = terminals.to_vec();
    list.sort_by(|left, right| compare_remote_terminals(left, right));
    let memory_key = terminal_memory_key(project_id, worktree_id);
    if let Some(remembered_id) = last_terminal_id_by_project.get(&memory_key)
        && let Some(terminal) = list.iter().find(|terminal| terminal.id == *remembered_id)
    {
        return Some(*terminal);
    }
    list.first().copied()
}

fn compare_remote_terminals(
    left: &RemoteRuntimeTerminal,
    right: &RemoteRuntimeTerminal,
) -> std::cmp::Ordering {
    left.layout_order
        .unwrap_or(usize::MAX)
        .cmp(&right.layout_order.unwrap_or(usize::MAX))
        .then_with(|| {
            left.created_at
                .as_deref()
                .unwrap_or_default()
                .cmp(right.created_at.as_deref().unwrap_or_default())
        })
        .then_with(|| left.id.cmp(&right.id))
}

fn terminal_matches_selected_worktree(
    terminal: &RemoteRuntimeTerminal,
    selected_worktree_id: Option<&str>,
) -> bool {
    let Some(project_id) = clean_optional_string(Some(terminal.project_id.clone())) else {
        return false;
    };
    let Some(selected_worktree_id) =
        normalize_worktree_scope(&project_id, selected_worktree_id.map(str::to_string))
    else {
        return false;
    };
    normalize_terminal_worktree_scope(&project_id, terminal.worktree_id.clone()).as_deref()
        == Some(selected_worktree_id.as_str())
}

fn terminal_memory_key(project_id: &str, worktree_id: Option<&str>) -> String {
    runtime_scope_key(project_id, worktree_id)
}

pub fn runtime_scope_key(project_id: &str, worktree_id: Option<&str>) -> String {
    let project_id = project_id.trim();
    let worktree_id = normalize_worktree_scope(project_id, worktree_id.map(str::to_string))
        .unwrap_or_else(|| project_id.to_string());
    format!("{project_id}::{worktree_id}")
}

pub fn runtime_scope_parts(scope_key: &str) -> Option<(&str, &str)> {
    let (project_id, worktree_id) = scope_key.split_once("::")?;
    let project_id = project_id.trim();
    let worktree_id = worktree_id.trim();
    if project_id.is_empty() || worktree_id.is_empty() {
        return None;
    }
    Some((project_id, worktree_id))
}

fn normalize_worktree_scope(project_id: &str, worktree_id: Option<String>) -> Option<String> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return None;
    }
    clean_optional_string(worktree_id).or_else(|| Some(project_id.to_string()))
}

fn normalize_terminal_worktree_scope(
    project_id: &str,
    worktree_id: Option<String>,
) -> Option<String> {
    normalize_worktree_scope(project_id, worktree_id)
}

fn ensure_default_worktree_scope(
    project_id: &str,
    mut worktrees: Vec<RemoteRuntimeWorktree>,
) -> Vec<RemoteRuntimeWorktree> {
    let project_id = project_id.trim();
    if project_id.is_empty() {
        return worktrees;
    }
    if worktrees
        .iter()
        .any(|worktree| worktree.project_id == project_id && worktree.id == project_id)
    {
        return worktrees;
    }
    if let Some(default) = worktrees
        .iter()
        .find(|worktree| worktree.project_id == project_id && worktree.is_default)
        .cloned()
    {
        let mut default_scope = default;
        default_scope.id = project_id.to_string();
        default_scope.is_default = true;
        worktrees.insert(0, default_scope);
    }
    worktrees
}
