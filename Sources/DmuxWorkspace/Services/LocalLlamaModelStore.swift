import CryptoKit
import Foundation

struct LocalLlamaModelDownloadSource: Codable, Equatable, Sendable {
    var id: String
    var region: String
    var url: URL
}

enum LocalLlamaModelDownloadRoute: String, Codable, CaseIterable, Hashable, Identifiable, Sendable {
    case china
    case international

    var id: String { rawValue }
}

struct LocalLlamaRecommendedRuntimeConfig: Codable, Equatable, Sendable {
    var contextTokens: Int
    var maxPredictionTokens: Int
    var temperature: Double?
}

struct LocalLlamaModelDescriptor: Identifiable, Codable, Equatable, Sendable {
    var id: String
    var displayName: String
    var localizedDescription: [String: String]
    var tier: String
    var tasks: [String]
    var recommended: Bool
    var fileName: String
    var byteCount: Int64
    var sha256: String
    var contextLength: Int
    var chatTemplate: String
    var minimumMemoryGB: Int
    var recommendedConfig: [String: LocalLlamaRecommendedRuntimeConfig]
    var downloadSources: [LocalLlamaModelDownloadSource]

    enum CodingKeys: String, CodingKey {
        case id
        case displayName
        case localizedDescription = "description"
        case tier
        case tasks
        case recommended
        case fileName
        case byteCount
        case sha256
        case contextLength
        case chatTemplate
        case minimumMemoryGB
        case recommendedConfig
        case downloadSources
    }

    var formattedSize: String {
        ByteCountFormatter.string(fromByteCount: byteCount, countStyle: .file)
    }

    func detail(language: AppLanguage) -> String {
        localizedText(from: localizedDescription, language: language)
    }

    var preferredDownloadSources: [LocalLlamaModelDownloadSource] {
        preferredDownloadSources(for: .china)
    }

    func preferredDownloadSources(
        for route: LocalLlamaModelDownloadRoute
    ) -> [LocalLlamaModelDownloadSource] {
        downloadSources.sorted { lhs, rhs in
            sourcePriority(lhs, route: route) < sourcePriority(rhs, route: route)
        }
    }

    private func sourcePriority(
        _ source: LocalLlamaModelDownloadSource,
        route: LocalLlamaModelDownloadRoute
    ) -> Int {
        let region = source.region.lowercased()
        let id = source.id.lowercased()
        switch route {
        case .china:
            if region == "cn" || id.contains("modelscope") {
                return 0
            }
            if region == "global" || id.contains("huggingface") {
                return 1
            }
        case .international:
            if region == "global" || id.contains("huggingface") {
                return 0
            }
            if region == "cn" || id.contains("modelscope") {
                return 1
            }
        }
        return 2
    }
}

struct LocalLlamaServerModelDescriptor: Codable, Equatable, Sendable {
    var id: String
    var displayName: String
    var localizedDescription: [String: String]
    var tasks: [String]
    var recommendedConfig: [String: LocalLlamaRecommendedRuntimeConfig]
    var sourceURL: URL

    enum CodingKeys: String, CodingKey {
        case id
        case displayName
        case localizedDescription = "description"
        case tasks
        case recommendedConfig
        case sourceURL
    }
}

struct LocalLlamaModelManifest: Codable, Equatable, Sendable {
    var schemaVersion: Int
    var updatedAt: String
    var models: [LocalLlamaModelDescriptor]
    var serverModels: [LocalLlamaServerModelDescriptor]
}

enum LocalLlamaModelInstallState: Equatable, Sendable {
    case notInstalled
    case downloading(progress: Double)
    case installed
    case failed(String)

    var isDownloading: Bool {
        if case .downloading = self {
            return true
        }
        return false
    }
}

