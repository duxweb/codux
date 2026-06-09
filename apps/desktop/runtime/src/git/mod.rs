use serde::{Deserialize, Serialize};
use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::mpsc::{self, RecvTimeoutError},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};

use notify::{Event, RecommendedWatcher, RecursiveMode, Watcher};

const MAX_DIFF_BYTES: usize = 96 * 1024;
const REVIEW_UNTRACKED_LINE_COUNT_LIMIT_BYTES: u64 = 2 * 1024 * 1024;
const GIT_WATCH_DEBOUNCE_MS: u64 = 900;
const COMMIT_CONTEXT_MAX_CHARS: usize = 24_000;
const COMMIT_CONTEXT_MAX_FILES: usize = 80;
const COMMIT_CONTEXT_MAX_LINES_PER_FILE: usize = 80;
const CODUX_MANAGED_MEMORY_ENTRYPOINT_MARKER: &str = "<!-- CODUX_MANAGED_MEMORY_ENTRYPOINT -->";

type GitRepository = git2::Repository;
pub type GitCancelToken = Arc<AtomicBool>;

include!("types.rs");
include!("watch.rs");
include!("service.rs");
include!("commands.rs");
include!("repository.rs");
include!("operations.rs");
include!("diff.rs");
include!("metadata.rs");

#[cfg(test)]
include!("tests.rs");
