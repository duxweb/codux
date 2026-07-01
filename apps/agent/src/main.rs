use clap::{Parser, Subcommand};
use codux_protocol::REMOTE_PROTOCOL_VERSION;

mod ai_stats;
mod cmd_config;
mod cmd_device;
mod cmd_pair;
mod cmd_service;
mod cmd_start;
mod cmd_update;
mod config_store;
mod device_store;
mod host;
mod logo;
mod memory;
mod paths;
mod projects;
mod runstate;
mod sessions;
mod smoke;
mod terminals;
mod web_test;
mod worktree;

const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Parser)]
#[command(
    name = "codux",
    bin_name = "codux",
    version = VERSION,
    about = "Codux headless host — run your projects' terminals, Git, AI and memory for remote desktops"
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Show the version and protocol revision.
    Version,
    /// Interactive setup wizard — writes/updates codux.toml.
    Config,
    /// Install and enable Codux as a system startup service.
    Install,
    /// Stop and remove the system service.
    Uninstall,
    /// Start the host (idempotent; prints the path if already running).
    Start {
        /// Run detached in the background (used by the service).
        #[arg(long)]
        detach: bool,
    },
    /// Stop the running host.
    Stop,
    /// Show whether the host is running, since when, and how many devices.
    Status,
    /// Print the pairing QR code in the terminal (starts the host if needed).
    Qrcode,
    /// Print the pairing ticket for the desktop to paste (starts the host if needed).
    Link,
    /// Check for a newer release and update in place.
    Update {
        /// Include beta pre-releases in the update channel.
        #[arg(long)]
        beta: bool,
    },
    /// List paired devices.
    Device,
    /// Remove a paired device by id.
    #[command(name = "device:del")]
    DeviceDel { id: String },
    /// Rename a paired device by id (prompts for the new name).
    #[command(name = "device:rename")]
    DeviceRename { id: String },
    /// Remove every paired device.
    #[command(name = "device:clear")]
    DeviceClear,
    /// Internal smoke tests (pty | transport | serve).
    #[command(name = "smoke", hide = true)]
    Smoke { kind: String },
}

fn main() {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match codux_runtime_live::wrapper_helper::handle_args(&args) {
        Ok(true) => return,
        Ok(false) => {}
        Err(error) => {
            eprintln!("{error}");
            std::process::exit(64);
        }
    }

    let cli = Cli::parse();
    let result = match cli.command {
        None => {
            print_version();
            println!();
            println!("Run `codux --help` for commands, or `codux config` to set up.");
            Ok(())
        }
        Some(Command::Version) => {
            print_version();
            Ok(())
        }
        Some(Command::Config) => cmd_config::run(),
        Some(Command::Install) => cmd_service::install(),
        Some(Command::Uninstall) => cmd_service::uninstall(),
        Some(Command::Start { detach }) => cmd_start::run(detach),
        Some(Command::Stop) => cmd_service::stop(),
        Some(Command::Status) => cmd_service::status(),
        Some(Command::Qrcode) => cmd_pair::qrcode(),
        Some(Command::Link) => cmd_pair::link(),
        Some(Command::Update { beta }) => cmd_update::run(VERSION, beta),
        Some(Command::Device) => cmd_device::list(),
        Some(Command::DeviceDel { id }) => cmd_device::del(&id),
        Some(Command::DeviceRename { id }) => cmd_device::rename(&id),
        Some(Command::DeviceClear) => cmd_device::clear(),
        Some(Command::Smoke { kind }) => smoke::run(&kind).map(|output| println!("{output}")),
    };
    if let Err(error) = result {
        eprintln!("error: {error}");
        std::process::exit(1);
    }
}

fn print_version() {
    println!("codux {VERSION}");
    println!("protocol {REMOTE_PROTOCOL_VERSION}");
}