enum LocalLlamaModelCatalog {
    static let defaultModelID = "qwen2.5-1.5b-instruct-q4-k-m"
    static let remoteManifestURL = URL(
        string:
            "https://raw.githubusercontent.com/duxweb/codux/main/Sources/DmuxWorkspace/Resources/local-llama-models.json"
    )!

    static var models: [LocalLlamaModelDescriptor] {
        manifest().models
    }

    static func descriptor(id: String) -> LocalLlamaModelDescriptor? {
        descriptor(id: id, in: models)
    }

    static func descriptor(id: String, in models: [LocalLlamaModelDescriptor])
        -> LocalLlamaModelDescriptor?
    {
        models.first { $0.id == id }
    }

    static func descriptor(for configuration: AppAIProviderConfiguration)
        -> LocalLlamaModelDescriptor?
    {
        descriptor(id: normalizedNonEmptyString(configuration.model) ?? defaultModelID)
            ?? descriptor(id: defaultModelID)
    }

    static func manifest() -> LocalLlamaModelManifest {
        let bundled = bundledManifest()
        guard let cached = cachedManifest() else {
            return bundled
        }
        return preferredManifest(cached: cached, bundled: bundled)
    }

    static func bundledManifest() -> LocalLlamaModelManifest {
        guard let url = Bundle.module.url(
            forResource: "local-llama-models",
            withExtension: "json"
        ),
              let data = try? Data(contentsOf: url),
              let manifest = try? decodeManifest(data)
        else {
            return fallbackManifest()
        }
        return manifest
    }

    static func fetchRemoteManifest() async throws -> LocalLlamaModelManifest {
        var request = URLRequest(url: remoteManifestURL)
        request.cachePolicy = .reloadIgnoringLocalCacheData
        request.timeoutInterval = 10

        let (data, response) = try await URLSession.shared.data(for: request)
        if let httpResponse = response as? HTTPURLResponse,
           !(200..<300).contains(httpResponse.statusCode) {
            throw AIProviderError.requestFailure(
                "Model manifest request failed with HTTP \(httpResponse.statusCode).")
        }

        let manifest = try decodeManifest(data)
        try writeCachedManifest(data)
        return preferredManifest(cached: manifest, bundled: bundledManifest())
    }

    private static func cachedManifest() -> LocalLlamaModelManifest? {
        let url = cachedManifestURL()
        guard let data = try? Data(contentsOf: url),
              let manifest = try? decodeManifest(data)
        else {
            return nil
        }
        return manifest
    }

    private static func writeCachedManifest(_ data: Data) throws {
        let url = cachedManifestURL()
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        let marker = directory.appendingPathComponent(".metadata_never_index", isDirectory: false)
        if FileManager.default.fileExists(atPath: marker.path) == false {
            FileManager.default.createFile(atPath: marker.path, contents: Data())
        }
        try data.write(to: url, options: .atomic)
    }

    private static func cachedManifestURL() -> URL {
        LocalLlamaModelStore()
            .modelsDirectoryURL()
            .appendingPathComponent("local-llama-models.json", isDirectory: false)
    }

    private static func decodeManifest(_ data: Data) throws -> LocalLlamaModelManifest {
        let manifest = try JSONDecoder().decode(LocalLlamaModelManifest.self, from: data)
        guard manifest.schemaVersion == 1, manifest.models.isEmpty == false else {
            throw AIProviderError.requestFailure("Unsupported local llama model manifest.")
        }
        return manifest
    }

    private static func preferredManifest(
        cached: LocalLlamaModelManifest,
        bundled: LocalLlamaModelManifest
    ) -> LocalLlamaModelManifest {
        if cached.updatedAt > bundled.updatedAt {
            return cached
        }
        if cached.updatedAt < bundled.updatedAt {
            return bundled
        }
        return cached.models.count >= bundled.models.count ? cached : bundled
    }

