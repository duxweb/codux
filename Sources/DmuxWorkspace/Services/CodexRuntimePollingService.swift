import Foundation

@MainActor
final class CodexRuntimePollingService {
    typealias TranscriptPathResolver = @Sendable (_ projectPath: String, _ aiSessionID: String) -> URL?

    static let shared = CodexRuntimePollingService()

    private struct FileSignature: Equatable, Sendable {
        var filePath: String
        var fileSize: Int
        var modifiedAt: TimeInterval
    }

    private struct TrackedTurn: Equatable, Sendable {
        var terminalID: UUID
        var projectPath: String
        var aiSessionID: String
        var transcriptPath: String?
        var turnSequence: UInt64
        var activatedAt: Date
        var lastFileSignature: FileSignature?
    }

    private struct InspectionResult: Sendable {
        var terminalID: UUID
        var turnSequence: UInt64
        var transcriptPath: String?
        var fileSignature: FileSignature?
        var resolution: AISessionStore.RuntimeResolution?
    }

    private let sessionStore: AISessionStore
    private let notificationCenter: NotificationCenter
    private let logger = AppDebugLog.shared
    private let transcriptPathResolver: TranscriptPathResolver

    private var runtimeBridgeObserver: NSObjectProtocol?
    private var pollingTask: Task<Void, Never>?
    private var trackedTurnsByTerminalID: [UUID: TrackedTurn] = [:]
    private var hasStarted = false

    init(
        sessionStore: AISessionStore = .shared,
        notificationCenter: NotificationCenter = .default,
        transcriptPathResolver: @escaping TranscriptPathResolver = { projectPath, aiSessionID in
            AIRuntimeSourceLocator.codexRolloutPath(projectPath: projectPath, externalSessionID: aiSessionID)
        }
    ) {
        self.sessionStore = sessionStore
        self.notificationCenter = notificationCenter
        self.transcriptPathResolver = transcriptPathResolver
    }

