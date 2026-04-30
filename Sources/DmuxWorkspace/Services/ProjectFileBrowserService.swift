import AppKit
import Foundation

struct ProjectFileBrowserService {
    private let fileManager: FileManager
    private let maxPreviewBytes: UInt64

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
