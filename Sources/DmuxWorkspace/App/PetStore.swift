import CryptoKit
import Foundation
import Observation

@MainActor
@Observable
final class PetStore {
    private static let stateVersion = 7
    private static let statsModelVersion = 3
    private static let statsRefreshInterval: TimeInterval = 3600
    static let realtimeSessionRetentionInterval: TimeInterval = 7 * 86_400
    private static let ledgerLogCategory = "pet-ledger"

    struct Storage: Sendable {
        var fileURL: URL?
        var cryptoNamespace: String

        static let live = Self.makeLive(bundleIdentifier: Bundle.main.bundleIdentifier ?? "")

        static func makeLive(bundleIdentifier: String) -> Storage {
            let normalizedBundleIdentifier = bundleIdentifier.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
            let isDeveloperVariant = normalizedBundleIdentifier.hasSuffix(".dev")
                || normalizedBundleIdentifier.hasSuffix(".debug")
            let rootURL = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first?
                .appendingPathComponent(isDeveloperVariant ? "dmux-dev" : "dmux", isDirectory: true)
            let cryptoNamespace = isDeveloperVariant ? "dev" : "prod"

            return Storage(
                fileURL: rootURL?.appendingPathComponent("pet-state.dat"),
                cryptoNamespace: cryptoNamespace
            )
        }

        static let inMemory = Storage(
            fileURL: nil,
            cryptoNamespace: "tests"
        )
    }

    static let shared = PetStore()

    private(set) var claimedAt: Date?
    private(set) var species: PetSpecies = .voidcat
    private(set) var customName: String = ""
    private(set) var currentHatchTokens: Int = 0
    private(set) var currentExperienceTokens: Int = 0
    private(set) var currentStats: PetStats = .neutral
    private(set) var statsUpdatedDay: Date?
    private(set) var lockedEvoPath: PetEvoPath?
    private(set) var legacy: [PetLegacyRecord] = []

    private var realtimeSessionTotals: [String: Int] = [:]
    private var realtimeSessionObservedAt: [String: Date] = [:]
    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared
    private let storage: Storage

    var isClaimed: Bool {
        claimedAt != nil
    }

    private init() {
        storage = .live
        load()
    }

    init(storage: Storage) {
        self.storage = storage
        load()
    }

    func claim(
        option: PetClaimOption,
        customName: String,
        realtimeSessionTotals: [String: Int] = [:],
        hiddenSpeciesChance: Double = 0.15
    ) {
        guard !isClaimed else {
            return
        }
        let claimTime = Date()
        claimedAt = claimTime
        species = option.resolveSpecies(hiddenSpeciesChance: hiddenSpeciesChance)
        self.customName = customName.trimmingCharacters(in: .whitespacesAndNewlines)
        currentHatchTokens = 0
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        self.realtimeSessionTotals = sanitizeRealtimeSessionTotals(realtimeSessionTotals)
        realtimeSessionObservedAt = Dictionary(
            uniqueKeysWithValues: self.realtimeSessionTotals.keys.map { ($0, claimTime) }
        )
        save()
    }

