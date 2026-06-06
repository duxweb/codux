use super::*;

pub const DRIVER: HistorySourceDriver = file_history_source_driver(
    "codewhale",
    codewhale_history_paths,
    parse_codewhale_history_file,
);

fn codewhale_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    codewhale_session_paths(&project.path, home)
}
