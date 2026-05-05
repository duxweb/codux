import AppKit
import XCTest
@testable import DmuxWorkspace

@MainActor
final class TerminalKeyRoutingTests: XCTestCase {
    func testFileBrowserShortcutsDoNotHandleTerminalResponder() {
        XCTAssertFalse(
            FileBrowserKeyboardFocusState.shouldHandleFileBrowserShortcut(
                isActive: true,
                isInlineRenaming: false,
                hasWindow: true,
                eventWindowMatches: true,
                isTerminalResponder: true
            )
        )
    }

    func testFileBrowserShortcutsHandleOnlyActivePanelFocus() {
        XCTAssertTrue(
            FileBrowserKeyboardFocusState.shouldHandleFileBrowserShortcut(
                isActive: true,
                isInlineRenaming: false,
                hasWindow: true,
                eventWindowMatches: true,
                isTerminalResponder: false
            )
        )
        XCTAssertFalse(
            FileBrowserKeyboardFocusState.shouldHandleFileBrowserShortcut(
                isActive: false,
                isInlineRenaming: false,
                hasWindow: true,
                eventWindowMatches: true,
                isTerminalResponder: false
            )
        )
        XCTAssertFalse(
            FileBrowserKeyboardFocusState.shouldHandleFileBrowserShortcut(
                isActive: true,
                isInlineRenaming: true,
                hasWindow: true,
                eventWindowMatches: true,
                isTerminalResponder: false
            )
        )
        XCTAssertFalse(
            FileBrowserKeyboardFocusState.shouldHandleFileBrowserShortcut(
                isActive: true,
                isInlineRenaming: false,
                hasWindow: true,
                eventWindowMatches: false,
                isTerminalResponder: false
            )
        )
    }

    func testMainMenuShortcutsAreNotRoutedToTerminalKeyDown() {
        XCTAssertFalse(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: true,
                isReservedApplicationShortcut: false
            )
        )
        XCTAssertFalse(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: true,
                isReservedApplicationShortcut: false
            )
        )
    }

    func testReservedApplicationShortcutsAreNotRoutedToTerminalKeyDown() {
        XCTAssertFalse(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: false,
                isReservedApplicationShortcut: true
            )
        )
    }

    func testNonMenuKeysStillRouteToTerminalKeyDown() {
        XCTAssertTrue(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: false,
                isReservedApplicationShortcut: false
            )
        )
        XCTAssertTrue(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: false,
                isReservedApplicationShortcut: false
            )
        )
        XCTAssertTrue(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: false,
                isReservedApplicationShortcut: false
            )
        )
        XCTAssertTrue(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: false,
                isReservedApplicationShortcut: false
            )
        )
        XCTAssertTrue(
            TerminalKeyRoutingPolicy.shouldRouteToTerminal(
                isMainMenuShortcut: false,
                isReservedApplicationShortcut: false
            )
        )
    }
}
