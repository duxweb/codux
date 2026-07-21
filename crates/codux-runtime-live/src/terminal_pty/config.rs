use super::*;
use codux_runtime_core::project::is_reserved_project_environment_key;
use codux_runtime_core::runtime_target::RuntimeTarget;
use serde::Serialize;

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TerminalPtyConfig {
    pub cwd: Option<String>,
    pub shell: Option<String>,
    pub command: Option<String>,
    pub cols: Option<u16>,
    pub rows: Option<u16>,
    pub scrollback_lines: Option<usize>,
    pub env: Option<HashMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_env: Option<HashMap<String, String>>,
    pub root_project_id: Option<String>,
    #[serde(default)]
    pub root_project_path: Option<String>,
    pub project_id: Option<String>,
    pub project_name: Option<String>,
    pub terminal_id: Option<String>,
    pub slot_id: Option<String>,
    pub session_key: Option<String>,
    pub worktree_id: Option<String>,
    pub title: Option<String>,
    pub tool: Option<String>,
    pub runtime_target: RuntimeTarget,
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
    pub root_project_path: PathBuf,
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
    pub runtime_target: RuntimeTarget,
    pub environment_variables: HashMap<String, String>,
}

impl TerminalLaunchContext {
    pub fn to_config(&self) -> TerminalPtyConfig {
        let project_env = project_environment_variables(&self.environment_variables);
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
            root_project_path: Some(self.root_project_path.display().to_string()),
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
            runtime_target: self.runtime_target.clone(),
            project_env,
            scrollback_lines: None,
            ..Default::default()
        }
    }
}

fn project_environment_variables(
    variables: &HashMap<String, String>,
) -> Option<HashMap<String, String>> {
    let variables = variables
        .iter()
        .filter_map(|(key, value)| {
            let key = key.trim();
            (!key.is_empty() && !is_reserved_project_environment_key(key))
                .then(|| (key.to_string(), value.clone()))
        })
        .collect::<HashMap<_, _>>();
    (!variables.is_empty()).then_some(variables)
}

pub(super) fn agent_worktree_terminal_scope(
    config: &TerminalPtyConfig,
    context: Option<&TerminalLaunchContext>,
) -> Option<codux_runtime_core::agent_worktree::AgentWorktreeTerminalScope> {
    use codux_runtime_core::agent_worktree::AgentWorktreeTerminalScope;

    let root_project_id = config
        .root_project_id
        .clone()
        .or_else(|| context.map(|context| context.root_project_id.clone()))
        .filter(|value| !value.trim().is_empty())?;
    let root_project_path = config
        .root_project_path
        .clone()
        .or_else(|| context.map(|context| context.root_project_path.display().to_string()))
        .filter(|value| !value.trim().is_empty())?;
    let source_worktree_id = config
        .worktree_id
        .clone()
        .or_else(|| config.project_id.clone())
        .or_else(|| context.map(|context| context.project_id.clone()))
        .filter(|value| !value.trim().is_empty())?;
    let source_worktree_path = requested_terminal_cwd(config, context)?;
    let project_name = config
        .project_name
        .clone()
        .or_else(|| context.map(|context| context.project_name.clone()))
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| "Codux".to_string());

    Some(AgentWorktreeTerminalScope {
        root_project_id,
        root_project_path,
        project_name,
        source_worktree_id,
        source_worktree_path,
        runtime_target: context
            .map(|context| context.runtime_target.clone())
            .unwrap_or_else(|| config.runtime_target.clone()),
    })
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
            Some(codux_runtime_core::path::display_path(trimmed))
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

pub(super) fn terminal_query_colors(config: &TerminalPtyConfig) -> Option<TerminalQueryColors> {
    let env = config.env.as_ref()?;
    Some(TerminalQueryColors {
        foreground: parse_osc_rgb(env.get("DMUX_TERMINAL_OSC_FG")?)?,
        background: parse_osc_rgb(env.get("DMUX_TERMINAL_OSC_BG")?)?,
    })
}

fn parse_osc_rgb(value: &str) -> Option<(u8, u8, u8)> {
    let mut channels = value.trim().strip_prefix("rgb:")?.split('/');
    let red = parse_osc_rgb_channel(channels.next()?)?;
    let green = parse_osc_rgb_channel(channels.next()?)?;
    let blue = parse_osc_rgb_channel(channels.next()?)?;
    channels.next().is_none().then_some((red, green, blue))
}

