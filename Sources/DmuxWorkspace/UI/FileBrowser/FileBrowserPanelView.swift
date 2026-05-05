import AppKit
import SwiftUI
import UniformTypeIdentifiers

private enum FileBrowserPasteboard {
    static let cutType = NSPasteboard.PasteboardType("com.duxweb.codux.file-cut")

    static var containsCutMarker: Bool {
        NSPasteboard.general.propertyList(forType: cutType) != nil
    }

    static func writeFile(_ item: ProjectFileItem, mode: TransferMode) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.writeObjects([item.url.standardizedFileURL as NSURL])
        if mode == .cut {
            NSPasteboard.general.setPropertyList(
                [item.url.standardizedFileURL.path],
                forType: cutType
            )
        }
    }

    enum TransferMode {
        case copy
        case cut
    }
}

@MainActor
@Observable
final class ProjectFileBrowserStore {
    var rootItem: ProjectFileItem?
    var childrenByPath: [String: [ProjectFileItem]] = [:]
    var expandedPaths: Set<String> = []
    var loadingPaths: Set<String> = []
    var selectedPath: String?
    var renamingPath: String?
    var renamingName = ""
    var renamingFocusToken = 0
    var pendingDeletePaths: Set<String> = []
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
            renamingPath = nil
            renamingName = ""
            renamingFocusToken = 0
            pendingDeletePaths.removeAll()
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
        renamingPath = nil
        renamingName = ""
        renamingFocusToken = 0
        pendingDeletePaths.removeAll()
        errorMessage = nil
        loadChildren(for: root)
    }

    func refresh() {
        guard let rootItem else {
            return
        }
        pendingDeletePaths = pendingDeletePaths.filter { findItem(withID: $0) != nil }
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

    var pendingDeleteCount: Int {
        pendingDeletePaths.count
    }

    func openPreview(_ item: ProjectFileItem) {
        guard item.isDirectory == false else { return }
        selectedPath = item.id
        switch service.openMode(for: item.url) {
        case .codePreview:
            ProjectFilePreviewWindowPresenter.show(fileURL: item.url, rootURL: rootURL ?? item.url.deletingLastPathComponent())
        case .systemApplication:
            NSWorkspace.shared.open(item.url)
        }
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

    func copySelectedFile() {
        guard let selectedPath, let item = findItem(withID: selectedPath) else { return }
        copyFile(item)
    }

    func cutSelectedFile() {
        guard let selectedPath, let item = findItem(withID: selectedPath) else { return }
        cutFile(item)
    }

    func copyFile(_ item: ProjectFileItem) {
        FileBrowserPasteboard.writeFile(item, mode: .copy)
    }

    func cutFile(_ item: ProjectFileItem) {
        FileBrowserPasteboard.writeFile(item, mode: .cut)
    }

    func revealInFinder(_ item: ProjectFileItem) {
        NSWorkspace.shared.activateFileViewerSelecting([item.url])
    }

    func renameSelectedItem() {
        guard let selectedPath, let item = findItem(withID: selectedPath) else { return }
        beginRename(item)
    }

    func deleteSelectedItem() {
        guard let selectedPath, let item = findItem(withID: selectedPath) else { return }
        markForDelete(item)
    }

    func markForDelete(_ item: ProjectFileItem) {
        selectedPath = item.id
        pendingDeletePaths.insert(item.id)
        if renamingPath == item.id {
            cancelRename()
        }
    }

    func cancelPendingDeletes() {
        pendingDeletePaths.removeAll()
    }

    func confirmPendingDeletes() {
        let items = pendingDeletePaths.compactMap { findItem(withID: $0) }
        guard items.isEmpty == false else {
            pendingDeletePaths.removeAll()
            return
        }
        moveToTrash(items)
    }

    func beginRename(_ item: ProjectFileItem) {
        selectedPath = item.id
        renamingPath = item.id
        renamingName = item.name
        renamingFocusToken &+= 1
    }

    func updateRenamingName(_ name: String) {
        renamingName = name
    }

    func commitRename(_ item: ProjectFileItem) -> Bool {
        guard renamingPath == item.id else { return false }
        let newName = renamingName
        if rename(item, to: newName) {
            renamingPath = nil
            renamingName = ""
            return true
        } else {
            renamingPath = item.id
            renamingName = newName
            renamingFocusToken &+= 1
            return false
        }
    }

    func cancelRename() {
        renamingPath = nil
        renamingName = ""
    }

    private func rename(_ item: ProjectFileItem, to newName: String) -> Bool {
        selectedPath = item.id
        pendingDeletePaths.remove(item.id)
        do {
            let oldPath = item.id
            let parentURL = item.url.deletingLastPathComponent()
            let newURL = try service.renameItem(at: item.url, to: newName)
            selectedPath = newURL.path
            childrenByPath.removeValue(forKey: parentURL.standardizedFileURL.path)
            if item.isDirectory {
                let wasExpanded = expandedPaths.remove(oldPath) != nil
                childrenByPath.removeValue(forKey: oldPath)
                pendingDeletePaths = pendingDeletePaths.filter { $0 != oldPath && !$0.hasPrefix(oldPath + "/") }
                if wasExpanded {
                    expandedPaths.insert(newURL.path)
                }
            }
            expandAndReload(directory: parentURL)
            if item.isDirectory, expandedPaths.contains(newURL.path) {
                expandAndReload(directory: newURL)
            }
            return true
        } catch {
            errorMessage = error.localizedDescription
            return false
        }
    }

    func canPasteFiles() -> Bool {
        service.fileURLsFromPasteboard().isEmpty == false && targetDirectoryForCurrentSelection() != nil
    }

    func pasteFiles(into item: ProjectFileItem? = nil) {
        let urls = service.fileURLsFromPasteboard()
        guard urls.isEmpty == false else { return }
        if FileBrowserPasteboard.containsCutMarker {
            moveFiles(urls, into: item)
            NSPasteboard.general.clearContents()
        } else {
            copyFiles(urls, into: item)
        }
    }

    func copyFiles(_ urls: [URL], into item: ProjectFileItem? = nil) {
        guard let targetDirectory = targetDirectory(for: item) else { return }
        performFileTransfer(mode: .copy, urls: urls, targetDirectory: targetDirectory)
    }

    func moveFiles(_ urls: [URL], into item: ProjectFileItem? = nil) {
        guard let targetDirectory = targetDirectory(for: item) else { return }
        performFileTransfer(mode: .move, urls: urls, targetDirectory: targetDirectory)
    }

    func moveToTrash(_ item: ProjectFileItem) {
        moveToTrash([item])
    }

    private func moveToTrash(_ items: [ProjectFileItem]) {
        let urls = items.map(\.url)
        let ids = Set(items.map(\.id))
        NSWorkspace.shared.recycle(urls) { [weak self] _, error in
            Task { @MainActor in
                guard let self else { return }
                if let error {
                    self.errorMessage = String(
                        format: String(localized: "files.panel.delete.failure_format", defaultValue: "Could not move to Trash: %@", bundle: .module),
                        error.localizedDescription
                    )
                    return
                }
                if let selectedPath = self.selectedPath, ids.contains(selectedPath) {
                    self.selectedPath = nil
                }
                self.pendingDeletePaths.subtract(ids)
                for item in items {
                    self.expandedPaths.remove(item.id)
                    self.loadingPaths.remove(item.id)
                    self.childrenByPath.removeValue(forKey: item.id)
                }
                self.refresh()
            }
        }
    }

    func dragProvider(for item: ProjectFileItem) -> NSItemProvider {
        let provider = NSItemProvider(contentsOf: item.url) ?? NSItemProvider()
        let payload = [item.url.standardizedFileURL.path]
        provider.registerDataRepresentation(
            forTypeIdentifier: FileBrowserDropPayload.internalType,
            visibility: .all
        ) { completion in
            let data = try? JSONEncoder().encode(payload)
            completion(data, nil)
            return nil
        }
        return provider
    }

    private enum TransferMode {
        case copy
        case move
    }

    private func performFileTransfer(mode: TransferMode, urls: [URL], targetDirectory: URL) {
        let sourceDirectories = Set(urls.map { $0.deletingLastPathComponent().standardizedFileURL.path })
        do {
            let movedURLs: [URL]
            switch mode {
            case .copy:
                movedURLs = try service.copyItems(
                    urls,
                    to: targetDirectory,
                    conflictResolver: { [weak self] sourceURL, destinationURL, suggestedName in
                        self?.resolveConflictName(sourceURL: sourceURL, destinationURL: destinationURL, suggestedName: suggestedName)
                    }
                )
            case .move:
                movedURLs = try service.moveItems(
                    urls,
                    to: targetDirectory,
                    conflictResolver: { [weak self] sourceURL, destinationURL, suggestedName in
                        self?.resolveConflictName(sourceURL: sourceURL, destinationURL: destinationURL, suggestedName: suggestedName)
                    }
                )
            }
            guard movedURLs.isEmpty == false else { return }
            selectedPath = movedURLs.last?.path
            expandAndReload(directory: targetDirectory)
            for path in sourceDirectories where path != targetDirectory.standardizedFileURL.path {
                expandAndReload(directory: URL(fileURLWithPath: path))
            }
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    private func targetDirectory(for item: ProjectFileItem?) -> URL? {
        if let item {
            return item.isDirectory ? item.url : item.url.deletingLastPathComponent()
        }
        return targetDirectoryForCurrentSelection()
    }

    private func targetDirectoryForCurrentSelection() -> URL? {
        if let selectedPath, let item = findItem(withID: selectedPath) {
            return item.isDirectory ? item.url : item.url.deletingLastPathComponent()
        }
        return rootURL
    }

    private func expandAndReload(directory: URL) {
        let path = directory.standardizedFileURL.path
        expandedPaths.insert(path)
        childrenByPath.removeValue(forKey: path)
        if let item = findItem(withID: path) ?? directoryItem(for: directory) {
            loadChildren(for: item)
        }
        if rootItem?.id == path {
            loadChildren(for: rootItem!)
        }
    }

    private func directoryItem(for url: URL) -> ProjectFileItem? {
        guard let rootURL else { return nil }
        let values = try? url.resourceValues(forKeys: [.isDirectoryKey, .isSymbolicLinkKey, .nameKey])
        guard values?.isDirectory == true else { return nil }
        return ProjectFileItem(
            url: url.standardizedFileURL,
            name: values?.name ?? url.lastPathComponent,
            relativePath: service.relativePathForDisplay(url: url, rootURL: rootURL),
            isDirectory: true,
            isSymbolicLink: values?.isSymbolicLink == true
        )
    }

    private func resolveConflictName(sourceURL: URL, destinationURL: URL, suggestedName: String) -> String? {
        let alert = NSAlert()
        alert.alertStyle = .informational
        alert.messageText = String(localized: "files.panel.rename_conflict.title", defaultValue: "A file with this name already exists", bundle: .module)
        alert.informativeText = String(
            format: String(localized: "files.panel.rename_conflict.message_format", defaultValue: "%@ already exists in this folder. Enter a new name for the copied file.", bundle: .module),
            destinationURL.lastPathComponent
        )
        alert.addButton(withTitle: String(localized: "files.panel.rename_conflict.rename", defaultValue: "Rename", bundle: .module))
        alert.addButton(withTitle: String(localized: "cancel", defaultValue: "Cancel", bundle: .module))

        let input = NSTextField(frame: NSRect(x: 0, y: 0, width: 320, height: 24))
        input.stringValue = suggestedName
        alert.accessoryView = input

        guard alert.runModal() == .alertFirstButtonReturn else {
            return nil
        }
        return input.stringValue
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
    @State private var isPanelDropTargeted = false
    @State private var isKeyboardActive = false
    @State private var keyboardFocusToken = 0

    var body: some View {
        VStack(spacing: 0) {
            header
            GitPanelSeparator()
            content
            if store.pendingDeleteCount > 0 {
                GitPanelSeparator()
                FileBrowserDeleteConfirmationBar(
                    count: store.pendingDeleteCount,
                    cancel: { store.cancelPendingDeletes() },
                    confirm: { store.confirmPendingDeletes() }
                )
            }
        }
        .background(Color.clear)
        .background {
            FileBrowserKeyboardHandler(
                isActive: $isKeyboardActive,
                focusToken: keyboardFocusToken,
                isInlineRenaming: store.renamingPath != nil,
                copy: { store.copySelectedFile() },
                cut: { store.cutSelectedFile() },
                rename: { store.renameSelectedItem() },
                delete: { store.deleteSelectedItem() },
                paste: { store.pasteFiles() }
            )
            .frame(width: 1, height: 1)
        }
        .contentShape(Rectangle())
        .simultaneousGesture(
            TapGesture().onEnded {
                activateFileBrowserKeyboard()
            }
        )
        .onPasteCommand(of: [UTType.fileURL]) { _ in
            store.pasteFiles()
        }
        .onDrop(
            of: FileBrowserDropPayload.acceptedTypes,
            isTargeted: $isPanelDropTargeted
        ) { providers in
            handleDrop(providers: providers, target: nil)
        }
        .overlay {
            if isPanelDropTargeted {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(AppTheme.focus.opacity(0.55), lineWidth: 1)
                    .padding(6)
                    .allowsHitTesting(false)
            }
        }
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
        .contextMenu {
            Button(String(localized: "files.panel.paste", defaultValue: "Paste", bundle: .module)) {
                store.pasteFiles()
            }
            .disabled(store.canPasteFiles() == false)
        }
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
                            isRenaming: store.renamingPath == row.item.id,
                            isPendingDelete: store.pendingDeletePaths.contains(row.item.id),
                            renamingName: store.renamingName,
                            renamingFocusToken: store.renamingFocusToken,
                            focus: { activateFileBrowserKeyboard() },
                            select: { store.select(row.item) },
                            toggle: { store.toggle(row.item) },
                            openPreview: { store.openPreview(row.item) },
                            edit: { store.edit(row.item) },
                            insertPathIntoTerminal: { model.insertPathIntoCurrentTerminal(row.item.url) },
                            copyPath: { store.copyPath(row.item) },
                            copyFile: { store.copyFile(row.item) },
                            cutFile: { store.cutFile(row.item) },
                            rename: { store.beginRename(row.item) },
                            updateRenamingName: { store.updateRenamingName($0) },
                            commitRename: {
                                if store.commitRename(row.item) {
                                    activateFileBrowserKeyboard()
                                }
                            },
                            cancelRename: {
                                store.cancelRename()
                                activateFileBrowserKeyboard()
                            },
                            paste: { store.pasteFiles(into: row.item) },
                            canPaste: store.canPasteFiles(),
                            delete: { store.markForDelete(row.item) },
                            reveal: { store.revealInFinder(row.item) },
                            dragProvider: { store.dragProvider(for: row.item) },
                            drop: { providers in handleDrop(providers: providers, target: row.item) }
                        )
                    }
                }
                .padding(.vertical, 8)
                .animation(.easeOut(duration: 0.16), value: store.visibleRows.map(\.id))
            }
        }
    }

    private func activateFileBrowserKeyboard() {
        isKeyboardActive = true
        keyboardFocusToken &+= 1
        FileBrowserKeyboardFocusState.isActive = true
        FileBrowserKeyboardFocusState.isInlineRenaming = store.renamingPath != nil
    }

    private func handleDrop(providers: [NSItemProvider], target: ProjectFileItem?) -> Bool {
        let payload = FileBrowserDropPayload.loadFromDraggingPasteboard(fallbackProviders: providers)
        switch payload {
        case .internalFiles(let urls):
            store.moveFiles(urls, into: target)
        case .externalFiles(let urls):
            store.copyFiles(urls, into: target)
        case .none:
            break
        }
        return true
    }
}

