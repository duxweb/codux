use super::{
    ProjectStore, TerminalLayoutRecord, TerminalLayoutsSnapshot, helpers::is_known_workspace_id,
    terminal_layout::sanitize_terminal_layout,
};
use crate::terminal_layout::terminal_layout_cache_namespace;

impl ProjectStore {
    pub fn terminal_layout(&self, project_id: &str) -> Option<TerminalLayoutRecord> {
        self.terminal_layouts_snapshot()
            .layouts
            .get(project_id)
            .cloned()
    }

    pub fn terminal_layouts_snapshot(&self) -> TerminalLayoutsSnapshot {
        let layouts =
            crate::persistent_cache::PersistentCacheStore::for_file(self.state_cache_file())
                .and_then(|cache| {
                    cache.scan_json::<TerminalLayoutRecord>(terminal_layout_cache_namespace())
                })
                .unwrap_or_default();
        TerminalLayoutsSnapshot { layouts }
    }

    pub fn save_terminal_layout(
        &self,
        project_id: String,
        layout: TerminalLayoutRecord,
    ) -> Result<TerminalLayoutRecord, String> {
        let snapshot = self.snapshot();
        if !is_known_workspace_id(&snapshot, &project_id) {
            return Err("Project workspace not found.".to_string());
        }
        let layout = sanitize_terminal_layout(layout)
            .ok_or_else(|| "Terminal layout is empty.".to_string())?;
        crate::persistent_cache::PersistentCacheStore::for_file(self.state_cache_file())?
            .put_json(terminal_layout_cache_namespace(), &project_id, &layout)?;
        Ok(layout)
    }
}
