import AppKit
import SwiftUI

struct WorkspaceView: View {
    let model: AppModel

    var body: some View {
        Group {
            if let workspace = model.selectedWorkspace {
                WorkspaceProjectView(model: model, workspace: workspace)
            } else {
                WorkspaceEmptyStateView(model: model)
            }
        }
    }
}

private struct WorkspaceProjectView: View {
    let model: AppModel
    let workspace: ProjectWorkspace

    var body: some View {
        VerticalTerminalSplitView(
            model: model,
            workspace: workspace,
            dividerColor: model.terminalDividerNSColor,
            hasBottomRegion: workspace.hasBottomTabs,
            bottomHeight: workspace.hasBottomTabs ? workspace.bottomPaneHeight : 160,
            top: {
                TopPaneRowView(model: model, workspace: workspace)
                    .frame(minHeight: 220, maxHeight: .infinity)
            },
            bottom: {
                AnyView(
                    Group {
                        if workspace.hasBottomTabs {
                            WorkspaceBottomRegion(model: model, workspace: workspace)
                                .frame(minHeight: 160, idealHeight: workspace.bottomPaneHeight, maxHeight: .infinity)
                        } else {
                            EmptyView()
                        }
                    }
                )
            }
        )
        .background(model.terminalChromeColor)
    }
}

private struct WorkspaceBottomRegion: View {
    let model: AppModel
    let workspace: ProjectWorkspace

    var body: some View {
        BottomTabbedPaneView(model: model, workspace: workspace)
        .clipped()
    }
}

private struct TopPaneRowView: View {
    let model: AppModel
    let workspace: ProjectWorkspace

    var body: some View {
        TopPaneSplitContainer(model: model, workspace: workspace)
    }
}

private struct TopPaneSplitContainer: NSViewControllerRepresentable {
    let model: AppModel
    let workspace: ProjectWorkspace

    func makeNSViewController(context: Context) -> TopPaneSplitController {
        TopPaneSplitController(model: model, workspace: workspace, dividerColor: model.terminalDividerNSColor)
    }

    func updateNSViewController(_ controller: TopPaneSplitController, context: Context) {
        controller.update(workspace: workspace, dividerColor: model.terminalDividerNSColor)
    }
}

private final class TopPaneSplitController: NSViewController, NSSplitViewDelegate {
    private let model: AppModel
    private let paneSplitView = DividerStyledHorizontalSplitView()
    private var paneHosts: [UUID: NSHostingController<TerminalPaneView>] = [:]
    private var currentSessionIDs: [UUID] = []
    private var currentWorkspace: ProjectWorkspace
    private var dividerColor: NSColor
    private let minimumPaneWidth: CGFloat = 220
    private var isApplyingLayout = false
    private var needsEqualDistribution = false

