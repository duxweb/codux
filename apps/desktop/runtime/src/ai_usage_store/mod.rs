use crate::ai_history_normalized::{
    AIHeatmapDay, AIHistoryProjectRequest, AIHistorySnapshot, AIProjectUsageSummary,
    AISessionSummary, AITimeBucket, AIUsageBreakdownItem, HistoryEntry, HistoryEvent, HistoryRole,
    JSONLParseSnapshot, ParsedHistory, deterministic_uuid, half_hour_bucket_start, history_key,
    local_day_start_seconds, now_seconds,
};
use crate::runtime_paths::app_support_dir;
use anyhow::{Context, Result};
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

include!("schema.rs");
include!("types.rs");
include!("store_core.rs");
include!("store_index_files.rs");
include!("store_project.rs");
include!("store_loaders.rs");
include!("connection.rs");
include!("external_summary.rs");
include!("buckets.rs");
include!("snapshot.rs");
include!("helpers.rs");

#[cfg(test)]
include!("tests.rs");
