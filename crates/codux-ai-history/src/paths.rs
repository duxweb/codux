//! Filesystem locations the AI history engine needs: the user home (to scan
//! each CLI's session directory) and the Codux application-support directory
//! (the default SQLite cache location). Copied from the desktop runtime so the
//! engine carries no desktop-crate dependency.

use std::path::PathBuf;

pub fn home_dir() -> PathBuf {
    std::env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .or_else(windows_user_profile)
        .unwrap_or_else(|| PathBuf::from("."))
}

#[cfg(target_os = "windows")]
fn windows_exe_data_dir() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    Some(exe.parent()?.join("Data"))
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

pub fn app_support_dir() -> PathBuf {
    app_support_candidates()
        .into_iter()
        .find(|path| path.join("state.json").is_file() || path.join("settings.json").is_file())
        .unwrap_or_else(default_app_support_dir)
}

pub fn app_support_candidates() -> Vec<PathBuf> {
    let home = home_dir();

    #[cfg(target_os = "macos")]
    {
        if cfg!(debug_assertions) {
            vec![home.join("Library/Application Support/Codux Dev")]
        } else {
            vec![home.join("Library/Application Support/Codux")]
        }
    }

    #[cfg(target_os = "windows")]
    {
        let base = std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join("AppData").join("Roaming"));
        if cfg!(debug_assertions) {
            vec![base.join("Codux Dev")]
        } else {
            // Installed layout keeps data in Data beside Codux.exe; existing %APPDATA% data keeps winning via the probe.
            let mut candidates = Vec::new();
            if let Some(data) = windows_exe_data_dir() {
                candidates.push(data);
            }
            candidates.push(base.join("Codux"));
            candidates
        }
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
