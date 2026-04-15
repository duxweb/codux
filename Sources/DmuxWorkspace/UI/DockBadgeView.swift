import AppKit

final class AppDockTileBadgeView: NSView {
    private let icon: NSImage
    private let count: Int

    init(icon: NSImage, count: Int) {
        self.icon = icon
        self.count = count
        super.init(frame: NSRect(origin: .zero, size: NSSize(width: 128, height: 128)))
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        icon.draw(in: bounds)

        let badgeDiameter: CGFloat = 34
        let badgeRect = NSRect(
            x: bounds.maxX - badgeDiameter - 8,
            y: 8,
            width: badgeDiameter,
            height: badgeDiameter
        )

        let badgePath = NSBezierPath(ovalIn: badgeRect)
        NSColor(calibratedRed: 0.18, green: 0.78, blue: 0.36, alpha: 1).setFill()
        badgePath.fill()

        NSColor.white.withAlphaComponent(0.22).setStroke()
        badgePath.lineWidth = 1.5
        badgePath.stroke()

        let text = count > 99 ? "99+" : "\(count)"
        let paragraph = NSMutableParagraphStyle()
        paragraph.alignment = .center
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: count > 99 ? 12 : 13, weight: .bold),
            .foregroundColor: NSColor.white,
            .paragraphStyle: paragraph
        ]
        let attributed = NSAttributedString(string: text, attributes: attributes)
        let textRect = badgeRect.offsetBy(dx: 0, dy: 7)
        attributed.draw(in: textRect)
    }
}
