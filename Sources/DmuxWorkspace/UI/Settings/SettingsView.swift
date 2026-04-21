import AppKit
import SwiftUI

private enum SettingsSectionTab: String, CaseIterable, Identifiable {
    case general
    case appearance
    case pet
    case tools
    case notifications
    case shortcuts
    case developer

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .general: return "gearshape"
        case .appearance: return "paintbrush"
        case .pet: return "pawprint"
        case .tools: return "terminal"
        case .notifications: return "bell.badge"
        case .shortcuts: return "keyboard"
        case .developer: return "wrench.and.screwdriver"
        }
    }

    var preferredContentHeight: CGFloat {
        switch self {
        case .general:
            return 390
        case .appearance:
            return 760
        case .pet:
            return 430
        case .tools:
            return 360
        case .notifications:
            return 620
        case .shortcuts:
            return 320
        case .developer:
            return 220
        }
    }
}

struct SettingsView: View {
    let model: AppModel
    @State private var selectedTab: SettingsSectionTab = .general

    var body: some View {
        TabView(selection: $selectedTab) {
            GeneralSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.general", defaultValue: "General", bundle: .module), systemImage: SettingsSectionTab.general.symbol)
                }
                .tag(SettingsSectionTab.general)

            AppearanceSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.appearance", defaultValue: "Appearance", bundle: .module), systemImage: SettingsSectionTab.appearance.symbol)
                }
                .tag(SettingsSectionTab.appearance)

            PetSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.pet", defaultValue: "Pet", bundle: .module), systemImage: SettingsSectionTab.pet.symbol)
                }
                .tag(SettingsSectionTab.pet)

            ToolSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.tools", defaultValue: "Tools", bundle: .module), systemImage: SettingsSectionTab.tools.symbol)
                }
                .tag(SettingsSectionTab.tools)

            NotificationSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.notifications", defaultValue: "Notifications", bundle: .module), systemImage: SettingsSectionTab.notifications.symbol)
                }
                .tag(SettingsSectionTab.notifications)

            ShortcutSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.shortcuts", defaultValue: "Shortcuts", bundle: .module), systemImage: SettingsSectionTab.shortcuts.symbol)
                }
                .tag(SettingsSectionTab.shortcuts)

            DeveloperSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.developer", defaultValue: "Developer", bundle: .module), systemImage: SettingsSectionTab.developer.symbol)
                }
                .tag(SettingsSectionTab.developer)
        }
        .frame(width: 640, height: selectedTab.preferredContentHeight)
        .background(Color(nsColor: .windowBackgroundColor))
        .background(
            SettingsWindowConfigurator(
                title: String(localized: "menu.settings", defaultValue: "Settings", bundle: .module),
                contentSize: NSSize(width: 640, height: selectedTab.preferredContentHeight)
            )
        )
    }
}

// MARK: - General

