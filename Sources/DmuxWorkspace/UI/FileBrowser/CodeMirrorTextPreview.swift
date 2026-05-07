import AppKit
import SwiftUI
import WebKit

struct ProjectFileEditorTheme: Equatable {
    var colorScheme: ColorScheme
    var foreground: String
    var background: String
    var caret: String
    var selectionBackground: String
    var palette: [String]
    var fontSize: Int

    init(
        colorScheme: ColorScheme = .light,
        foreground: String = "#24292F",
        background: String = "#FFFFFF",
        caret: String = "#24292F",
        selectionBackground: String = "#0969DA33",
        palette: [String] = [],
        fontSize: Int = 14
    ) {
        self.colorScheme = colorScheme
        self.foreground = foreground
        self.background = background
        self.caret = caret
        self.selectionBackground = selectionBackground
        self.palette = palette
        self.fontSize = Self.normalizedFontSize(fontSize)
    }

    init(appearance: AppEffectiveTerminalAppearance, fontSize: Int = 14) {
        self.init(
            colorScheme: appearance.isLight ? .light : .dark,
            foreground: appearance.foregroundColor.ghosttyHexString,
            background: appearance.backgroundColor.ghosttyHexString,
            caret: appearance.cursorColor.ghosttyHexString,
            selectionBackground: appearance.selectionBackgroundColor.ghosttyHexString,
            palette: appearance.paletteHexStrings,
            fontSize: fontSize
        )
    }

    var payload: Payload {
        Payload(
            colorScheme: colorScheme == .dark ? "dark" : "light",
            foreground: foreground,
            background: background,
            caret: caret,
            selectionBackground: selectionBackground,
            palette: palette,
            fontSize: fontSize,
            phrases: Self.searchPhrases()
        )
    }

    var nsBackgroundColor: NSColor {
        Self.nsColor(hexString: background, fallback: .textBackgroundColor)
    }

    private static func normalizedFontSize(_ value: Int) -> Int {
        max(10, min(28, value))
    }

    static func nsColor(hexString: String, fallback: NSColor) -> NSColor {
        let cleaned = hexString.trimmingCharacters(in: CharacterSet.alphanumerics.inverted)
        guard cleaned.count == 6,
              let value = UInt64(cleaned, radix: 16) else {
            return fallback
        }
        let red = CGFloat((value & 0xFF0000) >> 16) / 255.0
        let green = CGFloat((value & 0x00FF00) >> 8) / 255.0
        let blue = CGFloat(value & 0x0000FF) / 255.0
        return NSColor(red: red, green: green, blue: blue, alpha: 1)
    }

    struct Payload: Encodable {
        var colorScheme: String
        var foreground: String
        var background: String
        var caret: String
        var selectionBackground: String
        var palette: [String]
        var fontSize: Int
        var phrases: [String: String]
    }

    static func searchPhrases() -> [String: String] {
        [
            "Find": String(localized: "files.preview.search.find", defaultValue: "Find", bundle: .module),
            "Replace": String(localized: "files.preview.search.replace", defaultValue: "Replace", bundle: .module),
            "next": String(localized: "files.preview.search.next", defaultValue: "Next", bundle: .module),
            "previous": String(localized: "files.preview.search.previous", defaultValue: "Previous", bundle: .module),
            "all": String(localized: "files.preview.search.all", defaultValue: "All", bundle: .module),
            "match case": String(localized: "files.preview.search.match_case", defaultValue: "Match case", bundle: .module),
            "regexp": String(localized: "files.preview.search.regexp", defaultValue: "Regex", bundle: .module),
            "by word": String(localized: "files.preview.search.by_word", defaultValue: "Whole word", bundle: .module),
            "replace": String(localized: "files.preview.search.replace_action", defaultValue: "Replace", bundle: .module),
            "replace all": String(localized: "files.preview.search.replace_all", defaultValue: "Replace all", bundle: .module),
            "close": String(localized: "common.close", defaultValue: "Close", bundle: .module)
        ]
    }
}

struct CodeMirrorTextPreview: NSViewRepresentable {
    @Binding var text: String
    let colorScheme: ColorScheme
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
    let isLargeFileMode: Bool
    let onDirtyChanged: (Bool) -> Void
    let onTextSnapshot: (String) -> Void
    let onSaveRequested: () -> Void

