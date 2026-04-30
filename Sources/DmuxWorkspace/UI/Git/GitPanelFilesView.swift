import AppKit
import SwiftUI

struct GitFilesRegion: View {
    let model: AppModel
    let gitState: GitRepositoryState
    @Binding var stagedExpanded: Bool
    @Binding var changesExpanded: Bool
    @Binding var untrackedExpanded: Bool
    let scrollResetToken: UUID
    let clearFocus: () -> Void

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                Color.clear
                    .frame(height: 0)
                    .id("git-files-top")

                LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
                    GitListSection(
                        kind: .staged,
                        entries: gitState.staged,
                        accent: AppTheme.success,
                        isExpanded: $stagedExpanded,
                        primaryIcon: "minus.circle",
                        primaryAction: { model.unstage($0) },
                        secondaryIcon: nil,
                        secondaryAction: nil,
                        model: model
                    )

                    GitListSection(
                        kind: .changed,
                        entries: gitState.changes,
                        accent: AppTheme.warning,
                        isExpanded: $changesExpanded,
                        primaryIcon: "plus.circle",
                        primaryAction: { model.stage($0) },
                        secondaryIcon: "arrow.uturn.backward",
                        secondaryAction: { model.discard($0) },
                        model: model
                    )

                    GitListSection(
                        kind: .untracked,
                        entries: gitState.untracked,
                        accent: AppTheme.textSecondary,
                        isExpanded: $untrackedExpanded,
                        primaryIcon: "plus.circle",
                        primaryAction: { model.stage($0) },
                        secondaryIcon: "trash",
                        secondaryAction: { model.discard($0) },
                        model: model
                    )
                }
            }
            .scrollIndicators(.automatic)
            .background(Color.clear)
            .contentShape(Rectangle())
            .onTapGesture {
                clearFocus()
            }
            .onChange(of: scrollResetToken) { _, _ in
                proxy.scrollTo("git-files-top", anchor: .top)
            }
        }
    }
}

private struct GitListSection: View {
    let kind: GitFileKind
    let entries: [GitFileEntry]
    let accent: Color
    @Binding var isExpanded: Bool
    let primaryIcon: String
    let primaryAction: (GitFileEntry) -> Void
    let secondaryIcon: String?
    let secondaryAction: ((GitFileEntry) -> Void)?
    let model: AppModel

    private var selectedEntries: [GitFileEntry] {
        entries.filter { model.isGitEntrySelected($0) }
    }

    private var shouldShowHeaderActions: Bool {
        !selectedEntries.isEmpty
    }

    private var actionEntries: [GitFileEntry] {
        selectedEntries
    }

    private var headerActions: [GitSectionHeaderAction] {
        switch kind {
        case .staged:
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "minus", help: String(localized: "git.files.unstage_selected", defaultValue: "Unstage Selected", bundle: .module)) {
                    model.unstageEntries(actionEntries)
                }
            ]
        case .changed:
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "plus", help: String(localized: "git.files.stage_selected", defaultValue: "Stage Selected", bundle: .module)) {
                    model.stageEntries(actionEntries)
                },
                GitSectionHeaderAction(icon: "discard", help: String(localized: "git.files.discard_selected", defaultValue: "Discard Selected", bundle: .module)) {
                    model.discardEntries(actionEntries)
                }
            ]
        case .untracked:
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "plus", help: String(localized: "git.files.stage_selected", defaultValue: "Stage Selected", bundle: .module)) {
                    model.stageEntries(actionEntries)
                }
            ]
        }
    }

    var body: some View {
        Section {
            if isExpanded {
                GitListSectionContent(
                    entries: entries,
                    accent: accent,
                    model: model,
                    primaryIcon: primaryIcon,
                    primaryAction: primaryAction,
                    secondaryIcon: secondaryIcon,
                    secondaryAction: secondaryAction
                )
            }
        } header: {
            Button {
                isExpanded.toggle()
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(AppTheme.textSecondary)
                        .frame(width: 12, alignment: .center)

                    Text(displayTitle)
                        .font(.system(size: 12, weight: .semibold, design: .rounded))
                        .foregroundStyle(AppTheme.textSecondary)

                    Spacer()

                    if shouldShowHeaderActions, !headerActions.isEmpty {
                        HStack(spacing: 2) {
                            ForEach(Array(headerActions.enumerated()), id: \.offset) { _, action in
                                Button(action: action.action) {
                                    HeaderActionIcon(symbol: action.icon)
                                }
                                .buttonStyle(GitHeaderIconButtonStyle())
                                .help(action.help)
                            }
                        }
                    }

                    Text("\(entries.count)")
                        .font(.system(size: 11, weight: .bold, design: .rounded))
                        .foregroundStyle(accent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
                .padding(.leading, 10)
                .padding(.trailing, 14)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: 34)
            .background {
                RoundedRectangle(cornerRadius: 0, style: .continuous)
                    .fill(AppTheme.aiPanelCardBackground.opacity(0.6))
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(AppTheme.separator)
                    .frame(height: 1)
            }
            .zIndex(1)
        }
    }

    private var displayTitle: String {
        switch kind {
        case .staged:
            return String(localized: "git.files.staged", defaultValue: "Staged", bundle: .module)
        case .changed:
            return String(localized: "git.files.changes", defaultValue: "Changes", bundle: .module)
        case .untracked:
            return String(localized: "git.files.untracked", defaultValue: "Untracked", bundle: .module)
        }
    }
}

