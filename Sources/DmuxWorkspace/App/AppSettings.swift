import AppKit
import Foundation
import SwiftUI

struct AppSettings: Codable, Equatable {
    var language: AppLanguage = .system
    var themeMode: AppThemeMode = .system
    var terminalBackgroundPreset: AppTerminalBackgroundPreset = .obsidian
    var terminalFontSize = 14
    var terminalGPUAccelerationEnabled = true
    var terminalGPUMode: AppTerminalGPUMode = .balanced
    var iconStyle: AppIconStyle = .default
    var defaultTerminal: AppTerminalProfile = .zsh
    var toolPermissions = AppAIToolPermissionSettings()
    var notifications = AppNotificationSettings()
    var showsDockBadge = true
    var gitAutoRefreshInterval: TimeInterval = 60
    var aiAutoRefreshInterval: TimeInterval = 180
    var aiBackgroundRefreshInterval: TimeInterval = 600
    var developer = AppDeveloperSettings()
    var shortcuts = AppShortcutConfiguration.defaults

    init() {}

    enum CodingKeys: String, CodingKey {
        case language
        case themeMode
        case terminalBackgroundPreset
        case terminalFontSize
        case terminalGPUAccelerationEnabled
        case terminalGPUMode
        case iconStyle
        case defaultTerminal
        case toolPermissions
        case notifications
        case showsDockBadge
        case gitAutoRefreshInterval
        case aiAutoRefreshInterval
        case aiBackgroundRefreshInterval
        case developer
        case shortcuts
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        language = try container.decodeIfPresent(AppLanguage.self, forKey: .language) ?? .system
        themeMode = try container.decodeIfPresent(AppThemeMode.self, forKey: .themeMode) ?? .system
        terminalBackgroundPreset = try container.decodeIfPresent(AppTerminalBackgroundPreset.self, forKey: .terminalBackgroundPreset) ?? .obsidian
        terminalFontSize = max(10, min(28, try container.decodeIfPresent(Int.self, forKey: .terminalFontSize) ?? 14))
        terminalGPUAccelerationEnabled = try container.decodeIfPresent(Bool.self, forKey: .terminalGPUAccelerationEnabled) ?? true
        terminalGPUMode = try container.decodeIfPresent(AppTerminalGPUMode.self, forKey: .terminalGPUMode) ?? .balanced
        iconStyle = try container.decodeIfPresent(AppIconStyle.self, forKey: .iconStyle) ?? .default
        defaultTerminal = try container.decodeIfPresent(AppTerminalProfile.self, forKey: .defaultTerminal) ?? .zsh
        toolPermissions = try container.decodeIfPresent(AppAIToolPermissionSettings.self, forKey: .toolPermissions) ?? .init()
        notifications = try container.decodeIfPresent(AppNotificationSettings.self, forKey: .notifications) ?? .init()
        showsDockBadge = try container.decodeIfPresent(Bool.self, forKey: .showsDockBadge) ?? true
        gitAutoRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .gitAutoRefreshInterval) ?? 60
        aiAutoRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .aiAutoRefreshInterval) ?? 180
        aiBackgroundRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .aiBackgroundRefreshInterval) ?? 600
        developer = try container.decodeIfPresent(AppDeveloperSettings.self, forKey: .developer) ?? .init()
        shortcuts = (try container.decodeIfPresent(AppShortcutConfiguration.self, forKey: .shortcuts) ?? .defaults)
            .migratedFromLegacyDefaultsIfNeeded()
    }
}

enum AppAIToolPermissionMode: String, Codable, CaseIterable, Identifiable {
    case `default`
    case fullAccess

    var id: String { rawValue }

    var title: String {
        switch self {
        case .default:
            return String(localized: "settings.tools.permission.default", defaultValue: "Default", bundle: .module)
        case .fullAccess:
            return String(localized: "settings.tools.permission.full_access", defaultValue: "Full Access", bundle: .module)
        }
    }
}

struct AppAIToolPermissionSettings: Codable, Equatable {
    var codex: AppAIToolPermissionMode = .default
    var claudeCode: AppAIToolPermissionMode = .default
    var gemini: AppAIToolPermissionMode = .default
    var opencode: AppAIToolPermissionMode = .default
}

