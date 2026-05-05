import XCTest
@testable import DmuxWorkspace

@MainActor
final class PetFeatureTests: XCTestCase {
    func testRandomPetUsesAllSpeciesPool() {
        let count = Double(PetSpecies.allCases.count)
        for (index, species) in PetSpecies.allCases.enumerated() {
            let randomValue = (Double(index) + 0.01) / count
            XCTAssertEqual(
                PetClaimOption.random.resolveSpecies(hiddenSpeciesChance: 0.15, randomValue: randomValue),
                species
            )
        }
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

    func testPetProgressInfoAlwaysUsesSingleCompanionStage() {
        let initial = PetProgressInfo(totalXP: 0)
        XCTAssertEqual(initial.level, 1)
        XCTAssertEqual(initial.stage, .companion)

        let lateXP = PetProgressInfo.totalXPRequired(toReach: 86)
        XCTAssertEqual(PetProgressInfo(totalXP: lateXP).stage, .companion)
    }

    func testPetProgressInfoLevelCurveReachesConfiguredLevel100Target() {
        XCTAssertEqual(
            PetProgressInfo.totalXPRequired(toReach: PetProgressInfo.maxLevel),
            PetProgressInfo.targetXPToReachLevel100
        )
    }

    func testPetSpeciesNamesAreUnified() {
        XCTAssertEqual(PetSpecies.chaossprite.displayName, "Chaos")
        XCTAssertEqual(PetSpecies.code.displayName, "code")
        XCTAssertEqual(PetSpecies.dolphin.displayName, "Splash")
        XCTAssertEqual(PetSpecies.dragon.displayName, "Drako")
        XCTAssertEqual(PetSpecies.goose.displayName, "Goosey")
        XCTAssertEqual(PetSpecies.ox.displayName, "MooMoo")
        XCTAssertEqual(PetSpecies.panda.displayName, "Bamboo")
        XCTAssertEqual(PetSpecies.penguin.displayName, "Pingu")
        XCTAssertEqual(PetSpecies.phoenix.displayName, "Ember")
        XCTAssertEqual(PetSpecies.rusthound.displayName, "Ruff")
        XCTAssertEqual(PetSpecies.sheep.displayName, "BaaBaa")
        XCTAssertEqual(PetSpecies.voidcat.displayName, "Mimi")
    }

    func testPetSpeciesNamesCoverSupportedLocales() throws {
        let url = try XCTUnwrap(Bundle.module.url(forResource: "Localizable", withExtension: "xcstrings"))
        let data = try Data(contentsOf: url)
        let root = try XCTUnwrap(JSONSerialization.jsonObject(with: data) as? [String: Any])
        let strings = try XCTUnwrap(root["strings"] as? [String: Any])
        let locales = Set(["de", "en", "es", "fr", "ja", "ko", "pt-BR", "ru", "zh-Hans", "zh-Hant"])

        for species in PetSpecies.allCases {
            let key = "pet.species.\(species.rawValue).base"
            let entry = try XCTUnwrap(strings[key] as? [String: Any], "Missing \(key)")
            let localizations = try XCTUnwrap(entry["localizations"] as? [String: Any], "Missing localizations for \(key)")
            XCTAssertEqual(Set(localizations.keys), locales, "Incomplete locale coverage for \(key)")
        }

        let codeEntry = try XCTUnwrap(strings["pet.species.code.base"] as? [String: Any])
        let codeLocalizations = try XCTUnwrap(codeEntry["localizations"] as? [String: Any])
        for locale in locales {
            let localization = try XCTUnwrap(codeLocalizations[locale] as? [String: Any])
            let unit = try XCTUnwrap(localization["stringUnit"] as? [String: Any])
            XCTAssertEqual(unit["value"] as? String, "code")
        }
    }

    func testPetResolvedIdentityUsesCustomNameOrSpeciesFallback() {
        let named = PetStage.companion.resolvedIdentity(for: .voidcat, evoPath: .pathA, customName: "奶盖")
        XCTAssertEqual(named.title, "奶盖")
        XCTAssertEqual(named.subtitle, "Mimi")

        let fallback = PetStage.companion.resolvedIdentity(for: .voidcat, evoPath: .pathB, customName: " ")
        XCTAssertEqual(fallback.title, "Mimi")
        XCTAssertNil(fallback.subtitle)
    }

    func testPetCompactNumberUsesKMBSuffixes() {
        XCTAssertEqual(petFormatCompactNumber(999), "999")
        XCTAssertEqual(petFormatCompactNumber(12_300), "12.3K")
        XCTAssertEqual(petFormatCompactNumber(4_200_000), "4.2M")
        XCTAssertEqual(petFormatCompactNumber(3_600_000_000), "3.6B")
    }

    func testCompanionDisplayNameIsUnified() {
        XCTAssertEqual(PetStage.companion.displayName, "Companion")
    }

    func testPetDexCatalogUsesOneEntryPerSpecies() {
        XCTAssertEqual(PetDexEntry.allCases.count, PetSpecies.allCases.count)
        XCTAssertEqual(Set(PetDexEntry.allCases.map(\.species)), Set(PetSpecies.allCases))
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
        let claudeSessions = makePetSessions(
            count: 4,
            baseDate: baseDate,
            titlePrefix: "claude",
            tool: "claude",
            requestCount: 3,
            totalTokens: 60_000,
            activeDurationSeconds: 1_800
        )
        let codexSessions = makePetSessions(
            count: 4,
            baseDate: baseDate,
            titlePrefix: "codex",
            tool: "codex",
            requestCount: 3,
            totalTokens: 60_000,
            activeDurationSeconds: 1_800
        )

        XCTAssertEqual(
            AIStatsStore.computePetStats(from: claudeSessions).wisdom,
            AIStatsStore.computePetStats(from: codexSessions).wisdom
        )
    }

    func testComputePetStatsReturnsNeutralBaselineForLowSampleSize() {
        let sessions = makePetSessions(
            count: 2,
            baseDate: Date(timeIntervalSince1970: 1_700_000_000),
            requestCount: 10,
            totalTokens: 500_000,
            activeDurationSeconds: 1_800
        )

        XCTAssertEqual(
            AIStatsStore.computePetStats(from: sessions),
            PetStats(wisdom: 100, chaos: 100, night: 100, stamina: 100, empathy: 100)
        )
    }

    func testNightTraitRespondsToRecentNightWork() {
        let calendar = Calendar.autoupdatingCurrent
        let startOfDay = calendar.startOfDay(for: Date(timeIntervalSince1970: 1_700_000_000))
        let daySessions = makePetSessions(
            count: 8,
            baseDate: startOfDay.addingTimeInterval(10 * 3_600),
            requestCount: 4,
            totalTokens: 30_000,
            activeDurationSeconds: 600
        )
        let nightSessions = makePetSessions(
            count: 8,
            baseDate: startOfDay.addingTimeInterval(23 * 3_600),
            requestCount: 4,
            totalTokens: 30_000,
            activeDurationSeconds: 600
        )

        let dayStats = AIStatsStore.computePetStats(from: daySessions)
        let nightStats = AIStatsStore.computePetStats(from: nightSessions)
        XCTAssertGreaterThan(nightStats.night, 200)
        XCTAssertGreaterThanOrEqual(nightStats.night, dayStats.night)
    }

    func testLongSessionsDriveStaminaWithoutDependingOnTotalVolume() {
        let sessions = makePetSessions(
            count: 4,
            baseDate: Date(timeIntervalSince1970: 1_700_000_000),
            requestCount: 6,
            totalTokens: 30_000,
            activeDurationSeconds: 10_800
        )

        XCTAssertGreaterThan(AIStatsStore.computePetStats(from: sessions).stamina, 200)
    }

    func testShortBurstSessionsDriveChaos() {
        let sessions = makePetSessions(
            count: 8,
            baseDate: Date(timeIntervalSince1970: 1_700_000_000),
            requestCount: 3,
            totalTokens: 50_000,
            activeDurationSeconds: 60
        )

        XCTAssertGreaterThan(AIStatsStore.computePetStats(from: sessions).chaos, 200)
    }

    func testEmpathyRewardsIterativeRepairSessionsNotJustTinyPrompts() {
        let repairSessions = makePetSessions(
            count: 4,
            baseDate: Date(timeIntervalSince1970: 1_700_000_000),
            titlePrefix: "repair",
            requestCount: 10,
            totalTokens: 30_000,
            activeDurationSeconds: 1_800
        )
        let oneShotSessions = makePetSessions(
            count: 4,
            baseDate: Date(timeIntervalSince1970: 1_700_000_000),
            titlePrefix: "oneshot",
            requestCount: 1,
            totalTokens: 30_000,
            activeDurationSeconds: 1_800
        )

        let repairStats = AIStatsStore.computePetStats(from: repairSessions)
        let oneShotStats = AIStatsStore.computePetStats(from: oneShotSessions)
        XCTAssertGreaterThan(repairStats.empathy, 200)
        XCTAssertGreaterThan(repairStats.empathy, oneShotStats.empathy)
    }

    func testComputePetStatsCapsBelowTheoreticalMax() {
        let calendar = Calendar.autoupdatingCurrent
        let startOfDay = calendar.startOfDay(for: Date(timeIntervalSince1970: 1_700_000_000))
        let sessions = makePetSessions(
            count: 12,
            baseDate: startOfDay.addingTimeInterval(23 * 3_600),
            requestCount: 12,
            totalTokens: 50_000,
            activeDurationSeconds: 10_800
        )

        let stats = AIStatsStore.computePetStats(from: sessions)
        XCTAssertLessThan(stats.wisdom, 340)
        XCTAssertLessThan(stats.chaos, 340)
        XCTAssertLessThan(stats.night, 340)
        XCTAssertLessThan(stats.stamina, 340)
        XCTAssertLessThan(stats.empathy, 340)
    }

    func testComputePetStatsDifferentiatesStylesAtSameVolume() {
        let calendar = Calendar.autoupdatingCurrent
        let startOfDay = calendar.startOfDay(for: Date(timeIntervalSince1970: 1_700_000_000))
        let daySessions = makePetSessions(
            count: 8,
            baseDate: startOfDay.addingTimeInterval(10 * 3_600),
            requestCount: 4,
            totalTokens: 30_000,
            activeDurationSeconds: 600
        )
        let nightSessions = makePetSessions(
            count: 8,
            baseDate: startOfDay.addingTimeInterval(23 * 3_600),
            requestCount: 4,
            totalTokens: 30_000,
            activeDurationSeconds: 600
        )
        let longSessions = makePetSessions(
            count: 8,
            baseDate: startOfDay.addingTimeInterval(10 * 3_600),
            requestCount: 4,
            totalTokens: 30_000,
            activeDurationSeconds: 3_600
        )
        let shortSessions = makePetSessions(
            count: 8,
            baseDate: startOfDay.addingTimeInterval(10 * 3_600),
            requestCount: 4,
            totalTokens: 30_000,
            activeDurationSeconds: 60
        )

        XCTAssertEqual(daySessions.reduce(0) { $0 + $1.totalTokens }, nightSessions.reduce(0) { $0 + $1.totalTokens })
        XCTAssertEqual(longSessions.reduce(0) { $0 + $1.totalTokens }, shortSessions.reduce(0) { $0 + $1.totalTokens })

        let dayStats = AIStatsStore.computePetStats(from: daySessions)
        let nightStats = AIStatsStore.computePetStats(from: nightSessions)
        let longStats = AIStatsStore.computePetStats(from: longSessions)
        let shortStats = AIStatsStore.computePetStats(from: shortSessions)
        XCTAssertGreaterThanOrEqual(nightStats.night - dayStats.night, 80)
        XCTAssertGreaterThanOrEqual(longStats.stamina - shortStats.stamina, 80)
    }

    func testGentleObserverMeansNoTraitDataYet() {
        XCTAssertTrue(["空信号", "Null Signal"].contains(PetStats.neutral.personaTag))
    }

    private func makePetSessions(
        count: Int,
        baseDate: Date,
        titlePrefix: String = "session",
        tool: String = "codex",
        requestCount: Int,
        totalTokens: Int,
        activeDurationSeconds: Int
    ) -> [AISessionSummary] {
        (0..<count).map { index in
            let firstSeenAt = baseDate.addingTimeInterval(Double(index) * 600)
            let lastSeenAt = firstSeenAt.addingTimeInterval(Double(activeDurationSeconds))
            let inputTokens = totalTokens / 3
            let outputTokens = totalTokens - inputTokens
            return AISessionSummary(
                sessionID: UUID(),
                externalSessionID: nil,
                projectID: UUID(),
                projectName: "codux",
                sessionTitle: "\(titlePrefix)-\(index)",
                firstSeenAt: firstSeenAt,
                lastSeenAt: lastSeenAt,
                lastTool: tool,
                lastModel: "gpt-5.4-mini",
                requestCount: requestCount,
                totalInputTokens: inputTokens,
                totalOutputTokens: outputTokens,
                totalTokens: totalTokens,
                maxContextUsagePercent: nil,
                activeDurationSeconds: activeDurationSeconds,
                todayTokens: totalTokens
            )
        }
    }
}

@MainActor
final class PetStoreLifecycleTests: XCTestCase {
    func testClaimUsesBaselineAndOptionalCustomName() {
        let store = PetStore(storage: .inMemory)

        store.claim(option: .voidcat, customName: "  奶盖  ", totalNormalizedTokens: 432_100)

        XCTAssertTrue(store.isClaimed)
        XCTAssertEqual(store.species, .voidcat)
        XCTAssertEqual(store.customName, "奶盖")
        XCTAssertEqual(store.currentExperienceTokens, 0)
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 432_100)
    }

