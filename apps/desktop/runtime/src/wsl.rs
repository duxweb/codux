mod client;
mod discovery;
mod install;
mod manager;

#[cfg(target_os = "windows")]
fn command() -> std::process::Command {
    use std::os::windows::process::CommandExt;

    const CREATE_NO_WINDOW: u32 = 0x0800_0000;

    let mut command = std::process::Command::new("wsl.exe");
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

pub use client::{
    WslAgentWorktreeChanged, WslAgentWorktreeCreated, WslRuntimeClient, WslRuntimeEvent,
    WslRuntimeEventSink,
};
pub use discovery::{WslDistribution, WslOnlineDistribution};
pub use install::{WslInstallOperation, WslInstallProgress};
pub use manager::{
    WslDistributionCatalog, WslDistributionStatus, WslRuntimeInfo, WslRuntimeManager,
};

pub const WSL_RUNTIME_NOT_INSTALLED_ERROR: &str = "wsl_runtime_not_installed";
pub const WSL_RUNTIME_PROTOCOL_MISMATCH_ERROR: &str = "wsl_runtime_protocol_mismatch";
