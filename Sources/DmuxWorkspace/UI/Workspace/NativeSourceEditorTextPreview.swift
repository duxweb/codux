import AppKit
import CodeEditLanguages
import CodeEditSourceEditor
import SwiftUI

struct NativeSourceEditorTextPreview: View {
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
    let fileURL: URL
    let isLargeFileMode: Bool
    var onFocused: () -> Void = {}
    let onDirtyChanged: (Bool) -> Void
    let onTextSnapshot: (String) -> Void
    let onSaveRequested: () -> Void

    @State private var editorState = SourceEditorState()
    @State private var coordinator = NativeSourceEditorCoordinator()

    private var configuration: SourceEditorConfiguration {
        SourceEditorConfiguration(
            appearance: .init(
                theme: editorTheme.sourceEditorTheme,
                font: NSFont.monospacedSystemFont(ofSize: CGFloat(editorTheme.fontSize), weight: .regular),
                lineHeightMultiple: 1.36,
                wrapLines: !isLargeFileMode,
                tabWidth: 4
            ),
            behavior: .init(
                isEditable: true,
                isSelectable: true,
                indentOption: .spaces(count: 4)
            ),
            layout: .init(
                editorOverscroll: 0.18,
                contentInsets: NSEdgeInsets(top: 0, left: 0, bottom: 0, right: 0),
                additionalTextInsets: NSEdgeInsets(top: 8, left: 0, bottom: 12, right: 0)
            ),
            peripherals: .init(
                showGutter: true,
                showMinimap: !isLargeFileMode,
                showReformattingGuide: false,
                showFoldingRibbon: true
            )
        )
    }

    private var language: CodeLanguage {
        CodeLanguage.detectLanguageFrom(
            url: fileURL,
            prefixBuffer: String(text.prefix(4096)),
            suffixBuffer: String(text.suffix(4096))
        )
    }

    var body: some View {
        SourceEditor(
            $text,
            language: language,
            configuration: configuration,
            state: $editorState,
            coordinators: [coordinator]
        )
        .background(Color(nsColor: editorTheme.sourceEditorTheme.background))
        .clipped()
        .onAppear {
            coordinator.configure(onFocused: onFocused, onSaveRequested: onSaveRequested)
            onDirtyChanged(false)
            if focusToken > 0 {
                coordinator.focus()
            }
        }
        .onChange(of: focusToken) { _, _ in
            coordinator.focus()
        }
        .onChange(of: renderToken) { _, _ in
            coordinator.replaceText(text)
            onDirtyChanged(false)
        }
        .onChange(of: copyToken) { _, _ in
            coordinator.copy(fallbackText: text)
        }
        .onChange(of: pasteToken) { _, _ in
            coordinator.paste()
        }
        .onChange(of: undoToken) { _, _ in
            coordinator.undo()
        }
        .onChange(of: redoToken) { _, _ in
            coordinator.redo()
        }
        .onChange(of: findToken) { _, _ in
            showFindPanel()
        }
        .onChange(of: snapshotToken) { _, _ in
            onTextSnapshot(coordinator.currentText(fallback: text))
        }
        .onChange(of: markSavedToken) { _, _ in
            onDirtyChanged(false)
        }
    }

    private func showFindPanel() {
        if editorState.findPanelVisible == true {
            editorState.findPanelVisible = false
            DispatchQueue.main.async {
                editorState.findPanelVisible = true
            }
        } else {
            editorState.findPanelVisible = true
        }
    }
}

@MainActor
private final class NativeSourceEditorCoordinator: NSObject, @preconcurrency TextViewCoordinator {
    private weak var controller: TextViewController?
    private var pendingText: String?
    private var pendingFocus = false
    private var onFocused: () -> Void = {}
    private var onSaveRequested: () -> Void = {}

    func configure(
        onFocused: @escaping () -> Void,
        onSaveRequested: @escaping () -> Void
    ) {
        self.onFocused = onFocused
        self.onSaveRequested = onSaveRequested
    }

    func prepareCoordinator(controller: TextViewController) {
        self.controller = controller
        constrainLoadedEditorToBounds(controller)
        if let pendingText {
            replaceText(pendingText)
            self.pendingText = nil
        }
        if pendingFocus {
            pendingFocus = false
            focus()
        }
    }

    func controllerDidAppear(controller: TextViewController) {
        self.controller = controller
        constrainLoadedEditorToBounds(controller)
    }

    func textViewDidChangeSelection(controller: TextViewController, newPositions: [CursorPosition]) {
        self.controller = controller
        onFocused()
    }

