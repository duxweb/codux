impl MemoryService {
    pub(super) fn write_candidate_with_decision(
        &self,
        conn: &Connection,
        candidate: MemoryCandidate,
        explicit_decision: Option<MemoryWriteDecision>,
    ) -> Result<Option<StoredMemoryEntry>, String> {
        let decision = explicit_decision
            .or_else(|| self.decide_memory_write(conn, &candidate).ok().flatten())
            .unwrap_or_else(|| MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Create,
                target_entry_id: None,
                reason: "new durable memory".to_string(),
            });
        match decision.kind {
            MemoryWriteDecisionKind::Skip => {
                self.record_memory_decision(conn, MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Skip,
                    entry_id: None,
                    target_entry_id: decision.target_entry_id,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(None)
            }
            MemoryWriteDecisionKind::Archive => {
                if let Some(target_entry_id) = decision.target_entry_id.as_deref() {
                    self.archive_entries(conn, &[target_entry_id.to_string()])?;
                }
                self.record_memory_decision(conn, MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Archive,
                    entry_id: None,
                    target_entry_id: decision.target_entry_id,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(None)
            }
            MemoryWriteDecisionKind::Merge => {
                if let Some(target_entry_id) = decision.target_entry_id.as_deref() {
                    let entry = self.merge_candidate_into_entry(conn, target_entry_id, candidate)?;
                    self.record_memory_decision(conn, MemoryDecisionLog {
                        kind: MemoryWriteDecisionKind::Merge,
                        entry_id: Some(entry.id.clone()),
                        target_entry_id: Some(target_entry_id.to_string()),
                        reason: decision.reason,
                        created_at: now_seconds(),
                    })?;
                    Ok(Some(entry))
                } else {
                    let entry = self.upsert(conn, candidate)?;
                    self.record_memory_decision(conn, MemoryDecisionLog {
                        kind: MemoryWriteDecisionKind::Create,
                        entry_id: Some(entry.id.clone()),
                        target_entry_id: None,
                        reason: "merge decision had no target; created memory".to_string(),
                        created_at: now_seconds(),
                    })?;
                    Ok(Some(entry))
                }
            }
            MemoryWriteDecisionKind::Replace => {
                let target_entry_id = decision.target_entry_id.clone();
                let entry = self.upsert(conn, candidate)?;
                if let Some(target_entry_id) = target_entry_id.as_deref() {
                    self.supersede_entry(conn, target_entry_id, &entry.id)?;
                }
                self.record_memory_decision(conn, MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Replace,
                    entry_id: Some(entry.id.clone()),
                    target_entry_id,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(Some(entry))
            }
            MemoryWriteDecisionKind::Create => {
                let entry = self.upsert(conn, candidate)?;
                self.record_memory_decision(conn, MemoryDecisionLog {
                    kind: MemoryWriteDecisionKind::Create,
                    entry_id: Some(entry.id.clone()),
                    target_entry_id: None,
                    reason: decision.reason,
                    created_at: now_seconds(),
                })?;
                Ok(Some(entry))
            }
        }
    }

    fn decide_memory_write(
        &self,
        conn: &Connection,
        candidate: &MemoryCandidate,
    ) -> Result<Option<MemoryWriteDecision>, String> {
        if should_skip_memory_candidate(candidate) {
            return Ok(Some(MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Skip,
                target_entry_id: None,
                reason: "candidate is too short or low signal".to_string(),
            }));
        }
        let candidates = self.write_decision_candidates(conn, candidate)?;
        let Some(best) = candidates
            .iter()
            .map(|entry| (memory_similarity(&candidate.content, &entry.content), entry))
            .max_by(|left, right| {
                left.0
                    .partial_cmp(&right.0)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
        else {
            return Ok(None);
        };
        if best.0 >= MEMORY_MERGE_SIMILARITY_THRESHOLD {
            return Ok(Some(MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Merge,
                target_entry_id: Some(best.1.id.clone()),
                reason: format!("semantic duplicate score {:.2}", best.0),
            }));
        }
        if best.0 >= MEMORY_REPLACE_SIMILARITY_THRESHOLD
            && memory_candidate_conflicts(candidate, best.1)
        {
            return Ok(Some(MemoryWriteDecision {
                kind: MemoryWriteDecisionKind::Replace,
                target_entry_id: Some(best.1.id.clone()),
                reason: format!("conflicting newer memory score {:.2}", best.0),
            }));
        }
        Ok(None)
    }
}
