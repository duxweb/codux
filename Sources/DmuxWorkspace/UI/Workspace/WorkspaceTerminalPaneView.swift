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
    let onTaskMemos: () -> Void
    let showsCloseButton: Bool

    @State private var isHovered = false

    private let terminalEnvironmentService = AIRuntimeBridgeService()
    private var recoveryIssue: AppModel.TerminalRecoveryIssue? {
        model.terminalRecoveryIssue(for: session.id)
    }

    private var paneBackgroundColor: Color {
        return model.terminalChromeColor
    }

    private var terminalInsets: EdgeInsets {
        let base = CGFloat(10)
        return EdgeInsets(top: base, leading: base, bottom: base, trailing: base)
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
                if session.launchMode == .agent {
                    AgentPaneView(
                        model: model,
                        session: session,
                        isFocused: isFocused,
                        isVisible: isVisible,
                        showsInactiveOverlay: showsInactiveOverlay,
                        onSelect: onSelect,
                        onClose: onClose,
                        showsCloseButton: showsCloseButton
                    )
                } else if let recoveryIssue {
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

            if showsPaneControls {
                GhosttyTerminalPortalNativeAccessoryView(
                    isVisible: isVisible,
                    preferredSize: NSSize(width: paneControlsPreferredWidth, height: 52),
                    makeAccessoryView: makePaneControlsAccessoryView,
                    updateAccessoryView: updatePaneControlsAccessoryView
                )
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topTrailing)
            }
        }
        .contentShape(Rectangle())
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var showsPaneControls: Bool {
        session.launchMode != .agent && (showsCloseButton || onDetach != nil)
    }

    private var paneControlsPreferredWidth: CGFloat {
        CGFloat(34 + (onDetach == nil ? 0 : 26) + (showsCloseButton ? 26 : 0))
    }

    private func makePaneControlsAccessoryView() -> NSView {
        let accessory = TerminalPaneControlsAccessoryView()
        accessory.configure(
            mutedTextColor: NSColor(model.terminalMutedTextColor),
            chromeColor: NSColor(model.terminalChromeColor),
            taskMemoBackgroundColor: NSColor(taskMemoButtonBackground),
            showsDetach: onDetach != nil,
            showsClose: showsCloseButton,
            taskMemoQueuedCount: taskMemoQueuedCount,
            onTaskMemos: onTaskMemos,
            onDetach: onDetach,
            onClose: onClose
        )
        return accessory
    }

    private func updatePaneControlsAccessoryView(_ view: NSView) {
        guard let accessory = view as? TerminalPaneControlsAccessoryView else {
            return
        }
        accessory.configure(
            mutedTextColor: NSColor(model.terminalMutedTextColor),
            chromeColor: NSColor(model.terminalChromeColor),
            taskMemoBackgroundColor: NSColor(taskMemoButtonBackground),
            showsDetach: onDetach != nil,
            showsClose: showsCloseButton,
            taskMemoQueuedCount: taskMemoQueuedCount,
            onTaskMemos: onTaskMemos,
            onDetach: onDetach,
            onClose: onClose
        )
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
            onStartupFailure: { detail in model.noteTerminalStartupFailure(session.id, detail: detail) },
            onLoadingStateChanged: { isLoading in
                model.noteTerminalLoadingState(session.id, isLoading: isLoading)
            }
        )
        .id("terminal-\(session.id.uuidString)-\(model.terminalRecoveryRetryToken(for: session.id))-ghostty")
    }

    private func terminalEnvironment() -> [(String, String)] {
        terminalEnvironmentService.environmentResolution(for: session, aiSettings: model.appSettings.ai).pairs
    }

    private var taskMemoQueuedCount: Int {
        model.taskMemoCounts(projectID: session.projectID, sessionID: session.id).queued
    }

    private var taskMemoButtonBackground: Color {
        if model.rightPanel == .taskMemos && model.taskMemoFocusedSessionID == session.id {
            return AppTheme.focus.opacity(0.22)
        }
        if taskMemoQueuedCount > 0 {
            return AppTheme.success.opacity(0.2)
        }
        return model.terminalChromeColor.opacity(0.96)
    }
}

@MainActor
private final class TerminalPaneControlsAccessoryView: NSView {
    private let taskMemoButton = TerminalPaneControlButton(symbolName: "checklist")
    private let detachButton = TerminalPaneControlButton(symbolName: "square.on.square")
    private let closeButton = TerminalPaneControlButton(symbolName: "xmark")

