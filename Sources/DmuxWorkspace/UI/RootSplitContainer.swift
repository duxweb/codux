import AppKit
import SwiftUI

@MainActor
struct CoduxSplitDividerHitRegion {
    let rect: NSRect
    let cursor: NSCursor
    let targetView: NSView
}

@MainActor
protocol CoduxSplitDividerHitRegionProviding: AnyObject {
    func coduxSplitDividerHitRegions() -> [CoduxSplitDividerHitRegion]
}

extension NSSplitView {
    func coduxExpandedDividerRect(at dividerIndex: Int, outset: CGFloat = 12) -> NSRect {
        guard dividerIndex >= 0,
              dividerIndex < max(0, arrangedSubviews.count - 1) else {
            return .zero
        }

        let previousFrame = arrangedSubviews[dividerIndex].frame
        let nextFrame = arrangedSubviews[dividerIndex + 1].frame

        if isVertical {
            let forwardGap = abs(previousFrame.maxX - nextFrame.minX)
            let reverseGap = abs(nextFrame.maxX - previousFrame.minX)
            let dividerX = forwardGap <= reverseGap
                ? (previousFrame.maxX + nextFrame.minX) / 2
                : (nextFrame.maxX + previousFrame.minX) / 2
            return NSRect(
                x: dividerX - outset,
                y: bounds.minY,
                width: outset * 2,
                height: bounds.height
            )
        }

        let forwardGap = abs(previousFrame.maxY - nextFrame.minY)
        let reverseGap = abs(nextFrame.maxY - previousFrame.minY)
        let dividerY = forwardGap <= reverseGap
            ? (previousFrame.maxY + nextFrame.minY) / 2
            : (nextFrame.maxY + previousFrame.minY) / 2
        return NSRect(
            x: bounds.minX,
            y: dividerY - outset,
            width: bounds.width,
            height: outset * 2
        )
    }
}

struct RightPanelContainerView: View {
    let model: AppModel

    var body: some View {
        Group {
            switch model.rightPanel {
            case .git:
                GitPanelView(model: model, gitStore: model.gitStore)
            case .files:
                FileBrowserPanelView(model: model)
            case .aiStats:
                AIStatsPanelView(
                    model: model,
                    store: model.aiStatsStore,
                    currentProject: model.selectedProject,
                    isAutomaticRefreshInProgress: model.aiStatsStore.isAutomaticRefreshInProgress,
                    onRefresh: model.refreshCurrentAIIndexing,
                    onCancel: model.cancelCurrentAIIndexing
                )
            case .taskMemos:
                TaskMemoPanelView(model: model)
            case .ssh:
                SSHPanelView(model: model)
            case nil:
                Color.clear
            }
        }
        .background(Color.clear)
    }
}

struct TerminalHorizontalSplitContainer: NSViewControllerRepresentable {
    let model: AppModel

    func makeNSViewController(context: Context) -> TerminalHorizontalSplitController {
        TerminalHorizontalSplitController(model: model)
    }

    func updateNSViewController(_ controller: TerminalHorizontalSplitController, context: Context) {
        controller.updateLayout(rightPanelWidth: model.rightPanelWidth, rightPanel: model.rightPanel)
    }
}

final class TerminalHorizontalSplitController: NSViewController, NSSplitViewDelegate {
    private struct AppliedLayoutSignature: Equatable {
        var totalWidth: Int
        var rightPanel: RightPanelKind?
        var targetRightWidth: Int
        var targetDividerPosition: Int
    }

    private let model: AppModel
    private let splitView = DividerStyledHorizontalSplitView()
    private let workspaceHosting: NSHostingController<WorkspaceShellView>
    private let rightPanelHosting: NSHostingController<RightPanelContainerView>
    private let workspaceChromeContainer = NSView()
    private let workspaceContainer = NSView()
    private let workspaceBorderView = WorkspaceTopLeftBorderView()
    private let rightPanelContainer = NSView()
    private let rightPanelTopBorder = BorderLineView()
    private let rightPanelLeftBorder = BorderLineView()
    private var rightPanelWidthConstraint: NSLayoutConstraint?
    private var isApplyingLayout = false
    private var lastAppliedLayoutSignature: AppliedLayoutSignature?
    private var lastPanelVisibility: Bool?
    private var collapsedRightPanelSize = NSSize(width: 0, height: 0)
    private let workspaceCornerRadius: CGFloat = 22

