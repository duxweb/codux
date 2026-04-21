import AppKit
import Foundation

extension Notification.Name {
    static let dmuxTerminalFocusDidChange = Notification.Name("dmux.terminalFocusDidChange")
    static let dmuxTerminalOutputDidChange = Notification.Name("dmux.terminalOutputDidChange")
    static let dmuxTerminalInterruptDidSend = Notification.Name("dmux.terminalInterruptDidSend")
}

enum TerminalEditingShortcut: CaseIterable {
    case beginningOfLine
    case endOfLine

    var bytes: [UInt8] {
        switch self {
        case .beginningOfLine:
            return [0x01]
        case .endOfLine:
            return [0x05]
        }
    }

    static func match(keyCode: UInt16, modifiers: NSEvent.ModifierFlags) -> TerminalEditingShortcut? {
        switch (modifiers, keyCode) {
        case ([.command], 123):
            return .beginningOfLine
        case ([.command], 124):
            return .endOfLine
        default:
            return nil
        }
    }
}

@MainActor
protocol DmuxTerminalBackendRegistry: AnyObject {
    func release(sessionID: UUID)
    func beginStructuralResizeTransition(for sessionIDs: [UUID])
    func reconcileGeometry(for sessionIDs: [UUID])
    func terminateAll()
    func shellPID(for sessionID: UUID) -> Int32?
    func projectID(for sessionID: UUID) -> UUID?
    func sessionInstanceID(for sessionID: UUID) -> String?
    func sendText(_ text: String, to sessionID: UUID) -> Bool
    func sendInterrupt(to sessionID: UUID) -> Bool
    func sendEscape(to sessionID: UUID) -> Bool
    func focus(sessionID: UUID) -> Bool
    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut, responder: NSResponder?) -> Bool
    func sendNativeCommandArrow(keyCode: UInt16, responder: NSResponder?) -> Bool
    func forwardKeyDown(_ event: NSEvent, responder: NSResponder?) -> Bool
    func focusedSessionID() -> UUID?
    func ownsResponder(_ responder: NSResponder?) -> Bool
    func debugSnapshot() -> String
}

final class DmuxTerminalOutputEventEmitter: @unchecked Sendable {
    static let shared = DmuxTerminalOutputEventEmitter()

    private let lock = NSLock()
    private var lastPostedAtBySessionID: [UUID: TimeInterval] = [:]
    private let minimumInterval: TimeInterval = 0.35

    func noteOutput(sessionID: UUID) {
        let now = CFAbsoluteTimeGetCurrent()
        let shouldPost: Bool

        lock.lock()
        let lastPostedAt = lastPostedAtBySessionID[sessionID] ?? 0
        shouldPost = now - lastPostedAt >= minimumInterval
        if shouldPost {
            lastPostedAtBySessionID[sessionID] = now
        }
        lock.unlock()

        guard shouldPost else {
            return
        }

        DispatchQueue.main.async {
            NotificationCenter.default.post(name: .dmuxTerminalOutputDidChange, object: sessionID)
        }
    }

    func clear(sessionID: UUID) {
        lock.lock()
        lastPostedAtBySessionID[sessionID] = nil
        lock.unlock()
    }
}

@MainActor
final class DmuxTerminalBackend {
    static let shared = DmuxTerminalBackend()

    private let logger = AppDebugLog.shared

    private init() {
        logger.log("terminal-backend", "bootstrap backend=ghostty")
    }

    var registry: DmuxTerminalBackendRegistry {
        GhosttyTerminalRegistry.shared
    }

    func configure(using settings: AppSettings) {
        _ = settings
        logger.log("terminal-backend", "configure backend=ghostty")
    }
}
