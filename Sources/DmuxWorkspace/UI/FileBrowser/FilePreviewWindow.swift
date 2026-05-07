import AppKit
import MarkdownUI
import SwiftUI

private extension Notification.Name {
    static let projectFilePreviewShouldEdit = Notification.Name("codux.projectFilePreviewShouldEdit")
}

enum ProjectFilePreviewWindowPresenter {
    @MainActor private static var controllers: [String: NSWindowController] = [:]

    @MainActor
    static func show(
        fileURL: URL,
        rootURL: URL,
        startsEditing: Bool = false,
        editorTheme: ProjectFileEditorTheme = ProjectFileEditorTheme()
    ) {
        let key = fileURL.standardizedFileURL.path
        if let controller = controllers[key] {
            controller.window?.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            if startsEditing {
                NotificationCenter.default.post(
                    name: .projectFilePreviewShouldEdit,
                    object: nil,
                    userInfo: ["path": key]
                )
            }
            return
        }

        let preview = ProjectFileBrowserService().preview(for: fileURL, rootURL: rootURL)
        let contentView = ProjectFilePreviewView(
            initialPreview: preview,
            fileURL: fileURL,
            rootURL: rootURL,
            startsEditing: startsEditing,
            editorTheme: editorTheme
        )
        let hostingController = NSHostingController(rootView: contentView)
        hostingController.view.wantsLayer = true
        hostingController.view.layer?.backgroundColor = NSColor.clear.cgColor
        let window = NSWindow(contentViewController: hostingController)
        window.title = preview.title
        window.identifier = AppWindowIdentifier.filePreview
        window.setContentSize(NSSize(width: 940, height: 640))
        window.minSize = NSSize(width: 740, height: 420)
        window.styleMask = [.titled, .closable, .miniaturizable, .resizable, .fullSizeContentView]
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

private struct ProjectFilePreviewView: View {
    @State private var preview: ProjectFilePreview
    @State private var textContent: String
    @State private var savedTextContent: String
    @State private var editorFocusToken = 0
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
    @State private var pendingSnapshotAction: PendingEditorSnapshotAction?
    let fileURL: URL
    let rootURL: URL
    let editorTheme: ProjectFileEditorTheme

    init(
        initialPreview: ProjectFilePreview,
        fileURL: URL,
        rootURL: URL,
        startsEditing: Bool = false,
        editorTheme: ProjectFileEditorTheme = ProjectFileEditorTheme()
    ) {
        self.fileURL = fileURL
        self.rootURL = rootURL
        self.editorTheme = editorTheme
        _preview = State(initialValue: initialPreview)
        _textContent = State(initialValue: initialPreview.textContent ?? "")
        _savedTextContent = State(initialValue: initialPreview.textContent ?? "")
        _editorFocusToken = State(initialValue: initialPreview.textContent != nil ? 1 : 0)
    }

    private var hasUnsavedChanges: Bool {
        canEdit && (editorReportedDirty || textContent != savedTextContent)
    }

    private var chromeBackgroundColor: NSColor {
        editorTheme.nsBackgroundColor.blended(withFraction: 0.58, of: .controlBackgroundColor)
            ?? editorTheme.nsBackgroundColor
    }

    var body: some View {
        ZStack(alignment: .top) {
            VStack(spacing: 0) {
                Color.clear
                    .frame(height: ProjectFilePreviewLayoutPolicy.chromeHeight)
                Color(nsColor: editorTheme.nsBackgroundColor)
            }
            .allowsHitTesting(false)
            .zIndex(-1)

            previewContent
                .padding(.top, ProjectFilePreviewLayoutPolicy.chromeHeight)
                .clipped()
                .zIndex(0)

            toolbar
                .zIndex(2)
        }
        .background(FilePreviewWindowConfigurator(backgroundColor: chromeBackgroundColor))
        .background(FilePreviewCloseGuard(hasUnsavedChanges: hasUnsavedChanges))
        .ignoresSafeArea(.container, edges: .top)
        .onReceive(NotificationCenter.default.publisher(for: .projectFilePreviewShouldEdit)) { notification in
            guard let path = notification.userInfo?["path"] as? String,
                  path == fileURL.standardizedFileURL.path,
                  canEdit else {
                return
            }
            focusEditor()
        }
        .onChange(of: textContent) { _, newValue in
            if newValue != savedTextContent {
                statusMessage = nil
            }
        }
    }

    private var previewContent: some View {
        Group {
            switch preview.state {
            case .text:
                if ProjectFilePreviewLayoutPolicy.usesMarkdownSplitPreview(fileExtension: fileURL.pathExtension) {
                    MarkdownSplitPreview(
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
                        fileExtension: fileURL.pathExtension,
                        sourceURL: fileURL,
                        isLargeFileMode: isLargeFileMode,
                        onDirtyChanged: { editorReportedDirty = $0 },
                        onTextSnapshot: handleEditorSnapshot,
                        onSaveRequested: requestSave
                    )
                } else {
                    CodeMirrorTextPreview(
                        text: $textContent,
                        colorScheme: editorTheme.colorScheme,
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
                        fileExtension: fileURL.pathExtension,
                        isLargeFileMode: isLargeFileMode,
                        onDirtyChanged: { editorReportedDirty = $0 },
                        onTextSnapshot: handleEditorSnapshot,
                        onSaveRequested: requestSave
                    )
                }
            case let .largeText(metadata):
                LargeFileVirtualTextPreview(
                    fileURL: fileURL,
                    fileExtension: fileURL.pathExtension,
                    metadata: metadata
                )
            case let .message(message):
                VStack(spacing: 10) {
                    Image(systemName: "doc.badge.ellipsis")
                        .font(.system(size: 30, weight: .semibold))
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
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            HStack(spacing: 6) {
                Text(preview.title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.middle)
                    .help(preview.subtitle.isEmpty ? fileURL.path : preview.subtitle)

                if hasUnsavedChanges {
                    Circle()
                        .fill(AppTheme.warning)
                        .frame(width: 6, height: 6)
                        .help(String(localized: "files.preview.unsaved_changes", defaultValue: "Unsaved changes", bundle: .module))
                }
            }
            .frame(minWidth: 120, maxWidth: .infinity, alignment: .leading)

            if let statusMessage {
                Text(statusMessage)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
                    .padding(.horizontal, 7)
                    .frame(height: 22)
                    .background(
                        Capsule(style: .continuous)
                            .fill(Color(nsColor: .controlBackgroundColor).opacity(0.72))
                    )
            }

            Button {
                requestSave()
            } label: {
                Image(systemName: "checkmark.circle")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle(isActive: hasUnsavedChanges))
            .disabled(canEdit == false || hasUnsavedChanges == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.save", defaultValue: "Save", bundle: .module))
            .keyboardShortcut("s", modifiers: .command)

            Button {
                editorUndoToken &+= 1
            } label: {
                Image(systemName: "arrow.uturn.backward")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .disabled(canEdit == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.undo", defaultValue: "Undo", bundle: .module))

            Button {
                editorRedoToken &+= 1
            } label: {
                Image(systemName: "arrow.uturn.forward")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .disabled(canEdit == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.redo", defaultValue: "Redo", bundle: .module))

            Button {
                editorCopyToken &+= 1
            } label: {
                Image(systemName: "doc.on.doc")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .disabled(canEdit == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.copy", defaultValue: "Copy", bundle: .module))

            Button {
                editorPasteToken &+= 1
            } label: {
                Image(systemName: "doc.on.clipboard")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .disabled(canEdit == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.paste", defaultValue: "Paste", bundle: .module))

            Button {
                editorFindToken &+= 1
            } label: {
                Image(systemName: "magnifyingglass")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .disabled(canEdit == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.find", defaultValue: "Find", bundle: .module))

            Button {
                saveAs()
            } label: {
                Image(systemName: "square.and.arrow.down")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .disabled(canEdit == false)
            .filePreviewToolbarHelp(String(localized: "files.preview.save_as", defaultValue: "Save As", bundle: .module))

            Button {
                reloadPreviewIfAllowed()
            } label: {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .filePreviewToolbarHelp(String(localized: "files.preview.reload", defaultValue: "Reload", bundle: .module))

            Button {
                copyPath()
            } label: {
                Image(systemName: "link")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .filePreviewToolbarHelp(String(localized: "files.preview.copy_path", defaultValue: "Copy Path", bundle: .module))

            Button {
                NSWorkspace.shared.activateFileViewerSelecting([fileURL])
            } label: {
                Image(systemName: "folder")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .filePreviewToolbarHelp(String(localized: "files.preview.reveal_finder", defaultValue: "Reveal in Finder", bundle: .module))
        }
        .padding(.trailing, 14)
        .padding(.leading, 86)
        .frame(height: ProjectFilePreviewLayoutPolicy.chromeHeight)
        .background(alignment: .trailing) {
            Color(nsColor: chromeBackgroundColor)
                .padding(.leading, 76)
        }
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.42))
                .frame(height: 1)
        }
    }

    private var canEdit: Bool {
        if case .text = preview.state {
            return true
        }
        return false
    }

    private var isLargeFileMode: Bool {
        textContent.utf8.count > ProjectFilePreviewLayoutPolicy.largeCodeMirrorModeBytes
    }

    private func focusEditor() {
        guard canEdit else {
            return
        }
        editorFocusToken &+= 1
    }

    private func requestSave() {
        guard canEdit else {
            return
        }
        pendingSnapshotAction = .save
        editorSnapshotToken &+= 1
    }

    private func save(_ content: String) {
        do {
            try ProjectFileBrowserService().saveText(content, to: fileURL, rootURL: rootURL)
            textContent = content
            savedTextContent = content
            editorReportedDirty = false
            editorMarkSavedToken &+= 1
            statusMessage = String(localized: "files.preview.saved", defaultValue: "Saved", bundle: .module)
        } catch {
            statusMessage = String(
                format: String(localized: "files.preview.save_error_format", defaultValue: "Could not save: %@", bundle: .module),
                error.localizedDescription
            )
        }
    }

    private func saveAs() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = fileURL.lastPathComponent
        panel.directoryURL = fileURL.deletingLastPathComponent()
        guard panel.runModal() == .OK, let destinationURL = panel.url else {
            return
        }
        pendingSnapshotAction = .saveAs(destinationURL)
        editorSnapshotToken &+= 1
    }

    private func saveAs(_ content: String, to destinationURL: URL) {
        do {
            try content.write(to: destinationURL, atomically: true, encoding: .utf8)
            statusMessage = String(
                format: String(localized: "files.preview.saved_as_format", defaultValue: "Saved as %@", bundle: .module),
                destinationURL.lastPathComponent
            )
        } catch {
            statusMessage = String(
                format: String(localized: "files.preview.save_error_format", defaultValue: "Could not save: %@", bundle: .module),
                error.localizedDescription
            )
        }
    }

    private func reloadPreviewIfAllowed() {
        guard confirmDiscardChangesIfNeeded() else {
            return
        }
        reloadPreview()
    }

    private func reloadPreview() {
        let refreshedPreview = ProjectFileBrowserService().preview(for: fileURL, rootURL: rootURL)
        preview = refreshedPreview
        textContent = refreshedPreview.textContent ?? ""
        savedTextContent = refreshedPreview.textContent ?? ""
        editorReportedDirty = false
        editorFocusToken &+= 1
        editorRenderToken &+= 1
        statusMessage = nil
    }

    private func confirmDiscardChangesIfNeeded() -> Bool {
        guard hasUnsavedChanges else {
            return true
        }
        let alert = NSAlert()
        alert.alertStyle = .warning
        alert.messageText = String(localized: "files.preview.discard_changes.title", defaultValue: "Discard unsaved changes?", bundle: .module)
        alert.informativeText = String(localized: "files.preview.discard_changes.message", defaultValue: "This preview has edits that have not been saved to the original file.", bundle: .module)
        alert.addButton(withTitle: String(localized: "files.preview.discard_changes.discard", defaultValue: "Discard Changes", bundle: .module))
        alert.addButton(withTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module))
        return alert.runModal() == .alertFirstButtonReturn
    }

    private func copyPath() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(fileURL.path, forType: .string)
        statusMessage = String(localized: "files.preview.path_copied", defaultValue: "Path copied", bundle: .module)
    }

    private func handleEditorSnapshot(_ content: String) {
        let action = pendingSnapshotAction
        pendingSnapshotAction = nil
        switch action {
        case .save:
            save(content)
        case let .saveAs(destinationURL):
            saveAs(content, to: destinationURL)
        case nil:
            break
        }
    }
}

private enum PendingEditorSnapshotAction {
    case save
    case saveAs(URL)
}

private struct FilePreviewWindowConfigurator: NSViewRepresentable {
    let backgroundColor: NSColor

    func makeNSView(context: Context) -> NSView {
        ConfigView(backgroundColor: backgroundColor)
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        guard let view = nsView as? ConfigView else { return }
        view.backgroundColor = backgroundColor
        view.applyWindowConfigurationIfNeeded()
    }

    private final class ConfigView: NSView {
        var backgroundColor: NSColor
        private var hasScheduledDeferredConfiguration = false

        init(backgroundColor: NSColor) {
            self.backgroundColor = backgroundColor
            super.init(frame: .zero)
        }

        @available(*, unavailable)
        required init?(coder: NSCoder) {
            fatalError("init(coder:) has not been implemented")
        }

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            applyWindowConfigurationIfNeeded()
        }

        func applyWindowConfigurationIfNeeded() {
            guard let window else {
                return
            }
            window.identifier = AppWindowIdentifier.filePreview
            applyFilePreviewWindowChrome(window)
            window.backgroundColor = backgroundColor
            window.contentView?.wantsLayer = true
            window.contentView?.layer?.backgroundColor = NSColor.clear.cgColor
            guard hasScheduledDeferredConfiguration == false else {
                return
            }
            hasScheduledDeferredConfiguration = true
            Task { @MainActor [weak window] in
                guard let window else {
                    return
                }
                applyFilePreviewWindowChrome(window)
                window.backgroundColor = backgroundColor
                window.contentView?.wantsLayer = true
                window.contentView?.layer?.backgroundColor = NSColor.clear.cgColor
            }
        }
    }
}

private struct FilePreviewCloseGuard: NSViewRepresentable {
    let hasUnsavedChanges: Bool

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> CloseGuardView {
        let view = CloseGuardView()
        view.delegate = context.coordinator
        return view
    }

    func updateNSView(_ view: CloseGuardView, context: Context) {
        context.coordinator.hasUnsavedChanges = hasUnsavedChanges
        view.delegate = context.coordinator
        view.attachToWindowIfNeeded()
    }

    final class CloseGuardView: NSView {
        weak var delegate: NSWindowDelegate?
        private weak var installedWindow: NSWindow?

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            attachToWindowIfNeeded()
        }

        func attachToWindowIfNeeded() {
            guard let window, let delegate else {
                return
            }
            guard installedWindow !== window || window.delegate !== delegate else {
                return
            }
            window.delegate = delegate
            installedWindow = window
        }
    }

    final class Coordinator: NSObject, NSWindowDelegate {
        var hasUnsavedChanges = false

        func windowShouldClose(_ sender: NSWindow) -> Bool {
            guard hasUnsavedChanges else {
                return true
            }
            let alert = NSAlert()
            alert.alertStyle = .warning
            alert.messageText = String(localized: "files.preview.discard_changes.title", defaultValue: "Discard unsaved changes?", bundle: .module)
            alert.informativeText = String(localized: "files.preview.discard_changes.message", defaultValue: "This preview has edits that have not been saved to the original file.", bundle: .module)
            alert.addButton(withTitle: String(localized: "files.preview.discard_changes.discard", defaultValue: "Discard Changes", bundle: .module))
            alert.addButton(withTitle: String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module))
            return alert.runModal() == .alertFirstButtonReturn
        }
    }
}

enum ProjectFilePreviewLayoutPolicy {
    static let chromeHeight: CGFloat = 44
    static let largeCodeMirrorModeBytes = 20 * 1024 * 1024

    static func usesMarkdownSplitPreview(fileExtension: String) -> Bool {
        switch fileExtension.lowercased() {
        case "md", "markdown":
            return true
        default:
            return false
        }
    }
}

private struct FilePreviewToolbarButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled
    let isActive: Bool

    init(isActive: Bool = false) {
        self.isActive = isActive
    }

    func makeBody(configuration: Configuration) -> some View {
        FilePreviewToolbarButtonBody(
            configuration: configuration,
            isEnabled: isEnabled,
            isActive: isActive
        )
    }
}

private struct FilePreviewToolbarButtonBody: View {
    let configuration: ButtonStyle.Configuration
    let isEnabled: Bool
    let isActive: Bool
    @State private var isHovered = false

    var body: some View {
        configuration.label
            .foregroundStyle(foregroundColor)
            .frame(width: 28, height: 28)
            .background(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(backgroundColor)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .strokeBorder(borderColor, lineWidth: 0.5)
            )
            .contentShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
            .opacity(isEnabled ? 1 : 0.42)
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
        return isHovered || configuration.isPressed ? AppTheme.textPrimary : AppTheme.textSecondary
    }

    private var backgroundColor: Color {
        if isActive {
            return AppTheme.focus.opacity(configuration.isPressed ? 0.22 : 0.15)
        }
        if configuration.isPressed {
            return Color(nsColor: .tertiarySystemFill).opacity(0.95)
        }
        if isHovered {
            return Color(nsColor: .quaternarySystemFill).opacity(0.95)
        }
        return Color.clear
    }

    private var borderColor: Color {
        if isActive {
            return AppTheme.focus.opacity(0.32)
        }
        if isHovered || configuration.isPressed {
            return Color(nsColor: .separatorColor).opacity(0.36)
        }
        return Color.clear
    }
}

private struct FilePreviewToolbarHelpModifier: ViewModifier {
    let text: String

    func body(content: Content) -> some View {
        content
            .help(text)
            .floatingTooltip(text, placement: .below)
    }
}

private extension View {
    func filePreviewToolbarHelp(_ text: String) -> some View {
        modifier(FilePreviewToolbarHelpModifier(text: text))
    }
}

private struct MarkdownSplitPreview: View {
    @Binding var text: String
    let editorTheme: ProjectFileEditorTheme
    let focusToken: Int
    let renderToken: Int
    let copyToken: Int
    let pasteToken: Int
    let undoToken: Int
    let redoToken: Int
    let findToken: Int
    let snapshotToken: Int
    let markSavedToken: Int
    let fileExtension: String
    let sourceURL: URL
    let isLargeFileMode: Bool
    let onDirtyChanged: (Bool) -> Void
    let onTextSnapshot: (String) -> Void
    let onSaveRequested: () -> Void

    var body: some View {
        HStack(spacing: 0) {
            CodeMirrorTextPreview(
                text: $text,
                colorScheme: editorTheme.colorScheme,
                editorTheme: editorTheme,
                focusToken: focusToken,
                renderToken: renderToken,
                copyToken: copyToken,
                pasteToken: pasteToken,
                undoToken: undoToken,
                redoToken: redoToken,
                findToken: findToken,
                snapshotToken: snapshotToken,
                markSavedToken: markSavedToken,
                fileExtension: fileExtension,
                isLargeFileMode: isLargeFileMode,
                onDirtyChanged: onDirtyChanged,
                onTextSnapshot: onTextSnapshot,
                onSaveRequested: onSaveRequested
            )
            .frame(minWidth: 260)

            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.42))
                .frame(width: 1)

            MarkdownRenderedPreview(text: $text, sourceURL: sourceURL)
                .frame(minWidth: 260)
        }
    }
}

private struct MarkdownRenderedPreview: View {
    @Binding var text: String
    let sourceURL: URL
    @State private var renderedContent: MarkdownContent

    init(text: Binding<String>, sourceURL: URL) {
        self._text = text
        self.sourceURL = sourceURL
        _renderedContent = State(initialValue: MarkdownContent(text.wrappedValue))
    }

    var body: some View {
        ScrollView {
            Markdown(renderedContent, baseURL: sourceURL.deletingLastPathComponent(), imageBaseURL: sourceURL.deletingLastPathComponent())
                .markdownTheme(.gitHub)
                .markdownImageProvider(ProjectFilePreviewMarkdownImageProvider())
                .markdownInlineImageProvider(ProjectFilePreviewMarkdownInlineImageProvider())
                .textSelection(.enabled)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 22)
                .padding(.vertical, 20)
        }
        .background(Color(nsColor: .textBackgroundColor).opacity(0.24))
        .onChange(of: text) { _, newValue in
            renderedContent = MarkdownContent(newValue)
        }
    }
}

private struct ProjectFilePreviewMarkdownImageProvider: ImageProvider {
    func makeImage(url: URL?) -> some View {
        if let image = ProjectFilePreviewMarkdownImageLoader.image(for: url) {
            Image(nsImage: image)
                .resizable()
                .scaledToFit()
        } else {
            Color.clear
                .frame(width: 0, height: 0)
        }
    }
}

private struct ProjectFilePreviewMarkdownInlineImageProvider: InlineImageProvider {
    func image(with url: URL, label: String) async throws -> Image {
        if let image = ProjectFilePreviewMarkdownImageLoader.image(for: url) {
            return Image(nsImage: image)
        }
        return Image(nsImage: NSImage(size: .zero))
    }
}

private enum ProjectFilePreviewMarkdownImageLoader {
    static func image(for url: URL?) -> NSImage? {
        guard let url, url.isFileURL else {
            return nil
        }
        return NSImage(contentsOf: url)
    }
}

private struct LargeFileVirtualTextPreview: NSViewRepresentable {
    let fileURL: URL
    let fileExtension: String
    let metadata: ProjectLargeFilePreview

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> NSScrollView {
        let contentView = LargeFileVirtualTextView(
            fileURL: fileURL,
            fileExtension: fileExtension,
            metadata: metadata
        )
        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.documentView = contentView
        scrollView.contentView.postsBoundsChangedNotifications = true
        context.coordinator.bind(scrollView: scrollView, contentView: contentView)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let contentView = scrollView.documentView as? LargeFileVirtualTextView else {
            return
        }
        contentView.update(fileURL: fileURL, fileExtension: fileExtension, metadata: metadata)
        context.coordinator.bind(scrollView: scrollView, contentView: contentView)
    }

    final class Coordinator {
        private weak var observedClipView: NSClipView?
        private weak var contentView: LargeFileVirtualTextView?
        private var observer: NSObjectProtocol?

        deinit {
            if let observer {
                NotificationCenter.default.removeObserver(observer)
            }
        }

        @MainActor
        func bind(scrollView: NSScrollView, contentView: LargeFileVirtualTextView) {
            guard observedClipView !== scrollView.contentView || self.contentView !== contentView else {
                return
            }
            if let observer {
                NotificationCenter.default.removeObserver(observer)
            }
            observedClipView = scrollView.contentView
            self.contentView = contentView
            observer = NotificationCenter.default.addObserver(
                forName: NSView.boundsDidChangeNotification,
                object: scrollView.contentView,
                queue: .main
            ) { [weak contentView] _ in
                Task { @MainActor in
                    contentView?.visibleBoundsDidChange()
                }
            }
            contentView.visibleBoundsDidChange()
        }
    }
}

private final class LargeFileVirtualTextView: NSView {
    private struct LoadedWindow {
        var firstLine: Int
        var lines: [String]

        func contains(lineRange: Range<Int>) -> Bool {
            lineRange.lowerBound >= firstLine && lineRange.upperBound <= firstLine + lines.count
        }
    }

    private var fileURL: URL
    private var fileExtension: String
    private var metadata: ProjectLargeFilePreview
    private var loadedWindow: LoadedWindow?
    private var requestedLineRange: Range<Int>?
    private let readQueue = DispatchQueue(label: "Codux.LargeFilePreview.Read", qos: .userInitiated)
    private var readGeneration = 0
    private let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
    private let lineHeight: CGFloat = 17
    private let rulerWidth: CGFloat = 66
    private let textInsetX: CGFloat = 12
    private let textInsetY: CGFloat = 12
    private let visiblePrefetchLines = 260
    private let maxReadBytes = 768 * 1024

    init(fileURL: URL, fileExtension: String, metadata: ProjectLargeFilePreview) {
        self.fileURL = fileURL
        self.fileExtension = fileExtension
        self.metadata = metadata
        super.init(frame: .zero)
        wantsLayer = true
        updateFrameSize()
    }

    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var isFlipped: Bool { true }

    func update(fileURL: URL, fileExtension: String, metadata: ProjectLargeFilePreview) {
        let needsReload = self.fileURL != fileURL || self.metadata != metadata
        self.fileURL = fileURL
        self.fileExtension = fileExtension
        self.metadata = metadata
        updateFrameSize()
        if needsReload {
            loadedWindow = nil
            requestedLineRange = nil
            readGeneration &+= 1
            needsDisplay = true
            visibleBoundsDidChange()
        }
    }

    func visibleBoundsDidChange() {
        needsDisplay = true
        scheduleReadForVisibleRange()
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.textBackgroundColor.withAlphaComponent(0.24).setFill()
        bounds.fill()

        let visibleRange = visibleLineRange(expandingBy: 1)
        let attributes: [NSAttributedString.Key: Any] = [
            .font: font,
            .foregroundColor: NSColor.labelColor,
        ]
        let lineNumberAttributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular),
            .foregroundColor: NSColor.tertiaryLabelColor,
        ]

        NSColor.separatorColor.withAlphaComponent(0.32).setFill()
        NSRect(x: rulerWidth - 1, y: dirtyRect.minY, width: 1, height: dirtyRect.height).fill()

        for lineIndex in visibleRange {
            let y = textInsetY + CGFloat(lineIndex) * lineHeight
            let lineNumber = "\(lineIndex + 1)" as NSString
            lineNumber.draw(
                in: NSRect(x: 0, y: y, width: rulerWidth - 12, height: lineHeight),
                withAttributes: lineNumberAttributes
            )

            let text = loadedLine(at: lineIndex) ?? placeholderLine(for: lineIndex)
            let lineText = (text as NSString)
            lineText.draw(
                in: NSRect(x: rulerWidth + textInsetX, y: y, width: max(2000, bounds.width - rulerWidth - textInsetX), height: lineHeight),
                withAttributes: attributes
            )
        }
    }

    private func updateFrameSize() {
        let height = textInsetY * 2 + CGFloat(max(1, metadata.estimatedLineCount)) * lineHeight
        frame = NSRect(x: 0, y: 0, width: max(2400, frame.width), height: min(height, 40_000_000))
    }

    private func visibleLineRange(expandingBy extraLines: Int) -> Range<Int> {
        let visibleRect = enclosingScrollView?.contentView.documentVisibleRect ?? bounds
        let first = max(0, Int(floor((visibleRect.minY - textInsetY) / lineHeight)) - extraLines)
        let count = max(1, Int(ceil(visibleRect.height / lineHeight)) + extraLines * 2)
        let last = min(max(1, metadata.estimatedLineCount), first + count)
        return first..<max(first + 1, last)
    }

    private func scheduleReadForVisibleRange() {
        let visibleRange = visibleLineRange(expandingBy: visiblePrefetchLines)
        if loadedWindow?.contains(lineRange: visibleRange) == true {
            return
        }
        guard requestedLineRange != visibleRange else {
            return
        }
        requestedLineRange = visibleRange
        readGeneration &+= 1
        let generation = readGeneration
        let fileURL = fileURL
        let averageBytesPerLine = metadata.averageBytesPerLine
        let totalBytes = metadata.totalBytes
        let maxReadBytes = maxReadBytes
        let readStartLine = visibleRange.lowerBound
        let lineLimit = max(visibleRange.count, 1)

        readQueue.async { [weak self] in
            let startOffset = UInt64(max(0, floor(Double(readStartLine) * averageBytesPerLine)))
            let estimatedByteCount = Int(ceil(Double(lineLimit + 120) * averageBytesPerLine))
            let byteCount = max(64 * 1024, min(maxReadBytes, estimatedByteCount))
            let lines = Self.readLines(
                fileURL: fileURL,
                startOffset: min(startOffset, totalBytes),
                byteCount: byteCount,
                lineLimit: lineLimit + 120,
                dropsLeadingPartialLine: startOffset > 0
            )
            DispatchQueue.main.async {
                guard let self, generation == self.readGeneration else {
                    return
                }
                self.loadedWindow = LoadedWindow(firstLine: readStartLine, lines: lines)
                self.needsDisplay = true
            }
        }
    }

    private func loadedLine(at line: Int) -> String? {
        guard let loadedWindow else {
            return nil
        }
        let offset = line - loadedWindow.firstLine
        guard offset >= 0, offset < loadedWindow.lines.count else {
            return nil
        }
        return loadedWindow.lines[offset]
    }

    private func placeholderLine(for line: Int) -> String {
        line == 0 ? metadata.message : ""
    }

    nonisolated private static func readLines(
        fileURL: URL,
        startOffset: UInt64,
        byteCount: Int,
        lineLimit: Int,
        dropsLeadingPartialLine: Bool
    ) -> [String] {
        guard let handle = try? FileHandle(forReadingFrom: fileURL) else {
            return []
        }
        defer { try? handle.close() }
        do {
            try handle.seek(toOffset: startOffset)
            let data = try handle.read(upToCount: byteCount) ?? Data()
            guard data.isEmpty == false else {
                return []
            }
            var text = String(decoding: data, as: UTF8.self)
            if dropsLeadingPartialLine, let newline = text.firstIndex(of: "\n") {
                text = String(text[text.index(after: newline)...])
            }
            return Array(text.split(separator: "\n", omittingEmptySubsequences: false).prefix(lineLimit)).map(String.init)
        } catch {
            return []
        }
    }
}

private extension ProjectFilePreview {
    var textContent: String? {
        if case let .text(text) = state {
            return text.string
        }
        return nil
    }
}