struct AppNotificationChannelConfiguration: Codable, Equatable, Sendable {
    var isEnabled = false
    var endpoint = ""
    var token = ""
}

struct AppNotificationSettings: Codable, Equatable, Sendable {
    var bark = AppNotificationChannelConfiguration()
    var ntfy = AppNotificationChannelConfiguration()
    var wxpusher = AppNotificationChannelConfiguration()
    var feishu = AppNotificationChannelConfiguration()
    var dingTalk = AppNotificationChannelConfiguration()
    var weCom = AppNotificationChannelConfiguration()
    var telegram = AppNotificationChannelConfiguration()
    var discord = AppNotificationChannelConfiguration()
    var slack = AppNotificationChannelConfiguration()
    var webhook = AppNotificationChannelConfiguration()
}

enum AppNotificationChannel: CaseIterable, Identifiable, Sendable {
    case bark
    case ntfy
    case wxpusher
    case feishu
    case dingTalk
    case weCom
    case telegram
    case discord
    case slack
    case webhook

    var id: String { rawValue }

    private var rawValue: String {
        switch self {
        case .bark:
            return "bark"
        case .ntfy:
            return "ntfy"
        case .wxpusher:
            return "wxpusher"
        case .feishu:
            return "feishu"
        case .dingTalk:
            return "dingTalk"
        case .weCom:
            return "weCom"
        case .telegram:
            return "telegram"
        case .discord:
            return "discord"
        case .slack:
            return "slack"
        case .webhook:
            return "webhook"
        }
    }

    var title: String {
        switch self {
        case .bark:
            return "Bark"
        case .ntfy:
            return "ntfy"
        case .wxpusher:
            return "WxPusher"
        case .feishu:
            return "Feishu"
        case .dingTalk:
            return "DingTalk"
        case .weCom:
            return "WeCom"
        case .telegram:
            return "Telegram"
        case .discord:
            return "Discord"
        case .slack:
            return "Slack"
        case .webhook:
            return "Webhook"
        }
    }

    var endpointPlaceholder: String {
        switch self {
        case .bark:
            return "https://api.day.app"
        case .ntfy:
            return "https://ntfy.sh/topic"
        case .wxpusher:
            return "SPT_xxx"
        case .feishu:
            return "https://open.feishu.cn/open-apis/bot/v2/hook/..."
        case .dingTalk:
            return "https://oapi.dingtalk.com/robot/send?access_token=..."
        case .weCom:
            return "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key=..."
        case .telegram:
            return "chat_id"
        case .discord:
            return "https://discord.com/api/webhooks/..."
        case .slack:
            return "https://hooks.slack.com/services/..."
        case .webhook:
            return "https://example.com/webhook"
        }
    }

    var showsTokenField: Bool {
        switch self {
        case .wxpusher:
            return false
        default:
            return true
        }
    }

    var tokenPlaceholder: String {
        switch self {
        case .bark:
            return "device_key"
        case .ntfy:
            return "bearer_token"
        case .wxpusher:
            return "app_token"
        case .feishu:
            return "hook_token"
        case .dingTalk:
            return "access_token"
        case .weCom:
            return "key"
        case .telegram:
            return "bot_token"
        case .discord:
            return "webhook_token"
        case .slack:
            return "webhook_token"
        case .webhook:
            return "bearer_token"
        }
    }

    var websiteURL: URL? {
        switch self {
        case .bark:
            return URL(string: "https://bark.day.app")
        case .ntfy:
            return URL(string: "https://docs.ntfy.sh")
        case .wxpusher:
            return URL(string: "https://wxpusher.zjiecode.com/docs/#/")
        case .feishu:
            return URL(string: "https://open.feishu.cn")
        case .dingTalk:
            return URL(string: "https://open.dingtalk.com")
        case .weCom:
            return URL(string: "https://developer.work.weixin.qq.com")
        case .telegram:
            return URL(string: "https://core.telegram.org/bots/api")
        case .discord:
            return URL(string: "https://docs.discord.com/developers/resources/webhook")
        case .slack:
            return URL(string: "https://docs.slack.dev/messaging/sending-messages-using-incoming-webhooks/")
        case .webhook:
            return nil
        }
    }

