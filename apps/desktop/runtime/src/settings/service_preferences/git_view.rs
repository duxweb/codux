impl SettingsService {
    pub fn set_git_file_view_mode(&self, mode: &str) -> Result<SettingsSummary, String> {
        let mode = sanitize_git_file_view_mode(mode);
        self.update_git_string("fileViewMode", mode)
    }

    pub fn cycle_git_file_view_mode(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().git_file_view_mode;
        let next = if current == "flatten" {
            "tree"
        } else {
            "flatten"
        };
        self.update_git_string("fileViewMode", next.to_string())
    }
    pub fn set_git_review_compare_mode(&self, mode: &str) -> Result<SettingsSummary, String> {
        let mode = sanitize_git_review_compare_mode(mode);
        self.update_git_string("reviewCompareMode", mode)
    }

    pub fn cycle_git_review_compare_mode(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().git_review_compare_mode;
        let next = if current == "originToWorkingTree" {
            "workingTree"
        } else {
            "originToWorkingTree"
        };
        self.update_git_string("reviewCompareMode", next.to_string())
    }
}
