use super::{
    MemoryLaunchArtifacts, MemoryLaunchRequest, MemoryManagementRequest, MemoryManagementSnapshot,
    MemoryManagerSnapshot, MemoryManagerSnapshotRequest, MemoryService, count_queue,
    list_entries_for_management, list_summaries_for_management, normalized_non_empty,
    render_launch_memory_index,
};
use crate::MemoryProjectInfo;
use std::path::Path;

impl MemoryService {
    /// Open (creating if needed) the memory store under `support_dir`. The
    /// caller owns the location (desktop: `app_support_dir()`; host: its own).
    pub fn load_or_create(support_dir: std::path::PathBuf) -> Result<Self, String> {
        let service = Self::new(support_dir);
        service.ensure_queue_schema()?;
        Ok(service)
    }

    pub fn prepare_launch_artifacts(
        &self,
        runtime_root: &Path,
        request: MemoryLaunchRequest,
    ) -> Option<MemoryLaunchArtifacts> {
        let should_inject_memory =
            request.settings.memory.enabled && request.settings.memory.automatic_injection_enabled;
        let global_prompt = request.settings.global_prompt.as_str();
        let extra_context = request.extra_context.as_deref().unwrap_or_default();
        if !should_inject_memory
            && normalized_non_empty(global_prompt).is_none()
            && normalized_non_empty(extra_context).is_none()
        {
            return None;
        }

        let workspace_path = request.workspace_path.as_deref().unwrap_or_default();
        let input_hash = Self::launch_input_hash(&[
            &request.project_id,
            &request.project_name,
            workspace_path,
            global_prompt,
            extra_context,
            if should_inject_memory { "1" } else { "0" },
        ]);
        if Self::launch_artifacts_recently_prepared(&request.project_id, input_hash) {
            return Some(super::launch_artifact_paths(runtime_root, &request.project_id));
        }

        let project_profile = should_inject_memory
            .then(|| {
                self.project_profile_for_launch(
                    &request.project_id,
                    &request.project_name,
                    workspace_path,
                )
                .or_else(|| {
                    self.current_project_profile(&request.project_id)
                        .ok()
                        .flatten()
                })
            })
            .flatten();
        let summary = if should_inject_memory {
            self.summary_with_user_recall(
                Some(&request.project_id),
                request.settings.memory.allow_cross_project_user_recall,
            )
        } else {
            Default::default()
        };
        let artifacts = super::launch_artifact_paths(runtime_root, &request.project_id);
        let content = render_launch_memory_index(
            &request.project_id,
            &request.project_name,
            workspace_path,
            &summary,
            project_profile.as_ref(),
            Some(global_prompt),
            Some(extra_context),
        );

        self.write_launch_artifacts(&artifacts, &content, &super::render_recent_memory(&summary))?;
        Some(artifacts)
    }

    pub fn management_snapshot(
        &self,
        request: MemoryManagementRequest,
    ) -> Result<MemoryManagementSnapshot, String> {
        let conn = self.open_connection()?;
        let scope = super::normalize_scope(&request.scope);
        let project_id = (scope == "project")
            .then(|| request.project_id.as_deref())
            .flatten();
        let limit = request.limit.unwrap_or(100).clamp(1, 1000);
        Ok(MemoryManagementSnapshot {
            available: true,
            entries: list_entries_for_management(
                &conn,
                scope,
                project_id,
                request.tier.as_deref(),
                request.status.as_deref(),
                limit,
            )?,
            summaries: list_summaries_for_management(&conn, scope, project_id)?,
            extraction: super::MemoryExtractionSummary {
                queued: count_queue(&conn, &["queued", "pending"])?,
                running: count_queue(&conn, &["running"])?,
                failed: count_queue(&conn, &["failed"])?,
                last_error: super::latest_failed_queue_error(&conn)?,
            },
            error: None,
        })
    }

    pub fn manager_snapshot_for_request(
        &self,
        projects: &[MemoryProjectInfo],
        request: MemoryManagerSnapshotRequest,
    ) -> MemoryManagerSnapshot {
        self.manager_snapshot(
            projects,
            &request.scope,
            request.project_id.as_deref(),
            &request.tab,
            request.limit.unwrap_or(100),
        )
    }
}