    func configuration(from settings: AppNotificationSettings) -> AppNotificationChannelConfiguration {
        switch self {
        case .bark:
            return settings.bark
        case .ntfy:
            return settings.ntfy
        case .wxpusher:
            return settings.wxpusher
        case .feishu:
            return settings.feishu
        case .dingTalk:
            return settings.dingTalk
        case .weCom:
            return settings.weCom
        case .telegram:
            return settings.telegram
        case .discord:
            return settings.discord
        case .slack:
            return settings.slack
        case .webhook:
            return settings.webhook
        }
    }
}

enum AppSupportedAITool: CaseIterable, Identifiable {
    case codex
    case claudeCode
    case gemini
    case opencode

    var id: String { rawValue }

    private var rawValue: String {
        switch self {
        case .codex:
            return "codex"
        case .claudeCode:
            return "claudeCode"
        case .gemini:
            return "gemini"
        case .opencode:
            return "opencode"
        }
    }

    var title: String {
        switch self {
        case .codex:
            return "Codex"
        case .claudeCode:
            return "Claude Code"
        case .gemini:
            return "Gemini"
        case .opencode:
            return "OpenCode"
        }
    }

    func permissionMode(from settings: AppAIToolPermissionSettings) -> AppAIToolPermissionMode {
        switch self {
        case .codex:
            return settings.codex
        case .claudeCode:
            return settings.claudeCode
        case .gemini:
            return settings.gemini
        case .opencode:
            return settings.opencode
        }
    }
}

enum AppTerminalGPUMode: String, Codable, CaseIterable, Identifiable {
    case highPerformance
    case balanced
    case memorySaver

    var id: String { rawValue }

    var title: String {
        switch self {
        case .highPerformance:
            return String(localized: "settings.terminal_gpu_mode.high_performance", defaultValue: "High Performance", bundle: .module)
        case .balanced:
            return String(localized: "settings.terminal_gpu_mode.balanced", defaultValue: "Balanced", bundle: .module)
        case .memorySaver:
            return String(localized: "settings.terminal_gpu_mode.memory_saver", defaultValue: "Memory Saver", bundle: .module)
        }
    }

    var summary: String {
        switch self {
        case .highPerformance:
            return String(localized: "settings.terminal_gpu_mode.high_performance.summary", defaultValue: "Keeps the most aggressive GPU row cache for the smoothest redraws, with higher memory usage.", bundle: .module)
        case .balanced:
            return String(localized: "settings.terminal_gpu_mode.balanced.summary", defaultValue: "Uses the lighter GPU buffering mode even for a single terminal to reduce memory while keeping smooth everyday rendering.", bundle: .module)
        case .memorySaver:
            return String(localized: "settings.terminal_gpu_mode.memory_saver.summary", defaultValue: "Uses the lighter GPU buffer and disables Metal on background panes when possible for the lowest memory footprint.", bundle: .module)
        }
    }
}

struct AppDeveloperSettings: Codable, Equatable {
    var showsNotificationTestButton = false
    var showsDebugLogButton = false
    var showsPerformanceMonitor = false
    var performanceMonitorSamplingInterval: TimeInterval = 3

    init() {}

    enum CodingKeys: String, CodingKey {
        case showsNotificationTestButton
        case showsDebugLogButton
        case showsPerformanceMonitor
        case performanceMonitorSamplingInterval
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        showsNotificationTestButton = try container.decodeIfPresent(Bool.self, forKey: .showsNotificationTestButton) ?? false
        showsDebugLogButton = try container.decodeIfPresent(Bool.self, forKey: .showsDebugLogButton) ?? false
        showsPerformanceMonitor = try container.decodeIfPresent(Bool.self, forKey: .showsPerformanceMonitor) ?? false
        performanceMonitorSamplingInterval = max(
            1,
            try container.decodeIfPresent(TimeInterval.self, forKey: .performanceMonitorSamplingInterval) ?? 3
        )
    }
}

