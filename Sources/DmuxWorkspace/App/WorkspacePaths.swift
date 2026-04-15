import Foundation

enum WorkspacePaths {
    static func repositoryRoot(environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        if let override = environment["DMUX_WORKSPACE_ROOT"], !override.isEmpty {
            let overrideURL = URL(fileURLWithPath: override).standardizedFileURL
            if isRepositoryRoot(overrideURL) {
                return overrideURL
            }
        }

        for candidate in repositoryRootCandidates(environment: environment) {
            if let root = findRepositoryRoot(startingAt: candidate) {
                return root
            }
        }

        return URL(fileURLWithPath: FileManager.default.currentDirectoryPath).standardizedFileURL
    }

    static func repositoryResourceURL(_ relativePath: String, environment: [String: String] = ProcessInfo.processInfo.environment) -> URL {
        repositoryRoot(environment: environment)
            .appendingPathComponent(relativePath, isDirectory: false)
            .standardizedFileURL
    }

    private static func repositoryRootCandidates(environment: [String: String]) -> [URL] {
        var candidates: [URL] = []

        let currentDirectory = URL(fileURLWithPath: FileManager.default.currentDirectoryPath).standardizedFileURL
        candidates.append(currentDirectory)

        if let executableURL = Bundle.main.executableURL?.resolvingSymlinksInPath().standardizedFileURL {
            candidates.append(executableURL)
        }

        if let bundleURL = Bundle.main.bundleURL.resolvingSymlinksInPath().standardizedFileURL as URL? {
            candidates.append(bundleURL)
        }

        if let processPath = environment["_"] {
            candidates.append(URL(fileURLWithPath: processPath).resolvingSymlinksInPath().standardizedFileURL)
        }

        var deduplicated: [URL] = []
        var seen = Set<String>()
        for candidate in candidates {
            let key = candidate.path
            if seen.insert(key).inserted {
                deduplicated.append(candidate)
            }
        }
        return deduplicated
    }

    private static func findRepositoryRoot(startingAt url: URL) -> URL? {
        let fileManager = FileManager.default
        var candidate = url.hasDirectoryPath ? url : url.deletingLastPathComponent()

        while true {
            if isRepositoryRoot(candidate, fileManager: fileManager) {
                return candidate
            }

            let parent = candidate.deletingLastPathComponent()
            if parent.path == candidate.path {
                return nil
            }
            candidate = parent
        }
    }

    private static func isRepositoryRoot(_ url: URL, fileManager: FileManager = .default) -> Bool {
        let packageURL = url.appendingPathComponent("Package.swift", isDirectory: false)
        let shellHookURL = url.appendingPathComponent("scripts/shell-hooks/zsh/.zshrc", isDirectory: false)
        let wrapperURL = url.appendingPathComponent("scripts/wrappers/tool-wrapper.sh", isDirectory: false)
        return fileManager.fileExists(atPath: packageURL.path)
            && fileManager.fileExists(atPath: shellHookURL.path)
            && fileManager.fileExists(atPath: wrapperURL.path)
    }
}
