import AppKit
import Foundation

@MainActor
final class GhosttyTerminalRegistry {
    static let shared = GhosttyTerminalRegistry()

    private var containers: [UUID: GhosttyTerminalContainerView] = [:]
    private var explicitFocusedSessionID: UUID?

    func containerView(
        for session: TerminalSession,
        environment: [(String, String)],
        terminalBackgroundPreset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset,
        terminalFontSize: Int,
        isFocused: Bool,
        isVisible: Bool,
        showsInactiveOverlay: Bool,
        onInteraction: (() -> Void)?,
        onFocusConsumed: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) -> GhosttyTerminalContainerView {
        if let existing = containers[session.id] {
            existing.updateSession(
                session,
                environment: environment,
                terminalBackgroundPreset: terminalBackgroundPreset,
                backgroundColorPreset: backgroundColorPreset,
                terminalFontSize: terminalFontSize,
                isFocused: isFocused,
                isVisible: isVisible,
                showsInactiveOverlay: showsInactiveOverlay,
                onInteraction: onInteraction,
                onFocusConsumed: onFocusConsumed,
                onStartupSucceeded: onStartupSucceeded,
                onStartupFailure: onStartupFailure
            )
            return existing
        }

        let created = GhosttyTerminalContainerView(
            session: session,
            environment: environment,
            terminalFontSize: terminalFontSize,
            onInteraction: onInteraction,
            onFocusConsumed: onFocusConsumed,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        created.updateSession(
            session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset,
            terminalFontSize: terminalFontSize,
            isFocused: isFocused,
            isVisible: isVisible,
            showsInactiveOverlay: showsInactiveOverlay,
            onInteraction: onInteraction,
            onFocusConsumed: onFocusConsumed,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        containers[session.id] = created
        return created
    }

    func release(sessionID: UUID) {
        guard let container = containers.removeValue(forKey: sessionID) else {
            AppDebugLog.shared.log(
                "ghostty-lifecycle",
                "release-miss session=\(sessionID.uuidString)"
            )
            return
        }
        AppDebugLog.shared.log(
            "ghostty-lifecycle",
            "release session=\(sessionID.uuidString) remaining=\(containers.count)"
        )
        GhosttyTerminalPortalRegistry.detach(hostedView: container)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        container.removeFromSuperviewWithoutNeedingDisplay()
        CATransaction.commit()
        container.prepareForPermanentRemoval()
        if explicitFocusedSessionID == sessionID {
            explicitFocusedSessionID = nil
            NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: sessionID)
        }
    }

    func terminateAll() {
        let sessionIDs = Array(containers.keys)
        for sessionID in sessionIDs {
            release(sessionID: sessionID)
        }
    }

    func shellPID(for sessionID: UUID) -> Int32? {
        containers[sessionID]?.terminalShellPID
    }

    func projectID(for sessionID: UUID) -> UUID? {
        containers[sessionID]?.terminalProjectID
    }

    func sessionInstanceID(for sessionID: UUID) -> String? {
        containers[sessionID]?.terminalProcessInstanceID
    }

    func sendText(_ text: String, to sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.sendText(text)
        return true
    }

    func sendInterrupt(to sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.sendInterrupt()
        return true
    }

    func sendEscape(to sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.sendEscape()
        return true
    }

    @discardableResult
    func focus(sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.focusTerminal()
        return true
    }

    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut, responder: NSResponder?) -> Bool {
        if let responder,
           let container = containers.values.first(where: { $0.ownsResponder(responder) }) {
            container.sendEditingShortcut(shortcut)
            return true
        }
        if let sessionID = focusedSessionID(),
           let container = containers[sessionID] {
            container.sendEditingShortcut(shortcut)
            return true
        }
        return false
    }

    func sendNativeCommandArrow(keyCode: UInt16, responder: NSResponder?) -> Bool {
        if let responder,
           let container = containers.values.first(where: { $0.ownsResponder(responder) }) {
            return container.sendNativeCommandArrow(keyCode: keyCode)
        }
        if let sessionID = focusedSessionID(),
           let container = containers[sessionID] {
            return container.sendNativeCommandArrow(keyCode: keyCode)
        }
        return false
    }

    func forwardKeyDown(_ event: NSEvent, responder: NSResponder?) -> Bool {
        if let responder,
           let container = containers.values.first(where: { $0.ownsResponder(responder) }) {
            return container.forwardKeyDown(event)
        }
        if let sessionID = focusedSessionID(),
           let container = containers[sessionID] {
            return container.forwardKeyDown(event)
        }
        return false
    }

    func focusedSessionID() -> UUID? {
        if let explicitFocusedSessionID, containers[explicitFocusedSessionID] != nil {
            return explicitFocusedSessionID
        }
        return containers.first(where: { $0.value.isTerminalFocused })?.key
    }

    func markFocused(sessionID: UUID) {
        guard containers[sessionID] != nil else {
            return
        }
        guard explicitFocusedSessionID != sessionID else {
            return
        }
        explicitFocusedSessionID = sessionID
        NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: sessionID)
    }

    func clearFocusedSession() {
        guard explicitFocusedSessionID != nil else {
            return
        }
        explicitFocusedSessionID = nil
        NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: nil)
    }

    func ownsResponder(_ responder: NSResponder?) -> Bool {
        containers.values.contains { $0.ownsResponder(responder) }
    }

    func debugSnapshot() -> String {
        containers
            .map { sessionID, container in
                let shellPID = container.terminalShellPID.map(String.init) ?? "nil"
                let focused = container.isTerminalFocused ? "focused" : "blurred"
                return "\(sessionID.uuidString):\(shellPID):\(focused)"
            }
            .sorted()
            .joined(separator: ", ")
    }
}

extension GhosttyTerminalRegistry: DmuxTerminalBackendRegistry {}

extension Array where Element == String {
    func withCStringArray<T>(_ body: ([UnsafeMutablePointer<CChar>?]) -> T) -> T {
        let allocated = map { strdup($0) }
        defer {
            for ptr in allocated {
                free(ptr)
            }
        }
        return body(allocated + [nil])
    }
}