    private static func fallbackManifest() -> LocalLlamaModelManifest {
        LocalLlamaModelManifest(
            schemaVersion: 1,
            updatedAt: "2026-05-06",
            models: [
                LocalLlamaModelDescriptor(
                    id: defaultModelID,
                    displayName: "Qwen2.5 1.5B Instruct Q4_K_M",
                    localizedDescription: [
                        "en": "Recommended default for local memory extraction.",
                        "zh-Hans": "本机记忆提取的推荐默认模型。",
                        "zh-Hant": "本機記憶擷取的推薦預設模型。",
                    ],
                    tier: "balanced",
                    tasks: ["memory", "pet", "assistant-lite"],
                    recommended: true,
                    fileName: "qwen2.5-1.5b-instruct-q4_k_m.gguf",
                    byteCount: 1_117_320_736,
                    sha256:
                        "6a1a2eb6d15622bf3c96857206351ba97e1af16c30d7a74ee38970e434e9407e",
                    contextLength: 4096,
                    chatTemplate: "qwen",
                    minimumMemoryGB: 8,
                    recommendedConfig: [
                        "memory": LocalLlamaRecommendedRuntimeConfig(
                            contextTokens: 4096,
                            maxPredictionTokens: 768,
                            temperature: 0.1
                        )
                    ],
                    downloadSources: [
                        LocalLlamaModelDownloadSource(
                            id: "modelscope",
                            region: "cn",
                            url: URL(
                                string:
                                    "https://www.modelscope.cn/models/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/master/qwen2.5-1.5b-instruct-q4_k_m.gguf"
                            )!
                        ),
                        LocalLlamaModelDownloadSource(
                            id: "huggingface",
                            region: "global",
                            url: URL(
                                string:
                                    "https://huggingface.co/Qwen/Qwen2.5-1.5B-Instruct-GGUF/resolve/main/qwen2.5-1.5b-instruct-q4_k_m.gguf"
                            )!
                        ),
                    ]
                )
            ],
            serverModels: []
        )
    }
}

struct LocalLlamaModelStore: Sendable {
    func installStates(for models: [LocalLlamaModelDescriptor] = LocalLlamaModelCatalog.models)
        -> [String: LocalLlamaModelInstallState]
    {
        Dictionary(
            uniqueKeysWithValues: models.map { descriptor in
                (descriptor.id, isInstalled(descriptor) ? .installed : .notInstalled)
            }
        )
    }

    func isInstalled(_ descriptor: LocalLlamaModelDescriptor) -> Bool {
        installedURL(for: descriptor) != nil
    }

    func installedModelURL(for descriptor: LocalLlamaModelDescriptor) throws -> URL {
        guard let url = installedURL(for: descriptor) else {
            throw AIProviderError.localModelNotInstalled(descriptor.displayName)
        }
        return url
    }

    private func installedURL(for descriptor: LocalLlamaModelDescriptor) -> URL? {
        for url in candidateModelURLs(for: descriptor) {
            if isValidInstalledFile(url, descriptor: descriptor) {
                return url
            }
        }
        return nil
    }

    private func isValidInstalledFile(_ url: URL, descriptor: LocalLlamaModelDescriptor) -> Bool {
        guard FileManager.default.fileExists(atPath: url.path) else {
            return false
        }
        let attributes = try? FileManager.default.attributesOfItem(atPath: url.path)
        return attributes?[.size] as? Int64 == descriptor.byteCount
    }

    func remove(_ descriptor: LocalLlamaModelDescriptor) throws {
        for url in candidateModelURLs(for: descriptor) {
            if FileManager.default.fileExists(atPath: url.path) {
                try FileManager.default.removeItem(at: url)
            }
        }
    }