private struct FileBrowserRowView: View {
    let row: ProjectFileRow
    let isExpanded: Bool
    let isLoading: Bool
    let isSelected: Bool
    let isRenaming: Bool
    let isPendingDelete: Bool
    let renamingName: String
    let renamingFocusToken: Int
    let focus: () -> Void
    let select: () -> Void
    let toggle: () -> Void
    let openPreview: () -> Void
    let edit: () -> Void
    let insertPathIntoTerminal: () -> Void
    let copyPath: () -> Void
    let copyFile: () -> Void
    let cutFile: () -> Void
    let rename: () -> Void
    let updateRenamingName: (String) -> Void
    let commitRename: () -> Void
    let cancelRename: () -> Void
    let paste: () -> Void
    let canPaste: Bool
    let delete: () -> Void
    let reveal: () -> Void
    let dragProvider: () -> NSItemProvider
    let drop: ([NSItemProvider]) -> Bool
    @State private var isHovered = false
    @State private var isDropTargeted = false

    var body: some View {
        interactiveContent
            .contextMenu {
                Button(String(localized: "files.panel.open", defaultValue: "Open", bundle: .module), action: openPreview)
                    .disabled(row.item.isDirectory)
                Button(String(localized: "files.panel.edit", defaultValue: "Edit", bundle: .module), action: edit)
                    .disabled(row.item.isDirectory)
                Button(String(localized: "files.panel.insert_path_terminal", defaultValue: "Insert Path into Terminal", bundle: .module), action: insertPathIntoTerminal)
                Button(String(localized: "files.panel.copy_path", defaultValue: "Copy Path", bundle: .module), action: copyPath)
                Button(String(localized: "files.panel.copy", defaultValue: "Copy", bundle: .module), action: copyFile)
                Button(String(localized: "files.panel.cut", defaultValue: "Cut", bundle: .module), action: cutFile)
                Button(String(localized: "common.rename", defaultValue: "Rename", bundle: .module), action: rename)
                Button(String(localized: "files.panel.paste", defaultValue: "Paste", bundle: .module), action: paste)
                    .disabled(canPaste == false)
                Button(String(localized: "files.panel.reveal_finder", defaultValue: "Reveal in Finder", bundle: .module), action: reveal)
                Divider()
                Button(role: .destructive, action: delete) {
                    Text(String(localized: "files.panel.delete", defaultValue: "Move to Trash", bundle: .module))
                }
            }
            .help(row.item.relativePath.isEmpty ? row.item.name : row.item.relativePath)
            .onDrag(dragProvider)
            .onDrop(
                of: FileBrowserDropPayload.acceptedTypes,
                isTargeted: $isDropTargeted,
                perform: drop
            )
    }

