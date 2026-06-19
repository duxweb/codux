//! The Codux memory engine, extracted from the desktop runtime so it can run on
//! the headless host too (a remote-hosted project's memory is generated where
//! its AI sessions live). The engine owns its config types ([`config`]) and
//! calls the shared `codux-llm` crate through the [`llm`] bridge; the desktop
//! and the agent each convert their settings into [`MemoryConfig`] at the
//! boundary.

mod config;
pub(crate) mod llm;
mod transcript_paths;

pub use config::{
    MemoryConfig, MemoryPlanItem, MemoryPlanSnapshot, MemoryProjectInfo, MemoryProjectRecord,
    MemoryProvider, MemorySessionSnapshot, MemorySettings, home_dir, normalized_string,
};

include!("memory_root.rs");
