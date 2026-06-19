impl MemoryService {
    pub fn prepare_launch_artifacts_for_project(
        &self,
        runtime_root: &std::path::Path,
        project_id: &str,
        project_name: &str,
        workspace_path: &str,
    ) -> Option<MemoryLaunchArtifacts> {
        let input_hash = Self::launch_input_hash(&[project_id, project_name, workspace_path]);
        if Self::launch_artifacts_recently_prepared(project_id, input_hash) {
            return Some(launch_artifact_paths(runtime_root, project_id));
        }
        let project_profile = self
            .project_profile_for_launch(project_id, project_name, workspace_path)
            .or_else(|| self.current_project_profile(project_id).ok().flatten());
        let summary = self.summary(Some(project_id));
        if !summary.available && summary.recent_entries.is_empty() && project_profile.is_none() {
            return None;
        }

        let artifacts = launch_artifact_paths(runtime_root, project_id);
        let content = render_launch_memory_index(
            project_id,
            project_name,
            workspace_path,
            &summary,
            project_profile.as_ref(),
            None,
            None,
        );

        self.write_launch_artifacts(&artifacts, &content, &render_recent_memory(&summary))?;
        Some(artifacts)
    }

    /// Write the launch context files. The same content goes to the prompt file,
    /// MEMORY.md, and the per-agent AGENTS/CLAUDE/GEMINI files; memory-recent.md
    /// gets the recent block. Each file is only rewritten when its content
    /// actually changed, so the 8+ launch triggers don't churn the disk.
    pub(crate) fn write_launch_artifacts(
        &self,
        artifacts: &MemoryLaunchArtifacts,
        content: &str,
        recent: &str,
    ) -> Option<()> {
        fs::create_dir_all(&artifacts.workspace_root).ok()?;
        write_if_changed(&artifacts.prompt_file, content);
        write_if_changed(&artifacts.index_file, content);
        write_if_changed(&artifacts.workspace_root.join("memory-recent.md"), recent);
        write_if_changed(&artifacts.workspace_root.join("AGENTS.md"), content);
        write_if_changed(&artifacts.workspace_root.join("CLAUDE.md"), content);
        write_if_changed(&artifacts.workspace_root.join("GEMINI.md"), content);
        Some(())
    }

    /// Debounce repeated launch-artifact preparation for the same project + the
    /// same inputs. A burst of triggers (e.g. several terminal splits opened in
    /// a row) each rescans the repo for the project profile and rewrites the
    /// files; collapse them so only the first within the TTL does the work and
    /// the rest return the still-fresh on-disk artifacts. Memory changes are
    /// reflected on the next prepare after the (short) TTL.
    pub(crate) fn launch_artifacts_recently_prepared(project_id: &str, input_hash: u64) -> bool {
        // The debounce is a process-global; never short-circuit in tests, which
        // share the process and would otherwise see nondeterministic skips.
        if cfg!(test) {
            return false;
        }
        use std::sync::{LazyLock, Mutex};
        use std::time::{Duration, Instant};
        const TTL: Duration = Duration::from_secs(3);
        static DEBOUNCE: LazyLock<Mutex<std::collections::HashMap<String, (Instant, u64)>>> =
            LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));
        let Ok(mut map) = DEBOUNCE.lock() else {
            return false;
        };
        let now = Instant::now();
        if let Some((at, hash)) = map.get(project_id)
            && *hash == input_hash
            && now.duration_since(*at) < TTL
        {
            return true;
        }
        // Bound the map: it is keyed by project, so it only grows with distinct
        // projects, but clear it if it somehow gets large.
        if map.len() > 256 {
            map.clear();
        }
        map.insert(project_id.to_string(), (now, input_hash));
        false
    }

    pub(crate) fn launch_input_hash(parts: &[&str]) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for part in parts {
            part.hash(&mut hasher);
        }
        hasher.finish()
    }
}

fn write_if_changed(path: &std::path::Path, content: &str) {
    if fs::read_to_string(path).ok().as_deref() == Some(content) {
        return;
    }
    let _ = fs::write(path, content);
}