private struct GeneralSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Picker(String(localized: "settings.language", defaultValue: "Language", bundle: .module), selection: Binding(
                get: { model.appSettings.language },
                set: { model.updateLanguage($0) }
            )) {
                ForEach(AppLanguage.allCases) { language in
                    Text(language.title).tag(language)
                }
            }

            Picker(String(localized: "settings.default_shell", defaultValue: "Default Shell", bundle: .module), selection: Binding(
                get: { model.appSettings.defaultTerminal },
                set: { model.updateDefaultTerminal($0) }
            )) {
                ForEach(AppTerminalProfile.available) { terminal in
                    Text(terminal.title).tag(terminal)
                }
            }

            Toggle(String(localized: "settings.dock_badge", defaultValue: "Dock Badge", bundle: .module), isOn: Binding(
                get: { model.appSettings.showsDockBadge },
                set: { model.updateDockBadgeEnabled($0) }
            ))

            Picker(String(localized: "settings.git_auto_refresh", defaultValue: "Git Auto Refresh", bundle: .module), selection: Binding(
                get: { model.appSettings.gitAutoRefreshInterval },
                set: { model.updateGitAutoRefreshInterval($0) }
            )) {
                ForEach(RefreshIntervalOption.gitOptions, id: \.seconds) { option in
                    Text(option.title(model: model)).tag(option.seconds)
                }
            }

            Picker(String(localized: "settings.ai_auto_refresh", defaultValue: "AI Auto Refresh", bundle: .module), selection: Binding(
                get: { model.appSettings.aiAutoRefreshInterval },
                set: { model.updateAIAutomaticRefreshInterval($0) }
            )) {
                ForEach(RefreshIntervalOption.aiOptions, id: \.seconds) { option in
                    Text(option.title(model: model)).tag(option.seconds)
                }
            }

            Picker(String(localized: "settings.ai_background_refresh", defaultValue: "AI Background Refresh", bundle: .module), selection: Binding(
                get: { model.appSettings.aiBackgroundRefreshInterval },
                set: { model.updateAIBackgroundRefreshInterval($0) }
            )) {
                ForEach(RefreshIntervalOption.backgroundAIOptions, id: \.seconds) { option in
                    Text(option.title(model: model)).tag(option.seconds)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct PetSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section(String(localized: "settings.pet.section.general", defaultValue: "General", bundle: .module)) {
                Toggle(String(localized: "settings.pet.enabled", defaultValue: "Enable Pet", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.enabled },
                    set: { model.updatePetEnabled($0) }
                ))

                Toggle(String(localized: "settings.pet.static_mode", defaultValue: "Static Pet Sprite", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.staticMode },
                    set: { model.updatePetStaticMode($0) }
                ))
            }

            Section(String(localized: "settings.pet.section.reminders", defaultValue: "Reminders", bundle: .module)) {
                Toggle(String(localized: "settings.pet.reminder.hydration", defaultValue: "Hydration Reminder", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.hydrationReminderEnabled },
                    set: { model.updatePetHydrationReminderEnabled($0) }
                ))

                if model.appSettings.pet.hydrationReminderEnabled {
                    Picker(String(localized: "settings.pet.reminder.hydration_interval", defaultValue: "Hydration Interval", bundle: .module), selection: Binding(
                        get: { model.appSettings.pet.hydrationReminderInterval },
                        set: { model.updatePetHydrationReminderInterval($0) }
                    )) {
                        ForEach(RefreshIntervalOption.petReminderOptions, id: \.seconds) { option in
                            Text(option.title(model: model)).tag(option.seconds)
                        }
                    }
                }

                Toggle(String(localized: "settings.pet.reminder.sedentary", defaultValue: "Sedentary Reminder", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.sedentaryReminderEnabled },
                    set: { model.updatePetSedentaryReminderEnabled($0) }
                ))

                if model.appSettings.pet.sedentaryReminderEnabled {
                    Picker(String(localized: "settings.pet.reminder.sedentary_interval", defaultValue: "Sedentary Interval", bundle: .module), selection: Binding(
                        get: { model.appSettings.pet.sedentaryReminderInterval },
                        set: { model.updatePetSedentaryReminderInterval($0) }
                    )) {
                        ForEach(RefreshIntervalOption.petReminderOptions, id: \.seconds) { option in
                            Text(option.title(model: model)).tag(option.seconds)
                        }
                    }
                }

                Toggle(String(localized: "settings.pet.reminder.late_night", defaultValue: "Late-Night Reminder", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.lateNightReminderEnabled },
                    set: { model.updatePetLateNightReminderEnabled($0) }
                ))

                if model.appSettings.pet.lateNightReminderEnabled {
                    Picker(String(localized: "settings.pet.reminder.late_night_interval", defaultValue: "Late-Night Interval", bundle: .module), selection: Binding(
                        get: { model.appSettings.pet.lateNightReminderInterval },
                        set: { model.updatePetLateNightReminderInterval($0) }
                    )) {
                        ForEach(RefreshIntervalOption.petReminderOptions, id: \.seconds) { option in
                            Text(option.title(model: model)).tag(option.seconds)
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

// MARK: - Appearance

private struct AppearanceSettingsPane: View {
    let model: AppModel

    private var darkPresets: [AppTerminalBackgroundPreset] {
        AppTerminalBackgroundPreset.allCases.filter { !$0.isAutomatic && $0.isLight == false }
    }
    private var lightPresets: [AppTerminalBackgroundPreset] {
        AppTerminalBackgroundPreset.allCases.filter { !$0.isAutomatic && $0.isLight == true }
    }

    var body: some View {
        Form {
            // ── Theme ────────────────────────────────────────────────────
            Section(String(localized: "settings.theme", defaultValue: "Theme", bundle: .module)) {
                VStack(alignment: .leading, spacing: 14) {
                    // Auto
                    let autoPreset = AppTerminalBackgroundPreset.automatic
                    themeGrid(
                        header: nil,
                        presets: [autoPreset]
                    )

                    // Dark
                    themeGrid(
                        header: String(localized: "settings.theme.group.dark", defaultValue: "Dark", bundle: .module),
                        presets: darkPresets
                    )

                    // Light
                    themeGrid(
                        header: String(localized: "settings.theme.group.light", defaultValue: "Light", bundle: .module),
                        presets: lightPresets
                    )
                }
                .padding(.vertical, 6)
            }

            // ── Background Color ─────────────────────────────────────────
            Section(String(localized: "settings.background_color", defaultValue: "Background Color", bundle: .module)) {
                let fallback = model.appSettings.terminalBackgroundPreset
                    .effectiveAppearance(
                        backgroundColorPreset: .automatic,
                        automaticAppearance: model.automaticTerminalAppearance
                    )
                    .backgroundColor
                ColorSwatchGrid(
                    presets: AppBackgroundColorPreset.allCases,
                    selectedPreset: model.appSettings.backgroundColorPreset,
                    fallbackColor: fallback
                ) { model.updateBackgroundColorPreset($0) }
                .padding(.vertical, 4)
            }

            // ── Terminal Text ────────────────────────────────────────────
            Section(String(localized: "settings.terminal_text", defaultValue: "Terminal Text", bundle: .module)) {
                LabeledContent(String(localized: "settings.terminal_font_size", defaultValue: "Terminal Font Size", bundle: .module)) {
                    HStack(spacing: 8) {
                        TextField(
                            "",
                            value: Binding(
                                get: { model.appSettings.terminalFontSize },
                                set: { model.updateTerminalFontSize($0) }
                            ),
                            format: .number
                        )
                        .labelsHidden()
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 58)

                        Stepper(
                            "",
                            value: Binding(
                                get: { model.appSettings.terminalFontSize },
                                set: { model.updateTerminalFontSize($0) }
                            ),
                            in: 10...28
                        )
                        .labelsHidden()
                    }
                }
            }

            // ── App Icon ─────────────────────────────────────────────────
            Section(String(localized: "settings.app_icon", defaultValue: "App Icon", bundle: .module)) {
                HStack(spacing: 16) {
                    ForEach(AppIconStyle.allCases) { style in
                        AppIconPreviewCard(
                            title: style.title,
                            style: style,
                            isSelected: model.appSettings.iconStyle == style
                        ) {
                            model.updateAppIconStyle(style)
                        }
                    }
                }
                .padding(.vertical, 4)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private func themeGrid(header: String?, presets: [AppTerminalBackgroundPreset]) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            if let header {
                Text(header)
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
            }
            LazyVGrid(
                columns: [GridItem(.adaptive(minimum: 96, maximum: 120), spacing: 8)],
                alignment: .leading,
                spacing: 8
            ) {
                ForEach(presets) { preset in
                    ThemePreviewCard(
                        title: preset.title,
                        appearance: preset.effectiveAppearance(
                            backgroundColorPreset: .automatic,
                            automaticAppearance: model.automaticTerminalAppearance
                        ),
                        isSelected: model.appSettings.terminalBackgroundPreset == preset
                    ) {
                        model.updateTerminalBackgroundPreset(preset)
                    }
                }
            }
        }
    }
}

// macOS-native style theme card: colour preview on top, name below,
// selection ring around the whole card (like System Settings wallpapers).
private struct ThemePreviewCard: View {
    let title: String
    let appearance: AppEffectiveTerminalAppearance
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 5) {
                // Preview area
                ZStack(alignment: .topLeading) {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Color(nsColor: appearance.backgroundColor))
                        .frame(height: 46)

                    // Fake terminal lines
                    VStack(alignment: .leading, spacing: 3) {
                        Capsule()
                            .fill(Color(nsColor: appearance.mutedForegroundColor)
                                .opacity(appearance.isLight ? 0.30 : 0.25))
                            .frame(width: 14, height: 2.5)
                        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                            .fill(Color(nsColor: appearance.foregroundColor)
                                .opacity(appearance.isLight ? 0.80 : 0.90))
                            .frame(width: 28, height: 3)
                        RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                            .fill(Color(nsColor: appearance.mutedForegroundColor)
                                .opacity(appearance.isLight ? 0.55 : 0.65))
                            .frame(width: 20, height: 2.5)
                    }
                    .padding(7)
                }
                // Selection ring wraps just the preview
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(
                            isSelected ? Color.accentColor : Color(nsColor: .separatorColor).opacity(0.5),
                            lineWidth: isSelected ? 2 : 0.5
                        )
                )

                // Name
                Text(title)
                    .font(.system(size: 11, weight: isSelected ? .semibold : .regular))
                    .foregroundStyle(isSelected ? Color.primary : Color.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.85)
                    .frame(maxWidth: .infinity, alignment: .center)
            }
        }
        .buttonStyle(.plain)
    }
}

