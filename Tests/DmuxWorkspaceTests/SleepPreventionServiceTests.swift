import XCTest
import IOKit.pwr_mgt
@testable import DmuxWorkspace

@MainActor
final class SleepPreventionServiceTests: XCTestCase {
    func testAlwaysCreatesAndOffReleasesAssertion() {
        var createdReasons: [String] = []
        var releasedIDs: [IOPMAssertionID] = []
        let service = SleepPreventionService(
            isUsingExternalPower: { false },
            createAssertion: { reason in
                createdReasons.append(reason)
                return 42
            },
            releaseAssertion: { releasedIDs.append($0) },
            logger: nil
        )

        service.configure(mode: .always)
        XCTAssertTrue(service.isPreventingSleep)
        XCTAssertEqual(createdReasons.count, 1)

        service.configure(mode: .off)
        XCTAssertFalse(service.isPreventingSleep)
        XCTAssertEqual(releasedIDs, [42])
    }

    func testPowerAdapterOnlyFollowsPowerSource() {
        var isOnPower = false
        var createdCount = 0
        var releasedIDs: [IOPMAssertionID] = []
        let service = SleepPreventionService(
            isUsingExternalPower: { isOnPower },
            createAssertion: { _ in
                createdCount += 1
                return IOPMAssertionID(createdCount)
            },
            releaseAssertion: { releasedIDs.append($0) },
            logger: nil
        )

        service.configure(mode: .powerAdapterOnly)
        XCTAssertFalse(service.isPreventingSleep)
        XCTAssertEqual(createdCount, 0)

        isOnPower = true
        service.refreshAssertion()
        XCTAssertTrue(service.isPreventingSleep)
        XCTAssertEqual(createdCount, 1)

        service.refreshAssertion()
        XCTAssertEqual(createdCount, 1)

        isOnPower = false
        service.refreshAssertion()
        XCTAssertFalse(service.isPreventingSleep)
        XCTAssertEqual(releasedIDs, [1])
    }
}