enum AppLanguage: String, Codable, CaseIterable, Identifiable {
    case system
    case simplifiedChinese
    case traditionalChinese
    case english
    case japanese
    case korean
    case french
    case german
    case spanish
    case portugueseBrazil
    case russian

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: return String(localized: "settings.language.follow_system", defaultValue: "System", bundle: .module)
        case .simplifiedChinese: return "简体中文"
        case .traditionalChinese: return "繁體中文"
        case .english: return "English"
        case .japanese: return "日本語"
        case .korean: return "한국어"
        case .french: return "Français"
        case .german: return "Deutsch"
        case .spanish: return "Español"
        case .portugueseBrazil: return "Português (Brasil)"
        case .russian: return "Русский"
        }
    }

    var resolved: AppLanguage {
        switch self {
        case .system:
            let preferredLocalization = Locale.preferredLanguages.first?.lowercased()
                ?? Locale.autoupdatingCurrent.identifier.lowercased()
            if preferredLocalization.contains("zh-hant") || preferredLocalization.contains("zh-tw") || preferredLocalization.contains("zh-hk") || preferredLocalization.contains("zh-mo") {
                return .traditionalChinese
            }
            if preferredLocalization.contains("zh-hans") || preferredLocalization == "zh" || preferredLocalization.contains("zh-cn") || preferredLocalization.contains("zh-sg") {
                return .simplifiedChinese
            }
            if preferredLocalization.hasPrefix("ja") {
                return .japanese
            }
            if preferredLocalization.hasPrefix("ko") {
                return .korean
            }
            if preferredLocalization.hasPrefix("fr") {
                return .french
            }
            if preferredLocalization.hasPrefix("de") {
                return .german
            }
            if preferredLocalization.hasPrefix("es") {
                return .spanish
            }
            if preferredLocalization.hasPrefix("pt-br") {
                return .portugueseBrazil
            }
            if preferredLocalization.hasPrefix("ru") {
                return .russian
            }
            return .english
        case .simplifiedChinese, .traditionalChinese, .english, .japanese, .korean, .french, .german, .spanish, .portugueseBrazil, .russian:
            return self
        }
    }

    var localeIdentifier: String {
        switch resolved {
        case .system:
            return Locale.autoupdatingCurrent.identifier
        case .simplifiedChinese:
            return "zh_CN"
        case .traditionalChinese:
            return "zh_TW"
        case .english:
            return "en_US"
        case .japanese:
            return "ja_JP"
        case .korean:
            return "ko_KR"
        case .french:
            return "fr_FR"
        case .german:
            return "de_DE"
        case .spanish:
            return "es_ES"
        case .portugueseBrazil:
            return "pt_BR"
        case .russian:
            return "ru_RU"
        }
    }

    var appleLanguageIdentifiers: [String]? {
        switch self {
        case .system:
            return nil
        case .simplifiedChinese:
            return ["zh-Hans", "zh-CN", "zh"]
        case .traditionalChinese:
            return ["zh-Hant", "zh-TW", "zh-HK", "zh"]
        case .english:
            return ["en"]
        case .japanese:
            return ["ja"]
        case .korean:
            return ["ko"]
        case .french:
            return ["fr"]
        case .german:
            return ["de"]
        case .spanish:
            return ["es"]
        case .portugueseBrazil:
            return ["pt-BR", "pt"]
        case .russian:
            return ["ru"]
        }
    }
}

enum AppLanguageBootstrap {
    static let languageAtLaunch: AppLanguage = PersistenceService().loadStoredLanguagePreference()

    static func prepareForLaunch() {
        apply(language: languageAtLaunch)
    }

    static func apply(language: AppLanguage) {
        guard let bundleIdentifier = Bundle.main.bundleIdentifier, !bundleIdentifier.isEmpty else {
            return
        }

        let appID = bundleIdentifier as CFString
        if let identifiers = language.appleLanguageIdentifiers {
            CFPreferencesSetAppValue("AppleLanguages" as CFString, identifiers as CFPropertyList, appID)
            CFPreferencesSetAppValue("AppleLocale" as CFString, language.localeIdentifier as CFPropertyList, appID)
        } else {
            CFPreferencesSetAppValue("AppleLanguages" as CFString, nil, appID)
            CFPreferencesSetAppValue("AppleLocale" as CFString, nil, appID)
        }
        CFPreferencesAppSynchronize(appID)
    }
}

