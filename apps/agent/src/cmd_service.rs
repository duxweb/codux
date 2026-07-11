//! `codux install` / `uninstall` / `stop` / `status`.
//!
//! Install registers `codux start` with the platform service manager (launchd on
//! macOS, systemd --user on Linux, Task Scheduler on Windows) so the host comes
//! up at login/boot and is restarted if it exits.

use std::process::Command;

use crate::{device_store, paths, runstate};

const SERVICE_LABEL: &str = "com.codux.host";

pub fn status() -> Result<(), String> {
    let running = runstate::is_running();
    let devices = device_store::list().len();
    println!(
        "Codux host: {}",
        if running { "running" } else { "stopped" }
    );
    if running && let Some(status) = runstate::read_status() {
        println!("  started:  {}", status.started_at);
        println!("  device:   {}", status.device_name);
        if !status.node_id.is_empty() {
            println!("  node:     {}", status.node_id);
        }
        if !status.web_test_url.is_empty() {
            println!("  web:      {}", status.web_test_url);
        }
    }
    println!("  paired:   {devices} device(s)");
    println!("  config:   {}", paths::config_path().display());
    if let Ok(exe) = std::env::current_exe() {
        println!("  binary:   {}", exe.display());
    }
    Ok(())
}

pub fn stop() -> Result<(), String> {
    let Some(status) = runstate::read_status() else {
        if runstate::is_running() {
            return Err("host is running but its status file is missing".to_string());
        }
        println!("Codux host is not running.");
        return Ok(());
    };
    kill_pid(status.pid)?;
    runstate::clear_status();
    runstate::clear_ticket();
    println!("Stopped Codux host (pid {}).", status.pid);
    Ok(())
}

#[cfg(unix)]
fn kill_pid(pid: u32) -> Result<(), String> {
    let ok = Command::new("kill")
        .arg("-TERM")
        .arg(pid.to_string())
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(format!("failed to signal pid {pid}"))
    }
}

#[cfg(windows)]
fn kill_pid(pid: u32) -> Result<(), String> {
    let ok = Command::new("taskkill")
        .args(["/PID", &pid.to_string(), "/F"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if ok {
        Ok(())
    } else {
        Err(format!("failed to terminate pid {pid}"))
    }
}

fn exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map(|path| path.to_string_lossy().to_string())
        .map_err(|error| error.to_string())
}

// ---- macOS (launchd) --------------------------------------------------------

#[cfg(target_os = "macos")]
fn plist_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist"))
}

#[cfg(target_os = "macos")]
pub fn install() -> Result<(), String> {
    let exe = exe_path()?;
    let log = paths::log_path();
    let log = log.display();
    let path = plist_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let plist = format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key><string>{SERVICE_LABEL}</string>
  <key>ProgramArguments</key>
  <array><string>{exe}</string><string>start</string></array>
  <key>RunAtLoad</key><true/>
  <key>KeepAlive</key><true/>
  <key>StandardOutPath</key><string>{log}</string>
  <key>StandardErrorPath</key><string>{log}</string>
</dict>
</plist>
"#
    );
    std::fs::write(&path, plist).map_err(|error| error.to_string())?;
    let _ = Command::new("launchctl")
        .args(["unload", &path.to_string_lossy()])
        .status();
    let loaded = Command::new("launchctl")
        .args(["load", "-w", &path.to_string_lossy()])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !loaded {
        return Err("failed to load the launchd service".to_string());
    }
    println!("Installed launchd service: {}", path.display());
    println!("The host will start at login and restart if it exits.");
    Ok(())
}

#[cfg(target_os = "macos")]
pub fn uninstall() -> Result<(), String> {
    let path = plist_path();
    let _ = Command::new("launchctl")
        .args(["unload", "-w", &path.to_string_lossy()])
        .status();
    let _ = std::fs::remove_file(&path);
    let _ = stop();
    println!("Removed launchd service.");
    Ok(())
}

// ---- Linux (systemd --user) -------------------------------------------------

#[cfg(target_os = "linux")]
fn unit_path() -> std::path::PathBuf {
    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
    std::path::PathBuf::from(home)
        .join(".config/systemd/user")
        .join("codux.service")
}

#[cfg(target_os = "linux")]
pub fn install() -> Result<(), String> {
    let exe = exe_path()?;
    let path = unit_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| error.to_string())?;
    }
    let unit = format!(
        "[Unit]\nDescription=Codux headless host\nAfter=network-online.target\n\n\
         [Service]\nExecStart={exe} start\nRestart=on-failure\nRestartSec=3\n\n\
         [Install]\nWantedBy=default.target\n"
    );
    std::fs::write(&path, unit).map_err(|error| error.to_string())?;
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    let ok = Command::new("systemctl")
        .args(["--user", "enable", "--now", "codux.service"])
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !ok {
        return Err(
            "failed to enable the systemd service (is `systemctl --user` available?)".to_string(),
        );
    }
    println!("Installed systemd user service: {}", path.display());
    println!("Tip: run `loginctl enable-linger $USER` to keep it running after logout.");
    Ok(())
}

#[cfg(target_os = "linux")]
pub fn uninstall() -> Result<(), String> {
    let _ = Command::new("systemctl")
        .args(["--user", "disable", "--now", "codux.service"])
        .status();
    let _ = std::fs::remove_file(unit_path());
    let _ = Command::new("systemctl")
        .args(["--user", "daemon-reload"])
        .status();
    println!("Removed systemd user service.");
    Ok(())
}

// ---- Windows (Task Scheduler) ----------------------------------------------

#[cfg(target_os = "windows")]
pub fn install() -> Result<(), String> {
    let exe = exe_path()?;
    let ok = Command::new("schtasks")
        .args(["/Create", "/F", "/TN", "Codux", "/SC", "ONLOGON", "/TR"])
        .arg(format!("\"{exe}\" start"))
        .status()
        .map(|status| status.success())
        .unwrap_or(false);
    if !ok {
        return Err("failed to register the scheduled task".to_string());
    }
    println!("Installed Codux as a logon task (Task Scheduler: Codux).");
    Ok(())
}

#[cfg(target_os = "windows")]
pub fn uninstall() -> Result<(), String> {
    let _ = Command::new("schtasks")
        .args(["/Delete", "/F", "/TN", "Codux"])
        .status();
    let _ = stop();
    println!("Removed Codux scheduled task.");
    Ok(())
}

// ---- Other platforms --------------------------------------------------------

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn install() -> Result<(), String> {
    Err("service install is not supported on this platform; run `codux start` manually".to_string())
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
pub fn uninstall() -> Result<(), String> {
    Err("service install is not supported on this platform".to_string())
}
