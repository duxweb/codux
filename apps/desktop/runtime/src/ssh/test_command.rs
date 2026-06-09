use super::types::{SSHConnectionProfile, SSHProfileTestResult};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};
use uuid::Uuid;

pub(super) fn write_test_profile_file(profile: &SSHConnectionProfile) -> Result<PathBuf, String> {
    let path = std::env::temp_dir().join(format!("codux-ssh-test-{}.json", Uuid::new_v4()));
    let data = serde_json::to_vec_pretty(&vec![profile]).map_err(|error| error.to_string())?;
    fs::write(&path, data).map_err(|error| error.to_string())?;
    Ok(path)
}

pub(super) fn ssh_wrapper_path(runtime_assets: impl AsRef<Path>) -> PathBuf {
    #[cfg(windows)]
    {
        let runtime_assets = runtime_assets.as_ref();
        if runtime_assets.ends_with("bin") {
            runtime_assets.join("codux-ssh.ps1")
        } else {
            runtime_assets.join("scripts/wrappers/bin/codux-ssh.ps1")
        }
    }
    #[cfg(not(windows))]
    {
        let runtime_assets = runtime_assets.as_ref();
        if runtime_assets.ends_with("bin") {
            runtime_assets.join("codux-ssh")
        } else {
            runtime_assets.join("scripts/wrappers/bin/codux-ssh")
        }
    }
}

pub(super) fn run_ssh_test_command(
    wrapper: &Path,
    profile_id: &str,
    profiles_file: &Path,
) -> Result<Output, String> {
    let mut child = Command::new(wrapper)
        .arg(profile_id)
        .arg("--")
        .arg("echo codux-ssh-ok")
        .env("CODUX_SSH_PROFILES_FILE", profiles_file)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("Unable to run SSH test: {error}"))?;
    let stdout = read_child_pipe(child.stdout.take());
    let stderr = read_child_pipe(child.stderr.take());
    let start = Instant::now();

    loop {
        if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
            return Ok(Output {
                status,
                stdout: stdout.join().unwrap_or_default(),
                stderr: stderr.join().unwrap_or_default(),
            });
        }
        if start.elapsed() >= Duration::from_secs(12) {
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.join();
            let _ = stderr.join();
            return Err("SSH connection test timed out.".to_string());
        }
        thread::sleep(Duration::from_millis(50));
    }
}

pub(super) fn profile_test_result(output: Output) -> SSHProfileTestResult {
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    if output.status.success() && stdout.contains("codux-ssh-ok") {
        return SSHProfileTestResult {
            ok: true,
            message: "SSH connection test succeeded.".to_string(),
        };
    }
    let detail = stderr.trim();
    let message = if detail.is_empty() {
        format!(
            "SSH connection test failed with status {}.",
            output.status.code().unwrap_or(-1)
        )
    } else {
        detail.to_string()
    };
    SSHProfileTestResult { ok: false, message }
}

fn read_child_pipe<T>(pipe: Option<T>) -> thread::JoinHandle<Vec<u8>>
where
    T: Read + Send + 'static,
{
    thread::spawn(move || {
        let Some(mut pipe) = pipe else {
            return Vec::new();
        };
        let mut bytes = Vec::new();
        let _ = pipe.read_to_end(&mut bytes);
        bytes
    })
}
