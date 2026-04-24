import AppKit
import Foundation

extension AppModel {
    func updateLanguage(_ language: AppLanguage) {
        var settings = appSettings
        settings.language = language
        appSettings = settings
        persist()
        AppLanguageBootstrap.apply(language: language)
        aiStatsStore.refreshLocalizedStatusTexts()
        presentLanguageRestartPrompt()
    }

    func updateAppIconStyle(_ style: AppIconStyle) {
        var settings = appSettings
        settings.iconStyle = style
        appSettings = settings
        applyAppIcon()
        persist()
    }

    func updateTerminalBackgroundPreset(_ preset: AppTerminalBackgroundPreset) {
        guard appSettings.terminalBackgroundPreset != preset else {
            return
        }
        var settings = appSettings
        settings.terminalBackgroundPreset = preset
        appSettings = settings
        persist()
        presentThemeRestartPrompt()
    }

    func updateBackgroundColorPreset(_ preset: AppBackgroundColorPreset) {
        guard appSettings.backgroundColorPreset != preset else {
            return
        }
        var settings = appSettings
        settings.backgroundColorPreset = preset
        appSettings = settings
        persist()
        presentThemeRestartPrompt()
    }

    func updateTerminalFontSize(_ size: Int) {
        var settings = appSettings
        settings.terminalFontSize = max(10, min(28, size))
        appSettings = settings
        persist()
    }

    func updateDefaultTerminal(_ terminal: AppTerminalProfile) {
        let previousShell = appSettings.defaultTerminal.shellPath
        var settings = appSettings
        settings.defaultTerminal = terminal
        appSettings = settings
        let nextShell = terminal.shellPath
        for index in projects.indices where projects[index].shell == previousShell || AppTerminalProfile.allShellPaths.contains(projects[index].shell) {
            projects[index].shell = nextShell
        }
        for workspaceIndex in workspaces.indices {
            for sessionIndex in workspaces[workspaceIndex].sessions.indices where workspaces[workspaceIndex].sessions[sessionIndex].shell == previousShell || AppTerminalProfile.allShellPaths.contains(workspaces[workspaceIndex].sessions[sessionIndex].shell) {
                workspaces[workspaceIndex].sessions[sessionIndex].shell = nextShell
            }
        }
        persist()
    }

    func updateToolPermissionMode(_ mode: AppAIToolPermissionMode, for tool: AppSupportedAITool) {
        var settings = appSettings
        switch tool {
        case .codex:
            settings.ai.runtimeTools.codex = mode
        case .claudeCode:
            settings.ai.runtimeTools.claudeCode = mode
        case .gemini:
            settings.ai.runtimeTools.gemini = mode
        case .opencode:
            settings.ai.runtimeTools.opencode = mode
        }
        appSettings = settings
        toolPermissionSettingsService.sync(settings.ai.runtimeTools)
        persist()
    }

