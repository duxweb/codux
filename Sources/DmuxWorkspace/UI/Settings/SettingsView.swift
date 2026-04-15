import AppKit
import SwiftUI

private enum SettingsSectionTab: String, CaseIterable, Identifiable {
    case general
    case appearance
    case shortcuts
    case developer

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .general: return "gearshape"
        case .appearance: return "paintbrush"
        case .shortcuts: return "keyboard"
        case .developer: return "wrench.and.screwdriver"
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
                    Label(model.i18n("settings.tab.general", fallback: "General"), systemImage: SettingsSectionTab.general.symbol)
                }
                .tag(SettingsSectionTab.general)

            AppearanceSettingsPane(model: model)
                .tabItem {
                    Label(model.i18n("settings.tab.appearance", fallback: "Appearance"), systemImage: SettingsSectionTab.appearance.symbol)
                }
                .tag(SettingsSectionTab.appearance)

            ShortcutSettingsPane(model: model)
                .tabItem {
                    Label(model.i18n("settings.tab.shortcuts", fallback: "Shortcuts"), systemImage: SettingsSectionTab.shortcuts.symbol)
                }
                .tag(SettingsSectionTab.shortcuts)

            DeveloperSettingsPane(model: model)
                .tabItem {
                    Label(model.i18n("settings.tab.developer", fallback: "Developer"), systemImage: SettingsSectionTab.developer.symbol)
                }
                .tag(SettingsSectionTab.developer)
        }
        .id(model.appSettings.themeMode)
        .frame(width: 640, height: 460)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

// MARK: - General

private struct GeneralSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Picker(model.i18n("settings.language", fallback: "Language"), selection: Binding(
                get: { model.appSettings.language },
                set: { model.updateLanguage($0) }
            )) {
                ForEach(AppLanguage.allCases) { language in
                    Text(language.title(for: model.appSettings.language.resolved)).tag(language)
                }
            }

            Picker(model.i18n("settings.default_shell", fallback: "Default Shell"), selection: Binding(
                get: { model.appSettings.defaultTerminal },
                set: { model.updateDefaultTerminal($0) }
            )) {
                ForEach(AppTerminalProfile.available) { terminal in
                    Text(terminal.title).tag(terminal)
                }
            }

            Toggle(model.i18n("settings.dock_badge", fallback: "Dock Badge"), isOn: Binding(
                get: { model.appSettings.showsDockBadge },
                set: { model.updateDockBadgeEnabled($0) }
            ))

            Picker(model.i18n("settings.git_auto_refresh", fallback: "Git Auto Refresh"), selection: Binding(
                get: { model.appSettings.gitAutoRefreshInterval },
                set: { model.updateGitAutoRefreshInterval($0) }
            )) {
                ForEach(RefreshIntervalOption.gitOptions, id: \.seconds) { option in
                    Text(option.title(model: model)).tag(option.seconds)
                }
            }

            Picker(model.i18n("settings.ai_auto_refresh", fallback: "AI Auto Refresh"), selection: Binding(
                get: { model.appSettings.aiAutoRefreshInterval },
                set: { model.updateAIAutomaticRefreshInterval($0) }
            )) {
                ForEach(RefreshIntervalOption.aiOptions, id: \.seconds) { option in
                    Text(option.title(model: model)).tag(option.seconds)
                }
            }

            Picker(model.i18n("settings.ai_background_refresh", fallback: "AI Background Refresh"), selection: Binding(
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

// MARK: - Appearance

private struct AppearanceSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section(model.i18n("settings.theme", fallback: "Theme")) {
                HStack(spacing: 16) {
                    ForEach(AppThemeMode.allCases) { mode in
                        ThemeModePreviewCard(
                            title: themeModeTitle(mode),
                            mode: mode,
                            isSelected: model.appSettings.themeMode == mode
                        ) {
                            model.updateThemeMode(mode)
                        }
                    }
                }
                .padding(.vertical, 4)
            }

            Section(model.i18n("settings.terminal_background", fallback: "Terminal Background")) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 10) {
                        ForEach(AppTerminalBackgroundPreset.allCases) { preset in
                            TerminalBackgroundPreviewCard(
                                title: preset.title(for: model.appSettings.language.resolved),
                                preset: preset,
                                isSelected: model.appSettings.terminalBackgroundPreset == preset
                            ) {
                                model.updateTerminalBackgroundPreset(preset)
                            }
                        }
                    }
                }
                .padding(.vertical, 4)
            }

            Section(model.i18n("settings.app_icon", fallback: "App Icon")) {
                HStack(spacing: 16) {
                    ForEach(AppIconStyle.allCases) { style in
                        AppIconPreviewCard(
                            title: style.title(for: model.appSettings.language.resolved),
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

    private func themeModeTitle(_ mode: AppThemeMode) -> String {
        switch mode {
        case .system: return model.i18n("settings.theme.auto", fallback: "Auto")
        case .light: return model.i18n("settings.theme.light", fallback: "Light")
        case .dark: return model.i18n("settings.theme.dark", fallback: "Dark")
        }
    }
}

// MARK: - Shortcuts

private struct ShortcutSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section {
                shortcutRow(model.i18n("settings.shortcut.create_split", fallback: "Create Split"), target: .splitPane, value: model.appSettings.shortcuts.splitPane)
                shortcutRow(model.i18n("settings.shortcut.create_tab", fallback: "Create Tab"), target: .createTab, value: model.appSettings.shortcuts.createTab)
                shortcutRow(model.i18n("settings.shortcut.open_git_panel", fallback: "Git Panel"), target: .toggleGitPanel, value: model.appSettings.shortcuts.toggleGitPanel)
                shortcutRow(model.i18n("settings.shortcut.open_ai_panel", fallback: "AI Panel"), target: .toggleAIPanel, value: model.appSettings.shortcuts.toggleAIPanel)
            }

            Section(model.i18n("settings.shortcut.project_switch", fallback: "Project Switch Shortcuts")) {
                Text(model.i18n("settings.shortcut.project_switch_hint", fallback: "Use ⌘1-⌘9 to switch projects in sidebar order."))
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
                placeholder: model.i18n("settings.shortcut.record", fallback: "Record Shortcut")
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
            Toggle(model.i18n("settings.developer.notification_test", fallback: "Notification Test Button"), isOn: Binding(
                get: { model.appSettings.developer.showsNotificationTestButton },
                set: { model.updateDeveloperNotificationTestButtonEnabled($0) }
            ))

            Toggle(model.i18n("settings.developer.debug_log", fallback: "Debug Log Button"), isOn: Binding(
                get: { model.appSettings.developer.showsDebugLogButton },
                set: { model.updateDeveloperDebugLogButtonEnabled($0) }
            ))
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

private struct RefreshIntervalOption {
    let seconds: TimeInterval

    @MainActor
    func title(model: AppModel) -> String {
        let intValue = Int(seconds)
        if intValue % 60 == 0 {
            let minutes = intValue / 60
            return String(format: model.i18n("settings.interval.minutes_format", fallback: "%@ min"), "\(minutes)")
        }
        return String(format: model.i18n("settings.interval.seconds_format", fallback: "%@ sec"), "\(intValue)")
    }

    static let gitOptions = [30, 60, 120, 300, 600].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let aiOptions = [60, 120, 180, 300, 600].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
    static let backgroundAIOptions = [300, 600, 900, 1800].map { RefreshIntervalOption(seconds: TimeInterval($0)) }
}

// MARK: - Theme Preview Card

private struct ThemeModePreviewCard: View {
    let title: String
    let mode: AppThemeMode
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 6) {
                ZStack {
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(themeGradient)
                        .frame(width: 64, height: 42)

                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(windowBack)
                        .frame(width: 36, height: 24)
                        .overlay(
                            RoundedRectangle(cornerRadius: 5, style: .continuous)
                                .stroke(strokeColor, lineWidth: 0.5)
                        )
                        .offset(x: -6, y: -4)

                    RoundedRectangle(cornerRadius: 5, style: .continuous)
                        .fill(panelBack)
                        .frame(width: 36, height: 24)
                        .overlay(
                            RoundedRectangle(cornerRadius: 5, style: .continuous)
                                .stroke(strokeColor.opacity(0.6), lineWidth: 0.5)
                        )
                        .offset(x: 6, y: 4)
                }
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .stroke(isSelected ? Color.accentColor : Color(nsColor: .separatorColor), lineWidth: isSelected ? 2 : 0.5)
                )

                Text(title)
                    .font(.system(size: 11))
                    .foregroundStyle(isSelected ? .primary : .secondary)
            }
        }
        .buttonStyle(.plain)
    }

    private var themeGradient: LinearGradient {
        switch mode {
        case .system:
            return LinearGradient(colors: [Color(hex: 0x5D7FB6), Color(hex: 0x1C2342)], startPoint: .topLeading, endPoint: .bottomTrailing)
        case .light:
            return LinearGradient(colors: [Color(hex: 0x8EB5E8), Color(hex: 0xEDF1F7)], startPoint: .topLeading, endPoint: .bottomTrailing)
        case .dark:
            return LinearGradient(colors: [Color(hex: 0x2C3174), Color(hex: 0x11141D)], startPoint: .topLeading, endPoint: .bottomTrailing)
        }
    }

    private var windowBack: Color {
        switch mode {
        case .system, .light: return Color.white.opacity(0.92)
        case .dark: return Color(hex: 0x191B22)
        }
    }

    private var panelBack: Color {
        switch mode {
        case .system: return Color(hex: 0x202332)
        case .light: return Color.white.opacity(0.98)
        case .dark: return Color(hex: 0x0F1117)
        }
    }

    private var strokeColor: Color {
        mode == .dark ? Color.white.opacity(0.08) : Color.black.opacity(0.08)
    }
}