    func install(
        _ descriptor: LocalLlamaModelDescriptor,
        downloadRoute: LocalLlamaModelDownloadRoute = .china,
        progress: @escaping @Sendable (Double) async -> Void
    ) async throws {
        let directory = modelsDirectoryURL()
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        writeNoIndexMarker(in: directory)

        let stagingURL = directory.appendingPathComponent(
            "\(descriptor.fileName).download", isDirectory: false)
        if FileManager.default.fileExists(atPath: stagingURL.path) {
            try FileManager.default.removeItem(at: stagingURL)
        }

        var lastError: Error?
        for source in descriptor.preferredDownloadSources(for: downloadRoute) {
            do {
                if FileManager.default.fileExists(atPath: stagingURL.path) {
                    try FileManager.default.removeItem(at: stagingURL)
                }

                try await LocalLlamaModelDownload.download(
                    from: source.url,
                    to: stagingURL,
                    expectedByteCount: descriptor.byteCount,
                    progress: progress
                )

                let actualHash = try sha256File(at: stagingURL)
                guard actualHash.caseInsensitiveCompare(descriptor.sha256) == .orderedSame else {
                    throw AIProviderError.requestFailure(
                        "Downloaded model checksum did not match \(descriptor.displayName).")
                }
                lastError = nil
                break
            } catch {
                try? FileManager.default.removeItem(at: stagingURL)
                lastError = error
            }
        }

        if let lastError {
            throw lastError
        }

        let finalURL = modelURL(for: descriptor)
        if FileManager.default.fileExists(atPath: finalURL.path) {
            try FileManager.default.removeItem(at: finalURL)
        }
        try FileManager.default.moveItem(at: stagingURL, to: finalURL)
        await progress(1)
    }

    func modelURL(for descriptor: LocalLlamaModelDescriptor) -> URL {
        modelsDirectoryURL().appendingPathComponent(descriptor.fileName, isDirectory: false)
    }

    func modelsDirectoryURL() -> URL {
        FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first!
            .appendingPathComponent("Codux", isDirectory: true)
            .appendingPathComponent("Models", isDirectory: true)
            .appendingPathComponent("llama", isDirectory: true)
    }

    private func candidateModelURLs(for descriptor: LocalLlamaModelDescriptor) -> [URL] {
        var urls = [modelURL(for: descriptor)]
        let appSupport = FileManager.default
            .urls(for: .applicationSupportDirectory, in: .userDomainMask)
            .first
        for folderName in ["Codux-dev", "Codux-debug"] {
            if let appSupport {
                urls.append(
                    appSupport
                        .appendingPathComponent(folderName, isDirectory: true)
                        .appendingPathComponent("Models", isDirectory: true)
                        .appendingPathComponent("llama", isDirectory: true)
                        .appendingPathComponent(descriptor.fileName, isDirectory: false)
                )
            }
        }
        return urls
    }

    private func writeNoIndexMarker(in directory: URL) {
        let marker = directory.appendingPathComponent(".metadata_never_index", isDirectory: false)
        if FileManager.default.fileExists(atPath: marker.path) == false {
            FileManager.default.createFile(atPath: marker.path, contents: Data())
        }
    }

    private func sha256File(at url: URL) throws -> String {
        let handle = try FileHandle(forReadingFrom: url)
        defer { try? handle.close() }

        var hasher = SHA256()
        while true {
            guard let data = try handle.read(upToCount: 1_048_576), !data.isEmpty else {
                break
            }
            hasher.update(data: data)
        }

        return hasher.finalize().map { String(format: "%02x", $0) }.joined()
    }
}

private func localizedText(from values: [String: String], language: AppLanguage) -> String {
    for key in localizationKeys(for: language.resolved) {
        if let value = normalizedNonEmptyString(values[key]) {
            return value
        }
    }
    return normalizedNonEmptyString(values["en"])
        ?? normalizedNonEmptyString(values["zh-Hans"])
        ?? values.values.first
        ?? ""
}

