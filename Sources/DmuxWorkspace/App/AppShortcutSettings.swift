import AppKit
import Foundation
import SwiftUI

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

extension NSColor {
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

enum AppRuntimePaths {
    static func isRunningUnderXCTest(
        processInfo: ProcessInfo = .processInfo,
        bundle: Bundle = .main
    ) -> Bool {
        if processInfo.environment["XCTestConfigurationFilePath"] != nil {
            return true
        }
        if bundle.bundleURL.pathExtension == "xctest" {
            return true
        }
        return appDisplayName(bundle: bundle).lowercased() == "xctest"
    }

    static func appDisplayName(bundle: Bundle = .main) -> String {
        if let displayName = bundle.object(forInfoDictionaryKey: "CFBundleDisplayName") as? String {
            let sanitized = appSupportFolderName(appDisplayName: displayName)
            if !sanitized.isEmpty {
                return sanitized
            }
        }

        if let bundleName = bundle.object(forInfoDictionaryKey: kCFBundleNameKey as String) as? String {
            let sanitized = appSupportFolderName(appDisplayName: bundleName)
            if !sanitized.isEmpty {
                return sanitized
            }
        }

        let fallbackName = bundle.bundleURL
            .deletingPathExtension()
            .lastPathComponent
        let sanitizedFallback = appSupportFolderName(appDisplayName: fallbackName)
        return sanitizedFallback.isEmpty ? "Codux" : sanitizedFallback
    }

    static func appSupportFolderName(bundle: Bundle = .main) -> String {
        appDisplayName(bundle: bundle)
    }

    static func appSupportFolderName(appDisplayName: String) -> String {
        let sanitized = appDisplayName
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: "\n", with: " ")
            .replacingOccurrences(of: "\r", with: " ")
        return sanitized.isEmpty ? "Codux" : sanitized
    }

    static func runtimeOwnerID(bundle: Bundle = .main) -> String {
        runtimeOwnerID(
            appDisplayName: appDisplayName(bundle: bundle),
            bundleIdentifier: bundle.bundleIdentifier
        )
    }

    static func runtimeOwnerID(appDisplayName: String, bundleIdentifier: String? = nil) -> String {
        let base = appSupportFolderName(appDisplayName: appDisplayName)
        let mapped = base.lowercased().map { character -> Character in
            if character.isLetter || character.isNumber {
                return character
            }
            return "-"
        }
        let collapsed = String(mapped)
            .replacingOccurrences(of: "-+", with: "-", options: .regularExpression)
            .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
        if !collapsed.isEmpty {
            return collapsed
        }

        if let bundleIdentifier,
           let lastComponent = bundleIdentifier.split(separator: ".").last {
            let fallback = String(lastComponent)
                .lowercased()
                .map { $0.isLetter || $0.isNumber ? $0 : "-" }
            let normalized = String(fallback)
                .replacingOccurrences(of: "-+", with: "-", options: .regularExpression)
                .trimmingCharacters(in: CharacterSet(charactersIn: "-"))
            if !normalized.isEmpty {
                return normalized
            }
        }

        return "codux"
    }

    static func appSupportRootURL(
        fileManager: FileManager = .default,
        bundle: Bundle = .main
    ) -> URL? {
        fileManager
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first?
            .appendingPathComponent(appSupportFolderName(bundle: bundle), isDirectory: true)
    }

    static func temporaryRootURL(
        fileManager: FileManager = .default,
        bundle: Bundle = .main
    ) -> URL {
        temporaryRootURL(
            fileManager: fileManager,
            appDisplayName: appDisplayName(bundle: bundle),
            bundleIdentifier: bundle.bundleIdentifier
        )
    }

    static func temporaryRootURL(
        fileManager: FileManager = .default,
        appDisplayName: String,
        bundleIdentifier: String? = nil
    ) -> URL {
        fileManager.temporaryDirectory
            .appendingPathComponent(
                runtimeOwnerID(
                    appDisplayName: appDisplayName,
                    bundleIdentifier: bundleIdentifier
                ),
                isDirectory: true
            )
    }

    static func runtimeSupportRootURL(
        fileManager: FileManager = .default,
        bundle: Bundle = .main
    ) -> URL? {
        appSupportRootURL(fileManager: fileManager, bundle: bundle)?
            .appendingPathComponent("runtime-support", isDirectory: true)
    }
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
