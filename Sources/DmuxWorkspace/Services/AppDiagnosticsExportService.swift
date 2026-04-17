import AppKit
import Foundation
import UniformTypeIdentifiers

struct AppDiagnosticsExportService {
    private let fileManager = FileManager.default

    @MainActor
    func requestExportDestination(appDisplayName: String) throws -> URL {
        let savePanel = NSSavePanel()
        savePanel.canCreateDirectories = true
        savePanel.isExtensionHidden = false
        savePanel.allowedContentTypes = [.zip]
        savePanel.nameFieldStringValue = archiveFileName(appDisplayName: appDisplayName)
        savePanel.title = String(localized: "diagnostics.export.panel.title", defaultValue: "Export Diagnostics", bundle: .module)
        savePanel.message = String(localized: "diagnostics.export.panel.message", defaultValue: "Choose where to save the diagnostics archive.", bundle: .module)

        guard savePanel.runModal() == .OK, let destinationURL = savePanel.url else {
            throw CancellationError()
        }
        return destinationURL
    }

    func exportArchive(to destinationURL: URL, appDisplayName: String, appVersion: String) throws -> URL {
        let exportName = destinationURL.lastPathComponent

        let stagingRootURL = try makeStagingRoot()
        defer {
            try? fileManager.removeItem(at: stagingRootURL)
        }
        let bundleRootURL = stagingRootURL.appendingPathComponent(exportName.replacingOccurrences(of: ".zip", with: ""), isDirectory: true)
        try fileManager.createDirectory(at: bundleRootURL, withIntermediateDirectories: true)

        do {
            try writeManifest(to: bundleRootURL, appDisplayName: appDisplayName, appVersion: appVersion)
            try copyKnownFiles(to: bundleRootURL)
            try copyMatchingDiagnosticReports(to: bundleRootURL)

            if fileManager.fileExists(atPath: destinationURL.path) {
                try fileManager.removeItem(at: destinationURL)
            }

            try createZipArchive(sourceDirectoryURL: bundleRootURL, destinationURL: destinationURL)
            return destinationURL
        } catch {
            try? fileManager.removeItem(at: destinationURL)
            throw error
        }
    }

    private func makeStagingRoot() throws -> URL {
        let rootURL = fileManager.temporaryDirectory
            .appendingPathComponent("codux-diagnostics-\(UUID().uuidString)", isDirectory: true)
        try fileManager.createDirectory(at: rootURL, withIntermediateDirectories: true)
        return rootURL
    }

    private func archiveFileName(appDisplayName: String) -> String {
        let sanitizedName = appDisplayName
            .replacingOccurrences(of: "/", with: "-")
            .replacingOccurrences(of: ":", with: "-")
            .replacingOccurrences(of: "\n", with: " ")
        let timestamp = Self.timestampFormatter.string(from: Date())
        return "\(sanitizedName)-diagnostics-\(timestamp).zip"
    }

    private func writeManifest(to rootURL: URL, appDisplayName: String, appVersion: String) throws {
        let lines = [
            "App: \(appDisplayName)",
            "Version: \(appVersion)",
            "Bundle Identifier: \(Bundle.main.bundleIdentifier ?? "unknown")",
            "macOS: \(ProcessInfo.processInfo.operatingSystemVersionString)",
            "Exported At: \(ISO8601DateFormatter().string(from: Date()))",
            "Home: \(NSHomeDirectory())",
        ]
        let manifestURL = rootURL.appendingPathComponent("manifest.txt", isDirectory: false)
        try Data(lines.joined(separator: "\n").appending("\n").utf8).write(to: manifestURL, options: .atomic)
    }

