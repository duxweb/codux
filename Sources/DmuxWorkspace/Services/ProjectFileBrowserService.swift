import AppKit
import Foundation
import UniformTypeIdentifiers

enum ProjectFileOpenMode {
    case codePreview
    case systemApplication
}

struct ProjectFileBrowserService {
    private let fileManager: FileManager
    private let maxPreviewBytes: UInt64
    typealias ConflictResolver = (_ sourceURL: URL, _ destinationURL: URL, _ suggestedName: String) -> String?

    init(fileManager: FileManager = .default, maxPreviewBytes: UInt64 = 1_500_000) {
        self.fileManager = fileManager
        self.maxPreviewBytes = maxPreviewBytes
    }

    func rootItem(for project: Project) -> ProjectFileItem {
        let url = URL(fileURLWithPath: project.path, isDirectory: true).standardizedFileURL
        return ProjectFileItem(
            url: url,
            name: project.name.isEmpty ? url.lastPathComponent : project.name,
            relativePath: "",
            isDirectory: true,
            isSymbolicLink: false
        )
    }

    func children(of item: ProjectFileItem, rootURL: URL) throws -> [ProjectFileItem] {
        guard item.isDirectory else {
            return []
        }
        let keys: Set<URLResourceKey> = [
            .isDirectoryKey,
            .isSymbolicLinkKey,
            .nameKey,
            .isPackageKey,
        ]
        let urls = try fileManager.contentsOfDirectory(
            at: item.url,
            includingPropertiesForKeys: Array(keys),
            options: [.skipsSubdirectoryDescendants]
        )

        return urls.compactMap { url in
            guard let values = try? url.resourceValues(forKeys: keys) else {
                return nil
            }
            let name = values.name ?? url.lastPathComponent
            let isDirectory = values.isDirectory == true && values.isPackage != true
            return ProjectFileItem(
                url: url.standardizedFileURL,
                name: name,
                relativePath: relativePath(for: url, rootURL: rootURL),
                isDirectory: isDirectory,
                isSymbolicLink: values.isSymbolicLink == true
            )
        }
        .sorted { lhs, rhs in
            if lhs.isDirectory != rhs.isDirectory {
                return lhs.isDirectory && !rhs.isDirectory
            }
            return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
        }
    }

