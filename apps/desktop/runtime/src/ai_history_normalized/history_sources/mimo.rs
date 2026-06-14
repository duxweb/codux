use super::*;

pub const DRIVER: HistorySourceDriver =
    file_history_source_driver("mimo", mimo_history_paths, parse_opencode_history_file);

fn mimo_history_paths(_project: &AIHistoryProjectRequest, home: &Path) -> Vec<PathBuf> {
    [
        home.join(".local")
            .join("share")
            .join("mimo")
            .join("opencode.db"),
        home.join(".local")
            .join("share")
            .join("mimo-code")
            .join("opencode.db"),
        home.join(".local")
            .join("share")
            .join("mimo-code")
            .join("mimo.db"),
        home.join(".mimo").join("share").join("opencode.db"),
        home.join(".mimo-code").join("share").join("opencode.db"),
    ]
    .into_iter()
    .filter(|path| path.exists())
    .collect()
}
