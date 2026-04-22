import AppKit
import Darwin
import Foundation
import GhosttyTerminal
import QuartzCore

@MainActor
private final class GhosttyTerminalDimOverlayView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}

@MainActor
private final class GhosttyTerminalLoadingShieldView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.001, bounds.contains(point) else {
            return nil
        }
        return self
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }
}

@MainActor
final class GhosttyTerminalContainerView: NSView, TerminalSurfaceFocusDelegate, TerminalSurfaceCloseDelegate {
    private var terminalView = AppTerminalView(frame: .zero)
    private let inactiveOverlayView = GhosttyTerminalDimOverlayView(frame: .zero)
    private let loadingShieldView = GhosttyTerminalLoadingShieldView(frame: .zero)
    private var configuredSession: TerminalSession
    private var configuredEnvironment: [(String, String)]
    private var terminalBackgroundPreset: AppTerminalBackgroundPreset
    private var backgroundColorPreset: AppBackgroundColorPreset
    private var terminalFontSize: Int
    private var onInteraction: (() -> Void)?
    private var onFocusConsumed: (() -> Void)?
    private var onStartupSucceeded: (() -> Void)?
    private var onStartupFailure: ((String) -> Void)?
    private var hasStartedProcess = false
    private var hasReceivedInitialOutput = false
    private var pendingFocusRequest = false
    private var hasReportedStartupFailure = false
    private var pendingStartWorkItem: DispatchWorkItem?
    private var startupWatchdogWorkItem: DispatchWorkItem?
    private var pendingPermanentTearDown = false
    private var lastAppliedFocusedState: Bool?
    private var lastAppliedVisibleState: Bool?
    private var lastShowsInactiveOverlay = false
    private var viewportRefreshScheduled = false
    private var lastViewportRefreshSignature = ""
    private let startupDelay: TimeInterval = 0.18
    private let startupWatchdogDelay: TimeInterval = 3.5
    private let logger = AppDebugLog.shared

    private let processBridge: GhosttyPTYProcessBridge
    private let controller: TerminalController

    var isReadyForInteraction: Bool {
        hasReceivedInitialOutput
    }

