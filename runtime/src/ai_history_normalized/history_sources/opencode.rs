use super::*;

pub const DRIVER: HistorySourceDriver = file_history_source_driver(
    "opencode",
    opencode_history_paths,
    parse_opencode_history_file,
);

fn opencode_history_paths(_project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    opencode_history_source_paths(home)
}
