import XCTest
@testable import DmuxWorkspace

@MainActor
final class PetFeatureTests: XCTestCase {
    func testRandomEggUsesHiddenSpeciesChanceThenStandardPool() {
        XCTAssertEqual(PetClaimOption.random.resolveSpecies(hiddenSpeciesChance: 0.15, randomValue: 0.00), .chaossprite)
        XCTAssertEqual(PetClaimOption.random.resolveSpecies(hiddenSpeciesChance: 0.15, randomValue: 0.149), .chaossprite)

        XCTAssertEqual(PetClaimOption.random.resolveSpecies(hiddenSpeciesChance: 0.15, randomValue: 0.150), .voidcat)
        XCTAssertEqual(PetClaimOption.random.resolveSpecies(hiddenSpeciesChance: 0.15, randomValue: 0.500), .rusthound)
        XCTAssertEqual(PetClaimOption.random.resolveSpecies(hiddenSpeciesChance: 0.15, randomValue: 0.990), .goose)
    }

    func testHiddenSpeciesChanceUsesTwoRecentToolsForBoost() {
        XCTAssertEqual(AIStatsStore.hiddenPetSpeciesChance(forToolTotals: [:]), 0.15)
        XCTAssertEqual(
            AIStatsStore.hiddenPetSpeciesChance(forToolTotals: ["claude": 12_000_000]),
            0.15
        )
        XCTAssertEqual(
            AIStatsStore.hiddenPetSpeciesChance(forToolTotals: [
                "claude": 1,
                "opencode": 1,
            ]),
            0.50
        )
    }

    func testPetProgressInfoStaysEggUntilHatchThreshold() {
        let preHatch = PetProgressInfo(totalXP: 0, hatchTokens: PetProgressInfo.hatchThreshold - 1, evoPath: .pathA)
        XCTAssertEqual(preHatch.level, 0)
        XCTAssertEqual(preHatch.stage, .egg)
        XCTAssertTrue(preHatch.isHatching)

        let hatched = PetProgressInfo(totalXP: 0, hatchTokens: PetProgressInfo.hatchThreshold, evoPath: .pathA)
        XCTAssertEqual(hatched.level, 1)
        XCTAssertEqual(hatched.stage, .infant)
        XCTAssertFalse(hatched.isHatching)
        XCTAssertEqual(hatched.totalXP, 0)
    }

    func testPetProgressInfoUsesEvolutionPathForLateStages() {
        let evoAXP = PetProgressInfo.totalXPRequired(toReach: 61)
        let evoBXP = PetProgressInfo.totalXPRequired(toReach: 61)
        let megaXP = PetProgressInfo.totalXPRequired(toReach: 86)

        XCTAssertEqual(PetProgressInfo(totalXP: evoAXP, hatchTokens: PetProgressInfo.hatchThreshold, evoPath: .pathA).stage, .evoA)
        XCTAssertEqual(PetProgressInfo(totalXP: evoBXP, hatchTokens: PetProgressInfo.hatchThreshold, evoPath: .pathB).stage, .evoB)
        XCTAssertEqual(PetProgressInfo(totalXP: megaXP, hatchTokens: PetProgressInfo.hatchThreshold, evoPath: .pathA).stage, .megaA)
        XCTAssertEqual(PetProgressInfo(totalXP: megaXP, hatchTokens: PetProgressInfo.hatchThreshold, evoPath: .pathB).stage, .megaB)
    }

    func testPetProgressInfoLevelCurveReachesConfiguredLevel100Target() {
        XCTAssertEqual(
            PetProgressInfo.totalXPRequired(toReach: PetProgressInfo.maxLevel),
            PetProgressInfo.targetXPToReachLevel100
        )
    }

