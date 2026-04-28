import AppKit
import Foundation
import SwiftUI

enum AppAIStatisticsDisplayMode: String, Codable, CaseIterable, Identifiable {
    case normalized
    case includingCache

    var id: String { rawValue }

    var title: String {
        switch self {
        case .normalized:
            return String(localized: "settings.ai_statistics_mode.normalized", defaultValue: "Exclude Cache", bundle: .module)
        case .includingCache:
            return String(localized: "settings.ai_statistics_mode.including_cache", defaultValue: "Include Cache", bundle: .module)
        }
    }
}

struct AppSettings: Codable, Equatable {
    var language: AppLanguage = .system
    var terminalBackgroundPreset: AppTerminalBackgroundPreset = .automatic
    var backgroundColorPreset: AppBackgroundColorPreset = .automatic
    var terminalFontSize = 14
    var iconStyle: AppIconStyle = .default
    var defaultTerminal: AppTerminalProfile = .zsh
    var ai = AppAISettings()
    var notifications = AppNotificationSettings()
    var showsDockBadge = true
    var gitAutoRefreshInterval: TimeInterval = 60
    var aiAutoRefreshInterval: TimeInterval = 180
    var aiBackgroundRefreshInterval: TimeInterval = 600
    var aiStatisticsDisplayMode: AppAIStatisticsDisplayMode = .normalized
    var developer = AppDeveloperSettings()
    var shortcuts = AppShortcutConfiguration.defaults
    var pet = AppPetSettings()
    var remote = AppRemoteSettings()

    init() {}

    enum CodingKeys: String, CodingKey {
        case language
        case terminalBackgroundPreset
        case backgroundColorPreset
        case terminalFontSize
        case iconStyle
        case defaultTerminal
        case ai
        case toolPermissions
        case notifications
        case showsDockBadge
        case gitAutoRefreshInterval
        case aiAutoRefreshInterval
        case aiBackgroundRefreshInterval
        case aiStatisticsDisplayMode
        case developer
        case shortcuts
        case pet
        case remote
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        language = try container.decodeIfPresent(AppLanguage.self, forKey: .language) ?? .system
        terminalBackgroundPreset = try container.decodeIfPresent(AppTerminalBackgroundPreset.self, forKey: .terminalBackgroundPreset) ?? .automatic
        backgroundColorPreset = try container.decodeIfPresent(AppBackgroundColorPreset.self, forKey: .backgroundColorPreset) ?? .automatic
        terminalFontSize = max(10, min(28, try container.decodeIfPresent(Int.self, forKey: .terminalFontSize) ?? 14))
        iconStyle = try container.decodeIfPresent(AppIconStyle.self, forKey: .iconStyle) ?? .default
        defaultTerminal = try container.decodeIfPresent(AppTerminalProfile.self, forKey: .defaultTerminal) ?? .zsh
        if let decodedAI = try container.decodeIfPresent(AppAISettings.self, forKey: .ai) {
            ai = decodedAI
        } else {
            var decodedAI = AppAISettings()
            decodedAI.runtimeTools = try container.decodeIfPresent(AppAIToolPermissionSettings.self, forKey: .toolPermissions) ?? .init()
            ai = decodedAI
        }
        ai.migrateMissingDefaultProviders()
        notifications = try container.decodeIfPresent(AppNotificationSettings.self, forKey: .notifications) ?? .init()
        showsDockBadge = try container.decodeIfPresent(Bool.self, forKey: .showsDockBadge) ?? true
        gitAutoRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .gitAutoRefreshInterval) ?? 60
        aiAutoRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .aiAutoRefreshInterval) ?? 180
        aiBackgroundRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .aiBackgroundRefreshInterval) ?? 600
        aiStatisticsDisplayMode = try container.decodeIfPresent(AppAIStatisticsDisplayMode.self, forKey: .aiStatisticsDisplayMode) ?? .normalized
        developer = try container.decodeIfPresent(AppDeveloperSettings.self, forKey: .developer) ?? .init()
        shortcuts = (try container.decodeIfPresent(AppShortcutConfiguration.self, forKey: .shortcuts) ?? .defaults)
            .migratedFromLegacyDefaultsIfNeeded()
        pet = try container.decodeIfPresent(AppPetSettings.self, forKey: .pet) ?? .init()
        remote = try container.decodeIfPresent(AppRemoteSettings.self, forKey: .remote) ?? .init()
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(language, forKey: .language)
        try container.encode(terminalBackgroundPreset, forKey: .terminalBackgroundPreset)
        try container.encode(backgroundColorPreset, forKey: .backgroundColorPreset)
        try container.encode(terminalFontSize, forKey: .terminalFontSize)
        try container.encode(iconStyle, forKey: .iconStyle)
        try container.encode(defaultTerminal, forKey: .defaultTerminal)
        try container.encode(ai, forKey: .ai)
        try container.encode(ai.runtimeTools, forKey: .toolPermissions)
        try container.encode(notifications, forKey: .notifications)
        try container.encode(showsDockBadge, forKey: .showsDockBadge)
        try container.encode(gitAutoRefreshInterval, forKey: .gitAutoRefreshInterval)
        try container.encode(aiAutoRefreshInterval, forKey: .aiAutoRefreshInterval)
        try container.encode(aiBackgroundRefreshInterval, forKey: .aiBackgroundRefreshInterval)
        try container.encode(aiStatisticsDisplayMode, forKey: .aiStatisticsDisplayMode)
        try container.encode(developer, forKey: .developer)
        try container.encode(shortcuts, forKey: .shortcuts)
        try container.encode(pet, forKey: .pet)
        try container.encode(remote, forKey: .remote)
    }

    var toolPermissions: AppAIToolPermissionSettings {
        get { ai.runtimeTools }
        set { ai.runtimeTools = newValue }
    }
}