// Grid of color swatches (circle shape, smaller footprint).
private struct ColorSwatchGrid: View {
    let presets: [AppBackgroundColorPreset]
    let selectedPreset: AppBackgroundColorPreset
    let fallbackColor: NSColor
    let onSelect: (AppBackgroundColorPreset) -> Void

    private let columns = [GridItem(.adaptive(minimum: 44, maximum: 52), spacing: 8)]

    var body: some View {
        LazyVGrid(columns: columns, alignment: .leading, spacing: 10) {
            ForEach(presets) { preset in
                ColorSwatch(
                    title: preset.title,
                    swatchColor: preset.swatchColor ?? fallbackColor,
                    isAutomatic: preset.isAutomatic,
                    isSelected: selectedPreset == preset
                ) { onSelect(preset) }
            }
        }
    }
}

private struct ColorSwatch: View {
    let title: String
    let swatchColor: NSColor
    let isAutomatic: Bool
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 4) {
                ZStack {
                    Circle()
                        .fill(Color(nsColor: swatchColor))
                        .frame(width: 28, height: 28)
                        .overlay(
                            Circle()
                                .stroke(
                                    isSelected ? Color.accentColor : Color(nsColor: .separatorColor).opacity(0.4),
                                    lineWidth: isSelected ? 2.5 : 0.5
                                )
                        )
                        .shadow(color: .black.opacity(0.12), radius: 2, x: 0, y: 1)

                    if isAutomatic {
                        Text("A")
                            .font(.system(size: 11, weight: .bold, design: .rounded))
                            .foregroundStyle(Color(nsColor: swatchColor.dmuxPreviewTextColor))
                    }
                }

                Text(title)
                    .font(.system(size: 9.5, weight: isSelected ? .semibold : .regular))
                    .foregroundStyle(isSelected ? Color.primary : Color.secondary)
                    .lineLimit(1)
                    .minimumScaleFactor(0.8)
                    .frame(width: 44)
                    .multilineTextAlignment(.center)
            }
        }
        .buttonStyle(.plain)
    }
}