    init(model: AppModel, workspace: ProjectWorkspace, dividerColor: NSColor) {
        self.model = model
        self.currentWorkspace = workspace
        self.dividerColor = dividerColor
        super.init(nibName: nil, bundle: nil)
        paneSplitView.dividerStyle = NSSplitView.DividerStyle.thin
        paneSplitView.isVertical = true
        paneSplitView.delegate = self
        paneSplitView.customDividerColor = dividerColor
        rebuildPanes(for: workspace)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = NSView()
        paneSplitView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(paneSplitView)

        NSLayoutConstraint.activate([
            paneSplitView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            paneSplitView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            paneSplitView.topAnchor.constraint(equalTo: view.topAnchor),
            paneSplitView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        applyRatiosIfNeeded()
    }

    func update(workspace: ProjectWorkspace, dividerColor: NSColor) {
        currentWorkspace = workspace
        self.dividerColor = dividerColor
        paneSplitView.customDividerColor = dividerColor
        rebuildPanes(for: workspace)
        updatePaneViews(for: workspace)
        applyRatiosIfNeeded()
        DispatchQueue.main.async { [weak self] in
            self?.applyRatiosIfNeeded()
        }
    }

    private func rebuildPanes(for workspace: ProjectWorkspace) {
        guard currentSessionIDs != workspace.topSessionIDs else { return }
        currentSessionIDs = workspace.topSessionIDs
        needsEqualDistribution = true

        for view in paneSplitView.arrangedSubviews {
            view.removeFromSuperview()
        }

        for sessionID in workspace.topSessionIDs {
            guard let session = workspace.sessions.first(where: { $0.id == sessionID }) else { continue }

            let host: NSHostingController<TerminalPaneView>
            if let existing = paneHosts[sessionID] {
                host = existing
            } else {
                host = NSHostingController(
                    rootView: TerminalPaneView(
                        model: model,
                        session: session,
                        terminalBackgroundPreset: model.terminalBackgroundPreset,
                        isFocused: sessionID == workspace.selectedSessionID,
                        onSelect: { self.model.selectSession(sessionID) },
                        onClose: { self.model.closeSession(sessionID) },
                        showsCloseButton: true
                    )
                )
                paneHosts[sessionID] = host
            }

            if host.parent == nil {
                addChild(host)
            }

            paneSplitView.addArrangedSubview(host.view)
            host.view.translatesAutoresizingMaskIntoConstraints = true
        }

        let validIDs = Set(workspace.topSessionIDs)
        paneHosts = paneHosts.filter { validIDs.contains($0.key) }

        DispatchQueue.main.async { [weak self] in
            self?.applyRatiosIfNeeded()
        }
    }

    private func updatePaneViews(for workspace: ProjectWorkspace) {
        for sessionID in workspace.topSessionIDs {
            guard let session = workspace.sessions.first(where: { $0.id == sessionID }), let host = paneHosts[sessionID] else { continue }
            host.rootView = TerminalPaneView(
                model: model,
                session: session,
                terminalBackgroundPreset: model.terminalBackgroundPreset,
                isFocused: sessionID == workspace.selectedSessionID,
                onSelect: { self.model.selectSession(sessionID) },
                onClose: { self.model.closeSession(sessionID) },
                showsCloseButton: true
            )
        }
    }

    private func applyRatiosIfNeeded() {
        guard !isApplyingLayout, paneSplitView.bounds.width > 0, currentSessionIDs.count > 1 else { return }
        let ratios: [CGFloat]
        if needsEqualDistribution {
            let equal = 1 / CGFloat(currentSessionIDs.count)
            ratios = Array(repeating: equal, count: currentSessionIDs.count)
        } else {
            ratios = currentWorkspace.resolvedTopPaneRatios()
        }
        guard ratios.count == currentSessionIDs.count else { return }

        isApplyingLayout = true
        defer { isApplyingLayout = false }

        let dividerCount = max(0, currentSessionIDs.count - 1)
        let availableWidth = max(0, paneSplitView.bounds.width - CGFloat(dividerCount))
        var runningX: CGFloat = 0

        for index in 0 ..< dividerCount {
            runningX += availableWidth * ratios[index]
            paneSplitView.setPosition(runningX, ofDividerAt: index)
            runningX += paneSplitView.dividerThickness
        }

        paneSplitView.adjustSubviews()
        paneSplitView.needsDisplay = true
        if needsEqualDistribution {
            model.updateTopPaneRatios(ratios)
            needsEqualDistribution = false
        }
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        guard !isApplyingLayout, currentSessionIDs.count > 1, paneSplitView.isDraggingDivider else { return }
        let widths = paneSplitView.subviews.prefix(currentSessionIDs.count).map { $0.frame.width }
        let total = widths.reduce(0, +)
        guard total > 0 else { return }
        model.updateTopPaneRatios(widths.map { $0 / total })
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        let previousEdge = dividerIndex == 0 ? 0 : paneSplitView.subviews[dividerIndex - 1].frame.maxX + paneSplitView.dividerThickness
        return previousEdge + minimumPaneWidth
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        let nextFrame = paneSplitView.subviews[dividerIndex + 1].frame
        return nextFrame.maxX - minimumPaneWidth - paneSplitView.dividerThickness
    }
}

private struct BottomTabbedPaneView: View {
    let model: AppModel
    let workspace: ProjectWorkspace

    private var selectedBottomSessionID: UUID? {
        if let selected = workspace.selectedBottomTabSessionID,
           workspace.bottomTabSessionIDs.contains(selected) {
            return selected
        }
        return workspace.bottomTabSessionIDs.first
    }

    var body: some View {
        VStack(spacing: 0) {
            HStack(spacing: 8) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(Array(workspace.bottomTabSessionIDs.enumerated()), id: \.element) { index, sessionID in
                            let isSelected = sessionID == selectedBottomSessionID

                            TabChip(
                                model: model,
                                title: String(format: model.i18n("workspace.tab_format", fallback: "Tab %@"), "\(index + 1)"),
                                isSelected: isSelected,
                                onSelect: { model.selectBottomTabSession(sessionID) },
                                onClose: { model.closeSession(sessionID) }
                            )
                        }

                        Spacer(minLength: 0)
                    }
                    .frame(minWidth: 0, maxWidth: .infinity, alignment: .leading)
                }
                .frame(maxWidth: .infinity)

                bottomTabAddButton
            }
            .padding(.horizontal, 10)
            .padding(.top, 10)
            .padding(.bottom, 8)
            .background(
                Rectangle()
                    .fill(model.terminalChromeColor)
            )

            ZStack {
                ForEach(workspace.bottomTabSessionIDs, id: \.self) { sessionID in
                    if let session = workspace.sessions.first(where: { $0.id == sessionID }) {
                        TerminalPaneView(
                            model: model,
                            session: session,
                            terminalBackgroundPreset: model.terminalBackgroundPreset,
                            isFocused: sessionID == selectedBottomSessionID,
                            onSelect: { model.selectBottomTabSession(sessionID) },
                            onClose: {},
                            showsCloseButton: false
                        )
                        .opacity(sessionID == selectedBottomSessionID ? 1 : 0)
                        .allowsHitTesting(sessionID == selectedBottomSessionID)
                        .zIndex(sessionID == selectedBottomSessionID ? 1 : 0)
                    }
                }
            }
            .background(
                Rectangle()
                    .fill(model.terminalChromeColor)
            )
        }
        .background(
            Rectangle()
                .fill(model.terminalChromeColor)
        )
    }

    private var bottomTabAddButton: some View {
        Button(action: model.createBottomTab) {
            Image(systemName: "plus")
                .font(.system(size: 11, weight: .bold))
                .foregroundStyle(model.terminalMutedTextColor)
                .frame(width: 22, height: 22)
                .background(model.terminalBackgroundPreset.isLight ? Color.black.opacity(0.06) : Color.white.opacity(0.08))
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        }
        .buttonStyle(.plain)
    }
}

