use super::{MemoryProjectProfile, MemoryProjectProfileRefreshResult, MemoryService, now_seconds};
use crate::{
    MemoryConfig, MemoryProjectInfo, llm, normalized_string,
    extraction::{provider_summary, select_memory_provider, trim_memory_text},
};
use rusqlite::{OptionalExtension, params};

mod evidence;
mod llm_profile;

use evidence::build_project_profile;
use llm_profile::{
    decode_project_profile_llm_response_detailed, llm_project_profile_fingerprint,
    make_project_profile_llm_prompt, project_profile_content_with_memory_context,
    project_profile_fingerprints_match, project_profile_llm_refresh_due,
    project_profile_llm_source_fingerprint, project_profile_system_prompt,
};

impl MemoryService {
    pub fn project_profile_for_launch(
        &self,
        project_id: &str,
        project_name: &str,
        workspace_path: &str,
    ) -> Option<MemoryProjectProfile> {
        let generated = build_project_profile(project_id, project_name, workspace_path)?;
        self.upsert_project_profile(generated.clone())
            .ok()
            .or(Some(generated))
    }

    pub async fn force_refresh_project_profile_with_llm_detailed(
        &self,
        settings: &MemoryConfig,
        project: &MemoryProjectInfo,
    ) -> Option<MemoryProjectProfileRefreshResult> {
        self.refresh_project_profile_detailed(settings, project, true)
            .await
    }

    pub async fn refresh_project_profile_detailed(
        &self,
        settings: &MemoryConfig,
        project: &MemoryProjectInfo,
        force_llm: bool,
    ) -> Option<MemoryProjectProfileRefreshResult> {
        let generated = build_project_profile(&project.id, &project.name, &project.path)?;
        let memory_context = if force_llm {
            self.project_profile_memory_context(&project.id)
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let llm_source_fingerprint = if memory_context.is_empty() {
            generated.source_fingerprint.clone()
        } else {
            project_profile_llm_source_fingerprint(&generated.source_fingerprint, &memory_context)
        };
        let llm_fingerprint = llm_project_profile_fingerprint(&llm_source_fingerprint);
        let existing = self.current_project_profile(&project.id).ok().flatten();

        if !force_llm
            && existing
                .as_ref()
                .is_some_and(|profile| profile.source_fingerprint == llm_fingerprint)
        {
            return existing.map(|profile| MemoryProjectProfileRefreshResult {
                profile,
                used_llm: true,
                fallback_reason: None,
            });
        }

        if let Some(existing) = existing.as_ref() {
            if !force_llm && !project_profile_llm_refresh_due(existing, &generated) {
                if !project_profile_fingerprints_match(
                    &existing.source_fingerprint,
                    &generated.source_fingerprint,
                ) {
                    let profile = self
                        .upsert_project_profile(generated.clone())
                        .ok()
                        .unwrap_or(generated);
                    return Some(MemoryProjectProfileRefreshResult {
                        profile,
                        used_llm: false,
                        fallback_reason: Some("Repository fingerprint changed before LLM refresh was due; stored local scan.".to_string()),
                    });
                }
                return Some(MemoryProjectProfileRefreshResult {
                    profile: existing.clone(),
                    used_llm: existing.source_fingerprint.starts_with("llm-v1:"),
                    fallback_reason: None,
                });
            }
        }

        let Some(provider) = select_memory_provider(settings, None).cloned() else {
            let profile = self
                .upsert_project_profile(generated.clone())
                .ok()
                .unwrap_or(generated);
            return Some(MemoryProjectProfileRefreshResult {
                profile,
                used_llm: false,
                fallback_reason: Some(
                    "No enabled AI provider is configured for memory extraction; stored local scan."
                        .to_string(),
                ),
            });
        };

        let llm_profile_content =
            project_profile_content_with_memory_context(&generated.content, &memory_context);
        let prompt = make_project_profile_llm_prompt(&llm_profile_content);
        let response = llm::complete_with_provider_options(
            &provider,
            &prompt,
            Some(project_profile_system_prompt()),
            llm::LLMProviderCompletionOptions {
                max_tokens: 1400,
                temperature: 0.1,
                preserve_formatting: true,
                json_response: true,
                json_schema: None,
                timeout_seconds: 120,
            },
        )
        .await;

        match response {
            Ok(text) => match decode_project_profile_llm_response_detailed(&text) {
                Ok(content) => {
                    let profile = MemoryProjectProfile {
                        content,
                        source_fingerprint: llm_fingerprint,
                        ..generated
                    };
                    self.upsert_project_profile(profile.clone())
                        .ok()
                        .or(Some(profile))
                        .map(|profile| MemoryProjectProfileRefreshResult {
                            profile,
                            used_llm: true,
                            fallback_reason: None,
                        })
                }
                Err(error) => {
                    let profile = self
                        .upsert_project_profile(generated.clone())
                        .and_then(|profile| {
                            self.touch_project_profile(&project.id)?;
                            Ok(profile)
                        })
                        .ok()
                        .unwrap_or(generated);
                    Some(MemoryProjectProfileRefreshResult {
                        profile,
                        used_llm: false,
                        fallback_reason: Some(format!(
                            "LLM project profile decode failed: {error}; stored local scan. {}",
                            provider_summary(&provider)
                        )),
                    })
                }
            },
            Err(error) => {
                let profile = self
                    .upsert_project_profile(generated.clone())
                    .and_then(|profile| {
                        self.touch_project_profile(&project.id)?;
                        Ok(profile)
                    })
                    .ok()
                    .unwrap_or(generated);
                Some(MemoryProjectProfileRefreshResult {
                    profile,
                    used_llm: false,
                    fallback_reason: Some(format!(
                        "LLM request failed: {error}; stored local scan."
                    )),
                })
            }
        }
    }

    fn touch_project_profile(&self, project_id: &str) -> Result<(), String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.execute(
            "UPDATE memory_project_profiles SET updated_at = ?1 WHERE project_id = ?2;",
            params![now_seconds(), project_id],
        )
        .map_err(|error| error.to_string())?;
        Ok(())
    }