    @ViewBuilder
    private var interactiveContent: some View {
        if isRenaming {
            rowContent
        } else if row.item.isDirectory {
            rowContent
                .onTapGesture {
                    focus()
                    toggle()
                }
        } else {
            rowContent
                .onTapGesture {
                    focus()
                    select()
                }
                .simultaneousGesture(
                    TapGesture(count: 2).onEnded {
                        focus()
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

            nameView

            if row.item.isSymbolicLink {
                Image(systemName: "arrowshape.turn.up.right")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(AppTheme.textMuted)
            }
        }
        .opacity(isPendingDelete ? 0.46 : 1)
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
    private var nameView: some View {
        if isRenaming {
            FileBrowserInlineRenameField(
                text: renamingName,
                focusToken: renamingFocusToken,
                onTextChange: updateRenamingName,
                onCommit: commitRename,
                onCancel: cancelRename
            )
            .frame(maxWidth: .infinity, alignment: .leading)
        } else {
            Text(row.item.name)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .truncationMode(.middle)
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
                isPendingDelete
                    ? Color.red.opacity(isSelected ? 0.18 : 0.10)
                    : isSelected
                    ? AppTheme.focus.opacity(0.16)
                    : (isDropTargeted ? AppTheme.focus.opacity(0.12) : (isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear))
            )
            .padding(.horizontal, 8)
    }
}

private struct FileBrowserDeleteConfirmationBar: View {
    let count: Int
    let cancel: () -> Void
    let confirm: () -> Void
    @State private var hoveredAction: Action?

    private enum Action {
        case cancel
        case confirm
    }

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 6) {
                Image(systemName: "trash")
                    .font(.system(size: 12, weight: .semibold))
                Text(statusText)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
            }
            .foregroundStyle(Color.white.opacity(0.92))

            Spacer(minLength: 8)

            HStack(spacing: 12) {
                barButton(
                    action: .cancel,
                    systemImage: "xmark",
                    help: String(localized: "files.panel.delete.cancel", defaultValue: "Cancel Delete", bundle: .module),
                    perform: cancel
                )
                barButton(
                    action: .confirm,
                    systemImage: "checkmark",
                    help: String(localized: "files.panel.delete.confirm", defaultValue: "Confirm Delete", bundle: .module),
                    perform: confirm
                )
            }
            .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, 12)
        .frame(height: 32)
        .frame(maxWidth: .infinity)
        .background(Color.red.opacity(0.78))
    }

