impl MemoryService {
    pub fn prepare_launch_artifacts_for_project(
        &self,
        project_id: &str,
        project_name: &str,
        workspace_path: &str,
    ) -> Option<MemoryLaunchArtifacts> {
        let project_profile = self
            .project_profile_for_launch(project_id, project_name, workspace_path)
            .or_else(|| self.current_project_profile(project_id).ok().flatten());
        let summary = self.summary(Some(project_id));
        if !summary.available && summary.recent_entries.is_empty() && project_profile.is_none() {
            return None;
        }

        let artifacts = launch_artifact_paths(project_id);
        let content = render_launch_memory_index(
            project_id,
            project_name,
            workspace_path,
            &summary,
            project_profile.as_ref(),
            None,
            None,
        );

        fs::create_dir_all(&artifacts.workspace_root).ok()?;
        fs::write(&artifacts.prompt_file, &content).ok()?;
        fs::write(&artifacts.index_file, &content).ok()?;
        fs::write(
            artifacts.workspace_root.join("memory-recent.md"),
            render_recent_memory(&summary),
        )
        .ok()?;
        fs::write(artifacts.workspace_root.join("AGENTS.md"), &content).ok()?;
        fs::write(artifacts.workspace_root.join("CLAUDE.md"), &content).ok()?;
        fs::write(artifacts.workspace_root.join("GEMINI.md"), &content).ok()?;

        Some(artifacts)
    }
}
