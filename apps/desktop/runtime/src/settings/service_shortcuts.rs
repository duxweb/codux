impl SettingsService {
    pub fn set_shortcut(&self, shortcut_id: &str, value: &str) -> Result<SettingsSummary, String> {
        let shortcut_id = sanitize_shortcut_id(shortcut_id)?;
        let mut raw = self.raw_settings();
        let shortcuts = shortcuts_mut(&mut raw)?;
        shortcuts.insert(
            shortcut_id,
            Value::String(value.trim().chars().take(120).collect()),
        );
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn reset_shortcut(&self, shortcut_id: &str) -> Result<SettingsSummary, String> {
        let shortcut_id = sanitize_shortcut_id(shortcut_id)?;
        let mut raw = self.raw_settings();
        let shortcuts = shortcuts_mut(&mut raw)?;
        shortcuts.remove(&shortcut_id);
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }
}
