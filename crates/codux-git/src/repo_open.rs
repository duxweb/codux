pub(crate) fn git_path(path: &str) -> PathBuf {
    PathBuf::from(strip_windows_extended_prefix(path.trim()))
}

fn strip_windows_extended_prefix(path: &str) -> String {
    codux_runtime_core::path::display_path(path)
}

pub fn discover_repository(path: &str) -> Result<git2::Repository, git2::Error> {
    let path = git_path(path);
    GitRepository::discover(&path)
}

fn discover_git_repository(path: &str) -> Result<GitRepository, git2::Error> {
    discover_repository(path)
}

pub fn normalize_repository_path(path: &str) -> String {
    let path = git_path(path);
    path.canonicalize()
        .unwrap_or(path)
        .display()
        .to_string()
}

#[cfg(test)]
mod repo_open_tests {
    use super::*;

    #[test]
    fn strips_windows_extended_path_prefix() {
        assert_eq!(
            strip_windows_extended_prefix(r"\\?\F:\codux-gpui"),
            r"F:\codux-gpui"
        );
        assert_eq!(
            strip_windows_extended_prefix(r"\\?\UNC\server\share"),
            r"\\server\share"
        );
        assert_eq!(
            strip_windows_extended_prefix(r"F:\codux-gpui"),
            r"F:\codux-gpui"
        );
    }

}
