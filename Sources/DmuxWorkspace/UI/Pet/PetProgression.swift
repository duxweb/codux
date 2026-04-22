import SwiftUI

// MARK: - Pet Progression Model

struct PetProgressInfo {
    let level: Int
    let xpInLevel: Int
    let xpForLevel: Int
    let totalXP: Int
    let hatchTokens: Int
    let stage: PetStage

    static let hatchThreshold = 200_000_000
    static let xpBase = 7_500_000
    // Raised from 180_000 → 750_000 so the curve grows ~11x from Lv1 to Lv100
    // (was ~3.4x — almost flat, causing rapid level-ups at all stages).
    // Full run Lv1→100 now requires ~4.4B XP vs the old 1.6B.
    static let xpIncrement = 750_000
    static let maxLevel = 100
    static let postCapXP = xpBase + (maxLevel - 1) * xpIncrement
    static let infantRange = 1 ... 15
    static let childRange = 16 ... 35
    static let adultRange = 36 ... 60
    static let evoRange = 61 ... 85
    static let megaStartLevel = 86
    static let evoUnlockLevel = evoRange.lowerBound

    init(totalXP: Int, hatchTokens: Int, evoPath: PetEvoPath) {
        let growthXP = max(0, totalXP)
        let hatch = min(max(0, hatchTokens), Self.hatchThreshold)
        guard hatch >= Self.hatchThreshold else {
            self.level = 0
            self.xpInLevel = hatch
            self.xpForLevel = Self.hatchThreshold
            self.totalXP = 0
            self.hatchTokens = hatch
            self.stage = .egg
            return
        }

        let lvl = Self.levelFromXP(growthXP)
        let consumed = Self.totalXPRequired(toReach: lvl)
        self.level = lvl
        self.xpInLevel = max(0, growthXP - consumed)
        self.xpForLevel = Self.xpForLevel(lvl)
        self.totalXP = growthXP
        self.hatchTokens = hatch
        self.stage = PetStage.stage(for: lvl, evoPath: evoPath)
    }

    var xpProgress: Double {
        guard xpForLevel > 0 else { return 1.0 }
        return min(1.0, Double(xpInLevel) / Double(xpForLevel))
    }

    var hatchProgress: Double {
        min(1.0, Double(hatchTokens) / Double(Self.hatchThreshold))
    }

    var isHatching: Bool { stage == .egg }

    var hasUnlockedInheritance: Bool { level >= Self.maxLevel }

    static func xpForLevel(_ level: Int) -> Int {
        if level >= maxLevel {
            return postCapXP
        }
        return xpBase + max(0, level - 1) * xpIncrement
    }

    static func totalXPRequired(toReach level: Int) -> Int {
        guard level > 1 else {
            return 0
        }

        let cappedLevel = min(level, maxLevel)
        var total = 0
        for current in 1 ..< cappedLevel {
            total += xpBase + (current - 1) * xpIncrement
        }
        if level > maxLevel {
            total += (level - maxLevel) * postCapXP
        }
        return total
    }

    static func levelFromXP(_ totalXP: Int) -> Int {
        let total = max(0, totalXP)
        var level = 1
        var remaining = total

        while true {
            let needed = xpForLevel(level)
            if remaining < needed {
                break
            }
            remaining -= needed
            level += 1
        }

        return level
    }

    // MARK: - Daily pace limiter

    /// Number of calendar days from `hatchDate` to `now` (0-based: hatch day itself = 0).
    static func dayIndex(from hatchDate: Date, to now: Date = Date()) -> Int {
        let calendar = Calendar.current
        let start = calendar.startOfDay(for: hatchDate)
        let today = calendar.startOfDay(for: now)
        return max(0, calendar.dateComponents([.day], from: start, to: today).day ?? 0)
    }

    /// Expected level on day `dayIndex` based on a 30-day-to-level-100 pace.
    static func expectedLevel(forDayIndex dayIndex: Int) -> Int {
        let totalXPForCap = totalXPRequired(toReach: maxLevel + 1)
        let dailyBudget = Double(totalXPForCap) / 30.0
        let expectedXP = Int(Double(max(0, dayIndex)) * dailyBudget)
        return levelFromXP(expectedXP)
    }

    /// XP rate multiplier for today.
    /// Returns 1.0 if the pet is at or below the daily pace.
    /// Rate curve: on-pace = 100%, +1 level ahead = 50%, each further level
    /// -10 pp (min 5%). This gives an immediate meaningful brake the moment
    /// the pet outpaces the 30-day schedule, without a harsh cliff.
    static func dailyXPRate(currentLevel: Int, dayIndex: Int) -> Double {
        let expected = expectedLevel(forDayIndex: dayIndex)
        let levelsAhead = max(0, currentLevel - expected)
        guard levelsAhead > 0 else { return 1.0 }
        return max(0.05, 0.50 - Double(levelsAhead - 1) * 0.10)
    }
}

enum PetEvoPath: String, Codable {
    case pathA, pathB
}

enum PetStage: String {
    case egg
    case infant
    case child
    case adult
    case evoA = "evo_a"
    case evoB = "evo_b"
    case megaA = "mega_a"
    case megaB = "mega_b"