    private var statusText: String {
        String(
            format: String(localized: "files.panel.delete.pending_count_format", defaultValue: "%d item(s) marked for delete", bundle: .module),
            count
        )
    }

    private func barButton(
        action: Action,
        systemImage: String,
        help: String,
        perform: @escaping () -> Void
    ) -> some View {
        Button(action: perform) {
            Image(systemName: systemImage)
                .font(.system(size: 12, weight: .bold))
                .foregroundStyle(Color.white.opacity(0.96))
                .frame(width: 24, height: 22)
                .contentShape(Rectangle())
                .background {
                    RoundedRectangle(cornerRadius: 4, style: .continuous)
                        .fill(Color.white.opacity(hoveredAction == action ? 0.22 : 0.001))
                }
        }
        .buttonStyle(.plain)
        .help(help)
        .onHover { hovering in
            hoveredAction = hovering ? action : (hoveredAction == action ? nil : hoveredAction)
        }
    }
}

private enum FileBrowserDropPayload {
    static let internalType = "com.duxweb.codux.project-file-url-list"
    static let acceptedTypes = [internalType, UTType.fileURL.identifier]

    enum Payload {
        case internalFiles([URL])
        case externalFiles([URL])
        case none
    }

    static func loadFromDraggingPasteboard(fallbackProviders providers: [NSItemProvider]) -> Payload {
        let pasteboard = NSPasteboard(name: .drag)
        if let data = pasteboard.data(forType: NSPasteboard.PasteboardType(internalType)) {
            let paths = (try? JSONDecoder().decode([String].self, from: data)) ?? []
            let urls = paths.map { URL(fileURLWithPath: $0).standardizedFileURL }
            if urls.isEmpty == false {
                return .internalFiles(urls)
            }
        }
        let options: [NSPasteboard.ReadingOptionKey: Any] = [
            .urlReadingFileURLsOnly: true,
        ]
        if let urls = pasteboard.readObjects(forClasses: [NSURL.self], options: options) as? [URL],
           urls.isEmpty == false {
            return .externalFiles(urls.map { $0.standardizedFileURL })
        }
        if providers.contains(where: { $0.hasItemConformingToTypeIdentifier(internalType) }) {
            return .none
        }
        return .none
    }
}

