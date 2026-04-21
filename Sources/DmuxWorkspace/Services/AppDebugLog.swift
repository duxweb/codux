import AppKit
import Foundation
import Logging

private enum AppLogFileKind {
    case runtime
    case live
}

private final class AppDebugLogBackend: @unchecked Sendable {
    static let shared = AppDebugLogBackend()

    private enum LoggingProfile {
        case verbose
        case compact
    }

    private let fileManager = FileManager.default
    private let queue = DispatchQueue(label: "dmux.debug.log", qos: .utility)
    private var lastLoggedAtByDedupKey: [String: Date] = [:]
    private let dateFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        let options: ISO8601DateFormatter.Options = [.withInternetDateTime, .withFractionalSeconds]
        formatter.formatOptions = options
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

    func bootstrapIfNeeded() {
        LoggingSystem.bootstrap { label in
            AppDebugLogHandler(label: label)
        }
    }

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

    func runtimeLogFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "runtime", ext: "log"), isDirectory: false)
    }

    func previousRuntimeLogFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "runtime.previous", ext: "log"), isDirectory: false)
    }

    func liveLogFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "live", ext: "log"), isDirectory: false)
    }

    func previousLiveLogFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "live.previous", ext: "log"), isDirectory: false)
    }

    func performanceSummaryFileURL() -> URL {
        logsDirectoryURL().appendingPathComponent(logFileName(kind: "performance-summary", ext: "json"), isDirectory: false)
    }

    func write(
        label: String,
        level: Logger.Level,
        message: String,
        metadata: Logger.Metadata?,
        source: String
    ) {
        let runtimeURL = runtimeLogFileURL()
        let liveURL = liveLogFileURL()

        queue.async { [self] in
            let now = Date()
            guard shouldLog(category: label, message: message, now: now) else {
                return
            }

            let timestamp = dateFormatter.string(from: now)
            let metadataSuffix = formattedMetadata(metadata)
            let sourceSuffix = source.isEmpty ? "" : " source=\(source)"
            let levelPrefix = level == .info ? "" : " level=\(level.rawValue)"
            let line = "[\(timestamp)] [\(label)]\(levelPrefix)\(sourceSuffix) \(message)\(metadataSuffix)\n"
            let data = Data(line.utf8)

            writeLine(data, to: runtimeURL, archivedURL: previousRuntimeLogFileURL())
            if shouldWriteToLiveLog(category: label) {
                writeLine(data, to: liveURL, archivedURL: previousLiveLogFileURL())
            }
        }
    }

    func reset() {
        let runtimeURL = runtimeLogFileURL()
        let previousRuntimeURL = previousRuntimeLogFileURL()
        let liveURL = liveLogFileURL()
        let previousLiveURL = previousLiveLogFileURL()
        let performanceSummaryURL = performanceSummaryFileURL()

        queue.sync {
            lastLoggedAtByDedupKey.removeAll()
            try? fileManager.removeItem(at: runtimeURL)
            try? fileManager.removeItem(at: previousRuntimeURL)
            try? fileManager.removeItem(at: liveURL)
            try? fileManager.removeItem(at: previousLiveURL)
            try? fileManager.removeItem(at: performanceSummaryURL)
            fileManager.createFile(atPath: runtimeURL.path, contents: Data())
            fileManager.createFile(atPath: liveURL.path, contents: Data())
        }
    }

    func openRuntimeLogInSystemViewer() {
        let url = runtimeLogFileURL()
        ensureFileExists(at: url)
        NSWorkspace.shared.open(url)
    }

    func openLiveLogInSystemViewer() {
        let url = liveLogFileURL()
        ensureFileExists(at: url)
        NSWorkspace.shared.open(url)
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
            return message != "open runtime log" && message != "open live log"
        case "activity":
            return message.contains("phase=running:")
                || message.contains("phase=completed:")
                || message.contains("phase=failed:")
        case "activity-phase":
            return message.contains("phase=running:")
                || message.contains("phase=completed:")
                || message.contains("phase=failed:")
                || message.contains("source=runtime")
        case "claude-runtime":
            return !message.hasPrefix("suppress phase ")
        case "runtime-refresh":
            return message.hasPrefix("reset session=")
                || message.hasPrefix("stop session=")
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
            return 10
        case "runtime-store":
            if message.hasPrefix("snapshot session=") {
                return 15
            }
            if message.hasPrefix("live session=") {
                return 10
            }
            return nil
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
                return 15
            }
            if message.hasPrefix("drop source tool=opencode ") {
                return nil
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
        case "gemini-runtime":
            if message.hasPrefix("hit runtimeSession=") {
                return 20
            }
            return nil
        case "claude-runtime":
            if message.hasPrefix("hit source=") {
                return 20
            }
            return nil
        default:
            return nil
        }
    }

    private func shouldWriteToLiveLog(category: String) -> Bool {
        switch category {
        case "runtime-socket",
             "runtime-interrupt",
             "ai-session-store",
             "runtime-ingress":
            return true
        default:
            return false
        }
    }

    private func formattedMetadata(_ metadata: Logger.Metadata?) -> String {
        guard let metadata, !metadata.isEmpty else {
            return ""
        }
        let joined = metadata
            .sorted { $0.key < $1.key }
            .map { "\($0.key)=\($0.value)" }
            .joined(separator: " ")
        return joined.isEmpty ? "" : " \(joined)"
    }

    private func writeLine(_ data: Data, to fileURL: URL, archivedURL: URL) {
        Self.rotateIfNeeded(fileURL: fileURL, archivedURL: archivedURL, fileManager: fileManager)

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

    private func ensureFileExists(at url: URL) {
        if fileManager.fileExists(atPath: url.path) == false {
            fileManager.createFile(atPath: url.path, contents: Data())
        }
    }

    private static func rotateIfNeeded(fileURL: URL, archivedURL: URL, fileManager: FileManager) {
        guard let values = try? fileURL.resourceValues(forKeys: [.fileSizeKey]),
              let fileSize = values.fileSize,
              fileSize > 1_500_000 else {
            return
        }

        try? fileManager.removeItem(at: archivedURL)
        try? fileManager.moveItem(at: fileURL, to: archivedURL)
    }
}

