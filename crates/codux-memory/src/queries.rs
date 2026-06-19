use super::{
    MemoryEntryDecisionSummary, MemoryEntrySummary, MemoryManagerTargetRow,
    MemoryProjectProfileSummary, MemoryScopeOverview, MemorySummaryRow,
};
use crate::MemoryProjectInfo;
use rusqlite::{Connection, OptionalExtension, params};
use std::collections::{HashMap, HashSet};

include!("queries/counts.rs");
include!("queries/entries.rs");
include!("queries/management.rs");
include!("queries/summaries.rs");
include!("queries/profile.rs");
include!("queries/helpers.rs");
