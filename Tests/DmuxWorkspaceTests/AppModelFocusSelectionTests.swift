import XCTest
@testable import DmuxWorkspace

@MainActor
final class AppModelFocusSelectionTests: XCTestCase {
    func testDisplayedFocusedSessionPrefersPendingFocusRequest() {
        let requested = UUID()
        let registryFocused = UUID()
        let selected = UUID()

        let resolved = AppModel.resolveDisplayedFocusedTerminalSessionID(
            focusRequestID: requested,
            registryFocusedSessionID: registryFocused,
            selectedSessionID: selected
        )

        XCTAssertEqual(resolved, requested)
    }

    func testDisplayedFocusedSessionFallsBackToRegistryThenSelection() {
        let registryFocused = UUID()
        let selected = UUID()

        XCTAssertEqual(
            AppModel.resolveDisplayedFocusedTerminalSessionID(
                focusRequestID: nil,
                registryFocusedSessionID: registryFocused,
                selectedSessionID: selected
            ),
            registryFocused
        )

        XCTAssertEqual(
            AppModel.resolveDisplayedFocusedTerminalSessionID(
                focusRequestID: nil,
                registryFocusedSessionID: nil,
                selectedSessionID: selected
            ),
            selected
        )
    }


    func testDisplayedFocusedSessionIgnoresRegistryFocusOutsideVisibleWorkspace() {
        let staleRegistryFocused = UUID()
        let selected = UUID()

        XCTAssertEqual(
            AppModel.resolveDisplayedFocusedTerminalSessionID(
                focusRequestID: nil,
                registryFocusedSessionID: staleRegistryFocused,
                selectedSessionID: selected,
                visibleSessionIDs: [selected]
            ),
            selected
        )
    }

    func testBottomTabSelectionRefreshesWhenRegistryFocusIsStale() {
        let requested = UUID()
        let staleFocused = UUID()

        XCTAssertTrue(
            AppModel.shouldRefreshBottomTabSelection(
                requestedSessionID: requested,
                selectedSessionID: requested,
                selectedBottomTabSessionID: requested,
                pendingFocusRequestID: nil,
                registryFocusedSessionID: staleFocused
            )
        )
    }

    func testBottomTabSelectionSkipsNoOpWhenStateAlreadyAligned() {
        let requested = UUID()

        XCTAssertFalse(
            AppModel.shouldRefreshBottomTabSelection(
                requestedSessionID: requested,
                selectedSessionID: requested,
                selectedBottomTabSessionID: requested,
                pendingFocusRequestID: requested,
                registryFocusedSessionID: requested
            )
        )
    }
}
