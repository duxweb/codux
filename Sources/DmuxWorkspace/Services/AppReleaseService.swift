import Foundation

struct AppReleaseInfo {
    struct Asset: Decodable {
        let name: String
        let browserDownloadURL: URL

        private enum CodingKeys: String, CodingKey {
            case name
            case browserDownloadURL = "browser_download_url"
        }
    }

    let version: String
    let htmlURL: URL
    let body: String
    let assets: [Asset]
}

enum AppReleaseCheckResult {
    case upToDate(currentVersion: String, latestVersion: String)
    case updateAvailable(currentVersion: String, latest: AppReleaseInfo)
}

enum AppReleaseService {
    private static let latestReleaseURL = URL(string: "https://api.github.com/repos/duxweb/codux/releases/latest")!

    static func checkForUpdates(currentVersion: String) async throws -> AppReleaseCheckResult {
        var request = URLRequest(url: latestReleaseURL)
        request.timeoutInterval = 15
        request.setValue("application/vnd.github+json", forHTTPHeaderField: "Accept")
        request.setValue("codux", forHTTPHeaderField: "User-Agent")

        let (data, response) = try await URLSession.shared.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse, (200 ..< 300).contains(httpResponse.statusCode) else {
            throw NSError(domain: "codux.update", code: 1, userInfo: [
                NSLocalizedDescriptionKey: "Unable to fetch the latest release information."
            ])
        }

        let decoded = try JSONDecoder().decode(GitHubLatestReleaseResponse.self, from: data)
        let latest = AppReleaseInfo(
            version: normalizedVersion(decoded.tagName),
            htmlURL: decoded.htmlURL,
            body: decoded.body.trimmingCharacters(in: .whitespacesAndNewlines),
            assets: decoded.assets.map { asset in
                AppReleaseInfo.Asset(name: asset.name, browserDownloadURL: asset.browserDownloadURL)
            }
        )

        let normalizedCurrentVersion = normalizedVersion(currentVersion)
        if compareVersion(normalizedCurrentVersion, latest.version) == .orderedAscending {
            return .updateAvailable(currentVersion: normalizedCurrentVersion, latest: latest)
        }
        return .upToDate(currentVersion: normalizedCurrentVersion, latestVersion: latest.version)
    }

    static func preferredDownloadURL(for release: AppReleaseInfo) -> URL {
        if let dmg = release.assets.first(where: { $0.name.hasSuffix(".dmg") && $0.name.contains("macos-universal") }) {
            return dmg.browserDownloadURL
        }
        if let dmg = release.assets.first(where: { $0.name.hasSuffix(".dmg") }) {
            return dmg.browserDownloadURL
        }
        if let zip = release.assets.first(where: { $0.name.hasSuffix(".zip") }) {
            return zip.browserDownloadURL
        }
        return release.htmlURL
    }

    static func releaseNotesExcerpt(from body: String, limit: Int = 280) -> String? {
        let cleaned = body
            .replacingOccurrences(of: "\r\n", with: "\n")
            .replacingOccurrences(of: "\n\n\n", with: "\n\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleaned.isEmpty else {
            return nil
        }
        if cleaned.count <= limit {
            return cleaned
        }
        let endIndex = cleaned.index(cleaned.startIndex, offsetBy: limit)
        return String(cleaned[..<endIndex]).trimmingCharacters(in: .whitespacesAndNewlines) + "..."
    }

    private static func normalizedVersion(_ version: String) -> String {
        let trimmed = version.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.hasPrefix("v") || trimmed.hasPrefix("V") {
            return String(trimmed.dropFirst())
        }
        return trimmed
    }

    private static func compareVersion(_ lhs: String, _ rhs: String) -> ComparisonResult {
        let lhsComponents = versionComponents(from: lhs)
        let rhsComponents = versionComponents(from: rhs)
        let maxCount = max(lhsComponents.count, rhsComponents.count)

        for index in 0 ..< maxCount {
            let lhsValue = index < lhsComponents.count ? lhsComponents[index] : 0
            let rhsValue = index < rhsComponents.count ? rhsComponents[index] : 0
            if lhsValue < rhsValue {
                return .orderedAscending
            }
            if lhsValue > rhsValue {
                return .orderedDescending
            }
        }
        return .orderedSame
    }

    private static func versionComponents(from version: String) -> [Int] {
        version
            .split(whereSeparator: { !$0.isNumber })
            .compactMap { Int($0) }
    }
}

private struct GitHubLatestReleaseResponse: Decodable {
    struct Asset: Decodable {
        let name: String
        let browserDownloadURL: URL

        private enum CodingKeys: String, CodingKey {
            case name
            case browserDownloadURL = "browser_download_url"
        }
    }

    let tagName: String
    let htmlURL: URL
    let body: String
    let assets: [Asset]

    private enum CodingKeys: String, CodingKey {
        case tagName = "tag_name"
        case htmlURL = "html_url"
        case body
        case assets
    }
}
