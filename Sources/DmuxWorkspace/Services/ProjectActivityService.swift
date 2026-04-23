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
