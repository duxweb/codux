import Foundation

struct AIToolPermissionSettingsService {
    private let fileManager = FileManager.default
    private let debugLog = AppDebugLog.shared

    func sync(_ settings: AppAIToolPermissionSettings) {
        guard let fileURL = configFileURL() else {
            return
        }

        let payload = Payload(
            codex: settings.codex.rawValue,
            claudeCode: settings.claudeCode.rawValue,
            gemini: settings.gemini.rawValue,
            opencode: settings.opencode.rawValue,
            codexModel: settings.codexModel,
            claudeCodeModel: settings.claudeCodeModel,
            geminiModel: settings.geminiModel,
            opencodeModel: settings.opencodeModel
        )

        do {
            try fileManager.createDirectory(at: fileURL.deletingLastPathComponent(), withIntermediateDirectories: true)
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(payload)
            try data.write(to: fileURL, options: Data.WritingOptions.atomic)
        } catch {
            debugLog.log("tool-permissions", "sync failed path=\(fileURL.path) error=\(error.localizedDescription)")
        }
    }

    private func configFileURL() -> URL? {
        AppRuntimePaths.appSupportRootURL(fileManager: fileManager)?
            .appendingPathComponent("tool-permissions.json", isDirectory: false)
    }

    private struct Payload: Codable {
        let codex: String
        let claudeCode: String
        let gemini: String
        let opencode: String
        let codexModel: String
        let claudeCodeModel: String
        let geminiModel: String
        let opencodeModel: String
    }
}
