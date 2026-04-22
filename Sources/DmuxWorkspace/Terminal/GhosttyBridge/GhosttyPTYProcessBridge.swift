import AppKit
import Darwin
import Foundation
import GhosttyTerminal

private enum GhosttyWaitStatus {
    static func didExit(_ status: Int32) -> Bool {
        (status & 0x7f) == 0
    }

    static func exitStatus(_ status: Int32) -> Int32 {
        (status >> 8) & 0xff
    }

    static func didSignal(_ status: Int32) -> Bool {
        let code = status & 0x7f
        return code != 0 && code != 0x7f
    }

    static func termSignal(_ status: Int32) -> Int32 {
        status & 0x7f
    }
}

extension NSColor {
    var ghosttyHexString: String {
        let converted = usingColorSpace(.deviceRGB) ?? self
        let red = Int(round(converted.redComponent * 255.0))
        let green = Int(round(converted.greenComponent * 255.0))
        let blue = Int(round(converted.blueComponent * 255.0))
        return String(format: "#%02X%02X%02X", red, green, blue)
    }
}

final class GhosttyPTYProcessBridge: @unchecked Sendable {
    let sessionID: UUID
    let processInstanceID = UUID().uuidString.lowercased()
    lazy var terminalSession = InMemoryTerminalSession(
        write: { [weak self] data in
            self?.writeToProcess(data)
        },
        resize: { [weak self] viewport in
            self?.resizeProcess(viewport)
        }
    )

    var onFirstOutput: (() -> Void)?
    var onProcessTerminated: ((Int32?) -> Void)?

    private let logger = AppDebugLog.shared
    private let lock = NSLock()
    private let ioQueue = DispatchQueue(label: "codux.ghostty.pty", qos: .userInitiated)
    private var masterFD: Int32 = -1
    private var closeMasterFDOnCancel = true
    private var shellPID: Int32 = 0
    private var readSource: DispatchSourceRead?
    private var processSource: DispatchSourceProcess?
    private var launchDate = Date()
    private var hasObservedOutput = false
    private var lastViewport = InMemoryTerminalViewport(columns: 80, rows: 24)
    private var lastAppliedViewport: InMemoryTerminalViewport?
    private var pendingResizeWorkItem: DispatchWorkItem?
    private let resizeDebounceDelay: TimeInterval = 0.05

    init(sessionID: UUID) {
        self.sessionID = sessionID
    }

    deinit {
        terminateProcessTree()
    }

    func start(
        shell: String,
        shellName: String,
        command: String,
        cwd: String,
        environment: [(String, String)]
    ) {
        guard currentShellPID == nil else {
            return
        }

        var winsizeValue = winsize(
            ws_row: lastViewport.rows,
            ws_col: lastViewport.columns,
            ws_xpixel: UInt16(min(lastViewport.widthPixels, UInt32(UInt16.max))),
            ws_ypixel: UInt16(min(lastViewport.heightPixels, UInt32(UInt16.max)))
        )
        var master: Int32 = -1
        let launch = shellLaunchConfiguration(shellName: shellName, command: command)
        let pid = forkpty(&master, nil, nil, &winsizeValue)

        if pid < 0 {
            logger.log(
                "ghostty-process",
                "start-failed session=\(sessionID.uuidString) reason=forkpty errno=\(errno)"
            )
            onProcessTerminated?(nil)
            return
        }

        if pid == 0 {
            _ = chdir(cwd)
            var env = Dictionary(uniqueKeysWithValues: environment)
            env["DMUX_SESSION_INSTANCE_ID"] = processInstanceID
            if env["TERM"] == nil {
                env["TERM"] = "xterm-256color"
            }

            let execArguments = [launch.execName] + launch.args
            _ = execArguments.withCStringArray { argv in
                env
                    .map { "\($0.key)=\($0.value)" }
                    .withCStringArray { envp in
                        execve(shell, argv, envp)
                    }
            }

            let message = "Codux Ghostty exec failed: \(String(cString: strerror(errno)))\n"
            message.withCString { ptr in
                _ = write(STDERR_FILENO, ptr, strlen(ptr))
            }
            _exit(127)
        }

        configureParentAfterFork(masterFD: master, childPID: pid)
    }

