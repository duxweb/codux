import AppKit
import Darwin
import SwiftTerm
import SwiftUI
#if canImport(MetalKit)
import MetalKit
#endif

extension Notification.Name {
    static let dmuxTerminalFocusDidChange = Notification.Name("dmux.terminalFocusDidChange")
    static let dmuxTerminalOutputDidChange = Notification.Name("dmux.terminalOutputDidChange")
    static let dmuxTerminalInterruptDidSend = Notification.Name("dmux.terminalInterruptDidSend")
}

private final class SwiftTermOutputEventEmitter: @unchecked Sendable {
    static let shared = SwiftTermOutputEventEmitter()

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

private final class DmuxLocalProcessTerminalView: LocalProcessTerminalView {
    var sessionID: UUID?
    var onFirstOutput: (() -> Void)?
    private var hasObservedOutput = false

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func dataReceived(slice: ArraySlice<UInt8>) {
        if let sessionID, !slice.isEmpty {
            SwiftTermOutputEventEmitter.shared.noteOutput(sessionID: sessionID)
        }
        if !slice.isEmpty, hasObservedOutput == false {
            hasObservedOutput = true
            DispatchQueue.main.async { [weak self] in
                self?.onFirstOutput?()
            }
        }
        super.dataReceived(slice: slice)
    }

    override func send(source: TerminalView, data: ArraySlice<UInt8>) {
        if let sessionID,
           data.contains(0x03) {
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .dmuxTerminalInterruptDidSend,
                    object: sessionID
                )
            }
        }
        super.send(source: source, data: data)
    }

    func resetOutputObservation() {
        hasObservedOutput = false
    }
}

private extension NSColor {
    var swiftTermColor: SwiftTerm.Color {
        let converted = usingColorSpace(.deviceRGB) ?? self
        return SwiftTerm.Color(
            red: UInt16(max(0, min(65535, Int(converted.redComponent * 65535.0)))),
            green: UInt16(max(0, min(65535, Int(converted.greenComponent * 65535.0)))),
            blue: UInt16(max(0, min(65535, Int(converted.blueComponent * 65535.0))))
        )
    }
}

@MainActor
struct SwiftTermTerminalHostView: NSViewRepresentable {
    let session: TerminalSession
    let environment: [(String, String)]
    let terminalBackgroundPreset: AppTerminalBackgroundPreset
    let terminalFontSize: Int
    let useMetalRendering: Bool
    let gpuMode: AppTerminalGPUMode
    let isFocused: Bool
    let isVisible: Bool
    let prefersReducedMemoryMode: Bool
    let shouldFocus: Bool
    var onInteraction: (() -> Void)? = nil
    var onFocusConsumed: (() -> Void)? = nil
    var onStartupSucceeded: (() -> Void)? = nil
    var onStartupFailure: ((String) -> Void)? = nil

    func makeCoordinator() -> Coordinator {
        Coordinator(sessionID: session.id)
    }

