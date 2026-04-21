import Foundation
import Observation
import CryptoKit

@MainActor
@Observable
final class PetStore {
    private static let statsRefreshInterval: TimeInterval = 3600

    // Captures the XP state at the start of each calendar day so the
    // daily-pace limiter can apply a per-day rate to that day's token gains.
    struct DailyXPCapture: Codable, Equatable {
        /// Calendar day this capture belongs to (stored as start-of-day).
        let day: Date
        /// Rate-limited effective XP total at the moment this day started.
        let effectiveXPAtDayStart: Int
        /// Raw allTimeTokens at the moment this day started.
        let allTimeTokensAtDayStart: Int
        /// XP rate multiplier applied for tokens earned on this day (0.05 … 1.0).
        let rate: Double
    }

    struct Storage: Sendable {
        var fileURL: URL?
        var cryptoNamespace: String

        static let live = Self.makeLive(bundleIdentifier: Bundle.main.bundleIdentifier ?? "")

        static func makeLive(bundleIdentifier: String) -> Storage {
            let normalizedBundleID = bundleIdentifier.lowercased()
            let isDeveloperBuild = normalizedBundleID.hasSuffix(".dev") || normalizedBundleID.hasSuffix(".debug")
            let folderName = isDeveloperBuild ? "dmux-dev" : "dmux"
            let rootURL = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first?
                .appendingPathComponent(folderName)

            return Storage(
                fileURL: rootURL?.appendingPathComponent("pet-state.dat"),
                cryptoNamespace: isDeveloperBuild ? "dev" : "prod"
            )
        }

        static let inMemory = Storage(
            fileURL: nil,
            cryptoNamespace: "tests"
        )
    }

    static let shared = PetStore()

    private(set) var claimedAt: Date?
    private(set) var baselineAllTimeTokens: Int?
    private(set) var growthBaselineAllTimeTokens: Int?
    private(set) var species: PetSpecies = .voidcat
    private(set) var customName: String = ""
    private(set) var currentHatchTokens: Int = 0
    private(set) var currentExperienceTokens: Int = 0
    private(set) var currentStats: PetStats = .neutral
    private(set) var statsUpdatedDay: Date?
    private(set) var lockedEvoPath: PetEvoPath?
    private(set) var legacy: [PetLegacyRecord] = []
    /// Today's XP rate capture. Nil until the first refresh after hatch.
    private(set) var dailyXPCapture: DailyXPCapture?

    var isClaimed: Bool {
        baselineAllTimeTokens != nil
    }

    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared
    private let storage: Storage

    private init() {
        storage = .live
        load()
    }

    init(storage: Storage) {
        self.storage = storage
        load()
    }

    func claim(
        totalTokens: Int,
        option: PetClaimOption,
        customName: String,
        hiddenSpeciesChance: Double = 0.15
    ) {
        guard !isClaimed else {
            return
        }
        claimedAt = Date()
        baselineAllTimeTokens = max(0, totalTokens)
        growthBaselineAllTimeTokens = nil
        dailyXPCapture = nil
        species = option.resolveSpecies(hiddenSpeciesChance: hiddenSpeciesChance)
        self.customName = customName.trimmingCharacters(in: .whitespacesAndNewlines)
        currentHatchTokens = 0
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = Date()
        lockedEvoPath = nil
        save()
    }

    func rename(_ name: String) {
        guard isClaimed else {
            return
        }
        customName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        save()
    }

    func claimedTokens(currentAllTimeTokens: Int) -> Int {
        guard let baselineAllTimeTokens else {
            return 0
        }
        return max(0, currentAllTimeTokens - baselineAllTimeTokens)
    }

    func experienceTokens(currentAllTimeTokens: Int) -> Int {
        guard let growthBaselineAllTimeTokens else {
            return 0
        }
        return max(0, currentAllTimeTokens - growthBaselineAllTimeTokens)
    }

    func currentEvoPath() -> PetEvoPath {
        lockedEvoPath ?? previewEvoPath(for: currentStats)
    }

    func canInherit() -> Bool {
        isClaimed && PetProgressInfo.levelFromXP(currentExperienceTokens) >= PetProgressInfo.maxLevel
    }

