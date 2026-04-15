import AppKit
import Foundation

final class AppDebugLog: @unchecked Sendable {
    static let shared = AppDebugLog()

    private let fileManager = FileManager.default
    private let queue = DispatchQueue(label: "dmux.debug.log", qos: .utility)
    private let dateFormatter: ISO8601DateFormatter = {
        let formatter = ISO8601DateFormatter()
        formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
        return formatter
    }()

    private init() {}

    func logFileURL() -> URL {
        let appSupport = fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first!
        let directoryURL = appSupport.appendingPathComponent("dmux/logs", isDirectory: true)
        try? fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
        return directoryURL.appendingPathComponent("dmux-debug.log", isDirectory: false)
    }

    func log(_ category: String, _ message: String) {
        guard shouldLog(category: category, message: message) else {
            return
        }

        let timestamp = dateFormatter.string(from: Date())
        let line = "[\(timestamp)] [\(category)] \(message)\n"
        let fileURL = logFileURL()

        queue.async {
            let fileManager = FileManager.default
            Self.rotateIfNeeded(fileURL: fileURL, fileManager: fileManager)

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
        let archivedURL = fileURL.deletingLastPathComponent().appendingPathComponent("dmux-debug.previous.log", isDirectory: false)
        queue.sync {
            try? fileManager.removeItem(at: archivedURL)
            if fileManager.fileExists(atPath: fileURL.path) {
                try? fileManager.moveItem(at: fileURL, to: archivedURL)
            }
            fileManager.createFile(atPath: fileURL.path, contents: Data())
        }
    }

    private func shouldLog(category: String, message: String) -> Bool {
        switch category {
        case "terminal-env":
            return false
        case "app":
            return message != "open debug log"
        case "codex-hook":
            return !message.hasPrefix("ingest files=")
                && !message.hasPrefix("skip file=")
        default:
            return true
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

        let archivedURL = fileURL.deletingLastPathComponent().appendingPathComponent("dmux-debug.previous.log", isDirectory: false)
        try? fileManager.removeItem(at: archivedURL)
        try? fileManager.moveItem(at: fileURL, to: archivedURL)
    }
}
