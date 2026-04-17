import AppKit
import SwiftUI

struct RootView: View {
    let model: AppModel

    private let titlebarHeight: CGFloat = 42
    private let collapsedSidebarWidth: CGFloat = 70

    var body: some View {
        ZStack(alignment: .top) {
            AppWindowGlassBackground()

            HStack(spacing: 0) {
                SidebarView(model: model)
                    .frame(
                        minWidth: model.isSidebarExpanded ? 248 : collapsedSidebarWidth,
                        idealWidth: model.isSidebarExpanded ? 248 : collapsedSidebarWidth,
                        maxWidth: model.isSidebarExpanded ? 248 : collapsedSidebarWidth
                    )
                    .fixedSize(horizontal: true, vertical: false)

                TerminalHorizontalSplitContainer(model: model)
                    .frame(minWidth: 700, maxWidth: .infinity, maxHeight: .infinity)
            }
            .padding(.top, titlebarHeight)

            TitlebarOverlayView(model: model)
                .frame(height: titlebarHeight)
        }
        .background(Color.clear)
        .background(MainWorkspaceWindowConfigurator())
        .ignoresSafeArea(.container, edges: .top)
        .onAppear {
            model.noteRootViewAppeared()
        }
    }
}

private struct MainWorkspaceWindowConfigurator: NSViewRepresentable {
    func makeNSView(context: Context) -> NSView {
        ConfigView()
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        (nsView as? ConfigView)?.applyWindowConfigurationIfNeeded()
    }

    private final class ConfigView: NSView {
        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            applyWindowConfigurationIfNeeded()
        }

        func applyWindowConfigurationIfNeeded() {
            guard let window else {
                return
            }
            window.identifier = AppWindowIdentifier.main
            applyImmersiveWindowChrome(window)
        }
    }
}

private struct TitlebarOverlayView: View {
    let model: AppModel
    @State private var isShowingLevelPopover = false

    var body: some View {
        let _ = model.aiStatsStore.renderVersion
        let _ = model.performanceMonitor.renderVersion

        ZStack {
            TitlebarZoomSurface()

            HStack(spacing: 0) {
                HStack(spacing: 10) {
                    TitlebarGlyphButton(symbol: model.isSidebarExpanded ? "sidebar.left" : "sidebar.right", help: String(localized: "titlebar.projects", defaultValue: "Projects", bundle: .module)) {
                        model.toggleSidebarExpansion()
                    }

                    if model.appSettings.developer.showsNotificationTestButton {
                        TitlebarGlyphButton(symbol: "waveform.badge.magnifyingglass", help: String(localized: "titlebar.notification_test", defaultValue: "Notification Test", bundle: .module)) {
                            model.triggerActivityTest()
                        }
                    }

                    TitlebarGlyphButton(symbol: "rectangle.split.2x1", help: String(localized: "titlebar.split", defaultValue: "Split", bundle: .module)) {
                        model.splitSelectedPane(axis: .horizontal)
                    }

                    if model.appSettings.developer.showsPerformanceMonitor {
                        TitlebarPerformanceMonitorView(model: model)
                    }
                }
                .padding(.leading, 86)
                .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
                .frame(maxHeight: .infinity, alignment: .leading)

                Spacer(minLength: 0)

                HStack(spacing: 10) {
                    let hasVSCode = ApplicationIconAsset.isInstalled("com.microsoft.VSCode")
                    if model.selectedProject != nil {
                        TitlebarAITodayLevelButton(
                            model: model,
                            tokens: totalTodayTokens,
                            isShowingPopover: $isShowingLevelPopover
                        )
                    }

                    TitlebarOpenSplitButton(
                        model: model,
                        isEnabled: model.selectedProject != nil,
                        prefersVSCode: hasVSCode,
                        primaryAction: {
                            if hasVSCode {
                                model.openSelectedProjectInVSCode()
                            } else {
                                model.revealSelectedProjectInFinder()
                            }
                        },
                        revealInFinder: { model.revealSelectedProjectInFinder() },
                        openInVSCode: { model.openSelectedProjectInVSCode() },
                        openInTerminal: { model.openSelectedProjectInTerminal() },
                        openInITerm2: { model.openSelectedProjectInITerm2() },
                        openInGhostty: { model.openSelectedProjectInGhostty() },
                        openInXcode: { model.openSelectedProjectInXcode() }
                    )

                    if model.appSettings.developer.showsDebugLogButton {
                        TitlebarGlyphButton(symbol: "scroll", help: String(localized: "titlebar.debug_log", defaultValue: "Debug Log", bundle: .module)) {
                            model.openDebugLog()
                        }
                    }

                    TitlebarGlyphButton(symbol: "terminal", help: String(localized: "titlebar.tab", defaultValue: "Tab", bundle: .module)) {
                        model.createBottomTab()
                    }

                    TitlebarGlyphButton(symbol: "chart.bar.xaxis", help: String(localized: "titlebar.ai_assistant", defaultValue: "AI Assistant", bundle: .module)) {
                        model.toggleRightPanel(.aiStats)
                    }

                    TitlebarGlyphButton(symbol: "point.3.filled.connected.trianglepath.dotted", help: String(localized: "titlebar.git", defaultValue: "Git", bundle: .module)) {
                        model.toggleRightPanel(.git)
                    }
                }
                .padding(.trailing, 16)
                .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
                .frame(maxHeight: .infinity, alignment: .center)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)

            ProjectTitleView(project: model.selectedProject)
                .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        .background(Color.clear)
        .onChange(of: model.selectedProjectID) { _, _ in
            isShowingLevelPopover = false
        }
    }

    private var totalTodayTokens: Int {
        guard !model.projects.isEmpty else {
            return 0
        }
        return model.aiStatsStore.totalTodayTokensAcrossProjects(model.projects)
    }
}

private enum TitlebarControlMetrics {
    static let rowHeight: CGFloat = 30
    static let iconButtonSize: CGFloat = 30
    static let glyphIconSize: CGFloat = 15
    static let pillHeight: CGFloat = 26
    static let glyphCornerRadius: CGFloat = 7
    static let pillCornerRadius: CGFloat = 9
}

private struct TitlebarPerformanceMonitorView: View {
    let model: AppModel

