import AppKit
import SwiftUI

private enum WorkspaceEditorPendingAction {
    case save
    case saveAs(URL)
    case saveAndClose
}

private enum WorkspaceFileEditorLayoutPolicy {
    static let largeInlineEditorModeBytes = 20 * 1024 * 1024
}

struct WorkspaceFileEditorView: View {
    let model: AppModel
    let tab: WorkspaceFileTab
    let isSelected: Bool

    @State private var preview: ProjectFilePreview
    @State private var textContent: String
    @State private var savedTextContent: String
    @State private var editorFocusToken = 1
    @State private var editorRenderToken = 0
    @State private var editorCopyToken = 0
    @State private var editorPasteToken = 0
    @State private var editorUndoToken = 0
    @State private var editorRedoToken = 0
    @State private var editorFindToken = 0
    @State private var editorSnapshotToken = 0
    @State private var editorMarkSavedToken = 0
    @State private var editorReportedDirty = false
    @State private var statusMessage: String?
    @State private var pendingAction: WorkspaceEditorPendingAction?

    init(model: AppModel, tab: WorkspaceFileTab, isSelected: Bool) {
        self.model = model
        self.tab = tab
        self.isSelected = isSelected
        let initialPreview = ProjectFileBrowserService().preview(for: tab.fileURL, rootURL: tab.rootURL)
        _preview = State(initialValue: initialPreview)
        let initialText = Self.textContent(from: initialPreview) ?? ""
        _textContent = State(initialValue: initialText)
        _savedTextContent = State(initialValue: initialText)
    }

    private var editorTheme: ProjectFileEditorTheme {
        ProjectFileEditorTheme(
            appearance: model.terminalAppearance,
            fontSize: model.appSettings.terminalFontSize
        )
    }

    private var canEdit: Bool {
        if case .text = preview.state {
            return true
        }
        return false
    }

    private var isDirty: Bool {
        canEdit && (editorReportedDirty || textContent != savedTextContent)
    }

    private var isLargeFileMode: Bool {
        textContent.utf8.count > WorkspaceFileEditorLayoutPolicy.largeInlineEditorModeBytes
    }

    private var saveRequestToken: Int {
        model.workspaceFileEditorSaveRequestToken(for: tab.id)
    }

    private var saveAndCloseRequestToken: Int {
        model.workspaceFileEditorSaveAndCloseRequestToken(for: tab.id)
    }

    var body: some View {
        VStack(spacing: 0) {
            toolbar
            content
        }
        .background(Color(nsColor: editorTheme.nsBackgroundColor))
        .onAppear {
            model.setWorkspaceFileTabDirty(tab.id, isDirty: isDirty)
            if isSelected {
                activateEditorKeyboard(requestFocus: true)
            }
        }
        .onChange(of: isDirty) { _, newValue in
            model.setWorkspaceFileTabDirty(tab.id, isDirty: newValue)
        }
        .onChange(of: isSelected) { _, newValue in
            if newValue {
                activateEditorKeyboard(requestFocus: true)
            }
        }
        .onChange(of: saveRequestToken) { _, _ in
            guard isSelected else {
                return
            }
            requestSave()
        }
        .onChange(of: saveAndCloseRequestToken) { _, _ in
            requestSaveAndClose()
        }
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(preview.title)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                Text(preview.subtitle.isEmpty ? tab.fileURL.path : preview.subtitle)
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }
            .frame(minWidth: 160, maxWidth: .infinity, alignment: .leading)

            if let statusMessage {
                Text(statusMessage)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
            }

            WorkspaceEditorToolbarButton(symbol: "checkmark.circle", isActive: isDirty, help: String(localized: "files.preview.save", defaultValue: "Save", bundle: .module)) {
                requestSave()
            }
            .disabled(!canEdit || !isDirty)
            .keyboardShortcut("s", modifiers: .command)

