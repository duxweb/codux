use std::path::{Path, PathBuf};

pub const RUNTIME_ROOT_DIR_NAME: &str = "runtime-root";
pub const RUNTIME_EVENT_DIR_NAME: &str = "runtime-events";
pub const RUNTIME_SUPPORT_DIR_NAME: &str = "runtime-support";
pub const RUNTIME_LOG_FILE_NAME: &str = "runtime-rust.log";
pub const LIVE_LOG_FILE_NAME: &str = "live-rust.log";
pub const RUNTIME_LOG_PREVIEW_FILE_NAME: &str = "runtime-log-preview.txt";
pub const CLAUDE_SESSION_MAP_DIR_NAME: &str = "claude-session-map";
pub const OPENCODE_SESSION_MAP_DIR_NAME: &str = "opencode-session-map";

pub fn app_support_dir() -> PathBuf {
    app_support_candidates()
        .into_iter()
        .find(|path| path.join("state.json").is_file() || path.join("settings.json").is_file())
        .unwrap_or_else(default_app_support_dir)
}

pub fn runtime_temp_dir() -> PathBuf {
    std::env::temp_dir().join(app_slug())
}

pub fn runtime_log_path() -> PathBuf {
    runtime_log_path_in(&app_support_dir())
}

pub fn live_log_path() -> PathBuf {
    live_log_path_in(&runtime_temp_dir())
}

pub fn runtime_root_dir() -> PathBuf {
    runtime_root_dir_in(&runtime_temp_dir())
}

pub fn runtime_event_dir() -> PathBuf {
    runtime_event_dir_in(&runtime_temp_dir())
}

pub fn runtime_log_preview_path() -> PathBuf {
    runtime_log_preview_path_in(&runtime_temp_dir())
}

pub fn runtime_support_dir() -> PathBuf {
    runtime_support_dir_in(&app_support_dir())
}

pub fn claude_session_map_dir() -> PathBuf {
    claude_session_map_dir_in(&runtime_temp_dir())
}

pub fn opencode_session_map_dir() -> PathBuf {
    opencode_session_map_dir_in(&runtime_temp_dir())
}

pub fn runtime_log_path_in(app_support_dir: &Path) -> PathBuf {
    app_support_dir.join(RUNTIME_LOG_FILE_NAME)
}

pub fn live_log_path_in(runtime_temp_dir: &Path) -> PathBuf {
    runtime_temp_dir.join(LIVE_LOG_FILE_NAME)
}

pub fn runtime_root_dir_in(runtime_temp_dir: &Path) -> PathBuf {
    runtime_temp_dir.join(RUNTIME_ROOT_DIR_NAME)
}

pub fn runtime_event_dir_in(runtime_temp_dir: &Path) -> PathBuf {
    runtime_temp_dir.join(RUNTIME_EVENT_DIR_NAME)
}

pub fn runtime_log_preview_path_in(runtime_temp_dir: &Path) -> PathBuf {
    runtime_temp_dir.join(RUNTIME_LOG_PREVIEW_FILE_NAME)
}

pub fn runtime_support_dir_in(app_support_dir: &Path) -> PathBuf {
    app_support_dir.join(RUNTIME_SUPPORT_DIR_NAME)
}

pub fn claude_session_map_dir_in(runtime_temp_dir: &Path) -> PathBuf {
    runtime_temp_dir.join(CLAUDE_SESSION_MAP_DIR_NAME)
}

pub fn opencode_session_map_dir_in(runtime_temp_dir: &Path) -> PathBuf {
    runtime_temp_dir.join(OPENCODE_SESSION_MAP_DIR_NAME)
}

pub fn app_display_name() -> &'static str {
    if cfg!(debug_assertions) {
        "Codux Dev"
    } else {
        "Codux"
    }
}

pub fn app_slug() -> &'static str {
    if cfg!(debug_assertions) {
        "codux-dev"
    } else {
        "codux"
    }
}

pub fn app_support_candidates() -> Vec<PathBuf> {
    let home = home_dir();

    #[cfg(target_os = "macos")]
    {
        if cfg!(debug_assertions) {
            return vec![home.join("Library/Application Support/Codux Dev")];
        }
        return vec![home.join("Library/Application Support/Codux")];
    }

    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"));
        if cfg!(debug_assertions) {
            return vec![base.join("Codux Dev")];
        }
        return vec![base.join("Codux")];
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        if cfg!(debug_assertions) {
            vec![base.join("Codux Dev")]
        } else {
            vec![base.join("Codux")]
        }
    }
}

pub fn default_app_support_dir() -> PathBuf {
    let mut candidates = app_support_candidates();
    candidates
        .drain(..)
        .next()
        .unwrap_or_else(|| home_dir().join(".codux"))
}

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(windows_user_profile)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "windows")]
fn windows_user_profile() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let drive = std::env::var_os("HOMEDRIVE")?;
            let path = std::env::var_os("HOMEPATH")?;
            let mut home = PathBuf::from(drive);
            home.push(path);
            Some(home)
        })
}

#[cfg(not(target_os = "windows"))]
fn windows_user_profile() -> Option<PathBuf> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_name_matches_build_profile() {
        if cfg!(debug_assertions) {
            assert_eq!(app_display_name(), "Codux Dev");
        } else {
            assert_eq!(app_display_name(), "Codux");
        }
    }

    #[test]
    fn slug_matches_build_profile() {
        if cfg!(debug_assertions) {
            assert_eq!(app_slug(), "codux-dev");
            assert!(runtime_temp_dir().ends_with("codux-dev"));
        } else {
            assert_eq!(app_slug(), "codux");
            assert!(runtime_temp_dir().ends_with("codux"));
        }
    }

    #[test]
    fn support_dir_matches_build_profile() {
        let candidates = app_support_candidates();
        assert_eq!(candidates.len(), 1);
        if cfg!(debug_assertions) {
            assert!(candidates[0].ends_with("Codux Dev"));
        } else {
            assert!(candidates[0].ends_with("Codux"));
        }
    }
}
