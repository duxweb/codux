import AppKit
import Foundation
import SwiftUI

struct AppSettings: Codable, Equatable {
    var language: AppLanguage = .system
    var themeMode: AppThemeMode = .system
    var terminalBackgroundPreset: AppTerminalBackgroundPreset = .obsidian
    var iconStyle: AppIconStyle = .default
    var defaultTerminal: AppTerminalProfile = .zsh
    var showsDockBadge = false
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
        case iconStyle
        case defaultTerminal
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
        iconStyle = try container.decodeIfPresent(AppIconStyle.self, forKey: .iconStyle) ?? .default
        defaultTerminal = try container.decodeIfPresent(AppTerminalProfile.self, forKey: .defaultTerminal) ?? .zsh
        showsDockBadge = try container.decodeIfPresent(Bool.self, forKey: .showsDockBadge) ?? false
        gitAutoRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .gitAutoRefreshInterval) ?? 60
        aiAutoRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .aiAutoRefreshInterval) ?? 180
        aiBackgroundRefreshInterval = try container.decodeIfPresent(TimeInterval.self, forKey: .aiBackgroundRefreshInterval) ?? 600
        developer = try container.decodeIfPresent(AppDeveloperSettings.self, forKey: .developer) ?? .init()
        shortcuts = (try container.decodeIfPresent(AppShortcutConfiguration.self, forKey: .shortcuts) ?? .defaults)
            .migratedFromLegacyDefaultsIfNeeded()
    }
}

struct AppDeveloperSettings: Codable, Equatable {
    var showsNotificationTestButton = false
    var showsDebugLogButton = false
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
        case .system: return appI18n("settings.language.follow_system", fallback: "System")
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

    func title(for language: AppLanguage) -> String {
        if self == .system {
            return language.i18n("settings.language.follow_system", fallback: "System")
        }
        return title
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

    func i18n(_ key: String, fallback: String) -> String {
        AppLocalizationCatalog.localizedString(for: resolved, key: key, fallback: fallback)
    }
}

func appI18n(_ key: String, fallback: String) -> String {
    let language = PersistenceService().load()?.appSettings?.language.resolved ?? AppLanguage.system.resolved
    return AppLocalizationCatalog.localizedString(for: language, key: key, fallback: fallback)
}

enum AppThemeMode: String, Codable, CaseIterable, Identifiable {
    case system
    case light
    case dark

    var id: String { rawValue }

    var title: String {
        switch self {
        case .system: return appI18n("settings.theme.auto", fallback: "Auto")
        case .light: return appI18n("settings.theme.light", fallback: "Light")
        case .dark: return appI18n("settings.theme.dark", fallback: "Dark")
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

    func title(for language: AppLanguage) -> String {
        switch self {
        case .obsidian:
            return language.i18n("settings.terminal_background.preset.obsidian", fallback: "Obsidian")
        case .graphite:
            return language.i18n("settings.terminal_background.preset.graphite", fallback: "Graphite")
        case .midnight:
            return language.i18n("settings.terminal_background.preset.midnight", fallback: "Midnight")
        case .forest:
            return language.i18n("settings.terminal_background.preset.forest", fallback: "Forest")
        case .paper:
            return language.i18n("settings.terminal_background.preset.paper", fallback: "Paper")
        case .sand:
            return language.i18n("settings.terminal_background.preset.sand", fallback: "Sand")
        case .mist:
            return language.i18n("settings.terminal_background.preset.mist", fallback: "Mist")
        case .dawn:
            return language.i18n("settings.terminal_background.preset.dawn", fallback: "Dawn")
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
        case .default: return appI18n("settings.app_icon.option.default", fallback: "Default")
        case .cobalt: return appI18n("settings.app_icon.option.cobalt", fallback: "Cobalt")
        case .sunset: return appI18n("settings.app_icon.option.sunset", fallback: "Sunset")
        case .forest: return appI18n("settings.app_icon.option.forest", fallback: "Forest")
        }
    }

    func title(for language: AppLanguage) -> String {
        switch self {
        case .default:
            return language.i18n("settings.app_icon.option.default", fallback: "Default")
        case .cobalt:
            return language.i18n("settings.app_icon.option.cobalt", fallback: "Cobalt")
        case .sunset:
            return language.i18n("settings.app_icon.option.sunset", fallback: "Sunset")
        case .forest:
            return language.i18n("settings.app_icon.option.forest", fallback: "Forest")
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

enum AppLocalizationCatalog {
    private static let tables: [AppLanguage: [String: String]] = [
        .english: loadTable(for: .english),
        .simplifiedChinese: loadTable(for: .simplifiedChinese),
        .traditionalChinese: loadTable(for: .traditionalChinese),
        .japanese: loadTable(for: .japanese),
        .korean: loadTable(for: .korean),
        .french: loadTable(for: .french),
        .german: loadTable(for: .german),
        .spanish: loadTable(for: .spanish),
        .portugueseBrazil: loadTable(for: .portugueseBrazil),
        .russian: loadTable(for: .russian),
    ]

    static func localizedString(for language: AppLanguage, key: String, fallback: String) -> String {
        let resolved = language.resolved
        let normalizedKey = key.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedKey.isEmpty else {
            return fallback
        }
        let table = package(for: resolved)
        return table[normalizedKey] ?? fallback
    }

    private static func package(for language: AppLanguage) -> [String: String] {
        switch language {
        case .system:
            return [:]
        case .simplifiedChinese, .traditionalChinese, .english, .japanese, .korean, .french, .german, .spanish, .portugueseBrazil, .russian:
            return tables[language] ?? [:]
        }
    }

    private static func loadTable(for language: AppLanguage) -> [String: String] {
        let candidateURLs: [URL?] = [
            Bundle.module.url(
                forResource: language.resourceFileName,
                withExtension: "json",
                subdirectory: "Localization"
            ),
            Bundle.module.url(
                forResource: language.resourceFileName,
                withExtension: "json"
            ),
        ]

        guard let url = candidateURLs.compactMap({ $0 }).first,
              let data = try? Data(contentsOf: url),
              let table = try? JSONDecoder().decode([String: String].self, from: data) else {
            return [:]
        }
        return table
    }
}

private extension AppLanguage {
    var resourceFileName: String {
        switch self {
        case .system:
            return "system"
        case .simplifiedChinese:
            return "zh-Hans"
        case .traditionalChinese:
            return "zh-Hant"
        case .english:
            return "en"
        case .japanese:
            return "ja"
        case .korean:
            return "ko"
        case .french:
            return "fr"
        case .german:
            return "de"
        case .spanish:
            return "es"
        case .portugueseBrazil:
            return "pt-BR"
        case .russian:
            return "ru"
        }
    }
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
    static let github = URL(string: "https://github.com/duxweb/dmux")!
    static let issues = URL(string: "https://github.com/duxweb/dmux/issues")!
    static let website = URL(string: "https://dmux.app")!
    static let releases = URL(string: "https://github.com/duxweb/dmux/releases")!
}

enum AppIconRenderer {
    static func image(for style: AppIconStyle, size: CGFloat = 128) -> NSImage {
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
        NSColor.white.withAlphaComponent(0.4).setStroke()
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
        NSColor.white.setStroke()
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
