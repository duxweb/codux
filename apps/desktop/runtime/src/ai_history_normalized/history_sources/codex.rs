use super::*;

pub const DRIVER: HistorySourceDriver = jsonl_history_source_driver(
    "codex",
    codex_history_paths,
    parse_codex_history_file_snapshot,
);

fn codex_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    codex_session_paths(&project.path, home)
}
