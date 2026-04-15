import AppKit
import SwiftUI

private final class GitCredentialPanelViewModel: ObservableObject {
    @Published var username: String
    @Published var password: String
    var onConfirm: ((GitCredential) -> Void)?
    var onCancel: (() -> Void)?

    init(dialog: GitCredentialDialogState) {
        self.username = dialog.username
        self.password = dialog.password
    }
}

private struct GitCredentialDialogView: View {
    let dialog: GitCredentialDialogState
    @ObservedObject var viewModel: GitCredentialPanelViewModel
    @State private var isUsernameHovered = false
    @State private var isPasswordHovered = false

    private var header: AppDialogHeaderSpec {
        AppDialogHeaderSpec(title: dialog.title, message: dialog.message, icon: "person.crop.circle.badge.key", iconColor: AppTheme.warning)
    }

    private var isConfirmDisabled: Bool {
        viewModel.username.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty || viewModel.password.isEmpty
    }

    var body: some View {
        AppDialogFormLayout(
            header: header,
            width: 440,
            chromeTopInset: 8,
            contentSpacing: 12,
            headerTopPadding: 20,
            headerBottomPadding: 10,
            contentTopPadding: 6,
            contentBottomPadding: 16,
            footerTopPadding: 0,
            footerBottomPadding: 18
        ) {
            dialogField(title: appI18n("git.credential.username", fallback: "Username"), text: $viewModel.username, placeholder: appI18n("git.credential.username", fallback: "Username"), isSecure: false, isHovered: $isUsernameHovered)
            dialogField(title: appI18n("git.credential.password_or_token", fallback: "Password or Token"), text: $viewModel.password, placeholder: appI18n("git.credential.password_or_token", fallback: "Password or Token"), isSecure: true, isHovered: $isPasswordHovered)
        } actions: {
            Button(dialog.cancelTitle) { viewModel.onCancel?() }
                .buttonStyle(AppDialogSecondaryButtonStyle())
                .keyboardShortcut(.cancelAction)

            Button(dialog.confirmTitle) {
                viewModel.onConfirm?(GitCredential(
                    username: viewModel.username.trimmingCharacters(in: .whitespacesAndNewlines),
                    password: viewModel.password
                ))
            }
            .buttonStyle(AppDialogPrimaryButtonStyle(tint: AppTheme.warning))
            .disabled(isConfirmDisabled)
        }
    }

    private func dialogField(title: String, text: Binding<String>, placeholder: String, isSecure: Bool, isHovered: Binding<Bool>) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(title)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(Color(nsColor: .labelColor))

            if isSecure {
                SecureField(placeholder, text: text)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13, weight: .regular))
                    .foregroundStyle(Color(nsColor: .labelColor))
                    .padding(.horizontal, 12)
                    .frame(height: 36)
                    .appInputSurface(isFocused: false, isHovered: isHovered.wrappedValue)
                    .onHover { isHovered.wrappedValue = $0 }
            } else {
                TextField(placeholder, text: text)
                    .textFieldStyle(.plain)
                    .font(.system(size: 13, weight: .regular))
                    .foregroundStyle(Color(nsColor: .labelColor))
                    .padding(.horizontal, 12)
                    .frame(height: 36)
                    .appInputSurface(isFocused: false, isHovered: isHovered.wrappedValue)
                    .onHover { isHovered.wrappedValue = $0 }
            }
        }
    }
}

final class GitCredentialDialogController: AppDialogController<GitCredential> {
    private let viewModel: GitCredentialPanelViewModel

    init(dialog: GitCredentialDialogState) {
        self.viewModel = GitCredentialPanelViewModel(dialog: dialog)

        let panel = AppDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: 440, height: 260),
            styleMask: [.titled, .closable, .fullSizeContentView],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = false
        panel.level = .normal
        panel.titleVisibility = .hidden
        panel.titlebarAppearsTransparent = true
        panel.hasShadow = true
        panel.isMovableByWindowBackground = false
        panel.collectionBehavior = [.moveToActiveSpace]
        panel.standardWindowButton(.closeButton)?.isHidden = true
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true

        super.init(panel: panel)

        let contentView = GitCredentialDialogView(dialog: dialog, viewModel: viewModel)
        let hostingController = NSHostingController(rootView: contentView)
        hostingController.view.frame = NSRect(x: 0, y: 0, width: 440, height: 1)
        hostingController.view.autoresizingMask = [.width, .height]
        hostingController.view.layoutSubtreeIfNeeded()
        let contentHeight = max(1, hostingController.view.fittingSize.height)

        panel.contentViewController = hostingController
        panel.setContentSize(NSSize(width: 440, height: contentHeight))
        panel.minSize = NSSize(width: 440, height: contentHeight)
        panel.maxSize = NSSize(width: 440, height: contentHeight)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func prepareForPresentation() {
        viewModel.onConfirm = { [weak self] credential in
            self?.finish(with: .continue, value: credential)
        }
        viewModel.onCancel = { [weak self] in
            self?.finish(with: .abort)
        }
    }
}
