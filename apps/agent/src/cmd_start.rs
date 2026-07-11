//! `codux start` — launch the host. Idempotent: if one is already running it
//! prints where, instead of starting a second. `--detach` re-spawns itself in
//! the background (used by `qrcode`/`link` auto-start and the service).

use crate::config_store::CoduxConfig;
use crate::{host, logo, paths, runstate};

const VERSION: &str = env!("CARGO_PKG_VERSION");

pub fn run(detach: bool) -> Result<(), String> {
    // Make sure a stable identity exists before we derive the node key from it.
    let mut config = CoduxConfig::load();
    if config.ensure_identity()? {
        config.save()?;
    }

    if runstate::is_running() {
        print_already_running();
        return Ok(());
    }

    if detach {
        return spawn_detached();
    }

    run_foreground(config)
}

fn run_foreground(config: CoduxConfig) -> Result<(), String> {
    logo::print_banner(VERSION);
    let _lock = runstate::acquire_instance_lock()?;
    println!(
        "start time: {}",
        chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
    );
    println!("config:     {}", paths::config_path().display());
    println!("device:     {}", config.device_name);
    println!("relay:      {}", config.relay_preset);
    println!();

    let cfg = host::AgentHostConfig {
        host_id: config.host_id.clone(),
        host_token: config.host_token.clone(),
        name: config.device_name.clone(),
        relay_preset: config.relay_preset.clone(),
        relay_url: config.relay_url.clone(),
        relay_authentication: config.relay_authentication.clone(),
    };
    let runtime = tokio::runtime::Runtime::new().map_err(|error| error.to_string())?;
    let result = runtime.block_on(host::run_host(cfg));
    runstate::clear_status();
    runstate::clear_ticket();
    result
}

fn print_already_running() {
    println!("Codux host is already running.");
    if let Some(status) = runstate::read_status() {
        println!("  started: {}", status.started_at);
        println!("  device:  {}", status.device_name);
        if !status.web_test_url.is_empty() {
            println!("  web:     {}", status.web_test_url);
        }
    }
    if let Ok(exe) = std::env::current_exe() {
        println!("  binary:  {}", exe.display());
    }
    println!("  config:  {}", paths::config_path().display());
}

/// Re-launch `codux start` detached, with output redirected to the log file, so
/// the caller (`qrcode`/`link`/the service) can return immediately.
fn spawn_detached() -> Result<(), String> {
    use std::process::{Command, Stdio};

    paths::ensure_data_dir();
    let exe = std::env::current_exe().map_err(|error| error.to_string())?;
    let log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(paths::log_path())
        .map_err(|error| error.to_string())?;
    let log_err = log.try_clone().map_err(|error| error.to_string())?;

    let mut command = Command::new(exe);
    command
        .arg("start")
        .stdin(Stdio::null())
        .stdout(Stdio::from(log))
        .stderr(Stdio::from(log_err));
    detach_platform(&mut command);

    command
        .spawn()
        .map_err(|error| format!("failed to start background host: {error}"))?;
    // Give it a moment to acquire the lock / publish the ticket.
    std::thread::sleep(std::time::Duration::from_millis(600));
    if runstate::is_running() {
        println!("Codux host started in the background.");
        println!("  logs: {}", paths::log_path().display());
        Ok(())
    } else {
        Err(format!(
            "the background host did not come up; see {}",
            paths::log_path().display()
        ))
    }
}

#[cfg(unix)]
fn detach_platform(command: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;
    // New session so the daemon survives the terminal closing.
    unsafe {
        command.pre_exec(|| {
            libc_setsid();
            Ok(())
        });
    }
}

#[cfg(unix)]
fn libc_setsid() {
    // setsid(2) without pulling in the libc crate: syscall via the C ABI symbol.
    unsafe extern "C" {
        fn setsid() -> i32;
    }
    unsafe {
        setsid();
    }
}

#[cfg(windows)]
fn detach_platform(command: &mut std::process::Command) {
    use std::os::windows::process::CommandExt;
    const DETACHED_PROCESS: u32 = 0x0000_0008;
    const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
    command.creation_flags(DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP);
}