    private var cpuColor: Color {
        if model.performanceMonitor.cpuPercent >= 85 {
            return AppTheme.warning
        }
        if model.performanceMonitor.cpuPercent >= 60 {
            return AppTheme.warning.opacity(0.78)
        }
        return AppTheme.textSecondary
    }

    var body: some View {
        HStack(spacing: 8) {
            HStack(spacing: 4) {
                Image(systemName: "cpu")
                    .font(.system(size: 11, weight: .semibold))
                Text(model.performanceMonitor.formattedCPU)
            }
            .foregroundStyle(cpuColor)

            Rectangle()
                .fill(AppTheme.titlebarControlBorder)
                .frame(width: 0.5, height: 12)

            HStack(spacing: 4) {
                Image(systemName: "memorychip")
                    .font(.system(size: 11, weight: .semibold))
                Text(model.performanceMonitor.formattedMemory)
            }
            .foregroundStyle(AppTheme.textSecondary)

            Rectangle()
                .fill(AppTheme.titlebarControlBorder)
                .frame(width: 0.5, height: 12)

            HStack(spacing: 4) {
                Image(systemName: "display")
                    .font(.system(size: 11, weight: .semibold))
                Text(model.performanceMonitor.formattedGraphics)
            }
            .foregroundStyle(AppTheme.textSecondary)
        }
        .font(.system(size: 11.5, weight: .medium, design: .monospaced))
        .environment(\.symbolVariants, .none)
        .padding(.leading, 9)
        .padding(.trailing, 10)
        .frame(height: TitlebarControlMetrics.pillHeight)
        .background(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .fill(AppTheme.emphasizedControlFill)
        )
        .overlay(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .stroke(AppTheme.titlebarControlBorder, lineWidth: 0.5)
        )
        .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
        .floatingTooltip(
            String(localized: "settings.developer.performance_monitor", defaultValue: "Performance Monitor HUD", bundle: .module),
            placement: .below
        )
    }
}

private struct TitlebarZoomSurface: NSViewRepresentable {
    func makeNSView(context: Context) -> TitlebarZoomNSView {
        TitlebarZoomNSView()
    }

