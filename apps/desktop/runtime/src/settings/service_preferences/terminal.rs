impl SettingsService {
    fn set_numeric_string(
        &self,
        key: &str,
        value: &str,
        default: i64,
        min: i64,
        max: i64,
    ) -> Result<SettingsSummary, String> {
        self.update_string(key, numeric_string(value, default, min, max).to_string())
    }

    pub fn set_terminal_scrollback_lines(&self, lines: usize) -> Result<SettingsSummary, String> {
        self.update_string(
            "terminalScrollbackLines",
            lines.clamp(200, 10_000).to_string(),
        )
    }

    pub fn set_terminal_font_size(&self, size: &str) -> Result<SettingsSummary, String> {
        self.set_numeric_string("terminalFontSize", size, 14, 8, 28)
    }

    pub fn set_terminal_padding(&self, padding: &str) -> Result<SettingsSummary, String> {
        self.set_numeric_string("terminalPadding", padding, 0, 0, 40)
    }

    pub fn set_terminal_font_family(&self, family: &str) -> Result<SettingsSummary, String> {
        self.update_string("terminalFontFamily", sanitize_terminal_font_family(family))
    }

    pub fn toggle_terminal_paste_images_as_paths(&self) -> Result<SettingsSummary, String> {
        let mut raw = self.raw_settings();
        let current = raw
            .get("terminalPasteImagesAsPaths")
            .and_then(Value::as_bool)
            .unwrap_or(true);
        raw.insert(
            "terminalPasteImagesAsPaths".to_string(),
            Value::Bool(!current),
        );
        self.save_raw_settings(&raw)?;
        Ok(summary_from_raw(&raw))
    }

    pub fn set_terminal_scrollback_value(&self, lines: &str) -> Result<SettingsSummary, String> {
        self.set_numeric_string("terminalScrollbackLines", lines, 2000, 200, 10_000)
    }

    pub fn cycle_terminal_font_size(&self) -> Result<SettingsSummary, String> {
        let current = numeric_string(&self.summary().terminal_font_size, 14, 8, 28);
        let next = match current {
            8 => 10,
            10 => 12,
            12 => 14,
            14 => 16,
            16 => 18,
            18 => 20,
            20 => 24,
            24 => 28,
            28 => 8,
            _ => 14,
        };
        self.update_string("terminalFontSize", next.to_string())
    }

    pub fn cycle_terminal_scrollback_lines(&self) -> Result<SettingsSummary, String> {
        let current = numeric_string(&self.summary().terminal_scrollback_lines, 2000, 200, 10_000);
        let next = match current {
            500 => 1000,
            1000 => 2000,
            2000 => 5000,
            5000 => 10_000,
            10_000 => 500,
            _ => 500,
        };
        self.update_string("terminalScrollbackLines", next.to_string())
    }
}
