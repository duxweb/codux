import Foundation

struct AppTerminalGhosttyThemePreset: Sendable {
    let title: String
    let themeName: String
    let isLight: Bool
    let aliases: [String]
}

extension AppTerminalBackgroundPreset {
    static let legacyMappings: [String: Self] = [
        "auto": .automatic,
        "obsidian": .tokyoNightStorm,
        "graphite": .nord,
        "midnight": .tokyoNightNight,
        "forest": .everforestDarkHard,
        "paper": .tokyoNightDay,
        "sand": .gruvboxLight,
        "mist": .catppuccinLatte,
        "dawn": .rosePineDawn,
        "dracula": .tokyoNightStorm,
        "flexokiDark": .tokyoNightStorm,
        "flexokiLight": .tokyoNightDay,
        "githubDark": .nord,
        "rosePine": .rosePineMoon,
        "githubLight": .githubLightDefault,
        "draculaPlus": .poimandres,
        "materialOcean": .oxocarbon,
        "tokyoNight": .tokyoNightNight,
        "atomOneLight": .atomOneLight,
        "kanagawaWave": .kanagawaWave,
        "kanagawaLotus": .kanagawaLotus,
        "gruvboxDark": .gruvboxDark,
        "gruvboxLight": .gruvboxLight,
        "gruvboxMaterialDark": .gruvboxMaterialDark,
        "gruvboxMaterialLight": .gruvboxMaterialLight,
        "everforestDarkHard": .everforestDarkHard,
        "rosePineDawn": .rosePineDawn,
        "rosePineMoon": .rosePineMoon,
        "nightOwl": .nightOwl,
        "nordLight": .nordLight,
        "ayuMirage": .ayuMirage,
        "ayuLight": .ayuLight,
    ]

    var metadata: AppTerminalGhosttyThemePreset {
        Self.catalog[self] ?? Self.catalog[.tokyoNightStorm]!
    }

