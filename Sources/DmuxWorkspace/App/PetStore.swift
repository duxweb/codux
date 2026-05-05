import CryptoKit
import Foundation
import Observation

@MainActor
@Observable
final class PetStore {
    private static let stateVersion = 7
    private static let statsModelVersion = 3
    private static let statsRefreshInterval: TimeInterval = 3600
    private static let ledgerLogCategory = "pet-ledger"

    struct Storage: Sendable {
        var fileURL: URL?
        var cryptoNamespace: String
        var legacyFileURLs: [URL]
        var legacyCryptoNamespaces: [String]

        static let live = Self.makeLive(bundle: .main)

        static func makeLive(bundle: Bundle = .main) -> Storage {
            let rootURL = AppRuntimePaths.appSupportRootURL(bundle: bundle)
            let cryptoNamespace = AppRuntimePaths.runtimeOwnerID(bundle: bundle)
            let legacyLayout = legacyLayout(bundleIdentifier: bundle.bundleIdentifier ?? "")

            return Storage(
                fileURL: rootURL?.appendingPathComponent("pet-state.dat"),
                cryptoNamespace: cryptoNamespace,
                legacyFileURLs: legacyLayout.fileURLs,
                legacyCryptoNamespaces: legacyLayout.cryptoNamespaces
            )
        }

        static func makeLive(bundleIdentifier: String, appDisplayName: String? = nil) -> Storage {
            let resolvedDisplayName = appDisplayName ?? inferredDisplayName(bundleIdentifier: bundleIdentifier)
            let rootURL = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first?
                .appendingPathComponent(
                    AppRuntimePaths.appSupportFolderName(appDisplayName: resolvedDisplayName),
                    isDirectory: true
                )
            let cryptoNamespace = AppRuntimePaths.runtimeOwnerID(
                appDisplayName: resolvedDisplayName,
                bundleIdentifier: bundleIdentifier
            )
            let legacyLayout = legacyLayout(bundleIdentifier: bundleIdentifier)

            return Storage(
                fileURL: rootURL?.appendingPathComponent("pet-state.dat"),
                cryptoNamespace: cryptoNamespace,
                legacyFileURLs: legacyLayout.fileURLs,
                legacyCryptoNamespaces: legacyLayout.cryptoNamespaces
            )
        }

        static let inMemory = Storage(
            fileURL: nil,
            cryptoNamespace: "tests",
            legacyFileURLs: [],
            legacyCryptoNamespaces: []
        )

        private static func inferredDisplayName(bundleIdentifier: String) -> String {
            let normalizedBundleIdentifier = bundleIdentifier
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
            if normalizedBundleIdentifier.hasSuffix(".dev") {
                return "Codux-dev"
            }
            if normalizedBundleIdentifier.hasSuffix(".debug") {
                return "Codux-debug"
            }
            return "Codux"
        }

        private static func legacyLayout(bundleIdentifier: String) -> (fileURLs: [URL], cryptoNamespaces: [String]) {
            guard let appSupportURL = FileManager.default
                .urls(for: .applicationSupportDirectory, in: .userDomainMask)
                .first else {
                return ([], [])
            }

            let normalizedBundleIdentifier = bundleIdentifier
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
            let isDeveloperVariant = normalizedBundleIdentifier.hasSuffix(".dev")
                || normalizedBundleIdentifier.hasSuffix(".debug")

            let legacyRootName = isDeveloperVariant ? "dmux-dev" : "dmux"
            let legacyNamespace = isDeveloperVariant ? "dev" : "prod"

            return (
                [appSupportURL.appendingPathComponent(legacyRootName, isDirectory: true).appendingPathComponent("pet-state.dat")],
                [legacyNamespace]
            )
        }
    }

    static let shared = PetStore()