private struct GitListSectionContent: View {
    let entries: [GitFileEntry]
    let accent: Color
    let model: AppModel
    let primaryIcon: String
    let primaryAction: (GitFileEntry) -> Void
    let secondaryIcon: String?
    let secondaryAction: ((GitFileEntry) -> Void)?

    var body: some View {
        if entries.isEmpty {
            EmptyView()
        } else {
            VStack(spacing: 0) {
                ForEach(entries) { entry in
                    GitFileRow(
                        entry: entry,
                        accent: accent,
                        model: model,
                        primaryIcon: primaryIcon,
                        primaryAction: { primaryAction(entry) },
                        secondaryIcon: secondaryIcon,
                        secondaryAction: secondaryAction.map { action in { action(entry) } }
                    )
                }
            }
        }
    }
}

private struct GitFileRow: View {
    let entry: GitFileEntry
    let accent: Color
    let model: AppModel
    let primaryIcon: String
    let primaryAction: () -> Void
    let secondaryIcon: String?
    let secondaryAction: (() -> Void)?

    @State private var isHovered = false

    private var isSelected: Bool {
        model.isGitEntrySelected(entry)
    }

    var body: some View {
        Button {
            let modifierFlags = NSApp.currentEvent?.modifierFlags ?? NSEvent.modifierFlags
            let shiftPressed = modifierFlags.contains(.shift)
            let commandPressed = modifierFlags.contains(.command)

            if commandPressed {
                model.toggleGitEntrySelection(entry)
            } else {
                model.selectGitEntry(entry, extendingRange: shiftPressed)
            }
            model.loadDiff(for: entry)
        } label: {
            HStack(spacing: 8) {
                Color.clear
                    .frame(width: 12, height: 1)

                GitFilePathLabel(path: entry.path)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help(entry.path)

                trailingAccessorySlot
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 10)
            .padding(.trailing, 14)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .simultaneousGesture(
            TapGesture(count: 2).onEnded {
                model.openGitDiffWindow(for: entry)
            }
        )
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(rowBackground)
        .overlay {
            NativeContextMenuRegion(
                onOpen: {
                    model.prepareGitEntryContextMenu(entry)
                },
                menuProvider: {
                    buildGitFileContextMenu(model: model, fallbackEntry: entry)
                }
            )
        }
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var rowBackground: some View {
        Rectangle()
            .fill(baseRowColor)
            .overlay(alignment: .leading) {
                if isSelected {
                    Rectangle()
                        .fill(accent)
                        .frame(width: 2)
                }
            }
    }

    private var baseRowColor: Color {
        if isSelected {
            return AppTheme.focus.opacity(0.14)
        }

        return isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear
    }

    private var actionSlotWidth: CGFloat {
        secondaryIcon == nil ? 44 : 68
    }

    private var trailingSlotWidth: CGFloat {
        max(actionSlotWidth, GitStatusBadge.width)
    }

    private var trailingAccessorySlot: some View {
        Color.clear
            .frame(width: trailingSlotWidth, height: GitStatusBadge.height)
            .overlay(alignment: .trailing) {
                if isHovered {
                    GitHoverActions(
                        primaryIcon: primaryIcon,
                        primaryAction: primaryAction,
                        secondaryIcon: secondaryIcon,
                        secondaryAction: secondaryAction
                    )
                } else {
                    GitStatusBadge(entry: entry, accent: accent)
                }
            }
    }
}

private struct GitFilePathLabel: View {
    let path: String

    private var nsPath: NSString {
        path as NSString
    }

    private var fileName: String {
        nsPath.lastPathComponent
    }

    private var parentPath: String {
        let parent = nsPath.deletingLastPathComponent
        return parent == "." ? "" : parent
    }

    var body: some View {
        HStack(spacing: 0) {
            if parentPath.isEmpty == false {
                Text("\(parentPath)/")
                    .font(.system(size: 11, weight: .regular, design: .monospaced))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
                    .truncationMode(.head)
            }

            Text(fileName)
                .font(.system(size: 12, weight: .medium, design: .monospaced))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .truncationMode(.tail)
                .layoutPriority(1)
        }
    }
}

private struct GitStatusBadge: View {
    let entry: GitFileEntry
    let accent: Color

    static let width: CGFloat = 14
    static let height: CGFloat = 14

    private var label: String {
        switch entry.kind {
        case .staged:
            return "S"
        case .changed:
            return "M"
        case .untracked:
            return "U"
        }
    }

    var body: some View {
        Text(label)
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .foregroundStyle(accent)
            .frame(width: Self.width, height: Self.height, alignment: .trailing)
    }
}

private struct GitHoverActions: View {
    let primaryIcon: String
    let primaryAction: () -> Void
    let secondaryIcon: String?
    let secondaryAction: (() -> Void)?

    var body: some View {
        HStack(spacing: 2) {
            if let secondaryIcon, let secondaryAction {
                Button(action: secondaryAction) {
                    HoverActionIcon(symbol: secondaryIcon)
                }
                .buttonStyle(GitIconButtonStyle())
            }

            Button(action: primaryAction) {
                HoverActionIcon(symbol: primaryIcon)
            }
            .buttonStyle(GitIconButtonStyle())
        }
        .padding(.leading, 12)
    }
}

private struct GitSectionHeaderAction {
    let icon: String
    let help: String
    let action: () -> Void
}

private struct HeaderActionIcon: View {
    let symbol: String

    var body: some View {
        switch symbol {
        case "plus":
            Image(systemName: "plus")
                .font(.system(size: 10, weight: .semibold))
        case "minus":
            Image(systemName: "minus")
                .font(.system(size: 10, weight: .semibold))
        case "discard":
            Image(systemName: "arrow.uturn.backward")
                .font(.system(size: 10, weight: .semibold))
        default:
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
        }
    }
}

private struct HoverActionIcon: View {
    let symbol: String

    var body: some View {
        switch symbol {
        case "plus.circle":
            Image(systemName: "plus")
                .font(.system(size: 10, weight: .semibold))
        case "minus.circle":
            Image(systemName: "minus")
                .font(.system(size: 10, weight: .semibold))
        case "trash":
            Image(systemName: "trash")
                .font(.system(size: 10, weight: .semibold))
        case "arrow.uturn.backward":
            Image(systemName: "arrow.uturn.backward")
                .font(.system(size: 10, weight: .semibold))
        default:
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
        }
    }
}

enum GitFileDiffWindowPresenter {
    @MainActor private static var controllers: [String: NSWindowController] = [:]

    @MainActor
    static func show(entry: GitFileEntry, project: Project) {
        let key = "\(project.id.uuidString):\(entry.id)"
        if let controller = controllers[key] {
            controller.window?.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let contentView = GitFileDiffWindowView(entry: entry, project: project)
        let hostingController = NSHostingController(rootView: contentView)
        let window = NSWindow(contentViewController: hostingController)
        window.title = entry.path
        window.setContentSize(NSSize(width: 1080, height: 680))
        window.minSize = NSSize(width: 760, height: 420)
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable]
        window.isReleasedWhenClosed = false
        let controller = NSWindowController(window: window)
        controllers[key] = controller
        NotificationCenter.default.addObserver(
            forName: NSWindow.willCloseNotification,
            object: window,
            queue: .main
        ) { _ in
            Task { @MainActor in
                controllers[key] = nil
            }
        }
        controller.showWindow(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

private struct GitFileDiffWindowView: View {
    let entry: GitFileEntry
    let project: Project
    @State private var preview: GitFileDiffPreview?
    @State private var errorMessage: String?

    var body: some View {
        VStack(spacing: 0) {
            content
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .task(id: entry.id) {
            await loadDiff()
        }
    }

    @ViewBuilder
    private var content: some View {
        if let preview {
            GitSideBySideDiffView(preview: preview)
        } else if let errorMessage {
            VStack(spacing: 10) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 30, weight: .semibold))
                    .foregroundStyle(AppTheme.warning)
                Text(errorMessage)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(AppTheme.textSecondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 420)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(24)
        } else {
            VStack(spacing: 12) {
                ProgressView()
                    .controlSize(.small)
                Text(String(localized: "git.diff.loading", defaultValue: "Loading diff...", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
    }

    private func loadDiff() async {
        let path = project.path
        let entry = entry
        do {
            let loadedPreview = try await Task.detached {
                try GitService().sideBySideDiff(for: entry, at: path)
            }.value
            preview = loadedPreview
            errorMessage = nil
        } catch {
            preview = nil
            errorMessage = error.localizedDescription
        }
    }
}

private struct GitSideBySideDiffView: View {
    let preview: GitFileDiffPreview

    var body: some View {
        GeometryReader { proxy in
            let contentWidth = max(proxy.size.width, preferredContentWidth)
            ScrollView(.horizontal) {
                VStack(spacing: 0) {
                    HStack(spacing: 0) {
                        GitDiffColumnHeader(title: preview.newTitle, symbol: "plus.square.fill", color: AppTheme.success, side: .new)
                        Rectangle()
                            .fill(AppTheme.separator.opacity(0.6))
                            .frame(width: 1)
                        GitDiffColumnHeader(title: preview.oldTitle, symbol: "minus.square.fill", color: AppTheme.warning, side: .old)
                    }
                    .frame(width: contentWidth, height: 32)

                    GitPanelSeparator()

                    ScrollView(.vertical) {
                        LazyVStack(spacing: 0) {
                            ForEach(preview.rows) { row in
                                GitDiffRowView(row: row)
                            }
                        }
                        .frame(width: contentWidth, alignment: .leading)
                        .padding(.vertical, 4)
                    }
                    .frame(width: contentWidth, height: max(0, proxy.size.height - 33))
                }
                .frame(width: contentWidth, height: proxy.size.height, alignment: .top)
            }
            .background(Color(nsColor: .textBackgroundColor).opacity(0.4))
        }
    }

    private var preferredContentWidth: CGFloat {
        let maxLineLength = preview.rows.reduce(0) { partial, row in
            max(partial, row.newLine?.text.count ?? 0, row.oldLine?.text.count ?? 0)
        }
        let estimatedTextWidth = CGFloat(min(maxLineLength, 260)) * 7.2
        let columnWidth = max(360, 52 + 2 + 20 + estimatedTextWidth)
        return columnWidth * 2 + 1
    }
}

private struct GitDiffColumnHeader: View {
    let title: String
    let symbol: String
    let color: Color
    let side: GitDiffLineSide

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(color)
            Text(title)
                .font(.system(size: 12, weight: .semibold, design: .rounded))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .truncationMode(.middle)
            Spacer(minLength: 0)
        }
        .padding(.horizontal, 14)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(color.opacity(0.10))
    }
}

private struct GitDiffRowView: View {
    let row: GitFileDiffRow

    var body: some View {
        HStack(spacing: 0) {
            GitDiffLinePane(line: row.newLine, kind: row.kind, side: .new)
            Rectangle()
                .fill(AppTheme.separator.opacity(0.5))
                .frame(width: 1)
            GitDiffLinePane(line: row.oldLine, kind: row.kind, side: .old)
        }
        .frame(minHeight: 20)
    }
}

private enum GitDiffLineSide {
    case new
    case old
}

private struct GitDiffLinePane: View {
    let line: GitFileDiffLine?
    let kind: GitFileDiffRowKind
    let side: GitDiffLineSide

    var body: some View {
        HStack(spacing: 0) {
            ZStack {
                Color(nsColor: .controlBackgroundColor).opacity(0.5)
                Text(lineNumberText)
                    .font(.system(size: 10.5, weight: .regular, design: .monospaced))
                    .foregroundStyle(AppTheme.textMuted.opacity(0.7))
                    .frame(maxWidth: .infinity, alignment: .trailing)
                    .padding(.trailing, 10)
            }
            .frame(width: 52)

            Rectangle()
                .fill(markerColor)
                .frame(width: 2)

            Text(line?.text ?? "")
                .font(.system(size: 12, weight: .regular, design: .monospaced))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.leading, 7)
                .padding(.trailing, 10)
                .padding(.vertical, 1)
        }
        .frame(maxWidth: .infinity, minHeight: 20, alignment: .leading)
        .background(backgroundColor)
    }

    private var lineNumberText: String {
        guard let number = line?.number else {
            return ""
        }
        return "\(number)"
    }

    private var backgroundColor: Color {
        switch (kind, side) {
        case (.added, .new):
            return Color(nsColor: .systemGreen).opacity(0.18)
        case (.removed, .old):
            return Color(nsColor: .systemRed).opacity(0.18)
        case (.modified, .new):
            return Color(nsColor: .systemGreen).opacity(0.14)
        case (.modified, .old):
            return Color(nsColor: .systemOrange).opacity(0.18)
        default:
            return Color.clear
        }
    }

    private var markerColor: Color {
        switch (kind, side) {
        case (.added, .new):
            return Color(nsColor: .systemGreen).opacity(0.7)
        case (.removed, .old):
            return Color(nsColor: .systemRed).opacity(0.7)
        case (.modified, .new):
            return Color(nsColor: .systemGreen).opacity(0.5)
        case (.modified, .old):
            return Color(nsColor: .systemOrange).opacity(0.6)
        default:
            return Color.clear
        }
    }
}