enum AppThemeMode: String, Codable, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: return String(localized: "settings.theme.auto", defaultValue: "Auto", bundle: .module)
        case .light: return String(localized: "settings.theme.light", defaultValue: "Light", bundle: .module)
        case .dark: return String(localized: "settings.theme.dark", defaultValue: "Dark", bundle: .module)
        }
    }

    var colorScheme: ColorScheme? {
        switch self {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }

    var appearance: NSAppearance? {
        switch self {
        case .system:
            return nil
        case .light:
            return NSAppearance(named: .aqua)
        case .dark:
            return NSAppearance(named: .darkAqua)
        }
    }
}

enum AppTerminalBackgroundPreset: String, Codable, CaseIterable, Identifiable {
    case obsidian
    case graphite
    case midnight
    case forest
    case paper
    case sand
    case mist
    case dawn

    var id: String { rawValue }

    var title: String {
        switch self {
        case .obsidian:
            return String(localized: "settings.terminal_background.preset.obsidian", defaultValue: "Obsidian", bundle: .module)
        case .graphite:
            return String(localized: "settings.terminal_background.preset.graphite", defaultValue: "Graphite", bundle: .module)
        case .midnight:
            return String(localized: "settings.terminal_background.preset.midnight", defaultValue: "Midnight", bundle: .module)
        case .forest:
            return String(localized: "settings.terminal_background.preset.forest", defaultValue: "Forest", bundle: .module)
        case .paper:
            return String(localized: "settings.terminal_background.preset.paper", defaultValue: "Paper", bundle: .module)
        case .sand:
            return String(localized: "settings.terminal_background.preset.sand", defaultValue: "Sand", bundle: .module)
        case .mist:
            return String(localized: "settings.terminal_background.preset.mist", defaultValue: "Mist", bundle: .module)
        case .dawn:
            return String(localized: "settings.terminal_background.preset.dawn", defaultValue: "Dawn", bundle: .module)
        }
    }

    var backgroundColor: NSColor {
        switch self {
        case .obsidian:
            return .dmuxHex(0x1E1E1E)
        case .graphite:
            return .dmuxHex(0x292829)
        case .midnight:
            return .dmuxHex(0x162033)
        case .forest:
            return .dmuxHex(0x1A231D)
        case .paper:
            return .dmuxHex(0xF5F5F5)
        case .sand:
            return .dmuxHex(0xEFE6DA)
        case .mist:
            return .dmuxHex(0xE9EEF3)
        case .dawn:
            return .dmuxHex(0xF5F0FF)
        }
    }

    var foregroundColor: NSColor {
        switch self {
        case .obsidian:
            return .dmuxHex(0xE6EDF3)
        case .graphite:
            return .dmuxHex(0xE2E8EF)
        case .midnight:
            return .dmuxHex(0xE6EEF8)
        case .forest:
            return .dmuxHex(0xE7F0EA)
        case .paper:
            return .dmuxHex(0x202124)
        case .sand:
            return .dmuxHex(0x2C241D)
        case .mist:
            return .dmuxHex(0x1F2A36)
        case .dawn:
            return .dmuxHex(0x2E2742)
        }
    }

    var mutedForegroundColor: NSColor {
        switch self {
        case .obsidian:
            return .dmuxHex(0x9AA4B2)
        case .graphite:
            return .dmuxHex(0x959EAA)
        case .midnight:
            return .dmuxHex(0x9FB0C5)
        case .forest:
            return .dmuxHex(0x99A99D)
        case .paper:
            return .dmuxHex(0x6F7782)
        case .sand:
            return .dmuxHex(0x857564)
        case .mist:
            return .dmuxHex(0x687789)
        case .dawn:
            return .dmuxHex(0x8677A6)
        }
    }

    var dividerColor: NSColor {
        switch self {
        case .obsidian, .graphite, .midnight, .forest:
            return NSColor.white.withAlphaComponent(0.14)
        case .paper, .sand, .mist, .dawn:
            return NSColor.black.withAlphaComponent(0.12)
        }
    }

    var isLight: Bool {
        switch self {
        case .paper, .sand, .mist, .dawn:
            return true
        case .obsidian, .graphite, .midnight, .forest:
            return false
        }
    }
}