    func updateNSView(_ nsView: TitlebarZoomNSView, context: Context) {
    }
}

private final class TitlebarZoomNSView: NSView {
    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var mouseDownCanMoveWindow: Bool {
        true
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseUp(with event: NSEvent) {
        if event.clickCount == 2 {
            window?.performZoom(nil)
            return
        }
        super.mouseUp(with: event)
    }
}

private struct RightPanelContainerView: View {
    let model: AppModel

    var body: some View {
        Group {
            switch model.rightPanel {
            case .git:
                GitPanelView(model: model, gitStore: model.gitStore)
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

private struct TerminalHorizontalSplitContainer: NSViewControllerRepresentable {
    let model: AppModel

    func makeNSViewController(context: Context) -> TerminalHorizontalSplitController {
        TerminalHorizontalSplitController(model: model)
    }

    func updateNSViewController(_ controller: TerminalHorizontalSplitController, context: Context) {
        controller.updateLayout(rightPanelWidth: model.rightPanelWidth, rightPanel: model.rightPanel)
    }
}

private final class TerminalHorizontalSplitController: NSViewController, NSSplitViewDelegate {
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
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        updateLayout(rightPanelWidth: model.rightPanelWidth, rightPanel: model.rightPanel)
    }

    func updateLayout(rightPanelWidth: CGFloat, rightPanel: RightPanelKind?) {
        guard view.bounds.width > 0, !isApplyingLayout else { return }
        isApplyingLayout = true
        defer { isApplyingLayout = false }

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

    func splitViewDidResizeSubviews(_ notification: Notification) {
        guard model.rightPanel != nil, !isApplyingLayout, splitView.isDraggingDivider else { return }
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
    private(set) var isDraggingDivider = false

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

    override func mouseDown(with event: NSEvent) {
        let point = convert(event.locationInWindow, from: nil)
        isDraggingDivider = dividerRect().insetBy(dx: -3, dy: 0).contains(point)
        super.mouseDown(with: event)
        isDraggingDivider = false
    }

    private func dividerRect() -> NSRect {
        guard subviews.count >= 2, showsCustomDivider else { return .zero }
        let x = subviews[0].frame.maxX
        return NSRect(x: x, y: 0, width: dividerThickness, height: bounds.height)
    }
}

private struct TitlebarGlyphButton: View {
    let symbol: String
    let help: String
    let action: () -> Void
    @State private var isHovered = false

    private var opticalOffset: CGFloat {
        switch symbol {
        case "terminal":
            return -0.5
        case "rectangle.split.2x1", "rectangle.split.1x2":
            return 0
        case "sidebar.left", "sidebar.right":
            return 0.25
        case "scroll":
            return -0.25
        default:
            return 0
        }
    }

    var body: some View {
        Button(action: action) {
            ZStack {
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.glyphCornerRadius, style: .continuous)
                    .fill(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)

                Image(systemName: symbol)
                    .font(.system(size: TitlebarControlMetrics.glyphIconSize, weight: .regular))
                    .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
                    .offset(y: opticalOffset)
                    .frame(width: TitlebarControlMetrics.glyphIconSize, height: TitlebarControlMetrics.glyphIconSize)
            }
            .frame(width: TitlebarControlMetrics.iconButtonSize, height: TitlebarControlMetrics.iconButtonSize)
            .contentShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.glyphCornerRadius, style: .continuous))
        }
        .frame(width: TitlebarControlMetrics.iconButtonSize, height: TitlebarControlMetrics.iconButtonSize)
        .buttonStyle(.plain)
        .floatingTooltip(help, placement: .below)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}

private struct TitlebarOpenSplitButton: View {
    let model: AppModel
    let isEnabled: Bool
    let prefersVSCode: Bool
    let primaryAction: () -> Void
    let revealInFinder: () -> Void
    let openInVSCode: () -> Void
    let openInTerminal: () -> Void
    let openInITerm2: () -> Void
    let openInGhostty: () -> Void
    let openInXcode: () -> Void

    @State private var isHovered = false

    private let primaryWidth: CGFloat = 30
    private let menuWidth: CGFloat = 22

    var body: some View {
        HStack(spacing: 0) {
            Button(action: primaryAction) {
                PrimaryOpenIconView(prefersVSCode: prefersVSCode)
                    .frame(width: 16, height: 16)
                    .frame(width: primaryWidth, height: TitlebarControlMetrics.pillHeight)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!isEnabled)

            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(isHovered ? 0.42 : 0.3))
                .frame(width: 0.5, height: 16)

            Menu {
                Button {
                    openInVSCode()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.vscode", defaultValue: "Open in VS Code", bundle: .module),
                        icon: .bundle("com.microsoft.VSCode", fallbackSystemName: "chevron.left.forwardslash.chevron.right")
                    )
                }

                Button {
                    revealInFinder()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.finder", defaultValue: "Open in Finder", bundle: .module),
                        icon: .bundle("com.apple.finder", fallbackSystemName: "folder")
                    )
                }

                Button {
                    openInTerminal()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.terminal", defaultValue: "Open in Terminal", bundle: .module),
                        icon: .bundle("com.apple.Terminal", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInITerm2()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.iterm2", defaultValue: "Open in iTerm2", bundle: .module),
                        icon: .bundle("com.googlecode.iterm2", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInGhostty()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.ghostty", defaultValue: "Open in Ghostty", bundle: .module),
                        icon: .bundle("com.mitchellh.ghostty", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInXcode()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.xcode", defaultValue: "Open in Xcode", bundle: .module),
                        icon: .bundle("com.apple.dt.Xcode", fallbackSystemName: "hammer")
                    )
                }
            } label: {
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(isEnabled ? AppTheme.textSecondary : AppTheme.textMuted)
                    .frame(width: menuWidth, height: TitlebarControlMetrics.pillHeight)
                    .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .disabled(!isEnabled)
        }
        .frame(width: primaryWidth + menuWidth + 1, height: TitlebarControlMetrics.pillHeight)
        .background(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .fill(isHovered ? AppTheme.titlebarControlHoverFill : AppTheme.emphasizedControlFill)
        )
        .overlay(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .stroke(isHovered ? AppTheme.titlebarControlHoverBorder : AppTheme.titlebarControlBorder, lineWidth: 0.5)
        )
        .clipShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
        .floatingTooltip(
            prefersVSCode
            ? String(localized: "open.project.vscode", defaultValue: "Open Project in VS Code", bundle: .module)
            : String(localized: "open.project.finder", defaultValue: "Open Project in Finder", bundle: .module),
            placement: .below
        )
        .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
        .onHover { hovering in
            isHovered = hovering
        }
        .opacity(isEnabled ? 1 : 0.6)
    }
}

private struct TitlebarAITodayLevelButton: View {
    let model: AppModel
    let tokens: Int
    @Binding var isShowingPopover: Bool
    @State private var isHovered = false

    private var level: AITodayLevelTier {
        AITodayLevelTier.current(for: tokens)
    }

    var body: some View {
        Button {
            isShowingPopover.toggle()
        } label: {
            HStack(alignment: .center, spacing: 6) {
                AITodayLevelBadge(level: level, size: 19, compact: true)

                Text(level.localizedTitle(using: model))
                    .font(.system(size: 12.5, weight: .semibold, design: .rounded))
                    .foregroundStyle(AppTheme.textPrimary.opacity(isShowingPopover || isHovered ? 1 : 0.9))
                    .lineLimit(1)
            }
            .padding(.leading, 8)
            .padding(.trailing, 10)
            .frame(height: TitlebarControlMetrics.pillHeight)
            .background(
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                    .fill(
                        isShowingPopover
                        ? level.accent.opacity(0.2)
                        : (isHovered ? level.accent.opacity(0.13) : AppTheme.emphasizedControlFill)
                    )
            )
            .overlay(
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                    .stroke(
                        isShowingPopover
                        ? level.accent.opacity(0.3)
                        : (isHovered ? AppTheme.titlebarControlHoverBorder : AppTheme.titlebarControlBorder),
                        lineWidth: 0.5
                    )
            )
            .clipShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
        }
        .buttonStyle(.plain)
        .floatingTooltip(String(localized: "ai.today_level", defaultValue: "Today's Level", bundle: .module), enabled: !isShowingPopover, placement: .below)
        .popover(isPresented: $isShowingPopover, attachmentAnchor: .rect(.bounds), arrowEdge: .top) {
            AITodayLevelPopover(model: model, tokens: tokens, currentLevel: level)
        }
        .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}

private struct AITodayLevelPopover: View {
    let model: AppModel
    let tokens: Int
    let currentLevel: AITodayLevelTier

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 10) {
                AITodayLevelBadge(level: currentLevel, size: 34)

                VStack(alignment: .leading, spacing: 2) {
                    Text(String(localized: "ai.today_level", defaultValue: "Today's Level", bundle: .module))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.tertiary)

                    Text(currentLevel.localizedTitle(using: model))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)
                }

                Spacer(minLength: 12)

                VStack(alignment: .trailing, spacing: 2) {
                    Text(String(localized: "ai.today_tokens", defaultValue: "Today's Tokens", bundle: .module))
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.tertiary)

                    Text(formatCompactToken(tokens))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)
                }
            }