    private func copyKnownFiles(to rootURL: URL) throws {
        let appSupportRootURL = appSupportDirectoryURL()
        let logsDirectoryURL = rootURL.appendingPathComponent("logs", isDirectory: true)
        let stateDirectoryURL = rootURL.appendingPathComponent("state", isDirectory: true)
        let externalConfigDirectoryURL = rootURL.appendingPathComponent("external-config", isDirectory: true)
        let debugLog = AppDebugLog.shared

        try fileManager.createDirectory(at: logsDirectoryURL, withIntermediateDirectories: true)
        try fileManager.createDirectory(at: stateDirectoryURL, withIntermediateDirectories: true)
        try fileManager.createDirectory(at: externalConfigDirectoryURL, withIntermediateDirectories: true)

        copyIfExists(
            from: debugLog.logFileURL(),
            to: logsDirectoryURL.appendingPathComponent(debugLog.logFileURL().lastPathComponent, isDirectory: false)
        )
        copyIfExists(
            from: debugLog.previousLogFileURL(),
            to: logsDirectoryURL.appendingPathComponent(debugLog.previousLogFileURL().lastPathComponent, isDirectory: false)
        )
        copyIfExists(
            from: debugLog.performanceSummaryFileURL(),
            to: logsDirectoryURL.appendingPathComponent(debugLog.performanceSummaryFileURL().lastPathComponent, isDirectory: false)
        )
        copyIfExists(
            from: appSupportRootURL.appendingPathComponent("state.json", isDirectory: false),
            to: stateDirectoryURL.appendingPathComponent("state.json", isDirectory: false)
        )

        copyMatchingFiles(
            in: appSupportRootURL,
            to: stateDirectoryURL,
            matching: { $0.lastPathComponent.hasPrefix("state.invalid-") && $0.pathExtension == "json" }
        )

        let codexConfigRoot = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true).appendingPathComponent(".codex", isDirectory: true)
        let geminiConfigRoot = URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true).appendingPathComponent(".gemini", isDirectory: true)

        copyIfExists(
            from: codexConfigRoot.appendingPathComponent("hooks.json", isDirectory: false),
            to: externalConfigDirectoryURL.appendingPathComponent("codex-hooks.json", isDirectory: false)
        )
        copyMatchingFiles(
            in: codexConfigRoot,
            to: externalConfigDirectoryURL,
            matching: { $0.lastPathComponent.hasPrefix("hooks.invalid-") && $0.pathExtension == "json" }
        )
        copyIfExists(
            from: geminiConfigRoot.appendingPathComponent("settings.json", isDirectory: false),
            to: externalConfigDirectoryURL.appendingPathComponent("gemini-settings.json", isDirectory: false)
        )
        copyMatchingFiles(
            in: geminiConfigRoot,
            to: externalConfigDirectoryURL,
            matching: { $0.lastPathComponent.hasPrefix("settings.invalid-") && $0.pathExtension == "json" }
        )
    }

    private func copyMatchingDiagnosticReports(to rootURL: URL) throws {
        let reportsRootURL = diagnosticReportsDirectoryURL()
        guard fileManager.fileExists(atPath: reportsRootURL.path) else {
            return
        }

        let destinationURL = rootURL.appendingPathComponent("diagnostic-reports", isDirectory: true)
        try fileManager.createDirectory(at: destinationURL, withIntermediateDirectories: true)

        let reportURLs = (try? fileManager.contentsOfDirectory(
            at: reportsRootURL,
            includingPropertiesForKeys: [.contentModificationDateKey, .isRegularFileKey],
            options: [.skipsHiddenFiles]
        )) ?? []

        let matchingReportURLs = reportURLs
            .filter(isRelevantDiagnosticReport(_:))
            .sorted { lhs, rhs in
                let lhsDate = (try? lhs.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                let rhsDate = (try? rhs.resourceValues(forKeys: [.contentModificationDateKey]).contentModificationDate) ?? .distantPast
                return lhsDate > rhsDate
            }
            .prefix(12)

        for sourceURL in matchingReportURLs {
            copyIfExists(
                from: sourceURL,
                to: destinationURL.appendingPathComponent(sourceURL.lastPathComponent, isDirectory: false)
            )
        }
    }

    private func createZipArchive(sourceDirectoryURL: URL, destinationURL: URL) throws {
        let process = Process()
        process.executableURL = URL(fileURLWithPath: "/usr/bin/ditto")
        process.arguments = [
            "-c",
            "-k",
            "--sequesterRsrc",
            "--keepParent",
            sourceDirectoryURL.path,
            destinationURL.path,
        ]

        let outputPipe = Pipe()
        process.standardError = outputPipe

        try process.run()
        process.waitUntilExit()

        guard process.terminationStatus == 0 else {
            let outputData = outputPipe.fileHandleForReading.readDataToEndOfFile()
            let output = String(data: outputData, encoding: .utf8)?.trimmingCharacters(in: .whitespacesAndNewlines)
            throw NSError(
                domain: "codux.diagnostics",
                code: Int(process.terminationStatus),
                userInfo: [
                    NSLocalizedDescriptionKey: output?.isEmpty == false
                        ? output!
                        : String(localized: "diagnostics.export.failed", defaultValue: "Failed to export diagnostics.", bundle: .module)
                ]
            )
        }
    }

    private func copyIfExists(from sourceURL: URL, to destinationURL: URL) {
        guard fileManager.fileExists(atPath: sourceURL.path) else {
            return
        }
        try? fileManager.copyItem(at: sourceURL, to: destinationURL)
    }

    private func copyMatchingFiles(in directoryURL: URL, to destinationDirectoryURL: URL, matching predicate: (URL) -> Bool) {
        guard fileManager.fileExists(atPath: directoryURL.path),
              let fileURLs = try? fileManager.contentsOfDirectory(
                at: directoryURL,
                includingPropertiesForKeys: [.isRegularFileKey],
                options: [.skipsHiddenFiles]
              ) else {
            return
        }

        for sourceURL in fileURLs where predicate(sourceURL) {
            copyIfExists(
                from: sourceURL,
                to: destinationDirectoryURL.appendingPathComponent(sourceURL.lastPathComponent, isDirectory: false)
            )
        }
    }

    private func isRelevantDiagnosticReport(_ fileURL: URL) -> Bool {
        let name = fileURL.lastPathComponent.lowercased()
        let prefixes = ["codux-", "dmux-", "dmux-bin-"]
        let extensions = ["ips", "spin", "hang", "sample"]
        return prefixes.contains(where: { name.hasPrefix($0) }) && extensions.contains(fileURL.pathExtension.lowercased())
    }

    private func appSupportDirectoryURL() -> URL {
        AIRuntimeBridgeService().runtimeSupportRootURL(createIfNeeded: false)
    }

    private func diagnosticReportsDirectoryURL() -> URL {
        URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true)
            .appendingPathComponent("Library/Logs/DiagnosticReports", isDirectory: true)
    }
}

private extension AppDiagnosticsExportService {
    static let timestampFormatter: DateFormatter = {
        let formatter = DateFormatter()
        formatter.locale = Locale(identifier: "en_US_POSIX")
        formatter.dateFormat = "yyyyMMdd-HHmmss"
        return formatter
    }()
}
