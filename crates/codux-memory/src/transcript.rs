use super::queue::MemoryExtractionTask;
use crate::extraction::trim_memory_text;
use crate::{
    MemoryProjectRecord, MemorySessionSnapshot, MemorySettings, home_dir, normalized_string,
    transcript_paths::{
        claude_project_log_paths, find_codex_rollout_path, gemini_session_paths, paths_equivalent,
    },
};
use rusqlite::{Connection, params};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

#[derive(Debug, Clone)]
pub(super) struct MemoryProjectContext {
    pub(super) project_id: String,
    pub(super) project_name: String,
    pub(super) workspace_path: String,
}

#[derive(Debug, Clone)]
pub(super) struct TranscriptSource {
    pub(super) location: String,
    pub(super) fingerprint: String,
}

include!("transcript/context.rs");
include!("transcript/source.rs");
include!("transcript/read.rs");
include!("transcript/opencode.rs");
include!("transcript/compact.rs");
include!("transcript/helpers.rs");
