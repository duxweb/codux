import AppKit
import SwiftUI

private final class AgentSplitPanelViewModel: ObservableObject {
    @Published var selectedTool: AgentToolKind
    var onConfirm: ((AgentSplitDialogResult) -> Void)?
    var onCancel: (() -> Void)?

    init(dialog: AgentSplitDialogState) {
        selectedTool = dialog.selectedTool
    }
}

private struct AgentSplitPanelView: View {
    let dialog: AgentSplitDialogState
    @ObservedObject var viewModel: AgentSplitPanelViewModel

    private var header: AppDialogHeaderSpec {
        AppDialogHeaderSpec(
            title: dialog.title,
            message: dialog.message,
            icon: "sparkles",
            iconColor: AppTheme.focus
        )
    }

    var body: some View {
        AppDialogFormLayout(
            header: header,
            width: 460,
            chromeTopInset: 8,
            contentSpacing: 10,
            headerTopPadding: 20,
            headerBottomPadding: 12,
            contentTopPadding: 4,
            contentBottomPadding: 8,
            footerTopPadding: 10,
            footerBottomPadding: 20
        ) {
            VStack(spacing: 8) {
                ForEach(dialog.tools) { tool in
                    AgentToolChoiceRow(
                        tool: tool,
                        isSelected: tool == viewModel.selectedTool
                    ) {
                        viewModel.selectedTool = tool
                    }
                }
            }
        } actions: {
            Button(String(localized: "common.cancel", defaultValue: "Cancel", bundle: .module)) { viewModel.onCancel?() }
                .buttonStyle(AppDialogSecondaryButtonStyle())
                .keyboardShortcut(.cancelAction)

            Button(dialog.confirmTitle) {
                viewModel.onConfirm?(AgentSplitDialogResult(tool: viewModel.selectedTool))
            }
            .buttonStyle(AppDialogPrimaryButtonStyle())
            .keyboardShortcut(.return, modifiers: [])
        }
    }
}

private struct AgentToolChoiceRow: View {
    let tool: AgentToolKind
    let isSelected: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Image(systemName: tool.symbolName)
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(isSelected ? AppTheme.focus : AppTheme.textSecondary)
                    .frame(width: 20)

                Text(tool.displayName)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(AppTheme.textPrimary)

                Spacer(minLength: 0)

                if isSelected {
                    Image(systemName: "checkmark.circle.fill")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(AppTheme.focus)
                }
            }
            .padding(.horizontal, 12)
            .frame(height: 42)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(isSelected ? AppTheme.focus.opacity(0.12) : Color(nsColor: .tertiarySystemFill).opacity(0.62))
            )
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(isSelected ? AppTheme.focus.opacity(0.35) : Color(nsColor: .separatorColor).opacity(0.24), lineWidth: 1)
            }
        }
        .buttonStyle(.plain)
    }
}

final class AgentSplitPanelController: AppDialogController<AgentSplitDialogResult> {
    private let viewModel: AgentSplitPanelViewModel

    init(dialog: AgentSplitDialogState) {
        viewModel = AgentSplitPanelViewModel(dialog: dialog)

        let width: CGFloat = 460
        let panel = AppDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: width, height: 280),
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

        let contentView = AgentSplitPanelView(dialog: dialog, viewModel: viewModel)
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
        viewModel.onConfirm = { [weak self] result in
            self?.finish(with: .continue, value: result)
        }
        viewModel.onCancel = { [weak self] in
            self?.finish(with: .abort)
        }
    }
}