    private var showsDetach = false
    private var showsClose = false
    private var onTaskMemos: (() -> Void)?
    private var onDetach: (() -> Void)?
    private var onClose: (() -> Void)?

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = false

        addSubview(taskMemoButton)
        addSubview(detachButton)
        addSubview(closeButton)

        taskMemoButton.actionHandler = { [weak self] in self?.onTaskMemos?() }
        detachButton.actionHandler = { [weak self] in self?.onDetach?() }
        closeButton.actionHandler = { [weak self] in self?.onClose?() }
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func resetCursorRects() {
        super.resetCursorRects()
        for button in [taskMemoButton, detachButton, closeButton] where !button.isHidden {
            addCursorRect(button.frame, cursor: .pointingHand)
        }
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        layoutSubtreeIfNeeded()
        for button in [taskMemoButton, detachButton, closeButton] where !button.isHidden {
            if button.frame.contains(point) {
                return button
            }
        }
        return nil
    }

    func configure(
        mutedTextColor: NSColor,
        chromeColor: NSColor,
        taskMemoBackgroundColor: NSColor,
        showsDetach: Bool,
        showsClose: Bool,
        taskMemoQueuedCount: Int,
        onTaskMemos: @escaping () -> Void,
        onDetach: (() -> Void)?,
        onClose: @escaping () -> Void
    ) {
        self.showsDetach = showsDetach
        self.showsClose = showsClose
        self.onTaskMemos = onTaskMemos
        self.onDetach = onDetach
        self.onClose = onClose

        let hasQueuedTaskMemos = taskMemoQueuedCount > 0
        let taskMemoAccentColor = NSColor(calibratedRed: 63 / 255, green: 193 / 255, blue: 123 / 255, alpha: 1)
        taskMemoButton.configure(
            foregroundColor: hasQueuedTaskMemos ? taskMemoAccentColor : mutedTextColor,
            backgroundColor: taskMemoBackgroundColor,
            borderColor: hasQueuedTaskMemos ? taskMemoAccentColor.withAlphaComponent(0.72) : nil
        )
        detachButton.configure(foregroundColor: mutedTextColor, backgroundColor: chromeColor.withAlphaComponent(0.96))
        closeButton.configure(foregroundColor: mutedTextColor, backgroundColor: chromeColor.withAlphaComponent(0.96))

        detachButton.isHidden = !showsDetach
        closeButton.isHidden = !showsClose
        needsLayout = true
        window?.invalidateCursorRects(for: self)
    }

    override func layout() {
        super.layout()

        let buttonSize = CGSize(width: 20, height: 20)
        let spacing = CGFloat(6)
        let topPadding = CGFloat(10)
        let trailingPadding = CGFloat(10)
        var x = bounds.maxX - trailingPadding
        let y = bounds.maxY - topPadding - buttonSize.height

        let visibleButtons = [closeButton, detachButton, taskMemoButton].filter { !$0.isHidden }
        for button in visibleButtons {
            x -= buttonSize.width
            button.frame = CGRect(origin: CGPoint(x: x, y: y), size: buttonSize)
            x -= spacing
        }
    }
}

@MainActor
private final class TerminalPaneControlButton: NSButton {
    let symbolName: String
    var actionHandler: (() -> Void)?

    init(symbolName: String) {
        self.symbolName = symbolName
        super.init(frame: .zero)
        let symbolConfiguration = NSImage.SymbolConfiguration(pointSize: 9, weight: .semibold)
        image = NSImage(systemSymbolName: symbolName, accessibilityDescription: nil)?
            .withSymbolConfiguration(symbolConfiguration)
        imageScaling = .scaleProportionallyDown
        imagePosition = .imageOnly
        isBordered = false
        bezelStyle = .regularSquare
        setButtonType(.momentaryChange)
        wantsLayer = true
        layer?.cornerRadius = 7
        layer?.masksToBounds = true
        target = self
        action = #selector(handlePress)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    func configure(foregroundColor: NSColor, backgroundColor: NSColor, borderColor: NSColor? = nil) {
        contentTintColor = foregroundColor
        layer?.backgroundColor = backgroundColor.cgColor
        layer?.borderColor = borderColor?.cgColor
        layer?.borderWidth = borderColor == nil ? 0 : 0.8
    }

    @objc private func handlePress() {
        actionHandler?()
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
                    onTaskMemos: { model.openTaskMemoPanel(for: session.id) },
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