    func start() {
        guard hasStarted == false else {
            refreshLoopState()
            return
        }
        hasStarted = true
        runtimeBridgeObserver = notificationCenter.addObserver(
            forName: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            queue: .main
        ) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.refreshLoopState()
            }
        }
        refreshLoopState()
    }

    func reset() {
        trackedTurnsByTerminalID.removeAll()
        pollingTask?.cancel()
        pollingTask = nil
    }

    func pollOnceForTesting() async {
        refreshTrackedTurns()
        await evaluateTrackedTurnsOnce()
    }

    private func refreshLoopState() {
        refreshTrackedTurns()
        guard trackedTurnsByTerminalID.isEmpty == false else {
            pollingTask?.cancel()
            pollingTask = nil
            return
        }
        guard pollingTask == nil else {
            return
        }

        pollingTask = Task { [weak self] in
            await self?.pollingLoop()
        }
    }

    private func pollingLoop() async {
        while Task.isCancelled == false {
            refreshTrackedTurns()
            guard trackedTurnsByTerminalID.isEmpty == false else {
                break
            }

            await evaluateTrackedTurnsOnce()

            let interval = pollingInterval(for: Array(trackedTurnsByTerminalID.values))
            let nanoseconds = UInt64((interval * 1_000_000_000).rounded())
            try? await Task.sleep(nanoseconds: nanoseconds)
        }

        pollingTask = nil
        if trackedTurnsByTerminalID.isEmpty == false {
            refreshLoopState()
        }
    }

    private func refreshTrackedTurns() {
        let nextTargets = sessionStore.codexPollingTargets()
        let existing = trackedTurnsByTerminalID
        let now = Date()
        var nextTrackedTurns: [UUID: TrackedTurn] = [:]

        for target in nextTargets {
            if let current = existing[target.terminalID],
               current.turnSequence == target.turnSequence,
               current.aiSessionID == target.aiSessionID,
               current.projectPath == target.projectPath {
                let transcriptPath = target.transcriptPath ?? current.transcriptPath
                let didChangeTranscript = transcriptPath != current.transcriptPath
                nextTrackedTurns[target.terminalID] = TrackedTurn(
                    terminalID: current.terminalID,
                    projectPath: current.projectPath,
                    aiSessionID: current.aiSessionID,
                    transcriptPath: transcriptPath,
                    turnSequence: current.turnSequence,
                    activatedAt: current.activatedAt,
                    lastFileSignature: didChangeTranscript ? nil : current.lastFileSignature
                )
            } else {
                nextTrackedTurns[target.terminalID] = TrackedTurn(
                    terminalID: target.terminalID,
                    projectPath: target.projectPath,
                    aiSessionID: target.aiSessionID,
                    transcriptPath: target.transcriptPath,
                    turnSequence: target.turnSequence,
                    activatedAt: now,
                    lastFileSignature: nil
                )
            }
        }

        trackedTurnsByTerminalID = nextTrackedTurns
    }

    private func evaluateTrackedTurnsOnce() async {
        let trackedTurns = Array(trackedTurnsByTerminalID.values)
        guard trackedTurns.isEmpty == false else {
            return
        }

        let transcriptPathResolver = transcriptPathResolver
        let results = await withTaskGroup(of: InspectionResult.self, returning: [InspectionResult].self) { group in
            for trackedTurn in trackedTurns {
                group.addTask {
                    await Self.inspect(trackedTurn: trackedTurn, transcriptPathResolver: transcriptPathResolver)
                }
            }

            var collected: [InspectionResult] = []
            for await result in group {
                collected.append(result)
            }
            return collected
        }

        var didChangeStore = false
        for result in results {
            if var trackedTurn = trackedTurnsByTerminalID[result.terminalID],
               trackedTurn.turnSequence == result.turnSequence {
                trackedTurn.transcriptPath = result.transcriptPath ?? trackedTurn.transcriptPath
                trackedTurn.lastFileSignature = result.fileSignature ?? trackedTurn.lastFileSignature
                trackedTurnsByTerminalID[result.terminalID] = trackedTurn
            }

            guard let resolution = result.resolution else {
                continue
            }
            if sessionStore.applyRuntimeResolution(resolution, source: "codex-polling") {
                didChangeStore = true
            }
        }

        if didChangeStore {
            logger.log("codex-polling", "applied runtime resolutions count=\(results.filter { $0.resolution != nil }.count)")
            postRuntimeBridgeDidChange()
        }

        refreshTrackedTurns()
    }

    private func pollingInterval(for trackedTurns: [TrackedTurn]) -> TimeInterval {
        let youngestAge = trackedTurns
            .map { Date().timeIntervalSince($0.activatedAt) }
            .min() ?? 1
        switch youngestAge {
        case ..<3:
            return 0.25
        case ..<15:
            return 0.5
        default:
            return 1
        }
    }

    private func postRuntimeBridgeDidChange() {
        notificationCenter.post(name: .dmuxAIRuntimeActivityPulse, object: nil)
        notificationCenter.post(
            name: .dmuxAIRuntimeBridgeDidChange,
            object: nil,
            userInfo: ["kind": "codex-polling"]
        )
    }

    private static func inspect(
        trackedTurn: TrackedTurn,
        transcriptPathResolver: TranscriptPathResolver
    ) async -> InspectionResult {
        let transcriptURL: URL? = {
            if let transcriptPath = trackedTurn.transcriptPath, transcriptPath.isEmpty == false {
                return URL(fileURLWithPath: transcriptPath)
            }
            return transcriptPathResolver(trackedTurn.projectPath, trackedTurn.aiSessionID)
        }()

        guard let transcriptURL else {
            return InspectionResult(
                terminalID: trackedTurn.terminalID,
                turnSequence: trackedTurn.turnSequence,
                transcriptPath: nil,
                fileSignature: nil,
                resolution: nil
            )
        }

        let fileSignature = fileSignature(for: transcriptURL)
        if let fileSignature,
           fileSignature == trackedTurn.lastFileSignature {
            return InspectionResult(
                terminalID: trackedTurn.terminalID,
                turnSequence: trackedTurn.turnSequence,
                transcriptPath: transcriptURL.path,
                fileSignature: fileSignature,
                resolution: nil
            )
        }

        guard let parsedState = parseCodexRolloutRuntimeState(fileURL: transcriptURL),
              parsedState.wasInterrupted || parsedState.hasCompletedTurn || parsedState.responseState == .idle else {
            return InspectionResult(
                terminalID: trackedTurn.terminalID,
                turnSequence: trackedTurn.turnSequence,
                transcriptPath: transcriptURL.path,
                fileSignature: fileSignature,
                resolution: nil
            )
        }

        let updatedAt = parsedState.completedAt ?? parsedState.updatedAt ?? Date().timeIntervalSince1970
        return InspectionResult(
            terminalID: trackedTurn.terminalID,
            turnSequence: trackedTurn.turnSequence,
            transcriptPath: transcriptURL.path,
            fileSignature: fileSignature,
            resolution: AISessionStore.RuntimeResolution(
                terminalID: trackedTurn.terminalID,
                turnSequence: trackedTurn.turnSequence,
                updatedAt: updatedAt,
                model: parsedState.model,
                totalTokens: parsedState.totalTokens,
                transcriptPath: transcriptURL.path,
                wasInterrupted: parsedState.wasInterrupted,
                hasCompletedTurn: parsedState.hasCompletedTurn || parsedState.wasInterrupted == false
            )
        )
    }

    private static func fileSignature(for fileURL: URL) -> FileSignature? {
        guard let values = try? fileURL.resourceValues(forKeys: [.contentModificationDateKey, .fileSizeKey]),
              let modifiedAt = values.contentModificationDate?.timeIntervalSince1970,
              let fileSize = values.fileSize else {
            return nil
        }
        return FileSignature(
            filePath: fileURL.standardizedFileURL.path,
            fileSize: fileSize,
            modifiedAt: modifiedAt
        )
    }
}