            WorkspaceEditorToolbarButton(symbol: "arrow.uturn.backward", help: String(localized: "files.preview.undo", defaultValue: "Undo", bundle: .module)) {
                editorUndoToken &+= 1
            }
            .disabled(!canEdit)

            WorkspaceEditorToolbarButton(symbol: "arrow.uturn.forward", help: String(localized: "files.preview.redo", defaultValue: "Redo", bundle: .module)) {
                editorRedoToken &+= 1
            }
            .disabled(!canEdit)

            WorkspaceEditorToolbarButton(symbol: "magnifyingglass", help: String(localized: "files.preview.find", defaultValue: "Find", bundle: .module)) {
                editorFindToken &+= 1
            }
            .disabled(!canEdit)

            WorkspaceEditorToolbarButton(symbol: "square.and.arrow.down", help: String(localized: "files.preview.save_as", defaultValue: "Save As", bundle: .module)) {
                saveAs()
            }
            .disabled(!canEdit)

            WorkspaceEditorToolbarButton(symbol: "arrow.clockwise", help: String(localized: "files.preview.reload", defaultValue: "Reload", bundle: .module)) {
                reloadIfAllowed()
            }

            WorkspaceEditorToolbarButton(symbol: "folder", help: String(localized: "files.preview.reveal_finder", defaultValue: "Reveal in Finder", bundle: .module)) {
                NSWorkspace.shared.activateFileViewerSelecting([tab.fileURL])
            }
        }
        .padding(.horizontal, 14)
        .frame(height: 48)
        .background(Color(nsColor: editorTheme.nsBackgroundColor).opacity(0.96))
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.36))
                .frame(height: 1)
        }
    }

    @ViewBuilder
    private var content: some View {
        switch preview.state {
        case .text:
            NativeSourceEditorTextPreview(
                text: $textContent,
                editorTheme: editorTheme,
                focusToken: editorFocusToken,
                renderToken: editorRenderToken,
                copyToken: editorCopyToken,
                pasteToken: editorPasteToken,
                undoToken: editorUndoToken,
                redoToken: editorRedoToken,
                findToken: editorFindToken,
                snapshotToken: editorSnapshotToken,
                markSavedToken: editorMarkSavedToken,
                fileURL: tab.fileURL,
                isLargeFileMode: isLargeFileMode,
                onFocused: { activateEditorKeyboard() },
                onDirtyChanged: { editorReportedDirty = $0 },
                onTextSnapshot: handleSnapshot,
                onSaveRequested: requestSave
            )
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .clipped()
        case .largeText(_):
            VStack(spacing: 10) {
                Image(systemName: "doc.text.magnifyingglass")
                    .font(.system(size: 28, weight: .semibold))
                    .foregroundStyle(AppTheme.textMuted)
                Text(String(localized: "files.preview.large_file_message", defaultValue: "This file is too large for inline editing.", bundle: .module))
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(AppTheme.textSecondary)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        case let .message(message):
            VStack(spacing: 10) {
                Image(systemName: "doc.badge.ellipsis")
                    .font(.system(size: 28, weight: .semibold))
                    .foregroundStyle(AppTheme.textMuted)
                Text(message)
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(AppTheme.textSecondary)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 360)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(24)
        }
    }

    private func requestSave() {
        guard canEdit else {
            return
        }
        pendingAction = .save
        editorSnapshotToken &+= 1
    }

    private func requestSaveAndClose() {
        guard canEdit else {
            return
        }
        pendingAction = .saveAndClose
        editorSnapshotToken &+= 1
    }

    private func activateEditorKeyboard(requestFocus: Bool = false) {
        guard isSelected else {
            return
        }
        FileBrowserKeyboardFocusState.activateWorkspaceFileEditor(tabID: tab.id)
        if requestFocus {
            editorFocusToken &+= 1
        }
    }

    private func saveAs() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = tab.fileURL.lastPathComponent
        panel.directoryURL = tab.fileURL.deletingLastPathComponent()
        guard panel.runModal() == .OK, let destinationURL = panel.url else {
            return
        }
        pendingAction = .saveAs(destinationURL)
        editorSnapshotToken &+= 1
    }

    private func handleSnapshot(_ content: String) {
        let action = pendingAction
        pendingAction = nil
        switch action {
        case .save:
            _ = save(content)
        case let .saveAs(destinationURL):
            do {
                try content.write(to: destinationURL, atomically: true, encoding: .utf8)
                statusMessage = String(
                    format: String(localized: "files.preview.saved_as_format", defaultValue: "Saved as %@", bundle: .module),
                    destinationURL.lastPathComponent
                )
            } catch {
                statusMessage = error.localizedDescription
            }
        case .saveAndClose:
            if save(content) {
                model.closeWorkspaceFileTabAfterSaving(tabID: tab.id)
            }
        case nil:
            break
        }
    }

    @discardableResult
    private func save(_ content: String) -> Bool {
        do {
            try ProjectFileBrowserService().saveText(content, to: tab.fileURL, rootURL: tab.rootURL)
            textContent = content
            savedTextContent = content
            editorReportedDirty = false
            editorMarkSavedToken &+= 1
            model.setWorkspaceFileTabDirty(tab.id, isDirty: false)
            statusMessage = String(localized: "files.preview.saved", defaultValue: "Saved", bundle: .module)
            return true
        } catch {
            statusMessage = String(
                format: String(localized: "files.preview.save_error_format", defaultValue: "Could not save: %@", bundle: .module),
                error.localizedDescription
            )
            return false
        }
    }

    private func reloadIfAllowed() {
        guard !isDirty || confirmDiscardChanges() else {
            return
        }
        let refreshedPreview = ProjectFileBrowserService().preview(for: tab.fileURL, rootURL: tab.rootURL)
        preview = refreshedPreview
        let refreshedText = Self.textContent(from: refreshedPreview) ?? ""
        textContent = refreshedText
        savedTextContent = refreshedText
        editorReportedDirty = false
        editorRenderToken &+= 1
        editorFocusToken &+= 1
        model.setWorkspaceFileTabDirty(tab.id, isDirty: false)
        statusMessage = nil
    }

    private static func textContent(from preview: ProjectFilePreview) -> String? {
        if case let .text(text) = preview.state {
            return text.string
        }
        return nil
    }

    private func confirmDiscardChanges() -> Bool {
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = String(localized: "files.preview.discard_changes.title", defaultValue: "Discard unsaved changes?", bundle: .module)
        alert.informativeText = String(localized: "files.preview.discard_changes.message", defaultValue: "This preview has edits that have not been saved to the original file.", bundle: .module)
        alert.addButton(withTitle: String(localized: "files.preview.discard_changes.discard", defaultValue: "Discard Changes", bundle: .module))
        alert.addButton(withTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module))
        return alert.runModal() == .alertFirstButtonReturn
    }
}

private struct WorkspaceEditorToolbarButton: View {
    let symbol: String
    var isActive: Bool = false
    let help: String
    let action: () -> Void
    @Environment(\.isEnabled) private var isEnabled
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            Image(systemName: symbol)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(foregroundColor)
                .frame(width: 28, height: 28)
                .background(backgroundColor)
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        }
        .buttonStyle(.plain)
        .help(help)
        .floatingTooltip(help, placement: .below)
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var foregroundColor: Color {
        guard isEnabled else {
            return AppTheme.textMuted
        }
        if isActive {
            return AppTheme.focus
        }
        return isHovered ? AppTheme.textPrimary : AppTheme.textSecondary
    }

    private var backgroundColor: Color {
        if isActive {
            return AppTheme.focus.opacity(0.15)
        }
        if isHovered {
            return Color(nsColor: .quaternarySystemFill).opacity(0.95)
        }
        return Color.clear
    }
}
