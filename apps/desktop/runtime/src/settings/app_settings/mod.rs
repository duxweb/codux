mod defaults;
mod locale;
mod sanitize;
mod store;
mod types;

pub use locale::{locale_from_language_setting, sync_process_locale_preference};
pub use store::AppSettingsStore;
pub use types::*;