    func fileURLsFromPasteboard(_ pasteboard: NSPasteboard = .general) -> [URL] {
        let options: [NSPasteboard.ReadingOptionKey: Any] = [
            .urlReadingFileURLsOnly: true,
        ]
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: options) as? [URL] {
            return urls.map { $0.standardizedFileURL }
        }
        return []
    }

    func copyItems(
        _ sourceURLs: [URL],
        to targetDirectory: URL,
        conflictResolver: ConflictResolver
    ) throws -> [URL] {
        try transferItems(
            sourceURLs,
            to: targetDirectory,
            mode: .copy,
            conflictResolver: conflictResolver
        )
    }

    func moveItems(
        _ sourceURLs: [URL],
        to targetDirectory: URL,
        conflictResolver: ConflictResolver
    ) throws -> [URL] {
        try transferItems(
            sourceURLs,
            to: targetDirectory,
            mode: .move,
            conflictResolver: conflictResolver
        )
    }

    func renameItem(at sourceURL: URL, to newName: String) throws -> URL {
        let trimmedName = newName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmedName.isEmpty == false, trimmedName.contains("/") == false else {
            throw NSError(
                domain: "CoduxFileBrowser",
                code: 400,
                userInfo: [NSLocalizedDescriptionKey: String(localized: "files.panel.rename.invalid", defaultValue: "Enter a valid file name.", bundle: .module)]
            )
        }

        let source = sourceURL.standardizedFileURL
        let destination = source.deletingLastPathComponent().appendingPathComponent(trimmedName)
        guard source.path != destination.standardizedFileURL.path else {
            return source
        }
        guard fileManager.fileExists(atPath: destination.path) == false else {
            throw NSError(
                domain: "CoduxFileBrowser",
                code: 409,
                userInfo: [NSLocalizedDescriptionKey: String(localized: "files.panel.rename.exists", defaultValue: "A file with this name already exists.", bundle: .module)]
            )
        }
        try fileManager.moveItem(at: source, to: destination)
        return destination.standardizedFileURL
    }

    func preview(for fileURL: URL, rootURL: URL) -> ProjectFilePreview {
        let standardizedURL = fileURL.standardizedFileURL
        let title = standardizedURL.lastPathComponent
        let subtitle = relativePath(for: standardizedURL, rootURL: rootURL)
        let keys: Set<URLResourceKey> = [.isDirectoryKey, .fileSizeKey]

        guard let values = try? standardizedURL.resourceValues(forKeys: keys),
              values.isDirectory != true else {
            return ProjectFilePreview(
                title: title,
                subtitle: subtitle,
                state: .message(String(localized: "files.preview.directory", defaultValue: "Directories are shown in the file sidebar.", bundle: .module))
            )
        }

        let byteCount = UInt64(values.fileSize ?? 0)
        guard byteCount <= maxPreviewBytes else {
            return ProjectFilePreview(
                title: title,
                subtitle: subtitle,
                state: .message(
                    String(
                        format: String(localized: "files.preview.too_large_format", defaultValue: "This file is too large to preview safely (%@).", bundle: .module),
                        ByteCountFormatter.string(fromByteCount: Int64(byteCount), countStyle: .file)
                    )
                )
            )
        }

        guard let data = try? Data(contentsOf: standardizedURL) else {
            return ProjectFilePreview(
                title: title,
                subtitle: subtitle,
                state: .message(String(localized: "files.preview.read_error", defaultValue: "Could not read this file.", bundle: .module))
            )
        }

        guard data.isEmpty == false else {
            return ProjectFilePreview(
                title: title,
                subtitle: subtitle,
                state: .message(String(localized: "files.preview.empty", defaultValue: "This file is empty.", bundle: .module))
            )
        }

        guard data.contains(0) == false,
              let text = String(data: data, encoding: .utf8) ?? String(data: data, encoding: .utf16) else {
            return ProjectFilePreview(
                title: title,
                subtitle: subtitle,
                state: .message(String(localized: "files.preview.binary", defaultValue: "Binary files cannot be previewed here.", bundle: .module))
            )
        }

        return ProjectFilePreview(
            title: title,
            subtitle: subtitle,
            state: .text(ProjectFileSyntaxHighlighter.highlight(text: text, fileExtension: standardizedURL.pathExtension))
        )
    }

    func relativePathForDisplay(url: URL, rootURL: URL) -> String {
        relativePath(for: url, rootURL: rootURL)
    }

    func openMode(for fileURL: URL) -> ProjectFileOpenMode {
        let ext = fileURL.pathExtension.lowercased()
        if Self.systemApplicationExtensions.contains(ext) {
            return .systemApplication
        }
        guard let type = UTType(filenameExtension: ext) else {
            return .codePreview
        }
        if type.conforms(to: .image) ||
            type.conforms(to: .movie) ||
            type.conforms(to: .audiovisualContent) ||
            type.conforms(to: .presentation) ||
            type.conforms(to: .spreadsheet) {
            return .systemApplication
        }
        return .codePreview
    }

    private func relativePath(for url: URL, rootURL: URL) -> String {
        let rootPath = rootURL.standardizedFileURL.path
        let path = url.standardizedFileURL.path
        guard path.hasPrefix(rootPath) else {
            return url.lastPathComponent
        }
        let index = path.index(path.startIndex, offsetBy: rootPath.count)
        let suffix = path[index...].trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        return suffix.isEmpty ? url.lastPathComponent : suffix
    }

    private enum TransferMode {
        case copy
        case move
    }

    private func transferItems(
        _ sourceURLs: [URL],
        to targetDirectory: URL,
        mode: TransferMode,
        conflictResolver: ConflictResolver
    ) throws -> [URL] {
        let directory = targetDirectory.standardizedFileURL
        let values = try directory.resourceValues(forKeys: [.isDirectoryKey])
        guard values.isDirectory == true else {
            throw NSError(
                domain: "CoduxFileBrowser",
                code: 400,
                userInfo: [NSLocalizedDescriptionKey: String(localized: "files.panel.target_not_directory", defaultValue: "The drop target is not a folder.", bundle: .module)]
            )
        }

        var results: [URL] = []
        for rawSourceURL in sourceURLs {
            let sourceURL = rawSourceURL.standardizedFileURL
            guard fileManager.fileExists(atPath: sourceURL.path) else { continue }
            if mode == .move, isSameFile(sourceURL, directory) {
                continue
            }
            if mode == .move, isDirectory(sourceURL), directory.path.hasPrefix(sourceURL.path + "/") {
                throw NSError(
                    domain: "CoduxFileBrowser",
                    code: 409,
                    userInfo: [NSLocalizedDescriptionKey: String(localized: "files.panel.move_into_self", defaultValue: "A folder cannot be moved into itself.", bundle: .module)]
                )
            }

            guard let destinationURL = resolvedDestinationURL(
                sourceURL: sourceURL,
                targetDirectory: directory,
                conflictResolver: conflictResolver
            ) else {
                continue
            }
            if isSameFile(sourceURL, destinationURL) {
                continue
            }
            switch mode {
            case .copy:
                try fileManager.copyItem(at: sourceURL, to: destinationURL)
            case .move:
                try fileManager.moveItem(at: sourceURL, to: destinationURL)
            }
            results.append(destinationURL.standardizedFileURL)
        }
        return results
    }

    private func resolvedDestinationURL(
        sourceURL: URL,
        targetDirectory: URL,
        conflictResolver: ConflictResolver
    ) -> URL? {
        let defaultDestination = targetDirectory.appendingPathComponent(sourceURL.lastPathComponent)
        guard fileManager.fileExists(atPath: defaultDestination.path) else {
            return defaultDestination
        }

        let suggestedName = availableCopyName(for: sourceURL.lastPathComponent, in: targetDirectory)
        guard let replacementName = conflictResolver(sourceURL, defaultDestination, suggestedName)?
            .trimmingCharacters(in: .whitespacesAndNewlines),
              replacementName.isEmpty == false,
              replacementName.contains("/") == false else {
            return nil
        }
        let candidate = targetDirectory.appendingPathComponent(replacementName)
        guard fileManager.fileExists(atPath: candidate.path) == false else {
            return resolvedDestinationURL(
                sourceURL: URL(fileURLWithPath: replacementName),
                targetDirectory: targetDirectory,
                conflictResolver: conflictResolver
            )
        }
        return candidate
    }

    private func availableCopyName(for fileName: String, in directory: URL) -> String {
        let url = URL(fileURLWithPath: fileName)
        let base = url.deletingPathExtension().lastPathComponent
        let ext = url.pathExtension
        var index = 1
        while true {
            let suffix = index == 1 ? " copy" : " copy \(index)"
            let candidate = ext.isEmpty ? "\(base)\(suffix)" : "\(base)\(suffix).\(ext)"
            if fileManager.fileExists(atPath: directory.appendingPathComponent(candidate).path) == false {
                return candidate
            }
            index += 1
        }
    }

    private func isDirectory(_ url: URL) -> Bool {
        (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
    }

    private func isSameFile(_ lhs: URL, _ rhs: URL) -> Bool {
        lhs.standardizedFileURL.path == rhs.standardizedFileURL.path
    }

    private static let systemApplicationExtensions: Set<String> = [
        "apng", "avif", "bmp", "gif", "heic", "heif", "ico", "jpeg", "jpg", "png", "psd", "svg", "tif", "tiff", "webp",
        "3g2", "3gp", "avi", "m4v", "mkv", "mov", "mp4", "mpeg", "mpg", "webm", "wmv",
        "aac", "aiff", "flac", "m4a", "mp3", "ogg", "wav",
        "doc", "docx", "key", "numbers", "pages", "pdf", "ppt", "pptx", "xls", "xlsx",
    ]
}

