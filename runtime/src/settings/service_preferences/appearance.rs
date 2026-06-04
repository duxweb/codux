impl SettingsService {
    pub fn set_theme(&self, theme: &str) -> Result<SettingsSummary, String> {
        let value = sanitize_terminal_theme(theme);
        self.update_string("theme", value.to_string())
    }

    pub fn set_theme_color(&self, theme_color: &str) -> Result<SettingsSummary, String> {
        self.update_string("themeColor", sanitize_theme_color(theme_color))
    }

    pub fn set_icon_style(&self, icon_style: &str) -> Result<SettingsSummary, String> {
        self.update_string("iconStyle", sanitize_icon_style(icon_style))
    }

    pub fn cycle_theme(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().theme;
        let next = next_string_option(&current, terminal_theme_options(), "Auto");
        self.update_string("theme", next.to_string())
    }

    pub fn cycle_theme_color(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().theme_color;
        let next = next_string_option(
            &current,
            &[
                "Blue", "Sky", "Cyan", "Teal", "Emerald", "Green", "Lime", "Amber", "Orange",
                "Red", "Rose", "Pink", "Fuchsia", "Purple", "Violet", "Indigo",
            ],
            "Blue",
        );
        self.update_string("themeColor", next.to_string())
    }

    pub fn cycle_icon_style(&self) -> Result<SettingsSummary, String> {
        let current = self.summary().icon_style;
        let next = next_string_option(
            &current,
            &["default", "cobalt", "sunset", "forest"],
            "default",
        );
        self.update_string("iconStyle", next.to_string())
    }
}