private func localizationKeys(for language: AppLanguage) -> [String] {
    switch language {
    case .system:
        return localizationKeys(for: language.resolved)
    case .simplifiedChinese:
        return ["zh-Hans", "zh-CN", "zh", "en"]
    case .traditionalChinese:
        return ["zh-Hant", "zh-TW", "zh-HK", "zh", "en"]
    case .english:
        return ["en"]
    case .japanese:
        return ["ja", "en"]
    case .korean:
        return ["ko", "en"]
    case .french:
        return ["fr", "en"]
    case .german:
        return ["de", "en"]
    case .spanish:
        return ["es", "en"]
    case .portugueseBrazil:
        return ["pt-BR", "pt", "en"]
    case .russian:
        return ["ru", "en"]
    }
}

private enum LocalLlamaModelDownload {
    static func download(
        from url: URL,
        to destinationURL: URL,
        expectedByteCount: Int64,
        progress: @escaping @Sendable (Double) async -> Void
    ) async throws {
        let delegate = LocalLlamaDownloadDelegate(
            destinationURL: destinationURL,
            expectedByteCount: expectedByteCount,
            progress: progress
        )
        let session = URLSession(configuration: .default, delegate: delegate, delegateQueue: nil)
        defer { session.invalidateAndCancel() }

        try await withCheckedThrowingContinuation { continuation in
            delegate.setContinuation(continuation)
            session.downloadTask(with: url).resume()
        }
    }
}

private final class LocalLlamaDownloadDelegate: NSObject, URLSessionDownloadDelegate,
    @unchecked Sendable
{
    private let destinationURL: URL
    private let expectedByteCount: Int64
    private let progress: @Sendable (Double) async -> Void
    private let lock = NSLock()
    private var continuation: CheckedContinuation<Void, Error>?
    private var movedDownload = false

    init(
        destinationURL: URL,
        expectedByteCount: Int64,
        progress: @escaping @Sendable (Double) async -> Void
    ) {
        self.destinationURL = destinationURL
        self.expectedByteCount = expectedByteCount
        self.progress = progress
    }

    func setContinuation(_ continuation: CheckedContinuation<Void, Error>) {
        lock.withLock {
            self.continuation = continuation
        }
    }

    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didWriteData bytesWritten: Int64,
        totalBytesWritten: Int64,
        totalBytesExpectedToWrite: Int64
    ) {
        _ = bytesWritten
        let expected =
            totalBytesExpectedToWrite > 0 ? totalBytesExpectedToWrite : expectedByteCount
        guard expected > 0 else {
            return
        }
        let value = max(0, min(0.99, Double(totalBytesWritten) / Double(expected)))
        Task {
            await progress(value)
        }
    }

    func urlSession(
        _ session: URLSession,
        downloadTask: URLSessionDownloadTask,
        didFinishDownloadingTo location: URL
    ) {
        _ = session
        _ = downloadTask
        do {
            if FileManager.default.fileExists(atPath: destinationURL.path) {
                try FileManager.default.removeItem(at: destinationURL)
            }
            try FileManager.default.moveItem(at: location, to: destinationURL)
            lock.withLock {
                movedDownload = true
            }
        } catch {
            resume(.failure(error))
        }
    }

    func urlSession(
        _ session: URLSession,
        task: URLSessionTask,
        didCompleteWithError error: Error?
    ) {
        _ = session
        _ = task
        if let error {
            resume(.failure(error))
            return
        }
        let didMove = lock.withLock { movedDownload }
        guard didMove else {
            resume(.failure(AIProviderError.requestFailure("Model download did not finish.")))
            return
        }
        resume(.success(()))
    }

    private func resume(_ result: Result<Void, Error>) {
        let current = lock.withLock { () -> CheckedContinuation<Void, Error>? in
            let value = continuation
            continuation = nil
            return value
        }
        guard let current else {
            return
        }
        switch result {
        case .success:
            current.resume()
        case .failure(let error):
            current.resume(throwing: error)
        }
    }
}

private extension NSLock {
    func withLock<T>(_ work: () -> T) -> T {
        lock()
        defer { unlock() }
        return work()
    }
}