pub(super) fn apply_terminal_query_colors_env(
    config: &mut TerminalPtyConfig,
    colors: TerminalQueryColors,
) {
    let env = config.env.get_or_insert_with(HashMap::new);
    env.insert(
        "DMUX_TERMINAL_OSC_FG".to_string(),
        format_osc_rgb(colors.foreground),
    );
    env.insert(
        "DMUX_TERMINAL_OSC_BG".to_string(),
        format_osc_rgb(colors.background),
    );
}

fn format_osc_rgb((red, green, blue): (u8, u8, u8)) -> String {
    format!("rgb:{red:02x}{red:02x}/{green:02x}{green:02x}/{blue:02x}{blue:02x}")
}

fn parse_osc_rgb_channel(value: &str) -> Option<u8> {
    let digits = value.len();
    if !(1..=4).contains(&digits) || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return None;
    }
    let raw = u16::from_str_radix(value, 16).ok()? as u32;
    let max = (1_u32 << (digits * 4)) - 1;
    Some(((raw * 255 + max / 2) / max) as u8)
}

#[cfg(test)]
mod cwd_tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn terminal_cwd_preserves_host_path_text() {
        assert_eq!(
            normalize_terminal_cwd(Some("/var/folders/project".to_string())).as_deref(),
            Some("/var/folders/project")
        );
        assert_eq!(
            normalize_terminal_cwd(Some(r"\\?\F:\Projects\Codux".to_string())).as_deref(),
            Some(r"F:\Projects\Codux")
        );
    }

    #[test]
    fn launch_context_carries_project_environment_variables_into_pty_env() {
        let context = TerminalLaunchContext {
            root_project_id: "project-1".to_string(),
            root_project_path: PathBuf::from("/workspace/codux"),
            project_id: "project-1".to_string(),
            project_name: "Codux".to_string(),
            project_path: PathBuf::from("/workspace/codux"),
            support_dir: PathBuf::from("/support/Codux"),
            runtime_root: PathBuf::from("/runtime-root"),
            terminal_id: None,
            slot_id: None,
            session_key: None,
            session_title: None,
            session_cwd: None,
            session_instance_id: None,
            tool_permissions_file: None,
            memory_workspace_root: None,
            memory_prompt_file: None,
            memory_index_file: None,
            runtime_target: Default::default(),
            environment_variables: HashMap::from([
                ("API_BASE".to_string(), "https://example.test".to_string()),
                (" CODUX_PROJECT_ID ".to_string(), "reserved".to_string()),
            ]),
        };

        let config = context.to_config();
        assert_eq!(
            config
                .project_env
                .as_ref()
                .and_then(|env| env.get("API_BASE"))
                .map(String::as_str),
            Some("https://example.test")
        );
        assert!(
            config
                .project_env
                .as_ref()
                .is_none_or(|env| !env.contains_key("CODUX_PROJECT_ID"))
        );
        assert!(config.env.is_none());
    }
}

#[cfg(test)]
mod query_color_tests {
    use super::*;

    #[test]
    fn parses_xterm_dynamic_color_payloads() {
        let config = TerminalPtyConfig {
            env: Some(HashMap::from([
                (
                    "DMUX_TERMINAL_OSC_FG".to_string(),
                    "rgb:2a2a/3131/4040".to_string(),
                ),
                (
                    "DMUX_TERMINAL_OSC_BG".to_string(),
                    "rgb:fafa/fbfb/fcfc".to_string(),
                ),
            ])),
            ..Default::default()
        };

        assert_eq!(
            terminal_query_colors(&config),
            Some(TerminalQueryColors {
                foreground: (0x2a, 0x31, 0x40),
                background: (0xfa, 0xfb, 0xfc),
            })
        );
    }

    #[test]
    fn ignores_incomplete_or_invalid_dynamic_colors() {
        let config = TerminalPtyConfig {
            env: Some(HashMap::from([(
                "DMUX_TERMINAL_OSC_FG".to_string(),
                "rgb:ffff/ffff/ffff".to_string(),
            )])),
            ..Default::default()
        };

        assert_eq!(terminal_query_colors(&config), None);
        assert_eq!(parse_osc_rgb("#ffffff"), None);
        assert_eq!(parse_osc_rgb("rgb:gg/00/00"), None);
    }
}