enum AppIconStyle: String, Codable, CaseIterable, Identifiable {
    case `default`
    case cobalt
    case sunset
    case forest

    var id: String { rawValue }

    var title: String {
        switch self {
        case .default: return String(localized: "settings.app_icon.option.default", defaultValue: "Default", bundle: .module)
        case .cobalt: return String(localized: "settings.app_icon.option.cobalt", defaultValue: "Cobalt", bundle: .module)
        case .sunset: return String(localized: "settings.app_icon.option.sunset", defaultValue: "Sunset", bundle: .module)
        case .forest: return String(localized: "settings.app_icon.option.forest", defaultValue: "Forest", bundle: .module)
        }
    }

    fileprivate var iconFill: (top: NSColor, bottom: NSColor) {
        switch self {
        case .default:
            return (
                NSColor(calibratedRed: 0.24, green: 0.50, blue: 0.98, alpha: 1),
                NSColor(calibratedRed: 0.16, green: 0.36, blue: 0.86, alpha: 1)
            )
        case .cobalt:
            return (
                NSColor(calibratedRed: 0.12, green: 0.14, blue: 0.20, alpha: 1),
                NSColor(calibratedRed: 0.08, green: 0.09, blue: 0.13, alpha: 1)
            )
        case .sunset:
            return (
                NSColor(calibratedRed: 0.96, green: 0.42, blue: 0.32, alpha: 1),
                NSColor(calibratedRed: 0.88, green: 0.30, blue: 0.26, alpha: 1)
            )
        case .forest:
            return (
                NSColor(calibratedRed: 0.18, green: 0.62, blue: 0.45, alpha: 1),
                NSColor(calibratedRed: 0.12, green: 0.50, blue: 0.36, alpha: 1)
            )
        }
    }
}

enum AppTerminalProfile: String, Codable, CaseIterable, Identifiable {
    case zsh
    case bash
    case sh
    case fish

    var id: String { rawValue }

    var title: String {
        switch self {
        case .zsh: return "zsh"
        case .bash: return "bash"
        case .sh: return "sh"
        case .fish: return "fish"
        }
    }

    var shellPath: String {
        switch self {
        case .zsh: return "/bin/zsh"
        case .bash: return "/bin/bash"
        case .sh: return "/bin/sh"
        case .fish: return "/opt/homebrew/bin/fish"
        }
    }

    static var available: [AppTerminalProfile] {
        let fileManager = FileManager.default
        return allCases.filter { fileManager.fileExists(atPath: $0.shellPath) }
    }

    static var allShellPaths: Set<String> {
        Set(allCases.map(\.shellPath))
    }
}

struct AppShortcutConfiguration: Codable, Equatable {
    var splitPane: AppKeyboardShortcut?
    var createTab: AppKeyboardShortcut?
    var toggleGitPanel: AppKeyboardShortcut?
    var toggleAIPanel: AppKeyboardShortcut?

    static let defaults = AppShortcutConfiguration(
        splitPane: AppKeyboardShortcut(key: "t", modifiers: [.command]),
        createTab: AppKeyboardShortcut(key: "d", modifiers: [.command]),
        toggleGitPanel: AppKeyboardShortcut(key: "g", modifiers: [.command]),
        toggleAIPanel: AppKeyboardShortcut(key: "y", modifiers: [.command])
    )

    static let legacyDefaults = AppShortcutConfiguration(
        splitPane: AppKeyboardShortcut(key: "d", modifiers: [.command, .shift]),
        createTab: AppKeyboardShortcut(key: "t", modifiers: [.command, .shift]),
        toggleGitPanel: AppKeyboardShortcut(key: "g", modifiers: [.command, .shift]),
        toggleAIPanel: AppKeyboardShortcut(key: "a", modifiers: [.command, .shift])
    )

    static let swappedPlainDefaults = AppShortcutConfiguration(
        splitPane: AppKeyboardShortcut(key: "d", modifiers: [.command]),
        createTab: AppKeyboardShortcut(key: "t", modifiers: [.command]),
        toggleGitPanel: AppKeyboardShortcut(key: "g", modifiers: [.command]),
        toggleAIPanel: AppKeyboardShortcut(key: "y", modifiers: [.command])
    )

