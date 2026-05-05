import Foundation

func petL(_ key: StaticString, _ defaultValue: String.LocalizationValue) -> String {
    String(localized: key, defaultValue: defaultValue, bundle: .module)
}

struct PetStats: Codable, Equatable, Sendable {
    let wisdom: Int
    let chaos: Int
    let night: Int
    let stamina: Int
    let empathy: Int

    static let neutral = PetStats(wisdom: 0, chaos: 0, night: 0, stamina: 0, empathy: 0)
    static let traitDisplayMaxValue = 330

    var maxValue: Int {
        max(wisdom, chaos, night, stamina, empathy)
    }

    var personaTag: String {
        let values: [(String, Int)] = [
            ("wisdom", wisdom),
            ("chaos", chaos),
            ("night", night),
            ("stamina", stamina),
            ("empathy", empathy),
        ].sorted { lhs, rhs in
            if lhs.1 == rhs.1 {
                return lhs.0 < rhs.0
            }
            return lhs.1 > rhs.1
        }

        let strongest = values[0]
        let second = values.dropFirst().first?.1 ?? 0
        let dominantGap = strongest.1 - second
        let dominanceRatio = second > 0 ? Double(strongest.1) / Double(second) : Double(strongest.1)

        if strongest.1 == 0 {
            return petL("pet.persona.observer", "Null Signal")
        }
        if dominantGap < max(18, strongest.1 / 8) || dominanceRatio < 1.12 {
            return petL("pet.persona.balanced", "Zero Protocol")
        }
        if strongest.0 == "wisdom", wisdom >= max(chaos + 60, Int(Double(second) * 1.18)) {
            return night >= Int(Double(wisdom) * 0.72)
                ? petL("pet.persona.midnight_thinker", "Darknet Oracle")
                : petL("pet.persona.philosopher", "Core Architect")
        }
        if strongest.0 == "chaos", stamina >= Int(Double(chaos) * 0.7) {
            return petL("pet.persona.mad_scientist", "Rogue Compiler")
        }
        if strongest.0 == "night", empathy >= Int(Double(night) * 0.55) {
            return petL("pet.persona.night_companion", "Neon Specter")
        }
        if strongest.0 == "stamina", empathy >= Int(Double(stamina) * 0.6) {
            return petL("pet.persona.debug_comrade", "Neural Patch")
        }
        if strongest.0 == "night" {
            return petL("pet.persona.night_owl", "Shadow Crawler")
        }
        if strongest.0 == "chaos" {
            return dominantGap > 40
                ? petL("pet.persona.firebrand", "Overclock")
                : petL("pet.persona.action_seeker", "Full Throttle")
        }
        if strongest.0 == "stamina" {
            return dominantGap > 40
                ? petL("pet.persona.marathoner", "Iron Protocol")
                : petL("pet.persona.steady_type", "Steady Kernel")
        }
        if strongest.0 == "empathy" {
            return petL("pet.persona.debug_buddy", "Sync Node")
        }
        if strongest.0 == "wisdom" {
            return petL("pet.persona.wise_type", "Deep Cache")
        }
        return petL("pet.persona.observer", "Null Signal")
    }

    func applyingDamping(toward target: PetStats, factor: Double = 0.25) -> PetStats {
        func damp(_ current: Int, _ next: Int) -> Int {
            let delta = Double(next - current) * factor
            let step = Int(delta.rounded())
            if step == 0, current != next {
                return max(0, current + (next > current ? 1 : -1))
            }
            return max(0, current + step)
        }

        return PetStats(
            wisdom: damp(wisdom, target.wisdom),
            chaos: damp(chaos, target.chaos),
            night: damp(night, target.night),
            stamina: damp(stamina, target.stamina),
            empathy: damp(empathy, target.empathy)
        )
    }

    var widestCompactValueText: String {
        [wisdom, chaos, night, stamina, empathy]
            .map(petFormatCompactNumber)
            .max { lhs, rhs in lhs.count < rhs.count } ?? "0"
    }
}

