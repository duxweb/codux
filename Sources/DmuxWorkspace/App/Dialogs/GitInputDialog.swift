import AppKit
import SwiftUI

private final class GitInputPanelViewModel: ObservableObject {
    @Published var inputValue: String
    var onConfirm: ((String) -> Void)?
    var onCancel: (() -> Void)?

    init(initialValue: String) {
        self.inputValue = initialValue
    }
}

private struct GitSingleLineInputField: View {
    @Binding var text: String
    let placeholder: String
    let icon: String
    let iconColor: Color
    @FocusState.Binding var isFocused: Bool
    @Binding var isHovered: Bool
    let onSubmit: () -> Void

    var body: some View {
        ZStack(alignment: .leading) {
            HStack(spacing: 10) {
                Image(systemName: icon)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(isFocused ? iconColor : Color(nsColor: .tertiaryLabelColor))

                TextField("", text: $text)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13, weight: .medium, design: .monospaced))
                    .foregroundStyle(Color(nsColor: .labelColor))
                    .focused($isFocused)
                    .onSubmit(onSubmit)
                    .overlay(alignment: .leading) {
                        if text.isEmpty {
                            Text(placeholder)
                                .font(.system(size: 13, weight: .medium, design: .monospaced))
                                .foregroundStyle(Color(nsColor: .placeholderTextColor))
                                .allowsHitTesting(false)
                        }
                    }
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
        }
        .frame(minHeight: 42, alignment: .leading)
        .appInputSurface(isFocused: isFocused, isHovered: isHovered)
        .onHover { isHovered = $0 }
    }
}

private struct GitMultilineInputArea: View {
    @Binding var text: String
    let placeholder: String
    @FocusState.Binding var isFocused: Bool

    var body: some View {
        AppMultilineInputArea(
            text: $text,
            placeholder: placeholder,
            isFocused: Binding(get: { isFocused }, set: { isFocused = $0 }),
            font: .systemFont(ofSize: 13, weight: .regular),
            horizontalInset: 12,
            verticalInset: 10,
            enablesSpellChecking: true
        )
    }
}

private struct GitInputPanelView: View {
    let dialog: GitInputDialogState
    @ObservedObject var viewModel: GitInputPanelViewModel
    @FocusState private var isFocused: Bool
    @State private var isInputHovered = false

    private var dialogIcon: (name: String, color: Color) {
        switch dialog.kind {
        case .createBranch, .createBranchFromCommit:
            return ("arrow.triangle.branch", AppTheme.success)
        case .editLastCommitMessage(let pushed):
            return pushed ? ("exclamationmark.triangle.fill", AppTheme.warning) : ("pencil.line", AppTheme.focus)
        case .cloneRepository:
            return ("square.and.arrow.down", AppTheme.focus)
        case .renameAISession:
            return ("pencil.line", AppTheme.focus)
        }
    }

    private var isConfirmDisabled: Bool {
        viewModel.inputValue.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var header: AppDialogHeaderSpec {
        AppDialogHeaderSpec(title: dialog.title, message: dialog.message, icon: dialogIcon.name, iconColor: dialogIcon.color)
    }

    var body: some View {
        AppDialogFormLayout(
            header: header,
            width: nil,
            chromeTopInset: 8,
            contentSpacing: 0,
            headerTopPadding: 20,
            headerBottomPadding: 14,
            contentTopPadding: 2,
            contentBottomPadding: 4,
            footerTopPadding: 14,
            footerBottomPadding: 22
        ) {
            if dialog.isMultiline {
                GitMultilineInputArea(text: $viewModel.inputValue, placeholder: dialog.placeholder, isFocused: $isFocused)
                    .frame(height: 160)
            } else {
                GitSingleLineInputField(
                    text: $viewModel.inputValue,
                    placeholder: dialog.placeholder,
                    icon: dialogIcon.name,
                    iconColor: dialogIcon.color,
                    isFocused: $isFocused,
                    isHovered: $isInputHovered,
                    onSubmit: { submitIfValid() }
                )
            }
        } actions: {
            Button(appI18n("common.cancel", fallback: "Cancel")) { viewModel.onCancel?() }
                .buttonStyle(AppDialogSecondaryButtonStyle())
                .keyboardShortcut(.cancelAction)

            if dialog.isMultiline {
                Button(dialog.confirmTitle) { submitIfValid() }
                    .buttonStyle(AppDialogPrimaryButtonStyle())
                    .disabled(isConfirmDisabled)
                    .keyboardShortcut(.return, modifiers: .command)
            } else {
                Button(dialog.confirmTitle) { submitIfValid() }
                    .buttonStyle(AppDialogPrimaryButtonStyle())
                    .disabled(isConfirmDisabled)
                    .keyboardShortcut(.return, modifiers: [])
            }
        }
        .onAppear {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.05) {
                isFocused = true
            }
        }
    }

    private func submitIfValid() {
        let value = viewModel.inputValue.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !value.isEmpty else { return }
        viewModel.onConfirm?(value)
    }
}

final class GitInputPanelController: AppDialogController<String> {
    private let viewModel: GitInputPanelViewModel

    init(dialog: GitInputDialogState) {
        self.viewModel = GitInputPanelViewModel(initialValue: dialog.value)

        let width: CGFloat = dialog.isMultiline ? 480 : 440
        let panel = AppDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: width, height: 240),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = false
        panel.level = .normal
        panel.title = dialog.title
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.hasShadow = true
        panel.isMovableByWindowBackground = false
        panel.collectionBehavior = [.moveToActiveSpace]
        panel.standardWindowButton(.closeButton)?.isHidden = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true

        super.init(panel: panel)

        let contentView = GitInputPanelView(dialog: dialog, viewModel: viewModel)
            .frame(width: width, alignment: .topLeading)
        let hostingController = NSHostingController(rootView: contentView)
        hostingController.view.frame = NSRect(x: 0, y: 0, width: width, height: 1)
        hostingController.view.autoresizingMask = [.width, .height]
        hostingController.view.layoutSubtreeIfNeeded()

        let contentHeight = max(1, hostingController.view.fittingSize.height)
        panel.contentViewController = hostingController
        panel.setContentSize(NSSize(width: width, height: contentHeight))
        panel.minSize = NSSize(width: width, height: contentHeight)
        panel.maxSize = NSSize(width: width, height: contentHeight)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func prepareForPresentation() {
        viewModel.onConfirm = { [weak self] value in
            self?.finish(with: .continue, value: value)
        }
        viewModel.onCancel = { [weak self] in
            self?.finish(with: .abort)
        }
    }
}
