use super::*;

impl CoduxApp {
    pub(in crate::app) fn reveal_selected_file_entry(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.run_selected_file_system_action("reveal", cx);
    }

    pub(in crate::app) fn open_selected_file_entry(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "no selected file entry to open".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        self.open_file_entry(entry, window, cx);
    }

    pub(in crate::app) fn open_selected_file_preview(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(entry) = self.selected_file_entry() else {
            self.status_message = "no selected file entry to preview".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        if matches!(entry.kind, FileKind::Directory) {
            self.status_message = "directories are previewed in the file sidebar".to_string();
            self.invalidate_file_panel(cx);
            return;
        }
        self.open_file_preview_window(entry.relative_path, cx);
    }

    pub(in crate::app) fn send_file_path_to_active_terminal(
        &mut self,
        relative_path: String,
        cx: &mut Context<Self>,
    ) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = "no selected project for terminal path".to_string();
            self.invalidate_file_panel(cx);
            return;
        };
        let full_path = codux_runtime::path::join_path(&project_path, &relative_path);
        self.send_to_active_terminal(&shell_quote(&full_path), cx);
        self.status_message = format!("file path sent to terminal: {relative_path}");
        self.invalidate_file_panel(cx);
    }

    pub(in crate::app) fn run_selected_file_system_action(
        &mut self,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_path) = self.selected_file_entry.clone() else {
            self.status_message = format!("no selected file entry to {action}");
            self.invalidate_file_panel(cx);
            return;
        };
        self.run_file_system_action(action, entry_path, cx);
    }

    pub(in crate::app) fn run_active_file_editor_file_system_action(
        &mut self,
        action: &str,
        cx: &mut Context<Self>,
    ) {
        let Some(entry_path) = self.active_file_editor_tab.clone() else {
            self.status_message = format!("no active file to {action}");
            self.invalidate_file_panel(cx);
            return;
        };
        self.run_file_system_action(action, entry_path, cx);
    }

    pub(in crate::app) fn run_file_entry_system_action(
        &mut self,
        action: &str,
        entry_path: String,
        cx: &mut Context<Self>,
    ) {
        self.run_file_system_action(action, entry_path, cx);
    }

    pub(in crate::app) fn open_file_entry_external(
        &mut self,
        entry_path: String,
        cx: &mut Context<Self>,
    ) {
        self.run_file_system_action("open", entry_path, cx);
    }

    fn run_file_system_action(&mut self, action: &str, entry_path: String, cx: &mut Context<Self>) {
        let Some(project_path) = self.selected_worktree_path() else {
            self.status_message = format!("no selected project for file {action}");
            self.invalidate_file_panel(cx);
            return;
        };
        let result = match action {
            "reveal" => self
                .runtime_service
                .reveal_project_file_entry(&project_path, &entry_path),
            "open" => self
                .runtime_service
                .open_project_file_entry(&project_path, &entry_path),
            _ => Err(format!("unknown file action: {action}")),
        };
        match result {
            Ok(()) => self.status_message = format!("file {action} requested: {entry_path}"),
            Err(error) => self.status_message = format!("failed to {action} file entry: {error}"),
        }
        self.invalidate_file_panel(cx);
    }
}