    init(model: AppModel) {
        self.model = model
        self.workspaceHosting = NSHostingController(rootView: WorkspaceShellView(model: model))
        self.rightPanelHosting = NSHostingController(rootView: RightPanelContainerView(model: model))
        super.init(nibName: nil, bundle: nil)
        splitView.dividerStyle = .thin
        splitView.isVertical = true
        splitView.delegate = self
        splitView.customDividerColor = .clear
        splitView.dividerCursor = .resizeLeftRight
        splitView.isDividerInteractive = false
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = NSView()
        view.wantsLayer = true
        view.layer?.backgroundColor = NSColor.clear.cgColor

        splitView.translatesAutoresizingMaskIntoConstraints = false
        splitView.wantsLayer = true
        splitView.layer?.backgroundColor = NSColor.clear.cgColor
        view.addSubview(splitView)

        NSLayoutConstraint.activate([
            splitView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            splitView.topAnchor.constraint(equalTo: view.topAnchor),
            splitView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        addChild(workspaceHosting)
        addChild(rightPanelHosting)
        splitView.addArrangedSubview(workspaceChromeContainer)
        splitView.addArrangedSubview(rightPanelContainer)

        workspaceChromeContainer.translatesAutoresizingMaskIntoConstraints = false
        workspaceChromeContainer.wantsLayer = true
        workspaceChromeContainer.layer?.backgroundColor = NSColor.clear.cgColor
        workspaceChromeContainer.layer?.masksToBounds = true
        workspaceChromeContainer.layer?.cornerRadius = 22
        workspaceChromeContainer.layer?.maskedCorners = [.layerMinXMaxYCorner]
        workspaceContainer.translatesAutoresizingMaskIntoConstraints = false
        workspaceContainer.wantsLayer = true
        workspaceContainer.layer?.backgroundColor = NSColor.clear.cgColor
        workspaceContainer.layer?.masksToBounds = true
        workspaceContainer.layer?.cornerRadius = workspaceCornerRadius
        workspaceContainer.layer?.maskedCorners = [.layerMinXMaxYCorner]
        workspaceHosting.view.translatesAutoresizingMaskIntoConstraints = false
        workspaceHosting.view.wantsLayer = true
        workspaceHosting.view.layer?.backgroundColor = NSColor.clear.cgColor
        workspaceBorderView.cornerRadius = workspaceCornerRadius
        rightPanelContainer.translatesAutoresizingMaskIntoConstraints = false
        rightPanelContainer.wantsLayer = true
        rightPanelContainer.layer?.backgroundColor = NSColor.clear.cgColor
        workspaceChromeContainer.setContentHuggingPriority(.defaultLow, for: .horizontal)
        rightPanelContainer.setContentCompressionResistancePriority(.required, for: .horizontal)
        splitView.setHoldingPriority(.defaultLow, forSubviewAt: 0)
        splitView.setHoldingPriority(.dragThatCanResizeWindow, forSubviewAt: 1)
        collapsedRightPanelSize = NSSize(width: 0, height: splitView.bounds.height)

        workspaceChromeContainer.addSubview(workspaceContainer)
        NSLayoutConstraint.activate([
            workspaceContainer.leadingAnchor.constraint(equalTo: workspaceChromeContainer.leadingAnchor),
            workspaceContainer.trailingAnchor.constraint(equalTo: workspaceChromeContainer.trailingAnchor),
            workspaceContainer.topAnchor.constraint(equalTo: workspaceChromeContainer.topAnchor),
            workspaceContainer.bottomAnchor.constraint(equalTo: workspaceChromeContainer.bottomAnchor),
        ])

        workspaceContainer.addSubview(workspaceHosting.view)
        NSLayoutConstraint.activate([
            workspaceHosting.view.leadingAnchor.constraint(equalTo: workspaceContainer.leadingAnchor),
            workspaceHosting.view.trailingAnchor.constraint(equalTo: workspaceContainer.trailingAnchor),
            workspaceHosting.view.topAnchor.constraint(equalTo: workspaceContainer.topAnchor),
            workspaceHosting.view.bottomAnchor.constraint(equalTo: workspaceContainer.bottomAnchor),
        ])

        workspaceChromeContainer.addSubview(workspaceBorderView)
        NSLayoutConstraint.activate([
            workspaceBorderView.leadingAnchor.constraint(equalTo: workspaceChromeContainer.leadingAnchor),
            workspaceBorderView.trailingAnchor.constraint(equalTo: workspaceChromeContainer.trailingAnchor),
            workspaceBorderView.topAnchor.constraint(equalTo: workspaceChromeContainer.topAnchor),
            workspaceBorderView.bottomAnchor.constraint(equalTo: workspaceChromeContainer.bottomAnchor),
        ])

        rightPanelContainer.addSubview(rightPanelHosting.view)
        rightPanelHosting.view.translatesAutoresizingMaskIntoConstraints = false
        rightPanelHosting.view.wantsLayer = true
        rightPanelHosting.view.layer?.backgroundColor = NSColor.clear.cgColor
        NSLayoutConstraint.activate([
            rightPanelHosting.view.leadingAnchor.constraint(equalTo: rightPanelContainer.leadingAnchor),
            rightPanelHosting.view.trailingAnchor.constraint(equalTo: rightPanelContainer.trailingAnchor),
            rightPanelHosting.view.topAnchor.constraint(equalTo: rightPanelContainer.topAnchor),
            rightPanelHosting.view.bottomAnchor.constraint(equalTo: rightPanelContainer.bottomAnchor),
        ])

        rightPanelContainer.addSubview(rightPanelTopBorder)
        rightPanelContainer.addSubview(rightPanelLeftBorder)
        NSLayoutConstraint.activate([
            rightPanelTopBorder.leadingAnchor.constraint(equalTo: rightPanelContainer.leadingAnchor),
            rightPanelTopBorder.trailingAnchor.constraint(equalTo: rightPanelContainer.trailingAnchor),
            rightPanelTopBorder.topAnchor.constraint(equalTo: rightPanelContainer.topAnchor),
            rightPanelTopBorder.heightAnchor.constraint(equalToConstant: 1),

            rightPanelLeftBorder.leadingAnchor.constraint(equalTo: rightPanelContainer.leadingAnchor),
            rightPanelLeftBorder.topAnchor.constraint(equalTo: rightPanelContainer.topAnchor),
            rightPanelLeftBorder.bottomAnchor.constraint(equalTo: rightPanelContainer.bottomAnchor),
            rightPanelLeftBorder.widthAnchor.constraint(equalToConstant: 1),
        ])

        let widthConstraint = rightPanelContainer.widthAnchor.constraint(equalToConstant: 0)
        widthConstraint.isActive = true
        rightPanelWidthConstraint = widthConstraint

        refreshGhosttyPortalHostRegistration()
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        refreshGhosttyPortalHostRegistration()
        updateLayout(rightPanelWidth: model.rightPanelWidth, rightPanel: model.rightPanel)
    }

    override func viewDidAppear() {
        super.viewDidAppear()
        refreshGhosttyPortalHostRegistration()
    }

    override func viewDidDisappear() {
        super.viewDidDisappear()
        guard let window = view.window else {
            return
        }
        GhosttyPortalHostRegistry.unregister(hostView: workspaceContainer, for: window)
    }

    func updateLayout(rightPanelWidth: CGFloat, rightPanel: RightPanelKind?) {
        guard view.bounds.width > 0, !isApplyingLayout else { return }
        isApplyingLayout = true
        defer { isApplyingLayout = false }

        refreshGhosttyPortalHostRegistration()

        workspaceChromeContainer.layer?.backgroundColor = NSColor(model.terminalChromeColor).cgColor
        workspaceChromeContainer.layer?.borderWidth = 0
        workspaceBorderView.strokeColor = .separatorColor
        rightPanelTopBorder.lineColor = .separatorColor
        let isPanelVisible = rightPanel != nil
        let panelVisibilityChanged = lastPanelVisibility != isPanelVisible
        if isPanelVisible {
            rightPanelContainer.isHidden = false
            rightPanelHosting.view.isHidden = false
        }
        splitView.customDividerColor = isPanelVisible ? .separatorColor : .clear
        splitView.showsCustomDivider = isPanelVisible
        splitView.isDividerInteractive = isPanelVisible
        rightPanelTopBorder.isHidden = !isPanelVisible
        rightPanelLeftBorder.isHidden = true

        let totalWidth = splitView.bounds.width
        let targetRightWidth: CGFloat
        let targetPosition: CGFloat
        if isPanelVisible {
            let minWorkspaceWidth: CGFloat = 520
            let preferredMinRightWidth: CGFloat = 280
            let absoluteMaxRightWidth: CGFloat = 560
            let availableMaxRightWidth = max(280, min(absoluteMaxRightWidth, totalWidth - minWorkspaceWidth))
            let minRightWidth = min(preferredMinRightWidth, availableMaxRightWidth)
            let clampedWidth = min(max(rightPanelWidth, minRightWidth), availableMaxRightWidth)
            targetRightWidth = clampedWidth
            targetPosition = totalWidth - clampedWidth
        } else {
            targetRightWidth = 0
            targetPosition = totalWidth
        }

        let signature = AppliedLayoutSignature(
            totalWidth: Int(totalWidth.rounded()),
            rightPanel: rightPanel,
            targetRightWidth: Int(targetRightWidth.rounded()),
            targetDividerPosition: Int(targetPosition.rounded())
        )
        let currentDividerPosition = workspaceChromeContainer.frame.maxX
        let shouldApplyFrames = lastAppliedLayoutSignature != signature ||
            abs((rightPanelWidthConstraint?.constant ?? 0) - targetRightWidth) > 0.5 ||
            abs(currentDividerPosition - targetPosition) > 0.5

        guard shouldApplyFrames else {
            splitView.needsDisplay = true
            if panelVisibilityChanged {
                splitView.window?.invalidateCursorRects(for: splitView)
            }
            lastPanelVisibility = isPanelVisible
            return
        }

        rightPanelWidthConstraint?.constant = targetRightWidth
        splitView.setPosition(targetPosition, ofDividerAt: 0)

        if !isPanelVisible {
            collapsedRightPanelSize.height = splitView.bounds.height
            rightPanelContainer.setFrameSize(collapsedRightPanelSize)
            rightPanelHosting.view.isHidden = true
            rightPanelContainer.isHidden = true
        }

        splitView.adjustSubviews()
        splitView.needsDisplay = true
        if panelVisibilityChanged {
            splitView.window?.invalidateCursorRects(for: splitView)
        }
        lastPanelVisibility = isPanelVisible
        lastAppliedLayoutSignature = signature
    }

    private func refreshGhosttyPortalHostRegistration() {
        guard let window = view.window else {
            return
        }
        GhosttyPortalHostRegistry.register(hostView: workspaceContainer, for: window)
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        guard model.rightPanel != nil, !isApplyingLayout else { return }
        let width = rightPanelContainer.frame.width
        rightPanelWidthConstraint?.constant = width
        lastAppliedLayoutSignature = nil
        model.updateRightPanelWidth(width)
    }

    func splitView(_ splitView: NSSplitView, shouldAdjustSizeOfSubview view: NSView) -> Bool {
        view === workspaceChromeContainer
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        guard model.rightPanel != nil else {
            return splitView.bounds.width
        }
        let totalWidth = splitView.bounds.width
        return max(520, totalWidth - 560)
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        guard model.rightPanel != nil else {
            return splitView.bounds.width
        }
        return splitView.bounds.width - 280
    }

    func splitView(_ splitView: NSSplitView, additionalEffectiveRectOfDividerAt dividerIndex: Int) -> NSRect {
        guard model.rightPanel != nil,
              let splitView = splitView as? DividerStyledHorizontalSplitView,
              splitView.isDividerInteractive else {
            return .zero
        }
        return splitView.coduxExpandedDividerRect(at: dividerIndex)
    }

    func splitView(_ splitView: NSSplitView, shouldHideDividerAt dividerIndex: Int) -> Bool {
        model.rightPanel == nil
    }
}

private struct WorkspaceShellView: View {
    let model: AppModel

    var body: some View {
        HStack(spacing: 0) {
            if model.selectedProjectID != nil && model.isWorktreeSidebarExpanded {
                WorktreeSidebarView(model: model)
                    .frame(minWidth: 216, idealWidth: 216, maxWidth: 216)
                    .fixedSize(horizontal: true, vertical: false)
            }

            WorkspaceView(model: model)
                .frame(minWidth: 520, maxWidth: .infinity, maxHeight: .infinity)
        }
        .background(model.terminalChromeColor)
    }
}

private final class BorderLineView: NSView {
    var lineColor: NSColor = .separatorColor {
        didSet {
            layer?.backgroundColor = lineColor.cgColor
        }
    }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.backgroundColor = lineColor.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}

private final class WorkspaceTopLeftBorderView: NSView {
    var strokeColor: NSColor = .separatorColor {
        didSet {
            needsDisplay = true
        }
    }

    var cornerRadius: CGFloat = 22 {
        didSet {
            needsDisplay = true
        }
    }

    private let lineWidth: CGFloat = 1

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        translatesAutoresizingMaskIntoConstraints = false
        wantsLayer = true
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }

    override func draw(_ dirtyRect: NSRect) {
        super.draw(dirtyRect)

        guard let context = NSGraphicsContext.current?.cgContext else {
            return
        }

        let insetBounds = bounds.insetBy(dx: lineWidth / 2, dy: lineWidth / 2)
        let radius = min(cornerRadius, min(insetBounds.width, insetBounds.height) / 2)
        let path = CGMutablePath()

        path.move(to: CGPoint(x: insetBounds.minX, y: insetBounds.minY))
        path.addLine(to: CGPoint(x: insetBounds.minX, y: insetBounds.maxY - radius))
        path.addArc(
            tangent1End: CGPoint(x: insetBounds.minX, y: insetBounds.maxY),
            tangent2End: CGPoint(x: insetBounds.minX + radius, y: insetBounds.maxY),
            radius: radius
        )
        path.addLine(to: CGPoint(x: insetBounds.maxX, y: insetBounds.maxY))

        context.setShouldAntialias(true)
        context.setStrokeColor(strokeColor.cgColor)
        context.setLineWidth(lineWidth)
        context.setLineCap(.round)
        context.setLineJoin(.round)
        context.addPath(path)
        context.strokePath()
    }
}

final class DividerStyledHorizontalSplitView: NSSplitView, CoduxSplitDividerHitRegionProviding {
    private(set) var isTrackingDividerDrag = false

