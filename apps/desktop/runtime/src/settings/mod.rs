pub mod app_settings;

use crate::config::ConfigStore;
use crate::llm::{self, LLMProviderTestResult};
pub use app_settings::{
    AIRuntimeToolSettings, AppSettings, AppSettingsStore, NotificationChannelSettings,
    RemoteHostDeviceSettings, RemoteSettings, UpdateSettings, sync_process_locale_preference,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::{collections::HashMap, path::PathBuf};

include!("types.rs");
include!("service.rs");
include!("service_preferences.rs");
include!("service_ai_provider.rs");
include!("service_memory.rs");
include!("service_pet.rs");
include!("service_runtime_tools.rs");
include!("service_shortcuts.rs");
include!("sanitize.rs");
include!("default_summary.rs");
include!("summary.rs");
include!("options.rs");

#[cfg(test)]
include!("tests.rs");