// MARK: - App Icon Preview Card

private struct TerminalBackgroundPreviewCard: View {
    let title: String
    let preset: AppTerminalBackgroundPreset
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            VStack(spacing: 5) {
                ZStack(alignment: .topLeading) {
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .fill(Color(nsColor: preset.backgroundColor))
                        .frame(width: 64, height: 42)
                        .overlay(
                            RoundedRectangle(cornerRadius: 7, style: .continuous)
                                .stroke(isSelected ? Color.accentColor : Color(nsColor: preset.dividerColor), lineWidth: isSelected ? 2 : 0.5)
                        )

                    VStack(alignment: .leading, spacing: 4) {
                        Capsule()
                            .fill(Color(nsColor: preset.mutedForegroundColor).opacity(preset.isLight ? 0.28 : 0.22))
                            .frame(width: 16, height: 3)

                        RoundedRectangle(cornerRadius: 2, style: .continuous)
                            .fill(Color(nsColor: preset.foregroundColor).opacity(preset.isLight ? 0.82 : 0.92))
                            .frame(width: 32, height: 3.5)

                        RoundedRectangle(cornerRadius: 2, style: .continuous)
                            .fill(Color(nsColor: preset.mutedForegroundColor).opacity(preset.isLight ? 0.64 : 0.74))
                            .frame(width: 22, height: 3)
                    }
                    .padding(8)
                }

                Text(title)
                    .font(.system(size: 10))
                    .foregroundStyle(isSelected ? .primary : .secondary)
                    .lineLimit(1)
            }
        }
        .buttonStyle(.plain)
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