    func migratedFromLegacyDefaultsIfNeeded() -> AppShortcutConfiguration {
        if self == Self.legacyDefaults || self == Self.swappedPlainDefaults {
            return Self.defaults
        }
        return self
    }
}

struct AppKeyboardShortcut: Codable, Equatable {
    var key: String
    var modifiers: AppShortcutModifiers

    var title: String {
        let normalizedKey = key.count == 1 ? key.uppercased() : key
        return modifiers.symbols + normalizedKey
    }

    var keyEquivalent: KeyEquivalent {
        KeyEquivalent(Character(key.lowercased()))
    }

    var eventModifiers: EventModifiers {
        modifiers.eventModifiers
    }
}

struct AppShortcutModifiers: OptionSet, Codable, Equatable {
    let rawValue: Int

    static let command = AppShortcutModifiers(rawValue: 1 << 0)
    static let shift = AppShortcutModifiers(rawValue: 1 << 1)
    static let option = AppShortcutModifiers(rawValue: 1 << 2)
    static let control = AppShortcutModifiers(rawValue: 1 << 3)

    var symbols: String {
        var value = ""
        if contains(.control) { value += "^" }
        if contains(.option) { value += "⌥" }
        if contains(.shift) { value += "⇧" }
        if contains(.command) { value += "⌘" }
        return value
    }

    var eventModifiers: EventModifiers {
        var modifiers: EventModifiers = []
        if contains(.command) { modifiers.insert(.command) }
        if contains(.shift) { modifiers.insert(.shift) }
        if contains(.option) { modifiers.insert(.option) }
        if contains(.control) { modifiers.insert(.control) }
        return modifiers
    }

    init(rawValue: Int) {
        self.rawValue = rawValue
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        rawValue = try container.decode(Int.self)
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }
}

extension AppShortcutModifiers {
    static func from(eventModifiers: NSEvent.ModifierFlags) -> AppShortcutModifiers {
        var value: AppShortcutModifiers = []
        if eventModifiers.contains(.command) { value.insert(.command) }
        if eventModifiers.contains(.shift) { value.insert(.shift) }
        if eventModifiers.contains(.option) { value.insert(.option) }
        if eventModifiers.contains(.control) { value.insert(.control) }
        return value
    }
}

enum AppShortcutTarget {
    case splitPane
    case createTab
    case toggleGitPanel
    case toggleAIPanel
}

private extension NSColor {
    static func dmuxHex(_ hex: UInt, alpha: CGFloat = 1) -> NSColor {
        NSColor(
            calibratedRed: CGFloat((hex >> 16) & 0xFF) / 255,
            green: CGFloat((hex >> 8) & 0xFF) / 255,
            blue: CGFloat(hex & 0xFF) / 255,
            alpha: alpha
        )
    }
}

enum AppSupportLinks {
    static let github = URL(string: "https://github.com/duxweb/codux")!
    static let issues = URL(string: "https://github.com/duxweb/codux/issues")!
    static let website = URL(string: "https://codux.dux.cn")!
    static let releases = URL(string: "https://github.com/duxweb/codux/releases")!
}

enum AppIconRenderer {
    enum Variant {
        case standard
        case dev
        case debug

        static func current(bundle: Bundle = .main) -> Variant {
            let bundleIdentifier = (bundle.bundleIdentifier ?? "").lowercased()
            if bundleIdentifier.hasSuffix(".dev") {
                return .dev
            }
            if bundleIdentifier.hasSuffix(".debug") {
                return .debug
            }
            return .standard
        }
    }

