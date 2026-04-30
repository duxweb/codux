import Foundation
import UserNotifications

enum ProjectActivityPhase: Equatable {
    case idle
    case loading
    case running(tool: String)
    case waitingInput(tool: String)
    case completed(tool: String, finishedAt: Date, exitCode: Int?)
}

struct ProjectActivityService: @unchecked Sendable {
    private let externalNotificationService = AppExternalNotificationService.shared
    private let notificationCenter = UNUserNotificationCenter.current()

    private var supportsSystemNotifications: Bool {
        Bundle.main.bundleURL.pathExtension == "app" && Bundle.main.bundleIdentifier != nil
    }

    func requestNotificationPermission() {
        guard supportsSystemNotifications else {
            return
        }
        requestAuthorization(logPrefix: "authorization")
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
        notificationCenter.getNotificationSettings { settings in
            AppDebugLog.shared.log(
                "notifications",
                "\(statusLogMessage) status=\(settings.authorizationStatus.rawValue)"
            )

            switch settings.authorizationStatus {
            case .authorized, .provisional, .ephemeral:
                onAuthorized()
            case .notDetermined:
                requestAuthorization(logPrefix: requestLogPrefix, onGranted: onAuthorized)
            case .denied:
                AppDebugLog.shared.log("notifications", "\(statusLogMessage) skipped status=denied")
            @unknown default:
                AppDebugLog.shared.log("notifications", "\(statusLogMessage) skipped status=unknown")
            }
        }
    }

    private func enqueueNotification(projectName: String, tool: String, exitCode: Int?) {
        let event = AppExternalNotificationEvent(
            projectName: projectName,
            tool: tool,
            exitCode: exitCode
        )
        let content = UNMutableNotificationContent()
        content.title = event.title
        content.body = event.body
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "project-activity-\(projectName)-\(tool)-\(Date().timeIntervalSince1970)",
            content: content,
            trigger: nil
        )
        enqueue(
            request,
            successLogMessage: "enqueue success project=\(projectName) tool=\(tool)",
            failureLogMessage: "enqueue failed"
        )
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
        content.body = needsInputBody(
            projectName: projectName,
            tool: tool,
            notificationType: notificationType,
            targetToolName: targetToolName,
            message: message
        )
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "project-activity-waiting-\(projectName)-\(tool)-\(Date().timeIntervalSince1970)",
            content: content,
            trigger: nil
        )
        enqueue(
            request,
            successLogMessage: "waiting-input enqueue success project=\(projectName) tool=\(tool)",
            failureLogMessage: "waiting-input enqueue failed"
        )
    }

    private func enqueue(
        _ request: UNNotificationRequest,
        successLogMessage: String,
        failureLogMessage: String
    ) {
        notificationCenter.add(request) { error in
            if let error {
                AppDebugLog.shared.log("notifications", "\(failureLogMessage) error=\(error.localizedDescription)")
            } else {
                AppDebugLog.shared.log("notifications", successLogMessage)
            }
        }
    }

    private func requestAuthorization(
        logPrefix: String,
        onGranted: (@Sendable () -> Void)? = nil
    ) {
        notificationCenter.requestAuthorization(options: [.alert, .sound, .badge]) { granted, error in
            if let error {
                AppDebugLog.shared.log("notifications", "\(logPrefix) error=\(error.localizedDescription)")
                return
            }
            AppDebugLog.shared.log("notifications", "\(logPrefix) granted=\(granted)")
            guard granted else {
                AppDebugLog.shared.log("notifications", "\(logPrefix) denied")
                return
            }
            onGranted?()
        }
    }

    private func needsInputBody(
        projectName: String,
        tool: String,
        notificationType: String?,
        targetToolName: String?,
        message: String?
    ) -> String {
        if let message, !message.isEmpty {
            return message
        }
        if let targetToolName, !targetToolName.isEmpty {
            return String(
                format: String(
                    localized: "project.activity.permission_request_format",
                    defaultValue: "%@ needs confirmation for %@ in %@",
                    bundle: .module
                ),
                tool,
                targetToolName,
                projectName
            )
        }
        if let notificationType, !notificationType.isEmpty {
            return String(
                format: String(
                    localized: "project.activity.notification_request_format",
                    defaultValue: "%@ is waiting for %@ in %@",
                    bundle: .module
                ),
                tool,
                notificationType,
                projectName
            )
        }
        return String(
            format: String(
                localized: "project.activity.generic_input_request_format",
                defaultValue: "%@ is waiting for your input in %@",
                bundle: .module
            ),
            tool,
            projectName
        )
    }
}
