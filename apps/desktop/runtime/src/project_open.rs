use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpenApplicationSummary {
    pub id: String,
    pub label: String,
    pub category: String,
    pub installed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ProjectOpenApplicationRequest {
    pub project_path: String,
    pub application_id: String,
}

struct ProjectOpenApplicationSpec {
    id: &'static str,
    label: &'static str,
    category: &'static str,
    bundle_ids: &'static [&'static str],
    #[cfg(not(target_os = "macos"))]
    commands: &'static [&'static str],
}

#[cfg(target_os = "macos")]
macro_rules! project_open_application_spec {
    ($id:expr, $label:expr, $category:expr, $bundle_ids:expr, $commands:expr) => {
        ProjectOpenApplicationSpec {
            id: $id,
            label: $label,
            category: $category,
            bundle_ids: $bundle_ids,
        }
    };
}

#[cfg(not(target_os = "macos"))]
macro_rules! project_open_application_spec {
    ($id:expr, $label:expr, $category:expr, $bundle_ids:expr, $commands:expr) => {
        ProjectOpenApplicationSpec {
            id: $id,
            label: $label,
            category: $category,
            bundle_ids: $bundle_ids,
            commands: $commands,
        }
    };
}

const PROJECT_OPEN_APPLICATIONS: &[ProjectOpenApplicationSpec] = &[
    project_open_application_spec!(
        "vscode",
        "VS Code",
        "primary",
        &["com.microsoft.VSCode"],
        &["code"]
    ),
    project_open_application_spec!(
        "terminal",
        "Terminal",
        "primary",
        &["com.apple.Terminal"],
        &[
            "x-terminal-emulator",
            "gnome-terminal",
            "konsole",
            "xfce4-terminal"
        ]
    ),
    project_open_application_spec!(
        "iterm",
        "iTerm2",
        "primary",
        &["com.googlecode.iterm2"],
        &["iterm2"]
    ),
    project_open_application_spec!(
        "ghostty",
        "Ghostty",
        "primary",
        &["com.mitchellh.ghostty"],
        &["ghostty"]
    ),
    project_open_application_spec!(
        "xcode",
        "Xcode",
        "primary",
        &["com.apple.dt.Xcode"],
        &["xed"]
    ),
    project_open_application_spec!(
        "intellijIdea",
        "IntelliJ IDEA",
        "ide",
        &["com.jetbrains.intellij", "com.jetbrains.intellij.ce"],
        &["idea", "idea64"]
    ),
    project_open_application_spec!(
        "webStorm",
        "WebStorm",
        "ide",
        &["com.jetbrains.WebStorm"],
        &["webstorm"]
    ),
    project_open_application_spec!(
        "phpStorm",
        "PhpStorm",
        "ide",
        &["com.jetbrains.PhpStorm"],
        &["phpstorm"]
    ),
    project_open_application_spec!(
        "pyCharm",
        "PyCharm",
        "ide",
        &["com.jetbrains.pycharm", "com.jetbrains.pycharm.ce"],
        &["pycharm"]
    ),
    project_open_application_spec!(
        "goLand",
        "GoLand",
        "ide",
        &["com.jetbrains.goland"],
        &["goland"]
    ),
    project_open_application_spec!(
        "clion",
        "CLion",
        "ide",
        &["com.jetbrains.CLion"],
        &["clion"]
    ),
    project_open_application_spec!(
        "rider",
        "Rider",
        "ide",
        &["com.jetbrains.rider"],
        &["rider"]
    ),
    project_open_application_spec!(
        "androidStudio",
        "Android Studio",
        "ide",
        &["com.google.android.studio"],
        &["studio", "android-studio"]
    ),
    project_open_application_spec!(
        "cursor",
        "Cursor",
        "ide",
        &["com.todesktop.230313mzl4w4u92", "com.yuxin.CursorPro"],
        &["cursor"]
    ),
    project_open_application_spec!("zed", "Zed", "ide", &["dev.zed.Zed"], &["zed"]),
    project_open_application_spec!(
        "sublimeText",
        "Sublime Text",
        "ide",
        &["com.sublimetext.4", "com.sublimetext.3"],
        &["subl", "sublime_text"]
    ),
    project_open_application_spec!(
        "windsurf",
        "Windsurf",
        "ide",
        &["com.exafunction.windsurf"],
        &["windsurf"]
    ),
];

