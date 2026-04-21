import Foundation
import UserNotifications

enum ProjectActivityPhase: Equatable {
    case idle
    case running(tool: String)
    case waitingInput(tool: String)
    case completed(tool: String, finishedAt: Date, exitCode: Int?)
}

struct ProjectActivityPayload: Codable, Equatable {
    var tool: String
    var phase: String
    var updatedAt: Double
    var finishedAt: Double?
    var exitCode: Int?
}

struct ProjectActivityService: @unchecked Sendable {
    private let runningPhaseLifetime: TimeInterval = 15
    private let completedPhaseLifetime: TimeInterval = 20
    private let fileManager = FileManager.default
    private let runtimeBridgeService = AIRuntimeBridgeService()
    private let externalNotificationService = AppExternalNotificationService.shared

    private var supportsSystemNotifications: Bool {
        Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil
    }

    func statusDirectoryURL() -> URL {
        runtimeBridgeService.statusDirectoryURL()
    }

    func loadStatuses(projects: [Project]) -> [UUID: ProjectActivityPayload] {
        var result: [UUID: ProjectActivityPayload] = [:]
        for project in projects {
            guard let payload = loadStatus(projectID: project.id) else {
                continue
            }
            result[project.id] = payload
        }
        return result
    }

    func loadStatus(projectID: UUID) -> ProjectActivityPayload? {
        let fileURL = statusFileURL(for: projectID)
        guard let data = try? Data(contentsOf: fileURL),
              let payload = try? JSONDecoder().decode(ProjectActivityPayload.self, from: data) else {
            return nil
        }
        return payload
    }

    func phase(for payload: ProjectActivityPayload?) -> ProjectActivityPhase {
        guard let payload else { return .idle }
        switch payload.phase {
        case "running":
            let age = Date().timeIntervalSince1970 - payload.updatedAt
            if age > runningPhaseLifetime {
                return .idle
            }
            return .running(tool: payload.tool)
        case "completed":
            let age = Date().timeIntervalSince1970 - payload.updatedAt
            if age > completedPhaseLifetime {
                return .idle
            }
            return .completed(
                tool: payload.tool,
                finishedAt: Date(timeIntervalSince1970: payload.finishedAt ?? payload.updatedAt),
                exitCode: payload.exitCode
            )
        default:
            return .idle
        }
    }

    func completionToken(for payload: ProjectActivityPayload) -> String {
        "\(payload.tool)-\(payload.updatedAt)-\(payload.exitCode ?? -999)"
    }

