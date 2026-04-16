import AppKit
import Darwin
import SwiftTerm
import SwiftUI

extension Notification.Name {
    static let dmuxTerminalFocusDidChange = Notification.Name("dmux.terminalFocusDidChange")
    static let dmuxTerminalOutputDidChange = Notification.Name("dmux.terminalOutputDidChange")
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
    let shouldFocus: Bool
    var onInteraction: (() -> Void)? = nil
    var onFocusConsumed: (() -> Void)? = nil

    func makeCoordinator() -> Coordinator {
        Coordinator(sessionID: session.id)
    }

    func makeNSView(context: Context) -> SwiftTermTerminalContainerView {
        let view = SwiftTermTerminalRegistry.shared.containerView(
            for: session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            onInteraction: onInteraction
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
            onInteraction: onInteraction
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
    private let processInstanceID = UUID().uuidString.lowercased()
    private let processDelegateProxy = SwiftTermProcessDelegateProxy()
    private var onInteraction: (() -> Void)?
    private var hasStartedProcess = false
    private var hasReceivedInitialOutput = false
    private var pendingFocusRequest = false
    private let logger = AppDebugLog.shared
    private let debugAIFocus = ProcessInfo.processInfo.environment["DMUX_DEBUG_AI_FOCUS"] == "1"
    private lazy var clickRecognizer: NSClickGestureRecognizer = {
        let recognizer = NSClickGestureRecognizer(target: self, action: #selector(handleContainerClick))
        recognizer.buttonMask = 0x1
        return recognizer
    }()

    init(session: TerminalSession, environment: [(String, String)], onInteraction: (() -> Void)?) {
        self.configuredSession = session
        self.configuredEnvironment = environment
        self.terminalBackgroundPreset = .obsidian
        self.terminalView = DmuxLocalProcessTerminalView(frame: .zero)
        self.onInteraction = onInteraction
        super.init(frame: .zero)
        setup()
        startProcessIfPossible()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func updateSession(_ session: TerminalSession, environment: [(String, String)], terminalBackgroundPreset: AppTerminalBackgroundPreset, onInteraction: (() -> Void)?) {
        configuredEnvironment = environment
        applyTheme(terminalBackgroundPreset)
        self.onInteraction = onInteraction
        guard configuredSession.id != session.id else {
            startProcessIfPossible()
            return
        }
        SwiftTermOutputEventEmitter.shared.clear(sessionID: configuredSession.id)
        configuredSession = session
        terminalView.sessionID = session.id
        terminalView.resetOutputObservation()
        hasReceivedInitialOutput = false
        pendingFocusRequest = false
        updateLoadingShieldVisibility()
        terminalView.terminate()
        hasStartedProcess = false
        startProcessIfPossible()
    }

    func prepareForHostReuse() {
        if superview != nil {
            removeFromSuperviewWithoutNeedingDisplay()
        }
    }

    func focusTerminal() {
        guard hasReceivedInitialOutput else {
            pendingFocusRequest = true
            logger.log("terminal-focus", "defer session=\(configuredSession.id.uuidString) reason=waiting-for-output")
            return
        }
        guard window?.firstResponder !== terminalView else { return }
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
        applyTheme(.obsidian)

        terminalView.translatesAutoresizingMaskIntoConstraints = false
        terminalView.processDelegate = processDelegateProxy
        terminalView.caretColor = .systemBlue
        terminalView.optionAsMetaKey = false
        terminalView.allowMouseReporting = false
        terminalView.font = .monospacedSystemFont(ofSize: 14, weight: .regular)
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
        addGestureRecognizer(clickRecognizer)

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

    private func resetTransientTerminalModes() {
        terminalView.getTerminal().feed(text: "\u{1b}[<u")
    }

    private func applyTheme(_ preset: AppTerminalBackgroundPreset) {
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
        startProcessIfPossible()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        startProcessIfPossible()
    }

    private func startProcessIfPossible() {
        guard hasStartedProcess == false else {
            return
        }
        guard window != nil,
              bounds.width > 0,
              bounds.height > 0,
              terminalView.bounds.width > 0,
              terminalView.bounds.height > 0 else {
            return
        }

        hasStartedProcess = true
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
            "session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) shell=\(shell) cwd=\(configuredSession.cwd) size=\(Int(bounds.width))x\(Int(bounds.height)) termSize=\(Int(terminalView.bounds.width))x\(Int(terminalView.bounds.height)) cols=\(terminalView.getTerminal().cols) rows=\(terminalView.getTerminal().rows)"
        )
        terminalView.startProcess(
            executable: shell,
            args: launch.args,
            environment: (configuredEnvironment + [("DMUX_SESSION_INSTANCE_ID", processInstanceID)]).map { "\($0.0)=\($0.1)" },
            execName: launch.execName,
            currentDirectory: configuredSession.cwd
        )
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
        terminalView.setFrameSize(size)
        if debugAIFocus {
            let cols = terminalView.getTerminal().cols
            let rows = terminalView.getTerminal().rows
            print("[TerminalContainer] resize-sync sessionID=\(configuredSession.id.uuidString) reason=\(reason) cols=\(cols) rows=\(rows) size=\(Int(size.width))x\(Int(size.height))")
        }
    }

    private func handleInitialTerminalOutput() {
        guard hasReceivedInitialOutput == false else {
            return
        }
        hasReceivedInitialOutput = true
        updateLoadingShieldVisibility()
        logger.log(
            "terminal-ready",
            "session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) cols=\(terminalView.getTerminal().cols) rows=\(terminalView.getTerminal().rows)"
        )
        if pendingFocusRequest {
            pendingFocusRequest = false
            focusTerminal()
        }
    }

    private func handleProcessTermination(exitCode: Int32?) {
        resetTransientTerminalModes()
        if hasReceivedInitialOutput == false {
            hasReceivedInitialOutput = true
            updateLoadingShieldVisibility()
        }
        pendingFocusRequest = false
        logger.log(
            "terminal-exit",
            "session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) exit=\(exitCode.map(String.init) ?? "nil")"
        )
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
        terminalView.send(source: terminalView, data: bytes[...])
    }

    private func notifyInteraction() {
        onInteraction?()
    }

    @objc
    private func handleContainerClick() {
        focusTerminal()
        notifyInteraction()
    }
}

@MainActor
final class SwiftTermTerminalRegistry {
    static let shared = SwiftTermTerminalRegistry()

    private var containers: [UUID: SwiftTermTerminalContainerView] = [:]
    private var explicitFocusedSessionID: UUID?
    private let debugAIFocus = ProcessInfo.processInfo.environment["DMUX_DEBUG_AI_FOCUS"] == "1"

    func containerView(for session: TerminalSession, environment: [(String, String)], terminalBackgroundPreset: AppTerminalBackgroundPreset, onInteraction: (() -> Void)?) -> SwiftTermTerminalContainerView {
        if let existing = containers[session.id] {
            if debugAIFocus {
                print("[TerminalRegistry] reuse sessionID=\(session.id.uuidString) shellPID=\(existing.terminalShellPID.map(String.init) ?? "nil")")
            }
            existing.prepareForHostReuse()
            existing.updateSession(session, environment: environment, terminalBackgroundPreset: terminalBackgroundPreset, onInteraction: onInteraction)
            return existing
        }

        let created = SwiftTermTerminalContainerView(session: session, environment: environment, onInteraction: onInteraction)
        created.updateSession(session, environment: environment, terminalBackgroundPreset: terminalBackgroundPreset, onInteraction: onInteraction)
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