    static let catalog: [Self: AppTerminalGhosttyThemePreset] = [
        .tokyoNightStorm: .init(title: "TokyoNight Storm", themeName: "TokyoNight Storm", isLight: false, aliases: ["tokyonightstorm"]),
        .tokyoNightNight: .init(title: "TokyoNight Night", themeName: "TokyoNight Night", isLight: false, aliases: ["tokyonight", "tokyonightnight"]),
        .catppuccinMocha: .init(title: "Catppuccin Mocha", themeName: "Catppuccin Mocha", isLight: false, aliases: ["catppuccinmocha"]),
        .catppuccinMacchiato: .init(title: "Catppuccin Macchiato", themeName: "Catppuccin Macchiato", isLight: false, aliases: ["catppuccinmacchiato"]),
        .rosePineMoon: .init(title: "Rose Pine Moon", themeName: "Rose Pine Moon", isLight: false, aliases: ["rosepine", "rosepinemoon"]),
        .kanagawaWave: .init(title: "Kanagawa Wave", themeName: "Kanagawa Wave", isLight: false, aliases: ["kanagawawave"]),
        .kanagawaDragon: .init(title: "Kanagawa Dragon", themeName: "Kanagawa Dragon", isLight: false, aliases: ["kanagawadragon"]),
        .nightOwl: .init(title: "Night Owl", themeName: "Night Owl", isLight: false, aliases: ["nightowl"]),
        .gruvboxDark: .init(title: "Gruvbox Dark", themeName: "Gruvbox Dark", isLight: false, aliases: ["gruvboxdark"]),
        .gruvboxMaterialDark: .init(title: "Gruvbox Material Dark", themeName: "Gruvbox Material Dark", isLight: false, aliases: ["gruvboxmaterialdark"]),
        .everforestDarkHard: .init(title: "Everforest Dark Hard", themeName: "Everforest Dark Hard", isLight: false, aliases: ["everforestdarkhard"]),
        .nord: .init(title: "Nord", themeName: "Nord", isLight: false, aliases: ["nord"]),
        .oxocarbon: .init(title: "Oxocarbon", themeName: "Oxocarbon", isLight: false, aliases: ["oxocarbon"]),
        .poimandres: .init(title: "Poimandres", themeName: "Poimandres", isLight: false, aliases: ["poimandres"]),
        .ayuMirage: .init(title: "Ayu Mirage", themeName: "Ayu Mirage", isLight: false, aliases: ["ayumirage"]),
        .tokyoNightDay: .init(title: "TokyoNight Day", themeName: "TokyoNight Day", isLight: true, aliases: ["tokyonightday"]),
        .catppuccinLatte: .init(title: "Catppuccin Latte", themeName: "Catppuccin Latte", isLight: true, aliases: ["catppuccinlatte"]),
        .rosePineDawn: .init(title: "Rose Pine Dawn", themeName: "Rose Pine Dawn", isLight: true, aliases: ["rosepinedawn"]),
        .kanagawaLotus: .init(title: "Kanagawa Lotus", themeName: "Kanagawa Lotus", isLight: true, aliases: ["kanagawalotus"]),
        .gruvboxLight: .init(title: "Gruvbox Light", themeName: "Gruvbox Light", isLight: true, aliases: ["gruvboxlight"]),
        .gruvboxMaterialLight: .init(title: "Gruvbox Material Light", themeName: "Gruvbox Material Light", isLight: true, aliases: ["gruvboxmateriallight"]),
        .everforestLightMed: .init(title: "Everforest Light Med", themeName: "Everforest Light Med", isLight: true, aliases: ["everforestlightmed"]),
        .githubLightDefault: .init(title: "GitHub Light Default", themeName: "GitHub Light Default", isLight: true, aliases: ["githublight", "githublightdefault"]),
        .ayuLight: .init(title: "Ayu Light", themeName: "Ayu Light", isLight: true, aliases: ["ayulight"]),
        .atomOneLight: .init(title: "Atom One Light", themeName: "Atom One Light", isLight: true, aliases: ["atomonelight"]),
        .nordLight: .init(title: "Nord Light", themeName: "Nord Light", isLight: true, aliases: ["nordlight"]),
        .oneHalfLight: .init(title: "One Half Light", themeName: "One Half Light", isLight: true, aliases: ["onehalflight"]),
        .dawnfox: .init(title: "Dawnfox", themeName: "Dawnfox", isLight: true, aliases: ["dawnfox"]),
        .dayfox: .init(title: "Dayfox", themeName: "Dayfox", isLight: true, aliases: ["dayfox"]),
        .zenbonesLight: .init(title: "Zenbones Light", themeName: "Zenbones Light", isLight: true, aliases: ["zenboneslight"]),
    ]

    static func automaticFallbackPreset(prefersDarkAppearance: Bool) -> Self {
        prefersDarkAppearance ? .tokyoNightStorm : .tokyoNightDay
    }

    static func automaticMatch(forGhosttyThemeName name: String) -> Self? {
        let normalized = normalizeGhosttyThemeName(name)
        guard normalized.isEmpty == false else {
            return nil
        }
        return automaticThemeAliases[normalized]
    }

    private static let automaticThemeAliases: [String: Self] = {
        var aliases: [String: Self] = [:]

        func register(_ value: String, preset: Self) {
            aliases[normalizeGhosttyThemeName(value)] = preset
        }

        for (preset, metadata) in catalog {
            register(preset.rawValue, preset: preset)
            register(metadata.title, preset: preset)
            register(metadata.themeName, preset: preset)
            for alias in metadata.aliases {
                register(alias, preset: preset)
            }
        }

        return aliases
    }()

    private static func normalizeGhosttyThemeName(_ value: String) -> String {
        value
            .lowercased()
            .replacingOccurrences(
                of: #"[^a-z0-9]+"#,
                with: "",
                options: .regularExpression
            )
    }
}