private struct VerticalTerminalSplitView<Top: View, Bottom: View>: NSViewControllerRepresentable {
    let model: AppModel
    let workspace: ProjectWorkspace
    let dividerColor: NSColor
    let hasBottomRegion: Bool
    let bottomHeight: CGFloat
    let top: () -> Top
    let bottom: () -> Bottom

    func makeNSViewController(context: Context) -> SplitViewController<Top, Bottom> {
        SplitViewController(model: model, workspace: workspace, dividerColor: dividerColor, hasBottomRegion: hasBottomRegion, bottomHeight: bottomHeight, top: top(), bottom: bottom())
    }

    func updateNSViewController(_ controller: SplitViewController<Top, Bottom>, context: Context) {
        controller.update(workspace: workspace, top: top(), bottom: bottom(), dividerColor: dividerColor, hasBottomRegion: hasBottomRegion, bottomHeight: bottomHeight)
    }
}

private final class SplitViewController<Top: View, Bottom: View>: NSViewController, NSSplitViewDelegate {
    private unowned let model: AppModel
    private let splitView = DividerStyledSplitView()
    private let topHosting = NSHostingController(rootView: AnyView(EmptyView()))
    private let bottomHosting = NSHostingController(rootView: AnyView(EmptyView()))
    private var currentWorkspace: ProjectWorkspace
    private var dividerColor: NSColor
    private var hasBottomRegion: Bool
    private var bottomHeight: CGFloat
    private var isApplyingLayout = false
    private let minimumTopHeight: CGFloat = 220
    private let minimumBottomHeight: CGFloat = 160

