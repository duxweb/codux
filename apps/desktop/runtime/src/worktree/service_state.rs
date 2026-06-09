impl WorktreeService {
    pub fn select_worktree(&self, project_id: &str, worktree_id: &str) -> Result<(), String> {
        let mut raw = raw_snapshot(&self.state_file);
        let exists = raw
            .get("worktrees")
            .and_then(Value::as_array)
            .map(|worktrees| {
                worktrees.iter().any(|worktree| {
                    let Some(worktree) = worktree.as_object() else {
                        return false;
                    };
                    worktree.get("projectId").and_then(Value::as_str) == Some(project_id)
                        && worktree.get("id").and_then(Value::as_str) == Some(worktree_id)
                })
            })
            .unwrap_or(false);
        if !exists && project_id != worktree_id {
            return Err("Worktree not found.".to_string());
        }
        if !matches!(
            raw.get("selectedWorktreeIdByProject"),
            Some(Value::Object(_))
        ) {
            raw.insert(
                "selectedWorktreeIdByProject".to_string(),
                Value::Object(Map::new()),
            );
        }
        raw.get_mut("selectedWorktreeIdByProject")
            .and_then(Value::as_object_mut)
            .ok_or_else(|| "selectedWorktreeIdByProject is not an object.".to_string())?
            .insert(
                project_id.to_string(),
                Value::String(worktree_id.to_string()),
            );
        save_raw_snapshot(&self.state_file, &raw)
    }

    pub fn sync_from_git(
        &self,
        project_id: &str,
        project_path: &str,
    ) -> Result<WorktreeSummary, String> {
        let snapshot = scan_git_worktrees(project_id, project_path)?;
        let mut raw = raw_snapshot(&self.state_file);
        merge_worktree_snapshot(&mut raw, project_id, snapshot)?;
        save_raw_snapshot(&self.state_file, &raw)?;
        Ok(self.summary(Some(project_id), Some(project_path)))
    }

    fn update_task_title(&self, worktree_id: &str, title: &str) -> Result<(), String> {
        let mut raw = raw_snapshot(&self.state_file);
        if let Some(tasks) = raw.get_mut("worktreeTasks").and_then(Value::as_array_mut) {
            for task in tasks {
                let Some(task) = task.as_object_mut() else {
                    continue;
                };
                if task.get("worktreeId").and_then(Value::as_str) == Some(worktree_id) {
                    task.insert("title".to_string(), Value::String(title.to_string()));
                }
            }
        }
        save_raw_snapshot(&self.state_file, &raw)
    }
}
