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
