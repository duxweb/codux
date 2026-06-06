use super::*;

pub const DRIVER: HistorySourceDriver =
    file_history_source_driver("gemini", gemini_history_paths, parse_gemini_history_file);

fn gemini_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    gemini_session_paths(&project.path, home)
}
