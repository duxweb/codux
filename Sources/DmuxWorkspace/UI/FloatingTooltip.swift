import AppKit
import SwiftUI

enum FloatingTooltipPlacement {
    case below
    case right
}

private struct FloatingTooltipBubbleView: View {
    let text: String
    let textWidth: CGFloat
    static let maxWidth: CGFloat = 240
    private static let horizontalPadding: CGFloat = 10
    private static let textMaxWidth: CGFloat = maxWidth - horizontalPadding * 2
    private static let font = NSFont.systemFont(ofSize: 12, weight: .medium)

    static func preferredTextWidth(for text: String) -> CGFloat {
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        let naturalWidth =
            text
            .components(separatedBy: .newlines)
            .map { line in
                let measured = (line.isEmpty ? " " : line) as NSString
                return ceil(measured.size(withAttributes: attributes).width)
            }
            .max() ?? 1
        return max(1, min(textMaxWidth, naturalWidth))
    }

    var body: some View {
        Text(text)
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(AppTheme.textPrimary)
            .multilineTextAlignment(.leading)
            .lineLimit(nil)
            .frame(width: textWidth, alignment: .leading)
            .padding(.horizontal, Self.horizontalPadding)
            .padding(.vertical, 6)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(AppTheme.panel.opacity(0.98))
            )
            .fixedSize(horizontal: true, vertical: true)
            .shadow(color: Color.black.opacity(0.16), radius: 10, x: 0, y: 4)
            .allowsHitTesting(false)
    }
}

private struct FloatingTooltipAnchorReader: NSViewRepresentable {
    @Binding var anchorView: NSView?

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async {
            anchorView = view
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        DispatchQueue.main.async {
            anchorView = nsView
        }
    }
}

@MainActor
private final class FloatingTooltipPresenter {
    private var panel: NSPanel?
    private var hostingController: NSHostingController<AnyView>?
    private var showWorkItem: DispatchWorkItem?

    func scheduleShow(text: String, placement: FloatingTooltipPlacement, anchorView: NSView?) {
        showWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            Task { @MainActor in
                self?.show(text: text, placement: placement, anchorView: anchorView)
            }
        }
        showWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.08, execute: workItem)
    }

    func show(text: String, placement: FloatingTooltipPlacement, anchorView: NSView?) {
        guard let anchorView,
            let anchorWindow = anchorView.window,
            !text.isEmpty
        else {
            hide()
            return
        }

        let textWidth = FloatingTooltipBubbleView.preferredTextWidth(for: text)
        let content = AnyView(
            FloatingTooltipBubbleView(text: text, textWidth: textWidth)
                .padding(4)
                .background(Color.clear)
        )
        let hostingController = self.hostingController ?? NSHostingController(rootView: content)
        hostingController.rootView = content
        let targetSize = CGSize(
            width: FloatingTooltipBubbleView.maxWidth + 8, height: CGFloat.greatestFiniteMagnitude)
        let fittingSize = hostingController.sizeThatFits(in: targetSize)
        hostingController.view.frame = NSRect(origin: .zero, size: fittingSize)
        hostingController.view.layoutSubtreeIfNeeded()
        let contentSize = CGSize(
            width: max(1, min(targetSize.width, fittingSize.width)),
            height: max(1, fittingSize.height)
        )
        hostingController.view.frame = NSRect(origin: .zero, size: contentSize)

        let panel = self.panel ?? makePanel()
        hostingController.view.wantsLayer = true
        hostingController.view.layer?.backgroundColor = NSColor.clear.cgColor
        panel.contentView = hostingController.view
        panel.contentView?.wantsLayer = true
        panel.contentView?.layer?.backgroundColor = NSColor.clear.cgColor
        panel.setContentSize(contentSize)
        self.hostingController = hostingController
        self.panel = panel

        let anchorRectInWindow = anchorView.convert(anchorView.bounds, to: nil)
        let anchorRect = anchorWindow.convertToScreen(anchorRectInWindow)
        panel.setFrameOrigin(
            origin(
                for: placement, anchorRect: anchorRect, tooltipSize: contentSize,
                screen: anchorWindow.screen))
        panel.orderFront(nil)
    }

    func hide() {
        showWorkItem?.cancel()
        showWorkItem = nil
        panel?.orderOut(nil)
    }

    private func makePanel() -> NSPanel {
        let panel = NSPanel(
            contentRect: .zero,
            styleMask: [.borderless, .nonactivatingPanel],
            backing: .buffered,
            defer: false
        )
        panel.backgroundColor = .clear
        panel.isOpaque = false
        panel.hasShadow = false
        panel.ignoresMouseEvents = true
        panel.hidesOnDeactivate = false
        panel.level = .floating
        panel.collectionBehavior = [.transient, .ignoresCycle]
        return panel
    }

    private func origin(
        for placement: FloatingTooltipPlacement,
        anchorRect: CGRect,
        tooltipSize: CGSize,
        screen: NSScreen?
    ) -> CGPoint {
        let gap: CGFloat = 8
        var point: CGPoint
        switch placement {
        case .below:
            point = CGPoint(
                x: anchorRect.midX - tooltipSize.width / 2,
                y: anchorRect.minY - tooltipSize.height - gap)
        case .right:
            point = CGPoint(x: anchorRect.maxX + gap, y: anchorRect.midY - tooltipSize.height / 2)
        }

        guard let visibleFrame = screen?.visibleFrame ?? NSScreen.main?.visibleFrame else {
            return point
        }
        let padding: CGFloat = 6
        point.x = min(
            max(point.x, visibleFrame.minX + padding),
            visibleFrame.maxX - tooltipSize.width - padding)
        point.y = min(
            max(point.y, visibleFrame.minY + padding),
            visibleFrame.maxY - tooltipSize.height - padding)
        return point
    }
}

private struct FloatingTooltipModifier: ViewModifier {
    let text: String
    let enabled: Bool
    let placement: FloatingTooltipPlacement

    @State private var anchorView: NSView?
    @State private var isHovered = false
    @State private var presenter = FloatingTooltipPresenter()

    func body(content: Content) -> some View {
        content
            .overlay {
                GeometryReader { proxy in
                    FloatingTooltipAnchorReader(anchorView: $anchorView)
                        .frame(width: proxy.size.width, height: proxy.size.height)
                        .allowsHitTesting(false)
                }
            }
            .onHover { hovering in
                isHovered = hovering
                updateTooltip()
            }
            .onChange(of: enabled) { _, _ in updateTooltip() }
            .onChange(of: text) { _, _ in updateTooltip() }
            .onDisappear { presenter.hide() }
    }

    private func updateTooltip() {
        let trimmedText = text.trimmingCharacters(in: .whitespacesAndNewlines)
        if enabled, isHovered, !trimmedText.isEmpty {
            presenter.scheduleShow(text: trimmedText, placement: placement, anchorView: anchorView)
        } else {
            presenter.hide()
        }
    }
}

extension View {
    func floatingTooltip(_ text: String, enabled: Bool = true, placement: FloatingTooltipPlacement)
        -> some View
    {
        modifier(FloatingTooltipModifier(text: text, enabled: enabled, placement: placement))
    }
}