    static func stage(for level: Int, evoPath: PetEvoPath) -> PetStage {
        switch level {
        case PetProgressInfo.infantRange: return .infant
        case PetProgressInfo.childRange: return .child
        case PetProgressInfo.adultRange: return .adult
        case PetProgressInfo.evoRange: return evoPath == .pathA ? .evoA : .evoB
        default: return evoPath == .pathA ? .megaA : .megaB
        }
    }

    var displayName: String {
        switch self {
        case .egg: return petL("pet.stage.egg", "Hatching")
        case .infant: return petL("pet.stage.infant", "Infant")
        case .child: return petL("pet.stage.child", "Growing")
        case .adult: return petL("pet.stage.adult", "Adult")
        case .evoA, .evoB: return petL("pet.stage.awakened", "Awakened")
        case .megaA, .megaB: return petL("pet.stage.final_awakening", "Final Awakening")
        }
    }

    func speciesName(for species: PetSpecies, evoPath: PetEvoPath) -> String {
        switch species {
        case .voidcat:
            switch self {
            case .egg: return petL("pet.species.voidcat.egg", "花花蛋")
            case .infant: return petL("pet.species.voidcat.infant", "Huahua")
            case .child: return petL("pet.species.voidcat.child", "Shadow Cat")
            case .adult: return petL("pet.species.voidcat.adult", "Voidcat")
            case .evoA: return petL("pet.species.voidcat.evo_a", "Tomecat")
            case .evoB: return petL("pet.species.voidcat.evo_b", "Shadecat")
            case .megaA: return petL("pet.species.voidcat.mega_a", "Inkspirit")
            case .megaB: return petL("pet.species.voidcat.mega_b", "Nightspirit")
            }
        case .rusthound:
            switch self {
            case .egg: return petL("pet.species.rusthound.egg", "毛团蛋")
            case .infant: return petL("pet.species.rusthound.infant", "Furball")
            case .child: return petL("pet.species.rusthound.child", "Flop-Eared Pup")
            case .adult: return petL("pet.species.rusthound.adult", "Rusthound")
            case .evoA: return petL("pet.species.rusthound.evo_a", "Blazehound")
            case .evoB: return petL("pet.species.rusthound.evo_b", "Ironwolf")
            case .megaA: return petL("pet.species.rusthound.mega_a", "Sunflare")
            case .megaB: return petL("pet.species.rusthound.mega_b", "Bloodmoon")
            }
        case .goose:
            switch self {
            case .egg: return petL("pet.species.goose.egg", "啾啾蛋")
            case .infant: return petL("pet.species.goose.infant", "Chirpy")
            case .child: return petL("pet.species.goose.child", "Dozy")
            case .adult: return petL("pet.species.goose.adult", "Goosey")
            case .evoA: return petL("pet.species.goose.evo_a", "Dawnwing")
            case .evoB: return petL("pet.species.goose.evo_b", "Windwing")
            case .megaA: return petL("pet.species.goose.mega_a", "Wildfire")
            case .megaB: return petL("pet.species.goose.mega_b", "Tempest")
            }
        case .chaossprite:
            switch self {
            case .egg: return petL("pet.species.chaossprite.egg", "混沌蛋")
            case .infant: return petL("pet.species.chaossprite.infant", "Chaos")
            case .child: return petL("pet.species.chaossprite.child", "Mischief")
            case .adult: return petL("pet.species.chaossprite.adult", "Glimmer")
            case .evoA, .evoB: return petL("pet.species.chaossprite.evo", "Chaos Wisp")
            case .megaA, .megaB: return petL("pet.species.chaossprite.mega", "Prism Core")
            }
        }
    }

    var idleSpriteName: String { rawValue == "egg" ? "egg" : "\(rawValue)_idle" }

    var sleepSpriteName: String? {
        switch self {
        case .evoA, .evoB, .megaA, .megaB:
            return "\(rawValue)_sleep"
        default:
            return nil
        }
    }

    var idleFrameCount: Int {
        switch self {
        case .egg:
            return 1
        case .infant, .child, .adult, .megaA, .megaB:
            return 8
        case .evoA, .evoB:
            return 6
        }
    }

    var sleepFrameCount: Int { 8 }

    var nativeFrameSize: CGFloat {
        switch self {
        case .egg, .infant:
            return 256
        case .child, .adult:
            return 320
        case .evoA, .evoB:
            return 384
        case .megaA, .megaB:
            return 512
        }
    }

    var idleFrameDuration: TimeInterval {
        switch self {
        case .evoA, .evoB:
            return 0.600
        default:
            return 0.625
        }
    }

    var accentColor: Color {
        switch self {
        case .egg:
            return Color(hex: 0x888888)
        case .infant:
            return Color(hex: 0xC98663)
        case .child:
            return Color(hex: 0xC8D1E3)
        case .adult:
            return Color(hex: 0xE8AA34)
        case .evoA:
            return Color(hex: 0x2A80FF)
        case .evoB:
            return Color(hex: 0x9040FF)
        case .megaA:
            return Color(hex: 0xE0C040)
        case .megaB:
            return Color(hex: 0x6020CC)
        }
    }
}

extension PetStage {
    func resolvedIdentity(for species: PetSpecies, evoPath: PetEvoPath, customName: String) -> PetResolvedIdentity {
        let speciesName = speciesName(for: species, evoPath: evoPath)
        let trimmedName = customName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedName.isEmpty else {
            return PetResolvedIdentity(title: speciesName, subtitle: nil)
        }
        return PetResolvedIdentity(title: trimmedName, subtitle: speciesName)
    }
}
