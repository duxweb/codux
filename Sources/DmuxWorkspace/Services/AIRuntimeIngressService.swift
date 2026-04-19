import Darwin
import Foundation

extension Notification.Name {
    static let dmuxAIRuntimeBridgeDidChange = Notification.Name("dmuxAIRuntimeBridgeDidChange")
}

private actor AIRuntimeSocketEventPipeline {
    static let shared = AIRuntimeSocketEventPipeline()

    private var tailTasksByKey: [String: Task<Void, Never>] = [:]
    private var latestTokenByKey: [String: UInt64] = [:]
    private var nextToken: UInt64 = 0

    func enqueue(key: String, operation: @escaping @Sendable () async -> Void) {
        nextToken &+= 1
        let token = nextToken
        let previous = tailTasksByKey[key]
        latestTokenByKey[key] = token

        let task = Task {
            _ = await previous?.result
            await operation()
            finish(key: key, token: token)
        }

        tailTasksByKey[key] = task
    }

    private func finish(key: String, token: UInt64) {
        guard latestTokenByKey[key] == token else {
            return
        }
        latestTokenByKey[key] = nil
        tailTasksByKey[key] = nil
    }
}

@MainActor
final class AIRuntimeIngressService {
    static let shared = AIRuntimeIngressService()

    private let bridgeService = AIRuntimeBridgeService()
    private let logger = AppDebugLog.shared
    private let runtimeStore = AIRuntimeStateStore.shared
    private let usageStore = AIUsageStore()
    private let toolDriverFactory = AIToolDriverFactory.shared
    private let appLaunchCutoff = Date().timeIntervalSince1970
    private var runtimeSocketListenerFD: Int32 = -1
    private var runtimeSocketWatcher: DispatchSourceRead?
    private var runtimeSourceWatchersByPath: [String: DispatchSourceFileSystemObject] = [:]
    private var runtimeResponseSyncTasksByTool: [String: Task<Void, Never>] = [:]
    private var recentRuntimeEventAtByKey: [String: Date] = [:]
    private var latestProjects: [Project] = []
    private var liveEnvelopesBySessionID: [UUID: AIToolUsageEnvelope] = [:]
    private var responsePayloadsBySessionID: [UUID: AIResponseStatePayload] = [:]

    private init() {}

    func resetEphemeralState() {
        liveEnvelopesBySessionID.removeAll()
        responsePayloadsBySessionID.removeAll()
        bridgeService.clearAllClaudeSessionMappings()
        bridgeService.clearLegacyLiveRuntimeState()
    }

    func importRuntime(projects: [Project], projectID: UUID? = nil, liveSessionCutoff: Double? = nil) -> [AIToolUsageEnvelope] {
        latestProjects = projects
        let effectiveLiveSessionCutoff = max(liveSessionCutoff ?? 0, appLaunchCutoff)
        let allowedProjectIDs: Set<UUID> = {
            if let projectID {
                return [projectID]
            }
            return Set(projects.map(\.id))
        }()

        let liveEnvelopes = liveEnvelopesBySessionID.values
            .filter { envelope in
                guard let payloadProjectID = UUID(uuidString: envelope.projectId),
                      allowedProjectIDs.contains(payloadProjectID) else {
                    return false
                }
                let startedAt = envelope.startedAt ?? envelope.updatedAt
                return startedAt >= effectiveLiveSessionCutoff - 2
            }
            .filter { envelope in
                matchesCurrentTerminalInstance(
                    sessionIDString: envelope.sessionId,
                    sessionInstanceID: envelope.sessionInstanceId
                )
            }
            .map { effectiveLiveEnvelope($0) }
            .sorted { $0.updatedAt > $1.updatedAt }

        let responseStates = responsePayloadsBySessionID.values
            .filter { payload in
                guard let payloadProjectID = UUID(uuidString: payload.projectId),
                      allowedProjectIDs.contains(payloadProjectID) else {
                    return false
                }
                return matchesCurrentTerminalInstance(
                    sessionIDString: payload.sessionId,
                    sessionInstanceID: payload.sessionInstanceId
                )
            }

        for envelope in liveEnvelopes where envelope.status == "running" {
            runtimeStore.applyLiveEnvelope(envelope)
        }

        let liveSessionIDs = Set(liveEnvelopes.compactMap { UUID(uuidString: $0.sessionId) })
        for payload in responseStates
        where toolDriverFactory.appliesGenericResponsePayloads(for: payload.tool)
            && UUID(uuidString: payload.sessionId).map(liveSessionIDs.contains) == true {
            runtimeStore.applyResponsePayload(payload)
        }

        if let projectID {
            runtimeStore.prune(projectID: projectID, liveSessionIDs: liveSessionIDs)
        } else {
            let liveSessionIDsByProjectID = Dictionary(grouping: liveEnvelopes.compactMap { envelope -> (UUID, UUID)? in
                guard envelope.status == "running",
                      let projectID = UUID(uuidString: envelope.projectId),
                      let sessionID = UUID(uuidString: envelope.sessionId) else {
                    return nil
                }
                return (projectID, sessionID)
            }, by: \.0).mapValues { Set($0.map(\.1)) }

            for project in projects {
                runtimeStore.prune(projectID: project.id, liveSessionIDs: liveSessionIDsByProjectID[project.id] ?? [])
            }
        }

        return liveEnvelopes
    }