private struct ToolSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section(String(localized: "settings.tools.permissions", defaultValue: "Tool Permissions", bundle: .module)) {
                permissionRow(tool: .codex)
                permissionRow(tool: .claudeCode)
                permissionRow(tool: .gemini)
                permissionRow(tool: .opencode)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private func permissionRow(tool: AppSupportedAITool) -> some View {
        LabeledContent(tool.title) {
            Toggle(
                "",
                isOn: Binding(
                    get: { tool.permissionMode(from: model.appSettings.toolPermissions) == .fullAccess },
                    set: { isEnabled in
                        model.updateToolPermissionMode(isEnabled ? .fullAccess : .default, for: tool)
                    }
                )
            )
            .labelsHidden()
            .toggleStyle(.switch)
        }
    }
}

// MARK: - Shortcuts

private struct NotificationSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            ForEach(AppNotificationChannel.allCases) { channel in
                NotificationChannelCard(model: model, channel: channel)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct NotificationChannelCard: View {
    let model: AppModel
    let channel: AppNotificationChannel

    private var configuration: AppNotificationChannelConfiguration {
        channel.configuration(from: model.appSettings.notifications)
    }

    private var isEnabled: Bool {
        configuration.isEnabled
    }

    var body: some View {
        Section {
            headerRow

            if isEnabled {
                fieldBlock(
                    label: channel.endpointLabel,
                    placeholder: channel.endpointPlaceholder,
                    isSecure: false,
                    text: Binding(
                        get: { configuration.endpoint },
                        set: { model.updateNotificationChannelEndpoint($0, for: channel) }
                    )
                )

                if channel.showsTokenField {
                    fieldBlock(
                        label: channel.tokenLabel,
                        placeholder: channel.tokenPlaceholder,
                        isSecure: true,
                        text: Binding(
                            get: { configuration.token },
                            set: { model.updateNotificationChannelToken($0, for: channel) }
                        )
                    )
                }
            }
        }
    }

    private var headerRow: some View {
        HStack(spacing: 12) {
            ZStack {
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(channel.accentColor.opacity(0.15))
                    .frame(width: 30, height: 30)

                Image(systemName: channel.symbolName)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(channel.accentColor)
            }

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(channel.localizedTitle)
                        .font(.system(size: 13, weight: .semibold))
                        .foregroundStyle(.primary)

                    if let url = channel.websiteURL {
                        Button {
                            model.openURL(url)
                        } label: {
                            Image(systemName: "arrow.up.right")
                                .font(.system(size: 10, weight: .semibold))
                                .foregroundStyle(.tertiary)
                                .padding(2)
                                .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        .help(url.absoluteString)
                    }
                }

                Text(channel.descriptionText)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Spacer()

            Toggle(
                "",
                isOn: Binding(
                    get: { isEnabled },
                    set: { model.updateNotificationChannelEnabled($0, for: channel) }
                )
            )
            .labelsHidden()
            .toggleStyle(.switch)
            .controlSize(.small)
        }
        .padding(.vertical, 2)
    }

    @ViewBuilder
    private func fieldBlock(label: String, placeholder: String, isSecure: Bool, text: Binding<String>) -> some View {
        HStack(alignment: .center, spacing: 14) {
            Text(label)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .frame(width: 118, alignment: .leading)

            Spacer(minLength: 0)

            Group {
                if isSecure {
                    SecureField(
                        "",
                        text: text,
                        prompt: Text(placeholder)
                    )
                } else {
                    TextField(
                        "",
                        text: text,
                        prompt: Text(placeholder)
                    )
                }
            }
            .labelsHidden()
            .textFieldStyle(.plain)
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .fill(Color(nsColor: .textBackgroundColor))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 6, style: .continuous)
                    .stroke(Color(nsColor: .separatorColor).opacity(0.5), lineWidth: 0.5)
            )
            .frame(width: 360, alignment: .trailing)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private extension AppNotificationChannel {
    var localizedTitle: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.title", defaultValue: "Bark", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.title", defaultValue: "ntfy", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.title", defaultValue: "WxPusher", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.title", defaultValue: "Feishu", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.title", defaultValue: "DingTalk", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.title", defaultValue: "WeCom", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.title", defaultValue: "Telegram", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.title", defaultValue: "Discord", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.title", defaultValue: "Slack", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.title", defaultValue: "Webhook", bundle: .module)
        }
    }

    var descriptionText: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.description", defaultValue: "Send push alerts through a Bark server with your device key.", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.description", defaultValue: "Publish messages to an ntfy topic. Add a bearer token only when your server requires it.", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.description", defaultValue: "Send notifications to a WxPusher SPT target. No extra token is required.", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.description", defaultValue: "Post messages with a Feishu bot webhook. You can fill either the full URL or the hook token.", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.description", defaultValue: "Post messages with a DingTalk robot webhook. You can fill either the full URL or the access token.", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.description", defaultValue: "Post messages to a WeCom group bot. You can fill either the full URL or the webhook key.", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.description", defaultValue: "Send messages with a Telegram bot token and target chat ID.", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.description", defaultValue: "Deliver notifications to a Discord webhook. Optional auth token is only needed for custom gateways.", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.description", defaultValue: "Deliver notifications to a Slack incoming webhook. Optional auth token is only needed for custom gateways.", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.description", defaultValue: "Send JSON POST requests to your own endpoint. Add a bearer token if the receiver requires authorization.", bundle: .module)
        }
    }

    var endpointLabel: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.endpoint", defaultValue: "Server URL", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.endpoint", defaultValue: "Topic URL", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.endpoint", defaultValue: "SPT Token", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.endpoint", defaultValue: "Chat ID", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.endpoint", defaultValue: "Webhook URL", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.endpoint", defaultValue: "Request URL", bundle: .module)
        }
    }

    var tokenLabel: String {
        switch self {
        case .bark:
            return String(localized: "settings.notifications.channel.bark.token", defaultValue: "Device Key", bundle: .module)
        case .ntfy:
            return String(localized: "settings.notifications.channel.ntfy.token", defaultValue: "Bearer Token", bundle: .module)
        case .wxpusher:
            return String(localized: "settings.notifications.channel.wxpusher.token", defaultValue: "Token", bundle: .module)
        case .feishu:
            return String(localized: "settings.notifications.channel.feishu.token", defaultValue: "Hook Token", bundle: .module)
        case .dingTalk:
            return String(localized: "settings.notifications.channel.dingtalk.token", defaultValue: "Access Token", bundle: .module)
        case .weCom:
            return String(localized: "settings.notifications.channel.wecom.token", defaultValue: "Webhook Key", bundle: .module)
        case .telegram:
            return String(localized: "settings.notifications.channel.telegram.token", defaultValue: "Bot Token", bundle: .module)
        case .discord:
            return String(localized: "settings.notifications.channel.discord.token", defaultValue: "Optional Auth Token", bundle: .module)
        case .slack:
            return String(localized: "settings.notifications.channel.slack.token", defaultValue: "Optional Auth Token", bundle: .module)
        case .webhook:
            return String(localized: "settings.notifications.channel.webhook.token", defaultValue: "Bearer Token", bundle: .module)
        }
    }

    var symbolName: String {
        switch self {
        case .bark: return "bell.badge.fill"
        case .ntfy: return "bell.and.waves.left.and.right.fill"
        case .wxpusher: return "message.fill"
        case .feishu: return "sparkles"
        case .dingTalk: return "megaphone.fill"
        case .weCom: return "person.3.fill"
        case .telegram: return "paperplane.fill"
        case .discord: return "bubble.left.and.bubble.right.fill"
        case .slack: return "number.square.fill"
        case .webhook: return "link"
        }
    }

    var accentColor: Color {
        switch self {
        case .bark: return .orange
        case .ntfy: return .blue
        case .wxpusher: return .green
        case .feishu: return .teal
        case .dingTalk: return .cyan
        case .weCom: return .mint
        case .telegram: return Color(red: 0.15, green: 0.53, blue: 0.88)
        case .discord: return Color(red: 0.44, green: 0.43, blue: 0.87)
        case .slack: return Color(red: 0.89, green: 0.20, blue: 0.53)
        case .webhook: return .gray
        }
    }
}

