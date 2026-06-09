use std::{path::Path, process::Command};

pub(super) fn move_to_trash(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        let script = format!(
            "tell application \"Finder\" to delete POSIX file \"{}\"",
            apple_script_string(path)
        );
        return run_command_status("osascript", &["-e", &script], "move item to Trash");
    }

    #[cfg(target_os = "windows")]
    {
        let target = powershell_string(path);
        let action = if path.is_dir() {
            format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteDirectory({target}, 'OnlyErrorDialogs', 'SendToRecycleBin')"
            )
        } else {
            format!(
                "Add-Type -AssemblyName Microsoft.VisualBasic; [Microsoft.VisualBasic.FileIO.FileSystem]::DeleteFile({target}, 'OnlyErrorDialogs', 'SendToRecycleBin')"
            )
        };
        return run_command_status(
            "powershell.exe",
            &["-NoProfile", "-NonInteractive", "-Command", &action],
            "move item to Recycle Bin",
        );
    }

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        if run_command_status(
            "gio",
            &["trash", &path.display().to_string()],
            "move item to Trash",
        )
        .is_ok()
        {
            return Ok(());
        }
        if run_command_status(
            "kioclient6",
            &["move", &path.display().to_string(), "trash:/"],
            "move item to Trash",
        )
        .is_ok()
        {
            return Ok(());
        }
        return run_command_status(
            "kioclient5",
            &["move", &path.display().to_string(), "trash:/"],
            "move item to Trash",
        );
    }
}

pub(super) fn reveal_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return run_spawn_command("open", &["-R", &path.display().to_string()]);
    }
    #[cfg(target_os = "windows")]
    {
        return run_spawn_command("explorer", &["/select,", &path.display().to_string()]);
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        let parent = path.parent().unwrap_or(path);
        return run_spawn_command("xdg-open", &[&parent.display().to_string()]);
    }
}

pub(super) fn open_path(path: &Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        return run_spawn_command("open", &[&path.display().to_string()]);
    }
    #[cfg(target_os = "windows")]
    {
        return run_spawn_command("explorer", &[&path.display().to_string()]);
    }
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    {
        return run_spawn_command("xdg-open", &[&path.display().to_string()]);
    }
}

#[cfg(target_os = "macos")]
fn apple_script_string(path: &Path) -> String {
    path.display()
        .to_string()
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
}

#[cfg(target_os = "windows")]
fn powershell_string(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "''"))
}

fn run_command_status(program: &str, args: &[&str], action: &str) -> Result<(), String> {
    let output = Command::new(program)
        .args(args)
        .output()
        .map_err(|error| format!("Unable to {action}: {error}"))?;
    if output.status.success() {
        return Ok(());
    }
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if stderr.is_empty() {
        Err(format!("Unable to {action}."))
    } else {
        Err(stderr)
    }
}

fn run_spawn_command(program: &str, args: &[&str]) -> Result<(), String> {
    Command::new(program)
        .args(args)
        .spawn()
        .map(|_| ())
        .map_err(|error| error.to_string())
}
