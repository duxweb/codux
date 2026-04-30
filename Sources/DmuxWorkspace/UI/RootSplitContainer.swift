import AppKit
import SwiftUI

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
    private let model: AppModel
    private let splitView = DividerStyledHorizontalSplitView()
    private let workspaceHosting: NSHostingController<WorkspaceView>
    private let rightPanelHosting: NSHostingController<RightPanelContainerView>
    private let workspaceChromeContainer = NSView()
    private let workspaceContainer = NSView()
    private let workspaceBorderView = WorkspaceTopLeftBorderView()
    private let rightPanelContainer = NSView()
    private let rightPanelTopBorder = BorderLineView()
    private let rightPanelLeftBorder = BorderLineView()
    private var rightPanelWidthConstraint: NSLayoutConstraint?
    private var isApplyingLayout = false
    private var collapsedRightPanelSize = NSSize(width: 0, height: 0)
    private let workspaceCornerRadius: CGFloat = 22

    init(model: AppModel) {
        self.model = model
        self.workspaceHosting = NSHostingController(rootView: WorkspaceView(model: model))
        self.rightPanelHosting = NSHostingController(rootView: RightPanelContainerView(model: model))
        super.init(nibName: nil, bundle: nil)
        splitView.dividerStyle = .thin
        splitView.isVertical = true
        splitView.delegate = self
        splitView.customDividerColor = .clear
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
        splitView.customDividerColor = isPanelVisible ? .separatorColor : .clear
        splitView.showsCustomDivider = isPanelVisible
        rightPanelHosting.view.alphaValue = isPanelVisible ? 1 : 0
        rightPanelTopBorder.isHidden = !isPanelVisible
        rightPanelLeftBorder.isHidden = true

        if isPanelVisible {
            let clampedWidth = min(max(rightPanelWidth, 280), 560)
            rightPanelWidthConstraint?.constant = clampedWidth
            let totalWidth = splitView.bounds.width
            let position = max(max(520, totalWidth - 560), min(totalWidth - 280, totalWidth - clampedWidth))
            splitView.setPosition(position, ofDividerAt: 0)
        } else {
            rightPanelWidthConstraint?.constant = 0
            splitView.setPosition(splitView.bounds.width, ofDividerAt: 0)
            collapsedRightPanelSize.height = splitView.bounds.height
            rightPanelContainer.setFrameSize(collapsedRightPanelSize)
        }

        splitView.adjustSubviews()
        splitView.needsDisplay = true
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
        model.updateRightPanelWidth(width)
    }

    func splitView(_ splitView: NSSplitView, shouldAdjustSizeOfSubview view: NSView) -> Bool {
        view === workspaceChromeContainer
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        let totalWidth = splitView.bounds.width
        return max(520, totalWidth - 560)
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        splitView.bounds.width - 280
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

final class DividerStyledHorizontalSplitView: NSSplitView {
    var customDividerColor: NSColor = .separatorColor
    var showsCustomDivider = true

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
}
