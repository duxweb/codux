import AppKit
import Darwin
import Foundation
import GhosttyTerminal
import GhosttyTheme
import QuartzCore
import SwiftUI

enum GhosttyEmbeddedConfig {
    private struct ParsedThemeOverrides {
        var backgroundColor: NSColor?
        var foregroundColor: NSColor?
        var cursorColor: NSColor?
        var cursorTextColor: NSColor?
        var selectionBackgroundColor: NSColor?
        var selectionForegroundColor: NSColor?
        var palette: [Int: String] = [:]
    }

    struct ResolvedControllerConfig {
        let configSource: TerminalController.ConfigSource
        let userConfigPaths: [String]

        var prefersUserConfig: Bool {
            !userConfigPaths.isEmpty
        }

        var userConfigDescription: String? {
            guard !userConfigPaths.isEmpty else {
                return nil
            }
            return userConfigPaths.joined(separator: ", ")
        }
    }

    static let candidateRelativePaths = [
        ".config/ghostty/config.ghostty",
        ".config/ghostty/config",
        "Library/Application Support/com.mitchellh.ghostty/config.ghostty",
        "Library/Application Support/com.mitchellh.ghostty/config",
    ]

    static let candidateThemeDirectoryRelativePaths = [
        ".config/ghostty/themes",
        "Library/Application Support/com.mitchellh.ghostty/themes",
    ]

    static let fallbackEditingKeybinds = [
        "cmd+left=text:\\x01",
        "cmd+right=text:\\x05",
        "option+left=text:\\x1bb",
        "option+right=text:\\x1bf",
        "cmd+backspace=text:\\x15",
        "option+backspace=text:\\x17",
    ]

