import Foundation

struct CodexPetPackage: Equatable, Sendable {
    var directoryURL: URL
    var manifest: CodexPetManifest

    var spritesheetURL: URL {
        directoryURL.appendingPathComponent(manifest.spritesheetPath, isDirectory: false)
    }
}

struct CodexPetPackageService {
    var fileManager: FileManager = .default

    func packages(rootURL: URL = Self.defaultPetsRootURL()) -> [CodexPetPackage] {
        guard let directories = try? fileManager.contentsOfDirectory(
            at: rootURL,
            includingPropertiesForKeys: [.isDirectoryKey],
            options: [.skipsHiddenFiles]
        ) else {
            return []
        }

        return directories.compactMap(package(at:)).sorted {
            $0.manifest.displayName.localizedStandardCompare($1.manifest.displayName) == .orderedAscending
        }
    }

    func package(at directoryURL: URL) -> CodexPetPackage? {
        let manifestURL = directoryURL.appendingPathComponent("pet.json", isDirectory: false)
        guard let data = try? Data(contentsOf: manifestURL),
              let manifest = try? JSONDecoder().decode(CodexPetManifest.self, from: data),
              manifest.id.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false,
              manifest.spritesheetPath.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else {
            return nil
        }

        let package = CodexPetPackage(directoryURL: directoryURL, manifest: manifest)
        guard fileManager.fileExists(atPath: package.spritesheetURL.path) else {
            return nil
        }
        return package
    }

    static func defaultPetsRootURL(homeURL: URL? = nil) -> URL {
        if homeURL == nil,
           let codexHome = ProcessInfo.processInfo.environment["CODEX_HOME"]?.trimmingCharacters(in: .whitespacesAndNewlines),
           codexHome.isEmpty == false {
            return URL(fileURLWithPath: codexHome, isDirectory: true)
                .appendingPathComponent("pets", isDirectory: true)
        }
        return (homeURL ?? URL(fileURLWithPath: NSHomeDirectory(), isDirectory: true))
            .appendingPathComponent(".codex/pets", isDirectory: true)
    }
}
