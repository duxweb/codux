use crate::ai_usage_store::AIUsageStore;
use crate::runtime_paths::home_dir;
use anyhow::Result;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use uuid::Uuid;

const RECENT_HISTORY_SESSION_LIMIT: usize = 80;

include!("types.rs");
include!("indexing.rs");
include!("snapshot.rs");
include!("parsers_claude_codex.rs");
include!("parsers_codewhale.rs");
include!("parsers_other.rs");
include!("usage.rs");
include!("title.rs");
include!("paths.rs");
include!("fs_helpers.rs");
include!("history_driver.rs");
mod history_sources;

#[cfg(test)]
include!("tests.rs");
