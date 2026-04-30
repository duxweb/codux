import AppKit
import SwiftUI

@MainActor
@Observable
final class ProjectFileBrowserStore {
    var rootItem: ProjectFileItem?
    var childrenByPath: [String: [ProjectFileItem]] = [:]
    var expandedPaths: Set<String> = []
    var loadingPaths: Set<String> = []
    var selectedPath: String?
    var errorMessage: String?

    private let service: ProjectFileBrowserService
    private var rootURL: URL?
    private var loadedProjectID: UUID?

    init(service: ProjectFileBrowserService = ProjectFileBrowserService()) {
        self.service = service
    }

    var visibleRows: [ProjectFileRow] {
        guard let rootItem else {
            return []
        }
        return flattenedChildren(of: rootItem, depth: 0)
    }

    func load(project: Project?) {
        guard let project else {
            rootItem = nil
            rootURL = nil
            loadedProjectID = nil
            childrenByPath.removeAll()
            expandedPaths.removeAll()
            selectedPath = nil
            errorMessage = nil
            return
        }
        guard loadedProjectID != project.id else {
            return
        }
        let root = service.rootItem(for: project)
        rootItem = root
        rootURL = root.url
        loadedProjectID = project.id
        childrenByPath.removeAll()
        expandedPaths = [root.id]
        selectedPath = nil
        errorMessage = nil
        loadChildren(for: root)
    }

    func refresh() {
        guard let rootItem else {
            return
        }
        let rememberedExpanded = expandedPaths
        childrenByPath.removeAll()
        expandedPaths = rememberedExpanded.union([rootItem.id])
        loadChildren(for: rootItem)
        for path in rememberedExpanded where path != rootItem.id {
            if let item = findItem(withID: path) {
                loadChildren(for: item)
            }
        }
    }

    func toggle(_ item: ProjectFileItem) {
        guard item.isDirectory else { return }
        selectedPath = item.id
        if expandedPaths.contains(item.id) {
            withAnimation(.easeOut(duration: 0.16)) {
                _ = expandedPaths.remove(item.id)
            }
        } else {
            withAnimation(.easeOut(duration: 0.16)) {
                _ = expandedPaths.insert(item.id)
            }
            loadChildren(for: item)
        }
    }

    func select(_ item: ProjectFileItem) {
        selectedPath = item.id
    }

    func openPreview(_ item: ProjectFileItem) {
        guard item.isDirectory == false else { return }
        selectedPath = item.id
        ProjectFilePreviewWindowPresenter.show(fileURL: item.url, rootURL: rootURL ?? item.url.deletingLastPathComponent())
    }

    func edit(_ item: ProjectFileItem) {
        guard item.isDirectory == false else { return }
        selectedPath = item.id
        ProjectFilePreviewWindowPresenter.show(
            fileURL: item.url,
            rootURL: rootURL ?? item.url.deletingLastPathComponent(),
            startsEditing: true
        )
    }

    func copyPath(_ item: ProjectFileItem) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(item.url.standardizedFileURL.path, forType: .string)
    }

    func revealInFinder(_ item: ProjectFileItem) {
        NSWorkspace.shared.activateFileViewerSelecting([item.url])
    }

    func moveToTrash(_ item: ProjectFileItem) {
        selectedPath = item.id
        NSWorkspace.shared.recycle([item.url]) { [weak self] _, error in
            Task { @MainActor in
                guard let self else { return }
                if let error {
                    self.errorMessage = String(
                        format: String(localized: "files.panel.delete.failure_format", defaultValue: "Could not move to Trash: %@", bundle: .module),
                        error.localizedDescription
                    )
                    return
                }
                self.selectedPath = nil
                self.expandedPaths.remove(item.id)
                self.loadingPaths.remove(item.id)
                self.childrenByPath.removeValue(forKey: item.id)
                self.refresh()
            }
        }
    }

    private func loadChildren(for item: ProjectFileItem) {
        guard item.isDirectory,
              childrenByPath[item.id] == nil,
              loadingPaths.contains(item.id) == false,
              let rootURL else {
            return
        }
        loadingPaths.insert(item.id)
        do {
            childrenByPath[item.id] = try service.children(of: item, rootURL: rootURL)
            loadingPaths.remove(item.id)
        } catch {
            loadingPaths.remove(item.id)
            childrenByPath[item.id] = []
            errorMessage = error.localizedDescription
        }
    }

    private func flattenedChildren(of parent: ProjectFileItem, depth: Int) -> [ProjectFileRow] {
        guard let children = childrenByPath[parent.id] else {
            return []
        }
        var rows: [ProjectFileRow] = []
        for child in children {
            rows.append(ProjectFileRow(item: child, depth: depth))
            if child.isDirectory && expandedPaths.contains(child.id) {
                rows.append(contentsOf: flattenedChildren(of: child, depth: depth + 1))
            }
        }
        return rows
    }

    private func findItem(withID id: String) -> ProjectFileItem? {
        if rootItem?.id == id {
            return rootItem
        }
        return childrenByPath.values.flatMap { $0 }.first { $0.id == id }
    }
}

