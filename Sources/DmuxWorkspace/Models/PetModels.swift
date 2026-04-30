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

    var eggChoiceName: String {
        switch self {
        case .voidcat:      return petL("pet.species.voidcat.base", "Voidcat")
        case .rusthound:    return petL("pet.species.rusthound.base", "Ruff")
        case .goose:        return petL("pet.species.goose.base", "Goosey")
        case .chaossprite:  return petL("pet.species.chaossprite.egg", "Chaos Sprite")
        }
    }

    var englishName: String {
        switch self {
        case .voidcat:      return "VoidCat"
        case .rusthound:    return "RustHound"
        case .goose:        return "Goose"
        case .chaossprite:  return "ChaosSprite"
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
        }
    }
}

enum PetClaimOption: String, CaseIterable, Identifiable, Sendable {
    case voidcat
    case rusthound
    case goose
    case random

    private static let randomPool: [PetSpecies] = [.voidcat, .rusthound, .goose]

    var id: String { rawValue }

    var title: String {
        switch self {
        case .voidcat:
            return petL("pet.claim.voidcat.title", "Voidcat Egg")
        case .rusthound:
            return petL("pet.claim.rusthound.title", "Ruff Egg")
        case .goose:
            return petL("pet.claim.goose.title", "Goosey Egg")
        case .random:
            return petL("pet.claim.random.title", "Random Egg")
        }
    }

    var subtitle: String {
        switch self {
        case .voidcat:   return petL("pet.claim.voidcat.subtitle", "Wise and nocturnal")
        case .rusthound: return petL("pet.claim.rusthound.subtitle", "Fiery and stubborn")
        case .goose:     return petL("pet.claim.goose.subtitle", "Calm and healing")
        case .random:    return petL("pet.claim.random.subtitle", "Draw a surprise")
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
        case .random:
            let hiddenChance = min(max(hiddenSpeciesChance, 0), 0.50)
            let roll = min(max(randomValue ?? Double.random(in: 0..<1), 0), 0.999_999)
            if roll < hiddenChance {
                return .chaossprite
            }
            guard let randomValue else {
                return Self.randomPool.randomElement() ?? .voidcat
            }
            let clamped = min(max(randomValue, 0), 0.999_999)
            let normalized = (clamped - hiddenChance) / max(0.000_001, (1 - hiddenChance))
            let index = min(Int(normalized * Double(Self.randomPool.count)), Self.randomPool.count - 1)
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