private struct FileBrowserInlineRenameField: NSViewRepresentable {
    let text: String
    let focusToken: Int
    let onTextChange: (String) -> Void
    let onCommit: () -> Void
    let onCancel: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(parent: self)
    }

    func makeNSView(context: Context) -> NSTextField {
        let field = NSTextField(string: text)
        field.delegate = context.coordinator
        field.font = .systemFont(ofSize: 12, weight: .medium)
        field.isBordered = true
        field.isBezeled = true
        field.bezelStyle = .roundedBezel
        field.drawsBackground = true
        field.backgroundColor = NSColor.textBackgroundColor.withAlphaComponent(0.92)
        field.textColor = .labelColor
        field.focusRingType = .none
        return field
    }

    func updateNSView(_ nsView: NSTextField, context: Context) {
        context.coordinator.parent = self
        if nsView.stringValue != text {
            nsView.stringValue = text
        }
        context.coordinator.focusIfNeeded(field: nsView, focusToken: focusToken)
    }

    @MainActor
    final class Coordinator: NSObject, NSTextFieldDelegate {
        var parent: FileBrowserInlineRenameField
        private var lastFocusToken = 0
        private var didRequestFocus = false
        private var isFinishing = false

        init(parent: FileBrowserInlineRenameField) {
            self.parent = parent
        }

        func controlTextDidChange(_ notification: Notification) {
            guard let field = notification.object as? NSTextField else { return }
            parent.onTextChange(field.stringValue)
        }

        func controlTextDidEndEditing(_ notification: Notification) {
            finish(commit: true)
        }

        func control(
            _ control: NSControl,
            textView: NSTextView,
            doCommandBy commandSelector: Selector
        ) -> Bool {
            switch commandSelector {
            case #selector(NSResponder.insertNewline(_:)):
                parent.onTextChange(textView.string)
                finish(commit: true)
                return true
            case #selector(NSResponder.cancelOperation(_:)):
                finish(commit: false)
                return true
            default:
                return false
            }
        }

        func focusIfNeeded(field: NSTextField, focusToken: Int) {
            guard focusToken != lastFocusToken || didRequestFocus == false else { return }
            lastFocusToken = focusToken
            didRequestFocus = true
            DispatchQueue.main.async {
                guard let window = field.window else { return }
                window.makeFirstResponder(field)
                if let editor = window.fieldEditor(true, for: field) as? NSTextView {
                    editor.selectedRange = NSRange(location: 0, length: field.stringValue.count)
                    editor.insertionPointColor = .labelColor
                }
            }
        }

        private func finish(commit: Bool) {
            guard isFinishing == false else { return }
            isFinishing = true
            if commit {
                parent.onCommit()
            } else {
                parent.onCancel()
            }
            Task { @MainActor [weak self] in
                self?.isFinishing = false
            }
        }
    }
}

