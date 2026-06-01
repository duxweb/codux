use crate::runtime_paths::{
    live_log_path, runtime_event_dir as shared_runtime_event_dir,
    runtime_root_dir as shared_runtime_root_dir,
};
use std::path::PathBuf;

pub fn runtime_root_dir() -> PathBuf {
    shared_runtime_root_dir()
}

pub fn runtime_event_dir() -> PathBuf {
    shared_runtime_event_dir()
}

pub fn runtime_live_log_path() -> PathBuf {
    live_log_path()
}
