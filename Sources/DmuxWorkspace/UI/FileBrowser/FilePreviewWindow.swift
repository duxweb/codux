import AppKit
import SwiftUI

private extension Notification.Name {
    static let projectFilePreviewShouldEdit = Notification.Name("codux.projectFilePreviewShouldEdit")
}

enum ProjectFilePreviewWindowPresenter {
    @MainActor private static var controllers: [String: NSWindowController] = [:]

    @MainActor
    static func show(fileURL: URL, rootURL: URL, startsEditing: Bool = false) {
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
            startsEditing: startsEditing
        )
        let hostingController = NSHostingController(rootView: contentView)
        let window = NSWindow(contentViewController: hostingController)
        window.title = preview.title
        window.identifier = AppWindowIdentifier.filePreview
        window.setContentSize(NSSize(width: 860, height: 620))
        window.minSize = NSSize(width: 560, height: 360)
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

private struct ProjectFilePreviewView: View {
    @State private var preview: ProjectFilePreview
    @State private var textContent: String
    @State private var isEditing = false
    @State private var statusMessage: String?
    let fileURL: URL
    let rootURL: URL

    init(initialPreview: ProjectFilePreview, fileURL: URL, rootURL: URL, startsEditing: Bool = false) {
        self.fileURL = fileURL
        self.rootURL = rootURL
        _preview = State(initialValue: initialPreview)
        _textContent = State(initialValue: initialPreview.textContent ?? "")
        _isEditing = State(initialValue: startsEditing && initialPreview.textContent != nil)
    }

