import AppKit
import Foundation
import SwiftUI

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

struct AppAIToolPermissionSettings: Codable, Equatable, Sendable {
    var codex: AppAIToolPermissionMode = .default
    var claudeCode: AppAIToolPermissionMode = .default
    var gemini: AppAIToolPermissionMode = .default
    var opencode: AppAIToolPermissionMode = .default
    var codexModel: String = ""
    var claudeCodeModel: String = ""
    var geminiModel: String = ""
    var opencodeModel: String = ""

    func model(for tool: AppSupportedAITool) -> String {
        switch tool {
        case .codex:
            return codexModel
        case .claudeCode:
            return claudeCodeModel
        case .gemini:
            return geminiModel
        case .opencode:
            return opencodeModel
        }
    }
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

    var rawValue: String {
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

    var symbol: String {
        switch self {
        case .codex:
            return "terminal"
        case .claudeCode:
            return "sparkles"
        case .gemini:
            return "diamond"
        case .opencode:
            return "curlybraces"
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

struct AppDeveloperSettings: Codable, Equatable {
    var showsPerformanceMonitor = false
    var performanceMonitorSamplingInterval: TimeInterval = 3

    init() {}

    enum CodingKeys: String, CodingKey {
        case showsPerformanceMonitor
        case performanceMonitorSamplingInterval
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
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