struct FileBrowserPanelView: View {
    let model: AppModel
    @State private var store = ProjectFileBrowserStore()

    var body: some View {
        VStack(spacing: 0) {
            header
            GitPanelSeparator()
            content
        }
        .background(Color.clear)
        .onAppear {
            store.load(project: model.selectedProject)
        }
        .onChange(of: model.selectedProjectID) { _, _ in
            store.load(project: model.selectedProject)
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: "folder")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(AppTheme.textSecondary)
            VStack(alignment: .leading, spacing: 2) {
                Text(String(localized: "files.panel.title", defaultValue: "Files", bundle: .module))
                    .font(.system(size: 13, weight: .bold))
                    .foregroundStyle(AppTheme.textPrimary)
                Text(model.selectedProject?.name ?? String(localized: "files.panel.no_project", defaultValue: "No Project Selected", bundle: .module))
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
            }
            Spacer(minLength: 0)
            Button {
                store.refresh()
            } label: {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(GitToolbarIconButtonStyle())
            .help(String(localized: "files.panel.refresh", defaultValue: "Refresh Files", bundle: .module))
            .disabled(model.selectedProject == nil)
        }
        .padding(.horizontal, 16)
        .frame(height: 48)
    }

    @ViewBuilder
    private var content: some View {
        if model.selectedProject == nil {
            FileBrowserEmptyView(
                symbol: "folder.badge.questionmark",
                title: String(localized: "files.panel.no_project", defaultValue: "No Project Selected", bundle: .module),
                message: String(localized: "files.panel.no_project.help", defaultValue: "Select or add a project to browse its files.", bundle: .module)
            )
        } else if store.visibleRows.isEmpty {
            FileBrowserEmptyView(
                symbol: "folder",
                title: String(localized: "files.panel.empty", defaultValue: "No Files", bundle: .module),
                message: store.errorMessage ?? String(localized: "files.panel.empty.help", defaultValue: "This project folder has no visible files.", bundle: .module)
            )
        } else {
            ScrollView {
                LazyVStack(spacing: 0) {
                    ForEach(store.visibleRows) { row in
                        FileBrowserRowView(
                            row: row,
                            isExpanded: store.expandedPaths.contains(row.item.id),
                            isLoading: store.loadingPaths.contains(row.item.id),
                            isSelected: store.selectedPath == row.item.id,
                            select: { store.select(row.item) },
                            toggle: { store.toggle(row.item) },
                            openPreview: { store.openPreview(row.item) },
                            edit: { store.edit(row.item) },
                            insertPathIntoTerminal: { model.insertPathIntoCurrentTerminal(row.item.url) },
                            copyPath: { store.copyPath(row.item) },
                            delete: { store.moveToTrash(row.item) },
                            reveal: { store.revealInFinder(row.item) }
                        )
                    }
                }
                .padding(.vertical, 8)
                .animation(.easeOut(duration: 0.16), value: store.visibleRows.map(\.id))
            }
        }
    }
}

