import Foundation

struct CodexBarBinaryLocator {
    static func resolve(binaryName: String, environment: [String: String] = ProcessInfo.processInfo.environment) -> String? {
        if binaryName.contains("/") {
            return FileManager.default.isExecutableFile(atPath: binaryName) ? binaryName : nil
        }

        if let resolved = resolveInPath(binaryName: binaryName, path: environment["PATH"]) {
            return resolved
        }

        return resolveWithLoginShell(binaryName: binaryName, environment: environment)
    }

    private static func resolveInPath(binaryName: String, path: String?) -> String? {
        let effectivePath = path ?? "/usr/bin:/bin:/usr/sbin:/sbin:/usr/local/bin"
        for directory in effectivePath.split(separator: ":") {
            let candidate = String(directory) + "/" + binaryName
            if FileManager.default.isExecutableFile(atPath: candidate) {
                return candidate
            }
        }
        return nil
    }

    private static func resolveWithLoginShell(binaryName: String, environment: [String: String]) -> String? {
        let shell = environment["SHELL"] ?? "/bin/zsh"
        let process = Process()
        let outputPipe = Pipe()

        process.executableURL = URL(fileURLWithPath: shell)
        process.arguments = ["-ilc", "command -v -- \(shellEscaped(binaryName))"]
        process.standardOutput = outputPipe
        process.standardError = FileHandle.nullDevice
        process.environment = environment

        do {
            try process.run()
        } catch {
            return nil
        }

        process.waitUntilExit()
        guard process.terminationStatus == 0 else {
            return nil
        }

        let data = outputPipe.fileHandleForReading.readDataToEndOfFile()
        guard let raw = String(data: data, encoding: .utf8)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
            !raw.isEmpty,
            FileManager.default.isExecutableFile(atPath: raw) else {
            return nil
        }
        return raw
    }

    private static func shellEscaped(_ value: String) -> String {
        "'\(value.replacingOccurrences(of: "'", with: "'\\''"))'"
    }
}

enum CodexBarRPCError: Error {
    case executableNotFound
    case launchFailed(String)
    case malformedResponse
    case requestFailed(String)
    case closed
}

struct CodexBarCodexRateLimitSnapshot: Decodable, Equatable {
    struct Window: Decodable, Equatable {
        let usedPercent: Double
        let windowDurationMins: Int?
        let resetsAt: Int?
    }

    struct Credits: Decodable, Equatable {
        let hasCredits: Bool
        let unlimited: Bool
        let balance: String?
    }

    let primary: Window?
    let secondary: Window?
    let credits: Credits?
}

struct CodexBarCodexAccountSnapshot: Decodable, Equatable {
    enum Account: Equatable {
        case chatgpt(email: String?, planType: String?)
        case unknown
    }

    let account: Account?

    private enum CodingKeys: String, CodingKey {
        case account
    }

    private enum AccountCodingKeys: String, CodingKey {
        case kind
        case email
        case planType
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        guard container.contains(.account), !(try container.decodeNil(forKey: .account)) else {
            self.account = nil
            return
        }

        let nested = try container.nestedContainer(keyedBy: AccountCodingKeys.self, forKey: .account)
        let kind = (try? nested.decode(String.self, forKey: .kind)) ?? ""
        switch kind {
        case "chatgpt":
            self.account = .chatgpt(
                email: try? nested.decodeIfPresent(String.self, forKey: .email),
                planType: try? nested.decodeIfPresent(String.self, forKey: .planType)
            )
        default:
            self.account = .unknown
        }
    }
}

private struct CodexBarRateLimitsEnvelope: Decodable {
    let rateLimits: CodexBarCodexRateLimitSnapshot
}

private final class CodexBarRPCLineBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var buffer = Data()

    func appendAndDrainLines(_ data: Data) -> [Data] {
        lock.lock()
        defer { lock.unlock() }

        buffer.append(data)
        var lines: [Data] = []
        while let newline = buffer.firstIndex(of: 0x0A) {
            let line = Data(buffer[..<newline])
            buffer.removeSubrange(...newline)
            if !line.isEmpty {
                lines.append(line)
            }
        }
        return lines
    }
}

