import Foundation

struct ProjectFileItem: Identifiable, Equatable, Hashable {
    var id: String { url.path }
    var url: URL
    var name: String
    var relativePath: String
    var isDirectory: Bool
    var isSymbolicLink: Bool
}

struct ProjectFileRow: Identifiable, Equatable {
    var id: String { item.id }
    var item: ProjectFileItem
    var depth: Int
}

enum ProjectFilePreviewState: Equatable {
    case text(NSAttributedString)
    case message(String)
}

struct ProjectFilePreview {
    var title: String
    var subtitle: String
    var state: ProjectFilePreviewState
}