    private func matchesCurrentTerminalInstance(sessionIDString: String, sessionInstanceID: String?) -> Bool {
        guard let sessionID = UUID(uuidString: sessionIDString) else {
            return false
        }
        guard let expectedInstanceID = SwiftTermTerminalRegistry.shared.sessionInstanceID(for: sessionID) else {
            return true
        }
        guard let sessionInstanceID, !sessionInstanceID.isEmpty else {
            return false
        }
        return sessionInstanceID == expectedInstanceID
    }

    private func postRuntimeBridgeDidChange(
        kind: String,
        switchedSessionID: UUID? = nil,
        tool: String? = nil,
        clearedSessionID: UUID? = nil,
        clearedTool: String? = nil
    ) {
        var userInfo: [String: Any] = ["kind": kind]
        if let switchedSessionID, let tool {
            userInfo["didSwitchExternalSession"] = true
            userInfo["sessionID"] = switchedSessionID.uuidString
            userInfo["tool"] = canonicalToolName(tool)
        }
        if let clearedSessionID, let clearedTool {
            userInfo["didClearRuntimeSession"] = true
            userInfo["clearedSessionID"] = clearedSessionID.uuidString
            userInfo["clearedTool"] = canonicalToolName(clearedTool)
        }
        NotificationCenter.default.post(
            name: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            userInfo: userInfo
        )
    }

    func runtimeSourceDescriptors(for liveEnvelopes: [AIToolUsageEnvelope], projects: [Project]) -> [(path: String, tool: String)] {
        liveEnvelopes
            .filter { $0.status == "running" }
            .compactMap { envelope -> [(path: String, tool: String)]? in
                guard let projectID = UUID(uuidString: envelope.projectId),
                      let project = projects.first(where: { $0.id == projectID }),
                      let driver = toolDriverFactory.driver(for: envelope.tool) else {
                    return nil
                }
                return driver.runtimeSourceDescriptors(project: project, envelope: envelope).map {
                    (path: $0.path, tool: driver.id)
                }
            }
            .flatMap { $0 }
    }

    func canonicalToolName(_ tool: String) -> String {
        toolDriverFactory.canonicalToolName(tool)
    }

    func runtimeRefreshInterval(for tool: String) -> TimeInterval {
        toolDriverFactory.runtimeRefreshInterval(for: tool)
    }

    func isRealtimeTool(_ tool: String) -> Bool {
        toolDriverFactory.isRealtimeTool(tool)
    }

    func realtimeToolsNeedingSync(in liveEnvelopes: [AIToolUsageEnvelope], excluding excluded: Set<String> = []) -> Set<String> {
        Set(
            liveEnvelopes
                .filter { isRealtimeTool($0.tool) }
                .map { canonicalToolName($0.tool) }
        ).subtracting(excluded)
    }

    func clearResponseState(sessionID: UUID) {
        responsePayloadsBySessionID[sessionID] = nil
    }

    func clearLiveState(sessionID: UUID) {
        liveEnvelopesBySessionID[sessionID] = nil
        responsePayloadsBySessionID[sessionID] = nil
    }

