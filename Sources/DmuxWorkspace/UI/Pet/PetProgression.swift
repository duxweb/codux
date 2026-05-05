import SwiftUI

// MARK: - Pet Progression Model

struct PetProgressInfo {
    let level: Int
    let xpInLevel: Int
    let xpForLevel: Int
    let totalXP: Int
    let stage: PetStage

    static let dailyTargetXP = 40_000_000
    static let maxLevel = 100
    static let targetXPToReachLevel100 = dailyTargetXP * 30
    static let minXPPerLevel = 2_000_000
    static let maxXPPerLevel = 22_000_000
    static let postCapXP = levelXPRequirements.last ?? maxXPPerLevel

    private static let levelXPRequirements = buildLevelXPRequirements()
    private static let levelXPPrefixSums = buildLevelXPPrefixSums()

    init(totalXP: Int) {
        let growthXP = max(0, totalXP)
        let lvl = Self.levelFromXP(growthXP)
        let consumed = Self.totalXPRequired(toReach: lvl)
        self.level = lvl
        self.xpInLevel = max(0, growthXP - consumed)
        self.xpForLevel = Self.xpForLevel(lvl)
        self.totalXP = growthXP
        self.stage = .companion
    }

    var xpProgress: Double {
        guard xpForLevel > 0 else { return 1.0 }
        return min(1.0, Double(xpInLevel) / Double(xpForLevel))
    }

    var hasUnlockedArchive: Bool { level >= Self.maxLevel }

    static func xpForLevel(_ level: Int) -> Int {
        if level >= maxLevel {
            return postCapXP
        }
        let index = max(0, level - 1)
        guard index < levelXPRequirements.count else {
            return postCapXP
        }
        return levelXPRequirements[index]
    }

    static func totalXPRequired(toReach level: Int) -> Int {
        guard level > 1 else {
            return 0
        }

        let cappedLevel = min(level, maxLevel)
        let cappedIndex = max(0, cappedLevel - 2)
        var total = levelXPPrefixSums[cappedIndex]
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

    private static func buildLevelXPRequirements() -> [Int] {
        let count = maxLevel - 1
        guard count > 0 else { return [] }

        let weights = (0 ..< count).map { index -> Double in
            let progress = count == 1 ? 0 : Double(index) / Double(count - 1)
            return Double(minXPPerLevel) + Double(maxXPPerLevel - minXPPerLevel) * progress
        }
        let weightTotal = weights.reduce(0, +)
        guard weightTotal > 0 else {
            return Array(repeating: dailyTargetXP, count: count)
        }

        let scaled = weights.map { $0 / weightTotal * Double(targetXPToReachLevel100) }
        var requirements = scaled.map { Int($0.rounded(.down)) }
        let remainder = targetXPToReachLevel100 - requirements.reduce(0, +)
        if remainder > 0 {
            for offset in 0 ..< min(remainder, count) {
                let centeredIndex = Int(
                    ((Double(offset) + 0.5) * Double(count) / Double(remainder)).rounded(.down)
                )
                requirements[min(count - 1, centeredIndex)] += 1
            }
        }
        return requirements
    }

    private static func buildLevelXPPrefixSums() -> [Int] {
        var running = 0
        return levelXPRequirements.map {
            running += $0
            return running
        }
    }
}

// Persisted for old state files only. It no longer changes pet appearance.
enum PetEvoPath: String, Codable {
    case pathA, pathB
}

enum PetStage: String {
    case companion

    var displayName: String {
        petL("pet.stage.companion", "Companion")
    }

    func speciesName(for species: PetSpecies, evoPath: PetEvoPath = .pathA) -> String {
        species.displayName
    }

    var accentColor: Color { Color(hex: 0x2F8FFF) }
}

extension PetSpecies {
    var petAccentColor: Color {
        switch self {
        case .voidcat:      return Color(hex: 0x6A5CFF)
        case .rusthound:    return Color(hex: 0xFF8A3D)
        case .goose:        return Color(hex: 0x3E86F6)
        case .chaossprite:  return Color(hex: 0xFF4FA3)
        case .code:         return Color(hex: 0x2F8FFF)
        case .sheep:        return Color(hex: 0xF28FB8)
        case .ox:           return Color(hex: 0xF3B43F)
        case .dragon:       return Color(hex: 0xE04435)
        case .phoenix:      return Color(hex: 0xFF7A22)
        case .dolphin:      return Color(hex: 0x1E9BFF)
        case .penguin:      return Color(hex: 0x5C6D85)
        case .panda:        return Color(hex: 0x6A6F78)
        }
    }
}

extension PetStage {
    func resolvedIdentity(for species: PetSpecies, evoPath: PetEvoPath = .pathA, customName: String) -> PetResolvedIdentity {
        let speciesName = speciesName(for: species, evoPath: evoPath)
        let trimmedName = customName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedName.isEmpty else {
            return PetResolvedIdentity(title: speciesName, subtitle: nil)
        }
        return PetResolvedIdentity(title: trimmedName, subtitle: speciesName)
    }
}
