//! Shared AI usage-history engine: parses each CLI's on-disk session history,
//! normalizes it, caches it in SQLite, and serves per-project / global usage
//! snapshots. Extracted from the desktop runtime so the headless agent can host
//! the same AI stats over the remote protocol with full parity. GPUI-free.

pub mod agy_db;
pub mod indexer;
pub mod normalized;
pub mod omp_session;
pub mod paths;
pub mod trace;
pub mod usage_store;
