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

    private var supportsSystemNotifications: Bool {
        Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil
    }

    func statusDirectoryURL() -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let url = appSupport.appendingPathComponent("dmux/agent-status", isDirectory: true)
        try? fileManager.createDirectory(at: url, withIntermediateDirectories: true)
        return url
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
                        fallbackNotifyIfNeeded(projectName: projectName, tool: tool, exitCode: exitCode, reason: "authorization-error")
                        return
                    }
                    AppDebugLog.shared.log("notifications", "authorization-on-send granted=\(granted)")
                    guard granted else {
                        fallbackNotifyIfNeeded(projectName: projectName, tool: tool, exitCode: exitCode, reason: "authorization-denied")
                        return
                    }
                    enqueueNotification(projectName: projectName, tool: tool, exitCode: exitCode)
                }
            case .denied:
                AppDebugLog.shared.log("notifications", "enqueue skipped status=denied")
                fallbackNotifyIfNeeded(projectName: projectName, tool: tool, exitCode: exitCode, reason: "status-denied")
            @unknown default:
                AppDebugLog.shared.log("notifications", "enqueue skipped status=unknown")
                fallbackNotifyIfNeeded(projectName: projectName, tool: tool, exitCode: exitCode, reason: "status-unknown")
            }
        }
    }

    private func enqueueNotification(projectName: String, tool: String, exitCode: Int?) {
        let content = UNMutableNotificationContent()
        content.title = String(format: appI18n("project.activity.completed_format", fallback: "%@ completed"), tool)
        content.body = exitCode == nil || exitCode == 0
            ? String(format: appI18n("project.activity.finished_successfully_format", fallback: "%@ finished successfully"), projectName)
            : String(format: appI18n("project.activity.finished_with_exit_code_format", fallback: "%@ finished with exit code %@"), projectName, "\(exitCode ?? -1)")
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "project-activity-\(projectName)-\(tool)-\(Date().timeIntervalSince1970)",
            content: content,
            trigger: nil
        )
        UNUserNotificationCenter.current().add(request) { error in
            if let error {
                AppDebugLog.shared.log("notifications", "enqueue failed error=\(error.localizedDescription)")
                fallbackNotifyIfNeeded(projectName: projectName, tool: tool, exitCode: exitCode, reason: "enqueue-failed")
            } else {
                AppDebugLog.shared.log("notifications", "enqueue success project=\(projectName) tool=\(tool)")
            }
        }
    }

    private func fallbackNotifyIfNeeded(projectName: String, tool: String, exitCode: Int?, reason: String) {
        let title = String(format: appI18n("project.activity.completed_format", fallback: "%@ completed"), tool)
        let body = exitCode == nil || exitCode == 0
            ? String(format: appI18n("project.activity.finished_successfully_format", fallback: "%@ finished successfully"), projectName)
            : String(format: appI18n("project.activity.finished_with_exit_code_format", fallback: "%@ finished with exit code %@"), projectName, "\(exitCode ?? -1)")

        if sendBundledNotificationHelper(title: title, body: body) {
            AppDebugLog.shared.log("notifications", "fallback success transport=dmux-notify-helper reason=\(reason) project=\(projectName) tool=\(tool)")
            return
        }
        AppDebugLog.shared.log("notifications", "fallback failed reason=\(reason) project=\(projectName) tool=\(tool)")
    }

    private func sendBundledNotificationHelper(title: String, body: String) -> Bool {
        if let appBundle = bundledNotificationHelperAppURL() {
            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/open")
            process.arguments = [
                "-n",
                appBundle.path,
                "--args",
                "--title", title,
                "--message", body,
            ]
            let stdoutPipe = Pipe()
            let stderrPipe = Pipe()
            process.standardOutput = stdoutPipe
            process.standardError = stderrPipe
            do {
                AppDebugLog.shared.log("notifications", "notify-helper app=\(appBundle.path)")
                try process.run()
                process.waitUntilExit()
                if process.terminationStatus == 0 {
                    return true
                }
                let errorOutput = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                AppDebugLog.shared.log("notifications", "notify-helper app exit=\(process.terminationStatus) stderr=\(errorOutput)")
            } catch {
                AppDebugLog.shared.log("notifications", "notify-helper app failed error=\(error.localizedDescription)")
            }
        }

        guard let executable = bundledNotificationHelperURL() else {
            return false
        }

        let process = Process()
        process.executableURL = executable
        process.arguments = [
            "--title", title,
            "--message", body,
        ]
        let stdoutPipe = Pipe()
        let stderrPipe = Pipe()
        process.standardOutput = stdoutPipe
        process.standardError = stderrPipe
        do {
            AppDebugLog.shared.log("notifications", "notify-helper exec=\(executable.path)")
            try process.run()
            process.waitUntilExit()
            if process.terminationStatus == 0 {
                return true
            }
            let errorOutput = String(data: stderrPipe.fileHandleForReading.readDataToEndOfFile(), encoding: .utf8)?
                .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            AppDebugLog.shared.log("notifications", "notify-helper exit=\(process.terminationStatus) stderr=\(errorOutput)")
            return false
        } catch {
            AppDebugLog.shared.log("notifications", "notify-helper failed error=\(error.localizedDescription)")
            return false
        }
    }

    private func bundledNotificationHelperAppURL() -> URL? {
        let candidates = [
            Bundle.main.resourceURL?.appendingPathComponent("Helpers/dmux-notify-helper.app"),
            Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/Helpers/dmux-notify-helper.app"),
        ]

        for url in candidates.compactMap({ $0 }) where fileManager.fileExists(atPath: url.path) {
            return url
        }
        return nil
    }

    private func bundledNotificationHelperURL() -> URL? {
        let candidates = [
            Bundle.main.resourceURL?.appendingPathComponent("Helpers/dmux-notify-helper.app/Contents/MacOS/dmux-notify-helper"),
            Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/Helpers/dmux-notify-helper.app/Contents/MacOS/dmux-notify-helper"),
            Bundle.main.resourceURL?.appendingPathComponent("Helpers/dmux-notify-helper"),
            Bundle.main.bundleURL.appendingPathComponent("Contents/Resources/Helpers/dmux-notify-helper"),
        ]

        for url in candidates.compactMap({ $0 }) where fileManager.isExecutableFile(atPath: url.path) {
            return url
        }
        return nil
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
