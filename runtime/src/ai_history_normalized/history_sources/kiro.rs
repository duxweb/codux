use super::*;

pub const DRIVER: HistorySourceDriver =
    file_history_source_driver("kiro", kiro_history_paths, parse_kiro_history_file);

fn kiro_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    kiro_session_paths(&project.path, home)
}
