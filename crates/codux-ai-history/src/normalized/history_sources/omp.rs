use super::*;

pub const DRIVER: HistorySourceDriver =
    file_history_source_driver("omp", omp_history_paths, parse_omp_history_file);

fn omp_history_paths(project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    omp_session_paths(&project.path, home)
}
