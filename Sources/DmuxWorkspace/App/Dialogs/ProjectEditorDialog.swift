import AppKit
import Foundation
import SwiftUI

func appendProjectEditLog(_ message: String) {
    let line = "[ProjectEdit] \(message)\n"
    let url = URL(fileURLWithPath: "/tmp/dmux-dev.log")

    if let data = line.data(using: .utf8) {
        if FileManager.default.fileExists(atPath: url.path) {
            if let handle = try? FileHandle(forWritingTo: url) {
                defer { try? handle.close() }
                _ = try? handle.seekToEnd()
                try? handle.write(contentsOf: data)
                return
            }
        }

        try? data.write(to: url, options: .atomic)
    }
}

private final class ProjectEditorPanelViewModel: ObservableObject {
    @Published var name: String
    @Published var path: String
    @Published var badgeText: String
    @Published var badgeSymbol: String?
    @Published var badgeColorHex: String
    var onConfirm: ((ProjectEditorDialogState) -> Void)?
    var onCancel: (() -> Void)?
    var onChooseDirectory: (() -> Void)?

    init(dialog: ProjectEditorDialogState) {
        self.name = dialog.name
        self.path = dialog.path
        self.badgeText = dialog.badgeText
        self.badgeSymbol = dialog.badgeSymbol
        self.badgeColorHex = dialog.badgeColorHex.isEmpty ? systemAccentHexString() : dialog.badgeColorHex
    }
}

private struct ProjectEditorPanelView: View {
    let dialog: ProjectEditorDialogState
    @ObservedObject var viewModel: ProjectEditorPanelViewModel
    let onNameFieldCreated: ((NSTextField) -> Void)?
    @State private var isNameHovered = false
    @State private var isPathHovered = false

    private let presetSymbols = [nil, "terminal", "folder", "shippingbox", "hammer", "server.rack", "globe", "bolt", "wrench.and.screwdriver", "doc.text", "shippingbox.fill", "laptopcomputer", "cube.box", "paintpalette", "sparkles", "book", "person.2"]
    private var presetColors: [String] {
        let accent = systemAccentHexString()
        let base = ["#8C52FF", "#4C8BF5", "#15B8A6", "#32C766", "#FFB020", "#FF7A59", "#FF5C8A", "#7B61FF", "#00A3FF", "#6D9F71"]
        return [accent] + base.filter { $0.caseInsensitiveCompare(accent) != .orderedSame }
    }

    private var isConfirmDisabled: Bool {
        viewModel.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || viewModel.path.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var header: AppDialogHeaderSpec {
        let isCreating = dialog.confirmTitle == appI18n("common.create", fallback: "Create")
        return AppDialogHeaderSpec(
            title: dialog.title,
            message: dialog.message,
            icon: isCreating ? "folder.badge.plus" : "square.and.pencil",
            iconColor: isCreating ? AppTheme.focus : AppTheme.success
        )
    }

    var body: some View {
        AppDialogFormLayout(
            header: header,
            width: 520,
            chromeTopInset: 8,
            contentSpacing: 14,
            headerTopPadding: 20,
            headerBottomPadding: 8,
            contentTopPadding: 14,
            contentBottomPadding: 18,
            footerTopPadding: 0,
            footerBottomPadding: 18
        ) {
            field(title: appI18n("project.editor.name", fallback: "Project Name"), text: $viewModel.name, isHovered: $isNameHovered, placeholder: appI18n("common.required", fallback: "Required"), autofocus: true, onViewCreated: onNameFieldCreated)
            pathField

            VStack(alignment: .leading, spacing: 6) {
                Text(appI18n("project.editor.icon", fallback: "Project Icon"))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color(nsColor: .labelColor))
                LazyVGrid(columns: [GridItem(.adaptive(minimum: 36, maximum: 36), spacing: 8)], alignment: .leading, spacing: 8) {
                    ForEach(presetSymbols, id: \.self) { symbol in
                        Button {
                            viewModel.badgeSymbol = symbol
                        } label: {
                            ZStack {
                                RoundedRectangle(cornerRadius: 10, style: .continuous)
                                    .fill(viewModel.badgeSymbol == symbol ? Color(nsColor: .selectedContentBackgroundColor).opacity(0.2) : Color(nsColor: .quaternarySystemFill))
                                    .frame(width: 36, height: 36)
                                if let symbol {
                                    Image(systemName: symbol)
                                        .font(.system(size: 14, weight: .semibold))
                                        .foregroundStyle(Color(hexString: hexColor))
                                } else {
                                    Text(appI18n("common.none", fallback: "None"))
                                        .font(.system(size: 11, weight: .bold, design: .rounded))
                                        .foregroundStyle(Color(nsColor: .labelColor).opacity(0.82))
                                }
                            }
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }

            VStack(alignment: .leading, spacing: 8) {
                Text(appI18n("project.editor.color", fallback: "Project Color"))
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(Color(nsColor: .labelColor))
                HStack(spacing: 10) {
                    ForEach(presetColors, id: \.self) { color in
                        Button {
                            viewModel.badgeColorHex = color
                        } label: {
                            Circle()
                                .fill(Color(hexString: color))
                                .frame(width: 24, height: 24)
                                .overlay(
                                    Circle().stroke(viewModel.badgeColorHex == color ? Color(nsColor: .labelColor).opacity(0.6) : Color(nsColor: .separatorColor).opacity(0.5), lineWidth: viewModel.badgeColorHex == color ? 2.5 : 0.5)
                                )
                        }
                        .buttonStyle(.borderless)
                    }
                }
            }
        } actions: {
            Button(appI18n("common.cancel", fallback: "Cancel")) { viewModel.onCancel?() }
                .buttonStyle(AppDialogSecondaryButtonStyle())
                .keyboardShortcut(.cancelAction)

            Button(dialog.confirmTitle) {
                appendProjectEditLog("[PanelSaveTap] name=\(viewModel.name) path=\(viewModel.path) symbol=\(viewModel.badgeSymbol ?? "nil") color=\(viewModel.badgeColorHex)")
                viewModel.onConfirm?(ProjectEditorDialogState(
                    title: dialog.title,
                    message: dialog.message,
                    confirmTitle: dialog.confirmTitle,
                    name: viewModel.name,
                    path: viewModel.path,
                    badgeText: viewModel.badgeText.trimmingCharacters(in: .whitespacesAndNewlines),
                    badgeSymbol: viewModel.badgeSymbol,
                    badgeColorHex: viewModel.badgeColorHex
                ))
            }
            .buttonStyle(AppDialogPrimaryButtonStyle())
            .disabled(isConfirmDisabled)
        }
    }

    private var pathField: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(appI18n("project.editor.directory", fallback: "Project Directory"))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(Color(nsColor: .labelColor))
            HStack(spacing: 10) {
                field(title: nil, text: $viewModel.path, isHovered: $isPathHovered, placeholder: "/path/to/project")
                Button(appI18n("common.choose", fallback: "Choose")) {
                    viewModel.onChooseDirectory?()
                }
                .buttonStyle(AppDialogSecondaryButtonStyle())
            }
        }
    }

    private func field(title: String?, text: Binding<String>, isHovered: Binding<Bool>, placeholder: String, autofocus: Bool = false, onViewCreated: ((NSTextField) -> Void)? = nil) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            if let title {
                Text(title)
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(Color(nsColor: .labelColor))
            }
            AutofocusTextField(text: text, placeholder: placeholder, autofocus: autofocus, onViewCreated: onViewCreated)
                .padding(.horizontal, 12)
                .frame(height: 34)
                .background(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .fill(Color(nsColor: .textBackgroundColor))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8, style: .continuous)
                        .stroke(Color(nsColor: .separatorColor).opacity(isHovered.wrappedValue ? 0.8 : 0.5), lineWidth: 1)
                )
                .onHover { isHovered.wrappedValue = $0 }
        }
    }

    private var hexColor: String {
        viewModel.badgeColorHex.isEmpty ? "#6B2D73" : viewModel.badgeColorHex
    }
}

