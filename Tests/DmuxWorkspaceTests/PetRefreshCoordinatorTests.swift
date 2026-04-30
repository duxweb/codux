import XCTest
@testable import DmuxWorkspace

@MainActor
final class PetRefreshCoordinatorTests: XCTestCase {
    private final class TotalsBox: @unchecked Sendable {
        var value: [UUID: Int]

        init(_ value: [UUID: Int]) {
            self.value = value
        }
    }

    private final class IntBox: @unchecked Sendable {
        var value: Int

        init(_ value: Int) {
            self.value = value
        }
    }

    func testScheduleRefreshCoalescesIntoSingleDebouncedUpdate() async throws {
        let petStore = PetStore(storage: .inMemory)
        petStore.claim(option: .voidcat, customName: "")
        let projectID = UUID()
        let coordinator = PetRefreshCoordinator(
            petStore: petStore,
            liveRefreshDelay: .milliseconds(20)
        )

        let totals = TotalsBox([projectID: 100])
        var statsCallCount = 0

        coordinator.configure(
            totalNormalizedTokensByProject: { totals.value },
            computedStats: { _ in
                statsCallCount += 1
                return .neutral
            }
        )

        coordinator.scheduleRefresh(reason: .aiSession)
        totals.value = [projectID: 140]
        coordinator.scheduleRefresh(reason: .aiSession)
        totals.value = [projectID: 180]
        coordinator.scheduleRefresh(reason: .aiSession)

        try await Task.sleep(for: .milliseconds(60))

        XCTAssertEqual(petStore.projectNormalizedTokenWatermarks[projectID], 180)
        XCTAssertEqual(petStore.currentHatchTokens, 0)
        XCTAssertEqual(statsCallCount, 1)
    }

    func testScheduleRefreshUsesLatestProjectSnapshotAfterProjectRemoval() async throws {
        let petStore = PetStore(storage: .inMemory)
        petStore.claim(option: .voidcat, customName: "")
        let projectA = UUID()
        let projectB = UUID()
        let coordinator = PetRefreshCoordinator(
            petStore: petStore,
            liveRefreshDelay: .milliseconds(20)
        )

        let totals = TotalsBox([projectA: 120, projectB: 300])
        coordinator.configure(
            totalNormalizedTokensByProject: { totals.value },
            computedStats: { _ in .neutral }
        )

        coordinator.refreshNow(reason: .bootstrap, now: Date(timeIntervalSince1970: 1_700_000_000))

        totals.value = [projectA: 180]
        coordinator.scheduleRefresh(reason: .aiSession)

        try await Task.sleep(for: .milliseconds(60))

        XCTAssertEqual(petStore.projectNormalizedTokenWatermarks[projectA], 180)
        XCTAssertNil(petStore.projectNormalizedTokenWatermarks[projectB])
        XCTAssertEqual(petStore.globalNormalizedTotalWatermark, 180)
        XCTAssertEqual(petStore.currentHatchTokens, 60)
    }

    func testDailyRecordOnlyEmitsWhenCrossingMajorTokenBucket() {
        let petStore = PetStore(storage: .inMemory)
        petStore.claim(option: .voidcat, customName: "")
        let projectID = UUID()
        let coordinator = PetRefreshCoordinator(petStore: petStore)
        let totals = TotalsBox([projectID: 0])
        let dailyTokens = IntBox(22_245_000)
        var events: [PetSpeechEvent] = []

        coordinator.configure(
            totalNormalizedTokensByProject: { totals.value },
            computedStats: { _ in .neutral },
            dailyTotalTokens: { dailyTokens.value }
        )
        coordinator.onSpeechEvent = { events.append($0) }

        let now = Date(timeIntervalSince1970: 1_700_000_000)
        coordinator.refreshNow(reason: .bootstrap, now: now)
        dailyTokens.value = 22_246_000
        coordinator.refreshNow(reason: .aiSession, now: now.addingTimeInterval(10))
        dailyTokens.value = 29_999_999
        coordinator.refreshNow(reason: .aiSession, now: now.addingTimeInterval(20))

        XCTAssertTrue(events.isEmpty)

        dailyTokens.value = 30_000_000
        coordinator.refreshNow(reason: .aiSession, now: now.addingTimeInterval(30))
        dailyTokens.value = 30_001_000
        coordinator.refreshNow(reason: .aiSession, now: now.addingTimeInterval(40))

        XCTAssertEqual(events.map(\.kind), [.usageDailyRecord])
        XCTAssertEqual(events.first?.payload["tokensK"], "30000K")
    }
}
