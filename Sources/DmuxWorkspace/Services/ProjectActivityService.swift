import Foundation
import UserNotifications

enum ProjectActivityPhase: Equatable {
    case idle
    case running(tool: String)
    case completed(tool: String, finishedAt: Date, exitCode: Int?)
}

struct ProjectActivityPayload: Codable, Equatable {
    var projectId: String
    var projectName: String
    var tool: String
    var phase: String
    var updatedAt: Double
    var startedAt: Double?
    var finishedAt: Double?
    var exitCode: Int?
}

struct ProjectActivityService: @unchecked Sendable {
    private let fileManager = FileManager.default
    private let runtimeBridgeService = AIRuntimeBridgeService()

    private var supportsSystemNotifications: Bool {
        Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil
    }

    func statusDirectoryURL() -> URL {
        runtimeBridgeService.statusDirectoryURL()
    }

    func loadStatuses(projects: [Project]) -> [UUID: ProjectActivityPayload] {
        let directory = statusDirectoryURL()
        var result: [UUID: ProjectActivityPayload] = [:]
        for project in projects {
            let fileURL = directory.appendingPathComponent("\(project.id.uuidString).json")
            guard let data = try? Data(contentsOf: fileURL),
                  let payload = try? JSONDecoder().decode(ProjectActivityPayload.self, from: data) else {
                continue
            }
            result[project.id] = payload
        }
        return result
    }

    func phase(for payload: ProjectActivityPayload?) -> ProjectActivityPhase {
        guard let payload else { return .idle }
        switch payload.phase {
        case "running":
            let age = Date().timeIntervalSince1970 - payload.updatedAt
            if age > 15 {
                return .idle
            }
            return .running(tool: payload.tool)
        case "completed":
            let age = Date().timeIntervalSince1970 - payload.updatedAt
            if age > 20 {
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

    func notifyCompletion(projectName: String, tool: String, exitCode: Int?) {
        guard supportsSystemNotifications else {
            return
        }

        UNUserNotificationCenter.current().getNotificationSettings { settings in
            AppDebugLog.shared.log(
                "notifications",
                "enqueue completion project=\(projectName) tool=\(tool) status=\(settings.authorizationStatus.rawValue)"
            )

            switch settings.authorizationStatus {
            case .authorized, .provisional, .ephemeral:
                enqueueNotification(projectName: projectName, tool: tool, exitCode: exitCode)
            case .notDetermined:
                UNUserNotificationCenter.current().requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
                    if let error {
                        AppDebugLog.shared.log("notifications", "authorization-on-send error=\(error.localizedDescription)")
                        return
                    }
                    AppDebugLog.shared.log("notifications", "authorization-on-send granted=\(granted)")
                    guard granted else {
                        AppDebugLog.shared.log("notifications", "authorization-on-send denied")
                        return
                    }
                    enqueueNotification(projectName: projectName, tool: tool, exitCode: exitCode)
                }
            case .denied:
                AppDebugLog.shared.log("notifications", "enqueue skipped status=denied")
            @unknown default:
                AppDebugLog.shared.log("notifications", "enqueue skipped status=unknown")
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
    func writeTestStatus(project: Project, tool: String, phase: String, exitCode: Int? = nil) {
        let fileURL = statusDirectoryURL().appendingPathComponent("\(project.id.uuidString).json")
        let now = Date().timeIntervalSince1970

        var payload = ProjectActivityPayload(
            projectId: project.id.uuidString,
            projectName: project.name,
            tool: tool,
            phase: phase,
            updatedAt: now,
            startedAt: nil,
            finishedAt: nil,
            exitCode: exitCode
        )

        if phase == "running" {
            payload.startedAt = now
        }
        if phase == "completed" {
            payload.finishedAt = now
        }

        guard let data = try? JSONEncoder().encode(payload) else {
            return
        }
        try? data.write(to: fileURL, options: .atomic)
    }

    func clearStatus(for projectID: UUID) {
        let fileURL = statusDirectoryURL().appendingPathComponent("\(projectID.uuidString).json")
        try? fileManager.removeItem(at: fileURL)
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
}