    init(
        session: TerminalSession,
        environment: [(String, String)],
        terminalFontSize: Int,
        sessionResources: GhosttyTerminalSessionResources? = nil,
        onInteraction: (() -> Void)?,
        onFocusConsumed: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) {
        configuredSession = session
        configuredEnvironment = environment
        terminalBackgroundPreset = .tokyoNightStorm
        backgroundColorPreset = .automatic
        self.terminalFontSize = terminalFontSize
        self.onInteraction = onInteraction
        self.onFocusConsumed = onFocusConsumed
        self.onStartupSucceeded = onStartupSucceeded
        self.onStartupFailure = onStartupFailure
        if let sessionResources {
            processBridge = sessionResources.processBridge
            controller = sessionResources.controller
            hasStartedProcess = sessionResources.hasStartedProcess
            hasReceivedInitialOutput = sessionResources.hasReceivedInitialOutput
            pendingFocusRequest = sessionResources.pendingFocusRequest
            hasReportedStartupFailure = sessionResources.hasReportedStartupFailure
        } else {
            processBridge = GhosttyPTYProcessBridge(sessionID: session.id)
            controller = Self.makeController(
                backgroundPreset: .tokyoNightStorm,
                backgroundColorPreset: .automatic,
                logger: AppDebugLog.shared
            )
        }
        super.init(frame: .zero)
        setup()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func updateSession(
        _ session: TerminalSession,
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
    ) {
        configuredEnvironment = environment
        self.onInteraction = onInteraction
        self.onFocusConsumed = onFocusConsumed
        self.onStartupSucceeded = onStartupSucceeded
        self.onStartupFailure = onStartupFailure
        lastShowsInactiveOverlay = showsInactiveOverlay

        if self.terminalBackgroundPreset != terminalBackgroundPreset || self.backgroundColorPreset != backgroundColorPreset {
            self.backgroundColorPreset = backgroundColorPreset
            applyTerminalBackgroundPreset(terminalBackgroundPreset)
        }

        if self.terminalFontSize != terminalFontSize {
            self.terminalFontSize = terminalFontSize
            terminalView.configuration = surfaceOptions()
        }

        if configuredSession.id != session.id {
            cancelDeferredLifecycleWork()
            processBridge.terminateProcessTree()
            configuredSession = session
            hasStartedProcess = false
            hasReceivedInitialOutput = false
            hasReportedStartupFailure = false
            pendingFocusRequest = false
            lastAppliedFocusedState = nil
            lastAppliedVisibleState = nil
            lastViewportRefreshSignature = ""
            updateLoadingShieldVisibility()
            terminalView.configuration = surfaceOptions()
        } else {
            configuredSession = session
        }

        let focusChanged = lastAppliedFocusedState != isFocused
        let visibilityChanged = lastAppliedVisibleState != isVisible
        lastAppliedFocusedState = isFocused
        lastAppliedVisibleState = isVisible
        if visibilityChanged {
            terminalView.setSurfaceVisible(isVisible)
        }

        if isVisible {
            scheduleProcessStartIfPossible(reason: isFocused ? "update-focused" : "update-visible")
            scheduleViewportRefresh(reason: "update-session")
        }

        let showsDimOverlay = showsInactiveOverlay && isVisible && !isFocused
        setInactiveOverlay(visible: showsDimOverlay)
        applyEffectiveBackgroundColor()

        if isFocused && focusChanged {
            focusTerminal()
        }
    }

    func applyTerminalBackgroundPreset(_ preset: AppTerminalBackgroundPreset) {
        guard terminalBackgroundPreset != preset else {
            if controller.setTheme(Self.theme(for: preset, backgroundColorPreset: backgroundColorPreset)) == false {
                return
            }
            inactiveOverlayView.layer?.backgroundColor = Self.inactiveOverlayColor(for: preset, backgroundColorPreset: backgroundColorPreset).cgColor
            applyEffectiveBackgroundColor()
            return
        }
        terminalBackgroundPreset = preset
        _ = controller.setTheme(Self.theme(for: preset, backgroundColorPreset: backgroundColorPreset))
        inactiveOverlayView.layer?.backgroundColor = Self.inactiveOverlayColor(for: preset, backgroundColorPreset: backgroundColorPreset).cgColor

        let isFocused = lastAppliedFocusedState ?? false
        let isVisible = lastAppliedVisibleState ?? false
        let showsDimOverlay = lastShowsInactiveOverlay && isVisible && !isFocused
        setInactiveOverlay(visible: showsDimOverlay)
        applyEffectiveBackgroundColor()
    }

    func focusTerminal() {
        guard hasReceivedInitialOutput else {
            pendingFocusRequest = true
            if let window,
               let responder = window.firstResponder,
               GhosttyTerminalRegistry.shared.ownsResponder(responder),
               ownsResponder(responder) == false {
                window.makeFirstResponder(nil)
                GhosttyTerminalRegistry.shared.clearFocusedSession()
            }
            return
        }
        if window?.firstResponder === terminalView {
            GhosttyTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            onFocusConsumed?()
            return
        }
        if window?.makeFirstResponder(terminalView) == true,
           window?.firstResponder === terminalView {
            GhosttyTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            onFocusConsumed?()
        }
    }

    var terminalShellPID: Int32? {
        processBridge.currentShellPID
    }

    var terminalProjectID: UUID {
        configuredSession.projectID
    }

    var terminalProcessInstanceID: String {
        processBridge.processInstanceID
    }

    func terminateProcessTree() {
        processBridge.terminateProcessTree()
    }

    func prepareForPermanentRemoval() {
        logger.log(
            "ghostty-lifecycle",
            "prepare-remove session=\(configuredSession.id.uuidString) instance=\(processBridge.processInstanceID)"
        )
        cancelDeferredLifecycleWork()
        processBridge.terminateProcessTree()
        pendingPermanentTearDown = true
        finalizePermanentTearDownWhenDetached()
    }

    var isTerminalFocused: Bool {
        guard let responder = window?.firstResponder else {
            return false
        }
        if responder === terminalView {
            return true
        }
        if let view = responder as? NSView, view.isDescendant(of: terminalView) {
            return true
        }

        var next: NSResponder? = responder.nextResponder
        while let current = next {
            if current === terminalView {
                return true
            }
            next = current.nextResponder
        }
        return false
    }

    override func layout() {
        super.layout()
        if hasStartedProcess == false {
            scheduleProcessStartIfPossible(reason: "layout")
        }
        scheduleViewportRefresh(reason: "layout")
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            cancelDeferredLifecycleWork()
            lastViewportRefreshSignature = ""
            finalizePermanentTearDownWhenDetached()
            return
        }
        scheduleProcessStartIfPossible(reason: "window-attached")
        scheduleViewportRefresh(reason: "window-attached")
    }

