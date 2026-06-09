mod manager;
mod platform;
mod service;
#[cfg(test)]
mod tests;
mod types;

pub use manager::PowerManager;
pub use service::{PowerService, next_sleep_mode, normalize_sleep_mode};
pub use types::PowerSummary;