private struct FileBrowserRowView: View {
    let row: ProjectFileRow
    let isExpanded: Bool
    let isLoading: Bool
    let isSelected: Bool
    let select: () -> Void
    let toggle: () -> Void
    let openPreview: () -> Void
    let edit: () -> Void
    let insertPathIntoTerminal: () -> Void
    let copyPath: () -> Void
    let delete: () -> Void
    let reveal: () -> Void
    @State private var isHovered = false

    var body: some View {
        interactiveContent
            .contextMenu {
                Button(String(localized: "files.panel.open", defaultValue: "Open", bundle: .module), action: openPreview)
                    .disabled(row.item.isDirectory)
                Button(String(localized: "files.panel.edit", defaultValue: "Edit", bundle: .module), action: edit)
                    .disabled(row.item.isDirectory)
                Button(String(localized: "files.panel.insert_path_terminal", defaultValue: "Insert Path into Terminal", bundle: .module), action: insertPathIntoTerminal)
                Button(String(localized: "files.panel.copy_path", defaultValue: "Copy Path", bundle: .module), action: copyPath)
                Button(String(localized: "files.panel.reveal_finder", defaultValue: "Reveal in Finder", bundle: .module), action: reveal)
                Divider()
                Button(role: .destructive, action: delete) {
                    Text(String(localized: "files.panel.delete", defaultValue: "Move to Trash", bundle: .module))
                }
            }
            .help(row.item.relativePath.isEmpty ? row.item.name : row.item.relativePath)
    }

    @ViewBuilder
    private var interactiveContent: some View {
        if row.item.isDirectory {
            rowContent
                .onTapGesture {
                    toggle()
                }
        } else {
            rowContent
                .onTapGesture {
                    select()
                }
                .simultaneousGesture(
                    TapGesture(count: 2).onEnded {
                        openPreview()
                    }
                )
        }
    }

    private var rowContent: some View {
        HStack(spacing: 6) {
            Spacer()
                .frame(width: CGFloat(row.depth) * 14)

            disclosureView

            Image(systemName: row.item.isDirectory ? "folder" : iconName)
                .font(.system(size: 13, weight: .medium))
                .foregroundStyle(row.item.isDirectory ? AppTheme.focus : AppTheme.textSecondary)
                .frame(width: 16)

            Text(row.item.name)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .truncationMode(.middle)

            if row.item.isSymbolicLink {
                Image(systemName: "arrowshape.turn.up.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(AppTheme.textMuted)
            }
        }
        .padding(.leading, 12)
        .padding(.trailing, 10)
        .frame(height: 28)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(rowBackground)
        .contentShape(Rectangle())
        .onHover { hovering in
            isHovered = hovering
        }
    }

    @ViewBuilder
    private var disclosureView: some View {
        if row.item.isDirectory {
            Image(systemName: isLoading ? "hourglass" : "chevron.right")
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)
                .frame(width: 12)
                .rotationEffect(isExpanded && isLoading == false ? .degrees(90) : .zero)
                .animation(.easeOut(duration: 0.16), value: isExpanded)
        } else {
            Color.clear
                .frame(width: 12)
        }
    }

    private var iconName: String {
        switch row.item.url.pathExtension.lowercased() {
        case "swift", "js", "jsx", "ts", "tsx", "php", "rb", "py", "sh", "zsh", "bash":
            return "curlybraces"
        case "json", "toml", "yaml", "yml", "xml":
            return "doc.text"
        case "md", "txt", "log":
            return "doc.plaintext"
        case "png", "jpg", "jpeg", "gif", "webp", "heic":
            return "photo"
        default:
            return "doc"
        }
    }

    private var rowBackground: some View {
        RoundedRectangle(cornerRadius: 5, style: .continuous)
            .fill(
                isSelected
                    ? AppTheme.focus.opacity(0.16)
                    : (isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)
            )
            .padding(.horizontal, 8)
    }
}

private struct FileBrowserEmptyView: View {
    let symbol: String
    let title: String
    let message: String

    var body: some View {
        VStack(spacing: 12) {
            Image(systemName: symbol)
                .font(.system(size: 30, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)
            Text(title)
                .font(.system(size: 15, weight: .bold))
                .foregroundStyle(AppTheme.textPrimary)
            Text(message)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(AppTheme.textMuted)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 260)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(24)
    }
}