enum PetSpecies: String, Codable, CaseIterable, Equatable, Sendable {
    case voidcat
    case rusthound
    case goose
    case chaossprite
    case code
    case sheep
    case ox
    case dragon
    case phoenix
    case dolphin
    case penguin
    case panda

    var displayName: String {
        switch self {
        case .voidcat:      return petL("pet.species.voidcat.base", "Mimi")
        case .rusthound:    return petL("pet.species.rusthound.base", "Ruff")
        case .goose:        return petL("pet.species.goose.base", "Goosey")
        case .chaossprite:  return petL("pet.species.chaossprite.base", "Chaos")
        case .code:         return petL("pet.species.code.base", "code")
        case .sheep:        return petL("pet.species.sheep.base", "BaaBaa")
        case .ox:           return petL("pet.species.ox.base", "MooMoo")
        case .dragon:       return petL("pet.species.dragon.base", "Drako")
        case .phoenix:      return petL("pet.species.phoenix.base", "Ember")
        case .dolphin:      return petL("pet.species.dolphin.base", "Splash")
        case .penguin:      return petL("pet.species.penguin.base", "Pingu")
        case .panda:        return petL("pet.species.panda.base", "Bamboo")
        }
    }

    var assetFolder: String {
        rawValue
    }

    var isImplemented: Bool {
        true
    }

    var placeholderSymbol: String {
        switch self {
        case .voidcat:      return "cat.fill"
        case .rusthound:    return "dog.fill"
        case .goose:        return "bird.fill"
        case .chaossprite:  return "sparkles"
        case .code:         return "terminal.fill"
        case .sheep:        return "cloud.fill"
        case .ox:           return "shield.fill"
        case .dragon:       return "flame.fill"
        case .phoenix:      return "flame.circle.fill"
        case .dolphin:      return "water.waves"
        case .penguin:      return "snowflake"
        case .panda:        return "circle.grid.cross.fill"
        }
    }
}

enum PetClaimOption: String, CaseIterable, Identifiable, Sendable {
    case voidcat
    case rusthound
    case goose
    case chaossprite
    case code
    case sheep
    case ox
    case dragon
    case phoenix
    case dolphin
    case penguin
    case panda
    case random

    private static let randomPool: [PetSpecies] = PetSpecies.allCases

    var id: String { rawValue }

    var title: String {
        switch self {
        case .voidcat:
            return PetSpecies.voidcat.displayName
        case .rusthound:
            return PetSpecies.rusthound.displayName
        case .goose:
            return PetSpecies.goose.displayName
        case .chaossprite:
            return PetSpecies.chaossprite.displayName
        case .code:
            return PetSpecies.code.displayName
        case .sheep:
            return PetSpecies.sheep.displayName
        case .ox:
            return PetSpecies.ox.displayName
        case .dragon:
            return PetSpecies.dragon.displayName
        case .phoenix:
            return PetSpecies.phoenix.displayName
        case .dolphin:
            return PetSpecies.dolphin.displayName
        case .penguin:
            return PetSpecies.penguin.displayName
        case .panda:
            return PetSpecies.panda.displayName
        case .random:
            return petL("pet.claim.random.title", "随机")
        }
    }

    var subtitle: String {
        switch self {
        case .voidcat:      return petL("pet.claim.voidcat.subtitle", "专注、安静、夜间陪伴")
        case .rusthound:    return petL("pet.claim.rusthound.subtitle", "热情、稳定、持续推进")
        case .goose:        return petL("pet.claim.goose.subtitle", "轻松、治愈、节奏稳定")
        case .chaossprite:  return petL("pet.claim.chaossprite.subtitle", "灵感、变化、隐藏能量")
        case .code:         return petL("pet.claim.code.subtitle", "编码、终端、任务执行")
        case .sheep:        return petL("pet.claim.sheep.subtitle", "Soft, patient, keeps moving")
        case .ox:           return petL("pet.claim.ox.subtitle", "Grounded, reliable, steady")
        case .dragon:       return petL("pet.claim.dragon.subtitle", "Bold, fiery, task guardian")
        case .phoenix:      return petL("pet.claim.phoenix.subtitle", "Restart, recover, shine again")
        case .dolphin:      return petL("pet.claim.dolphin.subtitle", "Nimble, fresh, quick swimmer")
        case .penguin:      return petL("pet.claim.penguin.subtitle", "Calm, focused, cool-headed")
        case .panda:        return petL("pet.claim.panda.subtitle", "Round, gentle, steady pace")
        case .random:       return petL("pet.claim.random.subtitle", "随机选择一个伙伴")
        }
    }

