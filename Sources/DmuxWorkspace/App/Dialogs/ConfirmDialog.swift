import AppKit
import SwiftUI

private struct ConfirmDialogView: View {
    let dialog: ConfirmDialogState
    let onAction: (ConfirmDialogResult) -> Void

    private var header: AppDialogHeaderSpec {
        AppDialogHeaderSpec(title: dialog.title, message: dialog.message, icon: dialog.icon, iconColor: dialog.iconColor)
    }

    var body: some View {
        AppDialogFormLayout(
            header: header,
            width: 440,
            chromeTopInset: 8,
            contentSpacing: 0,
            headerTopPadding: 20,
            headerBottomPadding: 12,
            contentTopPadding: 0,
            contentBottomPadding: 4,
            footerTopPadding: 12,
            footerBottomPadding: 18
        ) {
            EmptyView()
        } actions: {
            if let cancelTitle = dialog.cancelTitle {
                Button(cancelTitle) { onAction(.cancel) }
                    .buttonStyle(AppDialogSecondaryButtonStyle())
                    .keyboardShortcut(.cancelAction)
            }

            if let secondaryTitle = dialog.secondaryTitle {
                Button(secondaryTitle) { onAction(.secondary) }
                    .buttonStyle(AppDialogSecondaryButtonStyle())

                Button(dialog.primaryTitle) { onAction(.primary) }
                    .buttonStyle(AppDialogPrimaryButtonStyle(tint: dialog.primaryTint))
            } else {
                Button(dialog.primaryTitle) { onAction(.primary) }
                    .buttonStyle(AppDialogPrimaryButtonStyle(tint: dialog.primaryTint))
                    .keyboardShortcut(.return, modifiers: [])
            }
        }
    }
}

final class ConfirmDialogController: AppDialogController<ConfirmDialogResult> {
    init(dialog: ConfirmDialogState) {
        let panel = AppDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: 440, height: 200),
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

        let contentView = ConfirmDialogView(dialog: dialog) { [weak self] result in
            self?.finish(with: result == .cancel ? .abort : .continue, value: result)
        }
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
}
