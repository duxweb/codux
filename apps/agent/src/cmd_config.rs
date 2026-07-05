//! `codux config` — interactive setup wizard. Reuses the existing config as the
//! defaults (so unchanged answers keep their current values) and never rotates
//! the host identity (host_id/token), which would invalidate paired desktops.

use dialoguer::theme::ColorfulTheme;
use dialoguer::{Input, Select};

use crate::config_store::{CoduxConfig, RELAY_PRESET_CUSTOM};

#[derive(Clone, Debug, Default)]
pub struct ConfigArgs {
    pub device_name: Option<String>,
    pub relay_preset: Option<String>,
    pub relay_url: Option<String>,
    pub relay_authentication: Option<String>,
}

impl ConfigArgs {
    pub fn is_non_interactive(&self) -> bool {
        self.device_name.is_some()
            || self.relay_preset.is_some()
            || self.relay_url.is_some()
            || self.relay_authentication.is_some()
    }
}

pub fn run(args: ConfigArgs) -> Result<(), String> {
    if args.is_non_interactive() {
        return run_non_interactive(args);
    }

    let mut config = CoduxConfig::load();
    let existed = CoduxConfig::exists();
    config.ensure_identity();
    let theme = ColorfulTheme::default();

    println!("Codux host setup\n");

    // 1. Device name (shown on paired desktops).
    let device_name: String = Input::with_theme(&theme)
        .with_prompt("Device name (shown on desktops)")
        .with_initial_text(config.device_name.clone())
        .interact_text()
        .map_err(|error| error.to_string())?;
    config.device_name = device_name.trim().to_string();

    // 2. Relay node — pick a preset or custom.
    let presets = codux_remote_transport::remote_relay_presets();
    let mut labels: Vec<String> = presets
        .iter()
        .map(|preset| format!("{} ({})", preset.name, preset.url))
        .collect();
    labels.push("Custom relay…".to_string());
    let current_index = if config.relay_preset == RELAY_PRESET_CUSTOM {
        labels.len() - 1
    } else {
        presets
            .iter()
            .position(|preset| preset.key == config.relay_preset)
            .unwrap_or(0)
    };
    let choice = Select::with_theme(&theme)
        .with_prompt("Relay network")
        .items(&labels)
        .default(current_index)
        .interact()
        .map_err(|error| error.to_string())?;

    if choice == presets.len() {
        // 3. Custom relay: enter the URL and verify reachability.
        config.relay_preset = RELAY_PRESET_CUSTOM.to_string();
        let url: String = Input::with_theme(&theme)
            .with_prompt("Custom relay URL")
            .with_initial_text(config.relay_url.clone())
            .interact_text()
            .map_err(|error| error.to_string())?;
        let url = url.trim().to_string();
        print!("Checking relay reachability… ");
        match check_relay(&url) {
            Ok(()) => println!("ok"),
            Err(error) => {
                println!("unreachable ({error})");
                return Err(
                    "the custom relay is not reachable; fix the URL and re-run `codux config`"
                        .to_string(),
                );
            }
        }
        config.relay_url = url;

        let auth: String = Input::with_theme(&theme)
            .with_prompt("Relay auth token (optional)")
            .with_initial_text(config.relay_authentication.clone())
            .allow_empty(true)
            .interact_text()
            .map_err(|error| error.to_string())?;
        config.relay_authentication = auth.trim().to_string();
    } else {
        config.relay_preset = presets[choice].key.clone();
        config.relay_url = String::new();
        config.relay_authentication = String::new();
    }

    // Preserve (or mint once) the stable host identity.
    config.ensure_identity();
    config.save()?;

    println!();
    println!(
        "{} {}",
        if existed { "Updated" } else { "Created" },
        crate::paths::config_path().display()
    );
    println!("Run `codux start` to launch the host, then `codux qrcode` to pair.");
    Ok(())
}

fn run_non_interactive(args: ConfigArgs) -> Result<(), String> {
    let mut config = CoduxConfig::load();
    let existed = CoduxConfig::exists();
    config.ensure_identity();

    if let Some(device_name) = args.device_name {
        let device_name = device_name.trim();
        if device_name.is_empty() {
            return Err("device name cannot be empty".to_string());
        }
        config.device_name = device_name.to_string();
    }

    apply_relay_args(
        &mut config,
        args.relay_preset.as_deref(),
        args.relay_url.as_deref(),
        args.relay_authentication.as_deref(),
    )?;
    config.ensure_identity();
    config.save()?;

    println!(
        "{} {}",
        if existed { "Updated" } else { "Created" },
        crate::paths::config_path().display()
    );
    println!("relay: {}", config.relay_preset);
    Ok(())
}

pub fn apply_relay_args(
    config: &mut CoduxConfig,
    relay_preset: Option<&str>,
    relay_url: Option<&str>,
    relay_authentication: Option<&str>,
) -> Result<bool, String> {
    let before = (
        config.relay_preset.clone(),
        config.relay_url.clone(),
        config.relay_authentication.clone(),
    );
    if let Some(url) = relay_url.map(str::trim).filter(|url| !url.is_empty()) {
        if let Some(preset) = relay_preset
            .map(str::trim)
            .filter(|preset| !preset.is_empty())
        {
            if preset != RELAY_PRESET_CUSTOM {
                return Err("--relay-url can only be used with --relay-preset custom".to_string());
            }
        }
        config.relay_preset = RELAY_PRESET_CUSTOM.to_string();
        config.relay_url = url.to_string();
    } else if let Some(preset) = relay_preset
        .map(str::trim)
        .filter(|preset| !preset.is_empty())
    {
        let normalized =
            codux_remote_transport::normalize_remote_relay_preset(preset, &config.relay_url);
        if normalized == RELAY_PRESET_CUSTOM && config.relay_url.trim().is_empty() {
            return Err("custom relay requires --relay-url".to_string());
        }
        config.relay_preset = normalized;
        if config.relay_preset != RELAY_PRESET_CUSTOM {
            config.relay_url = String::new();
        }
    }
    if let Some(authentication) = relay_authentication {
        config.relay_authentication = authentication.trim().to_string();
    } else if config.relay_preset != RELAY_PRESET_CUSTOM {
        config.relay_authentication = String::new();
    }
    Ok(before
        != (
            config.relay_preset.clone(),
            config.relay_url.clone(),
            config.relay_authentication.clone(),
        ))
}

/// A relay is reachable if an HTTPS request to it completes (any status). Connect
/// errors / timeouts mean unreachable.
fn check_relay(url: &str) -> Result<(), String> {
    if url.is_empty() {
        return Err("empty URL".to_string());
    }
    let client = reqwest::blocking::Client::builder()
        .timeout(std::time::Duration::from_secs(6))
        .build()
        .map_err(|error| error.to_string())?;
    client.get(url).send().map(|_| ()).map_err(|error| {
        if error.is_timeout() {
            "timed out".to_string()
        } else if error.is_connect() {
            "connection failed".to_string()
        } else {
            error.to_string()
        }
    })
}
