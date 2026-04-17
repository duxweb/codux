import Foundation

actor OpenCodeGlobalEventService {
    static let shared = OpenCodeGlobalEventService()

    private let logger = AppDebugLog.shared
    private let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = 4
        configuration.timeoutIntervalForResource = TimeInterval.greatestFiniteMagnitude
        return URLSession(configuration: configuration)
    }()

    private var streamTask: Task<Void, Never>?
    private var lastConnectAttemptAt: Date?
    private var statusByDirectory: [String: [String: AIResponseState]] = [:]

    func cachedStatuses(directory: String) async -> [String: AIResponseState]? {
        await ensureStreaming()
        let statuses = statusByDirectory[directory] ?? [:]
        return statuses.isEmpty ? nil : statuses
    }

    func sessionStatuses(directory: String) async -> [String: AIResponseState]? {
        if let cached = await cachedStatuses(directory: directory) {
            return cached
        }
        return await fetchStatuses(directory: directory)
    }

    private func ensureStreaming() async {
        if streamTask != nil {
            return
        }
        if let lastConnectAttemptAt,
           Date().timeIntervalSince(lastConnectAttemptAt) < 2 {
            return
        }

        lastConnectAttemptAt = Date()
        streamTask = Task(priority: .utility) { [weak self] in
            await self?.consumeStream()
        }
    }

    private func consumeStream() async {
        defer {
            streamTask = nil
        }

        guard let url = URL(string: "http://127.0.0.1:4096/global/event") else {
            return
        }

        var request = URLRequest(url: url)
        request.timeoutInterval = 4
        request.setValue("text/event-stream", forHTTPHeaderField: "Accept")

        do {
            let (bytes, response) = try await session.bytes(for: request)
            guard let httpResponse = response as? HTTPURLResponse,
                  (200 ..< 300).contains(httpResponse.statusCode) else {
                logger.log("opencode-global", "stream rejected")
                return
            }

            logger.log("opencode-global", "stream connected")

            var eventLines: [String] = []
            for try await line in bytes.lines {
                if Task.isCancelled {
                    logger.log("opencode-global", "stream cancelled")
                    return
                }

                if line.isEmpty {
                    handleSSEEvent(lines: eventLines)
                    eventLines.removeAll(keepingCapacity: true)
                    continue
                }

                eventLines.append(line)
            }
        } catch {
            logger.log("opencode-global", "stream failed error=\(error.localizedDescription)")
            return
        }
    }

    private func handleSSEEvent(lines: [String]) {
        guard !lines.isEmpty else {
            return
        }

        let payloadText = lines
            .filter { $0.hasPrefix("data:") }
            .map { String($0.dropFirst(5)).trimmingCharacters(in: .whitespaces) }
            .joined(separator: "\n")

        guard !payloadText.isEmpty,
              let data = payloadText.data(using: .utf8),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any],
              let directory = object["directory"] as? String,
              let payload = object["payload"] as? [String: Any],
              let type = payload["type"] as? String else {
            return
        }

        switch type {
        case "session.status":
            guard let properties = payload["properties"] as? [String: Any],
                  let sessionID = properties["sessionID"] as? String,
                  let status = properties["status"] as? [String: Any],
                  let statusType = status["type"] as? String,
                  let responseState = mapOpenCodeStatus(statusType) else {
                return
            }
            statusByDirectory[directory, default: [:]][sessionID] = responseState
        case "session.idle":
            guard let properties = payload["properties"] as? [String: Any],
                  let sessionID = properties["sessionID"] as? String else {
                return
            }
            statusByDirectory[directory, default: [:]][sessionID] = .idle
        case "session.deleted":
            guard let properties = payload["properties"] as? [String: Any],
                  let sessionID = properties["sessionID"] as? String else {
                return
            }
            statusByDirectory[directory]?[sessionID] = nil
        default:
            return
        }
    }

    private func fetchStatuses(directory: String) async -> [String: AIResponseState]? {
        var components = URLComponents(string: "http://127.0.0.1:4096/session/status")
        components?.queryItems = [
            URLQueryItem(name: "directory", value: directory),
        ]
        guard let url = components?.url else {
            return nil
        }

        var request = URLRequest(url: url)
        request.timeoutInterval = 0.18

        guard let (data, response) = try? await session.data(for: request),
              let httpResponse = response as? HTTPURLResponse,
              (200 ..< 300).contains(httpResponse.statusCode),
              let object = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else {
            return nil
        }

        var result: [String: AIResponseState] = [:]
        for (sessionID, rawStatus) in object {
            guard let rawStatus = rawStatus as? [String: Any],
                  let type = rawStatus["type"] as? String,
                  let responseState = mapOpenCodeStatus(type) else {
                continue
            }
            result[sessionID] = responseState
        }

        if !result.isEmpty {
            statusByDirectory[directory] = result
        }
        return result.isEmpty ? nil : result
    }

    private func mapOpenCodeStatus(_ statusType: String) -> AIResponseState? {
        switch statusType {
        case "busy", "retry":
            return .responding
        case "idle":
            return .idle
        default:
            return nil
        }
    }
}