    func resetOutputObservation() {
        lock.lock()
        hasObservedOutput = false
        lock.unlock()
    }

    var currentShellPID: Int32? {
        lock.lock()
        defer { lock.unlock() }
        return shellPID > 0 ? shellPID : nil
    }

    func terminateProcessTree() {
        let pid: Int32
        let fd: Int32
        let shouldCloseOnCancel: Bool
        let readSource: DispatchSourceRead?
        let processSource: DispatchSourceProcess?

        lock.lock()
        pid = shellPID
        fd = masterFD
        shouldCloseOnCancel = closeMasterFDOnCancel
        readSource = self.readSource
        processSource = self.processSource
        let pendingResizeWorkItem = self.pendingResizeWorkItem
        shellPID = 0
        masterFD = -1
        closeMasterFDOnCancel = false
        self.readSource = nil
        self.processSource = nil
        self.pendingResizeWorkItem = nil
        lastAppliedViewport = nil
        lock.unlock()

        pendingResizeWorkItem?.cancel()
        readSource?.cancel()
        processSource?.cancel()
        if fd >= 0, shouldCloseOnCancel {
            close(fd)
        }

        guard pid > 0 else {
            return
        }

        kill(-pid, SIGTERM)
        kill(pid, SIGTERM)

        ioQueue.asyncAfter(deadline: .now() + 1.0) {
            guard kill(pid, 0) == 0 else {
                return
            }
            kill(-pid, SIGKILL)
            kill(pid, SIGKILL)
        }
    }

    func sendText(_ text: String) {
        guard let data = text.data(using: .utf8), !data.isEmpty else {
            return
        }
        writeToProcess(data)
    }

    func sendInterrupt() {
        writeToProcess(Data([0x03]))
    }

