use super::super::{MemoryProjectProfile, evidence::bullet_lines, now_seconds};
use sha2::{Digest, Sha256};

const PROJECT_PROFILE_LLM_REFRESH_COOLDOWN_SECONDS: f64 = 6.0 * 60.0 * 60.0;

pub(in crate::profile) fn llm_project_profile_fingerprint(
    source_fingerprint: &str,
) -> String {
    format!("llm-v1:{source_fingerprint}")
}

pub(in crate::profile) fn project_profile_llm_source_fingerprint(
    source_fingerprint: &str,
    memory_context: &[String],
) -> String {
    let memory_hash = sha256_hex(&memory_context.join("\n"));
    format!("{source_fingerprint}:memory:{memory_hash}")
}

pub(in crate::profile) fn project_profile_content_with_memory_context(
    content: &str,
    memory_context: &[String],
) -> String {
    if memory_context.is_empty() {
        return content.to_string();
    }
    format!(
        "{content}\n\nProject memory signals:\n{}",
        bullet_lines(memory_context)
    )
}

pub(in crate::profile) fn project_profile_fingerprints_match(
    existing: &str,
    incoming: &str,
) -> bool {
    existing == incoming
        || (!incoming.starts_with("llm-v1:")
            && existing == llm_project_profile_fingerprint(incoming))
}

pub(in crate::profile) fn project_profile_llm_refresh_due(
    existing: &MemoryProjectProfile,
    generated: &MemoryProjectProfile,
) -> bool {
    if existing.source_fingerprint == llm_project_profile_fingerprint(&generated.source_fingerprint)
    {
        return false;
    }
    let changed = !project_profile_fingerprints_match(
        &existing.source_fingerprint,
        &generated.source_fingerprint,
    );
    if !changed && existing.source_fingerprint.starts_with("llm-v1:") {
        return false;
    }
    now_seconds() - existing.updated_at >= PROJECT_PROFILE_LLM_REFRESH_COOLDOWN_SECONDS
}

fn sha256_hex(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}
