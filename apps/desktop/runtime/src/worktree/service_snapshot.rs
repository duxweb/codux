impl WorktreeService {
    pub fn snapshot(&self, project_id: String, project_path: String) -> WorktreeSnapshot {
        let now = now_seconds();
        let root_path =
            repository_root(&project_path).unwrap_or_else(|| normalize_path(&project_path));
        let default_branch = current_branch(&root_path).unwrap_or_else(|| "main".to_string());
        match scan_git_worktrees(&project_id, &project_path) {
            Ok(scanned) => self.snapshot_from_scanned(project_id, scanned, None),
            Err(error) => WorktreeSnapshot {
                project_id: project_id.clone(),
                selected_worktree_id: project_id.clone(),
                worktrees: vec![project_worktree_snapshot(
                    project_id.clone(),
                    project_id.clone(),
                    default_branch.clone(),
                    default_branch,
                    root_path,
                    "todo".to_string(),
                    true,
                    now,
                )],
                tasks: Vec::new(),
                error: Some(error),
            },
        }
    }

    fn snapshot_from_scanned(
        &self,
        project_id: String,
        mut scanned: ScannedWorktreeSnapshot,
        error: Option<String>,
    ) -> WorktreeSnapshot {
        enrich_scanned_snapshot_from_state(&self.state_file, &mut scanned);
        let selected_worktree_id =
            selected_worktree_id_from_state(&self.state_file, &project_id, &scanned.worktrees)
                .unwrap_or_else(|| scanned.selected_worktree_id.clone());
        WorktreeSnapshot {
            project_id,
            selected_worktree_id,
            worktrees: scanned
                .worktrees
                .into_iter()
                .map(scanned_worktree_to_snapshot)
                .collect(),
            tasks: scanned
                .tasks
                .into_iter()
                .map(scanned_task_to_snapshot)
                .collect(),
            error,
        }
    }
}