private struct FileBrowserKeyboardHandler: NSViewRepresentable {
    @Binding var isActive: Bool
    let focusToken: Int
    let isInlineRenaming: Bool
    let copy: () -> Void
    let cut: () -> Void
    let rename: () -> Void
    let delete: () -> Void
    let paste: () -> Void

    func makeNSView(context: Context) -> KeyView {
        let view = KeyView()
        view.onDeactivate = {
            isActive = false
        }
        return view
    }

    func updateNSView(_ view: KeyView, context: Context) {
        view.isActive = isActive
        view.isInlineRenaming = isInlineRenaming
        FileBrowserKeyboardFocusState.isActive = isActive
        FileBrowserKeyboardFocusState.isInlineRenaming = isInlineRenaming
        view.requestFocusIfNeeded(focusToken: focusToken)
        view.onDeactivate = {
            isActive = false
        }
        view.copyAction = copy
        view.cutAction = cut
        view.renameAction = rename
        view.deleteAction = delete
        view.pasteAction = paste
    }

    static func dismantleNSView(_ nsView: KeyView, coordinator: ()) {
        nsView.uninstallMonitors()
    }

    @MainActor
    final class KeyView: NSView {
        var copyAction: (() -> Void)?
        var cutAction: (() -> Void)?
        var renameAction: (() -> Void)?
        var deleteAction: (() -> Void)?
        var pasteAction: (() -> Void)?
        var onDeactivate: (() -> Void)?
        var isActive = false
        var isInlineRenaming = false
        private var keyMonitor: Any?
        private var terminalFocusObserver: NSObjectProtocol?
        private var lastFocusToken = 0

