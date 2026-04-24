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

// MARK: - AI Settings

struct AISettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section(String(localized: "settings.tools.permissions", defaultValue: "Tool Permissions", bundle: .module)) {
                ForEach([AppSupportedAITool.codex, .claudeCode, .gemini, .opencode], id: \.id) { tool in
                    HStack {
                        Text(tool.title)
                        Spacer()
                        Toggle(
                            String(localized: "settings.ai.permission.full_access_toggle", defaultValue: "Full Access", bundle: .module),
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

            Section(String(localized: "settings.ai.section.memory", defaultValue: "Memory", bundle: .module)) {
                Toggle(
                    String(localized: "settings.ai.memory.enabled", defaultValue: "Enable Memory", bundle: .module),
                    isOn: Binding(
                        get: { model.appSettings.ai.memory.enabled },
                        set: { model.updateMemoryEnabled($0) }
                    )
                )
            }

            if model.appSettings.ai.memory.enabled {
                Section(String(localized: "settings.ai.memory.automatic_injection", defaultValue: "Automatic Injection", bundle: .module)) {
                    Toggle(
                        String(localized: "settings.ai.memory.automatic_injection", defaultValue: "Automatic Injection", bundle: .module),
                        isOn: Binding(
                            get: { model.appSettings.ai.memory.automaticInjectionEnabled },
                            set: { model.updateMemoryAutomaticInjectionEnabled($0) }
                        )
                    )

                    Toggle(
                        String(localized: "settings.ai.memory.automatic_extraction", defaultValue: "Automatic Extraction", bundle: .module),
                        isOn: Binding(
                            get: { model.appSettings.ai.memory.automaticExtractionEnabled },
                            set: { model.updateMemoryAutomaticExtractionEnabled($0) }
                        )
                    )

                    Toggle(
                        String(localized: "settings.ai.memory.cross_project_user", defaultValue: "Cross-Project User Memory", bundle: .module),
                        isOn: Binding(
                            get: { model.appSettings.ai.memory.allowCrossProjectUserRecall },
                            set: { model.updateMemoryAllowCrossProjectUserRecall($0) }
                        )
                    )
                }

                Section(String(localized: "settings.ai.memory.default_extraction_provider", defaultValue: "Default Extraction Provider", bundle: .module)) {
                    Picker(
                        String(localized: "settings.ai.memory.default_extraction_provider", defaultValue: "Default Extraction Provider", bundle: .module),
                        selection: Binding(
                            get: { model.appSettings.ai.memory.defaultExtractorProviderID },
                            set: { model.updateMemoryDefaultExtractorProviderID($0) }
                        )
                    ) {
                        Text(String(localized: "settings.ai.memory.extraction_provider.automatic", defaultValue: "Automatic", bundle: .module))
                            .tag(AppMemorySettings.automaticExtractorProviderID)
                        ForEach(model.appSettings.ai.providers.filter { $0.isEnabled && $0.useForMemoryExtraction }) { provider in
                            Text(provider.displayName).tag(provider.id)
                        }
                    }
                    Text(String(localized: "settings.ai.memory.extraction_provider.automatic_help", defaultValue: "Automatic uses the current terminal tool first, then falls back to provider priority.", bundle: .module))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)

                    Picker(
                        String(localized: "settings.ai.memory.user_working_recall", defaultValue: "User Working Recall", bundle: .module),
                        selection: Binding(
                            get: { model.appSettings.ai.memory.maxInjectedUserWorkingMemories },
                            set: { model.updateMemoryUserWorkingLimit($0) }
                        )
                    ) {
                        ForEach(0...24, id: \.self) { value in
                            Text("\(value)").tag(value)
                        }
                    }

                    Picker(
                        String(localized: "settings.ai.memory.project_working_recall", defaultValue: "Project Working Recall", bundle: .module),
                        selection: Binding(
                            get: { model.appSettings.ai.memory.maxInjectedProjectWorkingMemories },
                            set: { model.updateMemoryProjectWorkingLimit($0) }
                        )
                    ) {
                        ForEach(0...32, id: \.self) { value in
                            Text("\(value)").tag(value)
                        }
                    }
                }
            }

            ForEach(model.appSettings.ai.providers) { provider in
                Section(provider.displayName) {
                    HStack {
                        VStack(alignment: .leading, spacing: 2) {
                            Text(provider.kind.title)
                                .font(.system(size: 11))
                                .foregroundStyle(.secondary)
                        }
                        Spacer()
                        Toggle(
                            String(localized: "settings.ai.provider.enabled", defaultValue: "Enabled", bundle: .module),
                            isOn: Binding(
                                get: { model.appSettings.ai.provider(withID: provider.id)?.isEnabled ?? false },
                                set: { model.updateAIProviderEnabled($0, providerID: provider.id) }
                            )
                        )
                        .labelsHidden()
                        .toggleStyle(.switch)
                    }

                    if model.appSettings.ai.provider(withID: provider.id)?.isEnabled ?? false {
                        TextField(
                            String(localized: "settings.ai.provider.model", defaultValue: "Model", bundle: .module),
                            text: Binding(
                                get: { model.appSettings.ai.provider(withID: provider.id)?.model ?? "" },
                                set: { model.updateAIProviderModel($0, providerID: provider.id) }
                            )
                        )

                        if provider.kind == .openAICompatible {
                            TextField(
                                String(localized: "settings.ai.provider.base_url", defaultValue: "Base URL", bundle: .module),
                                text: Binding(
                                    get: { model.appSettings.ai.provider(withID: provider.id)?.baseURL ?? "" },
                                    set: { model.updateAIProviderBaseURL($0, providerID: provider.id) }
                                )
                            )

                            SecureField(
                                String(localized: "settings.ai.provider.api_key", defaultValue: "API Key", bundle: .module),
                                text: Binding(
                                    get: { model.storedAIProviderAPIKey(providerID: provider.id) },
                                    set: { model.updateAIProviderAPIKey($0, providerID: provider.id) }
                                )
                            )
                        }

                        Toggle(
                            String(localized: "settings.ai.provider.use_for_memory_extraction", defaultValue: "Use For Memory Extraction", bundle: .module),
                            isOn: Binding(
                                get: { model.appSettings.ai.provider(withID: provider.id)?.useForMemoryExtraction ?? false },
                                set: { model.updateAIProviderUseForMemoryExtraction($0, providerID: provider.id) }
                            )
                        )

                        Picker(
                            String(localized: "settings.ai.provider.priority", defaultValue: "Priority", bundle: .module),
                            selection: Binding(
                                get: { model.appSettings.ai.provider(withID: provider.id)?.priority ?? 0 },
                                set: { model.updateAIProviderPriority($0, providerID: provider.id) }
                            )
                        ) {
                            ForEach(0...32, id: \.self) { value in
                                Text("\(value)").tag(value)
                            }
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
