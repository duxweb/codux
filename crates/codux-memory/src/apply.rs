mod decision;
mod helpers;
mod summary;
mod types;

use super::{MemoryService, now_seconds, queue::MemoryExtractionTask};
use crate::{
    MemorySettings,
    extraction::{
        MemoryExtractionResponse, MemoryScope, MemoryTier, parse_uuid_string, valid_summary_content,
    },
};
use helpers::*;
use types::{MemoryCandidate, MemoryWriteDecision};
pub use types::{
    MemoryDecisionLog, MemoryEntryStatus, MemoryWriteDecisionKind, StoredMemoryEntry,
    StoredMemorySummary,
};

const DEFAULT_MEMORY_MODULE: &str = "general";
const MEMORY_WRITE_CANDIDATE_LIMIT: i64 = 8;
const MEMORY_MERGE_SIMILARITY_THRESHOLD: f64 = 0.64;
const MEMORY_REPLACE_SIMILARITY_THRESHOLD: f64 = 0.34;

impl MemoryService {
    pub fn apply_extraction_response(
        &self,
        response: MemoryExtractionResponse,
        task: &MemoryExtractionTask,
        settings: &MemorySettings,
    ) -> Result<(), String> {
        self.ensure_queue_schema()?;
        // Open one connection and wrap the whole apply in a single transaction:
        // every memory/summary/decision write below shares it instead of each
        // helper opening its own connection (which multiplied lock-contention
        // windows and was the bulk of the per-apply overhead). The apply is now
        // atomic as a bonus.
        let mut connection = self.open_or_create_connection()?;
        let tx = connection
            .transaction()
            .map_err(|error| error.to_string())?;
        let conn = &tx;
        for item in response.working_add {
            let Some(content) = normalized_non_empty(&item.content) else {
                continue;
            };
            if let Some(reason) = item.skip_reason.as_deref().and_then(normalized_non_empty) {
                self.record_memory_decision(
                    conn,
                    MemoryDecisionLog {
                        kind: MemoryWriteDecisionKind::Skip,
                        entry_id: None,
                        target_entry_id: None,
                        reason,
                        created_at: now_seconds(),
                    },
                )?;
                continue;
            }
            let scope = item.scope.unwrap_or(MemoryScope::Project);
            let project_id = (scope == MemoryScope::Project).then(|| task.project_id.clone());
            let explicit_decision = if let Some(target_entry_id) = item.replace {
                Some(MemoryWriteDecision {
                    kind: MemoryWriteDecisionKind::Replace,
                    target_entry_id: Some(target_entry_id),
                    reason: "provider marked this memory as replacing an existing entry"
                        .to_string(),
                })
            } else {
                item.merge_with
                    .first()
                    .cloned()
                    .map(|target_entry_id| MemoryWriteDecision {
                        kind: MemoryWriteDecisionKind::Merge,
                        target_entry_id: Some(target_entry_id),
                        reason: "provider marked this memory as a semantic merge".to_string(),
                    })
            };
            let archive_ids = item
                .archive
                .iter()
                .chain(item.merge_with.iter().skip(1))
                .cloned()
                .collect::<Vec<_>>();
            for archive_id in &archive_ids {
                self.archive_entries(conn, std::slice::from_ref(archive_id))?;
                self.record_memory_decision(
                    conn,
                    MemoryDecisionLog {
                        kind: MemoryWriteDecisionKind::Archive,
                        entry_id: None,
                        target_entry_id: Some(archive_id.clone()),
                        reason: "provider marked existing memory as stale or duplicate".to_string(),
                        created_at: now_seconds(),
                    },
                )?;
            }
            let _ = self.write_candidate_with_decision(
                conn,
                MemoryCandidate {
                    scope,
                    project_id,
                    tool_id: None,
                    module_key: item
                        .module_key
                        .or_else(|| Some(DEFAULT_MEMORY_MODULE.to_string())),
                    tier: item.tier.unwrap_or(MemoryTier::Working),
                    kind: item.kind,
                    content,
                    rationale: item
                        .rationale
                        .and_then(|value| normalized_non_empty(&value)),
                    source_tool: Some(task.tool.clone()),
                    source_session_id: Some(task.session_id.clone()),
                    source_fingerprint: Some(task.source_fingerprint.clone()),
                },
                explicit_decision,
            )?;
        }

        let merged_ids = response
            .merged_entry_ids
            .iter()
            .filter_map(|value| parse_uuid_string(value))
            .collect::<Vec<_>>();

        if let Some(content) = valid_summary_content(response.user_summary.as_deref().unwrap_or(""))
        {
            let summary = self.upsert_summary(
                conn,
                MemoryScope::User,
                None,
                None,
                &content,
                &merged_ids,
                settings.max_summary_versions,
            )?;
            self.mark_entries_merged(conn, &merged_ids, &summary.id)?;
            self.merge_stale_working_entries(
                conn,
                MemoryScope::User,
                None,
                settings.max_active_working_entries,
                &summary.id,
            )?;
        }
        let archive_ids = response
            .working_archive
            .iter()
            .filter_map(|value| parse_uuid_string(value))
            .collect::<Vec<_>>();
        self.archive_entries(conn, &archive_ids)?;
        self.trim_working_entries(
            conn,
            MemoryScope::User,
            None,
            settings.max_active_working_entries,
        )?;
        self.trim_working_entries(
            conn,
            MemoryScope::Project,
            Some(&task.project_id),
            settings.max_active_working_entries,
        )?;
        tx.commit().map_err(|error| error.to_string())?;
        Ok(())
    }
}
