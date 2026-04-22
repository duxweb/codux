import AppKit
import Foundation

struct AppEffectiveTerminalAppearance {
    let backgroundColor: NSColor
    let foregroundColor: NSColor
    let cursorColor: NSColor
    let cursorTextColor: NSColor
    let selectionBackgroundColor: NSColor
    let selectionForegroundColor: NSColor
    let paletteHexStrings: [String]
    let isLight: Bool
    let minimumContrast: Double

    var mutedForegroundColor: NSColor {
        foregroundColor.blended(
            withFraction: isLight ? 0.42 : 0.36,
            of: backgroundColor
        ) ?? foregroundColor.withAlphaComponent(isLight ? 0.58 : 0.74)
    }

    var dividerColor: NSColor {
        isLight
            ? NSColor.black.withAlphaComponent(0.12)
            : NSColor.white.withAlphaComponent(0.14)
    }

    var inactiveDimOpacity: CGFloat {
        isLight ? 0.07 : 0.22
    }

    var inactiveDimColor: NSColor {
        NSColor.black.withAlphaComponent(inactiveDimOpacity)
    }

    var inactiveBackgroundColor: NSColor {
        backgroundColor.blended(withFraction: inactiveDimOpacity, of: .black) ?? backgroundColor
    }

    func windowGlassTintColor(forDarkAppearance isDarkAppearance: Bool) -> NSColor {
        let base = backgroundColor.usingColorSpace(.extendedSRGB) ?? backgroundColor
        let blended: NSColor

        if isDarkAppearance {
            let fraction: CGFloat = isLight ? 0.58 : 0.18
            blended = base.blended(withFraction: fraction, of: .black) ?? base
            return blended.withAlphaComponent(isLight ? 0.34 : 0.46)
        }

        let fraction: CGFloat = isLight ? 0.10 : 0.78
        blended = base.blended(withFraction: fraction, of: .white) ?? base
        return blended.withAlphaComponent(isLight ? 0.68 : 0.78)
    }
}

enum AppTerminalBackgroundPreset: String, Codable, CaseIterable, Identifiable {
    case automatic
    case tokyoNightStorm
    case tokyoNightNight
    case catppuccinMocha
    case catppuccinMacchiato
    case rosePineMoon
    case kanagawaWave
    case kanagawaDragon
    case nightOwl
    case gruvboxDark
    case gruvboxMaterialDark
    case everforestDarkHard
    case nord
    case oxocarbon
    case poimandres
    case ayuMirage
    case tokyoNightDay
    case catppuccinLatte
    case rosePineDawn
    case kanagawaLotus
    case gruvboxLight
    case gruvboxMaterialLight
    case everforestLightMed
    case githubLightDefault
    case ayuLight
    case atomOneLight
    case nordLight
    case oneHalfLight
    case dawnfox
    case dayfox
    case zenbonesLight

    var id: String { rawValue }

    init(from decoder: Decoder) throws {
        let container = try decoder.singleValueContainer()
        let rawValue = try container.decode(String.self)
        self = Self(rawValue: rawValue) ?? Self.legacyMappings[rawValue] ?? .automatic
    }

    func encode(to encoder: Encoder) throws {
        var container = encoder.singleValueContainer()
        try container.encode(rawValue)
    }

    var title: String {
        if isAutomatic {
            return "Auto"
        }
        return metadata.title
    }

    var isAutomatic: Bool {
        self == .automatic
    }

    var isLight: Bool {
        if isAutomatic {
            return false
        }
        return metadata.isLight
    }

    func effectiveAppearance(
        backgroundColorPreset: AppBackgroundColorPreset,
        automaticAppearance: AppEffectiveTerminalAppearance? = nil
    ) -> AppEffectiveTerminalAppearance {
        let baseAppearance: AppEffectiveTerminalAppearance
        if isAutomatic {
            baseAppearance = automaticAppearance ?? Self.automaticFallbackPreset(prefersDarkAppearance: true)
                .effectiveAppearance(backgroundColorPreset: .automatic)
        } else if let resolved = GhosttyEmbeddedConfig.bundledThemeAppearance(named: metadata.themeName) {
            baseAppearance = resolved
        } else {
            baseAppearance = Self.automaticFallbackPreset(prefersDarkAppearance: metadata.isLight == false)
                .effectiveAppearance(backgroundColorPreset: .automatic)
        }

        let backgroundColor = backgroundColorPreset.swatchColor ?? baseAppearance.backgroundColor
        let overrideActive = backgroundColorPreset.isAutomatic == false
        let appearanceIsLight = overrideActive ? backgroundColorPreset.isLight : baseAppearance.isLight
        let shouldUseContrastFallback = overrideActive && appearanceIsLight != baseAppearance.isLight

        let foregroundColor: NSColor = shouldUseContrastFallback
            ? (appearanceIsLight ? .dmuxHex(0x282726) : .dmuxHex(0xFFFCF0))
            : baseAppearance.foregroundColor
        let cursorColor: NSColor = shouldUseContrastFallback ? foregroundColor : baseAppearance.cursorColor
        let cursorTextColor: NSColor = shouldUseContrastFallback ? backgroundColor : baseAppearance.cursorTextColor
        let selectionBackgroundColor: NSColor = shouldUseContrastFallback
            ? (backgroundColor.blended(withFraction: appearanceIsLight ? 0.18 : 0.24, of: foregroundColor) ?? backgroundColor)
            : baseAppearance.selectionBackgroundColor
        let selectionForegroundColor: NSColor = shouldUseContrastFallback ? foregroundColor : baseAppearance.selectionForegroundColor

        return AppEffectiveTerminalAppearance(
            backgroundColor: backgroundColor,
            foregroundColor: foregroundColor,
            cursorColor: cursorColor,
            cursorTextColor: cursorTextColor,
            selectionBackgroundColor: selectionBackgroundColor,
            selectionForegroundColor: selectionForegroundColor,
            paletteHexStrings: baseAppearance.paletteHexStrings,
            isLight: appearanceIsLight,
            minimumContrast: baseAppearance.minimumContrast
        )
    }

    var backgroundColor: NSColor { effectiveAppearance(backgroundColorPreset: .automatic).backgroundColor }
    var foregroundColor: NSColor { effectiveAppearance(backgroundColorPreset: .automatic).foregroundColor }
    var mutedForegroundColor: NSColor { effectiveAppearance(backgroundColorPreset: .automatic).mutedForegroundColor }
    var dividerColor: NSColor { effectiveAppearance(backgroundColorPreset: .automatic).dividerColor }
    var inactiveDimOpacity: CGFloat { effectiveAppearance(backgroundColorPreset: .automatic).inactiveDimOpacity }
    var inactiveDimColor: NSColor { effectiveAppearance(backgroundColorPreset: .automatic).inactiveDimColor }
    var inactiveBackgroundColor: NSColor { effectiveAppearance(backgroundColorPreset: .automatic).inactiveBackgroundColor }

    func windowGlassTintColor(forDarkAppearance isDarkAppearance: Bool) -> NSColor {
        effectiveAppearance(backgroundColorPreset: .automatic).windowGlassTintColor(forDarkAppearance: isDarkAppearance)
    }
}