    func makeNSView(context: Context) -> SwiftTermTerminalContainerView {
        AppDebugLog.shared.log(
            "startup-ui",
            "terminal-host make session=\(session.id.uuidString) project=\(session.projectID.uuidString)"
        )
        let view = SwiftTermTerminalRegistry.shared.containerView(
            for: session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            terminalFontSize: terminalFontSize,
            useMetalRendering: useMetalRendering,
            gpuMode: gpuMode,
            isFocused: isFocused,
            isVisible: isVisible,
            prefersReducedMemoryMode: prefersReducedMemoryMode,
            onInteraction: onInteraction,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        context.coordinator.containerView = view
        if shouldFocus {
            DispatchQueue.main.async {
                view.focusTerminal()
                onFocusConsumed?()
            }
        }
        return view
    }

    func updateNSView(_ nsView: SwiftTermTerminalContainerView, context: Context) {
        nsView.updateSession(
            session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            terminalFontSize: terminalFontSize,
            useMetalRendering: useMetalRendering,
            gpuMode: gpuMode,
            isFocused: isFocused,
            isVisible: isVisible,
            prefersReducedMemoryMode: prefersReducedMemoryMode,
            onInteraction: onInteraction,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        if shouldFocus {
            DispatchQueue.main.async {
                nsView.focusTerminal()
                onFocusConsumed?()
            }
        }
    }

    final class Coordinator {
        let sessionID: UUID
        weak var containerView: SwiftTermTerminalContainerView?

        init(sessionID: UUID) {
            self.sessionID = sessionID
        }
    }
}

@MainActor
final class SwiftTermTerminalContainerView: NSView {
    private let terminalView: DmuxLocalProcessTerminalView
    private let loadingShieldView = NSView(frame: .zero)
    private var configuredSession: TerminalSession
    private var configuredEnvironment: [(String, String)]
    private var terminalBackgroundPreset: AppTerminalBackgroundPreset
    private var terminalFontSize: Int = 14
    private var preferredMetalRenderingEnabled = true
    private var gpuMode: AppTerminalGPUMode = .balanced
    private var isFocusedTerminal = false
    private var isVisibleTerminal = true
    private var prefersReducedMemoryMode = false
#if canImport(MetalKit)
    private var lastAppliedMetalBufferingMode: MetalBufferingMode?
#endif
    private let processInstanceID = UUID().uuidString.lowercased()
    private let processDelegateProxy = SwiftTermProcessDelegateProxy()
    private var onInteraction: (() -> Void)?
    private var onStartupSucceeded: (() -> Void)?
    private var onStartupFailure: ((String) -> Void)?
    private var hasAppliedTheme = false
    private var hasStartedProcess = false
    private var hasReceivedInitialOutput = false
    private var pendingFocusRequest = false
    private var lastSyncedTerminalSize: CGSize = .zero
    private var pendingStartWorkItem: DispatchWorkItem?
    private var startupWatchdogWorkItem: DispatchWorkItem?
    private var hasReportedStartupFailure = false
    private var acceleratedRenderingUnavailable = false
    private var temporarilyPrefersCoreGraphicsDuringStructuralResize = false
    private var structuralResizeRestoreWorkItem: DispatchWorkItem?
    private var interactionEventMonitors: [Any] = []
    private var selectionAutoscrollTimer: Timer?
    private var selectionAutoscrollDelta: Int = 0
    private let logger = AppDebugLog.shared
    private let debugAIFocus = ProcessInfo.processInfo.environment["DMUX_DEBUG_AI_FOCUS"] == "1"
    private let startupDelay: TimeInterval = 0.22
    private let startupWatchdogDelay: TimeInterval = 3.5
    init(
        session: TerminalSession,
        environment: [(String, String)],
        terminalFontSize: Int,
        onInteraction: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) {
        self.configuredSession = session
        self.configuredEnvironment = environment
        self.terminalBackgroundPreset = .obsidian
        self.terminalFontSize = terminalFontSize
        self.terminalView = DmuxLocalProcessTerminalView(frame: .zero)
        self.onInteraction = onInteraction
        self.onStartupSucceeded = onStartupSucceeded
        self.onStartupFailure = onStartupFailure
        super.init(frame: .zero)
        logger.log(
            "startup-ui",
            "terminal-container init session=\(session.id.uuidString) project=\(session.projectID.uuidString) cwd=\(session.cwd)"
        )
        setup()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func updateSession(
        _ session: TerminalSession,
        environment: [(String, String)],
        terminalBackgroundPreset: AppTerminalBackgroundPreset,
        terminalFontSize: Int,
        useMetalRendering: Bool,
        gpuMode: AppTerminalGPUMode,
        isFocused: Bool,
        isVisible: Bool,
        prefersReducedMemoryMode: Bool,
        onInteraction: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) {
        configuredEnvironment = environment
        if self.terminalBackgroundPreset != terminalBackgroundPreset {
            applyTheme(terminalBackgroundPreset)
        }
        if self.terminalFontSize != terminalFontSize {
            self.terminalFontSize = terminalFontSize
            terminalView.font = .monospacedSystemFont(ofSize: CGFloat(terminalFontSize), weight: .regular)
        }

        let renderTuningChanged =
            renderSettleDelayMs(
                for: gpuMode,
                isFocused: isFocused,
                isVisible: isVisible,
                prefersReducedMemoryMode: prefersReducedMemoryMode
            ) != terminalView.syncSequenceSettleMs
        let accelerationConfigChanged =
            preferredMetalRenderingEnabled != useMetalRendering
            || self.gpuMode != gpuMode
            || isFocusedTerminal != isFocused
            || isVisibleTerminal != isVisible
            || self.prefersReducedMemoryMode != prefersReducedMemoryMode

        preferredMetalRenderingEnabled = useMetalRendering
        self.gpuMode = gpuMode
        isFocusedTerminal = isFocused
        isVisibleTerminal = isVisible
        self.prefersReducedMemoryMode = prefersReducedMemoryMode
        self.onInteraction = onInteraction
        self.onStartupSucceeded = onStartupSucceeded
        self.onStartupFailure = onStartupFailure

        if renderTuningChanged {
            applyTerminalRenderTuning()
        }
        if accelerationConfigChanged {
            configureAcceleratedRenderingIfPossible()
        }
        syncTerminalSizeIfNeeded(reason: "session-update")
        guard configuredSession.id != session.id else {
            scheduleProcessStartIfPossible(reason: "update-same-session")
            return
        }
        pendingStartWorkItem?.cancel()
        pendingStartWorkItem = nil
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
        SwiftTermOutputEventEmitter.shared.clear(sessionID: configuredSession.id)
        configuredSession = session
        terminalView.sessionID = session.id
        terminalView.resetOutputObservation()
        lastSyncedTerminalSize = .zero
        hasReceivedInitialOutput = false
        pendingFocusRequest = false
        hasReportedStartupFailure = false
        updateLoadingShieldVisibility()
        terminalView.terminate()
        hasStartedProcess = false
        logger.log(
            "startup-ui",
            "terminal-container rebind session=\(session.id.uuidString) project=\(session.projectID.uuidString)"
        )
        scheduleProcessStartIfPossible(reason: "update-new-session")
    }

    func beginStructuralResizeTransition(duration: TimeInterval = 0.22) {
        structuralResizeRestoreWorkItem?.cancel()

        temporarilyPrefersCoreGraphicsDuringStructuralResize = true
        configureAcceleratedRenderingIfPossible()

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.structuralResizeRestoreWorkItem = nil
            self.temporarilyPrefersCoreGraphicsDuringStructuralResize = false
            self.configureAcceleratedRenderingIfPossible()
        }
        structuralResizeRestoreWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + duration, execute: workItem)
    }

    func focusTerminal() {
        guard hasReceivedInitialOutput else {
            pendingFocusRequest = true
            logger.log("terminal-focus", "defer session=\(configuredSession.id.uuidString) reason=waiting-for-output")
            return
        }
        if window?.firstResponder === terminalView {
            SwiftTermTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            return
        }
        window?.makeFirstResponder(terminalView)
        SwiftTermTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
    }

    var terminalShellPID: Int32? {
        let pid = terminalView.process.shellPid
        return pid > 0 ? pid : nil
    }

    var terminalProjectID: UUID {
        configuredSession.projectID
    }

    var terminalProcessInstanceID: String {
        processInstanceID
    }

    func terminateProcessTree() {
        let shellPID = terminalShellPID ?? terminalView.process.shellPid
        resetTransientTerminalModes()
        guard shellPID > 0 else {
            terminalView.terminate()
            return
        }

        // forkpty children are normally session/process-group leaders. Kill the whole
        // process group first so spawned AI CLIs do not survive app/session teardown.
        kill(-shellPID, SIGTERM)
        kill(shellPID, SIGTERM)
        terminalView.terminate()

        DispatchQueue.global(qos: .utility).asyncAfter(deadline: .now() + 1.0) {
            guard kill(shellPID, 0) == 0 else {
                return
            }
            kill(-shellPID, SIGKILL)
            kill(shellPID, SIGKILL)
        }
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

    private func setup() {
        wantsLayer = true
        layerContentsRedrawPolicy = .duringViewResize
        applyTheme(.obsidian)
        logger.log(
            "startup-ui",
            "terminal-container setup session=\(configuredSession.id.uuidString)"
        )

        terminalView.translatesAutoresizingMaskIntoConstraints = false
        terminalView.layerContentsRedrawPolicy = .duringViewResize
        terminalView.processDelegate = processDelegateProxy
        terminalView.caretColor = .systemGray
        terminalView.terminal.setCursorStyle(.blinkBar)
        terminalView.optionAsMetaKey = true
        terminalView.allowMouseReporting = false
        terminalView.font = .monospacedSystemFont(ofSize: CGFloat(terminalFontSize), weight: .regular)
        terminalView.disableFullRedrawOnAnyChanges = true
        applyTerminalRenderTuning()
        terminalView.onFirstOutput = { [weak self] in
            self?.handleInitialTerminalOutput()
        }

        loadingShieldView.translatesAutoresizingMaskIntoConstraints = false
        loadingShieldView.wantsLayer = true

        processDelegateProxy.onProcessTerminated = { [weak self] exitCode in
            self?.handleProcessTermination(exitCode: exitCode)
        }

        if let scrollView = terminalView.enclosingScrollView {
            scrollView.drawsBackground = false
        }

        addSubview(terminalView)
        addSubview(loadingShieldView)

        NSLayoutConstraint.activate([
            terminalView.leadingAnchor.constraint(equalTo: leadingAnchor),
            terminalView.trailingAnchor.constraint(equalTo: trailingAnchor),
            terminalView.topAnchor.constraint(equalTo: topAnchor),
            terminalView.bottomAnchor.constraint(equalTo: bottomAnchor),
            loadingShieldView.leadingAnchor.constraint(equalTo: leadingAnchor),
            loadingShieldView.trailingAnchor.constraint(equalTo: trailingAnchor),
            loadingShieldView.topAnchor.constraint(equalTo: topAnchor),
            loadingShieldView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
        updateLoadingShieldVisibility()
    }

    private func applyTerminalRenderTuning() {
        let settleMs = renderSettleDelayMs(
            for: gpuMode,
            isFocused: isFocusedTerminal,
            isVisible: isVisibleTerminal,
            prefersReducedMemoryMode: prefersReducedMemoryMode
        )
        guard terminalView.syncSequenceSettleMs != settleMs else {
            return
        }

        terminalView.syncSequenceSettleMs = settleMs
    }

    private func renderSettleDelayMs(
        for mode: AppTerminalGPUMode,
        isFocused: Bool,
        isVisible: Bool,
        prefersReducedMemoryMode: Bool
    ) -> Int {
        switch mode {
        case .highPerformance:
            // Keep streaming output responsive, but stop forcing a more aggressive
            // redraw cadence than the terminal library's default.
            return 12
        case .balanced:
            return 16
        case .memorySaver:
            if isVisible && isFocused && prefersReducedMemoryMode == false {
                return 16
            }
            return 24
        }
    }

    private func resetTransientTerminalModes() {
        terminalView.getTerminal().feed(text: "\u{1b}[<u")
    }

    private func applyTheme(_ preset: AppTerminalBackgroundPreset) {
        guard hasAppliedTheme == false || terminalBackgroundPreset != preset else {
            return
        }
        hasAppliedTheme = true
        terminalBackgroundPreset = preset
        layer?.backgroundColor = preset.backgroundColor.cgColor
        terminalView.nativeBackgroundColor = preset.backgroundColor
        terminalView.nativeForegroundColor = preset.foregroundColor
        terminalView.getTerminal().backgroundColor = preset.backgroundColor.swiftTermColor
        terminalView.getTerminal().foregroundColor = preset.foregroundColor.swiftTermColor
        loadingShieldView.layer?.backgroundColor = preset.backgroundColor.cgColor
        needsDisplay = true
        terminalView.needsDisplay = true
    }

    override func layout() {
        super.layout()
        terminalView.needsLayout = true
        if hasStartedProcess {
            syncTerminalSizeIfNeeded(reason: "layout")
        } else {
            scheduleProcessStartIfPossible(reason: "layout")
        }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            pendingStartWorkItem?.cancel()
            pendingStartWorkItem = nil
            startupWatchdogWorkItem?.cancel()
            startupWatchdogWorkItem = nil
            removeInteractionEventMonitors()
            return
        }
        installInteractionEventMonitorsIfNeeded()
        configureAcceleratedRenderingIfPossible()
        scheduleProcessStartIfPossible(reason: "window-attached")
    }

    private func installInteractionEventMonitorsIfNeeded() {
        guard interactionEventMonitors.isEmpty else {
            return
        }
        let masks: [NSEvent.EventTypeMask] = [
            .keyDown,
            .leftMouseDown,
            .leftMouseDragged,
            .leftMouseUp,
            .rightMouseDown,
            .otherMouseDown,
            .scrollWheel
        ]
        interactionEventMonitors = masks.compactMap { mask in
            NSEvent.addLocalMonitorForEvents(matching: mask) { [weak self] event in
                self?.handlePotentialInteractionEvent(event) ?? event
            }
        }
    }

    private func removeInteractionEventMonitors() {
        for monitor in interactionEventMonitors {
            NSEvent.removeMonitor(monitor)
        }
        interactionEventMonitors.removeAll()
    }

    private func handlePotentialInteractionEvent(_ event: NSEvent) -> NSEvent? {
        guard isVisibleTerminal else {
            return event
        }
        guard event.window === window else {
            return event
        }
        switch event.type {
        case .keyDown:
            if ownsResponder(window?.firstResponder) {
                focusTerminal()
                handleTerminalInteraction()
            }
        case .leftMouseDown, .rightMouseDown, .otherMouseDown:
            let point = convert(event.locationInWindow, from: nil)
            if bounds.contains(point) {
                focusTerminal()
                handleTerminalInteraction()
            }
            stopSelectionAutoscroll()
        case .leftMouseDragged:
            updateSelectionAutoscroll(with: event)
        case .leftMouseUp:
            stopSelectionAutoscroll()
        case .scrollWheel:
            let point = convert(event.locationInWindow, from: nil)
            if bounds.contains(point) {
                if isTerminalFocused == false {
                    handleTerminalInteraction()
                }
            }
        default:
            break
        }
        return event
    }

    private func updateSelectionAutoscroll(with event: NSEvent) {
        let point = terminalView.convert(event.locationInWindow, from: nil)
        let edgeInset: CGFloat = 18

        let nextDelta: Int
        if point.y < edgeInset {
            let overflow = edgeInset - point.y
            nextDelta = -max(1, Int(overflow / 10.0))
        } else if point.y > terminalView.bounds.height - edgeInset {
            let overflow = point.y - (terminalView.bounds.height - edgeInset)
            nextDelta = max(1, Int(overflow / 10.0))
        } else {
            nextDelta = 0
        }

        selectionAutoscrollDelta = nextDelta
        guard nextDelta != 0 else {
            stopSelectionAutoscroll()
            return
        }

        if selectionAutoscrollTimer == nil {
            let timer = Timer.scheduledTimer(
                timeInterval: 0.05,
                target: self,
                selector: #selector(handleSelectionAutoscrollTimer(_:)),
                userInfo: nil,
                repeats: true
            )
            RunLoop.main.add(timer, forMode: .common)
            selectionAutoscrollTimer = timer
        }
    }

    @objc
    private func handleSelectionAutoscrollTimer(_ timer: Timer) {
        guard terminalView.selectionActive else {
            stopSelectionAutoscroll()
            return
        }
        if selectionAutoscrollDelta > 0 {
            terminalView.scrollUp(lines: selectionAutoscrollDelta)
        } else if selectionAutoscrollDelta < 0 {
            terminalView.scrollDown(lines: -selectionAutoscrollDelta)
        } else {
            stopSelectionAutoscroll()
        }
    }

    private func stopSelectionAutoscroll() {
        selectionAutoscrollDelta = 0
        selectionAutoscrollTimer?.invalidate()
        selectionAutoscrollTimer = nil
    }

    private func configureAcceleratedRenderingIfPossible() {
#if canImport(MetalKit)
        guard window != nil else {
            return
        }

        let desiredBufferingMode: MetalBufferingMode
        switch gpuMode {
        case .highPerformance:
            desiredBufferingMode = .perRowPersistent
        case .balanced:
            desiredBufferingMode = .perFrameAggregated
        case .memorySaver:
            desiredBufferingMode = .perFrameAggregated
        }
        if lastAppliedMetalBufferingMode != desiredBufferingMode {
            terminalView.metalBufferingMode = desiredBufferingMode
            lastAppliedMetalBufferingMode = desiredBufferingMode
            logger.log(
                "terminal-renderer",
                "session=\(configuredSession.id.uuidString) mode=\(gpuMode.rawValue) buffering=\(desiredBufferingMode == .perFrameAggregated ? "per-frame-aggregated" : "per-row-persistent") focused=\(isFocusedTerminal) visible=\(isVisibleTerminal) reducedMemory=\(prefersReducedMemoryMode)"
            )
        }

        let shouldUseMetal: Bool
        if preferredMetalRenderingEnabled == false {
            shouldUseMetal = false
        } else if temporarilyPrefersCoreGraphicsDuringStructuralResize {
            shouldUseMetal = false
        } else {
            switch gpuMode {
            case .highPerformance, .balanced:
                shouldUseMetal = true
            case .memorySaver:
                shouldUseMetal = isVisibleTerminal && isFocusedTerminal && !prefersReducedMemoryMode
            }
        }

        if shouldUseMetal {
            guard terminalView.isUsingMetalRenderer == false else {
                return
            }
            guard acceleratedRenderingUnavailable == false else {
                return
            }
            do {
                try terminalView.setUseMetal(true)
                logger.log(
                    "terminal-renderer",
                    "session=\(configuredSession.id.uuidString) renderer=metal buffering=\(String(describing: terminalView.metalBufferingMode))"
                )
            } catch {
                acceleratedRenderingUnavailable = true
                logger.log(
                    "terminal-renderer",
                    "session=\(configuredSession.id.uuidString) renderer=coregraphics reason=\(error.localizedDescription)"
                )
            }
        } else {
            acceleratedRenderingUnavailable = false
            guard terminalView.isUsingMetalRenderer else {
                return
            }
            do {
                try terminalView.setUseMetal(false)
                logger.log(
                    "terminal-renderer",
                    "session=\(configuredSession.id.uuidString) renderer=coregraphics reason=disabled-by-settings"
                )
            } catch {
                logger.log(
                    "terminal-renderer",
                    "session=\(configuredSession.id.uuidString) renderer=metal-disable-failed reason=\(error.localizedDescription)"
                )
            }
        }
#endif
    }

    private func scheduleProcessStartIfPossible(reason: String) {
        guard hasStartedProcess == false else {
            return
        }
        guard isVisibleTerminal || isFocusedTerminal else {
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
        logger.log(
            "terminal-start",
            "schedule session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) reason=\(reason) delayMs=\(Int(startupDelay * 1000))"
        )
        DispatchQueue.main.asyncAfter(deadline: .now() + startupDelay, execute: workItem)
    }

    private func startProcessIfPossible(trigger: String) {
        guard hasStartedProcess == false else {
            return
        }
        guard isVisibleTerminal || isFocusedTerminal else {
            scheduleProcessStartIfPossible(reason: "\(trigger)-awaiting-visibility")
            return
        }
        guard window != nil,
              bounds.width > 0,
              bounds.height > 0,
              terminalView.bounds.width > 0,
              terminalView.bounds.height > 0 else {
            scheduleProcessStartIfPossible(reason: "\(trigger)-awaiting-layout")
            return
        }

        hasStartedProcess = true
        hasReportedStartupFailure = false
        forceTerminalResizeSync(reason: "pre-start")
        terminalView.sessionID = configuredSession.id
        let shell = configuredSession.shell
        let shellName = URL(fileURLWithPath: shell).lastPathComponent
        let launch = shellLaunchConfiguration(shell: shell, shellName: shellName, command: configuredSession.command)

        if debugAIFocus {
            print("[TerminalContainer] start sessionID=\(configuredSession.id.uuidString) shell=\(shell) command=\(configuredSession.command) size=\(Int(bounds.width))x\(Int(bounds.height)) termSize=\(Int(terminalView.bounds.width))x\(Int(terminalView.bounds.height))")
        }
        logger.log(
            "terminal-start",
            "begin session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) reason=\(trigger) shell=\(shell) cwd=\(configuredSession.cwd) size=\(Int(bounds.width))x\(Int(bounds.height)) termSize=\(Int(terminalView.bounds.width))x\(Int(terminalView.bounds.height)) cols=\(terminalView.getTerminal().cols) rows=\(terminalView.getTerminal().rows)"
        )
        terminalView.startProcess(
            executable: shell,
            args: launch.args,
            environment: (configuredEnvironment + [("DMUX_SESSION_INSTANCE_ID", processInstanceID)]).map { "\($0.0)=\($0.1)" },
            execName: launch.execName,
            currentDirectory: configuredSession.cwd
        )
        if terminalShellPID != nil {
            markTerminalReady(reason: "process-started")
        }
        installStartupWatchdog()
        DispatchQueue.main.async { [weak self] in
            self?.forceTerminalResizeSync(reason: "post-start")
        }
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.12) { [weak self] in
            self?.forceTerminalResizeSync(reason: "settle")
        }
        if debugAIFocus {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                guard let self else { return }
                print("[TerminalContainer] sessionID=\(self.configuredSession.id.uuidString) shellPID=\(self.terminalShellPID.map(String.init) ?? "nil")")
            }
        }
    }

    private func forceTerminalResizeSync(reason: String) {
        layoutSubtreeIfNeeded()
        terminalView.layoutSubtreeIfNeeded()
        let size = terminalView.bounds.size
        guard size.width > 0, size.height > 0 else {
            return
        }
        lastSyncedTerminalSize = size
        terminalView.setFrameSize(size)
        if debugAIFocus {
            let cols = terminalView.getTerminal().cols
            let rows = terminalView.getTerminal().rows
            print("[TerminalContainer] resize-sync sessionID=\(configuredSession.id.uuidString) reason=\(reason) cols=\(cols) rows=\(rows) size=\(Int(size.width))x\(Int(size.height))")
        }
    }

    private func syncTerminalSizeIfNeeded(reason: String) {
        let size = terminalView.bounds.size
        guard size.width > 0, size.height > 0 else {
            return
        }
        guard abs(size.width - lastSyncedTerminalSize.width) > 0.5 || abs(size.height - lastSyncedTerminalSize.height) > 0.5 else {
            return
        }
        forceTerminalResizeSync(reason: reason)
    }

    private func handleInitialTerminalOutput() {
        markTerminalReady(reason: "initial-output")
    }

    private func handleTerminalInteraction() {
        notifyInteraction()
        guard gpuMode == .memorySaver else {
            return
        }
    }

    private func handleProcessTermination(exitCode: Int32?) {
        resetTransientTerminalModes()
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
        logger.log(
            "terminal-exit",
            "session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) exit=\(exitCode.map(String.init) ?? "nil")"
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
                self.logger.log(
                    "terminal-start",
                    "watchdog session=\(self.configuredSession.id.uuidString) shellPID=\(self.terminalShellPID.map(String.init) ?? "nil") result=alive-without-output"
                )
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
            "terminal-ready",
            "session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) reason=\(reason) cols=\(terminalView.getTerminal().cols) rows=\(terminalView.getTerminal().rows)"
        )
        onStartupSucceeded?()
        if pendingFocusRequest {
            pendingFocusRequest = false
            focusTerminal()
        }
    }

    private func reportStartupFailureIfNeeded(_ detail: String) {
        guard hasReportedStartupFailure == false else {
            return
        }
        hasReportedStartupFailure = true
        logger.log(
            "terminal-recovery",
            "container-failed session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) detail=\(detail)"
        )
        onStartupFailure?(detail)
    }

    private func updateLoadingShieldVisibility() {
        loadingShieldView.isHidden = hasReceivedInitialOutput
    }

    private func shellLaunchConfiguration(shell: String, shellName: String, command: String) -> (args: [String], execName: String) {
        guard command != shell else {
            switch shellName {
            case "zsh", "bash", "fish":
                // Match Terminal-style login shells so user profile/env initialization runs before dmux hooks.
                return (["-i", "-l"], shellName)
            default:
                return ([], shellName)
            }
        }

        switch shellName {
        case "zsh", "bash", "fish":
            return (["-i", "-l", "-c", command], shellName)
        default:
            return (["-lc", command], "-\(shellName)")
        }
    }

    override func menu(for event: NSEvent) -> NSMenu? {
        let menu = NSMenu()

        let copyItem = NSMenuItem(title: String(localized: "common.copy", defaultValue: "Copy", bundle: .module), action: #selector(copySelection), keyEquivalent: "")
        copyItem.target = self
        copyItem.isEnabled = terminalView.selectionActive
        menu.addItem(copyItem)

        let pasteItem = NSMenuItem(title: String(localized: "common.paste", defaultValue: "Paste", bundle: .module), action: #selector(pasteClipboard), keyEquivalent: "")
        pasteItem.target = self
        menu.addItem(pasteItem)

        menu.addItem(.separator())

        let clearItem = NSMenuItem(title: String(localized: "common.clear_screen", defaultValue: "Clear Screen", bundle: .module), action: #selector(clearScreen), keyEquivalent: "")
        clearItem.target = self
        menu.addItem(clearItem)

        let selectAllItem = NSMenuItem(title: String(localized: "common.select_all", defaultValue: "Select All", bundle: .module), action: #selector(selectAllText), keyEquivalent: "")
        selectAllItem.target = self
        menu.addItem(selectAllItem)

        return menu
    }

    @objc
    private func copySelection() {
        terminalView.copy(self)
    }

    @objc
    private func pasteClipboard() {
        terminalView.paste(self)
    }

    @objc
    private func selectAllText() {
        terminalView.selectAll(self)
    }

    @objc
    private func clearScreen() {
        let bytes = Array("clear\n".utf8)
        terminalView.send(source: terminalView, data: bytes[...])
    }

    func sendText(_ text: String) {
        let bytes = Array(text.utf8)
        guard !bytes.isEmpty else {
            return
        }
        handleTerminalInteraction()
        terminalView.send(source: terminalView, data: bytes[...])
    }

    func sendInterrupt() {
        let interrupt: [UInt8] = [3]
        handleTerminalInteraction()
        terminalView.send(source: terminalView, data: interrupt[...])
    }

    func sendEscape() {
        let escape: [UInt8] = [0x1b]
        handleTerminalInteraction()
        terminalView.send(source: terminalView, data: escape[...])
    }

    func sendNativeCommandArrow(keyCode: UInt16) -> Bool {
        let sequence: [UInt8]
        switch keyCode {
        case 123:
            sequence = terminalView.getTerminal().applicationCursor ? EscapeSequences.moveHomeApp : EscapeSequences.moveHomeNormal
        case 124:
            sequence = terminalView.getTerminal().applicationCursor ? EscapeSequences.moveEndApp : EscapeSequences.moveEndNormal
        default:
            return false
        }

        handleTerminalInteraction()
        terminalView.send(sequence)
        return true
    }

    func forwardKeyDown(_ event: NSEvent) -> Bool {
        handleTerminalInteraction()
        focusTerminal()
        terminalView.keyDown(with: event)
        return true
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

    private func notifyInteraction() {
        onInteraction?()
    }
}

@MainActor
final class SwiftTermTerminalRegistry {
    static let shared = SwiftTermTerminalRegistry()

    private var containers: [UUID: SwiftTermTerminalContainerView] = [:]
    private var explicitFocusedSessionID: UUID?
    private let debugAIFocus = ProcessInfo.processInfo.environment["DMUX_DEBUG_AI_FOCUS"] == "1"

    func containerView(
        for session: TerminalSession,
        environment: [(String, String)],
        terminalBackgroundPreset: AppTerminalBackgroundPreset,
        terminalFontSize: Int,
        useMetalRendering: Bool,
        gpuMode: AppTerminalGPUMode,
        isFocused: Bool,
        isVisible: Bool,
        prefersReducedMemoryMode: Bool,
        onInteraction: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) -> SwiftTermTerminalContainerView {
        if let existing = containers[session.id] {
            if debugAIFocus {
                print("[TerminalRegistry] reuse sessionID=\(session.id.uuidString) shellPID=\(existing.terminalShellPID.map(String.init) ?? "nil")")
            }
            existing.updateSession(
                session,
                environment: environment,
                terminalBackgroundPreset: terminalBackgroundPreset,
                terminalFontSize: terminalFontSize,
                useMetalRendering: useMetalRendering,
                gpuMode: gpuMode,
                isFocused: isFocused,
                isVisible: isVisible,
                prefersReducedMemoryMode: prefersReducedMemoryMode,
                onInteraction: onInteraction,
                onStartupSucceeded: onStartupSucceeded,
                onStartupFailure: onStartupFailure
            )
            return existing
        }

        let created = SwiftTermTerminalContainerView(
            session: session,
            environment: environment,
            terminalFontSize: terminalFontSize,
            onInteraction: onInteraction,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        created.updateSession(
            session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            terminalFontSize: terminalFontSize,
            useMetalRendering: useMetalRendering,
            gpuMode: gpuMode,
            isFocused: isFocused,
            isVisible: isVisible,
            prefersReducedMemoryMode: prefersReducedMemoryMode,
            onInteraction: onInteraction,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        containers[session.id] = created
        if debugAIFocus {
            print("[TerminalRegistry] create sessionID=\(session.id.uuidString) cwd=\(session.cwd)")
        }
        return created
    }

    func release(sessionID: UUID) {
        guard let container = containers.removeValue(forKey: sessionID) else { return }
        if debugAIFocus {
            print("[TerminalRegistry] release sessionID=\(sessionID.uuidString) shellPID=\(container.terminalShellPID.map(String.init) ?? "nil")")
        }
        container.terminateProcessTree()
        SwiftTermOutputEventEmitter.shared.clear(sessionID: sessionID)
        container.removeFromSuperviewWithoutNeedingDisplay()
        if explicitFocusedSessionID == sessionID {
            explicitFocusedSessionID = nil
            NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: sessionID)
        }
    }

    func beginStructuralResizeTransition(for sessionIDs: [UUID]) {
        for sessionID in sessionIDs {
            containers[sessionID]?.beginStructuralResizeTransition()
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
        if let explicitFocusedSessionID,
           containers[explicitFocusedSessionID] != nil {
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

final class SwiftTermProcessDelegateProxy: NSObject, LocalProcessTerminalViewDelegate {
    var onProcessTerminated: ((Int32?) -> Void)?

    nonisolated func sizeChanged(source: LocalProcessTerminalView, newCols: Int, newRows: Int) {}
    nonisolated func setTerminalTitle(source: LocalProcessTerminalView, title: String) {}
    nonisolated func hostCurrentDirectoryUpdate(source: TerminalView, directory: String?) {}
    nonisolated func processTerminated(source: TerminalView, exitCode: Int32?) {
        onProcessTerminated?(exitCode)
    }
}
