import Foundation
import Observation
import CryptoKit

@MainActor
@Observable
final class PetStore {
    private static let statsRefreshInterval: TimeInterval = 3600

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
            if growthBaselineAllTimeTokens != nil {
                growthBaselineAllTimeTokens = nil
                didChange = true
            }
            nextXP = 0
        } else {
            if growthBaselineAllTimeTokens == nil {
                growthBaselineAllTimeTokens = max(0, currentAllTimeTokens)
                didChange = true
            }
            nextXP = experienceTokens(currentAllTimeTokens: currentAllTimeTokens)
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
            growthBaselineAllTimeTokens = baselineAllTimeTokens + PetProgressInfo.hatchThreshold
        }
        currentStats = resolvedState.currentStats ?? .neutral
        statsUpdatedDay = resolvedState.statsUpdatedDay
        lockedEvoPath = resolvedState.lockedEvoPath
        legacy = resolvedState.legacy ?? []
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
            legacy: legacy
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
}