    func requestNotificationPermission() {
        guard supportsSystemNotifications else {
            return
        }
        UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
            if let error {
                AppDebugLog.shared.log("notifications", "authorization error=\(error.localizedDescription)")
                return
            }
            AppDebugLog.shared.log("notifications", "authorization granted=\(granted)")
        }
    }

    func notifyCompletion(projectName: String, tool: String, exitCode: Int?, settings: AppNotificationSettings) {
        Task.detached(priority: .utility) {
            await externalNotificationService.sendCompletion(
                settings: settings,
                projectName: projectName,
                tool: tool,
                exitCode: exitCode
            )
        }

        guard supportsSystemNotifications else {
            return
        }

        withAuthorizedSystemNotifications(
            statusLogMessage: "enqueue completion project=\(projectName) tool=\(tool)",
            requestLogPrefix: "authorization-on-send"
        ) {
            enqueueNotification(projectName: projectName, tool: tool, exitCode: exitCode)
        }
    }

    func notifyNeedsInput(
        projectName: String,
        tool: String,
        notificationType: String?,
        targetToolName: String?,
        message: String?
    ) {
        guard supportsSystemNotifications else {
            return
        }

        withAuthorizedSystemNotifications(
            statusLogMessage: "enqueue waiting-input project=\(projectName) tool=\(tool)",
            requestLogPrefix: "waiting-input authorization"
        ) {
            enqueueNeedsInputNotification(
                projectName: projectName,
                tool: tool,
                notificationType: notificationType,
                targetToolName: targetToolName,
                message: message
            )
        }
    }

    private func withAuthorizedSystemNotifications(
        statusLogMessage: String,
        requestLogPrefix: String,
        onAuthorized: @escaping @Sendable () -> Void
    ) {
        UNUserNotificationCenter.current().getNotificationSettings { settings in
            AppDebugLog.shared.log(
                "notifications",
                "\(statusLogMessage) status=\(settings.authorizationStatus.rawValue)"
            )

            switch settings.authorizationStatus {
            case .authorized, .provisional, .ephemeral:
                onAuthorized()
            case .notDetermined:
                UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
                    if let error {
                        AppDebugLog.shared.log("notifications", "\(requestLogPrefix) error=\(error.localizedDescription)")
                        return
                    }
                    AppDebugLog.shared.log("notifications", "\(requestLogPrefix) granted=\(granted)")
                    guard granted else {
                        AppDebugLog.shared.log("notifications", "\(requestLogPrefix) denied")
                        return
                    }
                    onAuthorized()
                }
            case .denied:
                AppDebugLog.shared.log("notifications", "\(statusLogMessage) skipped status=denied")
            @unknown default:
                AppDebugLog.shared.log("notifications", "\(statusLogMessage) skipped status=unknown")
            }
        }
    }

    private func enqueueNotification(projectName: String, tool: String, exitCode: Int?) {
        let content = UNMutableNotificationContent()
        content.title = String(format: String(localized: "project.activity.completed_format", defaultValue: "%@ completed", bundle: .module), tool)
        content.body = exitCode == nil || exitCode == 0
            ? String(format: String(localized: "project.activity.finished_successfully_format", defaultValue: "%@ finished successfully", bundle: .module), projectName)
            : String(format: String(localized: "project.activity.finished_with_exit_code_format", defaultValue: "%@ finished with exit code %@", bundle: .module), projectName, "\(exitCode ?? -1)")
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "project-activity-\(projectName)-\(tool)-\(Date().timeIntervalSince1970)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                AppDebugLog.shared.log("notifications", "enqueue failed error=\(error.localizedDescription)")
            } else {
                AppDebugLog.shared.log("notifications", "enqueue success project=\(projectName) tool=\(tool)")
            }
        }
    }

    private func enqueueNeedsInputNotification(
        projectName: String,
        tool: String,
        notificationType: String?,
        targetToolName: String?,
        message: String?
    ) {
        let content = UNMutableNotificationContent()
        content.title = String(
            localized: "project.activity.input_required",
            defaultValue: "Action required",
            bundle: .module
        )

        let body: String
        if let message, !message.isEmpty {
            body = message
        } else if let targetToolName, !targetToolName.isEmpty {
            body = String(
                format: String(
                    localized: "project.activity.permission_request_format",
                    defaultValue: "%@ needs confirmation for %@ in %@",
                    bundle: .module
                ),
                tool,
                targetToolName,
                projectName
            )
        } else if let notificationType, !notificationType.isEmpty {
            body = String(
                format: String(
                    localized: "project.activity.notification_request_format",
                    defaultValue: "%@ is waiting for %@ in %@",
                    bundle: .module
                ),
                tool,
                notificationType,
                projectName
            )
        } else {
            body = String(
                format: String(
                    localized: "project.activity.generic_input_request_format",
                    defaultValue: "%@ is waiting for your input in %@",
                    bundle: .module
                ),
                tool,
                projectName
            )
        }

        content.body = body
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "project-activity-waiting-\(projectName)-\(tool)-\(Date().timeIntervalSince1970)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                AppDebugLog.shared.log("notifications", "waiting-input enqueue failed error=\(error.localizedDescription)")
            } else {
                AppDebugLog.shared.log("notifications", "waiting-input enqueue success project=\(projectName) tool=\(tool)")
            }
        }
    }

    func clearStatus(for projectID: UUID) {
        try? fileManager.removeItem(at: statusFileURL(for: projectID))
    }

    func clearAllStatuses() {
        let directory = statusDirectoryURL()
        guard let fileURLs = try? fileManager.contentsOfDirectory(at: directory, includingPropertiesForKeys: nil) else {
            return
        }
        for fileURL in fileURLs where fileURL.pathExtension == "json" {
            try? fileManager.removeItem(at: fileURL)
        }
    }

    private func statusFileURL(for projectID: UUID) -> URL {
        statusDirectoryURL().appendingPathComponent("\(projectID.uuidString).json")
    }
}

struct AppExternalNotificationEvent: Sendable {
    let projectName: String
    let tool: String
    let exitCode: Int?

    var title: String {
        String(format: String(localized: "project.activity.completed_format", defaultValue: "%@ completed", bundle: .module), tool)
    }