    func updateMemoryEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.ai.memory.enabled = enabled
        appSettings = settings
        persist()
    }

    func updateMemoryAutomaticInjectionEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.ai.memory.automaticInjectionEnabled = enabled
        appSettings = settings
        persist()
    }

    func updateMemoryAutomaticExtractionEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.ai.memory.automaticExtractionEnabled = enabled
        appSettings = settings
        persist()
    }

    func updateMemoryAllowCrossProjectUserRecall(_ enabled: Bool) {
        var settings = appSettings
        settings.ai.memory.allowCrossProjectUserRecall = enabled
        appSettings = settings
        persist()
    }

    func updateMemoryDefaultExtractorProviderID(_ providerID: String) {
        var settings = appSettings
        settings.ai.memory.defaultExtractorProviderID = providerID
        appSettings = settings
        persist()
    }

    func updateMemoryUserWorkingLimit(_ limit: Int) {
        var settings = appSettings
        settings.ai.memory.maxInjectedUserWorkingMemories = max(0, min(24, limit))
        appSettings = settings
        persist()
    }

    func updateMemoryProjectWorkingLimit(_ limit: Int) {
        var settings = appSettings
        settings.ai.memory.maxInjectedProjectWorkingMemories = max(0, min(32, limit))
        appSettings = settings
        persist()
    }

    func updateAIProviderEnabled(_ enabled: Bool, providerID: String) {
        updateAIProvider(providerID: providerID) { provider in
            provider.isEnabled = enabled
            if provider.kind == .openAICompatible && enabled == false {
                provider.useForMemoryExtraction = false
            }
        }
    }

    func updateAIProviderUseForMemoryExtraction(_ enabled: Bool, providerID: String) {
        updateAIProvider(providerID: providerID) { provider in
            provider.useForMemoryExtraction = enabled
        }
    }

    func updateAIProviderModel(_ model: String, providerID: String) {
        updateAIProvider(providerID: providerID) { provider in
            provider.model = model.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    func updateAIProviderBaseURL(_ baseURL: String, providerID: String) {
        updateAIProvider(providerID: providerID) { provider in
            provider.baseURL = baseURL.trimmingCharacters(in: .whitespacesAndNewlines)
        }
    }

    func updateAIProviderPriority(_ priority: Int, providerID: String) {
        updateAIProvider(providerID: providerID) { provider in
            provider.priority = max(0, priority)
        }
    }

    func updateAIProviderAPIKey(_ value: String, providerID: String) {
        let reference = aiCredentialStore.defaultReference(for: providerID)
        aiCredentialStore.saveAPIKey(value, for: reference)
        updateAIProvider(providerID: providerID) { provider in
            provider.apiKeyReference = value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? nil : reference
        }
    }

    func storedAIProviderAPIKey(providerID: String) -> String {
        guard let provider = appSettings.ai.provider(withID: providerID) else {
            return ""
        }
        return aiCredentialStore.apiKey(for: provider.apiKeyReference) ?? ""
    }

    private func updateAIProvider(providerID: String, transform: (inout AppAIProviderConfiguration) -> Void) {
        var settings = appSettings
        guard let index = settings.ai.providers.firstIndex(where: { $0.id == providerID }) else {
            return
        }
        transform(&settings.ai.providers[index])
        settings.ai.migrateMissingDefaultProviders()
        if settings.ai.providers.contains(where: { $0.id == settings.ai.memory.defaultExtractorProviderID && $0.isEnabled && $0.useForMemoryExtraction }) == false {
            settings.ai.memory.defaultExtractorProviderID = settings.ai.preferredExtractionProviderID() ?? AppAIProviderKind.claude.builtInProviderID
        }
        appSettings = settings
        persist()
    }

    func updateNotificationChannelEnabled(_ enabled: Bool, for channel: AppNotificationChannel) {
        updateNotificationChannel(channel) { configuration in
            configuration.isEnabled = enabled
        }
    }

    func updateNotificationChannelEndpoint(_ endpoint: String, for channel: AppNotificationChannel) {
        updateNotificationChannel(channel) { configuration in
            configuration.endpoint = endpoint
        }
    }

    func updateNotificationChannelToken(_ token: String, for channel: AppNotificationChannel) {
        updateNotificationChannel(channel) { configuration in
            configuration.token = token
        }
    }

    func updateDockBadgeEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.showsDockBadge = enabled
        appSettings = settings
        updateDockBadge()
        persist()
    }

    func updatePetEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.pet.enabled = enabled
        appSettings = settings
        persist()
    }

    func updatePetStaticMode(_ enabled: Bool) {
        var settings = appSettings
        settings.pet.staticMode = enabled
        appSettings = settings
        persist()
    }

    func updatePetHydrationReminderEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.pet.hydrationReminderEnabled = enabled
        appSettings = settings
        persist()
    }

    func updatePetHydrationReminderInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.pet.hydrationReminderInterval = interval
        appSettings = settings
        persist()
    }

    func updatePetSedentaryReminderEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.pet.sedentaryReminderEnabled = enabled
        appSettings = settings
        persist()
    }

    func updatePetSedentaryReminderInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.pet.sedentaryReminderInterval = interval
        appSettings = settings
        persist()
    }

    func updatePetLateNightReminderEnabled(_ enabled: Bool) {
        var settings = appSettings
        settings.pet.lateNightReminderEnabled = enabled
        appSettings = settings
        persist()
    }

    func updatePetLateNightReminderInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.pet.lateNightReminderInterval = interval
        appSettings = settings
        persist()
    }

    func updateShortcut(_ shortcut: AppKeyboardShortcut?, for target: AppShortcutTarget) {
        switch target {
        case .splitPane:
            appSettings.shortcuts.splitPane = shortcut
        case .createTab:
            appSettings.shortcuts.createTab = shortcut
        case .toggleGitPanel:
            appSettings.shortcuts.toggleGitPanel = shortcut
        case .toggleAIPanel:
            appSettings.shortcuts.toggleAIPanel = shortcut
        }
        persist()
    }

    func updateGitAutoRefreshInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.gitAutoRefreshInterval = interval
        appSettings = settings
        gitStore.configureRemoteSyncInterval(interval)
        persist()
    }

    func updateAIAutomaticRefreshInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.aiAutoRefreshInterval = interval
        appSettings = settings
        aiStatsStore.configureIntervals(
            automatic: interval,
            background: appSettings.aiBackgroundRefreshInterval
        )
        persist()
    }

    func updateAIBackgroundRefreshInterval(_ interval: TimeInterval) {
        var settings = appSettings
        settings.aiBackgroundRefreshInterval = interval
        appSettings = settings
        aiStatsStore.configureIntervals(
            automatic: appSettings.aiAutoRefreshInterval,
            background: interval
        )
        persist()
    }

    func updateAIStatisticsDisplayMode(_ mode: AppAIStatisticsDisplayMode) {
        guard appSettings.aiStatisticsDisplayMode != mode else {
            return
        }
        var settings = appSettings
        settings.aiStatisticsDisplayMode = mode
        appSettings = settings
        persist()
    }

    func updateDeveloperPerformanceMonitorEnabled(_ enabled: Bool) {
        applyPerformanceMonitorSettings { developer in
            developer.showsPerformanceMonitor = enabled
        }
    }

    func updateDeveloperPerformanceMonitorSamplingInterval(_ interval: TimeInterval) {
        let normalizedInterval = max(1, interval)
        applyPerformanceMonitorSettings { developer in
            developer.performanceMonitorSamplingInterval = normalizedInterval
        }
    }

    func openURL(_ url: URL) {
        NSWorkspace.shared.open(url)
    }

    func checkForUpdates() {
        if appUpdaterService.isAvailable {
            do {
                try appUpdaterService.checkForUpdates()
            } catch {
                presentSparkleConfigurationError(error)
            }
            return
        }

        runLegacyUpdateCheck(interactive: true)
    }

    var appDisplayName: String {
        if let name = Bundle.main.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String, !name.isEmpty {
            return name
        }
        if let name = Bundle.main.object(forInfoDictionaryKey: kCFBundleNameKey as String) as? String, !name.isEmpty {
            return name
        }
        return "Codux"
    }

    var appVersionDescription: String {
        let version = currentAppVersion
        let build = (Bundle.main.object(forInfoDictionaryKey: kCFBundleVersionKey as String) as? String) ?? "dev"
        return "v\(version) (\(build))"
    }

    var localizedUserAgreementDocument: String {
        [
            String(localized: "about.user_agreement_body", defaultValue: "This app is currently a development preview. By using it, you understand that terminal, Git, and AI activity features read local project metadata and runtime state, but do not proactively upload your project contents.", bundle: .module),
            String(localized: "about.user_agreement_data", defaultValue: "Codux only reads the local state needed to display terminal sessions, Git repository status, AI tool activity, and local statistics. You are responsible for reviewing any third-party CLI behavior and network activity triggered by those tools.", bundle: .module),
            String(localized: "about.user_agreement_responsibility", defaultValue: "You are responsible for your local environment, file permissions, repository credentials, notification permissions, and any commands executed inside the terminal.", bundle: .module),
            String(localized: "about.user_agreement_license", defaultValue: "Codux is distributed as open-source software under the GPL-3.0 license. Continued use means you accept that this experimental software may change behavior, interface, and compatibility over time.", bundle: .module)
        ].joined(separator: "\n\n")
    }

    var appIconImage: NSImage {
        AppIconRenderer.image(for: appSettings.iconStyle, size: 160)
    }

    func scheduleLaunchUpdateCheckIfNeeded() {
        guard hasScheduledLaunchUpdateCheck == false else {
            return
        }
        hasScheduledLaunchUpdateCheck = true

        Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: 2_000_000_000)
            guard let self else {
                return
            }
            if self.appUpdaterService.isAvailable {
                self.appUpdaterService.performLaunchBackgroundCheckIfNeeded()
            } else {
                self.runLegacyUpdateCheck(interactive: false)
            }
        }
    }

    func applyThemeMode() {
        guard isSystemUIReady else {
            return
        }
        let appearance = effectiveThemeMode.appearance
        NSApp.appearance = appearance
        for window in NSApp.windows {
            guard !(window is NSPanel) else { continue }
            window.appearance = appearance
        }
    }

    func applyAppIcon() {
        guard isSystemUIReady else {
            return
        }
        persistApplicationBundleIcon()
        updateDockBadge()
        applyRuntimeDockIcon()
    }

    func updateDockBadge() {
        guard isSystemUIReady else {
            return
        }
        NSApp.dockTile.contentView = nil
        guard appSettings.showsDockBadge else {
            NSApp.dockTile.badgeLabel = nil
            NSApp.dockTile.display()
            return
        }

        let completedCount = activityByProjectID.values.reduce(into: 0) { partial, phase in
            if case .completed = phase {
                partial += 1
            }
        }
        NSApp.dockTile.badgeLabel = completedCount > 0 ? "\(completedCount)" : nil
        NSApp.dockTile.display()
    }

    private func applyPerformanceMonitorSettings(_ update: (inout AppDeveloperSettings) -> Void) {
        var settings = appSettings
        update(&settings.developer)
        appSettings = settings
        performanceMonitor.configure(
            isEnabled: settings.developer.showsPerformanceMonitor,
            sampleInterval: settings.developer.performanceMonitorSamplingInterval
        )
        persist()
    }

    private func updateNotificationChannel(
        _ channel: AppNotificationChannel,
        update: (inout AppNotificationChannelConfiguration) -> Void
    ) {
        var settings = appSettings
        switch channel {
        case .bark:
            update(&settings.notifications.bark)
        case .ntfy:
            update(&settings.notifications.ntfy)
        case .wxpusher:
            update(&settings.notifications.wxpusher)
        case .feishu:
            update(&settings.notifications.feishu)
        case .dingTalk:
            update(&settings.notifications.dingTalk)
        case .weCom:
            update(&settings.notifications.weCom)
        case .telegram:
            update(&settings.notifications.telegram)
        case .discord:
            update(&settings.notifications.discord)
        case .slack:
            update(&settings.notifications.slack)
        case .webhook:
            update(&settings.notifications.webhook)
        }
        appSettings = settings
        persist()
    }

    private func runLegacyUpdateCheck(interactive: Bool) {
        guard !isCheckingForUpdates else {
            return
        }
        isCheckingForUpdates = true
        if interactive {
            statusMessage = String(localized: "update.checking", defaultValue: "Checking for updates...", bundle: .module)
        }

        Task { @MainActor in
            defer { isCheckingForUpdates = false }

            do {
                let result = try await AppReleaseService.checkForUpdates(currentVersion: currentAppVersion)
                presentLegacyUpdateCheckResult(result, interactive: interactive)
            } catch {
                presentLegacyUpdateCheckError(error, interactive: interactive)
            }
        }
    }

    private func persistApplicationBundleIcon() {
        let bundlePath = Bundle.main.bundleURL.path
        let iconVariant = AppIconRenderer.Variant.current()
        let iconImage: NSImage? = {
            if appSettings.iconStyle == .default {
                return iconVariant == .standard
                    ? nil
                    : AppIconRenderer.image(for: appSettings.iconStyle, size: 1024, variant: iconVariant)
            }
            return AppIconRenderer.image(for: appSettings.iconStyle, size: 1024, variant: iconVariant)
        }()

        let didUpdate = NSWorkspace.shared.setIcon(iconImage, forFile: bundlePath, options: [])
        debugLog.log(
            "app",
            "bundle icon update style=\(appSettings.iconStyle.rawValue) variant=\(String(describing: iconVariant)) success=\(didUpdate) path=\(bundlePath)"
        )

        guard didUpdate else {
            return
        }

        NSWorkspace.shared.noteFileSystemChanged(bundlePath)
        let parentPath = Bundle.main.bundleURL.deletingLastPathComponent().path
        NSWorkspace.shared.noteFileSystemChanged(parentPath)
    }

    private func applyRuntimeDockIcon() {
        NSApplication.shared.applicationIconImage = AppIconRenderer.image(
            for: appSettings.iconStyle,
            variant: AppIconRenderer.Variant.current()
        )
    }

    private var currentAppVersion: String {
        ((Bundle.main.object(forInfoDictionaryKey: "CFBundleShortVersionString") as? String) ?? "0.1.0")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func presentLegacyUpdateCheckResult(_ result: AppReleaseCheckResult, interactive: Bool) {
        guard let parentWindow = presentationWindow() else {
            if interactive {
                statusMessage = String(localized: "app.window.main_missing", defaultValue: "Unable to find the main window.", bundle: .module)
            }
            return
        }

        switch result {
        case .upToDate(let currentVersion, let latestVersion):
            guard interactive else {
                return
            }
            statusMessage = String(localized: "update.latest.title", defaultValue: "You're up to date.", bundle: .module)
            let dialog = ConfirmDialogState(
                title: String(localized: "update.latest.title", defaultValue: "You're up to date.", bundle: .module),
                message: String(
                    format: String(localized: "update.latest.message_format", defaultValue: "Current version: v%@\nLatest release: v%@", bundle: .module),
                    currentVersion,
                    latestVersion
                ),
                icon: "checkmark.circle.fill",
                iconColor: AppTheme.success,
                primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
            )
            ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }

        case .updateAvailable(let currentVersion, let latest):
            statusMessage = String(
                format: String(localized: "update.available.message_format", defaultValue: "A new version v%@ is available. You are currently using v%@.", bundle: .module),
                latest.version,
                currentVersion
            )
            let notes = AppReleaseService.releaseNotesExcerpt(from: latest.body)
            var message = String(
                format: String(localized: "update.available.message_format", defaultValue: "A new version v%@ is available. You are currently using v%@.", bundle: .module),
                latest.version,
                currentVersion
            )
            if let notes, !notes.isEmpty {
                message += "\n\n" + notes
            }

            let dialog = ConfirmDialogState(
                title: String(localized: "update.available.title", defaultValue: "Update Available", bundle: .module),
                message: message,
                icon: "arrow.down.circle.fill",
                iconColor: AppTheme.focus,
                primaryTitle: String(localized: "update.available.open", defaultValue: "Download", bundle: .module),
                secondaryTitle: String(localized: "update.available.later", defaultValue: "Later", bundle: .module)
            )
            ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
                guard let self, result == .primary else {
                    return
                }
                self.openURL(AppReleaseService.preferredDownloadURL(for: latest))
            }
        }
    }

    private func presentLegacyUpdateCheckError(_ error: Error, interactive: Bool) {
        guard interactive else {
            return
        }

        guard let parentWindow = presentationWindow() else {
            statusMessage = error.localizedDescription
            return
        }

        statusMessage = error.localizedDescription
        let dialog = ConfirmDialogState(
            title: String(localized: "update.error.title", defaultValue: "Unable to Check for Updates", bundle: .module),
            message: String(localized: "update.error.message", defaultValue: "Please check your network connection and try again.", bundle: .module),
            icon: "wifi.exclamationmark",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
        )
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }
    }

    private func presentSparkleConfigurationError(_ error: Error) {
        guard let parentWindow = presentationWindow() else {
            statusMessage = error.localizedDescription
            return
        }

        statusMessage = error.localizedDescription
        let dialog = ConfirmDialogState(
            title: String(localized: "update.error.title", defaultValue: "Unable to Check for Updates", bundle: .module),
            message: error.localizedDescription,
            icon: "wifi.exclamationmark",
            iconColor: AppTheme.warning,
            primaryTitle: String(localized: "common.ok", defaultValue: "OK", bundle: .module)
        )
        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { _ in }
    }

    private func presentLanguageRestartPrompt() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "settings.language.restart_required", defaultValue: "Restart the app to apply the selected language.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "settings.language.restart_title", defaultValue: "Restart Required", bundle: .module),
            message: String(localized: "settings.language.restart_message", defaultValue: "Restart Codux to apply the selected language.", bundle: .module),
            icon: "globe",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.restart_now", defaultValue: "Restart Now", bundle: .module),
            secondaryTitle: String(localized: "common.later", defaultValue: "Later", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else {
                return
            }
            if result == .primary {
                self.relaunchApplication()
            } else {
                self.statusMessage = String(localized: "settings.language.restart_pending", defaultValue: "Language changes will apply after restart.", bundle: .module)
            }
        }
    }

    private func presentThemeRestartPrompt() {
        guard let parentWindow = presentationWindow() else {
            statusMessage = String(localized: "settings.theme.restart_required", defaultValue: "Restart the app to apply the selected theme.", bundle: .module)
            return
        }

        let dialog = ConfirmDialogState(
            title: String(localized: "settings.theme.restart_title", defaultValue: "Restart Required", bundle: .module),
            message: String(localized: "settings.theme.restart_message", defaultValue: "Restart Codux to apply the selected theme to the app and all terminals.", bundle: .module),
            icon: "paintpalette",
            iconColor: AppTheme.focus,
            primaryTitle: String(localized: "common.restart_now", defaultValue: "Restart Now", bundle: .module),
            secondaryTitle: String(localized: "common.later", defaultValue: "Later", bundle: .module)
        )

        ConfirmDialogPresenter.present(dialog: dialog, parentWindow: parentWindow) { [weak self] result in
            guard let self else {
                return
            }
            if result == .primary {
                self.relaunchApplication()
            } else {
                self.statusMessage = String(localized: "settings.theme.restart_pending", defaultValue: "Theme changes will apply after restart.", bundle: .module)
            }
        }
    }

    private func relaunchApplication() {
        AppDelegate.scheduleRelaunch(at: Bundle.main.bundleURL)
    }
}