final class ProjectEditorPanelController: AppDialogController<ProjectEditorDialogState> {
    private let viewModel: ProjectEditorPanelViewModel
    private weak var nameField: NSTextField?

    init(dialog: ProjectEditorDialogState) {
        self.viewModel = ProjectEditorPanelViewModel(dialog: dialog)

        let panel = AppDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: 520, height: 420),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = false
        panel.level = .normal
        panel.becomesKeyOnlyIfNeeded = false
        panel.hidesOnDeactivate = false
        panel.worksWhenModal = true
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.hasShadow = true
        panel.isMovableByWindowBackground = false
        panel.collectionBehavior = [.moveToActiveSpace]
        panel.standardWindowButton(.closeButton)?.isHidden = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true

        super.init(panel: panel)

        let contentView = ProjectEditorPanelView(
            dialog: dialog,
            viewModel: viewModel,
            onNameFieldCreated: { [weak self] field in
                self?.nameField = field
            }
        )
        let hostingController = NSHostingController(rootView: contentView)
        panel.contentViewController = hostingController
        panel.setContentSize(NSSize(width: 520, height: 420))
        panel.minSize = NSSize(width: 520, height: 420)
        panel.maxSize = NSSize(width: 520, height: 420)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func prepareForPresentation() {
        viewModel.onConfirm = { [weak self] value in
            appendProjectEditLog("[PanelConfirm] name=\(value.name) path=\(value.path) symbol=\(value.badgeSymbol ?? "nil") color=\(value.badgeColorHex)")
            self?.finish(with: .continue, value: value)
        }
        viewModel.onCancel = { [weak self] in
            self?.finish(with: .abort)
        }
        viewModel.onChooseDirectory = { [weak self] in
            guard let self, let window = self.window else { return }
            let panel = NSOpenPanel()
            panel.canChooseDirectories = true
            panel.canChooseFiles = false
            panel.allowsMultipleSelection = false
            panel.title = appI18n("project.editor.choose_directory.title", fallback: "Choose Project Directory")
            panel.prompt = appI18n("project.editor.choose_directory.prompt", fallback: "Choose")
            panel.message = appI18n("project.editor.choose_directory.message", fallback: "Select a folder for this project.")
            panel.beginSheetModal(for: window) { response in
                guard response == .OK, let url = panel.url else { return }
                self.viewModel.path = url.path
                if self.viewModel.name.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                    self.viewModel.name = url.lastPathComponent
                }
            }
        }
    }

    func windowDidBecomeKey(_ notification: Notification) {
        focusNameField()
    }

    private func focusNameField() {
        guard let window, let nameField else { return }

        DispatchQueue.main.async {
            guard window.sheetParent != nil else { return }
            window.makeFirstResponder(nil)
            window.makeFirstResponder(nameField)
            if let editor = window.fieldEditor(true, for: nameField) as? NSTextView {
                editor.insertionPointColor = .labelColor
                editor.selectedRange = NSRange(location: nameField.stringValue.count, length: 0)
            }
        }
    }
}