    var body: some View {
        VStack(spacing: 0) {
            toolbar
                .zIndex(2)

            Group {
                switch preview.state {
                case let .text(attributedText):
                    EditableTextPreview(
                        text: $textContent,
                        isEditing: isEditing,
                        fileExtension: fileURL.pathExtension,
                        highlightedText: attributedText
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
            .clipped()
            .zIndex(0)
        }
        .background(Color(nsColor: .windowBackgroundColor))
        .onReceive(NotificationCenter.default.publisher(for: .projectFilePreviewShouldEdit)) { notification in
            guard let path = notification.userInfo?["path"] as? String,
                  path == fileURL.standardizedFileURL.path,
                  canEdit else {
                return
            }
            isEditing = true
        }
    }

    private var toolbar: some View {
        HStack(spacing: 8) {
            Button {
                isEditing.toggle()
            } label: {
                Image(systemName: "pencil")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle(isActive: isEditing))
            .help(String(localized: "files.preview.edit_mode", defaultValue: "Edit Mode", bundle: .module))
            .disabled(canEdit == false)

            Button {
                saveAs()
            } label: {
                Image(systemName: "square.and.arrow.down")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .help(String(localized: "files.preview.save_as", defaultValue: "Save As", bundle: .module))
            .disabled(canEdit == false)

            Button {
                reloadPreview()
            } label: {
                Image(systemName: "arrow.clockwise")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .help(String(localized: "files.preview.reload", defaultValue: "Reload", bundle: .module))

            Button {
                copyPath()
            } label: {
                Image(systemName: "doc.on.doc")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .help(String(localized: "files.preview.copy_path", defaultValue: "Copy Path", bundle: .module))

            Button {
                NSWorkspace.shared.activateFileViewerSelecting([fileURL])
            } label: {
                Image(systemName: "folder")
                    .font(.system(size: 12, weight: .semibold))
            }
            .buttonStyle(FilePreviewToolbarButtonStyle())
            .help(String(localized: "files.preview.reveal_finder", defaultValue: "Reveal in Finder", bundle: .module))

            Spacer(minLength: 0)

            if let statusMessage {
                Text(statusMessage)
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
                    .padding(.horizontal, 8)
                    .frame(height: 24)
                    .background(
                        Capsule(style: .continuous)
                            .fill(Color(nsColor: .controlBackgroundColor).opacity(0.72))
                    )
            }
        }
        .padding(.horizontal, 16)
        .frame(height: 48)
        .background {
            ZStack {
                Color(nsColor: .windowBackgroundColor)
                Color(nsColor: .controlBackgroundColor).opacity(0.72)
            }
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

    private func saveAs() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = fileURL.lastPathComponent
        panel.directoryURL = fileURL.deletingLastPathComponent()
        guard panel.runModal() == .OK, let destinationURL = panel.url else {
            return
        }
        do {
            try textContent.write(to: destinationURL, atomically: true, encoding: .utf8)
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

    private func reloadPreview() {
        let refreshedPreview = ProjectFileBrowserService().preview(for: fileURL, rootURL: rootURL)
        preview = refreshedPreview
        textContent = refreshedPreview.textContent ?? ""
        isEditing = false
        statusMessage = nil
    }

    private func copyPath() {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(fileURL.path, forType: .string)
        statusMessage = String(localized: "files.preview.path_copied", defaultValue: "Path copied", bundle: .module)
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

private struct EditableTextPreview: NSViewRepresentable {
    @Binding var text: String
    let isEditing: Bool
    let fileExtension: String
    let highlightedText: NSAttributedString?

    func makeCoordinator() -> Coordinator {
        Coordinator(text: $text)
    }

    func makeNSView(context: Context) -> NSScrollView {
        let textView = NSTextView()
        textView.isSelectable = true
        textView.drawsBackground = false
        textView.delegate = context.coordinator
        textView.textContainerInset = NSSize(width: 14, height: 14)
        textView.textContainer?.widthTracksTextView = false
        textView.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.autoresizingMask = [.width]

        let scrollView = NSScrollView()
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.drawsBackground = false
        scrollView.documentView = textView
        let rulerView = TextLineNumberRulerView(textView: textView, scrollView: scrollView)
        scrollView.verticalRulerView = rulerView
        scrollView.hasVerticalRuler = true
        scrollView.rulersVisible = true
        scrollView.contentView.postsBoundsChangedNotifications = true
        context.coordinator.rulerView = rulerView
        context.coordinator.bind(scrollView: scrollView)
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else {
            return
        }
        textView.isEditable = isEditing
        textView.insertionPointColor = .labelColor
        context.coordinator.text = $text

        if isEditing {
            guard textView.string != text else {
                context.coordinator.lastRenderedText = text
                context.coordinator.lastRenderedEditingState = isEditing
                return
            }
            context.coordinator.isApplyingUpdate = true
            textView.string = text
            textView.font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
            textView.textColor = .labelColor
            context.coordinator.isApplyingUpdate = false
        } else {
            guard context.coordinator.lastRenderedText != text || context.coordinator.lastRenderedEditingState != isEditing else {
                return
            }
            context.coordinator.isApplyingUpdate = true
            let renderedText = highlightedText?.string == text
                ? highlightedText ?? NSAttributedString(string: text)
                : ProjectFileSyntaxHighlighter.highlight(text: text, fileExtension: fileExtension)
            textView.textStorage?.setAttributedString(renderedText)
            context.coordinator.isApplyingUpdate = false
        }
        context.coordinator.lastRenderedText = text
        context.coordinator.lastRenderedEditingState = isEditing
        context.coordinator.rulerView?.rebuildLineNumberCacheIfNeeded(for: textView.string)
        context.coordinator.rulerView?.refreshLineNumbers(invalidateLayout: true)
    }

    final class Coordinator: NSObject, NSTextViewDelegate {
        var text: Binding<String>
        var isApplyingUpdate = false
        var lastRenderedText = ""
        var lastRenderedEditingState = false
        weak var rulerView: TextLineNumberRulerView?
        private weak var observedClipView: NSClipView?

        init(text: Binding<String>) {
            self.text = text
        }

        deinit {
            NotificationCenter.default.removeObserver(self)
        }

        @MainActor
        func bind(scrollView: NSScrollView) {
            guard observedClipView !== scrollView.contentView else {
                return
            }
            NotificationCenter.default.removeObserver(self)
            let clipView = scrollView.contentView
            clipView.postsBoundsChangedNotifications = true
            observedClipView = clipView
            NotificationCenter.default.addObserver(
                self,
                selector: #selector(clipViewBoundsDidChange(_:)),
                name: NSView.boundsDidChangeNotification,
                object: clipView
            )
        }

        @MainActor
        @objc private func clipViewBoundsDidChange(_ notification: Notification) {
            rulerView?.refreshLineNumbers()
        }

        func textDidChange(_ notification: Notification) {
            guard isApplyingUpdate == false,
                  let textView = notification.object as? NSTextView else {
                return
            }
            text.wrappedValue = textView.string
            lastRenderedText = textView.string
            rulerView?.rebuildLineNumberCacheIfNeeded(for: textView.string)
            rulerView?.refreshLineNumbers(invalidateLayout: true)
        }
    }
}

private final class TextLineNumberRulerView: NSRulerView {
    private weak var textView: NSTextView?
    private var cachedLineText = ""
    private var lineStartUTF16Offsets: [Int] = [0]
    private let paragraphStyle: NSMutableParagraphStyle = {
        let style = NSMutableParagraphStyle()
        style.alignment = .right
        return style
    }()

    init(textView: NSTextView, scrollView: NSScrollView) {
        self.textView = textView
        super.init(scrollView: scrollView, orientation: .verticalRuler)
        clientView = textView
        ruleThickness = 50
    }

    required init(coder: NSCoder) {
        super.init(coder: coder)
    }

    @MainActor
    func rebuildLineNumberCacheIfNeeded(for text: String) {
        guard cachedLineText != text else {
            return
        }
        cachedLineText = text
        var starts = [0]
        starts.reserveCapacity(max(1, text.count / 40))
        var offset = 0
        for codeUnit in text.utf16 {
            offset += 1
            if codeUnit == 10 {
                starts.append(offset)
            }
        }
        lineStartUTF16Offsets = starts
    }

    @MainActor
    func refreshLineNumbers(invalidateLayout: Bool = false) {
        if invalidateLayout {
            invalidateHashMarks()
        }
        needsDisplay = true
    }

    override func drawHashMarksAndLabels(in rect: NSRect) {
        guard let textView,
              let layoutManager = textView.layoutManager,
              let textContainer = textView.textContainer else {
            return
        }

        let separatorRect = NSRect(x: rect.maxX - 1, y: rect.minY, width: 1, height: rect.height)
        NSColor.separatorColor.withAlphaComponent(0.32).setFill()
        separatorRect.fill()

        let containerOrigin = textView.textContainerOrigin
        let visibleRect = textView.visibleRect
        let visibleContainerRect = NSRect(
            x: visibleRect.minX - containerOrigin.x,
            y: visibleRect.minY - containerOrigin.y,
            width: visibleRect.width,
            height: visibleRect.height
        )
        let glyphRange = layoutManager.glyphRange(forBoundingRect: visibleContainerRect, in: textContainer)
        var lastLineNumber: Int?

        layoutManager.enumerateLineFragments(forGlyphRange: glyphRange) { _, usedRect, _, lineGlyphRange, _ in
            guard lineGlyphRange.location < layoutManager.numberOfGlyphs else {
                return
            }
            let characterIndex = layoutManager.characterIndexForGlyph(at: lineGlyphRange.location)
            let lineNumber = self.lineNumber(forUTF16Offset: characterIndex)
            guard lastLineNumber != lineNumber else {
                return
            }
            lastLineNumber = lineNumber
            let y = usedRect.minY + containerOrigin.y - visibleRect.minY
            let labelRect = NSRect(x: 0, y: y, width: self.ruleThickness - 12, height: 14)
            let attributes: [NSAttributedString.Key: Any] = [
                .font: NSFont.monospacedDigitSystemFont(ofSize: 10, weight: .regular),
                .foregroundColor: NSColor.tertiaryLabelColor,
                .paragraphStyle: self.paragraphStyle
            ]
            "\(lineNumber)".draw(in: labelRect, withAttributes: attributes)
        }
    }

    private func lineNumber(forUTF16Offset offset: Int) -> Int {
        var lowerBound = 0
        var upperBound = lineStartUTF16Offsets.count
        while lowerBound < upperBound {
            let middle = (lowerBound + upperBound) / 2
            if lineStartUTF16Offsets[middle] <= offset {
                lowerBound = middle + 1
            } else {
                upperBound = middle
            }
        }
        return max(1, lowerBound)
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