    func makeCoordinator() -> Coordinator {
        let coordinator = Coordinator(
            text: $text,
            onDirtyChanged: onDirtyChanged,
            onTextSnapshot: onTextSnapshot,
            onSaveRequested: onSaveRequested
        )
        coordinator.fileExtension = fileExtension
        coordinator.isLargeFileMode = isLargeFileMode
        coordinator.colorScheme = colorScheme
        coordinator.editorTheme = editorTheme
        return coordinator
    }

    func makeNSView(context: Context) -> WKWebView {
        let configuration = WKWebViewConfiguration()
        configuration.preferences.javaScriptCanOpenWindowsAutomatically = false
        configuration.defaultWebpagePreferences.allowsContentJavaScript = true
        configuration.userContentController.add(context.coordinator, name: "coduxEditor")

        let webView = WKWebView(frame: .zero, configuration: configuration)
        webView.navigationDelegate = context.coordinator
        webView.setValue(false, forKey: "drawsBackground")
        webView.wantsLayer = true
        context.coordinator.webView = webView
        context.coordinator.applyWebViewBackground()
        webView.isHidden = true
        context.coordinator.loadEditor(in: webView)
        return webView
    }

    func updateNSView(_ webView: WKWebView, context: Context) {
        context.coordinator.text = $text
        context.coordinator.onDirtyChanged = onDirtyChanged
        context.coordinator.onTextSnapshot = onTextSnapshot
        context.coordinator.onSaveRequested = onSaveRequested
        context.coordinator.fileExtension = fileExtension
        context.coordinator.isLargeFileMode = isLargeFileMode
        context.coordinator.colorScheme = colorScheme
        context.coordinator.editorTheme = editorTheme
        context.coordinator.applyWebViewBackground()
        context.coordinator.applyStateIfNeeded(
            text: text,
            renderToken: renderToken,
            focusToken: focusToken,
            copyToken: copyToken,
            pasteToken: pasteToken,
            undoToken: undoToken,
            redoToken: redoToken,
            findToken: findToken,
            snapshotToken: snapshotToken,
            markSavedToken: markSavedToken
        )
    }

    final class Coordinator: NSObject, WKScriptMessageHandler, WKNavigationDelegate {
        var text: Binding<String>
        var onDirtyChanged: (Bool) -> Void
        var onTextSnapshot: (String) -> Void
        var onSaveRequested: () -> Void
        weak var webView: WKWebView?
        var fileExtension = ""
        var isLargeFileMode = false
        var colorScheme: ColorScheme = .light
        var editorTheme = ProjectFileEditorTheme()

        private var isEditorReady = false
        private var isApplyingNativeText = false
        private var lastKnownText = ""
        private var appliedColorScheme: ColorScheme?
        private var appliedEditorTheme = ProjectFileEditorTheme()
        private var appliedRenderToken = -1
        private var appliedFocusToken = 0
        private var appliedCopyToken = 0
        private var appliedPasteToken = 0
        private var appliedUndoToken = 0
        private var appliedRedoToken = 0
        private var appliedFindToken = 0
        private var appliedSnapshotToken = 0
        private var appliedMarkSavedToken = 0
        private var pendingInitialText: String?

        init(
            text: Binding<String>,
            onDirtyChanged: @escaping (Bool) -> Void,
            onTextSnapshot: @escaping (String) -> Void,
            onSaveRequested: @escaping () -> Void
        ) {
            self.text = text
            self.onDirtyChanged = onDirtyChanged
            self.onTextSnapshot = onTextSnapshot
            self.onSaveRequested = onSaveRequested
        }

        func loadEditor(in webView: WKWebView) {
            let script = Self.bundledEditorScript()
            let html = """
            <!doctype html>
            <html data-editor-scheme="\(editorTheme.colorScheme == .dark ? "dark" : "light")">
            <head>
              <meta charset="utf-8">
              <meta name="viewport" content="width=device-width, initial-scale=1">
              <style>
                :root {
                  \(initialThemeCSSVariables())
                }
                html {
                  color-scheme: \(editorTheme.colorScheme == .dark ? "dark" : "light");
                }
                html, body, #editor {
                  width: 100%;
                  height: 100%;
                  margin: 0;
                  overflow: hidden;
                  background: var(--editor-bg);
                }
              </style>
            </head>
            <body>
              <div id="editor"></div>
              <script>
              \(script)
              </script>
            </body>
            </html>
            """
            webView.loadHTMLString(html, baseURL: Bundle.module.resourceURL)
        }