    func textViewDidChangeText(controller: TextViewController) {
        self.controller = controller
    }

    func destroy() {
        controller = nil
    }

    func focus() {
        guard let controller else {
            pendingFocus = true
            return
        }
        controller.view.window?.makeFirstResponder(controller.textView)
        onFocused()
    }

    func replaceText(_ value: String) {
        guard let controller else {
            pendingText = value
            return
        }
        if controller.text != value {
            controller.text = value
        }
    }

    func currentText(fallback: String) -> String {
        controller?.text ?? fallback
    }

    func copy(fallbackText: String) {
        guard let controller else {
            copyText(fallbackText)
            return
        }
        let pasteboard = NSPasteboard.general
        let changeCount = pasteboard.changeCount
        controller.textView.copy(self)
        if pasteboard.changeCount == changeCount {
            copyText(controller.text)
        }
    }

    func paste() {
        controller?.textView.paste(self)
    }

    func undo() {
        controller?.textView.undoManager?.undo()
    }

    func redo() {
        controller?.textView.undoManager?.redo()
    }

    private func copyText(_ value: String) {
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(value, forType: .string)
    }

    private func constrainLoadedEditorToBounds(_ controller: TextViewController) {
        guard controller.isViewLoaded else {
            return
        }
        let views: [NSView] = [
            controller.view,
            controller.scrollView,
            controller.scrollView.contentView,
            controller.scrollView.documentView
        ].compactMap { $0 }

        for view in views {
            view.clipsToBounds = true
            view.wantsLayer = true
            view.layer?.masksToBounds = true
        }
    }
}

private extension ProjectFileEditorTheme {
    var sourceEditorTheme: EditorTheme {
        let foregroundColor = rgbaColor(hexString: foreground, fallback: .labelColor)
        let backgroundColor = rgbaColor(hexString: background, fallback: .textBackgroundColor)
        let caretColor = rgbaColor(hexString: caret, fallback: foregroundColor)
        let selectionColor = rgbaColor(hexString: selectionBackground, fallback: .selectedTextBackgroundColor)
        let blue = paletteColor(at: 4, fallback: NSColor.systemBlue)
        let green = paletteColor(at: 2, fallback: NSColor.systemGreen)
        let yellow = paletteColor(at: 3, fallback: NSColor.systemYellow)
        let red = paletteColor(at: 1, fallback: NSColor.systemRed)
        let magenta = paletteColor(at: 5, fallback: NSColor.systemPurple)
        let cyan = paletteColor(at: 6, fallback: NSColor.systemTeal)
        return EditorTheme(
            text: .init(color: foregroundColor),
            insertionPoint: caretColor,
            invisibles: .init(color: foregroundColor.withAlphaComponent(0.35)),
            background: backgroundColor,
            lineHighlight: selectionColor.withAlphaComponent(max(selectionColor.alphaComponent, 0.16)),
            selection: selectionColor,
            keywords: .init(color: magenta, bold: true),
            commands: .init(color: blue),
            types: .init(color: yellow),
            attributes: .init(color: cyan),
            variables: .init(color: foregroundColor),
            values: .init(color: cyan),
            numbers: .init(color: red),
            strings: .init(color: green),
            characters: .init(color: green),
            comments: .init(color: foregroundColor.withAlphaComponent(0.55), italic: true)
        )
    }

    private func paletteColor(at index: Int, fallback: NSColor) -> NSColor {
        guard palette.indices.contains(index) else {
            return fallback
        }
        return rgbaColor(hexString: palette[index], fallback: fallback)
    }

    private func rgbaColor(hexString: String, fallback: NSColor) -> NSColor {
        let cleaned = hexString.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        guard cleaned.count == 6 || cleaned.count == 8,
              let value = UInt64(cleaned, radix: 16) else {
            return fallback
        }
        let red = CGFloat((value & (cleaned.count == 8 ? 0xFF000000 : 0xFF0000)) >> (cleaned.count == 8 ? 24 : 16)) / 255.0
        let green = CGFloat((value & (cleaned.count == 8 ? 0x00FF0000 : 0x00FF00)) >> (cleaned.count == 8 ? 16 : 8)) / 255.0
        let blue = CGFloat((value & (cleaned.count == 8 ? 0x0000FF00 : 0x0000FF)) >> (cleaned.count == 8 ? 8 : 0)) / 255.0
        let alpha = cleaned.count == 8 ? CGFloat(value & 0x000000FF) / 255.0 : 1
        return NSColor(red: red, green: green, blue: blue, alpha: alpha)
    }
}
