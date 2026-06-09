use super::platform::{PlatformSleepAssertion, platform_power_adapter_connected};
use super::types::PowerSummary;
use std::sync::Mutex;

pub struct PowerService {
    assertion: Mutex<Option<PlatformSleepAssertion>>,
}

impl Default for PowerService {
    fn default() -> Self {
        Self {
            assertion: Mutex::new(None),
        }
    }
}

impl PowerService {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn set_sleep_prevention(&self, mode: &str) -> PowerSummary {
        let adapter = platform_power_adapter_connected();
        let effective_enabled = match mode {
            "always" => true,
            "powerAdapterOnly" => adapter.unwrap_or(true),
            _ => false,
        };
        let mut summary = PowerSummary {
            mode: normalize_sleep_mode(mode).to_string(),
            effective_enabled,
            power_adapter_connected: adapter,
            assertion_active: false,
            error: None,
        };
        let mut assertion = match self.assertion.lock() {
            Ok(assertion) => assertion,
            Err(_) => {
                summary.error = Some("Power manager lock poisoned.".to_string());
                return summary;
            }
        };
        if !effective_enabled {
            if let Some(existing) = assertion.take() {
                existing.release();
            }
            return summary;
        }
        if assertion.is_none() {
            match PlatformSleepAssertion::create() {
                Ok(next) => *assertion = Some(next),
                Err(error) => summary.error = Some(error),
            }
        }
        summary.assertion_active = assertion.is_some();
        summary
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

pub fn next_sleep_mode(mode: &str) -> &'static str {
    match mode {
        "off" => "always",
        "always" => "powerAdapterOnly",
        "powerAdapterOnly" => "off",
        _ => "always",
    }
}

pub fn normalize_sleep_mode(mode: &str) -> &'static str {
    match mode {
        "always" => "always",
        "powerAdapterOnly" => "powerAdapterOnly",
        _ => "off",
    }
}
