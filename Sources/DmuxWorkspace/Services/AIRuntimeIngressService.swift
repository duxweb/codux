import Darwin
import Foundation

extension Notification.Name {
    static let dmuxAIRuntimeBridgeDidChange = Notification.Name("dmuxAIRuntimeBridgeDidChange")
}

@MainActor
final class AIRuntimeIngressService {
    static let shared = AIRuntimeIngressService()

    private let bridgeService: AIRuntimeBridgeService
    private let logger: AppDebugLog
    private let aiSessionStore: AISessionStore
    private let toolDriverFactory: AIToolDriverFactory

    private var runtimeSocketListenerFD: Int32 = -1
    private var runtimeSocketWatcher: DispatchSourceRead?
    private var recentRuntimeEventAtByKey: [String: Date] = [:]
    private var latestProjects: [Project] = []

    init(
        bridgeService: AIRuntimeBridgeService = AIRuntimeBridgeService(),
        logger: AppDebugLog = .shared,
        aiSessionStore: AISessionStore = .shared,
        toolDriverFactory: AIToolDriverFactory = .shared
    ) {
        self.bridgeService = bridgeService
        self.logger = logger
        self.aiSessionStore = aiSessionStore
        self.toolDriverFactory = toolDriverFactory
    }

    func resetEphemeralState() {
        aiSessionStore.reset()
        bridgeService.clearAllClaudeSessionMappings()
        recentRuntimeEventAtByKey.removeAll()
    }

    func importRuntime(projects: [Project]) {
        ensureSocketListening()
        latestProjects = projects
    }

    func canonicalToolName(_ tool: String) -> String {
        toolDriverFactory.canonicalToolName(tool)
    }

    func isRealtimeTool(_ tool: String) -> Bool {
        toolDriverFactory.isRealtimeTool(tool)
    }

    func clearLiveState(sessionID: UUID) {
        aiSessionStore.removeTerminal(sessionID)
        AIRuntimePollingService.shared.sync(reason: "terminal-removed")
    }

    func ingestManagedRuntimeSocketEventForTesting(kind: String, payloadData: Data) async {
        await processDecodedEvent(kind: kind, payloadData: payloadData)
    }

    func startWatching() {
        let socketPath = bridgeService.runtimeEventSocketURL().path
        if runtimeSocketListenerFD >= 0 {
            close(runtimeSocketListenerFD)
            runtimeSocketListenerFD = -1
        }
        unlink(socketPath)

        runtimeSocketWatcher?.cancel()
        runtimeSocketWatcher = nil

        let listener = socket(AF_UNIX, SOCK_STREAM, 0)
        guard listener >= 0 else {
            logger.log("runtime-socket", "create failed path=\(socketPath)")
            return
        }

        var address = sockaddr_un()
        address.sun_family = sa_family_t(AF_UNIX)
        let maxLength = MemoryLayout.size(ofValue: address.sun_path)
        let utf8 = socketPath.utf8CString
        guard utf8.count < maxLength else {
            logger.log("runtime-socket", "path too long path=\(socketPath)")
            close(listener)
            return
        }
        withUnsafeMutableBytes(of: &address.sun_path) { buffer in
            buffer.initializeMemory(as: CChar.self, repeating: 0)
            for (index, byte) in utf8.enumerated() {
                buffer[index] = UInt8(bitPattern: byte)
            }
        }

        let addressLength = socklen_t(MemoryLayout<sa_family_t>.size + utf8.count)
        let bindResult = withUnsafePointer(to: &address) {
            $0.withMemoryRebound(to: sockaddr.self, capacity: 1) { pointer in
                bind(listener, pointer, addressLength)
            }
        }
        guard bindResult == 0, listen(listener, 32) == 0 else {
            logger.log("runtime-socket", "bind/listen failed path=\(socketPath) errno=\(errno)")
            close(listener)
            unlink(socketPath)
            return
        }

        _ = fcntl(listener, F_SETFL, fcntl(listener, F_GETFL) | O_NONBLOCK)
        runtimeSocketListenerFD = listener
        logger.log("runtime-socket", "listening path=\(socketPath)")

        let watcher = DispatchSource.makeReadSource(fileDescriptor: listener, queue: .main)
        watcher.setEventHandler { [weak self] in
            self?.acceptPendingRuntimeConnections()
        }
        watcher.setCancelHandler { [weak self] in
            guard let self else {
                return
            }
            if self.runtimeSocketListenerFD == listener {
                self.runtimeSocketListenerFD = -1
                self.runtimeSocketWatcher = nil
            }
        }
        watcher.resume()
        runtimeSocketWatcher = watcher
    }