actor CodexBarCodexRPCClient {
    private let process = Process()
    private let stdinPipe = Pipe()
    private let stdoutPipe = Pipe()
    private let stderrPipe = Pipe()
    private var nextID = 1
    private let stdoutStream: AsyncStream<Data>
    private let stdoutContinuation: AsyncStream<Data>.Continuation

    init(environment: [String: String] = ProcessInfo.processInfo.environment) throws {
        var continuation: AsyncStream<Data>.Continuation!
        self.stdoutStream = AsyncStream<Data> { streamContinuation in
            continuation = streamContinuation
        }
        self.stdoutContinuation = continuation

        guard let executable = CodexBarBinaryLocator.resolve(binaryName: "codex", environment: environment) else {
            throw CodexBarRPCError.executableNotFound
        }

        process.executableURL = URL(fileURLWithPath: "/usr/bin/env")
        process.arguments = [executable, "-s", "read-only", "-a", "untrusted", "app-server"]
        process.environment = environment
        process.standardInput = stdinPipe
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe

        do {
            try process.run()
        } catch {
            throw CodexBarRPCError.launchFailed(error.localizedDescription)
        }

        let lineBuffer = CodexBarRPCLineBuffer()
        stdoutPipe.fileHandleForReading.readabilityHandler = { [stdoutContinuation] handle in
            let data = handle.availableData
            if data.isEmpty {
                handle.readabilityHandler = nil
                stdoutContinuation.finish()
                return
            }

            for line in lineBuffer.appendAndDrainLines(data) {
                stdoutContinuation.yield(line)
            }
        }

        stderrPipe.fileHandleForReading.readabilityHandler = { handle in
            let data = handle.availableData
            if data.isEmpty {
                handle.readabilityHandler = nil
            }
        }
    }

    deinit {
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        if process.isRunning {
            process.terminate()
        }
    }

    func initialize(clientName: String = "dmux", clientVersion: String = "0.1.0") async throws {
        _ = try await request(method: "initialize", params: [
            "clientInfo": [
                "name": clientName,
                "version": clientVersion,
            ],
        ])
        try sendNotification(method: "initialized")
    }

    func fetchAccount() async throws -> CodexBarCodexAccountSnapshot {
        let message = try await request(method: "account/read", params: [:])
        return try decodeResult(from: message)
    }

    func fetchRateLimits() async throws -> CodexBarCodexRateLimitSnapshot {
        let message = try await request(method: "account/rateLimits/read", params: [:])
        guard let result = message["result"] else {
            throw CodexBarRPCError.malformedResponse
        }
        let data = try JSONSerialization.data(withJSONObject: result)
        return try JSONDecoder().decode(CodexBarRateLimitsEnvelope.self, from: data).rateLimits
    }

    func shutdown() async {
        stdoutPipe.fileHandleForReading.readabilityHandler = nil
        stderrPipe.fileHandleForReading.readabilityHandler = nil
        if process.isRunning {
            process.terminate()
        }
    }

    private func request(method: String, params: [String: Any]) async throws -> [String: Any] {
        let id = nextID
        nextID += 1

        let payload: [String: Any] = [
            "id": id,
            "method": method,
            "params": params,
        ]
        let data = try JSONSerialization.data(withJSONObject: payload)
        stdinPipe.fileHandleForWriting.write(data)
        stdinPipe.fileHandleForWriting.write(Data([0x0A]))

        while true {
            let message = try await readNextMessage()
            if message["id"] == nil {
                continue
            }
            guard let messageID = jsonID(message["id"]), messageID == id else {
                continue
            }
            if let error = message["error"] as? [String: Any] {
                let text = (error["message"] as? String) ?? "unknown rpc error"
                throw CodexBarRPCError.requestFailed(text)
            }
            return message
        }
    }

    private func sendNotification(method: String) throws {
        let payload: [String: Any] = [
            "method": method,
            "params": [:],
        ]
        let data = try JSONSerialization.data(withJSONObject: payload)
        stdinPipe.fileHandleForWriting.write(data)
        stdinPipe.fileHandleForWriting.write(Data([0x0A]))
    }

    private func readNextMessage() async throws -> [String: Any] {
        for await line in stdoutStream {
            if line.isEmpty {
                continue
            }
            if let object = try? JSONSerialization.jsonObject(with: line) as? [String: Any] {
                return object
            }
        }
        throw CodexBarRPCError.closed
    }

    private func decodeResult<T: Decodable>(from message: [String: Any]) throws -> T {
        guard let result = message["result"] else {
            throw CodexBarRPCError.malformedResponse
        }
        let data = try JSONSerialization.data(withJSONObject: result)
        return try JSONDecoder().decode(T.self, from: data)
    }

    private func jsonID(_ value: Any?) -> Int? {
        switch value {
        case let int as Int:
            return int
        case let number as NSNumber:
            return number.intValue
        default:
            return nil
        }
    }
}

struct CodexBarCodexRuntimeState: Equatable {
    var accountEmail: String?
    var accountPlan: String?
    var rateLimits: CodexBarCodexRateLimitSnapshot?
    var updatedAt: Date
}

actor CodexBarCodexRuntimeTransport {
    static let shared = CodexBarCodexRuntimeTransport()

    private var client: CodexBarCodexRPCClient?
    private var cachedState: CodexBarCodexRuntimeState?
    private var lastRefreshAt: Date?
    private let minimumRefreshInterval: TimeInterval = 15

    func latestState(forceRefresh: Bool = false, environment: [String: String] = ProcessInfo.processInfo.environment) async -> CodexBarCodexRuntimeState? {
        if !forceRefresh,
           let cachedState,
           let lastRefreshAt,
           Date().timeIntervalSince(lastRefreshAt) < minimumRefreshInterval {
            return cachedState
        }

        do {
            let client = try await rpcClient(environment: environment)
            async let accountTask = try? client.fetchAccount()
            async let limitsTask = try? client.fetchRateLimits()
            let account = await accountTask
            let limits = await limitsTask

            let nextState = CodexBarCodexRuntimeState(
                accountEmail: account?.account.flatMap {
                    if case let .chatgpt(email, _) = $0 { return email }
                    return nil
                },
                accountPlan: account?.account.flatMap {
                    if case let .chatgpt(_, planType) = $0 { return planType }
                    return nil
                },
                rateLimits: limits,
                updatedAt: Date()
            )
            cachedState = nextState
            lastRefreshAt = Date()
            return nextState
        } catch {
            await reset()
            return cachedState
        }
    }

    func reset() async {
        if let client {
            await client.shutdown()
        }
        client = nil
        lastRefreshAt = nil
    }

    private func rpcClient(environment: [String: String]) async throws -> CodexBarCodexRPCClient {
        if let client {
            return client
        }
        let newClient = try CodexBarCodexRPCClient(environment: environment)
        try await newClient.initialize(clientName: "dmux", clientVersion: "0.1.0")
        client = newClient
        return newClient
    }
}