pub fn project_open_applications() -> Vec<ProjectOpenApplicationSummary> {
    PROJECT_OPEN_APPLICATIONS
        .iter()
        .map(|spec| ProjectOpenApplicationSummary {
            id: spec.id.to_string(),
            label: spec.label.to_string(),
            category: spec.category.to_string(),
            installed: project_open_application_installed(spec),
        })
        .collect()
}

pub fn project_open_in_application(request: ProjectOpenApplicationRequest) -> Result<(), String> {
    let path = PathBuf::from(request.project_path.trim());
    if !path.is_dir() {
        return Err("Project path does not exist.".to_string());
    }

    let spec = PROJECT_OPEN_APPLICATIONS
        .iter()
        .find(|item| item.id == request.application_id)
        .ok_or_else(|| "Unsupported application.".to_string())?;

    open_project_path_in_application(&path, spec)
}

pub fn project_reveal_in_file_manager(project_path: &str) -> Result<(), String> {
    let path = PathBuf::from(project_path.trim());
    if !path.exists() {
        return Err("Project path does not exist.".to_string());
    }

    #[cfg(target_os = "macos")]
    {
        return Command::new("open")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string());
    }

    #[cfg(target_os = "windows")]
    {
        return Command::new("explorer")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string());
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        Command::new("xdg-open")
            .arg(&path)
            .spawn()
            .map(|_| ())
            .map_err(|error| error.to_string())
    }
}

#[cfg(target_os = "macos")]
fn project_open_application_url(spec: &ProjectOpenApplicationSpec) -> Option<String> {
    spec.bundle_ids.iter().find_map(|bundle_id| {
        Command::new("mdfind")
            .arg(format!("kMDItemCFBundleIdentifier == '{bundle_id}'"))
            .output()
            .ok()
            .and_then(|output| {
                if !output.status.success() {
                    return None;
                }
                String::from_utf8_lossy(&output.stdout)
                    .lines()
                    .map(str::trim)
                    .find(|line| !line.is_empty())
                    .map(ToOwned::to_owned)
            })
    })
}

fn project_open_application_installed(spec: &ProjectOpenApplicationSpec) -> bool {
    #[cfg(target_os = "macos")]
    {
        project_open_application_url(spec).is_some()
    }

    #[cfg(not(target_os = "macos"))]
    {
        spec.commands.iter().any(|command| command_in_path(command))
    }
}

fn open_project_path_in_application(
    path: &Path,
    spec: &ProjectOpenApplicationSpec,
) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        for bundle_id in spec.bundle_ids {
            if Command::new("open")
                .args(["-b", bundle_id, &path.display().to_string()])
                .status()
                .map(|status| status.success())
                .unwrap_or(false)
            {
                return Ok(());
            }
        }

        if spec.id == "vscode" {
            let url = format!("vscode://file{}", path.display());
            return Command::new("open")
                .arg(url)
                .spawn()
                .map(|_| ())
                .map_err(|error| error.to_string());
        }

        return Err(format!("{} not found.", spec.label));
    }

    #[cfg(not(target_os = "macos"))]
    {
        for command in spec.commands {
            if command_in_path(command) {
                return Command::new(command)
                    .arg(path)
                    .spawn()
                    .map(|_| ())
                    .map_err(|error| error.to_string());
            }
        }

        Err(format!("{} not found.", spec.label))
    }
}

#[cfg(not(target_os = "macos"))]
fn command_in_path(command: &str) -> bool {
    let path = std::env::var_os("PATH").unwrap_or_default();
    std::env::split_paths(&path).any(|dir| {
        let candidate = dir.join(command);
        if candidate.is_file() {
            return true;
        }

        #[cfg(target_os = "windows")]
        {
            return ["exe", "cmd", "bat"]
                .iter()
                .any(|extension| dir.join(format!("{command}.{extension}")).is_file());
        }

        #[cfg(not(target_os = "windows"))]
        false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_open_applications_match_tauri_ids() {
        let ids = PROJECT_OPEN_APPLICATIONS
            .iter()
            .map(|app| app.id)
            .collect::<Vec<_>>();

        assert_eq!(ids.first(), Some(&"vscode"));
        assert!(ids.contains(&"cursor"));
        assert!(ids.contains(&"windsurf"));
    }
}
