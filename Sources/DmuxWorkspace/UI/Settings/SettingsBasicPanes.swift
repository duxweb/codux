import AppKit
import SwiftUI

struct GeneralSettingsPane: View {
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

            Picker(String(localized: "settings.ai_statistics_mode", defaultValue: "AI Statistics Mode", bundle: .module), selection: Binding(
                get: { model.appSettings.aiStatisticsDisplayMode },
                set: { model.updateAIStatisticsDisplayMode($0) }
            )) {
                ForEach(AppAIStatisticsDisplayMode.allCases) { mode in
                    Text(mode.title).tag(mode)
                }
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }
}

struct PetSettingsPane: View {
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

struct ToolSettingsPane: View {
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

struct DeveloperSettingsPane: View {
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