    func rename(_ name: String) {
        guard isClaimed else {
            return
        }
        customName = name.trimmingCharacters(in: .whitespacesAndNewlines)
        save()
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
        species = .voidcat
        customName = ""
        currentHatchTokens = 0
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        realtimeSessionTotals = [:]
        realtimeSessionObservedAt = [:]
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

    func shouldRefreshStats(now: Date = .init()) -> Bool {
        guard isClaimed else {
            return false
        }
        guard let statsUpdatedDay else {
            return true
        }
        if currentStats == .neutral {
            return true
        }
        return now.timeIntervalSince(statsUpdatedDay) >= Self.statsRefreshInterval
    }

    func refreshDerivedState(
        realtimeSessionTotals nextRealtimeSessionTotals: [String: Int],
        computedStats: PetStats?,
        now: Date = .init()
    ) {
        guard isClaimed else {
            return
        }

        var didChange = false
        let sanitizedTotals = sanitizeRealtimeSessionTotals(nextRealtimeSessionTotals)

        if realtimeSessionTotals.isEmpty,
           (currentHatchTokens > 0 || currentExperienceTokens > 0),
           !sanitizedTotals.isEmpty {
            realtimeSessionTotals = sanitizedTotals
            for key in sanitizedTotals.keys {
                realtimeSessionObservedAt[key] = now
            }
            didChange = true
            debugLog.log(
                Self.ledgerLogCategory,
                "bootstrap-watermarks sessions=\(sanitizedTotals.count) hatch=\(currentHatchTokens) xp=\(currentExperienceTokens)"
            )
        }

        var deltaTokens = 0
        for (sessionKey, sessionTotal) in sanitizedTotals {
            let previousTotal = realtimeSessionTotals[sessionKey] ?? 0
            guard sessionTotal > previousTotal else {
                realtimeSessionObservedAt[sessionKey] = now
                continue
            }
            deltaTokens += sessionTotal - previousTotal
            realtimeSessionTotals[sessionKey] = sessionTotal
            realtimeSessionObservedAt[sessionKey] = now
            didChange = true
        }

        pruneStaleRealtimeSessions(activeKeys: Set(sanitizedTotals.keys), now: now, didChange: &didChange)

        if deltaTokens > 0 {
            let hatchRemaining = max(0, PetProgressInfo.hatchThreshold - currentHatchTokens)
            let hatchDelta = min(hatchRemaining, deltaTokens)
            let experienceDelta = max(0, deltaTokens - hatchDelta)
            currentHatchTokens += hatchDelta
            currentExperienceTokens += experienceDelta
            didChange = true
            debugLog.log(
                Self.ledgerLogCategory,
                "apply-delta delta=\(deltaTokens) hatchDelta=\(hatchDelta) xpDelta=\(experienceDelta) hatch=\(currentHatchTokens) xp=\(currentExperienceTokens)"
            )
        }

        if let computedStats {
            if statsUpdatedDay == nil {
                currentStats = computedStats.maxValue > 0 ? computedStats : .neutral
                statsUpdatedDay = now
                didChange = true
            } else if currentStats == .neutral, computedStats.maxValue > 0 {
                currentStats = computedStats
                statsUpdatedDay = now
                didChange = true
            } else if let lastUpdatedAt = statsUpdatedDay,
                      now.timeIntervalSince(lastUpdatedAt) >= Self.statsRefreshInterval,
                      currentStats != computedStats {
                currentStats = currentStats.applyingDamping(toward: computedStats)
                statsUpdatedDay = now
                didChange = true
            }
        }

        if lockedEvoPath == nil,
           currentHatchTokens >= PetProgressInfo.hatchThreshold,
           PetProgressInfo.levelFromXP(currentExperienceTokens) >= PetProgressInfo.evoUnlockLevel {
            lockedEvoPath = previewEvoPath(for: currentStats)
            didChange = true
        }

        if didChange {
            save()
        }
    }

    func debugForceExperienceTokens(_ experienceTokens: Int, now: Date = .init()) {
        guard isClaimed else {
            return
        }
        currentHatchTokens = PetProgressInfo.hatchThreshold
        currentExperienceTokens = max(0, experienceTokens)
        realtimeSessionTotals = [:]
        realtimeSessionObservedAt = [:]
        if statsUpdatedDay == nil {
            statsUpdatedDay = now
        }
        if PetProgressInfo.levelFromXP(currentExperienceTokens) >= PetProgressInfo.evoUnlockLevel {
            lockedEvoPath = previewEvoPath(for: currentStats)
        } else {
            lockedEvoPath = nil
        }
        save()
    }

    func debugCompleteHatch(now: Date = .init()) {
        guard isClaimed else {
            return
        }
        currentHatchTokens = PetProgressInfo.hatchThreshold
        currentExperienceTokens = 0
        realtimeSessionTotals = [:]
        realtimeSessionObservedAt = [:]
        if statsUpdatedDay == nil {
            statsUpdatedDay = now
        }
        lockedEvoPath = nil
        save()
    }

    func debugSwitchSpecies(_ nextSpecies: PetSpecies, now: Date = .init()) {
        if !isClaimed {
            claimedAt = now
            currentHatchTokens = 0
            currentExperienceTokens = 0
            currentStats = .neutral
            statsUpdatedDay = nil
            realtimeSessionTotals = [:]
            realtimeSessionObservedAt = [:]
        }
        species = nextSpecies
        customName = ""
        if currentHatchTokens >= PetProgressInfo.hatchThreshold,
           PetProgressInfo.levelFromXP(currentExperienceTokens) >= PetProgressInfo.evoUnlockLevel {
            lockedEvoPath = previewEvoPath(for: currentStats)
        } else {
            lockedEvoPath = nil
        }
        save()
    }

    private func sanitizeRealtimeSessionTotals(_ totals: [String: Int]) -> [String: Int] {
        totals.reduce(into: [:]) { partial, item in
            let key = item.key.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !key.isEmpty else {
                return
            }
            partial[key] = max(partial[key] ?? 0, max(0, item.value))
        }
    }

    private func load() {
        guard let resolvedState = loadStateFile() else {
            return
        }

        claimedAt = resolvedState.claimedAt
        species = resolvedState.species ?? .voidcat
        customName = resolvedState.customName ?? ""
        currentStats = resolvedState.currentStats ?? .neutral
        statsUpdatedDay = resolvedState.statsUpdatedDay
        lockedEvoPath = resolvedState.lockedEvoPath
        legacy = resolvedState.legacy ?? []

        if resolvedState.stateVersion == Self.stateVersion {
            applyLedgerState(from: resolvedState)
            if resolvedState.statsModelVersion != Self.statsModelVersion {
                currentStats = .neutral
                statsUpdatedDay = nil
                debugLog.log(
                    Self.ledgerLogCategory,
                    "invalidate-stats-cache from=\(resolvedState.statsModelVersion ?? 0) to=\(Self.statsModelVersion)"
                )
                save()
            }
            return
        }

        if shouldPreserveLedgerState(for: resolvedState) {
            applyLedgerState(from: resolvedState)
            claimedAt = Date()
            currentStats = .neutral
            statsUpdatedDay = nil
            lockedEvoPath = nil
            debugLog.log(
                Self.ledgerLogCategory,
                "migrate-reset-stats version=\(resolvedState.stateVersion ?? 0) preservedLedger=true resetClaimedAt=true hatch=\(currentHatchTokens) xp=\(currentExperienceTokens)"
            )
            save()
            return
        }

        let legacyWasHatched = legacyStateWasHatched(resolvedState)
        claimedAt = Date()
        currentHatchTokens = legacyWasHatched ? PetProgressInfo.hatchThreshold : 0
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        realtimeSessionTotals = [:]
        realtimeSessionObservedAt = [:]
        debugLog.log(
            Self.ledgerLogCategory,
            "migrate-reset version=\(resolvedState.stateVersion ?? 0) hatched=\(legacyWasHatched) resetClaimedAt=true hatch=\(currentHatchTokens) xp=\(currentExperienceTokens)"
        )
        save()
    }

    private func applyLedgerState(from state: PersistedPetState) {
        currentHatchTokens = max(0, state.currentHatchTokens ?? 0)
        currentExperienceTokens = max(0, state.currentExperienceTokens ?? 0)
        currentStats = state.currentStats ?? .neutral
        statsUpdatedDay = state.statsUpdatedDay
        lockedEvoPath = state.lockedEvoPath
        realtimeSessionTotals = sanitizeRealtimeSessionTotals(state.realtimeSessionTotals ?? [:])
        let persistedObservedAt = state.realtimeSessionObservedAt ?? [:]
        realtimeSessionObservedAt = realtimeSessionTotals.reduce(into: [:]) { partial, item in
            partial[item.key] = persistedObservedAt[item.key] ?? Date()
        }
    }

    private func shouldPreserveLedgerState(for state: PersistedPetState) -> Bool {
        guard state.stateVersion == 4 else {
            return false
        }
        let realtimeTotals = sanitizeRealtimeSessionTotals(state.realtimeSessionTotals ?? [:])
        return !realtimeTotals.isEmpty
    }

    private func legacyStateWasHatched(_ state: PersistedPetState) -> Bool {
        if let hatchTokens = state.currentHatchTokens {
            return hatchTokens >= PetProgressInfo.hatchThreshold
        }
        let legacyXP = max(0, state.currentExperienceTokens ?? 0)
        if legacyXP >= PetProgressInfo.hatchThreshold {
            return true
        }
        return (state.growthBaselineAllTimeTokens ?? 0) > 0
    }

    private func save() {
        let state = PersistedPetState(
            stateVersion: Self.stateVersion,
            statsModelVersion: Self.statsModelVersion,
            claimedAt: claimedAt,
            species: species,
            customName: customName,
            currentHatchTokens: currentHatchTokens,
            currentExperienceTokens: currentExperienceTokens,
            currentStats: currentStats,
            statsUpdatedDay: statsUpdatedDay,
            lockedEvoPath: lockedEvoPath,
            legacy: legacy,
            realtimeSessionTotals: realtimeSessionTotals,
            realtimeSessionObservedAt: realtimeSessionObservedAt
        )
        saveStateFile(state)
    }

    private func pruneStaleRealtimeSessions(activeKeys: Set<String>, now: Date, didChange: inout Bool) {
        guard !realtimeSessionTotals.isEmpty else {
            realtimeSessionObservedAt = [:]
            return
        }

        var staleKeys: [String] = []
        for key in realtimeSessionTotals.keys where !activeKeys.contains(key) {
            let observedAt = realtimeSessionObservedAt[key] ?? now
            guard now.timeIntervalSince(observedAt) >= Self.realtimeSessionRetentionInterval else {
                continue
            }
            staleKeys.append(key)
        }

        guard !staleKeys.isEmpty else {
            return
        }

        for key in staleKeys {
            realtimeSessionTotals.removeValue(forKey: key)
            realtimeSessionObservedAt.removeValue(forKey: key)
        }
        didChange = true
        debugLog.log(Self.ledgerLogCategory, "prune-watermarks removed=\(staleKeys.count)")
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
            try data.write(to: fileURL, options: .atomic)
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
    var stateVersion: Int?
    var statsModelVersion: Int?
    var claimedAt: Date?
    var species: PetSpecies?
    var customName: String?
    var currentHatchTokens: Int?
    var currentExperienceTokens: Int?
    var currentStats: PetStats?
    var statsUpdatedDay: Date?
    var lockedEvoPath: PetEvoPath?
    var legacy: [PetLegacyRecord]?
    var realtimeSessionTotals: [String: Int]?
    var realtimeSessionObservedAt: [String: Date]?

    // Legacy fields kept for one-way migration.
    var progressionVersion: Int?
    var dailyPaceVersion: Int?
    var baselineAllTimeTokens: Int?
    var growthBaselineAllTimeTokens: Int?
}