    func startWatching() {
        runtimeSocketWatcher?.cancel()
        runtimeSocketWatcher = nil
        if runtimeSocketListenerFD >= 0 {
            close(runtimeSocketListenerFD)
            runtimeSocketListenerFD = -1
        }

        let socketPath = bridgeService.runtimeEventSocketURL().path
        unlink(socketPath)

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
            guard let self else { return }
            if self.runtimeSocketListenerFD >= 0 {
                close(self.runtimeSocketListenerFD)
                self.runtimeSocketListenerFD = -1
            }
            unlink(socketPath)
        }
        watcher.resume()
        runtimeSocketWatcher = watcher
    }

    func refreshRuntimeSources(projects: [Project], liveEnvelopes: [AIToolUsageEnvelope]) {
        latestProjects = projects
        let nextDescriptors = liveEnvelopes
            .filter { $0.status == "running" }
            .compactMap { envelope -> [(AIToolRuntimeSourceDescriptor, String)]? in
                guard let projectID = UUID(uuidString: envelope.projectId),
                      let project = projects.first(where: { $0.id == projectID }),
                      let driver = toolDriverFactory.driver(for: envelope.tool) else {
                    return nil
                }
                return driver.runtimeSourceDescriptors(project: project, envelope: envelope).map {
                    ($0, driver.id)
                }
            }
            .flatMap { $0 }
        let nextPaths = Set(nextDescriptors.map(\.0.path))

        for (path, watcher) in runtimeSourceWatchersByPath where !nextPaths.contains(path) {
            watcher.cancel()
            runtimeSourceWatchersByPath[path] = nil
        }

        for descriptor in nextDescriptors {
            if runtimeSourceWatchersByPath[descriptor.0.path] != nil {
                continue
            }

            let fd = open(descriptor.0.path, O_EVTONLY)
            guard fd >= 0 else {
                continue
            }

            let watcher = DispatchSource.makeFileSystemObjectSource(
                fileDescriptor: fd,
                eventMask: [.write, .extend, .rename, .delete],
                queue: .main
            )
            watcher.setEventHandler { [weak self] in
                guard let self else {
                    return
                }
                let events = watcher.data
                if descriptor.0.watchKind == .file && (events.contains(.rename) || events.contains(.delete)) {
                    watcher.cancel()
                    self.runtimeSourceWatchersByPath[descriptor.0.path] = nil
                    NotificationCenter.default.post(
                        name: .dmuxAIRuntimeBridgeDidChange,
                        object: nil,
                        userInfo: ["kind": "runtime-source"]
                    )
                    return
                }

                let dedupeKey = "source|\(descriptor.1)|\(descriptor.0.watchKind.rawValue)|\(descriptor.0.path)"
                guard self.shouldAcceptRuntimeEvent(key: dedupeKey, ttl: 0.18) else {
                    self.logger.log(
                        "runtime-ingress",
                        "drop source tool=\(descriptor.1) path=\(descriptor.0.path) reason=dedupe"
                    )
                    return
                }
                self.scheduleRuntimeIngressHandling(for: descriptor.0, tool: descriptor.1)
            }
            watcher.setCancelHandler {
                close(fd)
            }
            watcher.resume()
            runtimeSourceWatchersByPath[descriptor.0.path] = watcher
        }
    }

    private func scheduleRuntimeIngressHandling(for descriptor: AIToolRuntimeSourceDescriptor, tool: String) {
        let canonicalTool = canonicalToolName(tool)
        runtimeResponseSyncTasksByTool[canonicalTool]?.cancel()

        let currentProjects = latestProjects
        let liveEnvelopes = importRuntime(projects: currentProjects)
        let debounceDelay = max(0.08, min(0.24, runtimeRefreshInterval(for: canonicalTool) * 0.35))

        runtimeResponseSyncTasksByTool[canonicalTool] = Task { [weak self] in
            try? await Task.sleep(for: .seconds(debounceDelay))
            guard !Task.isCancelled else {
                return
            }

            guard let self, !Task.isCancelled else {
                return
            }

            if let driver = self.toolDriverFactory.driver(for: canonicalTool),
               let customUpdate = await driver.handleRuntimeIngressEvent(
                   descriptor: descriptor,
                   projects: currentProjects,
                   liveEnvelopes: liveEnvelopes
               ) {
                let hasActivityUpdate = !customUpdate.responsePayloads.isEmpty || !customUpdate.runtimeSnapshotsBySessionID.isEmpty
                var switchedSessionID: UUID?
                var switchedTool: String?
                for payload in customUpdate.responsePayloads {
                    self.runtimeStore.applyResponsePayload(payload)
                    self.logger.log(
                        "runtime-ingress",
                        "payload tool=\(canonicalTool) session=\(payload.sessionId) response=\(payload.responseState.rawValue) source=\(payload.source?.rawValue ?? "unknown")"
                    )
                }
                for (sessionID, snapshot) in customUpdate.runtimeSnapshotsBySessionID {
                    let result = self.runtimeStore.applyRuntimeSnapshot(sessionID: sessionID, snapshot: snapshot)
                    if result?.didSwitchExternalSession == true {
                        switchedSessionID = sessionID
                        switchedTool = snapshot.tool
                    }
                    self.logger.log(
                        "runtime-ingress",
                        "snapshot tool=\(snapshot.tool) session=\(sessionID.uuidString) model=\(snapshot.model ?? "nil") total=\(snapshot.totalTokens) response=\(snapshot.responseState?.rawValue ?? "nil") source=\(snapshot.source.rawValue)"
                    )
                }
                self.runtimeResponseSyncTasksByTool[canonicalTool] = nil
                if hasActivityUpdate {
                    NotificationCenter.default.post(
                        name: .dmuxAIRuntimeActivityPulse,
                        object: nil
                    )
                }
                self.postRuntimeBridgeDidChange(
                    kind: "runtime-source",
                    switchedSessionID: switchedSessionID,
                    tool: switchedTool
                )
                return
            }

            self.runtimeResponseSyncTasksByTool[canonicalTool] = nil
            NotificationCenter.default.post(
                name: .dmuxAIRuntimeBridgeDidChange,
                object: nil,
                userInfo: ["kind": "runtime-source"]
            )
        }
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

        switch kind {
        case "usage":
            guard var envelope = try? JSONDecoder().decode(AIToolUsageEnvelope.self, from: payloadData) else {
                logger.log("runtime-socket", "drop kind=usage reason=decode")
                return
            }
            envelope.source = envelope.source ?? .socket
            guard shouldAcceptRuntimeEvent(
                key: runtimeSocketEventKey(kind: kind, data: payloadData),
                ttl: 0.9
            ) else {
                logger.log("runtime-socket", "drop kind=usage session=\(envelope.sessionId) reason=dedupe")
                return
            }
            guard matchesCurrentTerminalInstance(
                sessionIDString: envelope.sessionId,
                sessionInstanceID: envelope.sessionInstanceId
            ) else {
                logger.log(
                    "runtime-socket",
                    "drop kind=usage session=\(envelope.sessionId) instance=\(envelope.sessionInstanceId ?? "nil") reason=instance-mismatch"
                )
                return
            }
            guard let sessionID = UUID(uuidString: envelope.sessionId) else {
                logger.log("runtime-socket", "drop kind=usage session=\(envelope.sessionId) reason=invalid-session-id")
                return
            }
            let effectiveEnvelope = effectiveLiveEnvelope(envelope)
            persistManagedRealtimeEnvelope(effectiveEnvelope)
            if effectiveEnvelope.status == "running" {
                liveEnvelopesBySessionID[sessionID] = effectiveEnvelope
                runtimeStore.applyLiveEnvelope(effectiveEnvelope)
            } else {
                liveEnvelopesBySessionID[sessionID] = nil
                responsePayloadsBySessionID[sessionID] = nil
                runtimeStore.clearSession(sessionID)
            }
            logger.log(
                "runtime-socket",
                "accept kind=usage tool=\(canonicalToolName(effectiveEnvelope.tool)) session=\(effectiveEnvelope.sessionId) status=\(effectiveEnvelope.status) instance=\(effectiveEnvelope.sessionInstanceId ?? "nil") source=\(effectiveEnvelope.source?.rawValue ?? "unknown")"
            )
            if effectiveEnvelope.status == "running" {
                postRuntimeBridgeDidChange(kind: "runtime-socket")
            } else {
                postRuntimeBridgeDidChange(
                    kind: "runtime-socket",
                    clearedSessionID: sessionID,
                    clearedTool: effectiveEnvelope.tool
                )
            }

        case "response":
            guard var payload = try? JSONDecoder().decode(AIResponseStatePayload.self, from: payloadData) else {
                logger.log("runtime-socket", "drop kind=response reason=decode")
                return
            }
            payload.source = payload.source ?? .socket
            guard shouldAcceptRuntimeEvent(
                key: runtimeSocketEventKey(kind: kind, data: payloadData),
                ttl: 0.9
            ) else {
                logger.log("runtime-socket", "drop kind=response session=\(payload.sessionId) reason=dedupe")
                return
            }
            guard matchesCurrentTerminalInstance(
                sessionIDString: payload.sessionId,
                sessionInstanceID: payload.sessionInstanceId
            ) else {
                logger.log(
                    "runtime-socket",
                    "drop kind=response session=\(payload.sessionId) instance=\(payload.sessionInstanceId ?? "nil") reason=instance-mismatch"
                )
                return
            }
            guard let sessionID = UUID(uuidString: payload.sessionId) else {
                logger.log("runtime-socket", "drop kind=response session=\(payload.sessionId) reason=invalid-session-id")
                return
            }
            persistManagedRealtimeResponse(payload)
            responsePayloadsBySessionID[sessionID] = payload
            if liveEnvelopesBySessionID[sessionID] != nil,
               toolDriverFactory.appliesGenericResponsePayloads(for: payload.tool) {
                runtimeStore.applyResponsePayload(payload)
            }
            logger.log(
                "runtime-socket",
                "accept kind=response tool=\(canonicalToolName(payload.tool)) session=\(payload.sessionId) state=\(payload.responseState.rawValue) instance=\(payload.sessionInstanceId ?? "nil") source=\(payload.source?.rawValue ?? "unknown")"
            )
            postRuntimeBridgeDidChange(kind: "runtime-socket")

        default:
            guard shouldAcceptRuntimeEvent(
                key: runtimeSocketEventKey(kind: kind, data: payloadData),
                ttl: 1.5
            ) else {
                logger.log("runtime-socket", "drop kind=\(kind) reason=dedupe")
                return
            }
            if let pipelineKey = runtimeEventPipelineKey(kind: kind, payloadData: payloadData) {
                Task { [weak self] in
                    guard let self else {
                        return
                    }
                    await AIRuntimeSocketEventPipeline.shared.enqueue(key: pipelineKey) { [weak self] in
                        guard let self else {
                            return
                        }
                        await self.processManagedRuntimeSocketEvent(kind: kind, payloadData: payloadData)
                    }
                }
                return
            }
            Task { [weak self] in
                guard let self else {
                    return
                }
                await self.processManagedRuntimeSocketEvent(kind: kind, payloadData: payloadData)
            }
        }
    }

    private func runtimeEventPipelineKey(kind: String, payloadData: Data) -> String? {
        guard kind == "codex-hook",
              let envelope = try? JSONDecoder().decode(CodexHookRuntimeEnvelope.self, from: payloadData),
              !envelope.dmuxSessionId.isEmpty else {
            return nil
        }
        return "\(kind)|\(envelope.dmuxSessionId)"
    }

    private func processManagedRuntimeSocketEvent(kind: String, payloadData: Data) async {
        let context = await MainActor.run {
            (
                projects: latestProjects,
                liveEnvelopes: liveEnvelopesBySessionID.values.sorted { $0.updatedAt > $1.updatedAt },
                existingRuntime: currentRuntimeSnapshotsBySessionID()
            )
        }

        guard let update = await AIToolDriverFactory.shared.handleRuntimeSocketEvent(
            kind: kind,
            payloadData: payloadData,
            projects: context.projects,
            liveEnvelopes: context.liveEnvelopes,
            existingRuntime: context.existingRuntime
        ) else {
            return
        }

        await MainActor.run {
            var switchedSessionID: UUID?
            var switchedTool: String?
            for payload in update.responsePayloads {
                if let sessionID = UUID(uuidString: payload.sessionId) {
                    responsePayloadsBySessionID[sessionID] = payload
                }
                runtimeStore.applyResponsePayload(payload)
            }
            for (sessionID, snapshot) in update.runtimeSnapshotsBySessionID {
                let result = runtimeStore.applyRuntimeSnapshot(sessionID: sessionID, snapshot: snapshot)
                if result?.didSwitchExternalSession == true {
                    switchedSessionID = sessionID
                    switchedTool = snapshot.tool
                }
            }
            postRuntimeBridgeDidChange(
                kind: "runtime-socket",
                switchedSessionID: switchedSessionID,
                tool: switchedTool
            )
        }
    }

    private func currentRuntimeSnapshotsBySessionID() -> [UUID: AIRuntimeContextSnapshot] {
        let snapshots = latestProjects.flatMap { project in
            runtimeStore.liveSnapshots(projectID: project.id)
        }
        return Dictionary(uniqueKeysWithValues: snapshots.map { snapshot in
            (
                snapshot.sessionID,
                AIRuntimeContextSnapshot(
                    tool: snapshot.tool ?? "",
                    externalSessionID: snapshot.externalSessionID,
                    model: snapshot.model,
                    inputTokens: snapshot.currentInputTokens,
                    outputTokens: snapshot.currentOutputTokens,
                    totalTokens: snapshot.currentTotalTokens,
                    updatedAt: snapshot.updatedAt.timeIntervalSince1970,
                    responseState: snapshot.responseState,
                    source: .probe
                )
            )
        })
    }

    private func effectiveLiveEnvelope(_ envelope: AIToolUsageEnvelope) -> AIToolUsageEnvelope {
        guard toolDriverFactory.allowsRuntimeExternalSessionSwitch(for: envelope.tool),
              let sessionID = UUID(uuidString: envelope.sessionId),
              let binding = runtimeStore.terminalBindingsByID[sessionID],
              let existingExternalSessionID = normalizedExternalSessionID(binding.lastKnownExternalSessionID) else {
            return envelope
        }

        let incomingExternalSessionID = normalizedExternalSessionID(envelope.externalSessionID)
        guard existingExternalSessionID != incomingExternalSessionID else {
            return envelope
        }

        var effectiveEnvelope = envelope
        effectiveEnvelope.externalSessionID = existingExternalSessionID
        if (effectiveEnvelope.model?.isEmpty ?? true),
           let existingModel = normalizedNonEmptyString(binding.lastKnownModel) {
            effectiveEnvelope.model = existingModel
        }
        if effectiveEnvelope.responseState == nil {
            effectiveEnvelope.responseState = binding.responseState
        }

        logger.log(
            "runtime-ingress",
            "normalize live session=\(sessionID.uuidString) tool=\(canonicalToolName(envelope.tool)) external=\(incomingExternalSessionID ?? "nil")->\(existingExternalSessionID)"
        )
        return effectiveEnvelope
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
        var hasher = Hasher()
        hasher.combine(kind)
        hasher.combine(data.count)
        for byte in data.prefix(128) {
            hasher.combine(byte)
        }
        return "socket|\(kind)|\(hasher.finalize())"
    }

    private func normalizedExternalSessionID(_ value: String?) -> String? {
        normalizedNonEmptyString(value)
    }

    private func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private func shouldPersistManagedRealtime(tool: String) -> Bool {
        _ = tool
        return false
    }

    private func persistManagedRealtimeEnvelope(_ envelope: AIToolUsageEnvelope) {
        guard shouldPersistManagedRealtime(tool: envelope.tool),
              let projectID = UUID(uuidString: envelope.projectId) else {
            return
        }

        let project = latestProjects.first(where: { $0.id == projectID })
        let projectPath = project?.path ?? envelope.projectPath
        let projectName = project?.name ?? envelope.projectName
        guard let projectPath, !projectPath.isEmpty else {
            return
        }

        let canonicalTool = canonicalToolName(envelope.tool)
        let recordID = managedRecordID(for: envelope, canonicalTool: canonicalTool)
        let existing = usageStore.managedRealtimeRecord(recordID: recordID)
        let startedAt = envelope.startedAt.map { Date(timeIntervalSince1970: $0) } ?? Date(timeIntervalSince1970: envelope.updatedAt)
        let updatedAt = Date(timeIntervalSince1970: envelope.updatedAt)
        let finishedAt = envelope.finishedAt.map { Date(timeIntervalSince1970: $0) }
        let maxContextUsagePercent = max(existing?.maxContextUsagePercent ?? 0, envelope.contextUsagePercent ?? 0)

        let record = AIManagedRealtimeSessionRecord(
            recordID: recordID,
            invocationID: envelope.invocationId ?? existing?.invocationID,
            runtimeSessionID: envelope.sessionId,
            externalSessionID: envelope.externalSessionID ?? existing?.externalSessionID,
            projectID: projectID,
            projectPath: projectPath,
            projectName: projectName,
            sessionTitle: resolvedManagedSessionTitle(envelope: envelope, existing: existing, fallbackProjectName: projectName),
            tool: canonicalTool,
            model: envelope.model ?? existing?.model,
            startedAt: min(existing?.startedAt ?? startedAt, startedAt),
            updatedAt: max(existing?.updatedAt ?? updatedAt, updatedAt),
            finishedAt: finishedAt ?? existing?.finishedAt,
            status: envelope.status,
            responseState: envelope.responseState ?? existing?.responseState,
            totalInputTokens: max(existing?.totalInputTokens ?? 0, envelope.inputTokens ?? 0),
            totalOutputTokens: max(existing?.totalOutputTokens ?? 0, envelope.outputTokens ?? 0),
            totalTokens: max(existing?.totalTokens ?? 0, envelope.totalTokens ?? 0),
            maxContextUsagePercent: maxContextUsagePercent > 0 ? maxContextUsagePercent : existing?.maxContextUsagePercent
        )
        usageStore.saveManagedRealtimeRecord(record)
        logger.log(
            "managed-ai-session",
            "save tool=\(canonicalTool) record=\(recordID) project=\(projectName) status=\(record.status) external=\(record.externalSessionID ?? "nil") invocation=\(record.invocationID ?? "nil")"
        )
    }

    private func persistManagedRealtimeResponse(_ payload: AIResponseStatePayload) {
        guard shouldPersistManagedRealtime(tool: payload.tool),
              let projectID = UUID(uuidString: payload.projectId) else {
            return
        }

        let project = latestProjects.first(where: { $0.id == projectID })
        let projectPath = project?.path ?? payload.projectPath
        let projectName = project?.name
        guard let projectPath, !projectPath.isEmpty else {
            return
        }

        let canonicalTool = canonicalToolName(payload.tool)
        let recordID = managedRecordID(for: payload, canonicalTool: canonicalTool)
        guard var existing = usageStore.managedRealtimeRecord(recordID: recordID) else {
            return
        }

        existing.responseState = payload.responseState
        existing.updatedAt = max(existing.updatedAt, Date(timeIntervalSince1970: payload.updatedAt))
        existing.projectPath = projectPath
        if let projectName {
            existing.projectName = projectName
        }
        usageStore.saveManagedRealtimeRecord(existing)
        logger.log(
            "managed-ai-session",
            "response tool=\(canonicalTool) record=\(recordID) state=\(payload.responseState.rawValue) project=\(existing.projectName)"
        )
    }

    private func managedRecordID(for envelope: AIToolUsageEnvelope, canonicalTool: String) -> String {
        if let invocationID = envelope.invocationId, !invocationID.isEmpty {
            return invocationID
        }
        if let externalSessionID = envelope.externalSessionID, !externalSessionID.isEmpty {
            return "\(canonicalTool):\(externalSessionID)"
        }
        let startedAt = envelope.startedAt ?? envelope.updatedAt
        return "\(canonicalTool):\(envelope.sessionId):\(String(format: "%.3f", startedAt))"
    }

    private func managedRecordID(for payload: AIResponseStatePayload, canonicalTool: String) -> String {
        if let invocationID = payload.invocationId, !invocationID.isEmpty {
            return invocationID
        }
        return "\(canonicalTool):\(payload.sessionId):\(String(format: "%.3f", payload.updatedAt))"
    }

    private func resolvedManagedSessionTitle(
        envelope: AIToolUsageEnvelope,
        existing: AIManagedRealtimeSessionRecord?,
        fallbackProjectName: String
    ) -> String {
        let candidate = envelope.sessionTitle.trimmingCharacters(in: .whitespacesAndNewlines)
        if !candidate.isEmpty, candidate != "Terminal" {
            return candidate
        }
        if let existingTitle = existing?.sessionTitle, !existingTitle.isEmpty {
            return existingTitle
        }
        return fallbackProjectName
    }
}