    func testPetStageSpeciesNameFollowsEvolutionPath() {
        XCTAssertTrue(["书卷猫", "Tomecat"].contains(PetStage.evoA.speciesName(for: .voidcat, evoPath: .pathA)))
        XCTAssertTrue(["暗影猫", "Shadecat"].contains(PetStage.evoB.speciesName(for: .voidcat, evoPath: .pathB)))
        XCTAssertTrue(["艳阳", "Sunflare"].contains(PetStage.megaA.speciesName(for: .rusthound, evoPath: .pathA)))
        XCTAssertTrue(["血月", "Bloodmoon"].contains(PetStage.megaB.speciesName(for: .rusthound, evoPath: .pathB)))
    }

    func testPetResolvedIdentityUsesCustomNameOrSpeciesFallback() {
        let named = PetStage.adult.resolvedIdentity(for: .voidcat, evoPath: .pathA, customName: "奶盖")
        XCTAssertEqual(named.title, "奶盖")
        XCTAssertTrue(["墨瞳猫", "Voidcat"].contains(named.subtitle ?? ""))

        let fallback = PetStage.evoB.resolvedIdentity(for: .voidcat, evoPath: .pathB, customName: " ")
        XCTAssertTrue(["暗影猫", "Shadecat"].contains(fallback.title))
        XCTAssertNil(fallback.subtitle)
    }

    func testPetCompactNumberUsesKMBSuffixes() {
        XCTAssertEqual(petFormatCompactNumber(999), "999")
        XCTAssertEqual(petFormatCompactNumber(12_300), "12.3K")
        XCTAssertEqual(petFormatCompactNumber(4_200_000), "4.2M")
        XCTAssertEqual(petFormatCompactNumber(3_600_000_000), "3.6B")
    }

    func testFinalAwakeningDisplayNameIsUnified() {
        XCTAssertTrue(["最终觉醒", "Final Awakening"].contains(PetStage.megaA.displayName))
        XCTAssertTrue(["最终觉醒", "Final Awakening"].contains(PetStage.megaB.displayName))
    }

    func testPetDexCatalogUsesAllSpeciesAcrossSevenPlayableStagesWithoutEgg() {
        XCTAssertEqual(PetDexEntry.catalogStages.count, 7)
        XCTAssertEqual(PetDexEntry.allCases.count, PetSpecies.allCases.count * 7)
        XCTAssertFalse(PetDexEntry.allCases.contains { $0.stage == .egg })
    }

    func testPetStatsApplyingDampingMovesTowardTargetWithoutOvershoot() {
        let current = PetStats(wisdom: 10, chaos: 50, night: 90, stamina: 0, empathy: 5)
        let target = PetStats(wisdom: 50, chaos: 20, night: 30, stamina: 100, empathy: 9)

        let damped = current.applyingDamping(toward: target, factor: 0.25)

        XCTAssertEqual(damped.wisdom, 20)
        XCTAssertEqual(damped.chaos, 42)
        XCTAssertEqual(damped.night, 75)
        XCTAssertEqual(damped.stamina, 25)
        XCTAssertEqual(damped.empathy, 6)
    }

    func testBalancedStatsDoNotCollapseToSingleDominantPersona() {
        let balanced = PetStats(wisdom: 100, chaos: 94, night: 91, stamina: 88, empathy: 86)
        XCTAssertTrue(["零号协议", "Zero Protocol"].contains(balanced.personaTag))
    }