enum ProjectFileSyntaxHighlighter {
    static func highlight(text: String, fileExtension: String) -> NSAttributedString {
        let attributed = NSMutableAttributedString(
            string: text,
            attributes: [
                .font: NSFont.monospacedSystemFont(ofSize: 12, weight: .regular),
                .foregroundColor: NSColor.labelColor,
            ]
        )
        let fullRange = NSRange(location: 0, length: attributed.length)
        let normalizedExtension = fileExtension.lowercased()

        apply(pattern: #"(?m)//.*$|#.*$"#, to: attributed, range: fullRange, color: .secondaryLabelColor)
        apply(pattern: #""(?:\\.|[^"\\])*"|'(?:\\.|[^'\\])*'"#, to: attributed, range: fullRange, color: NSColor.systemGreen)
        apply(pattern: #"\b\d+(?:\.\d+)?\b"#, to: attributed, range: fullRange, color: NSColor.systemOrange)

        let keywords = keywords(for: normalizedExtension)
        if keywords.isEmpty == false {
            let pattern = "\\b(" + keywords.map(NSRegularExpression.escapedPattern(for:)).joined(separator: "|") + ")\\b"
            apply(pattern: pattern, to: attributed, range: fullRange, color: NSColor.systemBlue, fontWeight: .semibold)
        }

        return attributed
    }

    private static func keywords(for fileExtension: String) -> [String] {
        switch fileExtension {
        case "swift":
            return ["actor", "class", "enum", "extension", "func", "import", "let", "private", "protocol", "return", "static", "struct", "var"]
        case "js", "jsx", "ts", "tsx":
            return ["async", "await", "class", "const", "export", "from", "function", "import", "interface", "let", "return", "type"]
        case "php":
            return ["class", "echo", "extends", "function", "namespace", "private", "protected", "public", "return", "use"]
        case "rb":
            return ["class", "def", "do", "end", "module", "private", "require", "return"]
        case "sh", "bash", "zsh":
            return ["case", "do", "done", "elif", "else", "esac", "fi", "for", "function", "if", "in", "then", "while"]
        default:
            return []
        }
    }

    private static func apply(
        pattern: String,
        to attributed: NSMutableAttributedString,
        range: NSRange,
        color: NSColor,
        fontWeight: NSFont.Weight? = nil
    ) {
        guard let regex = try? NSRegularExpression(pattern: pattern) else {
            return
        }
        let matches = regex.matches(in: attributed.string, range: range)
        for match in matches {
            attributed.addAttribute(.foregroundColor, value: color, range: match.range)
            if let fontWeight {
                attributed.addAttribute(.font, value: NSFont.monospacedSystemFont(ofSize: 12, weight: fontWeight), range: match.range)
            }
        }
    }
}