    func sendEscape() {
        writeToProcess(Data([0x1b]))
    }

    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut) {
        writeToProcess(Data(shortcut.bytes))
    }

    func sendNativeCommandArrow(keyCode: UInt16) -> Bool {
        guard let shortcut = TerminalEditingShortcut.match(
            keyCode: keyCode,
            modifiers: [.command]
        ) else {
            return false
        }
        sendEditingShortcut(shortcut)
        return true
    }

    private func configureParentAfterFork(masterFD: Int32, childPID: Int32) {
        _ = fcntl(masterFD, F_SETFL, O_NONBLOCK)

        lock.lock()
        self.masterFD = masterFD
        closeMasterFDOnCancel = true
        shellPID = childPID
        launchDate = Date()
        lastAppliedViewport = nil
        readSource = DispatchSource.makeReadSource(fileDescriptor: masterFD, queue: ioQueue)
        processSource = DispatchSource.makeProcessSource(identifier: childPID, eventMask: .exit, queue: ioQueue)
        let readSource = self.readSource
        let processSource = self.processSource
        lock.unlock()

        logger.log(
            "ghostty-process",
            "started session=\(sessionID.uuidString) shellPID=\(childPID) instance=\(processInstanceID)"
        )

        readSource?.setEventHandler { [weak self] in
            self?.consumeReadableOutput()
        }
        readSource?.setCancelHandler {
            self.lock.lock()
            let shouldClose = self.closeMasterFDOnCancel
            self.closeMasterFDOnCancel = false
            self.lock.unlock()
            if shouldClose {
                _ = close(masterFD)
            }
        }
        readSource?.resume()

        processSource?.setEventHandler { [weak self] in
            self?.handleProcessExit(expectedPID: childPID)
        }
        processSource?.resume()

        resizeProcess(lastViewport)
    }

    private func consumeReadableOutput() {
        let fd: Int32
        lock.lock()
        fd = masterFD
        lock.unlock()

        guard fd >= 0 else {
            return
        }

        var buffer = [UInt8](repeating: 0, count: 16384)
        while true {
            let count = read(fd, &buffer, buffer.count)
            if count > 0 {
                let data = Data(buffer.prefix(count))
                var fireFirstOutput = false

                lock.lock()
                if hasObservedOutput == false {
                    hasObservedOutput = true
                    fireFirstOutput = true
                }
                lock.unlock()

                terminalSession.receive(data)
                if fireFirstOutput {
                    DispatchQueue.main.async { [weak self] in
                        self?.onFirstOutput?()
                    }
                }
                continue
            }

            if count == 0 {
                return
            }

            if errno == EAGAIN || errno == EWOULDBLOCK {
                return
            }

            if errno == EINTR {
                continue
            }

            return
        }
    }

    private func handleProcessExit(expectedPID: Int32) {
        var status: Int32 = 0
        let waitedPID = waitpid(expectedPID, &status, 0)
        let exitCode: Int32?
        if waitedPID > 0, GhosttyWaitStatus.didExit(status) {
            exitCode = GhosttyWaitStatus.exitStatus(status)
        } else if waitedPID > 0, GhosttyWaitStatus.didSignal(status) {
            exitCode = 128 + GhosttyWaitStatus.termSignal(status)
        } else {
            exitCode = nil
        }

        let runtimeMs = max(0, UInt64(Date().timeIntervalSince(launchDate) * 1000))
        terminalSession.finish(
            exitCode: UInt32(max(0, exitCode ?? 0)),
            runtimeMilliseconds: runtimeMs
        )

        lock.lock()
        let readSource = self.readSource
        self.readSource = nil
        self.processSource = nil
        shellPID = 0
        masterFD = -1
        closeMasterFDOnCancel = false
        lock.unlock()

        readSource?.cancel()
        logger.log(
            "ghostty-process",
            "exited session=\(sessionID.uuidString) exit=\(exitCode.map(String.init) ?? "nil")"
        )

        DispatchQueue.main.async { [weak self] in
            self?.onProcessTerminated?(exitCode)
        }
    }

    private func writeToProcess(_ data: Data) {
        let fd: Int32
        lock.lock()
        fd = masterFD
        lock.unlock()

        guard fd >= 0, !data.isEmpty else {
            return
        }

        if data.contains(0x03) {
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .dmuxTerminalInterruptDidSend,
                    object: self.sessionID
                )
            }
        }

        ioQueue.async {
            data.withUnsafeBytes { buffer in
                guard var base = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return
                }
                var remaining = buffer.count
                while remaining > 0 {
                    let written = write(fd, base, remaining)
                    if written > 0 {
                        remaining -= written
                        base = base.advanced(by: written)
                        continue
                    }
                    if written < 0, errno == EINTR {
                        continue
                    }
                    break
                }
            }
        }
    }

    private func resizeProcess(_ viewport: InMemoryTerminalViewport) {
        lock.lock()
        lastViewport = viewport
        let fd = masterFD
        pendingResizeWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            self?.applyPendingResize()
        }
        pendingResizeWorkItem = workItem
        lock.unlock()
        guard fd >= 0 else {
            return
        }

        ioQueue.asyncAfter(deadline: .now() + resizeDebounceDelay, execute: workItem)
    }

    private func applyPendingResize() {
        let viewport: InMemoryTerminalViewport
        let fd: Int32
        let pid: Int32

        lock.lock()
        pendingResizeWorkItem = nil
        viewport = lastViewport
        fd = masterFD
        pid = shellPID
        if lastAppliedViewport == viewport {
            lock.unlock()
            return
        }
        lastAppliedViewport = viewport
        lock.unlock()

        guard fd >= 0, viewport.columns > 0, viewport.rows > 0 else {
            return
        }

        var winsizeValue = winsize(
            ws_row: viewport.rows,
            ws_col: viewport.columns,
            ws_xpixel: UInt16(min(viewport.widthPixels, UInt32(UInt16.max))),
            ws_ypixel: UInt16(min(viewport.heightPixels, UInt32(UInt16.max)))
        )
        _ = ioctl(fd, TIOCSWINSZ, &winsizeValue)
        if pid > 0 {
            kill(pid, SIGWINCH)
            kill(-pid, SIGWINCH)
        }
    }

    private func shellLaunchConfiguration(shellName: String, command: String) -> (args: [String], execName: String) {
        switch shellName {
        case "zsh", "bash", "fish":
            if command == shellName || command.hasSuffix("/\(shellName)") {
                return (["-i", "-l"], shellName)
            }
            return (["-i", "-l", "-c", command], shellName)
        default:
            return command == shellName ? ([], shellName) : (["-lc", command], "-\(shellName)")
        }
    }
}