// MARK: - Shortcuts

private struct ShortcutSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section {
                shortcutRow(String(localized: "settings.shortcut.create_split", defaultValue: "Create Split", bundle: .module), target: .splitPane, value: model.appSettings.shortcuts.splitPane)
                shortcutRow(String(localized: "settings.shortcut.create_tab", defaultValue: "Create Tab", bundle: .module), target: .createTab, value: model.appSettings.shortcuts.createTab)
                shortcutRow(String(localized: "settings.shortcut.open_git_panel", defaultValue: "Git Panel", bundle: .module), target: .toggleGitPanel, value: model.appSettings.shortcuts.toggleGitPanel)
                shortcutRow(String(localized: "settings.shortcut.open_ai_panel", defaultValue: "AI Panel", bundle: .module), target: .toggleAIPanel, value: model.appSettings.shortcuts.toggleAIPanel)
            }

            Section(String(localized: "settings.shortcut.project_switch", defaultValue: "Project Switch Shortcuts", bundle: .module)) {
                Text(String(localized: "settings.shortcut.project_switch_hint", defaultValue: "Use ⌘1-⌘9 to switch projects in sidebar order.", bundle: .module))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private func shortcutRow(_ title: String, target: AppShortcutTarget, value: AppKeyboardShortcut?) -> some View {
        LabeledContent(title) {
            ShortcutRecorderField(
                value: value,
                placeholder: String(localized: "settings.shortcut.record", defaultValue: "Record Shortcut", bundle: .module)
            ) { shortcut in
                model.updateShortcut(shortcut, for: target)
            }
        }
    }
}