    init(model: AppModel, workspace: ProjectWorkspace, dividerColor: NSColor, hasBottomRegion: Bool, bottomHeight: CGFloat, top: Top, bottom: Bottom) {
        self.model = model
        self.currentWorkspace = workspace
        self.dividerColor = dividerColor
        self.hasBottomRegion = hasBottomRegion
        self.bottomHeight = bottomHeight
        super.init(nibName: nil, bundle: nil)
        splitView.dividerStyle = .thin
        splitView.isVertical = false
        splitView.delegate = self
        splitView.customDividerColor = dividerColor
        topHosting.rootView = AnyView(top)
        bottomHosting.rootView = AnyView(bottom)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func loadView() {
        view = NSView()
        splitView.translatesAutoresizingMaskIntoConstraints = false
        view.addSubview(splitView)

        NSLayoutConstraint.activate([
            splitView.leadingAnchor.constraint(equalTo: view.leadingAnchor),
            splitView.trailingAnchor.constraint(equalTo: view.trailingAnchor),
            splitView.topAnchor.constraint(equalTo: view.topAnchor),
            splitView.bottomAnchor.constraint(equalTo: view.bottomAnchor),
        ])

        let topItem = NSSplitViewItem(viewController: topHosting)
        let bottomItem = NSSplitViewItem(viewController: bottomHosting)
        addChild(topHosting)
        addChild(bottomHosting)
        splitView.addArrangedSubview(topHosting.view)
        splitView.addArrangedSubview(bottomHosting.view)
        topHosting.view.translatesAutoresizingMaskIntoConstraints = false
        bottomHosting.view.translatesAutoresizingMaskIntoConstraints = false
        topItem.minimumThickness = 220
        bottomItem.minimumThickness = 0
    }

    override func viewDidLayout() {
        super.viewDidLayout()
        applyBottomHeightIfNeeded()
    }

    func update(workspace: ProjectWorkspace, top: Top, bottom: Bottom, dividerColor: NSColor, hasBottomRegion: Bool, bottomHeight: CGFloat) {
        currentWorkspace = workspace
        self.dividerColor = dividerColor
        self.hasBottomRegion = hasBottomRegion
        topHosting.rootView = AnyView(top)
        bottomHosting.rootView = AnyView(bottom)
        splitView.customDividerColor = dividerColor
        self.bottomHeight = bottomHeight
        applyBottomHeightIfNeeded()
        splitView.needsDisplay = true
    }

    private func applyBottomHeightIfNeeded() {
        guard view.bounds.height > 0, !isApplyingLayout else { return }
        isApplyingLayout = true
        defer { isApplyingLayout = false }

        splitView.customDividerColor = hasBottomRegion ? dividerColor : .clear
        bottomHosting.view.alphaValue = hasBottomRegion ? 1 : 0

        if !hasBottomRegion {
            splitView.setPosition(splitView.bounds.height, ofDividerAt: 0)
            splitView.adjustSubviews()
            return
        }

        let totalHeight = splitView.bounds.height
        let clampedBottomHeight = min(max(bottomHeight, minimumBottomHeight), max(minimumBottomHeight, totalHeight - minimumTopHeight))
        let dividerPosition = max(minimumTopHeight, totalHeight - clampedBottomHeight)
        splitView.setPosition(dividerPosition, ofDividerAt: 0)
        splitView.adjustSubviews()
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        guard hasBottomRegion else { return splitView.bounds.height }
        return minimumTopHeight
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        guard hasBottomRegion else { return splitView.bounds.height }
        return splitView.bounds.height - minimumBottomHeight
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        guard !isApplyingLayout, hasBottomRegion else { return }
        let newBottomHeight = bottomHosting.view.frame.height
        model.updateBottomPaneHeight(newBottomHeight, for: currentWorkspace.projectID, availableHeight: splitView.bounds.height)
    }
}

private final class DividerStyledSplitView: NSSplitView {
    var customDividerColor: NSColor = .separatorColor