    func ensureSocketListening() {
        let socketPath = bridgeService.runtimeEventSocketURL().path
        let listenerMissing = runtimeSocketWatcher == nil || runtimeSocketListenerFD < 0
        let socketMissing = FileManager.default.fileExists(atPath: socketPath) == false
        guard listenerMissing || socketMissing else {
            return
        }
        logger.log(
            "runtime-socket",
            "self-heal restart path=\(socketPath) reason=\(listenerMissing ? "listener-missing" : "socket-missing")"
        )
        startWatching()
    }

    private func acceptPendingRuntimeConnections() {
        while runtimeSocketListenerFD >= 0 {
            let connectionFD = accept(runtimeSocketListenerFD, nil, nil)
            if connectionFD < 0 {
                if errno == EAGAIN || errno == EWOULDBLOCK {
                    break
                }
                return
            }

            DispatchQueue.global(qos: .utility).async { [weak self] in
                var data = Data()
                var buffer = [UInt8](repeating: 0, count: 4096)
                while true {
                    let count = read(connectionFD, &buffer, buffer.count)
                    if count > 0 {
                        data.append(buffer, count: count)
                        continue
                    }
                    break
                }
                close(connectionFD)
                guard !data.isEmpty else {
                    return
                }
                Task { @MainActor [weak self] in
                    self?.processRuntimeSocketEvent(data)
                }
            }
        }
    }

    private func processRuntimeSocketEvent(_ data: Data) {
        guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let kind = object["kind"] as? String,
              let payloadObject = object["payload"] else {
            logger.log("runtime-socket", "invalid payload")
            return
        }

        guard JSONSerialization.isValidJSONObject(payloadObject),
              let payloadData = try? JSONSerialization.data(withJSONObject: payloadObject) else {
            logger.log("runtime-socket", "invalid json kind=\(kind)")
            return
        }

        logger.log(
            "runtime-socket",
            "received kind=\(kind) bytes=\(payloadData.count)"
        )

        Task { [weak self] in
            guard let self else {
                return
            }
            await self.processDecodedEvent(kind: kind, payloadData: payloadData)
        }
    }

    private func processDecodedEvent(kind: String, payloadData: Data) async {
        switch kind {
        case "ai-hook":
            guard let event = try? JSONDecoder().decode(AIHookEvent.self, from: payloadData) else {
                logger.log("runtime-socket", "drop kind=ai-hook reason=decode")
                return
            }
            logger.log(
                "runtime-ingress",
                "receive ai-hook terminal=\(event.terminalID.uuidString) tool=\(event.tool) kind=\(event.kind.rawValue) external=\(event.aiSessionID ?? "nil") total=\(event.totalTokens.map(String.init) ?? "nil")"
            )
            guard shouldAcceptRuntimeEvent(
                key: runtimeSocketEventKey(kind: kind, data: payloadData),
                ttl: 0.35
            ) else {
                logger.log("runtime-socket", "drop kind=ai-hook terminal=\(event.terminalID.uuidString) reason=dedupe")
                return
            }
            await processAIHookEvent(event)

        case "manual-interrupt":
            guard let event = try? JSONDecoder().decode(AIManualInterruptEvent.self, from: payloadData) else {
                logger.log("runtime-socket", "drop kind=manual-interrupt reason=decode")
                return
            }
            logger.log(
                "runtime-interrupt",
                "receive manual-interrupt terminal=\(event.terminalID.uuidString) updatedAt=\(event.updatedAt)"
            )
            processManualInterruptEvent(event)

        case "opencode-runtime":
            guard let envelope = try? JSONDecoder().decode(AIToolUsageEnvelope.self, from: payloadData) else {
                logger.log("runtime-socket", "drop kind=opencode-runtime reason=decode")
                return
            }
            guard shouldAcceptRuntimeEvent(
                key: runtimeSocketEventKey(kind: kind, data: payloadData),
                ttl: 0.35
            ) else {
                logger.log("runtime-socket", "drop kind=opencode-runtime session=\(envelope.sessionId) reason=dedupe")
                return
            }
            processOpencodeRuntimeEnvelope(envelope)

        default:
            logger.log("runtime-socket", "ignore kind=\(kind) reason=unsupported")
        }
    }