    var customDividerColor: NSColor = .separatorColor {
        didSet {
            guard oldValue.isEqual(customDividerColor) == false else {
                return
            }
            needsDisplay = true
        }
    }
    var showsCustomDivider = true {
        didSet {
            guard oldValue != showsCustomDivider else {
                return
            }
            window?.invalidateCursorRects(for: self)
        }
    }
    var dividerCursor: NSCursor = .resizeLeftRight {
        didSet {
            guard oldValue !== dividerCursor else {
                return
            }
            window?.invalidateCursorRects(for: self)
        }
    }
    var isDividerInteractive = true {
        didSet {
            guard oldValue != isDividerInteractive else {
                return
            }
            window?.invalidateCursorRects(for: self)
        }
    }

    override var dividerColor: NSColor {
        customDividerColor
    }

    override var dividerThickness: CGFloat {
        showsCustomDivider ? 1 : 0
    }

    override func drawDivider(in rect: NSRect) {
        guard showsCustomDivider else { return }
        customDividerColor.setFill()
        rect.fill()
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard isDividerInteractive else {
            let hitView = super.hitTest(point)
            return hitView === self ? nil : hitView
        }
        if isDividerInteractive, pointIsInsideExpandedDivider(point) {
            return self
        }
        return super.hitTest(point)
    }