    func testWisdomNoLongerDependsOnClaudeToolBias() {
        let baseDate = Date(timeIntervalSince1970: 1_700_000_000)
        let claudeSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "claude",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(1_800),
            lastTool: "claude",
            lastModel: "haiku",
            requestCount: 10,
            totalInputTokens: 50_000,
            totalOutputTokens: 450_000,
            totalTokens: 500_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 1_800,
            todayTokens: 500_000
        )
        let codexSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "codex",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(1_800),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 10,
            totalInputTokens: 50_000,
            totalOutputTokens: 450_000,
            totalTokens: 500_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 1_800,
            todayTokens: 500_000
        )

        XCTAssertEqual(
            AIStatsStore.computePetStats(from: [claudeSession]).wisdom,
            AIStatsStore.computePetStats(from: [codexSession]).wisdom
        )
    }

    func testEmpathyRewardsIterativeRepairSessionsNotJustTinyPrompts() {
        let baseDate = Date(timeIntervalSince1970: 1_700_000_000)
        let iterativeRepair = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "repair",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(1_800),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 7,
            totalInputTokens: 180_000,
            totalOutputTokens: 520_000,
            totalTokens: 700_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 1_800,
            todayTokens: 700_000
        )
        let oneShotLong = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "oneshot",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(1_800),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 1,
            totalInputTokens: 300_000,
            totalOutputTokens: 400_000,
            totalTokens: 700_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 1_800,
            todayTokens: 700_000
        )

        XCTAssertGreaterThan(
            AIStatsStore.computePetStats(from: [iterativeRepair]).empathy,
            AIStatsStore.computePetStats(from: [oneShotLong]).empathy
        )
    }

    func testNightTraitSoftStartsWithoutTenPercentHardCutoff() {
        let calendar = Calendar.current
        let referenceDate = Date(timeIntervalSince1970: 1_700_000_000)
        let startOfDay = calendar.startOfDay(for: referenceDate)
        let daySessions = (0..<19).map { index in
            AISessionSummary(
                sessionID: UUID(),
                externalSessionID: nil,
                projectID: UUID(),
                projectName: "codux",
                sessionTitle: "day-\(index)",
                firstSeenAt: startOfDay.addingTimeInterval(Double(index % 8) * 3_600 + 9 * 3_600),
                lastSeenAt: startOfDay.addingTimeInterval(Double(index % 8) * 3_600 + 9 * 3_600 + 180),
                lastTool: "codex",
                lastModel: "gpt-5.4-mini",
                requestCount: 2,
                totalInputTokens: 200,
                totalOutputTokens: 300,
                totalTokens: 500,
                maxContextUsagePercent: nil,
                activeDurationSeconds: 180,
                todayTokens: 500
            )
        }
        let nightSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "night",
            firstSeenAt: startOfDay.addingTimeInterval(23 * 3_600),
            lastSeenAt: startOfDay.addingTimeInterval(23 * 3_600 + 180),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 2,
            totalInputTokens: 200,
            totalOutputTokens: 300,
            totalTokens: 500,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 180,
            todayTokens: 500
        )

        let stats = AIStatsStore.computePetStats(from: daySessions + [nightSession])
        XCTAssertGreaterThan(stats.night, 0)
    }

    func testSingleLongSessionDoesNotOverfavorStaminaOverWisdom() {
        let baseDate = Date(timeIntervalSince1970: 1_700_000_000)
        let longSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "deep-work",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(1_800),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 1,
            totalInputTokens: 1_000,
            totalOutputTokens: 2_000,
            totalTokens: 3_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 1_800,
            todayTokens: 3_000
        )

        let stats = AIStatsStore.computePetStats(from: [longSession])
        XCTAssertGreaterThan(stats.wisdom, stats.stamina)
    }

    func testStaminaBackfillsFromSessionSpanAndTurnCountWhenActiveDurationIsSparse() {
        let baseDate = Date(timeIntervalSince1970: 1_700_000_000)
        let sustainedSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "sustained",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(360),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 4,
            totalInputTokens: 8_000,
            totalOutputTokens: 22_000,
            totalTokens: 30_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 2,
            todayTokens: 30_000
        )
        let burstSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "burst",
            firstSeenAt: baseDate,
            lastSeenAt: baseDate.addingTimeInterval(3),
            lastTool: "codex",
            lastModel: "gpt-5.4-mini",
            requestCount: 1,
            totalInputTokens: 8_000,
            totalOutputTokens: 22_000,
            totalTokens: 30_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 2,
            todayTokens: 30_000
        )

        XCTAssertGreaterThan(
            AIStatsStore.computePetStats(from: [sustainedSession]).stamina,
            AIStatsStore.computePetStats(from: [burstSession]).stamina
        )
    }

    func testPetStatsComputedFromClaimedSessionsOnly() {
        let claimDate = Date(timeIntervalSince1970: 1_700_000_000)
        let historicalSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "old",
            firstSeenAt: claimDate.addingTimeInterval(-86_400),
            lastSeenAt: claimDate.addingTimeInterval(-60),
            lastTool: "claude",
            lastModel: "claude-haiku",
            requestCount: 20,
            totalInputTokens: 100_000,
            totalOutputTokens: 4_900_000,
            totalTokens: 5_000_000,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 7_200,
            todayTokens: 0
        )
        let claimedSession = AISessionSummary(
            sessionID: UUID(),
            externalSessionID: nil,
            projectID: UUID(),
            projectName: "codux",
            sessionTitle: "new",
            firstSeenAt: claimDate.addingTimeInterval(60),
            lastSeenAt: claimDate.addingTimeInterval(3_600),
            lastTool: "opencode",
            lastModel: "minimax",
            requestCount: 2,
            totalInputTokens: 50,
            totalOutputTokens: 150,
            totalTokens: 200,
            maxContextUsagePercent: nil,
            activeDurationSeconds: 120,
            todayTokens: 200
        )

        let claimedOnly = AIStatsStore.computePetStats(from: [claimedSession])
        let mixed = AIStatsStore.computePetStats(from: [historicalSession, claimedSession].filter { $0.lastSeenAt >= claimDate })

        XCTAssertEqual(claimedOnly, mixed)
    }

    func testGentleObserverMeansNoTraitDataYet() {
        XCTAssertTrue(["空信号", "Null Signal"].contains(PetStats.neutral.personaTag))
    }
}