    private func processAIHookEvent(_ event: AIHookEvent) async {
        let currentSession = aiSessionStore.session(for: event.terminalID)
        let resolvedEvent = await toolDriverFactory.resolveHookEvent(event, currentSession: currentSession)
        let shouldBackfillCompletion = shouldScheduleClaudeTurnCompletedBackfill(
            originalEvent: event,
            resolvedEvent: resolvedEvent,
            currentSession: currentSession
        )
        logger.log(
            "runtime-ingress",
            "apply ai-hook terminal=\(resolvedEvent.terminalID.uuidString) tool=\(resolvedEvent.tool) kind=\(resolvedEvent.kind.rawValue) external=\(resolvedEvent.aiSessionID ?? "nil") model=\(resolvedEvent.model ?? "nil") total=\(resolvedEvent.totalTokens.map(String.init) ?? "nil")"
        )
        let didChange = aiSessionStore.apply(resolvedEvent)
        guard didChange else {
            logger.log(
                "runtime-ingress",
                "skip ai-hook terminal=\(resolvedEvent.terminalID.uuidString) tool=\(resolvedEvent.tool) kind=\(resolvedEvent.kind.rawValue) reason=no-change"
            )
            return
        }

        AIRuntimePollingService.shared.noteHookApplied(
            for: resolvedEvent.terminalID,
            reason: resolvedEvent.kind.rawValue
        )
        postRuntimeBridgeDidChange(kind: "ai-hook", asynchronously: true)
        if shouldBackfillCompletion {
            scheduleClaudeTurnCompletedBackfill(for: resolvedEvent)
        }
    }

    private func processOpencodeRuntimeEnvelope(_ envelope: AIToolUsageEnvelope) {
        let didChange = aiSessionStore.applyOpencodeEnvelope(envelope)
        guard didChange else {
            return
        }

        if let terminalID = UUID(uuidString: envelope.sessionId) {
            AIRuntimePollingService.shared.noteHookApplied(
                for: terminalID,
                reason: "opencode-runtime"
            )
        }
        postRuntimeBridgeDidChange(kind: "opencode-runtime", asynchronously: true)
    }

    private func processManualInterruptEvent(_ event: AIManualInterruptEvent) {
        let previousState = aiSessionStore.session(for: event.terminalID)
        let didChange = aiSessionStore.markInterrupted(
            terminalID: event.terminalID,
            updatedAt: event.updatedAt
        )
        guard didChange else {
            logger.log("runtime-socket", "drop kind=manual-interrupt terminal=\(event.terminalID.uuidString) reason=no-live-session")
            return
        }

        let nextState = aiSessionStore.session(for: event.terminalID)
        logger.log(
            "runtime-interrupt",
            "applied manual-interrupt terminal=\(event.terminalID.uuidString) prev=\(previousState?.state.rawValue ?? "nil") next=\(nextState?.state.rawValue ?? "nil") interrupted=\(nextState?.wasInterrupted == true)"
        )

        postRuntimeBridgeDidChange(kind: "manual-interrupt", asynchronously: true)
    }

    private func postRuntimeBridgeDidChange(kind: String, asynchronously: Bool = false) {
        let deliver = {
            NotificationCenter.default.post(name: .dmuxAIRuntimeActivityPulse, object: nil)
            NotificationCenter.default.post(
                name: .dmuxAIRuntimeBridgeDidChange,
                object: nil,
                userInfo: ["kind": kind]
            )
        }

        if asynchronously {
            Task { @MainActor in
                deliver()
            }
        } else {
            deliver()
        }
    }

    private func shouldAcceptRuntimeEvent(key: String, ttl: TimeInterval) -> Bool {
        let now = Date()
        recentRuntimeEventAtByKey = recentRuntimeEventAtByKey.filter {
            now.timeIntervalSince($0.value) < max(ttl * 4, 2)
        }
        if let previous = recentRuntimeEventAtByKey[key],
           now.timeIntervalSince(previous) < ttl {
            return false
        }
        recentRuntimeEventAtByKey[key] = now
        return true
    }