    private(set) var claimedAt: Date?
    private(set) var species: PetSpecies = .voidcat
    private(set) var customName: String = ""
    private(set) var currentExperienceTokens: Int = 0
    private(set) var currentStats: PetStats = .neutral
    private(set) var statsUpdatedDay: Date?
    private(set) var lockedEvoPath: PetEvoPath?
    private(set) var legacy: [PetLegacyRecord] = []
    private(set) var globalNormalizedTotalWatermark: Int?
    private(set) var projectNormalizedTokenWatermarks: [UUID: Int] = [:]
    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared
    private let storage: Storage
    var onSpeechEvent: (@MainActor (PetSpeechEvent) -> Void)?

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
        totalNormalizedTokens: Int = 0,
        hiddenSpeciesChance: Double = 0.15
    ) {
        guard !isClaimed else {
            return
        }
        claimedAt = Date()
        species = option.resolveSpecies(hiddenSpeciesChance: hiddenSpeciesChance)
        self.customName = customName.trimmingCharacters(in: .whitespacesAndNewlines)
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        globalNormalizedTotalWatermark = max(0, totalNormalizedTokens)
        projectNormalizedTokenWatermarks = [:]
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
        .pathA
    }

    func canArchive() -> Bool {
        isClaimed && PetProgressInfo.levelFromXP(currentExperienceTokens) >= PetProgressInfo.maxLevel
    }

    func archiveCurrentPet() {
        guard isClaimed, canArchive() else {
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
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        globalNormalizedTotalWatermark = nil
        projectNormalizedTokenWatermarks = [:]
        save()
    }

    func forgetProjectBaseline(_ projectID: UUID) {
        guard projectNormalizedTokenWatermarks.removeValue(forKey: projectID) != nil else {
            return
        }
        globalNormalizedTotalWatermark = projectNormalizedTokenWatermarks.isEmpty
            ? nil
            : projectNormalizedTokenWatermarks.values.reduce(0, +)
        debugLog.log(
            Self.ledgerLogCategory,
            "forget-project-watermark project=\(projectID.uuidString) remaining=\(projectNormalizedTokenWatermarks.count)"
        )
        save()
    }

    func forgetProjectBaselines(_ projectIDs: some Sequence<UUID>) {
        var removedAny = false
        for projectID in projectIDs {
            if projectNormalizedTokenWatermarks.removeValue(forKey: projectID) != nil {
                removedAny = true
            }
        }
        guard removedAny else {
            return
        }
        globalNormalizedTotalWatermark = projectNormalizedTokenWatermarks.isEmpty
            ? nil
            : projectNormalizedTokenWatermarks.values.reduce(0, +)
        debugLog.log(
            Self.ledgerLogCategory,
            "forget-project-watermarks remaining=\(projectNormalizedTokenWatermarks.count)"
        )
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
        case .code:
            return .pathA
        case .sheep, .ox, .dragon, .phoenix, .dolphin, .penguin, .panda:
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
        totalNormalizedTokens nextTotalNormalizedTokens: Int,
        computedStats: PetStats?,
        now: Date = .init()
    ) {
        refreshDerivedState(
            totalNormalizedTokensByProject: [:],
            fallbackTotalNormalizedTokens: nextTotalNormalizedTokens,
            computedStats: computedStats,
            now: now
        )
    }

    func refreshDerivedState(
        totalNormalizedTokensByProject nextTotalNormalizedTokensByProject: [UUID: Int],
        computedStats: PetStats?,
        now: Date = .init()
    ) {
        refreshDerivedState(
            totalNormalizedTokensByProject: nextTotalNormalizedTokensByProject,
            fallbackTotalNormalizedTokens: nil,
            computedStats: computedStats,
            now: now
        )
    }

    private func refreshDerivedState(
        totalNormalizedTokensByProject nextTotalNormalizedTokensByProject: [UUID: Int],
        fallbackTotalNormalizedTokens: Int?,
        computedStats: PetStats?,
        now: Date
    ) {
        guard isClaimed else {
            return
        }

        let previousLevel = PetProgressInfo.levelFromXP(currentExperienceTokens)
        let previousStats = currentStats
        var didChange = false
        var deltaTokens = 0
        var effectiveTotalNormalizedTokens = 0
        let sanitizedTotalsByProject = nextTotalNormalizedTokensByProject.reduce(into: [UUID: Int]()) { partial, entry in
            partial[entry.key] = max(0, entry.value)
        }

        if sanitizedTotalsByProject.isEmpty == false {
            let currentProjectIDs = Set(sanitizedTotalsByProject.keys)
            let staleProjectIDs = projectNormalizedTokenWatermarks.keys.filter { !currentProjectIDs.contains($0) }
            if staleProjectIDs.isEmpty == false {
                for projectID in staleProjectIDs {
                    projectNormalizedTokenWatermarks.removeValue(forKey: projectID)
                }
                didChange = true
                debugLog.log(
                    Self.ledgerLogCategory,
                    "prune-stale-project-watermarks removed=\(staleProjectIDs.count) remaining=\(projectNormalizedTokenWatermarks.count)"
                )
            }

            for (projectID, total) in sanitizedTotalsByProject {
                if let previousTotal = projectNormalizedTokenWatermarks[projectID] {
                    let projectDelta = max(0, total - previousTotal)
                    deltaTokens += projectDelta
                    if total > previousTotal {
                        projectNormalizedTokenWatermarks[projectID] = total
                        didChange = true
                    }
                } else {
                    projectNormalizedTokenWatermarks[projectID] = total
                    didChange = true
                    debugLog.log(
                        Self.ledgerLogCategory,
                        "bootstrap-project-watermark project=\(projectID.uuidString) total=\(total)"
                    )
                }
            }

            let aggregatedWatermark = projectNormalizedTokenWatermarks.values.reduce(0, +)
            effectiveTotalNormalizedTokens = aggregatedWatermark
            if globalNormalizedTotalWatermark != aggregatedWatermark {
                globalNormalizedTotalWatermark = aggregatedWatermark
                didChange = true
            }
        } else {
            let sanitizedTotal = max(0, fallbackTotalNormalizedTokens ?? 0)
            effectiveTotalNormalizedTokens = sanitizedTotal

            if globalNormalizedTotalWatermark == nil {
                globalNormalizedTotalWatermark = 0
                didChange = true
                debugLog.log(
                    Self.ledgerLogCategory,
                    "bootstrap-watermark total=\(sanitizedTotal) xp=\(currentExperienceTokens)"
                )
            }

            let previousTotal = globalNormalizedTotalWatermark ?? 0
            deltaTokens = max(0, sanitizedTotal - previousTotal)
            if sanitizedTotal > previousTotal {
                globalNormalizedTotalWatermark = sanitizedTotal
                didChange = true
            }
        }

        if deltaTokens > 0 {
            currentExperienceTokens += deltaTokens
            didChange = true
            debugLog.log(
                Self.ledgerLogCategory,
                "apply-delta delta=\(deltaTokens) total=\(effectiveTotalNormalizedTokens) xp=\(currentExperienceTokens)"
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

        if didChange {
            save()
            emitMilestoneSpeechEvents(
                previousLevel: previousLevel,
                previousStats: previousStats,
                now: now
            )
        }
    }

    private func emitMilestoneSpeechEvents(
        previousLevel: Int,
        previousStats: PetStats,
        now: Date
    ) {
        let nextLevel = PetProgressInfo.levelFromXP(currentExperienceTokens)
        if nextLevel > previousLevel {
            onSpeechEvent?(
                PetSpeechEvent(
                    kind: .petLevelUp,
                    payload: ["level": "\(nextLevel)"],
                    occurredAt: now
                )
            )
        }

        for threshold in [200, 250, 300] {
            for entry in statBreakthroughEntries(previous: previousStats, next: currentStats, threshold: threshold) {
                onSpeechEvent?(
                    PetSpeechEvent(
                        kind: .petStatBreakthrough,
                        payload: [
                            "stat": entry.name,
                            "value": "\(threshold)",
                        ],
                        occurredAt: now
                    )
                )
            }
        }
    }

    private func statBreakthroughEntries(
        previous: PetStats,
        next: PetStats,
        threshold: Int
    ) -> [(name: String, value: Int)] {
        [
            (petL("pet.attribute.wisdom", "Wisdom"), previous.wisdom, next.wisdom),
            (petL("pet.attribute.chaos", "Chaos"), previous.chaos, next.chaos),
            (petL("pet.attribute.night", "Night"), previous.night, next.night),
            (petL("pet.attribute.stamina", "Stamina"), previous.stamina, next.stamina),
            (petL("pet.attribute.empathy", "Empathy"), previous.empathy, next.empathy),
        ]
        .compactMap { entry in
            entry.1 < threshold && entry.2 >= threshold ? (entry.0, entry.2) : nil
        }
    }

    func debugForceExperienceTokens(_ experienceTokens: Int, now: Date = .init()) {
        guard isClaimed else {
            return
        }
        currentExperienceTokens = max(0, experienceTokens)
        globalNormalizedTotalWatermark = nil
        projectNormalizedTokenWatermarks = [:]
        if statsUpdatedDay == nil {
            statsUpdatedDay = now
        }
        lockedEvoPath = nil
        save()
    }

    func debugSwitchSpecies(_ nextSpecies: PetSpecies, now: Date = .init()) {
        if !isClaimed {
            claimedAt = now
            currentExperienceTokens = 0
            currentStats = .neutral
            statsUpdatedDay = nil
            globalNormalizedTotalWatermark = nil
            projectNormalizedTokenWatermarks = [:]
        }
        species = nextSpecies
        customName = ""
        lockedEvoPath = nil
        save()
    }

    private func load() {
        guard let loadedState = loadStateFile() else {
            return
        }
        let resolvedState = loadedState.state

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
            } else if loadedState.needsRewrite {
                debugLog.log(
                    "pet-state",
                    "rewrite current-format path=\(storage.fileURL?.path ?? "nil") reason=legacy-layout"
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
                "migrate-reset-stats version=\(resolvedState.stateVersion ?? 0) preservedLedger=true resetClaimedAt=true xp=\(currentExperienceTokens)"
            )
            save()
            return
        }

        claimedAt = Date()
        currentExperienceTokens = 0
        currentStats = .neutral
        statsUpdatedDay = nil
        lockedEvoPath = nil
        globalNormalizedTotalWatermark = nil
        debugLog.log(
            Self.ledgerLogCategory,
            "migrate-reset version=\(resolvedState.stateVersion ?? 0) resetClaimedAt=true xp=\(currentExperienceTokens)"
        )
        save()
    }

    private func applyLedgerState(from state: PersistedPetState) {
        currentExperienceTokens = max(0, state.currentExperienceTokens ?? 0)
        currentStats = state.currentStats ?? .neutral
        statsUpdatedDay = state.statsUpdatedDay
        lockedEvoPath = state.lockedEvoPath
        globalNormalizedTotalWatermark = state.globalNormalizedTotalWatermark
        projectNormalizedTokenWatermarks = state.projectNormalizedTokenWatermarks ?? [:]
    }

    private func shouldPreserveLedgerState(for state: PersistedPetState) -> Bool {
        guard state.stateVersion == 4 else {
            return false
        }
        return (state.legacyPreXPTokenCount ?? 0) > 0 || (state.currentExperienceTokens ?? 0) > 0
    }

    private func save() {
        let state = PersistedPetState(
            stateVersion: Self.stateVersion,
            statsModelVersion: Self.statsModelVersion,
            claimedAt: claimedAt,
            species: species,
            customName: customName,
            legacyPreXPTokenCount: nil,
            currentExperienceTokens: currentExperienceTokens,
            currentStats: currentStats,
            statsUpdatedDay: statsUpdatedDay,
            lockedEvoPath: lockedEvoPath,
            legacy: legacy,
            globalNormalizedTotalWatermark: globalNormalizedTotalWatermark,
            projectNormalizedTokenWatermarks: projectNormalizedTokenWatermarks
        )
        saveStateFile(state)
    }

    private struct LoadedStateFile {
        var state: PersistedPetState
        var needsRewrite: Bool
    }

    private struct DecryptedStateData {
        var data: Data
        var usedLegacyCryptoNamespace: Bool
    }

    private func loadStateFile() -> LoadedStateFile? {
        let migratedLegacyFile = migrateLegacyStateFileIfNeeded()
        guard let fileURL = stateFileURL(),
              fileManager.fileExists(atPath: fileURL.path),
              let rawData = try? Data(contentsOf: fileURL),
              let data = decryptedStateData(from: rawData),
              let state = try? JSONDecoder().decode(PersistedPetState.self, from: data.data) else {
            return nil
        }
        return LoadedStateFile(
            state: state,
            needsRewrite: migratedLegacyFile || data.usedLegacyCryptoNamespace
        )
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

    private func cipherKey(namespace: String) -> SymmetricKey {
        let material = "dmux.pet.state.v2|\(namespace)|codux".data(using: .utf8) ?? Data()
        let digest = SHA256.hash(data: material)
        return SymmetricKey(data: Data(digest))
    }

    private func encryptedStateData(from data: Data) throws -> Data {
        let sealed = try AES.GCM.seal(data, using: cipherKey(namespace: storage.cryptoNamespace))
        guard let combined = sealed.combined else {
            throw CocoaError(.coderInvalidValue)
        }
        return combined
    }

    private func decryptedStateData(from data: Data) -> DecryptedStateData? {
        if let opened = openedStateData(from: data, namespace: storage.cryptoNamespace) {
            return DecryptedStateData(data: opened, usedLegacyCryptoNamespace: false)
        }

        for legacyNamespace in storage.legacyCryptoNamespaces {
            if let opened = openedStateData(from: data, namespace: legacyNamespace) {
                return DecryptedStateData(data: opened, usedLegacyCryptoNamespace: true)
            }
        }

        return DecryptedStateData(data: data, usedLegacyCryptoNamespace: false)
    }

    private func openedStateData(from data: Data, namespace: String) -> Data? {
        if let sealed = try? AES.GCM.SealedBox(combined: data),
           let opened = try? AES.GCM.open(sealed, using: cipherKey(namespace: namespace)) {
            return opened
        }
        return nil
    }

    private func migrateLegacyStateFileIfNeeded() -> Bool {
        guard let fileURL = stateFileURL(),
              !fileManager.fileExists(atPath: fileURL.path) else {
            return false
        }

        for legacyURL in storage.legacyFileURLs where fileManager.fileExists(atPath: legacyURL.path) {
            do {
                try fileManager.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
                try fileManager.moveItem(at: legacyURL, to: fileURL)
                debugLog.log(
                    "pet-state",
                    "migrated legacy file from=\(legacyURL.path) to=\(fileURL.path)"
                )
                return true
            } catch {
                debugLog.log(
                    "pet-state",
                    "migrate legacy file failed from=\(legacyURL.path) to=\(fileURL.path) error=\(error.localizedDescription)"
                )
            }
        }

        return false
    }
}

private struct PersistedPetState: Codable, Equatable {
    var stateVersion: Int?
    var statsModelVersion: Int?
    var claimedAt: Date?
    var species: PetSpecies?
    var customName: String?
    var legacyPreXPTokenCount: Int?
    var currentExperienceTokens: Int?
    var currentStats: PetStats?
    var statsUpdatedDay: Date?
    var lockedEvoPath: PetEvoPath?
    var legacy: [PetLegacyRecord]?
    var globalNormalizedTotalWatermark: Int?
    var projectNormalizedTokenWatermarks: [UUID: Int]?

    // Legacy fields kept only while older pre-XP state inference still reads them.
    var progressionVersion: Int?
    var dailyPaceVersion: Int?
    var baselineAllTimeTokens: Int?
    var growthBaselineAllTimeTokens: Int?

    enum CodingKeys: String, CodingKey {
        case stateVersion
        case statsModelVersion
        case claimedAt
        case species
        case customName
        case legacyPreXPTokenCount = "currentHatchTokens"
        case currentExperienceTokens
        case currentStats
        case statsUpdatedDay
        case lockedEvoPath
        case legacy
        case globalNormalizedTotalWatermark
        case projectNormalizedTokenWatermarks
        case progressionVersion
        case dailyPaceVersion
        case baselineAllTimeTokens
        case growthBaselineAllTimeTokens
    }
}