    func inheritCurrentPet() {
        guard isClaimed, canInherit() else {
            return
        }

        let record = PetLegacyRecord(
            id: UUID(),
            species: species,
            customName: customName,
            evoPath: currentEvoPath(),
            totalXP: currentExperienceTokens,
            stats: currentStats,
            retiredAt: Date()
        )
        legacy.insert(record, at: 0)

        claimedAt = nil
        baselineAllTimeTokens = nil
        growthBaselineAllTimeTokens = nil
        dailyXPCapture = nil
        species = .voidcat
        customName = ""
        currentHatchTokens = 0
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        save()
    }

    func previewEvoPath(for stats: PetStats) -> PetEvoPath {
        switch species {
        case .voidcat:
            return stats.wisdom >= stats.night ? .pathA : .pathB
        case .rusthound:
            return stats.chaos >= stats.stamina ? .pathA : .pathB
        case .goose:
            return stats.empathy >= stats.chaos ? .pathA : .pathB
        case .chaossprite:
            return .pathA
        }
    }

    func refreshDerivedState(currentAllTimeTokens: Int, computedStats: PetStats, now: Date = .init()) {
        guard isClaimed else {
            return
        }

        var didChange = false
        let claimed = claimedTokens(currentAllTimeTokens: currentAllTimeTokens)
        let nextHatchTokens = min(claimed, PetProgressInfo.hatchThreshold)
        let nextXP: Int

        if nextHatchTokens < PetProgressInfo.hatchThreshold {
            // Still in egg phase — reset growth baseline and daily capture.
            if growthBaselineAllTimeTokens != nil {
                growthBaselineAllTimeTokens = nil
                dailyXPCapture = nil
                didChange = true
            }
            nextXP = 0
        } else {
            // Hatched — set growth baseline on first entry.
            if growthBaselineAllTimeTokens == nil {
                growthBaselineAllTimeTokens = max(0, currentAllTimeTokens)
                dailyXPCapture = nil
                didChange = true
            }

            // ── Daily pace limiter ──────────────────────────────────────────
            // Each calendar day we compute a rate multiplier based on how far
            // ahead of the 30-day-to-Lv100 pace the pet is:
            //   • At pace or behind  → rate = 1.0 (no penalty)
            //   • N levels ahead     → rate = max(5%, 1 - N × 20%)
            // The rate applies only to tokens earned *today*. At day rollover
            // we snapshot the current effective XP and re-evaluate the rate,
            // so recovering or slowing down is reflected the next morning.
            let calendar = Calendar.current
            let today = calendar.startOfDay(for: now)

            let capture: DailyXPCapture
            if let existing = dailyXPCapture, calendar.isDate(existing.day, inSameDayAs: today) {
                // Same day — keep the existing rate and snapshot.
                capture = existing
            } else {
                // New day (or first time after hatch): determine today's effective
                // XP total and recalculate the rate for today.
                let effectiveXPSoFar: Int
                if let prev = dailyXPCapture {
                    // Carry forward the rate-limited XP already accumulated.
                    let rawPrevDayGain = max(0, currentAllTimeTokens - prev.allTimeTokensAtDayStart)
                    effectiveXPSoFar = prev.effectiveXPAtDayStart + Int(Double(rawPrevDayGain) * prev.rate)
                } else {
                    // First capture after hatch — raw XP with no limiter yet.
                    effectiveXPSoFar = max(0, currentAllTimeTokens - (growthBaselineAllTimeTokens ?? currentAllTimeTokens))
                }

                let currentLevel = PetProgressInfo.levelFromXP(effectiveXPSoFar)
                let dayIndex: Int
                if let hatchDate = claimedAt {
                    dayIndex = PetProgressInfo.dayIndex(from: hatchDate, to: now)
                } else {
                    dayIndex = 0
                }
                let rate = PetProgressInfo.dailyXPRate(currentLevel: currentLevel, dayIndex: dayIndex)

                let newCapture = DailyXPCapture(
                    day: today,
                    effectiveXPAtDayStart: effectiveXPSoFar,
                    allTimeTokensAtDayStart: currentAllTimeTokens,
                    rate: rate
                )
                dailyXPCapture = newCapture
                capture = newCapture
                didChange = true

                debugLog.log(
                    "pet-pace",
                    "day-rollover dayIndex=\(dayIndex) level=\(currentLevel) "
                    + "expected=\(PetProgressInfo.expectedLevel(forDayIndex: dayIndex)) "
                    + "rate=\(String(format: "%.0f%%", rate * 100)) effectiveXP=\(effectiveXPSoFar)"
                )
            }

            // Apply the day's rate to today's raw token gain.
            let rawDailyGain = max(0, currentAllTimeTokens - capture.allTimeTokensAtDayStart)
            nextXP = capture.effectiveXPAtDayStart + Int(Double(rawDailyGain) * capture.rate)
        }

        if currentHatchTokens != nextHatchTokens {
            currentHatchTokens = nextHatchTokens
            didChange = true
        }
        if currentExperienceTokens != nextXP {
            currentExperienceTokens = nextXP
            didChange = true
        }

        if statsUpdatedDay == nil {
            currentStats = computedStats.maxValue > 0 ? computedStats : .neutral
            statsUpdatedDay = now
            didChange = true
        } else if currentStats == .neutral, computedStats.maxValue > 0 {
            currentStats = computedStats
            statsUpdatedDay = now
            didChange = true
        } else if let lastUpdatedAt = statsUpdatedDay,
                  now.timeIntervalSince(lastUpdatedAt) >= Self.statsRefreshInterval {
            currentStats = currentStats.applyingDamping(toward: computedStats)
            statsUpdatedDay = now
            didChange = true
        }

        if lockedEvoPath == nil,
           PetProgressInfo.levelFromXP(nextXP) >= PetProgressInfo.evoUnlockLevel {
            lockedEvoPath = previewEvoPath(for: computedStats)
            didChange = true
        }

        if didChange {
            save()
        }
    }

