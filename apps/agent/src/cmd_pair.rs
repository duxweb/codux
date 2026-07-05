//! `codux link` and `codux qrcode` — surface the pairing ticket the running host
//! publishes. If the host isn't up yet, start it in the background first. The
//! headless host auto-confirms pairing (holding the one-time ticket is the gate),
//! so no second confirmation is needed.

use qrcode::QrCode;
use qrcode::render::unicode;
use std::time::Duration;

use crate::{cmd_config, cmd_service, cmd_start, config_store::CoduxConfig, runstate};

#[derive(Clone, Debug, Default)]
pub struct PairArgs {
    pub relay_preset: Option<String>,
    pub relay_url: Option<String>,
    pub relay_authentication: Option<String>,
}

impl PairArgs {
    fn has_relay_override(&self) -> bool {
        self.relay_preset.is_some()
            || self.relay_url.is_some()
            || self.relay_authentication.is_some()
    }
}

/// Print the pasteable pairing ticket for the desktop to connect.
pub fn link(args: PairArgs) -> Result<(), String> {
    let ticket = ensure_ticket(args)?;
    println!("{ticket}");
    Ok(())
}

/// Render the pairing QR code in the terminal, with the host status above it.
pub fn qrcode(args: PairArgs) -> Result<(), String> {
    let ticket = ensure_ticket(args)?;
    if let Some(status) = runstate::read_status() {
        println!(
            "Host running · started {} · device “{}”",
            status.started_at, status.device_name
        );
    }
    println!();
    let code = QrCode::new(ticket.as_bytes()).map_err(|error| error.to_string())?;
    let rendered = code.render::<unicode::Dense1x2>().quiet_zone(true).build();
    println!("{rendered}");
    println!("Scan with the Codux mobile app, or paste the link below on desktop:");
    println!("{ticket}");
    Ok(())
}

/// Return the published pairing ticket, starting the host in the background if
/// it isn't already running.
fn ensure_ticket(args: PairArgs) -> Result<String, String> {
    if args.has_relay_override() {
        let mut config = CoduxConfig::load();
        config.ensure_identity();
        let changed = cmd_config::apply_relay_args(
            &mut config,
            args.relay_preset.as_deref(),
            args.relay_url.as_deref(),
            args.relay_authentication.as_deref(),
        )?;
        if changed {
            config.save()?;
            if runstate::is_running() {
                println!("Relay changed — restarting Codux host…");
                cmd_service::stop()?;
            }
        }
    }
    if !runstate::is_running() {
        println!("Codux host is not running — starting it…");
        cmd_start::run(true)?;
    }
    for _ in 0..40 {
        if let Some(ticket) = runstate::read_ticket() {
            return Ok(ticket);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    Err("the host is up but has not published a pairing ticket yet".to_string())
}