    override var dividerColor: NSColor {
        customDividerColor
    }

    override var dividerThickness: CGFloat {
        1
    }

    override func drawDivider(in rect: NSRect) {
        customDividerColor.setFill()
        rect.fill()
    }
}

private struct TabChip: View {
    let model: AppModel
    let title: String
    let isSelected: Bool
    let onSelect: () -> Void
    let onClose: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: onSelect) {
            HStack(spacing: 8) {
                Text(title)
                    .font(.system(size: 11, weight: .semibold, design: .rounded))
                    .foregroundStyle(isSelected ? model.terminalTextColor : model.terminalMutedTextColor)
                    .lineLimit(1)
                    .frame(maxWidth: .infinity, alignment: .leading)

                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 9, weight: .bold))
                        .foregroundStyle(model.terminalMutedTextColor)
                        .opacity(isHovered || isSelected ? 1.0 : 0.0)
                        .frame(width: 16, height: 16)
                }
                .buttonStyle(.plain)
                .appCursor(.pointingHand)
            }
            .padding(.horizontal, 12)
            .frame(minWidth: 84, minHeight: 28)
            .background(isSelected ? selectedFill : (isHovered ? hoverFill : Color.clear))
            .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
            .overlay {
                if isSelected {
                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                        .stroke(selectedStroke, lineWidth: 1)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var hoverFill: Color {
        model.terminalBackgroundPreset.isLight ? Color.black.opacity(0.04) : Color.white.opacity(0.05)
    }

    private var selectedFill: Color {
        model.terminalBackgroundPreset.isLight ? Color.black.opacity(0.07) : Color.white.opacity(0.11)
    }

    private var selectedStroke: Color {
        model.terminalBackgroundPreset.isLight ? Color.black.opacity(0.12) : Color.white.opacity(0.1)
    }
}

private struct TerminalPaneView: View {
    let model: AppModel
    let session: TerminalSession
    let terminalBackgroundPreset: AppTerminalBackgroundPreset
    let isFocused: Bool
    let onSelect: () -> Void
    let onClose: () -> Void
    let showsCloseButton: Bool

    @State private var isHovered = false

    private let terminalEnvironmentService = AIRuntimeBridgeService()
    private let terminalInsets = EdgeInsets(top: 10, leading: 10, bottom: 10, trailing: 10)

    var body: some View {
        ZStack {
            Rectangle()
                .fill(model.terminalChromeColor)

            terminalHost
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                .padding(terminalInsets)

            if showsCloseButton && (isHovered || isFocused) {
                VStack {
                    HStack {
                        Spacer()

                        Button(action: onClose) {
                            Image(systemName: "xmark")
                                .font(.system(size: 10, weight: .bold))
                                .foregroundStyle(model.terminalMutedTextColor)
                                .frame(width: 20, height: 20)
                                .background(model.terminalChromeColor.opacity(0.96))
                                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                        }
                        .buttonStyle(.plain)
                        .appCursor(.pointingHand)
                    }

                    Spacer()
                }
                .padding(10)
            }
        }
        .contentShape(Rectangle())
        .simultaneousGesture(TapGesture().onEnded {
            onSelect()
        })
        .onHover { hovering in
            isHovered = hovering
        }
    }

    @ViewBuilder
    private var terminalHost: some View {
        SwiftTermTerminalHostView(
            session: session,
            environment: terminalEnvironmentService.environment(for: session),
            terminalBackgroundPreset: terminalBackgroundPreset,
            shouldFocus: model.terminalFocusRequestID == session.id,
            onInteraction: onSelect,
            onFocusConsumed: { model.consumeTerminalFocusRequest(session.id) }
        )
    }
}