    static func resolvedUserConfigFileURLs(
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> [URL] {
        var urls: [URL] = []
        var seenPaths = Set<String>()
        for relativePath in candidateRelativePaths {
            let url = homeDirectoryURL.appendingPathComponent(relativePath, isDirectory: false)
            guard existingNonDirectoryURL(url, fileManager: fileManager) != nil,
                  seenPaths.insert(url.path).inserted else {
                continue
            }
            urls.append(url)
        }
        return urls
    }

    static func resolvedControllerConfig(
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> ResolvedControllerConfig {
        let urls = resolvedUserConfigFileURLs(
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
        guard !urls.isEmpty else {
            return ResolvedControllerConfig(
                configSource: .generated(fallbackEditingConfigContents()),
                userConfigPaths: []
            )
        }

        let mergedContents = mergedUserConfigContents(
            urls.map { url in
                (url, (try? String(contentsOf: url, encoding: .utf8)) ?? "")
            }
        )
        return ResolvedControllerConfig(
            configSource: .generated(mergedContents),
            userConfigPaths: urls.map(\.path)
        )
    }

    static func fallbackEditingConfigContents() -> String {
        fallbackEditingKeybinds
            .map { "keybind = \($0)" }
            .joined(separator: "\n") + "\n"
    }

    static func mergedUserConfigContents(_ userConfigEntries: [(URL, String)]) -> String {
        var sections = [fallbackEditingConfigContents().trimmingCharacters(in: .whitespacesAndNewlines)]

        for (url, contents) in userConfigEntries {
            let normalizedContents = sanitizedEmbeddedUserConfigContents(contents)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !normalizedContents.isEmpty else {
                continue
            }
            sections.append("# Source: \(url.path)\n\(normalizedContents)")
        }

        return sections.joined(separator: "\n\n") + "\n"
    }

    private static func sanitizedEmbeddedUserConfigContents(_ contents: String) -> String {
        let lines = contents.components(separatedBy: .newlines)
        var keptLines: [String] = []
        keptLines.reserveCapacity(lines.count)

        for rawLine in lines {
            guard let assignment = parseConfigAssignment(from: rawLine) else {
                keptLines.append(rawLine)
                continue
            }

            // Embedded Ghostty uses a different host/composition path than the
            // standalone app. Cursor thickness amplification is visually unstable
            // here, so keep the cursor style/opacity but ignore explicit thickness.
            if assignment.key != "adjust-cursor-thickness" {
                keptLines.append(rawLine)
            }
        }

        return keptLines.joined(separator: "\n")
    }

    static func resolvedAutomaticTerminalAppearance(
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser,
        prefersDarkAppearance: Bool = true
    ) -> AppEffectiveTerminalAppearance {
        let fallbackBase = AppTerminalBackgroundPreset
            .automaticFallbackPreset(prefersDarkAppearance: prefersDarkAppearance)
            .effectiveAppearance(backgroundColorPreset: .automatic)
        let fallback = AppEffectiveTerminalAppearance(
            backgroundColor: AppBackgroundColorPreset.base950.swatchColor ?? fallbackBase.backgroundColor,
            foregroundColor: fallbackBase.foregroundColor,
            cursorColor: fallbackBase.cursorColor,
            cursorTextColor: fallbackBase.cursorTextColor,
            selectionBackgroundColor: fallbackBase.selectionBackgroundColor,
            selectionForegroundColor: fallbackBase.selectionForegroundColor,
            paletteHexStrings: fallbackBase.paletteHexStrings,
            isLight: false,
            minimumContrast: fallbackBase.minimumContrast
        )

        let urls = resolvedUserConfigFileURLs(
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
        guard !urls.isEmpty else {
            return fallback
        }

        let entries = urls.map { url in
            (url, (try? String(contentsOf: url, encoding: .utf8)) ?? "")
        }
        return automaticTerminalAppearance(
            from: entries,
            fallback: fallback,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
    }

    static func automaticTerminalAppearance(
        from userConfigEntries: [(URL, String)],
        fallback: AppEffectiveTerminalAppearance,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> AppEffectiveTerminalAppearance {
        var appearance = fallback

        for (_, contents) in userConfigEntries {
            for rawLine in contents.components(separatedBy: .newlines) {
                guard let assignment = parseConfigAssignment(from: rawLine) else {
                    continue
                }
                let key = assignment.key
                let rawValue = assignment.value

                if key == "theme",
                   let themeName = Self.unquoted(rawValue) as String? {
                    if let themeAppearance = Self.bundledThemeAppearance(
                        named: themeName,
                        fileManager: fileManager,
                        homeDirectoryURL: homeDirectoryURL
                    ) {
                        appearance = themeAppearance
                        continue
                    }
                    if let themePreset = AppTerminalBackgroundPreset.automaticMatch(
                        forGhosttyThemeName: themeName
                    ) {
                        appearance = themePreset.effectiveAppearance(backgroundColorPreset: .automatic)
                        continue
                    }
                }

                if key == "palette",
                   let paletteIndex = Self.parsePaletteIndex(from: rawValue),
                   let paletteColor = Self.parseHexColorString(from: rawValue) {
                    var palette = appearance.paletteHexStrings
                    while palette.count <= paletteIndex {
                        palette.append(appearance.backgroundColor.ghosttyHexString)
                    }
                    palette[paletteIndex] = paletteColor
                    appearance = AppEffectiveTerminalAppearance(
                        backgroundColor: appearance.backgroundColor,
                        foregroundColor: appearance.foregroundColor,
                        cursorColor: appearance.cursorColor,
                        cursorTextColor: appearance.cursorTextColor,
                        selectionBackgroundColor: appearance.selectionBackgroundColor,
                        selectionForegroundColor: appearance.selectionForegroundColor,
                        paletteHexStrings: palette,
                        isLight: appearance.isLight,
                        minimumContrast: appearance.minimumContrast
                    )
                    continue
                }

                var overrides = ParsedThemeOverrides()

                switch key {
                case "background":
                    overrides.backgroundColor = Self.parseColor(from: rawValue)
                case "foreground":
                    overrides.foregroundColor = Self.parseColor(from: rawValue)
                case "cursor-color":
                    overrides.cursorColor = Self.parseColor(from: rawValue)
                case "cursor-text":
                    overrides.cursorTextColor = Self.parseColor(from: rawValue)
                case "selection-background":
                    overrides.selectionBackgroundColor = Self.parseColor(from: rawValue)
                case "selection-foreground":
                    overrides.selectionForegroundColor = Self.parseColor(from: rawValue)
                default:
                    continue
                }

                appearance = apply(overrides: overrides, to: appearance)
            }
        }

        return appearance
    }

    private static func apply(
        overrides: ParsedThemeOverrides,
        to appearance: AppEffectiveTerminalAppearance
    ) -> AppEffectiveTerminalAppearance {
        let backgroundColor = overrides.backgroundColor ?? appearance.backgroundColor
        let isLight = backgroundColor.dmuxPerceivedBrightness >= 0.72
        return AppEffectiveTerminalAppearance(
            backgroundColor: backgroundColor,
            foregroundColor: overrides.foregroundColor ?? appearance.foregroundColor,
            cursorColor: overrides.cursorColor ?? appearance.cursorColor,
            cursorTextColor: overrides.cursorTextColor ?? appearance.cursorTextColor,
            selectionBackgroundColor: overrides.selectionBackgroundColor ?? appearance.selectionBackgroundColor,
            selectionForegroundColor: overrides.selectionForegroundColor ?? appearance.selectionForegroundColor,
            paletteHexStrings: overrides.palette.isEmpty ? appearance.paletteHexStrings : mergedPalette(overrides.palette, into: appearance.paletteHexStrings),
            isLight: isLight,
            minimumContrast: isLight ? 1.05 : 1.0
        )
    }

    static func bundledThemeAppearance(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> AppEffectiveTerminalAppearance? {
        let definition = resolvedThemeDefinition(
            named: name,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
        return definition.map { appearance(from: $0) }
    }

    private static func resolvedThemeDefinition(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> GhosttyThemeDefinition? {
        if let userTheme = resolvedUserThemeDefinition(
            named: name,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        ) {
            return userTheme
        }

        if let exact = GhosttyThemeCatalog.theme(named: name) {
            return exact
        }

        let normalizedQuery = normalizeThemeName(name)
        guard normalizedQuery.isEmpty == false else {
            return nil
        }

        return GhosttyThemeCatalog
            .search(name)
            .first(where: { normalizeThemeName($0.name) == normalizedQuery })
    }

    private static func resolvedUserThemeDefinition(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> GhosttyThemeDefinition? {
        guard let url = resolvedUserThemeFileURL(
            named: name,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        ),
        let contents = try? String(contentsOf: url, encoding: .utf8) else {
            return nil
        }

        return parseUserThemeDefinition(
            name: url.lastPathComponent,
            contents: contents
        )
    }

    private static func resolvedUserThemeFileURL(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL? {
        let candidates = [name, name.lowercased(), name.capitalized]
        for relativePath in candidateThemeDirectoryRelativePaths {
            let directory = homeDirectoryURL.appendingPathComponent(relativePath, isDirectory: true)
            for candidate in candidates {
                let url = directory.appendingPathComponent(candidate, isDirectory: false)
                if existingNonDirectoryURL(url, fileManager: fileManager) != nil {
                    return url
                }
            }
        }
        return nil
    }

    private static func existingNonDirectoryURL(
        _ url: URL,
        fileManager: FileManager
    ) -> URL? {
        var isDirectory = ObjCBool(false)
        guard fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory),
              isDirectory.boolValue == false else {
            return nil
        }
        return url
    }

    private static func parseConfigAssignment(
        from rawLine: String
    ) -> (key: String, value: String)? {
        let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
        guard line.isEmpty == false,
              line.hasPrefix("#") == false,
              let separatorIndex = line.firstIndex(of: "=") else {
            return nil
        }

        let key = line[..<separatorIndex]
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        let value = String(
            line[line.index(after: separatorIndex)...]
                .trimmingCharacters(in: .whitespacesAndNewlines)
        )
        guard value.isEmpty == false else {
            return nil
        }
        return (key, value)
    }

    private static func parseUserThemeDefinition(
        name: String,
        contents: String
    ) -> GhosttyThemeDefinition? {
        var background: String?
        var foreground: String?
        var cursorColor: String?
        var cursorText: String?
        var selectionBackground: String?
        var selectionForeground: String?
        var palette: [Int: String] = [:]

        for rawLine in contents.components(separatedBy: .newlines) {
            guard let assignment = parseConfigAssignment(from: rawLine) else {
                continue
            }
            let key = assignment.key
            let rawValue = assignment.value

            switch key {
            case "background":
                background = parseHexColorString(from: rawValue)
            case "foreground":
                foreground = parseHexColorString(from: rawValue)
            case "cursor-color":
                cursorColor = parseHexColorString(from: rawValue)
            case "cursor-text":
                cursorText = parseHexColorString(from: rawValue)
            case "selection-background":
                selectionBackground = parseHexColorString(from: rawValue)
            case "selection-foreground":
                selectionForeground = parseHexColorString(from: rawValue)
            case "palette":
                if let index = parsePaletteIndex(from: rawValue),
                   let color = parseHexColorString(from: rawValue) {
                    palette[index] = String(color.dropFirst())
                }
            default:
                continue
            }
        }

        guard let background, let foreground else {
            return nil
        }

        return GhosttyThemeDefinition(
            name: name,
            background: background,
            foreground: foreground,
            cursorColor: cursorColor,
            cursorText: cursorText,
            selectionBackground: selectionBackground,
            selectionForeground: selectionForeground,
            palette: palette
        )
    }

    private static func appearance(from definition: GhosttyThemeDefinition) -> AppEffectiveTerminalAppearance {
        let backgroundColor = parseThemeDefinitionColor(definition.background) ?? .black
        let foregroundColor = parseThemeDefinitionColor(definition.foreground)
            ?? (backgroundColor.dmuxPerceivedBrightness >= 0.72 ? .black : .white)
        let selectionBackgroundColor = parseThemeDefinitionColor(definition.selectionBackground)
            ?? (backgroundColor.blended(withFraction: 0.2, of: foregroundColor) ?? backgroundColor)
        let selectionForegroundColor = parseThemeDefinitionColor(definition.selectionForeground)
            ?? foregroundColor
        let cursorColor = parseThemeDefinitionColor(definition.cursorColor) ?? foregroundColor
        let cursorTextColor = parseThemeDefinitionColor(definition.cursorText) ?? backgroundColor
        let isLight = backgroundColor.dmuxPerceivedBrightness >= 0.72
        let paletteHexStrings = (0..<16).map { index in
            normalizedHexColorString(definition.palette[index]) ?? backgroundColor.ghosttyHexString
        }

        return AppEffectiveTerminalAppearance(
            backgroundColor: backgroundColor,
            foregroundColor: foregroundColor,
            cursorColor: cursorColor,
            cursorTextColor: cursorTextColor,
            selectionBackgroundColor: selectionBackgroundColor,
            selectionForegroundColor: selectionForegroundColor,
            paletteHexStrings: paletteHexStrings,
            isLight: isLight,
            minimumContrast: isLight ? 1.05 : 1.0
        )
    }

    private static func mergedPalette(
        _ overrides: [Int: String],
        into palette: [String]
    ) -> [String] {
        var palette = palette
        for (index, color) in overrides {
            while palette.count <= index {
                palette.append("#000000")
            }
            palette[index] = color
        }
        return palette
    }

    private static func parsePaletteIndex(from rawValue: String) -> Int? {
        guard let match = rawValue.range(
            of: #"^\s*(\d+)\s*="#,
            options: .regularExpression
        ) else {
            return nil
        }
        let prefix = rawValue[match]
        let digits = prefix.replacingOccurrences(of: #"[^0-9]"#, with: "", options: .regularExpression)
        return Int(digits)
    }

    private static func parseColor(from rawValue: String) -> NSColor? {
        guard let hex = parseHexColorString(from: rawValue),
              let rgb = UInt(hex.dropFirst(), radix: 16) else {
            return nil
        }
        return NSColor(
            calibratedRed: CGFloat((rgb >> 16) & 0xFF) / 255,
            green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255,
            alpha: 1
        )
    }

    private static func parseColor(from rawValue: String?) -> NSColor? {
        guard let rawValue else {
            return nil
        }
        return parseColor(from: rawValue)
    }

    private static func parseHexColorString(from rawValue: String) -> String? {
        guard let match = rawValue.range(
            of: #"#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})"#,
            options: .regularExpression
        ) else {
            return nil
        }
        let value = String(rawValue[match])
        if value.count == 9 {
            return "#\(value.dropFirst().prefix(6))"
        }
        return value.uppercased()
    }

    private static func parseThemeDefinitionColor(_ rawValue: String?) -> NSColor? {
        guard let normalized = normalizedHexColorString(rawValue),
              let rgb = UInt(normalized.dropFirst(), radix: 16) else {
            return nil
        }
        return NSColor(
            calibratedRed: CGFloat((rgb >> 16) & 0xFF) / 255,
            green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255,
            alpha: 1
        )
    }

    private static func normalizedHexColorString(_ rawValue: String?) -> String? {
        guard let rawValue else {
            return nil
        }

        if let hashed = parseHexColorString(from: rawValue) {
            return hashed
        }

        let trimmed = rawValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let match = trimmed.range(
            of: #"^([0-9a-fA-F]{6}|[0-9a-fA-F]{8})$"#,
            options: .regularExpression
        ) else {
            return nil
        }

        let value = String(trimmed[match]).uppercased()
        if value.count == 8 {
            return "#\(value.prefix(6))"
        }
        return "#\(value)"
    }

    private static func unquoted<S: StringProtocol>(_ value: S) -> String {
        let string = String(value).trimmingCharacters(in: .whitespacesAndNewlines)
        guard string.count >= 2 else {
            return string
        }
        if (string.hasPrefix("\"") && string.hasSuffix("\"")) || (string.hasPrefix("'") && string.hasSuffix("'")) {
            return String(string.dropFirst().dropLast())
        }
        return string
    }

    private static func normalizeThemeName(_ value: String) -> String {
        value
            .lowercased()
            .replacingOccurrences(
                of: #"[^a-z0-9]+"#,
                with: "",
                options: .regularExpression
            )
    }

    static func terminalConfiguration() -> TerminalConfiguration {
        TerminalConfiguration { builder in
            builder.withBackgroundOpacity(1.0)
            builder.withBackgroundBlur(0)
            builder.withWindowPaddingX(0)
            builder.withWindowPaddingY(0)
        }
    }
}