@MainActor
final class PetStoreLifecycleTests: XCTestCase {
    func testClaimUsesBaselineAndOptionalCustomName() {
        let store = PetStore(storage: .inMemory)

        store.claim(option: .voidcat, customName: "  奶盖  ")

        XCTAssertTrue(store.isClaimed)
        XCTAssertEqual(store.species, .voidcat)
        XCTAssertEqual(store.customName, "奶盖")
        XCTAssertEqual(store.currentExperienceTokens, 0)
        XCTAssertEqual(store.currentHatchTokens, 0)
    }

    func testRefreshDerivedStateLocksEvolutionPathOnceUnlocked() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .rusthound, customName: "")

        let unlockXP = PetProgressInfo.totalXPRequired(toReach: PetProgressInfo.evoUnlockLevel)
        let targetStats = PetStats(wisdom: 5, chaos: 90, night: 10, stamina: 20, empathy: 3)
        store.debugCompleteHatch()
        store.debugForceExperienceTokens(unlockXP)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: targetStats,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertEqual(store.currentEvoPath(), .pathA)

        let oppositeStats = PetStats(wisdom: 5, chaos: 10, night: 10, stamina: 95, empathy: 3)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: oppositeStats,
            now: Date(timeIntervalSince1970: 1_700_086_400)
        )

        XCTAssertEqual(store.currentEvoPath(), .pathA)
    }

    func testRefreshDerivedStateAppliesInitialTraitDataOnSameDay() {
        let store = PetStore(storage: .inMemory)
        let claimTime = Date(timeIntervalSince1970: 1_700_000_000)
        store.claim(option: .voidcat, customName: "")

        let traits = PetStats(wisdom: 88, chaos: 12, night: 44, stamina: 10, empathy: 9)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: traits,
            now: claimTime
        )

        XCTAssertEqual(store.currentStats, traits)
        XCTAssertFalse(["佛系观察者", "Gentle Observer"].contains(store.currentStats.personaTag))
    }

    func testClaimStartsTraitsAtZeroBeforeAnyAccumulation() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "")

        XCTAssertEqual(store.currentStats, .neutral)
    }

    func testRefreshDerivedStateUpdatesTraitsHourlyAfterClaim() {
        let store = PetStore(storage: .inMemory)
        let claimTime = Date(timeIntervalSince1970: 1_700_000_000)
        store.claim(option: .voidcat, customName: "")

        let initial = PetStats(wisdom: 80, chaos: 10, night: 20, stamina: 5, empathy: 3)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: initial,
            now: claimTime.addingTimeInterval(60)
        )
        XCTAssertEqual(store.currentStats, initial)

        let next = PetStats(wisdom: 20, chaos: 90, night: 10, stamina: 15, empathy: 8)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: next,
            now: claimTime.addingTimeInterval(1800)
        )
        XCTAssertEqual(store.currentStats, initial)

        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: next,
            now: claimTime.addingTimeInterval(3660)
        )
        XCTAssertNotEqual(store.currentStats, initial)
    }

    func testRefreshDerivedStateCanSkipTraitRefreshWhileStillApplyingTokenDelta() {
        let store = PetStore(storage: .inMemory)
        let claimTime = Date(timeIntervalSince1970: 1_700_000_000)
        store.claim(option: .voidcat, customName: "")

        let initial = PetStats(wisdom: 80, chaos: 10, night: 20, stamina: 5, empathy: 3)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: initial,
            now: claimTime
        )

        store.refreshDerivedState(
            realtimeSessionTotals: ["session": 123_456],
            computedStats: nil,
            now: claimTime.addingTimeInterval(120)
        )

        XCTAssertEqual(store.currentStats, initial)
        XCTAssertEqual(store.currentHatchTokens, 123_456)
        XCTAssertEqual(store.currentExperienceTokens, 0)
    }

    func testRefreshDerivedStatePrunesInactiveWatermarksOnlyAfterRetentionWindow() {
        let store = PetStore(storage: .inMemory)
        let claimTime = Date(timeIntervalSince1970: 1_700_000_000)
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            realtimeSessionTotals: ["session": 100],
            computedStats: nil,
            now: claimTime
        )
        XCTAssertEqual(store.currentHatchTokens, 100)

        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: nil,
            now: claimTime.addingTimeInterval(PetStore.realtimeSessionRetentionInterval - 1)
        )
        store.refreshDerivedState(
            realtimeSessionTotals: ["session": 120],
            computedStats: nil,
            now: claimTime.addingTimeInterval(PetStore.realtimeSessionRetentionInterval)
        )
        XCTAssertEqual(store.currentHatchTokens, 120)

        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: nil,
            now: claimTime.addingTimeInterval(PetStore.realtimeSessionRetentionInterval * 2)
        )
        store.refreshDerivedState(
            realtimeSessionTotals: ["session": 140],
            computedStats: nil,
            now: claimTime.addingTimeInterval(PetStore.realtimeSessionRetentionInterval * 2 + 1)
        )
        XCTAssertEqual(store.currentHatchTokens, 120)
    }

    func testEncryptedDatStorageRoundTripsWithoutKeychain() {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let fileURL = tempDir.appendingPathComponent("pet-state.dat")
        let storage = PetStore.Storage(
            fileURL: fileURL,
            cryptoNamespace: "tests-roundtrip"
        )

        do {
            let store = PetStore(storage: storage)
            store.claim(option: .rusthound, customName: "火花")

            let reloaded = PetStore(storage: storage)
            XCTAssertTrue(reloaded.isClaimed)
            XCTAssertEqual(reloaded.species, .rusthound)
            XCTAssertEqual(reloaded.customName, "火花")

            let raw = try Data(contentsOf: fileURL)
            let text = String(data: raw, encoding: .utf8) ?? ""
            XCTAssertFalse(text.contains("火花"))
            XCTAssertFalse(text.contains("rusthound"))
        } catch {
            XCTFail("Encrypted dat roundtrip failed: \(error)")
        }
    }

    func testLiveStorageSeparatesDeveloperAndReleaseData() {
        let release = PetStore.Storage.makeLive(bundleIdentifier: "com.duxweb.dmux")
        let dev = PetStore.Storage.makeLive(bundleIdentifier: "com.duxweb.dmux.dev")

        XCTAssertEqual(release.fileURL?.lastPathComponent, "pet-state.dat")
        XCTAssertEqual(dev.fileURL?.lastPathComponent, "pet-state.dat")
        XCTAssertNotEqual(release.fileURL?.path, dev.fileURL?.path)
        XCTAssertTrue(release.fileURL?.path.contains("/dmux/") ?? false)
        XCTAssertTrue(dev.fileURL?.path.contains("/dmux-dev/") ?? false)
    }

    func testInheritArchivesCurrentPetAndResetsClaimState() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .goose, customName: "阿呆")

        let maxXP = PetProgressInfo.totalXPRequired(toReach: PetProgressInfo.maxLevel)
        let stats = PetStats(wisdom: 8, chaos: 25, night: 12, stamina: 20, empathy: 80)
        store.debugCompleteHatch()
        store.debugForceExperienceTokens(maxXP)
        store.refreshDerivedState(
            realtimeSessionTotals: [:],
            computedStats: stats,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertTrue(store.canInherit())
        store.inheritCurrentPet()

        XCTAssertFalse(store.isClaimed)
        XCTAssertEqual(store.species, .voidcat)
        XCTAssertEqual(store.customName, "")
        XCTAssertEqual(store.currentExperienceTokens, 0)
        XCTAssertEqual(store.currentStats, .neutral)
        XCTAssertEqual(store.legacy.count, 1)
        XCTAssertEqual(store.legacy[0].species, .goose)
        XCTAssertEqual(store.legacy[0].customName, "阿呆")
        XCTAssertEqual(store.legacy[0].evoPath, .pathA)
    }

    func testDebugForceExperienceTokensMovesPetToRequestedXP() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "")

        store.debugForceExperienceTokens(0)

        XCTAssertEqual(store.currentHatchTokens, PetProgressInfo.hatchThreshold)
        XCTAssertEqual(store.currentExperienceTokens, 0)
    }

    func testDebugSwitchSpeciesPreservesClaimAndResetsName() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "旧名字")
        store.debugForceExperienceTokens(PetProgressInfo.totalXPRequired(toReach: 70))

        store.debugSwitchSpecies(.chaossprite)

        XCTAssertTrue(store.isClaimed)
        XCTAssertEqual(store.species, .chaossprite)
        XCTAssertEqual(store.customName, "")
        XCTAssertEqual(store.currentExperienceTokens, PetProgressInfo.totalXPRequired(toReach: 70))
        XCTAssertEqual(store.currentEvoPath(), .pathA)
    }

    func testHatchThresholdDoesNotCountTowardGrowthXP() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            realtimeSessionTotals: ["session": PetProgressInfo.hatchThreshold],
            computedStats: .neutral,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertEqual(store.currentHatchTokens, PetProgressInfo.hatchThreshold)
        XCTAssertEqual(store.currentExperienceTokens, 0)
        XCTAssertEqual(
            PetProgressInfo(totalXP: store.currentExperienceTokens, hatchTokens: store.currentHatchTokens, evoPath: .pathA).level,
            1
        )
    }

    func testFirstHatchCarriesOverflowIntoGrowthXP() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "")

        let overflow = PetProgressInfo.xpForLevel(1) * 2
        store.refreshDerivedState(
            realtimeSessionTotals: ["session": PetProgressInfo.hatchThreshold + overflow],
            computedStats: .neutral,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertEqual(store.currentHatchTokens, PetProgressInfo.hatchThreshold)
        XCTAssertEqual(store.currentExperienceTokens, overflow)
        XCTAssertEqual(
            PetProgressInfo(totalXP: store.currentExperienceTokens, hatchTokens: store.currentHatchTokens, evoPath: .pathA).level,
            2
        )

        store.refreshDerivedState(
            realtimeSessionTotals: ["session": PetProgressInfo.hatchThreshold + overflow + 123],
            computedStats: .neutral,
            now: Date(timeIntervalSince1970: 1_700_000_100)
        )

        XCTAssertEqual(store.currentExperienceTokens, overflow + 123)
    }
}