    func testRefreshDerivedStateKeepsSingleCompatPath() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .rusthound, customName: "")

        let unlockXP = PetProgressInfo.totalXPRequired(toReach: PetProgressInfo.maxLevel)
        let targetStats = PetStats(wisdom: 5, chaos: 90, night: 10, stamina: 20, empathy: 3)
        store.debugForceExperienceTokens(unlockXP)
        store.refreshDerivedState(
            totalNormalizedTokens: 0,
            computedStats: targetStats,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertEqual(store.currentEvoPath(), .pathA)

        let oppositeStats = PetStats(wisdom: 5, chaos: 10, night: 10, stamina: 95, empathy: 3)
        store.refreshDerivedState(
            totalNormalizedTokens: 0,
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
            totalNormalizedTokens: 0,
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
            totalNormalizedTokens: 0,
            computedStats: initial,
            now: claimTime.addingTimeInterval(60)
        )
        XCTAssertEqual(store.currentStats, initial)

        let next = PetStats(wisdom: 20, chaos: 90, night: 10, stamina: 15, empathy: 8)
        store.refreshDerivedState(
            totalNormalizedTokens: 0,
            computedStats: next,
            now: claimTime.addingTimeInterval(1800)
        )
        XCTAssertEqual(store.currentStats, initial)

        store.refreshDerivedState(
            totalNormalizedTokens: 0,
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
            totalNormalizedTokens: 0,
            computedStats: initial,
            now: claimTime
        )

        store.refreshDerivedState(
            totalNormalizedTokens: 123_456,
            computedStats: nil,
            now: claimTime.addingTimeInterval(120)
        )

        XCTAssertEqual(store.currentStats, initial)
        XCTAssertEqual(store.currentExperienceTokens, 123_456)
    }

    func testRefreshDerivedStateUsesMonotonicGlobalWatermark() {
        let store = PetStore(storage: .inMemory)
        let claimTime = Date(timeIntervalSince1970: 1_700_000_000)
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokens: 100,
            computedStats: nil,
            now: claimTime
        )
        XCTAssertEqual(store.currentExperienceTokens, 100)
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 100)

        store.refreshDerivedState(
            totalNormalizedTokens: 80,
            computedStats: nil,
            now: claimTime.addingTimeInterval(60)
        )
        store.refreshDerivedState(
            totalNormalizedTokens: 120,
            computedStats: nil,
            now: claimTime.addingTimeInterval(120)
        )
        XCTAssertEqual(store.currentExperienceTokens, 120)
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 120)

        store.refreshDerivedState(
            totalNormalizedTokens: 90,
            computedStats: nil,
            now: claimTime.addingTimeInterval(180)
        )
        store.refreshDerivedState(
            totalNormalizedTokens: 140,
            computedStats: nil,
            now: claimTime.addingTimeInterval(240)
        )
        XCTAssertEqual(store.currentExperienceTokens, 140)
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 140)
    }

    func testRefreshDerivedStateBootstrapsNewProjectHistoryWithoutGrantingPetXP() {
        let store = PetStore(storage: .inMemory)
        let projectA = UUID()
        let projectB = UUID()
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 100],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )
        XCTAssertEqual(store.currentExperienceTokens, 0)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectA], 100)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 140],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_060)
        )
        XCTAssertEqual(store.currentExperienceTokens, 40)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectA], 140)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 140, projectB: 900],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_120)
        )
        XCTAssertEqual(store.currentExperienceTokens, 40)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectB], 900)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 140, projectB: 980],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_180)
        )
        XCTAssertEqual(store.currentExperienceTokens, 120)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectB], 980)
    }

    func testForgettingProjectBaselineMakesReaddedProjectStartFromFreshBaseline() {
        let store = PetStore(storage: .inMemory)
        let projectA = UUID()
        let projectB = UUID()
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 120, projectB: 300],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )
        XCTAssertEqual(store.currentExperienceTokens, 0)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 180, projectB: 340],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_060)
        )
        XCTAssertEqual(store.currentExperienceTokens, 100)

        store.forgetProjectBaseline(projectB)
        XCTAssertNil(store.projectNormalizedTokenWatermarks[projectB])
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 180)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 180, projectB: 900],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_120)
        )
        XCTAssertEqual(store.currentExperienceTokens, 100)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectB], 900)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 180, projectB: 980],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_180)
        )
        XCTAssertEqual(store.currentExperienceTokens, 180)
    }

    func testProjectRemovalKeepsPetProgressStable() {
        let store = PetStore(storage: .inMemory)
        let projectA = UUID()
        let projectB = UUID()
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 120, projectB: 300],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )
        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 180, projectB: 340],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_060)
        )
        XCTAssertEqual(store.currentExperienceTokens, 100)

        store.forgetProjectBaseline(projectB)
        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 180],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_120)
        )
        XCTAssertEqual(store.currentExperienceTokens, 100)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectA], 180)
        XCTAssertNil(store.projectNormalizedTokenWatermarks[projectB])
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 180)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 260],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_180)
        )
        XCTAssertEqual(store.currentExperienceTokens, 180)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 260, projectB: 900],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_240)
        )
        XCTAssertEqual(store.currentExperienceTokens, 180)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectB], 900)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 260, projectB: 980],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_300)
        )
        XCTAssertEqual(store.currentExperienceTokens, 260)
    }

    func testRefreshDerivedStatePrunesMissingProjectBaselineFromSnapshotTotals() {
        let store = PetStore(storage: .inMemory)
        let projectA = UUID()
        let projectB = UUID()
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 120, projectB: 300],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )
        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 180, projectB: 340],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_060)
        )
        XCTAssertEqual(store.currentExperienceTokens, 100)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 260],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_120)
        )

        XCTAssertEqual(store.currentExperienceTokens, 180)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[projectA], 260)
        XCTAssertNil(store.projectNormalizedTokenWatermarks[projectB])
        XCTAssertEqual(store.globalNormalizedTotalWatermark, 260)
    }

    func testReopenedProjectsStartFreshAfterAllBaselinesAreForgotten() {
        let store = PetStore(storage: .inMemory)
        let projectA = UUID()
        let projectB = UUID()
        let reopenedProjectA = UUID()
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 200, projectB: 400],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )
        store.refreshDerivedState(
            totalNormalizedTokensByProject: [projectA: 260, projectB: 460],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_060)
        )
        XCTAssertEqual(store.currentExperienceTokens, 120)

        store.forgetProjectBaselines([projectA, projectB])
        XCTAssertTrue(store.projectNormalizedTokenWatermarks.isEmpty)
        XCTAssertNil(store.globalNormalizedTotalWatermark)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [reopenedProjectA: 960],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_120)
        )
        XCTAssertEqual(store.currentExperienceTokens, 120)
        XCTAssertEqual(store.projectNormalizedTokenWatermarks[reopenedProjectA], 960)

        store.refreshDerivedState(
            totalNormalizedTokensByProject: [reopenedProjectA: 1_040],
            computedStats: nil,
            now: Date(timeIntervalSince1970: 1_700_000_180)
        )
        XCTAssertEqual(store.currentExperienceTokens, 200)
    }

    func testEncryptedDatStorageRoundTripsWithoutKeychain() {
        let tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let fileURL = tempDir.appendingPathComponent("pet-state.dat")
        let storage = PetStore.Storage(
            fileURL: fileURL,
            cryptoNamespace: "tests-roundtrip",
            legacyFileURLs: [],
            legacyCryptoNamespaces: []
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
        let release = PetStore.Storage.makeLive(
            bundleIdentifier: "com.duxweb.dmux",
            appDisplayName: "Codux"
        )
        let dev = PetStore.Storage.makeLive(
            bundleIdentifier: "com.duxweb.dmux.dev",
            appDisplayName: "Codux-dev"
        )

        XCTAssertEqual(release.fileURL?.lastPathComponent, "pet-state.dat")
        XCTAssertEqual(dev.fileURL?.lastPathComponent, "pet-state.dat")
        XCTAssertNotEqual(release.fileURL?.path, dev.fileURL?.path)
        XCTAssertTrue(release.fileURL?.path.contains("/Codux/") ?? false)
        XCTAssertTrue(dev.fileURL?.path.contains("/Codux-dev/") ?? false)
        XCTAssertEqual(release.cryptoNamespace, "codux")
        XCTAssertEqual(dev.cryptoNamespace, "codux-dev")
        XCTAssertTrue(release.legacyFileURLs.first?.path.contains("/dmux/") ?? false)
        XCTAssertTrue(dev.legacyFileURLs.first?.path.contains("/dmux-dev/") ?? false)
        XCTAssertEqual(release.legacyCryptoNamespaces, ["prod"])
        XCTAssertEqual(dev.legacyCryptoNamespaces, ["dev"])
    }

    func testReleaseStorageMigratesLegacyFileAndNamespace() throws {
        let rootURL = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        let newRootURL = rootURL.appendingPathComponent("Codux", isDirectory: true)
        let legacyRootURL = rootURL.appendingPathComponent("dmux", isDirectory: true)
        let newFileURL = newRootURL.appendingPathComponent("pet-state.dat")
        let legacyFileURL = legacyRootURL.appendingPathComponent("pet-state.dat")

        let legacyStorage = PetStore.Storage(
            fileURL: legacyFileURL,
            cryptoNamespace: "prod",
            legacyFileURLs: [],
            legacyCryptoNamespaces: []
        )
        let migratedStorage = PetStore.Storage(
            fileURL: newFileURL,
            cryptoNamespace: "codux",
            legacyFileURLs: [legacyFileURL],
            legacyCryptoNamespaces: ["prod"]
        )

        let legacyStore = PetStore(storage: legacyStorage)
        legacyStore.claim(option: .goose, customName: "旧宠物", totalNormalizedTokens: 123_456)

        XCTAssertTrue(FileManager.default.fileExists(atPath: legacyFileURL.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: newFileURL.path))

        let migratedStore = PetStore(storage: migratedStorage)

        XCTAssertTrue(migratedStore.isClaimed)
        XCTAssertEqual(migratedStore.species, .goose)
        XCTAssertEqual(migratedStore.customName, "旧宠物")
        XCTAssertEqual(migratedStore.globalNormalizedTotalWatermark, 123_456)
        XCTAssertTrue(FileManager.default.fileExists(atPath: newFileURL.path))
        XCTAssertFalse(FileManager.default.fileExists(atPath: legacyFileURL.path))

        let reloadedStore = PetStore(storage: migratedStorage)
        XCTAssertTrue(reloadedStore.isClaimed)
        XCTAssertEqual(reloadedStore.species, .goose)
        XCTAssertEqual(reloadedStore.customName, "旧宠物")
    }

    func testArchiveCurrentPetResetsClaimState() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .goose, customName: "阿呆")

        let maxXP = PetProgressInfo.totalXPRequired(toReach: PetProgressInfo.maxLevel)
        let stats = PetStats(wisdom: 8, chaos: 25, night: 12, stamina: 20, empathy: 80)
        store.debugForceExperienceTokens(maxXP)
        store.refreshDerivedState(
            totalNormalizedTokens: 0,
            computedStats: stats,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertTrue(store.canArchive())
        store.archiveCurrentPet()

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

    func testFirstRefreshAddsTokensDirectlyToExperience() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "")

        store.refreshDerivedState(
            totalNormalizedTokens: 123,
            computedStats: .neutral,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertEqual(store.currentExperienceTokens, 123)
        XCTAssertEqual(
            PetProgressInfo(totalXP: store.currentExperienceTokens).level,
            1
        )
    }

    func testSubsequentRefreshAppliesOnlyPositiveTokenDelta() {
        let store = PetStore(storage: .inMemory)
        store.claim(option: .voidcat, customName: "")

        let overflow = PetProgressInfo.xpForLevel(1) * 2
        store.refreshDerivedState(
            totalNormalizedTokens: overflow,
            computedStats: .neutral,
            now: Date(timeIntervalSince1970: 1_700_000_000)
        )

        XCTAssertEqual(store.currentExperienceTokens, overflow)
        XCTAssertGreaterThanOrEqual(PetProgressInfo(totalXP: store.currentExperienceTokens).level, 2)

        store.refreshDerivedState(
            totalNormalizedTokens: overflow + 123,
            computedStats: .neutral,
            now: Date(timeIntervalSince1970: 1_700_000_100)
        )

        XCTAssertEqual(store.currentExperienceTokens, overflow + 123)
    }
}
