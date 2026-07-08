use super::*;

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPtyConfig {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub command: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub scrollback_lines: Option<usize>,
    pub env: Option<HashMap<String, String>>,
    pub root_project_id: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub worktree_id: Option<String>,
    pub title: Option<String>,
    pub tool: Option<String>,
    /// When set, the terminal runs on this remote device over the controller.
    pub host_device_id: Option<String>,
    pub support_dir: Option<PathBuf>,
    pub runtime_root: Option<PathBuf>,
    pub session_instance_id: Option<String>,
    pub tool_permissions_file: Option<PathBuf>,
    pub memory_workspace_root: Option<PathBuf>,
    pub memory_prompt_file: Option<PathBuf>,
    pub memory_index_file: Option<PathBuf>,
}

impl From<TerminalLaunchConfig> for TerminalPtyConfig {
    fn from(config: TerminalLaunchConfig) -> Self {
        Self {
            cwd: config.cwd,
            shell: config.shell,
            command: config.command,
            cols: config.cols,
            rows: config.rows,
            scrollback_lines: config.scrollback_lines,
            env: config.env,
            root_project_id: None,
            project_id: config.project_id,
            project_name: config.project_name,
            terminal_id: config.terminal_id,
            slot_id: config.slot_id,
            session_key: config.session_key,
            title: config.title,
            tool: config.tool,
            worktree_id: config.worktree_id,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug)]
pub struct TerminalLaunchContext {
    pub root_project_id: String,
    pub project_id: String,
    pub project_name: String,
    pub project_path: PathBuf,
    pub support_dir: PathBuf,
    pub runtime_root: PathBuf,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub session_title: Option<String>,
    pub session_cwd: Option<PathBuf>,
    pub session_instance_id: Option<String>,
    pub tool_permissions_file: Option<PathBuf>,
    pub memory_workspace_root: Option<PathBuf>,
    pub memory_prompt_file: Option<PathBuf>,
    pub memory_index_file: Option<PathBuf>,
    pub host_device_id: Option<String>,
}

impl TerminalLaunchContext {
    pub fn to_config(&self) -> TerminalPtyConfig {
        TerminalPtyConfig {
            cwd: Some(
                self.session_cwd
                    .as_ref()
                    .unwrap_or(&self.project_path)
                    .display()
                    .to_string(),
            ),
            project_id: Some(self.project_id.clone()),
            root_project_id: Some(self.root_project_id.clone()),
            worktree_id: Some(self.project_id.clone()),
            project_name: Some(self.project_name.clone()),
            terminal_id: self.terminal_id.clone(),
            slot_id: self.slot_id.clone(),
            session_key: self.session_key.clone(),
            title: self.session_title.clone(),
            support_dir: Some(self.support_dir.clone()),
            runtime_root: Some(self.runtime_root.clone()),
            session_instance_id: self.session_instance_id.clone(),
            tool_permissions_file: self.tool_permissions_file.clone(),
            memory_workspace_root: self.memory_workspace_root.clone(),
            memory_prompt_file: self.memory_prompt_file.clone(),
            memory_index_file: self.memory_index_file.clone(),
            host_device_id: self.host_device_id.clone(),
            scrollback_lines: None,
            ..Default::default()
        }
    }
}

#[derive(Clone, Debug, Default)]
pub(super) struct RequestedTerminalIdentity {
    pub(super) cwd: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) session_key: Option<String>,
}

impl RequestedTerminalIdentity {
    pub(super) fn from_config(
        config: &TerminalPtyConfig,
        context: Option<&TerminalLaunchContext>,
    ) -> Self {
        Self {
            cwd: requested_terminal_cwd(config, context),
            project_id: config
                .project_id
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| context.map(|context| context.project_id.clone())),
            session_key: config
                .session_key
                .clone()
                .filter(|value| !value.trim().is_empty())
                .or_else(|| context.and_then(|context| context.session_key.clone())),
        }
    }
}

pub(super) fn normalize_terminal_cwd(cwd: Option<String>) -> Option<String> {
    cwd.and_then(|value| {
        let trimmed = value.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(normalize_terminal_path(trimmed))
        }
    })
}

pub(super) fn requested_terminal_cwd(
    config: &TerminalPtyConfig,
    context: Option<&TerminalLaunchContext>,
) -> Option<String> {
    normalize_terminal_cwd(
        config
            .cwd
            .clone()
            .or_else(|| {
                context.and_then(|context| {
                    context
                        .session_cwd
                        .as_ref()
                        .map(|cwd| cwd.display().to_string())
                })
            })
            .or_else(|| context.map(|context| context.project_path.display().to_string())),
    )
}