    func debugForceExperienceTokens(_ experienceTokens: Int, currentAllTimeTokens: Int, now: Date = .init()) {
        guard isClaimed else {
            return
        }
        let clampedXP = max(0, experienceTokens)
        baselineAllTimeTokens = max(0, currentAllTimeTokens - PetProgressInfo.hatchThreshold - clampedXP)
        growthBaselineAllTimeTokens = max(0, currentAllTimeTokens - clampedXP)
        currentHatchTokens = PetProgressInfo.hatchThreshold
        currentExperienceTokens = clampedXP
        if statsUpdatedDay == nil {
            statsUpdatedDay = now
        }
        if PetProgressInfo.levelFromXP(clampedXP) >= PetProgressInfo.evoUnlockLevel {
            lockedEvoPath = previewEvoPath(for: currentStats)
        } else {
            lockedEvoPath = nil
        }
        save()
    }

    func debugCompleteHatch(currentAllTimeTokens: Int, now: Date = .init()) {
        guard isClaimed else {
            return
        }
        baselineAllTimeTokens = max(0, currentAllTimeTokens - PetProgressInfo.hatchThreshold)
        growthBaselineAllTimeTokens = max(0, currentAllTimeTokens)
        currentHatchTokens = PetProgressInfo.hatchThreshold
        currentExperienceTokens = 0
        if statsUpdatedDay == nil {
            statsUpdatedDay = now
        }
        lockedEvoPath = nil
        save()
    }

    func debugSwitchSpecies(_ nextSpecies: PetSpecies, currentAllTimeTokens: Int, now: Date = .init()) {
        if !isClaimed {
            claimedAt = now
            baselineAllTimeTokens = max(0, currentAllTimeTokens)
            growthBaselineAllTimeTokens = nil
            currentHatchTokens = 0
            currentExperienceTokens = 0
            currentStats = .neutral
            statsUpdatedDay = now
        } else if currentHatchTokens >= PetProgressInfo.hatchThreshold {
            growthBaselineAllTimeTokens = max(0, currentAllTimeTokens - currentExperienceTokens)
        }
        species = nextSpecies
        customName = ""
        if PetProgressInfo.levelFromXP(currentExperienceTokens) >= PetProgressInfo.evoUnlockLevel {
            lockedEvoPath = previewEvoPath(for: currentStats)
        } else {
            lockedEvoPath = nil
        }
        save()
    }