            VStack(spacing: 6) {
                ForEach(AITodayLevelTier.allCases) { level in
                    let isCurrent = level == currentLevel

                    HStack(spacing: 10) {
                        AITodayLevelBadge(level: level, size: 24)

                        VStack(alignment: .leading, spacing: 1) {
                            Text(level.localizedTitle(using: model))
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(.primary)

                            Text(String(format: String(localized: "common.need_format", defaultValue: "Need %@", bundle: .module), formatCompactToken(level.minimumTokens)))
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(.tertiary)
                        }

                        Spacer(minLength: 12)

                        if isCurrent {
                            Text(String(localized: "common.current", defaultValue: "Current", bundle: .module))
                                .font(.system(size: 11, weight: .bold, design: .rounded))
                                .foregroundStyle(level.accent)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(
                                    Capsule()
                                        .fill(level.accent.opacity(0.16))
                                )
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(isCurrent ? level.accent.opacity(0.1) : Color.clear)
                    )
                }
            }
        }
        .padding(14)
        .frame(width: 280)
    }
}

private struct AITodayLevelBadge: View {
    let level: AITodayLevelTier
    let size: CGFloat
    var compact = false

    var body: some View {
        ZStack {
            Circle()
                .fill(
                    LinearGradient(
                        colors: [level.accent, level.accent.adjustingBrightness(-0.25)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

            Image(systemName: level.symbol)
                .font(.system(size: size * 0.4, weight: .bold))
                .foregroundStyle(.white.opacity(0.95))
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
    }
}

private enum AITodayLevelTier: String, CaseIterable, Identifiable {
    case blankSlate
    case bronze
    case silver
    case gold
    case platinum
    case diamond
    case master
    case grandmaster

    var id: String { rawValue }

    @MainActor
    func localizedTitle(using model: AppModel) -> String {
        switch self {
        case .blankSlate: return String(localized: "rank.iron", defaultValue: "Idle", bundle: .module)
        case .bronze: return String(localized: "rank.bronze", defaultValue: "Light", bundle: .module)
        case .silver: return String(localized: "rank.silver", defaultValue: "Active", bundle: .module)
        case .gold: return String(localized: "rank.gold", defaultValue: "Focus", bundle: .module)
        case .platinum: return String(localized: "rank.platinum", defaultValue: "Intense", bundle: .module)
        case .diamond: return String(localized: "rank.diamond", defaultValue: "Grind", bundle: .module)
        case .master: return String(localized: "rank.master", defaultValue: "Limit", bundle: .module)
        case .grandmaster: return String(localized: "rank.grandmaster", defaultValue: "Godlike", bundle: .module)
        }
    }

    var minimumTokens: Int {
        switch self {
        case .blankSlate: return 0
        case .bronze: return 5_000_000
        case .silver: return 10_000_000
        case .gold: return 30_000_000
        case .platinum: return 70_000_000
        case .diamond: return 100_000_000
        case .master: return 200_000_000
        case .grandmaster: return 300_000_000
        }
    }

    var accent: Color {
        switch self {
        case .blankSlate: return Color(hex: 0x5B616D)
        case .bronze: return Color(hex: 0xC98663)
        case .silver: return Color(hex: 0xC8D1E3)
        case .gold:
            return Color(
                nsColor: NSColor(name: nil) { appearance in
                    let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    if isDark {
                        return NSColor(calibratedRed: 232 / 255, green: 170 / 255, blue: 52 / 255, alpha: 1)
                    }
                    return NSColor(calibratedRed: 221 / 255, green: 126 / 255, blue: 27 / 255, alpha: 1)
                }
            )
        case .platinum: return Color(hex: 0x7ED6D8)
        case .diamond: return Color(hex: 0x59A7FF)
        case .master: return Color(hex: 0x9A72FF)
        case .grandmaster: return Color(hex: 0xFF5E8E)
        }
    }

    var symbol: String {
        switch self {
        case .blankSlate: return "minus"
        case .bronze: return "bolt.fill"
        case .silver: return "shield.fill"
        case .gold: return "star.fill"
        case .platinum: return "star.circle.fill"
        case .diamond: return "diamond.fill"
        case .master: return "crown.fill"
        case .grandmaster: return "crown.fill"
        }
    }

    static func current(for tokens: Int) -> AITodayLevelTier {
        let clamped = max(0, tokens)
        return allCases.last(where: { clamped >= $0.minimumTokens }) ?? .blankSlate
    }
}

private func formatCompactToken(_ value: Int) -> String {
    let clamped = max(0, value)
    if clamped >= 1_000_000 {
        let formatted = Double(clamped) / 1_000_000
        return formatted >= 10 ? "\(Int(formatted.rounded(.down)))M" : String(format: "%.1fM", formatted)
    }
    if clamped >= 1_000 {
        let formatted = Double(clamped) / 1_000
        return formatted >= 10 ? "\(Int(formatted.rounded(.down)))K" : String(format: "%.1fK", formatted)
    }
    return "\(clamped)"
}

private enum AppLauncherMenuIcon {
    case bundle(String, fallbackSystemName: String)
}

private struct AppLauncherMenuLabel: View {
    let title: String
    let icon: AppLauncherMenuIcon

    var body: some View {
        HStack(spacing: 10) {
            switch icon {
            case let .bundle(bundleIdentifier, fallbackSystemName):
                ApplicationIconView(bundleIdentifier: bundleIdentifier, fallbackSystemName: fallbackSystemName)
            }

            Text(title)
        }
    }
}

private struct ApplicationIconView: View {
    let bundleIdentifier: String
    let fallbackSystemName: String

    var body: some View {
        Group {
            if let image = ApplicationIconAsset.image(for: bundleIdentifier) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
            } else {
                Image(systemName: fallbackSystemName)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(AppTheme.textSecondary)
            }
        }
        .frame(width: 18, height: 18)
    }
}

private enum ApplicationIconAsset {
    static func image(for bundleIdentifier: String, size: CGFloat = 18) -> NSImage? {
        guard let appURL = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) else {
            return nil
        }
        let image = NSWorkspace.shared.icon(forFile: appURL.path)
        image.size = NSSize(width: size, height: size)
        return image
    }

    static func isInstalled(_ bundleIdentifier: String) -> Bool {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) != nil
    }
}

private struct PrimaryOpenIconView: View {
    let prefersVSCode: Bool

    var body: some View {
        Group {
            if prefersVSCode, let image = ApplicationIconAsset.image(for: "com.microsoft.VSCode", size: 16) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
            } else if let image = ApplicationIconAsset.image(for: "com.apple.finder", size: 16) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
            } else {
                Image(systemName: "folder")
                    .font(.system(size: 12, weight: .semibold))
            }
        }
    }
}

private struct ProjectTitleView: View {
    let project: Project?

    var body: some View {
        Text(project?.name ?? "Codux")
            .font(.system(size: 13, weight: .semibold, design: .rounded))
            .foregroundStyle(AppTheme.textPrimary)
            .lineLimit(1)
            .frame(maxWidth: 260)
            .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
    }
}
