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

            Picker(String(localized: "settings.sleep_prevention", defaultValue: "Prevent Mac Sleep", bundle: .module), selection: Binding(
                get: { model.appSettings.sleepPreventionMode },
                set: { model.updateSleepPreventionMode($0) }
            )) {
                ForEach(AppSleepPreventionMode.allCases) { mode in
                    Text(mode.title).tag(mode)
                }
            }

            Text(String(localized: "settings.sleep_prevention.help", defaultValue: "Allows the display to turn off, but prevents the Mac from idle sleeping while enabled.", bundle: .module))
                .font(.system(size: 12))
                .foregroundStyle(.secondary)

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

                Toggle(String(localized: "settings.pet.desktop_widget", defaultValue: "Desktop Pet", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.desktopWidgetEnabled },
                    set: { model.updatePetDesktopWidgetEnabled($0) }
                ))
                .disabled(!model.appSettings.pet.enabled)

                Toggle(String(localized: "settings.pet.static_mode", defaultValue: "Static Pet Sprite", bundle: .module), isOn: Binding(
                    get: { model.appSettings.pet.staticMode },
                    set: { model.updatePetStaticMode($0) }
                ))
            }

            Section(String(localized: "settings.pet.speech.section", defaultValue: "Pet Speech", bundle: .module)) {
                Picker(String(localized: "settings.pet.speech.mode", defaultValue: "Mode", bundle: .module), selection: Binding(
                    get: { model.appSettings.ai.pet.speechMode },
                    set: { model.updatePetSpeechMode($0) }
                )) {
                    ForEach(PetSpeechMode.allCases) { mode in
                        Text(mode.title).tag(mode)
                    }
                }

                Picker(String(localized: "settings.pet.speech.frequency", defaultValue: "Frequency", bundle: .module), selection: Binding(
                    get: { model.appSettings.ai.pet.speechFrequency },
                    set: { model.updatePetSpeechFrequency($0) }
                )) {
                    ForEach(PetSpeechFrequency.allCases) { frequency in
                        Text(petSpeechFrequencyOptionTitle(frequency)).tag(frequency)
                    }
                }
                .disabled(model.appSettings.ai.pet.speechMode == .off)
                Text(String(localized: "settings.pet.speech.frequency_help", defaultValue: "Frequency is estimated per hour, not a daily cap. The shortest global cooldown is 30 seconds.", bundle: .module))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)

                Toggle(String(localized: "settings.pet.speech.quiet_during_work", defaultValue: "Speak Less During Work Hours", bundle: .module), isOn: Binding(
                    get: { model.appSettings.ai.pet.speechQuietDuringWork },
                    set: { model.updatePetSpeechQuietDuringWork($0) }
                ))
                .disabled(model.appSettings.ai.pet.speechMode == .off)

                Toggle(String(localized: "settings.pet.speech.louder_at_night", defaultValue: "Speak More At Night", bundle: .module), isOn: Binding(
                    get: { model.appSettings.ai.pet.speechLouderAtNight },
                    set: { model.updatePetSpeechLouderAtNight($0) }
                ))
                .disabled(model.appSettings.ai.pet.speechMode == .off)

                Toggle(String(localized: "settings.pet.speech.mute_on_fullscreen", defaultValue: "Mute In Full Screen", bundle: .module), isOn: Binding(
                    get: { model.appSettings.ai.pet.speechMuteOnFullscreen },
                    set: { model.updatePetSpeechMuteOnFullscreen($0) }
                ))
                .disabled(model.appSettings.ai.pet.speechMode == .off)

                Toggle(String(localized: "settings.pet.speech.quiet_hours", defaultValue: "Quiet Hours 22:00-08:00", bundle: .module), isOn: Binding(
                    get: { model.appSettings.ai.pet.speechQuietHoursStart != nil && model.appSettings.ai.pet.speechQuietHoursEnd != nil },
                    set: { enabled in
                        model.updatePetSpeechQuietHours(start: enabled ? 22 : nil, end: enabled ? 8 : nil)
                    }
                ))
                .disabled(model.appSettings.ai.pet.speechMode == .off)

                HStack {
                    Button(String(localized: "settings.pet.speech.mute_30_minutes", defaultValue: "Mute 30 Minutes", bundle: .module)) {
                        model.updatePetSpeechTemporaryMuteUntil(Date().addingTimeInterval(1800))
                    }
                    Button(String(localized: "settings.pet.speech.unmute", defaultValue: "Cancel Temporary Mute", bundle: .module)) {
                        model.updatePetSpeechTemporaryMuteUntil(nil)
                    }
                }
                .disabled(model.appSettings.ai.pet.speechMode == .off)
            }

            Section(String(localized: "settings.pet.llm.section", defaultValue: "Pet LLM", bundle: .module)) {
                Toggle(String(localized: "settings.pet.llm.enabled", defaultValue: "Enable LLM Line Polishing", bundle: .module), isOn: Binding(
                    get: { model.appSettings.ai.pet.speechLLMEnabled },
                    set: { model.updatePetSpeechLLMEnabled($0) }
                ))
                .disabled(model.appSettings.ai.pet.speechMode == .off)

                Picker(String(localized: "settings.pet.llm.channel", defaultValue: "LLM Channel", bundle: .module), selection: Binding(
                    get: { model.appSettings.ai.pet.speechProviderID },
                    set: { model.updatePetSpeechProviderID($0) }
                )) {
                    Text(String(localized: "settings.pet.llm.channel.automatic", defaultValue: "Automatic", bundle: .module)).tag(AppAIPetSettings.automaticSpeechProviderID)
                    ForEach(model.appSettings.ai.providers.filter { $0.kind.supportsAPICompletion }) { provider in
                        Text(provider.displayName).tag(provider.id)
                    }
                }
                .disabled(model.appSettings.ai.pet.speechMode == .off || !model.appSettings.ai.pet.speechLLMEnabled)

                Text(String(localized: "settings.pet.llm.help", defaultValue: "Only rhythm and milestone messages try LLM polishing. Template lines are used if it fails, times out, or no API channel is available.", bundle: .module))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
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

private func petSpeechFrequencyOptionTitle(_ frequency: PetSpeechFrequency) -> String {
    String(
        format: String(localized: "settings.pet.speech.frequency_option_format", defaultValue: "%@ · %@/hour · cooldown %@", bundle: .module),
        frequency.title,
        frequency.config.estimatedHourlyCount,
        petSpeechCooldownTitle(frequency.config.globalCooldown)
    )
}

private func petSpeechCooldownTitle(_ seconds: TimeInterval) -> String {
    if seconds >= 60 {
        let minutes = Int(seconds / 60)
        return String(
            format: String(localized: "settings.pet.speech.cooldown.minutes_format", defaultValue: "%d min", bundle: .module),
            minutes
        )
    }
    return String(
        format: String(localized: "settings.pet.speech.cooldown.seconds_format", defaultValue: "%d sec", bundle: .module),
        Int(seconds)
    )
}

// MARK: - AI Settings

struct AISettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            ForEach([AppSupportedAITool.codex, .claudeCode, .gemini, .opencode], id: \.id) { tool in
                Section(
                    String(
                        format: String(localized: "settings.ai.tool.configuration_format", defaultValue: "%@ Configuration", bundle: .module),
                        tool.title
                    )
                ) {
                    Toggle(
                        String(localized: "settings.ai.permission.full_access_toggle", defaultValue: "Full Access", bundle: .module),
                        isOn: Binding(
                            get: { tool.permissionMode(from: model.appSettings.ai.runtimeTools) == .fullAccess },
                            set: { isEnabled in
                                model.updateToolPermissionMode(isEnabled ? .fullAccess : .default, for: tool)
                            }
                        )
                    )

                    TextField(
                        String(localized: "settings.ai.tool.default_model", defaultValue: "Default Model", bundle: .module),
                        text: Binding(
                            get: { model.appSettings.ai.runtimeTools.model(for: tool) },
                            set: { model.updateToolDefaultModel($0, for: tool) }
                        )
                    )
                }
            }

            Section(String(localized: "settings.ai.global_prompt", defaultValue: "Global Prompt", bundle: .module)) {
                TextEditor(text: Binding(
                    get: { model.appSettings.ai.globalPrompt },
                    set: { model.updateAIGlobalPrompt($0) }
                ))
                .font(.system(size: 12))
                .frame(minHeight: 90)

                Text(String(localized: "settings.ai.global_prompt_help", defaultValue: "Injected when supported tools start. It is merged with memory context and written to each tool's launch context.", bundle: .module))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
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
                        ForEach(model.appSettings.ai.providers.filter { $0.isEnabled && $0.useForMemoryExtraction && $0.kind.supportsAPICompletion }) { provider in
                            Text(provider.displayName).tag(provider.id)
                        }
                    }
                    Text(String(localized: "settings.ai.memory.extraction_provider.automatic_help", defaultValue: "Automatic uses selected API channels by priority.", bundle: .module))
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

            Section(String(localized: "settings.ai.section.providers", defaultValue: "API Channels", bundle: .module)) {
                HStack(spacing: 12) {
                    Text(String(localized: "settings.ai.provider.api_only_help", defaultValue: "API channels for memory extraction and pet LLM lines.", bundle: .module))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                        .lineLimit(2)

                    Spacer(minLength: 16)

                    Menu {
                        Button("OpenAI-Compatible API") {
                            model.addAIProviderChannel(kind: .openAICompatible)
                        }
                        Button("Claude API") {
                            model.addAIProviderChannel(kind: .anthropic)
                        }
                        Button("DeepSeek API") {
                            model.addAIProviderChannel(
                                kind: .openAICompatible,
                                displayName: "DeepSeek API",
                                model: "deepseek-chat",
                                baseURL: "https://api.deepseek.com"
                            )
                        }
                    } label: {
                        Label(String(localized: "settings.ai.provider.add", defaultValue: "Add API Channel", bundle: .module), systemImage: "plus")
                    }
                    .fixedSize()
                }
            }

            ForEach(model.appSettings.ai.providers.filter { $0.kind.supportsAPICompletion }) { provider in
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
                            String(localized: "settings.ai.provider.name", defaultValue: "Name", bundle: .module),
                            text: Binding(
                                get: { model.appSettings.ai.provider(withID: provider.id)?.displayName ?? "" },
                                set: { model.updateAIProviderDisplayName($0, providerID: provider.id) }
                            )
                        )

                        TextField(
                            String(localized: "settings.ai.provider.model", defaultValue: "Model", bundle: .module),
                            text: Binding(
                                get: { model.appSettings.ai.provider(withID: provider.id)?.model ?? "" },
                                set: { model.updateAIProviderModel($0, providerID: provider.id) }
                            )
                        )

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

                        if provider.kind == .openAICompatible {
                            Text(String(localized: "settings.ai.provider.openai_compatible_help", defaultValue: "Use this for OpenAI, DeepSeek, and other /v1/chat/completions-compatible APIs.", bundle: .module))
                                .font(.system(size: 12))
                                .foregroundStyle(.secondary)
                        } else if provider.kind == .anthropic {
                            Text(String(localized: "settings.ai.provider.anthropic_help", defaultValue: "Uses Anthropic Messages API at /v1/messages.", bundle: .module))
                                .font(.system(size: 12))
                                .foregroundStyle(.secondary)
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

                        let testState = model.aiProviderTestStates[provider.id]
                        HStack(spacing: 8) {
                            Button {
                                model.testAIProvider(providerID: provider.id)
                            } label: {
                                if testState?.isTesting == true {
                                    ProgressView()
                                        .controlSize(.small)
                                }
                                Text(String(localized: "settings.ai.provider.test", defaultValue: "Test", bundle: .module))
                            }
                            .disabled(testState?.isTesting == true)

                            if provider.id.hasPrefix("api-") {
                                Button(role: .destructive) {
                                    model.removeAIProviderChannel(providerID: provider.id)
                                } label: {
                                    Label(String(localized: "settings.ai.provider.remove", defaultValue: "Remove", bundle: .module), systemImage: "trash")
                                }
                            }

                            if let message = testState?.message {
                                Text(message)
                                    .font(.system(size: 12))
                                    .foregroundStyle(testState?.status == .failed ? .red : .secondary)
                                    .lineLimit(2)
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