    fn upsert_project_profile(
        &self,
        profile: MemoryProjectProfile,
    ) -> Result<MemoryProjectProfile, String> {
        self.ensure_queue_schema()?;
        let now = now_seconds();
        let conn = self.open_connection()?;
        let existing = conn
            .query_row(
                r#"
                SELECT project_id, content, source_fingerprint, created_at, updated_at
                FROM memory_project_profiles
                WHERE project_id = ?1
                LIMIT 1;
                "#,
                params![profile.project_id],
                memory_project_profile_from_row,
            )
            .optional()
            .map_err(|error| error.to_string())?;
        if let Some(existing) = existing {
            if project_profile_fingerprints_match(
                &existing.source_fingerprint,
                &profile.source_fingerprint,
            ) {
                return Ok(existing);
            }
            conn.execute(
                r#"
                UPDATE memory_project_profiles
                SET content = ?1, source_fingerprint = ?2, updated_at = ?3
                WHERE project_id = ?4;
                "#,
                params![
                    profile.content,
                    profile.source_fingerprint,
                    now,
                    existing.project_id
                ],
            )
            .map_err(|error| error.to_string())?;
            return Ok(MemoryProjectProfile {
                created_at: existing.created_at,
                updated_at: now,
                ..profile
            });
        }
        conn.execute(
            r#"
            INSERT INTO memory_project_profiles (
                project_id, content, source_fingerprint, created_at, updated_at
            ) VALUES (?1, ?2, ?3, ?4, ?5);
            "#,
            params![
                profile.project_id,
                profile.content,
                profile.source_fingerprint,
                now,
                now
            ],
        )
        .map_err(|error| error.to_string())?;
        Ok(MemoryProjectProfile {
            created_at: now,
            updated_at: now,
            ..profile
        })
    }

    pub(super) fn current_project_profile(
        &self,
        project_id: &str,
    ) -> Result<Option<MemoryProjectProfile>, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        conn.query_row(
            r#"
            SELECT project_id, content, source_fingerprint, created_at, updated_at
            FROM memory_project_profiles
            WHERE project_id = ?1
            LIMIT 1;
            "#,
            params![project_id],
            memory_project_profile_from_row,
        )
        .optional()
        .map_err(|error| error.to_string())
    }

    fn project_profile_memory_context(&self, project_id: &str) -> Result<Vec<String>, String> {
        self.ensure_queue_schema()?;
        let conn = self.open_connection()?;
        let mut signals = Vec::new();
        if let Some((content, version)) = conn
            .query_row(
                r#"
                SELECT content, version
                FROM memory_summaries
                WHERE scope = 'project'
                  AND project_id = ?1
                  AND tool_id IS NULL
                LIMIT 1;
                "#,
                params![project_id],
                |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)),
            )
            .optional()
            .map_err(|error| error.to_string())?
            .and_then(|(content, version)| {
                normalized_string(Some(&content)).map(|content| (content, version))
            })
        {
            signals.push(format!(
                "Project summary v{}: {}",
                version,
                trim_memory_text(&content, 700)
            ));
        }

        let mut statement = conn
            .prepare(
                r#"
                SELECT COALESCE(module_key, 'general'), kind, content
                FROM memory_entries
                WHERE scope = 'project'
                  AND project_id = ?1
                  AND tier IN ('core', 'working')
                  AND status = 'active'
                  AND superseded_by IS NULL
                ORDER BY access_count DESC, updated_at DESC
                LIMIT 16;
                "#,
            )
            .map_err(|error| error.to_string())?;
        let rows = statement
            .query_map(params![project_id], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })
            .map_err(|error| error.to_string())?;
        for row in rows {
            let (module, kind, content) = row.map_err(|error| error.to_string())?;
            signals.push(format!(
                "{} / {}: {}",
                module,
                kind,
                trim_memory_text(&content, 180)
            ));
        }
        Ok(signals.into_iter().take(18).collect())
    }
}

fn memory_project_profile_from_row(
    row: &rusqlite::Row<'_>,
) -> rusqlite::Result<MemoryProjectProfile> {
    Ok(MemoryProjectProfile {
        project_id: row.get(0)?,
        content: row.get(1)?,
        source_fingerprint: row.get(2)?,
        created_at: row.get(3)?,
        updated_at: row.get(4)?,
    })
}

#[cfg(test)]
mod tests;