    private func load() {
        let fileState = loadStateFile()

        guard let resolvedState = fileState else {
            return
        }
        claimedAt = resolvedState.claimedAt
        baselineAllTimeTokens = resolvedState.baselineAllTimeTokens
        growthBaselineAllTimeTokens = resolvedState.growthBaselineAllTimeTokens
        species = resolvedState.species ?? .voidcat
        customName = resolvedState.customName ?? ""
        let migratedXP = resolvedState.currentExperienceTokens ?? 0
        if let persistedHatchTokens = resolvedState.currentHatchTokens {
            currentHatchTokens = persistedHatchTokens
            currentExperienceTokens = migratedXP
        } else {
            currentHatchTokens = min(migratedXP, PetProgressInfo.hatchThreshold)
            currentExperienceTokens = max(0, migratedXP - PetProgressInfo.hatchThreshold)
        }
        if growthBaselineAllTimeTokens == nil,
           currentHatchTokens >= PetProgressInfo.hatchThreshold,
           let baselineAllTimeTokens {
            // Derive the best-effort growth baseline from persisted XP.
            // The old migration used `baselineAllTimeTokens + hatchThreshold`,
            // which under-estimates the baseline when the user had already
            // accumulated tokens before hatching — making XP appear much
            // larger than it actually is (e.g. reaching Lv57 the day after
            // hatching). Using the persisted XP to back-calculate gives the
            // correct baseline even after an app restart.
            let persisted = currentExperienceTokens
            if persisted > 0 {
                // Back-calculate: baseline = currentAllTimeTokens at hatch time
                // We don't know that value, but we do know:
                //   XP = allTimeAtHatch - growthBaseline → baseline = allTimeAtHatch - XP
                // Best proxy for allTimeAtHatch = baselineAllTimeTokens + hatchThreshold
                let estimatedAllTimeAtHatch = baselineAllTimeTokens + PetProgressInfo.hatchThreshold
                growthBaselineAllTimeTokens = max(0, estimatedAllTimeAtHatch - persisted)
            } else {
                growthBaselineAllTimeTokens = baselineAllTimeTokens + PetProgressInfo.hatchThreshold
            }
        }
        currentStats = resolvedState.currentStats ?? .neutral
        statsUpdatedDay = resolvedState.statsUpdatedDay
        lockedEvoPath = resolvedState.lockedEvoPath
        legacy = resolvedState.legacy ?? []
        dailyXPCapture = resolvedState.dailyXPCapture
    }

    private func save() {
        let state = PersistedPetState(
            claimedAt: claimedAt,
            baselineAllTimeTokens: baselineAllTimeTokens,
            growthBaselineAllTimeTokens: growthBaselineAllTimeTokens,
            species: species,
            customName: customName,
            currentHatchTokens: currentHatchTokens,
            currentExperienceTokens: currentExperienceTokens,
            currentStats: currentStats,
            statsUpdatedDay: statsUpdatedDay,
            lockedEvoPath: lockedEvoPath,
            legacy: legacy,
            dailyXPCapture: dailyXPCapture
        )
        saveStateFile(state)
    }

    private func loadStateFile() -> PersistedPetState? {
        guard let fileURL = stateFileURL(),
              fileManager.fileExists(atPath: fileURL.path),
              let rawData = try? Data(contentsOf: fileURL),
              let data = decryptedStateData(from: rawData),
              let state = try? JSONDecoder().decode(PersistedPetState.self, from: data) else {
            return nil
        }
        return state
    }

    private func saveStateFile(_ state: PersistedPetState) {
        guard let fileURL = stateFileURL() else {
            return
        }
        do {
            try fileManager.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.sortedKeys]
            let jsonData = try encoder.encode(state)
            let data = try encryptedStateData(from: jsonData)
            try data.write(to: fileURL, options: Data.WritingOptions.atomic)
        } catch {
            debugLog.log("pet-state", "save failed path=\(fileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func stateFileURL() -> URL? {
        storage.fileURL
    }

    private func cipherKey() -> SymmetricKey {
        let material = "dmux.pet.state.v2|\(storage.cryptoNamespace)|codux".data(using: .utf8) ?? Data()
        let digest = SHA256.hash(data: material)
        return SymmetricKey(data: Data(digest))
    }

    private func encryptedStateData(from data: Data) throws -> Data {
        let sealed = try AES.GCM.seal(data, using: cipherKey())
        guard let combined = sealed.combined else {
            throw CocoaError(.coderInvalidValue)
        }
        return combined
    }

    private func decryptedStateData(from data: Data) -> Data? {
        if let sealed = try? AES.GCM.SealedBox(combined: data),
           let opened = try? AES.GCM.open(sealed, using: cipherKey()) {
            return opened
        }
        return data
    }
}

private struct PersistedPetState: Codable, Equatable {
    var claimedAt: Date?
    var baselineAllTimeTokens: Int?
    var growthBaselineAllTimeTokens: Int?
    var species: PetSpecies?
    var customName: String?
    var currentHatchTokens: Int?
    var currentExperienceTokens: Int?
    var currentStats: PetStats?
    var statsUpdatedDay: Date?
    var lockedEvoPath: PetEvoPath?
    var legacy: [PetLegacyRecord]?
    var dailyXPCapture: PetStore.DailyXPCapture?
}