struct AppRemoteSettings: Codable, Equatable {
    var isEnabled = false
    var serverURL = "http://127.0.0.1:8088"
    var hostID = ""
    var hostToken = ""
    var hostPrivateKey = ""
    var hostPublicKey = ""
    var cachedDevices: [RemoteHostDevice] = []

    init() {}

    enum CodingKeys: String, CodingKey {
        case isEnabled
        case serverURL
        case hostID
        case hostToken
        case hostPrivateKey
        case hostPublicKey
        case cachedDevices
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        isEnabled = try container.decodeIfPresent(Bool.self, forKey: .isEnabled) ?? false
        serverURL = try container.decodeIfPresent(String.self, forKey: .serverURL) ?? "http://127.0.0.1:8088"
        hostID = try container.decodeIfPresent(String.self, forKey: .hostID) ?? ""
        hostToken = try container.decodeIfPresent(String.self, forKey: .hostToken) ?? ""
        hostPrivateKey = try container.decodeIfPresent(String.self, forKey: .hostPrivateKey) ?? ""
        hostPublicKey = try container.decodeIfPresent(String.self, forKey: .hostPublicKey) ?? ""
        cachedDevices = Self.normalizedCachedDevices(
            try container.decodeIfPresent([RemoteHostDevice].self, forKey: .cachedDevices) ?? [],
            hostID: hostID
        )
    }

    var displayCachedDevices: [RemoteHostDevice] {
        Self.normalizedCachedDevices(cachedDevices, hostID: hostID)
    }

    mutating func cacheDevices(_ devices: [RemoteHostDevice]) {
        cachedDevices = Self.normalizedCachedDevices(devices, hostID: hostID)
    }

    mutating func removeCachedDevice(id deviceID: String) {
        cachedDevices.removeAll { $0.id == deviceID }
    }

    private static func normalizedCachedDevices(
        _ devices: [RemoteHostDevice],
        hostID: String
    ) -> [RemoteHostDevice] {
        devices
            .filter { device in
                device.revokedAt == nil && (hostID.isEmpty || device.hostId == hostID)
            }
            .map { device in
                var cached = device
                cached.online = false
                return cached
            }
            .sorted { lhs, rhs in
                if lhs.lastSeen != rhs.lastSeen {
                    return lhs.lastSeen > rhs.lastSeen
                }
                return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
            }
    }
}

struct AppPetSettings: Codable, Equatable {
    var enabled = true
    var staticMode = false
    var hydrationReminderEnabled = true
    var hydrationReminderInterval: TimeInterval = 7200
    var sedentaryReminderEnabled = true
    var sedentaryReminderInterval: TimeInterval = 1800
    var lateNightReminderEnabled = true
    var lateNightReminderInterval: TimeInterval = 3600

    init() {}

    enum CodingKeys: String, CodingKey {
        case enabled
        case staticMode
        case hydrationReminderEnabled
        case hydrationReminderInterval
        case sedentaryReminderEnabled
        case sedentaryReminderInterval
        case lateNightReminderEnabled
        case lateNightReminderInterval
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        enabled = try container.decodeIfPresent(Bool.self, forKey: .enabled) ?? true
        staticMode = try container.decodeIfPresent(Bool.self, forKey: .staticMode) ?? false
        hydrationReminderEnabled = try container.decodeIfPresent(Bool.self, forKey: .hydrationReminderEnabled) ?? true
        hydrationReminderInterval = max(300, try container.decodeIfPresent(TimeInterval.self, forKey: .hydrationReminderInterval) ?? 7200)
        sedentaryReminderEnabled = try container.decodeIfPresent(Bool.self, forKey: .sedentaryReminderEnabled) ?? true
        sedentaryReminderInterval = max(300, try container.decodeIfPresent(TimeInterval.self, forKey: .sedentaryReminderInterval) ?? 1800)
        lateNightReminderEnabled = try container.decodeIfPresent(Bool.self, forKey: .lateNightReminderEnabled) ?? true
        lateNightReminderInterval = max(300, try container.decodeIfPresent(TimeInterval.self, forKey: .lateNightReminderInterval) ?? 3600)
    }
}