    var symbol: String {
        switch self {
        case .voidcat:
            return "cat.fill"
        case .rusthound:
            return "dog.fill"
        case .goose:
            return "bird.fill"
        case .chaossprite:
            return "sparkles"
        case .code:
            return "terminal.fill"
        case .sheep:
            return "cloud.fill"
        case .ox:
            return "shield.fill"
        case .dragon:
            return "flame.fill"
        case .phoenix:
            return "flame.circle.fill"
        case .dolphin:
            return "water.waves"
        case .penguin:
            return "snowflake"
        case .panda:
            return "circle.grid.cross.fill"
        case .random:
            return "sparkles"
        }
    }

    func resolveSpecies(
        hiddenSpeciesChance: Double = 0.15,
        randomValue: Double? = nil
    ) -> PetSpecies {
        switch self {
        case .voidcat:
            return .voidcat
        case .rusthound:
            return .rusthound
        case .goose:
            return .goose
        case .chaossprite:
            return .chaossprite
        case .code:
            return .code
        case .sheep:
            return .sheep
        case .ox:
            return .ox
        case .dragon:
            return .dragon
        case .phoenix:
            return .phoenix
        case .dolphin:
            return .dolphin
        case .penguin:
            return .penguin
        case .panda:
            return .panda
        case .random:
            guard let randomValue else {
                return Self.randomPool.randomElement() ?? .voidcat
            }
            let clamped = min(max(randomValue, 0), 0.999_999)
            let index = min(Int(clamped * Double(Self.randomPool.count)), Self.randomPool.count - 1)
            return Self.randomPool[index]
        }
    }

    var previewSpecies: PetSpecies? {
        switch self {
        case .voidcat:
            return .voidcat
        case .rusthound:
            return .rusthound
        case .goose:
            return .goose
        case .chaossprite:
            return .chaossprite
        case .code:
            return .code
        case .sheep:
            return .sheep
        case .ox:
            return .ox
        case .dragon:
            return .dragon
        case .phoenix:
            return .phoenix
        case .dolphin:
            return .dolphin
        case .penguin:
            return .penguin
        case .panda:
            return .panda
        case .random:
            return nil
        }
    }
}

struct PetLegacyRecord: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let species: PetSpecies
    let customName: String
    let evoPath: PetEvoPath
    let totalXP: Int
    let stats: PetStats
    let retiredAt: Date
}

struct PetResolvedIdentity: Equatable, Sendable {
    let title: String
    let subtitle: String?
}

extension PetLegacyRecord {
    func resolvedIdentity(for stage: PetStage) -> PetResolvedIdentity {
        stage.resolvedIdentity(for: species, evoPath: evoPath, customName: customName)
    }
}

func petFormatCompactNumber(_ value: Int) -> String {
    let absolute = abs(value)
    let sign = value < 0 ? "-" : ""

    func format(_ divisor: Double, suffix: String) -> String {
        let scaled = Double(absolute) / divisor
        let digits: String
        if scaled >= 100 {
            digits = String(format: "%.0f", scaled)
        } else if scaled >= 10 {
            digits = String(format: "%.1f", scaled)
        } else {
            digits = String(format: "%.2f", scaled)
        }
        let cleaned = digits.contains(".")
            ? digits
                .replacingOccurrences(of: #"0+$"#, with: "", options: .regularExpression)
                .replacingOccurrences(of: #"\.$"#, with: "", options: .regularExpression)
            : digits
        return "\(sign)\(cleaned)\(suffix)"
    }

    switch absolute {
    case 1_000_000_000...:
        return format(1_000_000_000, suffix: "B")
    case 1_000_000...:
        return format(1_000_000, suffix: "M")
    case 1_000...:
        return format(1_000, suffix: "K")
    default:
        return "\(value)"
    }
}