private struct AppDebugLogHandler: LogHandler {
    let label: String
    var metadata: Logger.Metadata = [:]
    var logLevel: Logger.Level = .info

    subscript(metadataKey metadataKey: String) -> Logger.Metadata.Value? {
        get { metadata[metadataKey] }
        set { metadata[metadataKey] = newValue }
    }

    func log(event: LogEvent) {
        guard event.level >= logLevel else {
            return
        }

        var mergedMetadata = self.metadata
        if let metadata = event.metadata {
            mergedMetadata.merge(metadata) { _, new in new }
        }

        AppDebugLogBackend.shared.write(
            label: label,
            level: event.level,
            message: event.message.description,
            metadata: mergedMetadata,
            source: event.source
        )
    }
}

final class AppDebugLog: @unchecked Sendable {
    static let shared = AppDebugLog()

    private let backend = AppDebugLogBackend.shared

    private init() {
        backend.bootstrapIfNeeded()
    }

    func logsDirectoryURL() -> URL {
        backend.logsDirectoryURL()
    }

    func logFileURL() -> URL {
        backend.runtimeLogFileURL()
    }

    func previousLogFileURL() -> URL {
        backend.previousRuntimeLogFileURL()
    }

    func liveLogFileURL() -> URL {
        backend.liveLogFileURL()
    }

    func previousLiveLogFileURL() -> URL {
        backend.previousLiveLogFileURL()
    }

    func performanceSummaryFileURL() -> URL {
        backend.performanceSummaryFileURL()
    }

    func log(_ category: String, _ message: String, level: Logger.Level = .info) {
        var logger = Logger(label: category)
        logger.logLevel = level
        logger.log(level: level, "\(message)")
    }

    func reset() {
        backend.reset()
    }

    func openInSystemViewer() {
        backend.openRuntimeLogInSystemViewer()
    }

    func openRuntimeLogInSystemViewer() {
        backend.openRuntimeLogInSystemViewer()
    }

    func openLiveLogInSystemViewer() {
        backend.openLiveLogInSystemViewer()
    }
}