        func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
            isEditorReady = true
            let initialText = pendingInitialText ?? text.wrappedValue
            pendingInitialText = nil
            initializeEditor(text: initialText)
            DispatchQueue.main.async {
                webView.isHidden = false
            }
        }

        func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
            guard message.name == "coduxEditor",
                  let payload = message.body as? [String: Any],
                  let type = payload["type"] as? String else {
                return
            }

            DispatchQueue.main.async { [weak self] in
                self?.handleEditorEvent(type: type, payload: payload)
            }
        }

        func applyStateIfNeeded(
            text: String,
            renderToken: Int,
            focusToken: Int,
            copyToken: Int,
            pasteToken: Int,
            undoToken: Int,
            redoToken: Int,
            findToken: Int,
            snapshotToken: Int,
            markSavedToken: Int
        ) {
            if isEditorReady == false {
                pendingInitialText = text
                return
            }

            if appliedColorScheme != colorScheme || appliedEditorTheme != editorTheme {
                appliedColorScheme = colorScheme
                appliedEditorTheme = editorTheme
                setEditorTheme()
            }

            if appliedRenderToken != renderToken {
                appliedRenderToken = renderToken
                setEditorText(text)
            } else if lastKnownText.isEmpty && text.isEmpty == false {
                setEditorText(text)
            }

            if focusToken != appliedFocusToken {
                appliedFocusToken = focusToken
                evaluate("window.CoduxCodeMirrorEditor.focus()")
            }
            if copyToken != appliedCopyToken {
                appliedCopyToken = copyToken
                copySelectionOrDocument()
            }
            if pasteToken != appliedPasteToken {
                appliedPasteToken = pasteToken
                pasteFromPasteboard()
            }
            if undoToken != appliedUndoToken {
                appliedUndoToken = undoToken
                evaluate("window.CoduxCodeMirrorEditor.undo()")
            }
            if redoToken != appliedRedoToken {
                appliedRedoToken = redoToken
                evaluate("window.CoduxCodeMirrorEditor.redo()")
            }
            if findToken != appliedFindToken {
                appliedFindToken = findToken
                evaluate("window.CoduxCodeMirrorEditor.find()")
            }
            if snapshotToken != appliedSnapshotToken {
                appliedSnapshotToken = snapshotToken
                requestTextSnapshot()
            }
            if markSavedToken != appliedMarkSavedToken {
                appliedMarkSavedToken = markSavedToken
                evaluate("window.CoduxCodeMirrorEditor.markSaved()")
            }
        }

        private func handleEditorEvent(type: String, payload: [String: Any]) {
            switch type {
            case "ready":
                break
            case "contentChanged":
                if let isDirty = payload["dirty"] as? Bool {
                    onDirtyChanged(isDirty)
                }
                guard isApplyingNativeText == false,
                      let updatedText = payload["text"] as? String else {
                    return
                }
                lastKnownText = updatedText
                if text.wrappedValue != updatedText {
                    text.wrappedValue = updatedText
                }
            case "saveRequested":
                onSaveRequested()
            default:
                break
            }
        }

        private func initializeEditor(text: String) {
            lastKnownText = text
            appliedColorScheme = colorScheme
            appliedEditorTheme = editorTheme
            appliedRenderToken = max(appliedRenderToken, 0)
            let payload = EditorPayload(
                text: text,
                fileExtension: fileExtension,
                largeFileMode: isLargeFileMode,
                theme: editorTheme.payload
            )
            callEditor(function: "initialize", payload: payload)
        }

        private func setEditorText(_ text: String) {
            lastKnownText = text
            isApplyingNativeText = true
            let payload = EditorPayload(
                text: text,
                fileExtension: fileExtension,
                largeFileMode: isLargeFileMode,
                theme: editorTheme.payload
            )
            callEditor(function: "setText", payload: payload)
            isApplyingNativeText = false
        }

        private func setEditorTheme() {
            applyWebViewBackground()
            callEditor(function: "setTheme", payload: editorTheme.payload)
        }

        func applyWebViewBackground() {
            guard let webView else {
                return
            }
            let backgroundColor = editorTheme.nsBackgroundColor
            webView.layer?.backgroundColor = backgroundColor.cgColor
            if #available(macOS 12.0, *) {
                webView.underPageBackgroundColor = backgroundColor
            }
        }

        private func copySelectionOrDocument() {
            evaluate("window.CoduxCodeMirrorEditor.selectedText()") { [weak self] result, _ in
                let selectedText = result as? String
                DispatchQueue.main.async {
                    guard let self else {
                        return
                    }
                    let copyText = selectedText?.isEmpty == false ? selectedText! : self.text.wrappedValue
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(copyText, forType: .string)
                }
            }
        }

        private func requestTextSnapshot() {
            evaluate("window.CoduxCodeMirrorEditor.getText()") { [weak self] result, _ in
                let snapshot = result as? String
                DispatchQueue.main.async {
                    guard let self, let snapshot else {
                        return
                    }
                    self.lastKnownText = snapshot
                    if self.text.wrappedValue != snapshot {
                        self.text.wrappedValue = snapshot
                    }
                    self.onTextSnapshot(snapshot)
                }
            }
        }

        private func pasteFromPasteboard() {
            guard let pasteText = NSPasteboard.general.string(forType: .string) else {
                return
            }
            callEditor(function: "pasteText", payload: pasteText)
        }

        private func callEditor<T: Encodable>(
            function: String,
            payload: T
        ) {
            guard let json = Self.jsonLiteral(payload) else {
                return
            }
            evaluate("window.CoduxCodeMirrorEditor.\(function)(\(json))")
        }

        private func evaluate(
            _ script: String,
            completion: (@Sendable (Any?, Error?) -> Void)? = nil
        ) {
            webView?.evaluateJavaScript(script, completionHandler: completion)
        }

        private func initialThemeCSSVariables() -> String {
            let isDark = editorTheme.colorScheme == .dark
            let fontSize = editorTheme.fontSize
            let lineHeight = Int(round(Double(fontSize) * 1.42))
            return """
                  color-scheme: \(isDark ? "dark" : "light");
                  --editor-bg: \(editorTheme.background);
                  --editor-fg: \(editorTheme.foreground);
                  --editor-caret: \(editorTheme.caret);
                  --gutter-bg: \(isDark ? "rgba(17, 19, 24, 0.72)" : "rgba(240, 241, 243, 0.66)");
                  --gutter-fg: \(isDark ? "#8b929c" : "#747b84");
                  --separator: \(isDark ? "rgba(232, 234, 237, 0.14)" : "rgba(30, 35, 41, 0.14)");
                  --active-bg: \(isDark ? "rgba(88, 166, 255, 0.13)" : "rgba(0, 122, 255, 0.08)");
                  --selection-bg: \(editorTheme.selectionBackground);
                  --panel-bg: \(isDark ? "#1d2026" : "#f3f4f6");
                  --input-bg: \(isDark ? "#15181d" : "#ffffff");
                  --button-bg: \(isDark ? "#22262d" : "#f7f8fa");
                  --editor-font-size: \(fontSize)px;
                  --editor-line-height: \(lineHeight)px;
            """
        }

        private static func bundledEditorScript() -> String {
            let urls = [
                Bundle.module.url(forResource: "codux-editor.bundle", withExtension: "js", subdirectory: "CodeMirror"),
                Bundle.module.url(forResource: "codux-editor.bundle", withExtension: "js", subdirectory: "Resources/CodeMirror"),
                Bundle.module.url(forResource: "codux-editor.bundle", withExtension: "js")
            ]
            for url in urls.compactMap({ $0 }) {
                if let script = try? String(contentsOf: url, encoding: .utf8) {
                    return script
                }
            }
            return "window.CoduxCodeMirrorEditor={initialize:function(){},setText:function(){},setTheme:function(){},getText:function(){return ''},markSaved:function(){},focus:function(){},undo:function(){},redo:function(){},find:function(){},selectedText:function(){return ''},pasteText:function(){}};"
        }

        private static func jsonLiteral<T: Encodable>(_ value: T) -> String? {
            guard let data = try? JSONEncoder().encode(value) else {
                return nil
            }
            return String(data: data, encoding: .utf8)
        }

    }
}

private struct EditorPayload: Encodable {
    var text: String
    var fileExtension: String
    var largeFileMode: Bool
    var theme: ProjectFileEditorTheme.Payload
}