    private func runtimeSocketEventKey(kind: String, data: Data) -> String {
        if kind == "ai-hook",
           let event = try? JSONDecoder().decode(AIHookEvent.self, from: data) {
            let timeBucket = Int(event.updatedAt * 10)
            let sessionID = event.aiSessionID?.isEmpty == false ? event.aiSessionID! : event.terminalID.uuidString
            return "socket|ai-hook|\(sessionID)|\(event.kind.rawValue)|\(timeBucket)"
        }

        let sessionID: String = {
            guard let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
                return "unknown"
            }
            if let value = object["sessionId"] as? String, !value.isEmpty {
                return value
            }
            if let value = object["session_id"] as? String, !value.isEmpty {
                return value
            }
            if let value = object["terminalID"] as? String, !value.isEmpty {
                return value
            }
            if let value = object["terminalId"] as? String, !value.isEmpty {
                return value
            }
            return "unknown"
        }()
        let bucket = Int(Date().timeIntervalSince1970 * 2)
        return "socket|\(kind)|\(sessionID)|\(bucket)"
    }

    private func shouldScheduleClaudeTurnCompletedBackfill(
        originalEvent: AIHookEvent,
        resolvedEvent: AIHookEvent,
        currentSession: AISessionStore.TerminalSessionState?
    ) -> Bool {
        guard canonicalToolName(resolvedEvent.tool) == "claude",
              resolvedEvent.kind == .turnCompleted,
              resolvedEvent.metadata?.wasInterrupted != true else {
            return false
        }
        let previousTotal = currentSession?.committedTotalTokens ?? 0
        let resolvedTotal = resolvedEvent.totalTokens ?? 0
        return originalEvent.totalTokens == nil && resolvedTotal <= previousTotal
    }

    private func scheduleClaudeTurnCompletedBackfill(for event: AIHookEvent) {
        let terminalID = event.terminalID
        let expectedExternalSessionID = normalizedNonEmptyString(event.aiSessionID)
        let expectedInstanceID = normalizedNonEmptyString(event.terminalInstanceID)
        logger.log(
            "runtime-ingress",
            "schedule ai-hook-backfill terminal=\(terminalID.uuidString) tool=claude external=\(expectedExternalSessionID ?? "nil")"
        )

        Task { @MainActor [weak self] in
            await self?.runClaudeTurnCompletedBackfill(
                terminalID: terminalID,
                expectedExternalSessionID: expectedExternalSessionID,
                expectedInstanceID: expectedInstanceID
            )
        }
    }

    private func runClaudeTurnCompletedBackfill(
        terminalID: UUID,
        expectedExternalSessionID: String?,
        expectedInstanceID: String?
    ) async {
        for attempt in 1...24 {
            try? await Task.sleep(for: .milliseconds(500))

            guard let session = aiSessionStore.session(for: terminalID) else {
                logger.log(
                    "runtime-ingress",
                    "cancel ai-hook-backfill terminal=\(terminalID.uuidString) reason=missing-session"
                )
                return
            }

            guard session.state == .idle,
                  session.hasCompletedTurn,
                  session.wasInterrupted == false,
                  session.tool == "claude" else {
                logger.log(
                    "runtime-ingress",
                    "cancel ai-hook-backfill terminal=\(terminalID.uuidString) reason=session-advanced"
                )
                return
            }

            if let expectedExternalSessionID,
               normalizedNonEmptyString(session.aiSessionID) != expectedExternalSessionID {
                logger.log(
                    "runtime-ingress",
                    "cancel ai-hook-backfill terminal=\(terminalID.uuidString) reason=session-mismatch"
                )
                return
            }
            if let expectedInstanceID,
               normalizedNonEmptyString(session.terminalInstanceID) != expectedInstanceID {
                logger.log(
                    "runtime-ingress",
                    "cancel ai-hook-backfill terminal=\(terminalID.uuidString) reason=instance-mismatch"
                )
                return
            }

            guard let driver = toolDriverFactory.driver(for: session.tool),
                  let snapshot = await driver.runtimeSnapshot(for: session) else {
                continue
            }

            let previousTotal = session.committedTotalTokens
            let previousInput = session.committedInputTokens
            let previousOutput = session.committedOutputTokens
            let previousCached = session.committedCachedInputTokens
            let didChange = aiSessionStore.applyRuntimeSnapshot(
                terminalID: terminalID,
                snapshot: snapshot
            )
            let usageAdvanced = snapshot.totalTokens > previousTotal
                || snapshot.inputTokens > previousInput
                || snapshot.outputTokens > previousOutput
                || snapshot.cachedInputTokens > previousCached
            if didChange {
                logger.log(
                    "runtime-ingress",
                    "apply ai-hook-backfill terminal=\(terminalID.uuidString) tool=claude external=\(snapshot.externalSessionID ?? "nil") total=\(snapshot.totalTokens) attempt=\(attempt)"
                )
                postRuntimeBridgeDidChange(kind: "ai-hook-backfill", asynchronously: true)
            }
            if didChange && usageAdvanced {
                return
            }
        }

        logger.log(
            "runtime-ingress",
            "timeout ai-hook-backfill terminal=\(terminalID.uuidString) tool=claude external=\(expectedExternalSessionID ?? "nil")"
        )
    }
}