    var body: String {
        if let exitCode, exitCode != 0 {
            return String(
                format: String(localized: "project.activity.finished_with_exit_code_format", defaultValue: "%@ finished with exit code %@", bundle: .module),
                projectName,
                "\(exitCode)"
            )
        }

        return String(
            format: String(localized: "project.activity.finished_successfully_format", defaultValue: "%@ finished successfully", bundle: .module),
            projectName
        )
    }

    var plainText: String {
        "\(title)\n\(body)"
    }

    var markdownText: String {
        "**\(title)**\n\(body)"
    }
}

private enum AppExternalNotificationDriverError: LocalizedError {
    case invalidConfiguration(String)
    case invalidPayload(String)

    var errorDescription: String? {
        switch self {
        case .invalidConfiguration(let message):
            return message
        case .invalidPayload(let message):
            return message
        }
    }
}

private protocol AppExternalNotificationDriver: Sendable {
    var channel: AppNotificationChannel { get }
    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest
}

private let appExternalNotificationRequestTimeout: TimeInterval = 8
private let appExternalNotificationResourceTimeout: TimeInterval = 15

actor AppExternalNotificationService {
    static let shared = AppExternalNotificationService()

    private let logger = AppDebugLog.shared
    private let session: URLSession = {
        let configuration = URLSessionConfiguration.ephemeral
        configuration.timeoutIntervalForRequest = appExternalNotificationRequestTimeout
        configuration.timeoutIntervalForResource = appExternalNotificationResourceTimeout
        configuration.waitsForConnectivity = false
        configuration.requestCachePolicy = .reloadIgnoringLocalCacheData
        configuration.urlCache = nil
        configuration.httpCookieStorage = nil
        return URLSession(configuration: configuration)
    }()

    func sendCompletion(settings: AppNotificationSettings, projectName: String, tool: String, exitCode: Int?) async {
        let event = AppExternalNotificationEvent(projectName: projectName, tool: tool, exitCode: exitCode)
        let enabledChannels = AppNotificationChannel.allCases.filter {
            $0.configuration(from: settings).isEnabled
        }

        guard !enabledChannels.isEmpty else {
            return
        }

        let channelList = enabledChannels.map(\.title).joined(separator: ",")
        logger.log(
            "notifications",
            "external dispatch project=\(projectName) tool=\(tool) channels=\(channelList)"
        )

        await withTaskGroup(of: Void.self) { group in
            for channel in enabledChannels {
                let configuration = channel.configuration(from: settings)
                group.addTask { [session, logger] in
                    let startedAt = Date()
                    do {
                        let request = try AppNotificationDriverFactory.driver(for: channel).makeRequest(
                            configuration: configuration,
                            event: event
                        )
                        let debugTarget = sanitizedNotificationTarget(for: request.url)
                        let requestTimeout = request.timeoutInterval > 0 ? request.timeoutInterval : appExternalNotificationRequestTimeout
                        logger.log(
                            "notifications",
                            "external request channel=\(channel.title) method=\(request.httpMethod ?? "GET") target=\(debugTarget) timeout=\(Int(requestTimeout))s bodyBytes=\(request.httpBody?.count ?? 0)"
                        )

                        let (data, response) = try await session.data(for: request)
                        let elapsedMs = Int(Date().timeIntervalSince(startedAt) * 1000)
                        guard let httpResponse = response as? HTTPURLResponse else {
                            logger.log(
                                "notifications",
                                "external failed channel=\(channel.title) target=\(debugTarget) elapsedMs=\(elapsedMs) reason=invalid-response"
                            )
                            return
                        }

                        guard (200 ..< 300).contains(httpResponse.statusCode) else {
                            logger.log(
                                "notifications",
                                "external failed channel=\(channel.title) target=\(debugTarget) elapsedMs=\(elapsedMs) status=\(httpResponse.statusCode) response=\(notificationResponsePreview(data))"
                            )
                            return
                        }

                        logger.log(
                            "notifications",
                            "external success channel=\(channel.title) target=\(debugTarget) elapsedMs=\(elapsedMs) status=\(httpResponse.statusCode) responseBytes=\(data.count)"
                        )
                    } catch {
                        let elapsedMs = Int(Date().timeIntervalSince(startedAt) * 1000)
                        logger.log(
                            "notifications",
                            "external failed channel=\(channel.title) elapsedMs=\(elapsedMs) error=\(notificationErrorSummary(error))"
                        )
                    }
                }
            }
        }
    }
}