    func sendText(_ text: String) {
        notifyInteraction()
        processBridge.sendText(text)
    }

    func sendInterrupt() {
        notifyInteraction()
        processBridge.sendInterrupt()
    }

    func sendEscape() {
        notifyInteraction()
        processBridge.sendEscape()
    }

    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut) {
        notifyInteraction()
        processBridge.sendEditingShortcut(shortcut)
    }

    func sendNativeCommandArrow(keyCode: UInt16) -> Bool {
        notifyInteraction()
        return processBridge.sendNativeCommandArrow(keyCode: keyCode)
    }

    func forwardKeyDown(_ event: NSEvent) -> Bool {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return false
        }
        notifyInteraction()
        focusTerminal()
        terminalView.keyDown(with: event)
        return true
    }

    func forwardMouseDown(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.mouseDown(with: event)
    }

    func forwardRightMouseDown(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.rightMouseDown(with: event)
    }

    func forwardOtherMouseDown(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.otherMouseDown(with: event)
    }

    func forwardScrollWheel(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.scrollWheel(with: event)
    }

    override func scrollWheel(with event: NSEvent) {
        forwardScrollWheel(event)
    }

    func portalDidUpdateFrame() {
        scheduleViewportRefresh(reason: "portal-frame")
    }

    func ownsResponder(_ responder: NSResponder?) -> Bool {
        guard let responder else {
            return false
        }
        if responder === terminalView {
            return true
        }
        if let view = responder as? NSView, view.isDescendant(of: terminalView) {
            return true
        }
        var next: NSResponder? = responder.nextResponder
        while let current = next {
            if current === terminalView {
                return true
            }
            next = current.nextResponder
        }
        return false
    }

    func terminalDidChangeFocus(_ focused: Bool) {
        if focused {
            notifyInteraction()
            GhosttyTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            onFocusConsumed?()
        }
    }

    func terminalDidClose(processAlive: Bool) {
        logger.log(
            "ghostty-process",
            "surface-closed session=\(configuredSession.id.uuidString) processAlive=\(processAlive)"
        )
    }

    private func setup() {
        wantsLayer = true
        let appearance = Self.resolvedAppearance(
            for: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset
        )
        layer?.backgroundColor = appearance.backgroundColor.cgColor
        layerContentsRedrawPolicy = .duringViewResize

        loadingShieldView.translatesAutoresizingMaskIntoConstraints = false
        loadingShieldView.wantsLayer = true
        loadingShieldView.layer?.backgroundColor = appearance.backgroundColor.cgColor
        inactiveOverlayView.translatesAutoresizingMaskIntoConstraints = false
        inactiveOverlayView.wantsLayer = true
        inactiveOverlayView.layer?.backgroundColor = Self.inactiveOverlayColor(for: terminalBackgroundPreset, backgroundColorPreset: backgroundColorPreset).cgColor
        inactiveOverlayView.isHidden = true

        configureTerminalView(terminalView)
        terminalView.setSurfaceVisible(false)
        addSubview(terminalView)
        addSubview(loadingShieldView)
        addSubview(inactiveOverlayView)
        pinTerminalView(terminalView)
        pinInactiveOverlayView()
        pinLoadingShieldView()

        bindProcessCallbacks()
        updateLoadingShieldVisibility()
    }

    fileprivate func transplantSessionResources() -> GhosttyTerminalSessionResources {
        cancelDeferredLifecycleWork()
        let shouldRefocus = pendingFocusRequest || isTerminalFocused
        processBridge.onFirstOutput = nil
        processBridge.onProcessTerminated = nil

        tearDownTerminalView()

        return GhosttyTerminalSessionResources(
            processBridge: processBridge,
            controller: controller,
            hasStartedProcess: hasStartedProcess,
            hasReceivedInitialOutput: hasReceivedInitialOutput,
            pendingFocusRequest: shouldRefocus,
            hasReportedStartupFailure: hasReportedStartupFailure
        )
    }

    private func bindProcessCallbacks() {
        processBridge.onFirstOutput = { [weak self] in
            self?.markTerminalReady(reason: "initial-output")
        }
        processBridge.onProcessTerminated = { [weak self] exitCode in
            self?.handleProcessTermination(exitCode: exitCode)
        }
    }

    private func cancelDeferredLifecycleWork() {
        pendingStartWorkItem?.cancel()
        pendingStartWorkItem = nil
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
    }

    private func scheduleProcessStartIfPossible(reason: String) {
        guard hasStartedProcess == false else {
            return
        }
        guard window != nil, bounds.width > 0, bounds.height > 0 else {
            return
        }
        guard pendingStartWorkItem == nil else {
            return
        }

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.pendingStartWorkItem = nil
            self.startProcessIfPossible(trigger: reason)
        }
        pendingStartWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + startupDelay, execute: workItem)
    }

    private func startProcessIfPossible(trigger: String) {
        guard hasStartedProcess == false else {
            return
        }
        guard window != nil, bounds.width > 0, bounds.height > 0 else {
            scheduleProcessStartIfPossible(reason: "\(trigger)-awaiting-layout")
            return
        }

        let shell = configuredSession.shell
        let shellName = URL(fileURLWithPath: shell).lastPathComponent
        hasStartedProcess = true
        hasReportedStartupFailure = false
        processBridge.resetOutputObservation()
        terminalView.configuration = surfaceOptions()
        processBridge.start(
            shell: shell,
            shellName: shellName,
            command: configuredSession.command,
            cwd: configuredSession.cwd,
            environment: configuredEnvironment
        )

        installStartupWatchdog()
        logger.log(
            "ghostty-process",
            "start session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) reason=\(trigger) shell=\(shell)"
        )
    }

    private func installStartupWatchdog() {
        startupWatchdogWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.startupWatchdogWorkItem = nil
            guard self.hasReceivedInitialOutput == false else {
                return
            }
            guard self.terminalShellPID == nil else {
                self.markTerminalReady(reason: "alive-without-output")
                return
            }
            self.reportStartupFailureIfNeeded("shell pid not available after \(Int(self.startupWatchdogDelay * 1000))ms")
        }
        startupWatchdogWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + startupWatchdogDelay, execute: workItem)
    }

    private func markTerminalReady(reason: String) {
        guard hasReceivedInitialOutput == false else {
            return
        }
        hasReceivedInitialOutput = true
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
        hasReportedStartupFailure = false
        updateLoadingShieldVisibility()
        logger.log(
            "ghostty-process",
            "ready session=\(configuredSession.id.uuidString) reason=\(reason)"
        )
        scheduleViewportRefresh(reason: "terminal-ready")
        onStartupSucceeded?()
        if pendingFocusRequest {
            pendingFocusRequest = false
            focusTerminal()
        }
    }

    private func handleProcessTermination(exitCode: Int32?) {
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
        if hasReceivedInitialOutput == false {
            hasReceivedInitialOutput = true
            updateLoadingShieldVisibility()
            reportStartupFailureIfNeeded(
                "process exited before terminal became ready (exit=\(exitCode.map(String.init) ?? "nil"))"
            )
        }
        pendingFocusRequest = false
    }

    private func reportStartupFailureIfNeeded(_ detail: String) {
        guard hasReportedStartupFailure == false else {
            return
        }
        hasReportedStartupFailure = true
        logger.log(
            "terminal-recovery",
            "ghostty-container-failed session=\(configuredSession.id.uuidString) detail=\(detail)"
        )
        onStartupFailure?(detail)
    }

    private func updateLoadingShieldVisibility() {
        loadingShieldView.isHidden = hasReceivedInitialOutput
        terminalView.alphaValue = hasReceivedInitialOutput ? 1 : 0
        if hasReceivedInitialOutput == false,
           let responder = window?.firstResponder,
           ownsResponder(responder) {
            window?.makeFirstResponder(nil)
        }
    }

    private func notifyInteraction() {
        onInteraction?()
    }

    private func scheduleViewportRefresh(reason: String) {
        guard window != nil, bounds.width > 1, bounds.height > 1 else {
            return
        }
        guard viewportRefreshScheduled == false else {
            return
        }

        viewportRefreshScheduled = true
        DispatchQueue.main.async { [weak self] in
            guard let self else {
                return
            }
            self.viewportRefreshScheduled = false
            self.performViewportRefreshIfNeeded(reason: reason)
        }
    }

    private func performViewportRefreshIfNeeded(reason: String) {
        guard window != nil, bounds.width > 1, bounds.height > 1 else {
            lastViewportRefreshSignature = ""
            return
        }

        let signature = "\(window?.windowNumber ?? -1)|\(Int(bounds.width.rounded(.down)))x\(Int(bounds.height.rounded(.down)))|started=\(hasStartedProcess)|ready=\(hasReceivedInitialOutput)"
        guard signature != lastViewportRefreshSignature else {
            return
        }
        lastViewportRefreshSignature = signature

        needsLayout = true
        layoutSubtreeIfNeeded()
        if terminalView.frame.size != bounds.size {
            terminalView.frame = bounds
        }
        terminalView.needsLayout = true
        terminalView.layoutSubtreeIfNeeded()

        logger.log(
            "ghostty-metrics",
            "viewport-refresh session=\(configuredSession.id.uuidString) reason=\(reason) size=\(Int(bounds.width.rounded(.down)))x\(Int(bounds.height.rounded(.down))) window=\(window?.windowNumber ?? -1)"
        )
    }

    private func tearDownTerminalView() {
        terminalView.setSurfaceVisible(false)
        if window?.firstResponder === terminalView {
            window?.makeFirstResponder(nil)
        }
        terminalView.delegate = nil
        terminalView.controller = nil
    }

    private func finalizePermanentTearDownWhenDetached() {
        guard pendingPermanentTearDown else {
            return
        }

        guard window == nil else {
            DispatchQueue.main.async { [weak self] in
                self?.finalizePermanentTearDownWhenDetached()
            }
            return
        }

        pendingPermanentTearDown = false
        tearDownTerminalView()
    }

    func prepareForPointerInteraction() {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        notifyInteraction()
        focusTerminal()
    }

    private func configureTerminalView(_ view: AppTerminalView) {
        view.translatesAutoresizingMaskIntoConstraints = false
        view.delegate = self
        view.controller = controller
        view.configuration = surfaceOptions()
    }

    private func pinTerminalView(_ view: AppTerminalView) {
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: leadingAnchor),
            view.trailingAnchor.constraint(equalTo: trailingAnchor),
            view.topAnchor.constraint(equalTo: topAnchor),
            view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    private func pinLoadingShieldView() {
        NSLayoutConstraint.activate([
            loadingShieldView.leadingAnchor.constraint(equalTo: leadingAnchor),
            loadingShieldView.trailingAnchor.constraint(equalTo: trailingAnchor),
            loadingShieldView.topAnchor.constraint(equalTo: topAnchor),
            loadingShieldView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    private func pinInactiveOverlayView() {
        NSLayoutConstraint.activate([
            inactiveOverlayView.leadingAnchor.constraint(equalTo: leadingAnchor),
            inactiveOverlayView.trailingAnchor.constraint(equalTo: trailingAnchor),
            inactiveOverlayView.topAnchor.constraint(equalTo: topAnchor),
            inactiveOverlayView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    private static func makeController(
        backgroundPreset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset,
        logger: AppDebugLog
    ) -> TerminalController {
        let resolvedConfig = GhosttyEmbeddedConfig.resolvedControllerConfig()
        logger.log(
            "ghostty-config",
            "source=\(resolvedConfig.prefersUserConfig ? "user" : "embedded") path=\(resolvedConfig.userConfigDescription ?? Self.configSourceDescription(resolvedConfig.configSource))"
        )
        return TerminalController(
            configSource: resolvedConfig.configSource,
            theme: Self.theme(for: backgroundPreset, backgroundColorPreset: backgroundColorPreset),
            terminalConfiguration: GhosttyEmbeddedConfig.terminalConfiguration()
        )
    }

    private static func resolvedAppearance(
        for preset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset
    ) -> AppEffectiveTerminalAppearance {
        let automaticAppearance = GhosttyEmbeddedConfig.resolvedAutomaticTerminalAppearance(
            prefersDarkAppearance: true
        )
        return preset.effectiveAppearance(
            backgroundColorPreset: backgroundColorPreset,
            automaticAppearance: automaticAppearance
        )
    }

    private static func theme(for preset: AppTerminalBackgroundPreset, backgroundColorPreset: AppBackgroundColorPreset) -> TerminalTheme {
        let appearance = resolvedAppearance(
            for: preset,
            backgroundColorPreset: backgroundColorPreset
        )
        return theme(from: appearance)
    }

    private static func theme(from appearance: AppEffectiveTerminalAppearance) -> TerminalTheme {
        let configuration = TerminalConfiguration { builder in
            builder.withBackground(appearance.backgroundColor.ghosttyHexString)
            builder.withForeground(appearance.foregroundColor.ghosttyHexString)
            builder.withSelectionBackground(appearance.selectionBackgroundColor.ghosttyHexString)
            builder.withSelectionForeground(appearance.selectionForegroundColor.ghosttyHexString)
            builder.withCursorColor(appearance.cursorColor.ghosttyHexString)
            builder.withCursorText(appearance.cursorTextColor.ghosttyHexString)
            builder.withBoldColor(appearance.foregroundColor.ghosttyHexString)
            builder.withMinimumContrast(appearance.minimumContrast)
            builder.withBackgroundOpacity(1.0)

            for (index, color) in appearance.paletteHexStrings.enumerated() {
                builder.withPalette(index, color: color)
            }
        }
        return TerminalTheme(light: configuration, dark: configuration)
    }

    private static func inactiveOverlayColor(for preset: AppTerminalBackgroundPreset, backgroundColorPreset: AppBackgroundColorPreset) -> NSColor {
        resolvedAppearance(
            for: preset,
            backgroundColorPreset: backgroundColorPreset
        ).inactiveDimColor
    }

    private func applyEffectiveBackgroundColor() {
        let backgroundColor = effectiveBackgroundColor()
        layer?.backgroundColor = backgroundColor.cgColor
        loadingShieldView.layer?.backgroundColor = backgroundColor.cgColor
    }

    private func effectiveBackgroundColor() -> NSColor {
        return Self.resolvedAppearance(
            for: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset
        ).backgroundColor
    }

    private func setInactiveOverlay(visible: Bool) {
        inactiveOverlayView.isHidden = !visible
    }

    private func surfaceOptions() -> TerminalSurfaceOptions {
        TerminalSurfaceOptions(
            backend: .inMemory(processBridge.terminalSession),
            fontSize: Float(terminalFontSize),
            workingDirectory: configuredSession.cwd,
            context: .window
        )
    }

    private static func configSourceDescription(_ source: TerminalController.ConfigSource) -> String {
        switch source {
        case .none:
            return "none"
        case let .file(path):
            return path
        case let .generated(contents):
            return "generated:\(contents.count)"
        }
    }
}
