use crate::paths::home_dir;
use crate::usage_store::AIUsageStore;
use anyhow::Result;
use chrono::{DateTime, Datelike, Local, TimeZone, Timelike};
use codux_runtime_core::path::optional_local_path_equals as paths_equivalent;
use rusqlite::Connection;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
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