        override var acceptsFirstResponder: Bool {
            true
        }

        override func becomeFirstResponder() -> Bool {
            isActive = true
            FileBrowserKeyboardFocusState.isActive = true
            FileBrowserKeyboardFocusState.isInlineRenaming = isInlineRenaming
            return true
        }

        override func resignFirstResponder() -> Bool {
            FileBrowserKeyboardFocusState.isActive = false
            FileBrowserKeyboardFocusState.isInlineRenaming = false
            return true
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            installKeyMonitorIfNeeded()
            installTerminalFocusObserverIfNeeded()
        }

        func requestFocusIfNeeded(focusToken: Int) {
            guard isActive, focusToken != lastFocusToken else {
                return
            }
            lastFocusToken = focusToken
            DispatchQueue.main.async { [weak self] in
                guard let self, self.isActive, let window = self.window else {
                    return
                }
                window.makeFirstResponder(self)
            }
        }

        func uninstallMonitors() {
            if let keyMonitor {
                NSEvent.removeMonitor(keyMonitor)
            }
            keyMonitor = nil
            if let terminalFocusObserver {
                NotificationCenter.default.removeObserver(terminalFocusObserver)
            }
            terminalFocusObserver = nil
        }

        private func installKeyMonitorIfNeeded() {
            guard keyMonitor == nil else { return }
            keyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
                guard let self else { return event }
                guard self.shouldHandleLocally(event),
                      self.handleFileBrowserKeyDown(event) else {
                    return event
                }
                return nil
            }
        }

