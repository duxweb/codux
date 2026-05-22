use std::path::PathBuf;

pub fn app_support_dir() -> PathBuf {
    #[cfg(target_os = "macos")]
    {
        return home_dir()
            .join("Library")
            .join("Application Support")
            .join(app_display_name());
    }
    #[cfg(target_os = "windows")]
    {
        if let Some(appdata) = std::env::var_os("APPDATA") {
            return PathBuf::from(appdata).join(app_display_name());
        }
        return home_dir()
            .join("AppData")
            .join("Roaming")
            .join(app_display_name());
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        if let Some(config_home) = std::env::var_os("XDG_CONFIG_HOME") {
            return PathBuf::from(config_home).join(app_slug());
        }
        return home_dir().join(".config").join(app_slug());
    }
}

pub fn runtime_temp_dir() -> PathBuf {
    std::env::temp_dir().join(app_slug())
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
