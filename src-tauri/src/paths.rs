use std::path::PathBuf;

pub fn app_support_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        return home_dir()
            .join("Library")
            .join("Application Support")
            .join("Codux Tauri");
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join("Codux Tauri");
        }
        return home_dir()
            .join("AppData")
            .join("Roaming")
            .join("Codux Tauri");
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(config_home).join("codux-tauri");
        }
        return home_dir().join(".config").join("codux-tauri");
    }
}

pub fn runtime_temp_dir() -> PathBuf {
    std::env::temp_dir().join("codux-tauri")
}

pub fn home_dir() -> PathBuf {
    std::env::var("HOME")
        .ok()
        .filter(|value| !value.trim().is_empty())
        .map(PathBuf::from)
        .or_else(windows_user_profile)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "windows")]
fn windows_user_profile() -> Option<PathBuf> {
    std::env::var_os("USERPROFILE").map(PathBuf::from)
}

#[cfg(not(target_os = "windows"))]
fn windows_user_profile() -> Option<PathBuf> {
    None
}