        private func installTerminalFocusObserverIfNeeded() {
            guard terminalFocusObserver == nil else { return }
            terminalFocusObserver = NotificationCenter.default.addObserver(
                forName: .dmuxTerminalFocusDidChange,
                object: nil,
                queue: .main
            ) { [weak self] notification in
                let hasFocusedTerminal = notification.object != nil
                Task { @MainActor [weak self] in
                    guard hasFocusedTerminal else { return }
                    self?.isActive = false
                    FileBrowserKeyboardFocusState.isActive = false
                    FileBrowserKeyboardFocusState.isInlineRenaming = false
                    self?.onDeactivate?()
                }
            }
        }

        private func shouldHandleLocally(_ event: NSEvent) -> Bool {
            let currentWindow = window
            let eventWindowMatches = event.window.map { $0 === currentWindow } ?? true
            let isTerminalResponder = currentWindow?.firstResponder.map {
                DmuxTerminalBackend.shared.registry.ownsResponder($0)
            } ?? false

            return FileBrowserKeyboardFocusState.shouldHandleFileBrowserShortcut(
                isActive: isActive,
                isInlineRenaming: isInlineRenaming,
                hasWindow: currentWindow != nil,
                eventWindowMatches: eventWindowMatches,
                isTerminalResponder: isTerminalResponder
            )
        }

        private func handleFileBrowserKeyDown(_ event: NSEvent) -> Bool {
            let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
            if modifiers.contains(.command),
               event.charactersIgnoringModifiers?.lowercased() == "c" {
                copyAction?()
                return true
            }
            if modifiers.contains(.command),
               event.charactersIgnoringModifiers?.lowercased() == "x" {
                cutAction?()
                return true
            }
            if modifiers.contains(.command),
               event.charactersIgnoringModifiers?.lowercased() == "v" {
                pasteAction?()
                return true
            }

            switch event.keyCode {
            case 36, 76:
                renameAction?()
                return true
            case 51, 117:
                deleteAction?()
                return true
            default:
                return false
            }
        }
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
