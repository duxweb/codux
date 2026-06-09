use super::{
    MemoryService, now_seconds,
    transcript::{
        memory_project_context, memory_project_context_for_task, resolve_transcript_for_task,
        resolve_transcript_for_task_with_settings, resolve_transcript_source, session_identifier,
    },
};
use crate::{
    ai_runtime::snapshot::AISessionSnapshot,
    llm,
    memory::extraction::{
        PromptMemoryEntry, PromptMemorySummary, decode_extraction_response,
        ensure_memory_provider_available, extraction_system_prompt, make_extraction_prompt,
        provider_summary, select_memory_provider, should_stop_memory_queue_after_error,
    },
    project_store::ProjectWorkspaceRecord,
    runtime_state::ProjectInfo,
    settings::{AIMemorySettings, AISettings},
};
use rusqlite::{OptionalExtension, params};
use std::time::Duration;

const MEMORY_EXTRACTION_TASK_INTERVAL: Duration = Duration::from_secs(1);

mod db;
mod prompt_context;
mod schema;
mod types;

use db::{
    active_extraction_tasks as db_active_extraction_tasks,
    failed_extraction_tasks as db_failed_extraction_tasks, latest_failed_error,
    memory_task_from_row, queue_count,
};
use prompt_context::prompt_entries;
pub use types::{
    MemoryEnqueueResult, MemoryExtractionStatus, MemoryExtractionStatusSnapshot,
    MemoryExtractionTask,
};

include!("queue/enqueue.rs");
include!("queue/process.rs");
include!("queue/service_helpers.rs");
