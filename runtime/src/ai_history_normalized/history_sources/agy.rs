use super::*;

pub const DRIVER: HistorySourceDriver =
    file_history_source_driver("agy", agy_history_paths, parse_agy_history_file);

fn agy_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    agy_session_paths(&project.path, home)
}