private struct DeveloperSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Toggle(String(localized: "settings.developer.performance_monitor", defaultValue: "Performance Monitor HUD", bundle: .module), isOn: Binding(
                get: { model.appSettings.developer.showsPerformanceMonitor },
                set: { model.updateDeveloperPerformanceMonitorEnabled($0) }
            ))

            Picker(String(localized: "settings.developer.performance_monitor_interval", defaultValue: "Performance Monitor Interval", bundle: .module), selection: Binding(
                get: { model.appSettings.developer.performanceMonitorSamplingInterval },
                set: { model.updateDeveloperPerformanceMonitorSamplingInterval($0) }
            )) {
                ForEach(RefreshIntervalOption.performanceMonitorOptions, id: \.seconds) { option in
                    Text(option.title(model: model)).tag(option.seconds)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct SettingsWindowConfigurator: NSViewRepresentable {
    let title: String
    let contentSize: NSSize

    func makeNSView(context: Context) -> NSView {
        ConfigView(title: title, contentSize: contentSize)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let configView = nsView as? ConfigView else {
            return
        }
        configView.title = title
        configView.contentSize = contentSize
        configView.applyWindowConfigurationIfNeeded()
    }

    private final class ConfigView: NSView {
        var title: String
        var contentSize: NSSize
        private var lastAppliedFrameSize: NSSize?

        init(title: String, contentSize: NSSize) {
            self.title = title
            self.contentSize = contentSize
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            applyWindowConfigurationIfNeeded()
            DispatchQueue.main.async { [weak self] in
                self?.applyWindowConfigurationIfNeeded()
            }
        }

        func applyWindowConfigurationIfNeeded() {
            guard let window else {
                return
            }
            window.identifier = AppWindowIdentifier.settings
            applyStandardWindowChrome(window, title: title, toolbarStyle: .preference)

            let targetContentRect = NSRect(origin: .zero, size: contentSize)
            let targetFrame = window.frameRect(forContentRect: targetContentRect)
            let targetFrameSize = targetFrame.size

            guard lastAppliedFrameSize != targetFrameSize
                || abs(window.frame.size.width - targetFrameSize.width) > 0.5
                || abs(window.frame.size.height - targetFrameSize.height) > 0.5 else {
                return
            }

            var nextFrame = window.frame
            nextFrame.origin.y += nextFrame.height - targetFrameSize.height
            nextFrame.size = targetFrameSize

            lastAppliedFrameSize = targetFrameSize
            window.setFrame(nextFrame, display: true, animate: false)
        }
    }
}

private struct RefreshIntervalOption {
    let seconds: TimeInterval

    @MainActor
    func title(model: AppModel) -> String {
        let intValue = Int(seconds)
        if intValue % 60 == 0 {
            let minutes = intValue / 60
            return String(format: String(localized: "settings.interval.minutes_format", defaultValue: "%@ min", bundle: .module), "\(minutes)")
        }
        return String(format: String(localized: "settings.interval.seconds_format", defaultValue: "%@ sec", bundle: .module), "\(intValue)")
    }

    static let gitOptions = [30, 60, 120, 300, 600].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let aiOptions = [60, 120, 180, 300, 600].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let backgroundAIOptions = [300, 600, 900, 1800].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let performanceMonitorOptions = [1, 2, 3, 5, 10].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let petReminderOptions = [900, 1800, 2700, 3600, 5400, 7200, 10800].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
}

// MARK: - Terminal Theme Preview Card

private extension NSColor {
    var dmuxSettingsPerceivedBrightness: CGFloat {
        let resolved = usingColorSpace(.deviceRGB) ?? self
        return (resolved.redComponent * 0.299) + (resolved.greenComponent * 0.587) + (resolved.blueComponent * 0.114)
    }

    var dmuxPreviewTextColor: NSColor {
        if dmuxSettingsPerceivedBrightness >= 0.72 {
            return NSColor(calibratedRed: 40 / 255, green: 39 / 255, blue: 38 / 255, alpha: 1)
        }
        return NSColor(calibratedRed: 1, green: 252 / 255, blue: 240 / 255, alpha: 1)
    }
}

// MARK: - App Icon Preview Card

private struct AppIconPreviewCard: View {
    let title: String
    let style: AppIconStyle
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                Image(nsImage: AppIconRenderer.image(for: style, size: 96))
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
                    .frame(width: 48, height: 48)
                    .overlay(
                        RoundedRectangle(cornerRadius: 12, style: .continuous)
                            .stroke(isSelected ? Color.accentColor : Color.clear, lineWidth: 2)
                    )

                Text(title)
                    .font(.system(size: 11))
                    .foregroundStyle(isSelected ? .primary : .secondary)
            }
        }
        .buttonStyle(.plain)
    }
}