    override func mouseDown(with event: NSEvent) {
        guard isDividerInteractive else {
            return
        }
        let point = convert(event.locationInWindow, from: nil)
        guard isDividerInteractive, pointIsInsideExpandedDivider(point) else {
            super.mouseDown(with: event)
            return
        }

        isTrackingDividerDrag = true
        defer { isTrackingDividerDrag = false }
        super.mouseDown(with: event)
    }

    override func mouseMoved(with event: NSEvent) {
        guard isDividerInteractive else {
            NSCursor.arrow.set()
            return
        }
        let point = convert(event.locationInWindow, from: nil)
        if isDividerInteractive, pointIsInsideExpandedDivider(point) {
            dividerCursor.set()
            return
        }
        super.mouseMoved(with: event)
    }

    override func cursorUpdate(with event: NSEvent) {
        guard isDividerInteractive else {
            NSCursor.arrow.set()
            return
        }
        let point = convert(event.locationInWindow, from: nil)
        if isDividerInteractive, pointIsInsideExpandedDivider(point) {
            dividerCursor.set()
            return
        }
        super.cursorUpdate(with: event)
    }

    override func resetCursorRects() {
        guard isDividerInteractive else {
            return
        }
        super.resetCursorRects()
        for region in coduxSplitDividerHitRegions() {
            addCursorRect(region.rect, cursor: region.cursor)
        }
    }

    func coduxSplitDividerHitRegions() -> [CoduxSplitDividerHitRegion] {
        guard isDividerInteractive, dividerThickness > 0 else {
            return []
        }
        return expandedDividerRects().map { rect in
            CoduxSplitDividerHitRegion(rect: rect, cursor: dividerCursor, targetView: self)
        }
    }

    private func pointIsInsideExpandedDivider(_ point: NSPoint) -> Bool {
        coduxSplitDividerHitRegions().contains { $0.rect.contains(point) }
    }

    private func expandedDividerRects() -> [NSRect] {
        guard arrangedSubviews.count > 1 else {
            return []
        }
        return (0 ..< (arrangedSubviews.count - 1)).map { dividerIndex in
            coduxExpandedDividerRect(at: dividerIndex)
        }
    }
}
