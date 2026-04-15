import AppKit
import Foundation
import UserNotifications

enum NotifyHelperError: LocalizedError {
    case missingValue(String)

    var errorDescription: String? {
        switch self {
        case .missingValue(let flag):
            return "Missing value for \(flag)"
        }
    }
}

enum NotifyHelperLog {
    static func write(_ message: String) {
        let fileManager = FileManager.default
        guard let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first else {
            return
        }
        let logsDirectory = appSupport.appendingPathComponent("dmux/logs", isDirectory: true)
        try? fileManager.createDirectory(at: logsDirectory, withIntermediateDirectories: true)
        let logFile = logsDirectory.appendingPathComponent("dmux-notify-helper.log")
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        let line = "[\(formatter.string(from: Date()))] \(message)\n"
        if let data = line.data(using: .utf8) {
            if fileManager.fileExists(atPath: logFile.path),
               let handle = try? FileHandle(forWritingTo: logFile) {
                _ = try? handle.seekToEnd()
                try? handle.write(contentsOf: data)
                try? handle.close()
            } else {
                try? data.write(to: logFile, options: .atomic)
            }
        }
    }
}

struct NotifyPayload {
    var title: String = ""
    var message: String = ""
    var subtitle: String?

    init(arguments: [String]) throws {
        var iterator = arguments.makeIterator()
        while let argument = iterator.next() {
            switch argument {
            case "--title":
                guard let value = iterator.next() else { throw NotifyHelperError.missingValue(argument) }
                title = value
            case "--message":
                guard let value = iterator.next() else { throw NotifyHelperError.missingValue(argument) }
                message = value
            case "--subtitle":
                guard let value = iterator.next() else { throw NotifyHelperError.missingValue(argument) }
                subtitle = value
            default:
                continue
            }
        }
    }
}

@main
struct DmuxNotifyHelper {
    static func main() {
        do {
            let app = NSApplication.shared
            app.setActivationPolicy(.accessory)
            NotifyHelperLog.write("launch pid=\(ProcessInfo.processInfo.processIdentifier)")

            let payload = try NotifyPayload(arguments: Array(CommandLine.arguments.dropFirst()))
            guard !payload.title.isEmpty, !payload.message.isEmpty else {
                fputs("usage: dmux-notify-helper --title <title> --message <message> [--subtitle <subtitle>]\n", stderr)
                Foundation.exit(64)
            }
            NotifyHelperLog.write("payload title=\(payload.title) subtitle=\(payload.subtitle ?? "nil")")

            if deliverUserNotification(payload) {
                NotifyHelperLog.write("delivery success")
                Foundation.exit(0)
            }
            NotifyHelperLog.write("delivery failed")
            fputs("dmux-notify-helper error: unable to deliver notification via UNUserNotificationCenter\n", stderr)
            Foundation.exit(1)
        } catch {
            NotifyHelperLog.write("fatal error=\(error.localizedDescription)")
            fputs("dmux-notify-helper error: \(error.localizedDescription)\n", stderr)
            Foundation.exit(1)
        }
    }

    private static func deliverUserNotification(_ payload: NotifyPayload) -> Bool {
        let center = UNUserNotificationCenter.current()

        let authorizationStatus = currentAuthorizationStatus(center: center)
        NotifyHelperLog.write("authorization status=\(authorizationStatus.rawValue)")
        switch authorizationStatus {
        case .authorized, .provisional, .ephemeral:
            break
        case .notDetermined:
            let granted = requestAuthorization(center: center)
            NotifyHelperLog.write("authorization requested granted=\(granted)")
            guard granted else {
                return false
            }
        case .denied:
            NotifyHelperLog.write("authorization denied")
            return false
        @unknown default:
            NotifyHelperLog.write("authorization unknown")
            return false
        }

        let content = UNMutableNotificationContent()
        content.title = payload.title
        content.body = payload.message
        if let subtitle = payload.subtitle, !subtitle.isEmpty {
            content.subtitle = subtitle
        }
        content.sound = .default

        let request = UNNotificationRequest(
            identifier: "dmux-notify-\(UUID().uuidString)",
            content: content,
            trigger: nil
        )

        let semaphore = DispatchSemaphore(value: 0)
        final class DeliveryBox: @unchecked Sendable { var error: Error? }
        let box = DeliveryBox()
        center.add(request) { error in
            box.error = error
            semaphore.signal()
        }
        semaphore.wait()
        if let error = box.error {
            NotifyHelperLog.write("center.add error=\(error.localizedDescription)")
            return false
        }
        Thread.sleep(forTimeInterval: 0.35)
        return true
    }

    private static func currentAuthorizationStatus(center: UNUserNotificationCenter) -> UNAuthorizationStatus {
        let semaphore = DispatchSemaphore(value: 0)
        final class StatusBox: @unchecked Sendable { var value: UNAuthorizationStatus = .notDetermined }
        let box = StatusBox()
        center.getNotificationSettings { settings in
            box.value = settings.authorizationStatus
            semaphore.signal()
        }
        semaphore.wait()
        return box.value
    }

    private static func requestAuthorization(center: UNUserNotificationCenter) -> Bool {
        let semaphore = DispatchSemaphore(value: 0)
        final class GrantBox: @unchecked Sendable { var granted = false }
        let box = GrantBox()
        center.requestAuthorization(options: [.alert, .sound, .badge]) { allowed, _ in
            box.granted = allowed
            semaphore.signal()
        }
        semaphore.wait()
        return box.granted
    }
}
