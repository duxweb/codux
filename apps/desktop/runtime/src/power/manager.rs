use super::{
    platform::{PlatformSleepAssertion, platform_power_adapter_connected},
    service::normalize_sleep_mode,
    types::PowerSummary,
};
use crate::settings::AppSettingsStore;
use std::{
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

pub struct PowerManager {
    assertion: Mutex<Option<PlatformSleepAssertion>>,
    sync_started: Mutex<bool>,
}

impl Default for PowerManager {
    fn default() -> Self {
        Self {
            assertion: Mutex::new(None),
            sync_started: Mutex::new(false),
        }
    }
}

impl PowerManager {
    pub fn start_settings_sync(
        self: &Arc<Self>,
        settings: Arc<AppSettingsStore>,
    ) -> Result<(), String> {
        {
            let mut started = self
                .sync_started
                .lock()
                .map_err(|_| "Power manager sync lock poisoned.".to_string())?;
            if *started {
                return Ok(());
            }
            *started = true;
        }

        let manager = Arc::clone(self);
        manager.set_sleep_prevention(settings.snapshot().sleep_mode)?;
        let _ = thread::Builder::new()
            .name("codux-power-settings-sync".to_string())
            .spawn(move || {
                loop {
                    thread::sleep(Duration::from_secs(60));
                    let _ = manager.set_sleep_prevention(settings.snapshot().sleep_mode);
                }
            });
        Ok(())
    }

    pub fn set_sleep_prevention(&self, mode: String) -> Result<bool, String> {
        let enabled = match mode.as_str() {
            "always" => true,
            "powerAdapterOnly" => platform_power_adapter_connected().unwrap_or(true),
            _ => false,
        };
        let mut assertion = self
            .assertion
            .lock()
            .map_err(|_| "Power manager lock poisoned.".to_string())?;

        if !enabled {
            if let Some(existing) = assertion.take() {
                existing.release();
            }
            return Ok(false);
        }

        if assertion.is_none() {
            *assertion = Some(PlatformSleepAssertion::create()?);
        }
        Ok(assertion.is_some())
    }

    pub fn summary(&self, mode: &str) -> PowerSummary {
        let adapter = platform_power_adapter_connected();
        let assertion_active = self
            .assertion
            .lock()
            .map(|assertion| assertion.is_some())
            .unwrap_or(false);
        PowerSummary {
            mode: normalize_sleep_mode(mode).to_string(),
            effective_enabled: match mode {
                "always" => true,
                "powerAdapterOnly" => adapter.unwrap_or(true),
                _ => false,
            },
            power_adapter_connected: adapter,
            assertion_active,
            error: None,
        }
    }
}