    static func image(for style: AppIconStyle, size: CGFloat = 128, variant: Variant = .current()) -> NSImage {
        let image = NSImage(size: NSSize(width: size, height: size))
        image.lockFocus()
        defer { image.unlockFocus() }

        guard NSGraphicsContext.current != nil else { return image }

        let inset = size * 0.08
        let insetRect = NSRect(x: inset, y: inset, width: size - inset * 2, height: size - inset * 2)
        let radius = size * 0.22
        let shape = NSBezierPath(roundedRect: insetRect, xRadius: radius, yRadius: radius)
        let fill = style.iconFill

        NSGraphicsContext.saveGraphicsState()
        shape.addClip()

        // 1. Background gradient
        let bg = NSGradient(starting: fill.top, ending: fill.bottom)!
        bg.draw(in: insetRect, angle: 90)

        // 2. Top highlight
        let hlCenter = CGPoint(x: insetRect.midX, y: insetRect.maxY - size * 0.08)
        if let hlGrad = NSGradient(
            colors: [NSColor.white.withAlphaComponent(0.10), NSColor.white.withAlphaComponent(0.0)],
            atLocations: [0.0, 1.0],
            colorSpace: .deviceRGB
        ) {
            hlGrad.draw(fromCenter: hlCenter, radius: 0, toCenter: hlCenter, radius: size * 0.50, options: [.drawsAfterEndingLocation])
        }

        // 3. Bottom vignette
        let vCenter = CGPoint(x: insetRect.midX, y: insetRect.minY)
        if let vGrad = NSGradient(
            colors: [NSColor.black.withAlphaComponent(0.08), NSColor.black.withAlphaComponent(0.0)],
            atLocations: [0.0, 1.0],
            colorSpace: .deviceRGB
        ) {
            vGrad.draw(fromCenter: vCenter, radius: 0, toCenter: vCenter, radius: size * 0.45, options: [.drawsAfterEndingLocation])
        }

        // 4. Layered chevrons ">" — terminal mark with depth
        let cx = insetRect.midX
        let cy = insetRect.midY
        let chevronH = size * 0.30
        let chevronW = size * 0.17
        let weight = size * 0.09

        // 4a. Back chevron — larger, offset left, semi-transparent
        let backOffsetX = size * -0.10
        let backScale: CGFloat = 1.0
        let backChevron = NSBezierPath()
        backChevron.move(to: CGPoint(x: cx + backOffsetX - chevronW * 0.5 * backScale, y: cy + chevronH * 0.5 * backScale))
        backChevron.line(to: CGPoint(x: cx + backOffsetX + chevronW * 0.5 * backScale, y: cy))
        backChevron.line(to: CGPoint(x: cx + backOffsetX - chevronW * 0.5 * backScale, y: cy - chevronH * 0.5 * backScale))
        let backChevronColor: NSColor = .white.withAlphaComponent(0.4)
        backChevronColor.setStroke()
        backChevron.lineWidth = weight * backScale
        backChevron.lineCapStyle = .square
        backChevron.lineJoinStyle = .miter
        backChevron.stroke()

        // 4b. Front chevron — main, offset right, full white with shadow
        let frontOffsetX = size * 0.10

        let shadow = NSShadow()
        shadow.shadowColor = NSColor.black.withAlphaComponent(0.2)
        shadow.shadowOffset = NSSize(width: 0, height: -size * 0.01)
        shadow.shadowBlurRadius = size * 0.02
        shadow.set()

        let frontChevron = NSBezierPath()
        frontChevron.move(to: CGPoint(x: cx + frontOffsetX - chevronW * 0.5, y: cy + chevronH * 0.5))
        frontChevron.line(to: CGPoint(x: cx + frontOffsetX + chevronW * 0.5, y: cy))
        frontChevron.line(to: CGPoint(x: cx + frontOffsetX - chevronW * 0.5, y: cy - chevronH * 0.5))
        let frontChevronColor: NSColor = {
            guard style == .default else {
                return .white
            }
            switch variant {
            case .standard, .debug:
                return .white
            case .dev:
                return NSColor(calibratedRed: 1.00, green: 0.88, blue: 0.22, alpha: 1.0)
            }
        }()
        frontChevronColor.setStroke()
        frontChevron.lineWidth = weight
        frontChevron.lineCapStyle = .square
        frontChevron.lineJoinStyle = .miter
        frontChevron.stroke()

        let noShadow = NSShadow()
        noShadow.shadowColor = nil
        noShadow.set()

        // 5. Inner edge highlight
        let innerShape = NSBezierPath(roundedRect: insetRect.insetBy(dx: 0.5, dy: 0.5), xRadius: radius, yRadius: radius)
        NSColor.white.withAlphaComponent(0.08).setStroke()
        innerShape.lineWidth = 1.0
        innerShape.stroke()

        NSGraphicsContext.restoreGraphicsState()

        return image
    }

}
