import AppKit
import SwiftUI

struct TerminalPaneView: View {
    let model: AppModel
    let session: TerminalSession
    let terminalBackgroundPreset: AppTerminalBackgroundPreset
    let backgroundColorPreset: AppBackgroundColorPreset
    let isFocused: Bool
    let isVisible: Bool
    let showsInactiveOverlay: Bool
    let onSelect: () -> Void
    let onClose: () -> Void
    let onDetach: (() -> Void)?
    let showsCloseButton: Bool

    @State private var isHovered = false

    private let terminalEnvironmentService = AIRuntimeBridgeService()
    private var recoveryIssue: AppModel.TerminalRecoveryIssue? {
        model.terminalRecoveryIssue(for: session.id)
    }

    private var paneBackgroundColor: Color {
        return model.terminalChromeColor
    }

    private var hasPaneControls: Bool {
        showsCloseButton || onDetach != nil
    }

    private var terminalInsets: EdgeInsets {
        let base = CGFloat(10)
        guard hasPaneControls else {
            return EdgeInsets(top: base, leading: base, bottom: base, trailing: base)
        }
        return EdgeInsets(top: base, leading: base, bottom: base, trailing: 56)
    }

    var body: some View {
        ZStack {
            Rectangle()
                .fill(paneBackgroundColor)

            if showsInactiveOverlay && !isFocused {
                Rectangle()
                    .fill(Color(nsColor: model.terminalInactiveDimColor))
            }

            Group {
                if let recoveryIssue {
                    TerminalRecoveryFallbackView(
                        model: model,
                        session: session,
                        issue: recoveryIssue,
                        onRetry: { model.retryTerminalRecovery(session.id) }
                    )
                } else {
                    terminalHost
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(terminalInsets)

            if showsCloseButton || onDetach != nil {
                VStack {
                    HStack {
                        Spacer()

                        HStack(spacing: 6) {
                            if let onDetach {
                                Button(action: onDetach) {
                                    Image(systemName: "square.on.square")
                                        .font(.system(size: 10, weight: .bold))
                                        .foregroundStyle(model.terminalMutedTextColor)
                                        .frame(width: 20, height: 20)
                                        .background(model.terminalChromeColor.opacity(0.96))
                                        .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                                }
                                .buttonStyle(.plain)
                                .appCursor(.pointingHand)
                            }

                            if showsCloseButton {
                                Button(action: onClose) {
                                    Image(systemName: "xmark")
                                        .font(.system(size: 10, weight: .bold))
                                        .foregroundStyle(model.terminalMutedTextColor)
                                        .frame(width: 20, height: 20)
                                        .background(model.terminalChromeColor.opacity(0.96))
                                        .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                                }
                                .buttonStyle(.plain)
                                .appCursor(.pointingHand)
                            }
                        }
                    }

                    Spacer()
                }
                .padding(10)
                .opacity(isHovered || isFocused ? 1 : 0.82)
            }
        }
        .contentShape(Rectangle())
        .onHover { hovering in
            isHovered = hovering
        }
    }

    @ViewBuilder
    private var terminalHost: some View {
        GhosttyTerminalHostView(
            session: session,
            environment: terminalEnvironment(),
            terminalBackgroundPreset: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset,
            terminalFontSize: model.appSettings.terminalFontSize,
            isFocused: isFocused,
            isVisible: isVisible,
            showsInactiveOverlay: showsInactiveOverlay,
            shouldFocus: model.terminalFocusRequestID == session.id,
            onInteraction: onSelect,
            onFocusConsumed: { model.consumeTerminalFocusRequest(session.id) },
            onStartupSucceeded: { model.noteTerminalStartupSucceeded(session.id) },
            onStartupFailure: { detail in model.noteTerminalStartupFailure(session.id, detail: detail) }
        )
        .id("terminal-\(session.id.uuidString)-\(model.terminalRecoveryRetryToken(for: session.id))-ghostty")
    }

    private func terminalEnvironment() -> [(String, String)] {
        terminalEnvironmentService.environmentResolution(for: session, aiSettings: model.appSettings.ai).pairs
    }
}

@MainActor
private final class DetachedTerminalWindow: NSWindow {
    override func sendEvent(_ event: NSEvent) {
        guard event.type == .scrollWheel else {
            super.sendEvent(event)
            return
        }

        let responder = firstResponder
        guard responder == nil || DmuxTerminalBackend.shared.registry.ownsResponder(responder) else {
            super.sendEvent(event)
            return
        }

        if DmuxTerminalBackend.shared.registry.forwardScrollWheel(event, responder: responder) {
            return
        }

        super.sendEvent(event)
    }
}

@MainActor
enum DetachedTerminalWindowPresenter {
    private static var controllers: [UUID: NSWindowController] = [:]
    private static var delegates: [UUID: DetachedTerminalWindowDelegate] = [:]

    static func show(model: AppModel, sessionID: UUID) {
        guard model.isDetachedTerminal(sessionID),
              let session = model.terminalSession(for: sessionID) else {
            return
        }

        if let controller = controllers[sessionID],
           let window = controller.window {
            configureWindow(window, model: model, session: session)
            window.contentMinSize = NSSize(width: 640, height: 360)
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            DispatchQueue.main.async {
                model.requestTerminalFocus(sessionID)
            }
            return
        }

        let window = DetachedTerminalWindow(
            contentRect: NSRect(x: 0, y: 0, width: 980, height: 700),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.detachedTerminal(sessionID)
        configureWindow(window, model: model, session: session)
        window.contentMinSize = NSSize(width: 640, height: 360)
        window.isReleasedWhenClosed = false

        let hosting = NSHostingController(
            rootView: AnyView(
                DetachedTerminalWindowView(model: model, sessionID: sessionID)
            )
        )
        window.contentViewController = hosting
        GhosttyPortalHostRegistry.register(hostView: hosting.view, for: window)

        let delegate = DetachedTerminalWindowDelegate(model: model, sessionID: sessionID, hostView: hosting.view)
        window.delegate = delegate

        let controller = NSWindowController(window: window)
        delegates[sessionID] = delegate
        controllers[sessionID] = controller

        if let parentWindow = NSApp.keyWindow ?? NSApp.mainWindow {
            let origin = NSPoint(x: parentWindow.frame.minX + 36, y: parentWindow.frame.minY - 36)
            window.setFrameOrigin(origin)
        } else {
            window.center()
        }

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        DispatchQueue.main.async {
            model.requestTerminalFocus(sessionID)
        }
    }

    static func dismiss(sessionID: UUID, restoreOnClose: Bool) {
        guard let controller = controllers[sessionID],
              let delegate = delegates[sessionID] else {
            return
        }

        delegate.shouldRestoreOnClose = restoreOnClose
        controller.window?.close()
        controllers[sessionID] = nil
        delegates[sessionID] = nil
    }

    static func clear(sessionID: UUID) {
        controllers[sessionID] = nil
        delegates[sessionID] = nil
    }

    private static func detachedWindowTitle(for session: TerminalSession) -> String {
        if session.title.isEmpty {
            return session.projectName
        }
        return "\(session.projectName) - \(session.title)"
    }

    private static func configureWindow(_ window: NSWindow, model: AppModel, session: TerminalSession) {
        applyStandardWindowChrome(
            window,
            title: detachedWindowTitle(for: session),
            backgroundColor: model.terminalAppearance.backgroundColor
        )
    }
}

@MainActor
private final class DetachedTerminalWindowDelegate: NSObject, NSWindowDelegate {
    weak var model: AppModel?
    let sessionID: UUID
    weak var hostView: NSView?
    var shouldRestoreOnClose = true
    private var hasScheduledRestore = false

    init(model: AppModel, sessionID: UUID, hostView: NSView?) {
        self.model = model
        self.sessionID = sessionID
        self.hostView = hostView
    }

    func windowDidBecomeKey(_ notification: Notification) {
        DispatchQueue.main.async { [weak self] in
            guard let self else { return }
            self.model?.requestTerminalFocus(self.sessionID)
        }
    }

    func windowWillClose(_ notification: Notification) {
        guard hasScheduledRestore == false else {
            return
        }
        hasScheduledRestore = true
        let model = self.model
        let sessionID = self.sessionID
        let shouldRestoreOnClose = self.shouldRestoreOnClose
        if let window = notification.object as? NSWindow, let hostView {
            GhosttyPortalHostRegistry.unregister(hostView: hostView, for: window)
        }
        DetachedTerminalWindowPresenter.clear(sessionID: sessionID)
        guard shouldRestoreOnClose else {
            return
        }
        DispatchQueue.main.async {
            model?.restoreDetachedSession(sessionID)
        }
    }
}

private struct DetachedTerminalWindowView: View {
    let model: AppModel
    let sessionID: UUID

    var body: some View {
        Group {
            if let session = model.terminalSession(for: sessionID) {
                TerminalPaneView(
                    model: model,
                    session: session,
                    terminalBackgroundPreset: model.terminalBackgroundPreset,
                    backgroundColorPreset: model.backgroundColorPreset,
                    isFocused: true,
                    isVisible: true,
                    showsInactiveOverlay: false,
                    onSelect: { model.requestTerminalFocus(session.id) },
                    onClose: {},
                    onDetach: nil,
                    showsCloseButton: false
                )
            } else {
                Rectangle()
                    .fill(model.terminalChromeColor)
            }
        }
        .background(model.terminalChromeColor)
    }
}

private struct TerminalRecoveryFallbackView: View {
    let model: AppModel
    let session: TerminalSession
    let issue: AppModel.TerminalRecoveryIssue
    let onRetry: () -> Void

    private var cardFill: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.035) : Color.white.opacity(0.05)
    }

    private var cardStroke: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.1) : Color.white.opacity(0.1)
    }

    private var accent: Color {
        model.terminalUsesLightBackground ? AppTheme.warning.opacity(0.88) : AppTheme.warning.opacity(0.92)
    }

    var body: some View {
        VStack {
            Spacer()

            VStack(spacing: 14) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 22, weight: .semibold))
                    .foregroundStyle(accent)

                VStack(spacing: 6) {
                    Text(issue.message)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(model.terminalTextColor.opacity(0.92))
                        .multilineTextAlignment(.center)

                    Text(session.cwd)
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(model.terminalMutedTextColor.opacity(0.78))
                        .lineLimit(2)
                        .multilineTextAlignment(.center)

                    Text(issue.detail)
                        .font(.system(size: 12))
                        .foregroundStyle(model.terminalMutedTextColor.opacity(0.82))
                        .multilineTextAlignment(.center)
                }

                HStack(spacing: 10) {
                    Button(action: onRetry) {
                        Label(String(localized: "common.retry", defaultValue: "Retry", bundle: .module), systemImage: "arrow.clockwise")
                            .font(.system(size: 13, weight: .semibold))
                            .padding(.horizontal, 14)
                            .padding(.vertical, 6)
                    }
                    .buttonStyle(.borderedProminent)

                    Button(action: { model.openSelectedProjectInTerminal() }) {
                        Label(String(localized: "open.terminal", defaultValue: "Open in Terminal", bundle: .module), systemImage: "terminal")
                            .font(.system(size: 13, weight: .medium))
                            .padding(.horizontal, 14)
                            .padding(.vertical, 6)
                    }
                    .buttonStyle(.bordered)
                }
            }
            .padding(.horizontal, 22)
            .padding(.vertical, 24)
            .background(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(cardFill)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .stroke(cardStroke, lineWidth: 1)
            )

            Spacer()
        }
        .padding(20)
    }
}
