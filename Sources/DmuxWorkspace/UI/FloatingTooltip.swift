import AppKit
import SwiftUI

enum FloatingTooltipPlacement {
    case below
    case right
}

@MainActor
final class FloatingTooltipManager {
    static let shared = FloatingTooltipManager()

    private let panel: FloatingTooltipPanel
    private let hostingController: NSHostingController<FloatingTooltipBubbleView>
    private weak var parentWindow: NSWindow?
    private var currentText: String?

    private init() {
        panel = FloatingTooltipPanel(
            contentRect: NSRect(x: 0, y: 0, width: 10, height: 10),
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.isFloatingPanel = true
        panel.level = .statusBar
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = false
        panel.hidesOnDeactivate = false
        panel.ignoresMouseEvents = true
        panel.collectionBehavior = [.moveToActiveSpace, .transient]

        hostingController = NSHostingController(rootView: FloatingTooltipBubbleView(text: ""))
        hostingController.view.translatesAutoresizingMaskIntoConstraints = false
        hostingController.view.wantsLayer = true
        hostingController.view.layer?.masksToBounds = false
        panel.contentViewController = hostingController
    }

    func show(text: String, from anchorView: NSView, placement: FloatingTooltipPlacement) {
        guard let window = anchorView.window else { return }

        let anchorRectInWindow = anchorView.convert(anchorView.bounds, to: nil)
        let anchorRectOnScreen = window.convertToScreen(anchorRectInWindow)
        let contentSize = FloatingTooltipBubbleView.size(for: text)
        let origin = tooltipOrigin(
            for: anchorRectOnScreen,
            contentSize: contentSize,
            placement: placement
        )

        hostingController.rootView = FloatingTooltipBubbleView(text: text)
        panel.setFrame(NSRect(origin: origin, size: contentSize), display: false)

        if parentWindow !== window {
            if let parentWindow {
                parentWindow.removeChildWindow(panel)
            }
            window.addChildWindow(panel, ordered: .above)
            self.parentWindow = window
        }

        if !panel.isVisible {
            panel.orderFront(nil)
        }

        currentText = text
    }

    func hide(text: String? = nil) {
        if let text, currentText != text {
            return
        }
        panel.orderOut(nil)
        currentText = nil
    }

    private func tooltipOrigin(
        for anchorRect: NSRect,
        contentSize: NSSize,
        placement: FloatingTooltipPlacement
    ) -> NSPoint {
        let offset: CGFloat = 8
        switch placement {
        case .below:
            return NSPoint(
                x: round(anchorRect.midX - contentSize.width / 2),
                y: round(anchorRect.minY - contentSize.height - offset)
            )
        case .right:
            return NSPoint(
                x: round(anchorRect.maxX + offset),
                y: round(anchorRect.midY - contentSize.height / 2)
            )
        }
    }
}

private final class FloatingTooltipPanel: NSPanel {
    override var canBecomeKey: Bool { false }
    override var canBecomeMain: Bool { false }
}

private struct FloatingTooltipBubbleView: View {
    let text: String

    static func size(for text: String) -> NSSize {
        let font = NSFont.systemFont(ofSize: 12, weight: .medium)
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        let width = ceil((text as NSString).size(withAttributes: attributes).width) + 20
        return NSSize(width: width, height: 30)
    }

    var body: some View {
        Text(text)
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(AppTheme.textPrimary)
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(AppTheme.panel.opacity(0.98))
            )
            .fixedSize()
    }
}

private struct FloatingTooltipAnchorView: NSViewRepresentable {
    @Binding var view: NSView?

    func makeNSView(context: Context) -> NSView {
        let nsView = NSView(frame: .zero)
        DispatchQueue.main.async {
            view = nsView
        }
        return nsView
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        DispatchQueue.main.async {
            view = nsView
        }
    }
}

struct FloatingTooltipModifier: ViewModifier {
    let text: String
    let enabled: Bool
    let placement: FloatingTooltipPlacement

    @State private var anchorView: NSView?

    func body(content: Content) -> some View {
        content
            .background(FloatingTooltipAnchorView(view: $anchorView))
            .onHover { hovering in
                guard enabled else {
                    FloatingTooltipManager.shared.hide(text: text)
                    return
                }
                guard let anchorView else { return }
                if hovering {
                    FloatingTooltipManager.shared.show(text: text, from: anchorView, placement: placement)
                } else {
                    FloatingTooltipManager.shared.hide(text: text)
                }
            }
            .onDisappear {
                FloatingTooltipManager.shared.hide(text: text)
            }
    }
}

extension View {
    func floatingTooltip(_ text: String, enabled: Bool = true, placement: FloatingTooltipPlacement) -> some View {
        modifier(FloatingTooltipModifier(text: text, enabled: enabled, placement: placement))
    }
}