private enum AppNotificationDriverFactory {
    static func driver(for channel: AppNotificationChannel) -> any AppExternalNotificationDriver {
        switch channel {
        case .bark:
            return BarkNotificationDriver()
        case .ntfy:
            return NtfyNotificationDriver()
        case .wxpusher:
            return WxPusherNotificationDriver()
        case .feishu:
            return FeishuNotificationDriver()
        case .dingTalk:
            return DingTalkNotificationDriver()
        case .weCom:
            return WeComNotificationDriver()
        case .telegram:
            return TelegramNotificationDriver()
        case .discord:
            return DiscordNotificationDriver()
        case .slack:
            return SlackNotificationDriver()
        case .webhook:
            return WebhookNotificationDriver()
        }
    }
}

private struct BarkNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .bark

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let deviceKey = configuration.token.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !deviceKey.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing device_key")
        }

        let endpoint = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        let baseURL = URL(string: endpoint.isEmpty ? "https://api.day.app" : endpoint)
        guard let baseURL else {
            throw AppExternalNotificationDriverError.invalidConfiguration("invalid bark endpoint")
        }

        let requestURL: URL
        if baseURL.path.hasSuffix("/push") {
            requestURL = baseURL
        } else {
            requestURL = baseURL.appendingPathComponent("push", isDirectory: false)
        }

        return try makeJSONRequest(
            url: requestURL,
            body: [
                "device_key": deviceKey,
                "title": event.title,
                "body": event.body,
                "group": "Codux",
            ]
        )
    }
}

private struct NtfyNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .ntfy

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let endpoint = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: endpoint), !endpoint.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing ntfy topic url")
        }

        var request = URLRequest(url: url)
        request.httpMethod = "POST"
        request.httpBody = Data(event.body.utf8)
        request.setValue("text/plain; charset=utf-8", forHTTPHeaderField: "Content-Type")
        request.setValue(event.title, forHTTPHeaderField: "Title")
        let token = configuration.token.trimmingCharacters(in: .whitespacesAndNewlines)
        if !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        return preparedNotificationRequest(request)
    }
}

private struct WxPusherNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .wxpusher

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let target = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !target.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing wxpusher target")
        }

        guard target.uppercased().hasPrefix("SPT_") else {
            throw AppExternalNotificationDriverError.invalidConfiguration("wxpusher only supports SPT mode")
        }

        let encodedMessage = event.plainText.addingPercentEncoding(withAllowedCharacters: .urlPathAllowed) ?? event.plainText
        guard let url = URL(string: "https://wxpusher.zjiecode.com/api/send/message/\(target)/\(encodedMessage)") else {
            throw AppExternalNotificationDriverError.invalidConfiguration("invalid wxpusher spt target")
        }
        var request = URLRequest(url: url)
        request.httpMethod = "GET"
        return preparedNotificationRequest(request)
    }
}

private struct FeishuNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .feishu

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let url = try resolvedWebhookURL(
            configuration: configuration,
            defaultPrefix: "https://open.feishu.cn/open-apis/bot/v2/hook/"
        )
        return try makeJSONRequest(
            url: url,
            body: [
                "msg_type": "text",
                "content": [
                    "text": event.plainText,
                ],
            ]
        )
    }
}

private struct DingTalkNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .dingTalk

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let url = try resolvedWebhookURL(
            configuration: configuration,
            defaultPrefix: "https://oapi.dingtalk.com/robot/send?access_token="
        )
        return try makeJSONRequest(
            url: url,
            body: [
                "msgtype": "text",
                "text": [
                    "content": event.plainText,
                ],
            ]
        )
    }
}

private struct WeComNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .weCom

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let url = try resolvedWebhookURL(
            configuration: configuration,
            defaultPrefix: "https://qyapi.weixin.qq.com/cgi-bin/webhook/send?key="
        )
        return try makeJSONRequest(
            url: url,
            body: [
                "msgtype": "markdown",
                "markdown": [
                    "content": event.markdownText,
                ],
            ]
        )
    }
}

private struct TelegramNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .telegram

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let botToken = configuration.token.trimmingCharacters(in: .whitespacesAndNewlines)
        let chatID = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !botToken.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing telegram bot_token")
        }
        guard !chatID.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing telegram chat_id")
        }

        guard let url = URL(string: "https://api.telegram.org/bot\(botToken)/sendMessage") else {
            throw AppExternalNotificationDriverError.invalidConfiguration("invalid telegram url")
        }

        return try makeJSONRequest(
            url: url,
            body: [
                "chat_id": chatID,
                "text": event.plainText,
            ]
        )
    }
}

