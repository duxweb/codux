import Foundation

actor OpenCodeGlobalEventService {
    static let shared = OpenCodeGlobalEventService()

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
                return
            }

            var eventLines: [String] = []
            for try await line in bytes.lines {
                if Task.isCancelled {
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
