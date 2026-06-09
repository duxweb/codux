use super::*;

pub const DRIVER: HistorySourceDriver = jsonl_history_source_driver(
    "claude",
    claude_history_paths,
    parse_claude_history_file_snapshot,
);

fn claude_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    claude_project_log_paths(&project.path, home)
}