private struct DiscordNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .discord

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let endpoint = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: endpoint), !endpoint.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing discord webhook url")
        }
        return try makeJSONRequest(
            url: url,
            body: [
                "content": event.markdownText,
                "username": "Codux",
            ]
        )
    }
}

private struct SlackNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .slack

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let endpoint = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: endpoint), !endpoint.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing slack webhook url")
        }
        return try makeJSONRequest(
            url: url,
            body: [
                "text": "*\(event.title)*\n\(event.body)",
                "username": "Codux",
            ]
        )
    }
}

private struct WebhookNotificationDriver: AppExternalNotificationDriver {
    let channel: AppNotificationChannel = .webhook

    func makeRequest(configuration: AppNotificationChannelConfiguration, event: AppExternalNotificationEvent) throws -> URLRequest {
        let endpoint = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
        guard let url = URL(string: endpoint), !endpoint.isEmpty else {
            throw AppExternalNotificationDriverError.invalidConfiguration("missing webhook url")
        }
        let token = configuration.token.trimmingCharacters(in: .whitespacesAndNewlines)
        var request = try makeJSONRequest(
            url: url,
            body: [
                "title": event.title,
                "body": event.body,
                "projectName": event.projectName,
                "tool": event.tool,
                "exitCode": event.exitCode.map { $0 as Any } ?? NSNull(),
            ]
        )
        if !token.isEmpty {
            request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        }
        return request
    }
}

private func resolvedWebhookURL(
    configuration: AppNotificationChannelConfiguration,
    defaultPrefix: String
) throws -> URL {
    let endpoint = configuration.endpoint.trimmingCharacters(in: .whitespacesAndNewlines)
    if !endpoint.isEmpty, let url = URL(string: endpoint) {
        return url
    }

    let token = configuration.token.trimmingCharacters(in: .whitespacesAndNewlines)
    guard !token.isEmpty, let url = URL(string: defaultPrefix + token) else {
        throw AppExternalNotificationDriverError.invalidConfiguration("missing webhook address")
    }
    return url
}

private func makeJSONRequest(url: URL, body: [String: Any]) throws -> URLRequest {
    guard JSONSerialization.isValidJSONObject(body) else {
        throw AppExternalNotificationDriverError.invalidPayload("invalid json body")
    }

    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.httpBody = try JSONSerialization.data(withJSONObject: body)
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.setValue("application/json", forHTTPHeaderField: "Accept")
    return preparedNotificationRequest(request)
}

private func preparedNotificationRequest(_ request: URLRequest) -> URLRequest {
    var request = request
    request.timeoutInterval = appExternalNotificationRequestTimeout
    request.cachePolicy = .reloadIgnoringLocalCacheData
    request.httpShouldHandleCookies = false
    return request
}

private func sanitizedNotificationTarget(for url: URL?) -> String {
    guard let url else {
        return "unknown"
    }

    let scheme = url.scheme ?? "https"
    let host = url.host ?? "unknown-host"
    let path = url.path.isEmpty ? "/" : url.path

    if host.contains("api.telegram.org") {
        return "\(scheme)://\(host)/bot***/sendMessage"
    }
    if host.contains("wxpusher.zjiecode.com") {
        return "\(scheme)://\(host)/api/send/message/***"
    }
    if path.contains("/hook/") {
        return "\(scheme)://\(host)\(path.replacingOccurrences(of: #"/hook/[^/]+"#, with: "/hook/***", options: .regularExpression))"
    }
    if let query = url.query, !query.isEmpty {
        return "\(scheme)://\(host)\(path)?***"
    }
    return "\(scheme)://\(host)\(path)"
}

private func notificationResponsePreview(_ data: Data) -> String {
    guard !data.isEmpty else {
        return "empty"
    }
    let raw = String(decoding: data.prefix(240), as: UTF8.self)
        .replacingOccurrences(of: "\n", with: " ")
        .replacingOccurrences(of: "\r", with: " ")
        .trimmingCharacters(in: .whitespacesAndNewlines)
    return raw.isEmpty ? "non-text-\(data.count)b" : raw
}

private func notificationErrorSummary(_ error: Error) -> String {
    if let urlError = error as? URLError {
        return "url-\(urlError.code.rawValue) \(urlError.localizedDescription)"
    }
    return error.localizedDescription
}