// MARK: - Shortcut Recorder

private struct ShortcutRecorderField: View {
    let value: AppKeyboardShortcut?
    let placeholder: String
    let onChange: (AppKeyboardShortcut?) -> Void
    @State private var isRecording = false

    var body: some View {
        HStack(spacing: 6) {
            ShortcutRecorderRepresentable(
                isRecording: $isRecording,
                onRecord: onChange
            )
            .frame(width: 0, height: 0)

            Button {
                isRecording = true
            } label: {
                HStack(spacing: 6) {
                    Text(isRecording ? "..." : (value?.title ?? placeholder))
                        .font(.system(size: 12, design: .rounded))
                        .foregroundStyle(value == nil && !isRecording ? .tertiary : .primary)

                    Image(systemName: "keyboard")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Color(nsColor: .controlBackgroundColor))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(isRecording ? Color.accentColor : Color(nsColor: .separatorColor), lineWidth: isRecording ? 1.5 : 0.5)
                )
            }
            .buttonStyle(.plain)

            if value != nil {
                Button {
                    onChange(nil)
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 12))
                        .foregroundStyle(.tertiary)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

private struct ShortcutRecorderRepresentable: NSViewRepresentable {
    @Binding var isRecording: Bool
    let onRecord: (AppKeyboardShortcut?) -> Void

    func makeNSView(context: Context) -> ShortcutRecorderNSView {
        let view = ShortcutRecorderNSView()
        view.onRecord = onRecord
        view.onCancel = { isRecording = false }
        return view
    }

    func updateNSView(_ nsView: ShortcutRecorderNSView, context: Context) {
        nsView.onRecord = { value in
            onRecord(value)
            isRecording = false
        }
        nsView.onCancel = {
            isRecording = false
        }
        if isRecording, nsView.window?.firstResponder !== nsView {
            DispatchQueue.main.async {
                nsView.window?.makeFirstResponder(nsView)
            }
        }
    }
}

private final class ShortcutRecorderNSView: NSView {
    var onRecord: ((AppKeyboardShortcut?) -> Void)?
    var onCancel: (() -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 53:
            onCancel?()
            return
        case 51, 117:
            onRecord?(nil)
            return
        default:
            break
        }

        let modifiers = AppShortcutModifiers.from(eventModifiers: event.modifierFlags)
        guard !modifiers.isEmpty else {
            NSSound.beep()
            return
        }

        let cleaned = (event.charactersIgnoringModifiers ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard let character = cleaned.first, character.isLetter || character.isNumber else {
            NSSound.beep()
            return
        }

        onRecord?(AppKeyboardShortcut(key: String(character), modifiers: modifiers))
    }
}
