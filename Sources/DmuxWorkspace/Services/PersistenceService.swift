import Foundation

struct PersistenceService {
    private let fileManager = FileManager.default

    func load() -> AppSnapshot? {
        guard let fileURL = stateFileURL(),
              fileManager.fileExists(atPath: fileURL.path),
              let data = try? Data(contentsOf: fileURL) else {
            return nil
        }

        return try? JSONDecoder().decode(AppSnapshot.self, from: data)
    }

    func save(_ snapshot: AppSnapshot) {
        guard let directoryURL = appSupportDirectoryURL() else {
            return
        }

        do {
            try fileManager.createDirectory(at: directoryURL, withIntermediateDirectories: true)
            let data = try JSONEncoder.pretty.encode(snapshot)
            try data.write(to: directoryURL.appendingPathComponent("state.json"), options: .atomic)
        } catch {
            assertionFailure("Failed to save app state: \(error)")
        }
    }

    private func stateFileURL() -> URL? {
        appSupportDirectoryURL()?.appendingPathComponent("state.json")
    }

    private func appSupportDirectoryURL() -> URL? {
        fileManager.urls(for: .applicationSupportDirectory, in: .userDomainMask).first?.appendingPathComponent("dmux")
    }
}

private extension JSONEncoder {
    static var pretty: JSONEncoder {
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return encoder
    }
}
