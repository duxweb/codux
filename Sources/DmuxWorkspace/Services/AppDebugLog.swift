import AppKit
import Foundation

final class AppDebugLog: @unchecked Sendable {
    static let shared = AppDebugLog()

    private enum LoggingProfile {
        case verbose
        case compact
    }

    private let fileManager = FileManager.default
    private let queue = DispatchQueue(label: "dmux.debug.log", qos: .utility)
    private var lastLoggedAtByDedupKey: [String: Date] = [:]
    private let dateFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()
    private lazy var loggingProfile: LoggingProfile = {
        if let override = ProcessInfo.processInfo.environment["DMUX_LOG_PROFILE"]?.lowercased() {
            if override == "verbose" || override == "debug" {
                return .verbose
            }
            if override == "compact" || override == "release" {
                return .compact
            }
        }

        if let defaultsOverride = UserDefaults.standard.object(forKey: "dmux.logProfile") as? String {
            let normalized = defaultsOverride.lowercased()
            if normalized == "verbose" || normalized == "debug" {
                return .verbose
            }
            if normalized == "compact" || normalized == "release" {
                return .compact
            }
        }

        if let infoOverride = Bundle.main.object(forInfoDictionaryKey: "DMUXLogProfile") as? String {
            let normalized = infoOverride.lowercased()
            if normalized == "verbose" || normalized == "debug" {
                return .verbose
            }
            if normalized == "compact" || normalized == "release" {
                return .compact
            }
        }

        #if DEBUG
        return .verbose
        #else
        return .compact
        #endif
    }()

    private init() {}

    func logsDirectoryURL() -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let directoryURL = appSupport.appendingPathComponent("dmux/logs", isDirectory: true)
        try? fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        return directoryURL
    }

    private func logFileName(kind: String, ext: String) -> String {
        let bundleName = Bundle.main.bundleURL.deletingPathExtension().lastPathComponent.lowercased()
        let channel = bundleName.contains("dev") ? "dev" : "release"
        return "dmux-\(kind).\(channel).\(ext)"
    }

    func logFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "debug", ext: "log"), isDirectory: false)
    }

    func previousLogFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "debug.previous", ext: "log"), isDirectory: false)
    }

    func performanceSummaryFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "performance-summary", ext: "json"), isDirectory: false)
    }

    func log(_ category: String, _ message: String) {
        let fileURL = logFileURL()

        queue.async { [self] in
            let now = Date()
            guard shouldLog(category: category, message: message, now: now) else {
                return
            }

            let fileManager = FileManager.default
            Self.rotateIfNeeded(fileURL: fileURL, fileManager: fileManager)

            let timestamp = dateFormatter.string(from: now)
            let line = "[\(timestamp)] [\(category)] \(message)\n"
            let data = Data(line.utf8)
            if fileManager.fileExists(atPath: fileURL.path) == false {
                fileManager.createFile(atPath: fileURL.path, contents: data)
                return
            }

            guard let handle = try? FileHandle(forWritingTo: fileURL) else {
                return
            }
            defer {
                try? handle.close()
            }
            do {
                try handle.seekToEnd()
                try handle.write(contentsOf: data)
            } catch {
                return
            }
        }
    }

    func reset() {
        let fileURL = logFileURL()
        let archivedURL = previousLogFileURL()
        let performanceSummaryURL = performanceSummaryFileURL()
        queue.sync {
            lastLoggedAtByDedupKey.removeAll()
            try? fileManager.removeItem(at: fileURL)
            try? fileManager.removeItem(at: archivedURL)
            try? fileManager.removeItem(at: performanceSummaryURL)
            fileManager.createFile(atPath: fileURL.path, contents: Data())
        }
    }

    private func shouldLog(category: String, message: String, now: Date) -> Bool {
        if loggingProfile == .compact {
            switch category {
            case "startup-ui", "runtime-hooks", "terminal-start", "terminal-ready", "terminal-env":
                return false
            default:
                break
            }
        }

        switch category {
        case "terminal-env":
            return false
        case "app":
            return message != "open debug log"
        case "codex-hook":
            return !message.hasPrefix("ingest files=")
                && !message.hasPrefix("skip file=")
        default:
            break
        }

        guard let dedupeInterval = dedupeInterval(for: category, message: message) else {
            return true
        }

        let key = "\(category)|\(message)"
        if let lastLoggedAt = lastLoggedAtByDedupKey[key],
           now.timeIntervalSince(lastLoggedAt) < dedupeInterval {
            return false
        }

        lastLoggedAtByDedupKey[key] = now
        return true
    }

    private func dedupeInterval(for category: String, message: String) -> TimeInterval? {
        switch category {
        case "activity-phase":
            return 5
        case "startup-ui":
            if message.hasPrefix("workspace-view ")
                || message.hasPrefix("top-pane ")
                || message.hasPrefix("terminal-pane appear")
                || message.hasPrefix("terminal-host make") {
                return 1.5
            }
            return nil
        case "runtime-ingress":
            if message.hasPrefix("normalize live session=") {
                return 5
            }
            return nil
        case "opencode-global":
            if message.hasPrefix("stream failed error=") {
                return 30
            }
            if message == "stream connected"
                || message == "stream cancelled"
                || message == "stream rejected" {
                return 10
            }
            return nil
        default:
            return nil
        }
    }

    func openInSystemViewer() {
        let url = logFileURL()
        if fileManager.fileExists(atPath: url.path) == false {
            fileManager.createFile(atPath: url.path, contents: Data())
        }
        NSWorkspace.shared.open(url)
    }

    private static func rotateIfNeeded(fileURL: URL, fileManager: FileManager) {
        guard let values = try? fileURL.resourceValues(forKeys: [.fileSizeKey]),
              let fileSize = values.fileSize,
              fileSize > 1_500_000 else {
            return
        }

        let fileName = fileURL.lastPathComponent
        let archivedName = fileName.replacingOccurrences(of: ".log", with: ".previous.log")
        let archivedURL = fileURL.deletingLastPathComponent().appendingPathComponent(archivedName, isDirectory: false)
        try? fileManager.removeItem(at: archivedURL)
        try? fileManager.moveItem(at: fileURL, to: archivedURL)
    }
}
