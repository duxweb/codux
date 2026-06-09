use super::*;

pub const DRIVER: HistorySourceDriver = file_history_source_driver(
    "kimi",
    kimi_history_paths,
    parse_kimi_history_file,
);

fn kimi_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    kimi_session_paths(&project.path, home)
}
