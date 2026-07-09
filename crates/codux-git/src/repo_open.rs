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

pub fn git_repository_owner_mismatch(error: &str) -> bool {
    error.contains("is not owned by current user")
}

fn safe_directory_path(path: &str) -> String {
    let path = git_path(path);
    let path = find_repository_root(&path)
        .unwrap_or_else(|| path.clone())
        .canonicalize()
        .unwrap_or(path);
    git_config_path(&path.display().to_string())
}

fn git_config_path(path: &str) -> String {
    codux_runtime_core::path::display_path(path)
        .replace('\\', "/")
}

fn find_repository_root(path: &Path) -> Option<PathBuf> {
    let mut current = if path.is_file() {
        path.parent()?.to_path_buf()
    } else {
        path.to_path_buf()
    };
    loop {
        if current.join(".git").exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
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

    #[test]
    fn formats_safe_directory_for_git_config() {
        assert_eq!(git_config_path(r"\\?\F:\codux-gpui"), "F:/codux-gpui");
        assert_eq!(git_config_path("/Volumes/Web/codux"), "/Volumes/Web/codux");
    }

}
