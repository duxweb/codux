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
        .onAppear {
            model.noteWorkspaceViewAppeared()
        }
        .onChange(of: model.selectedProjectID) { _, _ in
            model.noteWorkspaceViewAppeared()
        }
    }
}

private struct WorkspaceProjectView: View {
    let model: AppModel
    let workspace: ProjectWorkspace

    private var activeTerminalSessionID: UUID? {
        let _ = model.terminalFocusRenderVersion
        return model.displayedFocusedTerminalSessionID
    }

    private var hasMultipleVisibleTerminalPanes: Bool {
        workspace.topSessionIDs.count + (workspace.hasBottomTabs ? 1 : 0) > 1
    }

    var body: some View {
        VerticalTerminalSplitView(
            model: model,
            workspace: workspace,
            dividerColor: model.terminalDividerNSColor,
            hasBottomRegion: workspace.hasBottomTabs,
            bottomHeight: workspace.hasBottomTabs ? workspace.bottomPaneHeight : 160,
            top: {
                TopPaneRowView(
                    model: model,
                    workspace: workspace,
                    activeTerminalSessionID: activeTerminalSessionID,
                    showsInactiveOverlay: hasMultipleVisibleTerminalPanes
                )
                    .frame(minHeight: 220, maxHeight: .infinity)
            },
            bottom: {
                AnyView(
                    Group {
                        if workspace.hasBottomTabs {
                            WorkspaceBottomRegion(
                                model: model,
                                workspace: workspace,
                                activeTerminalSessionID: activeTerminalSessionID,
                                showsInactiveOverlay: hasMultipleVisibleTerminalPanes
                            )
                                .frame(minHeight: 160, idealHeight: workspace.bottomPaneHeight, maxHeight: .infinity)
                        } else {
                            EmptyView()
                        }
                    }
                )
            }
        )
        .background(model.terminalChromeColor)
        .transaction { transaction in
            transaction.disablesAnimations = true
        }
    }
}

private struct WorkspaceBottomRegion: View {
    let model: AppModel
    let workspace: ProjectWorkspace
    let activeTerminalSessionID: UUID?
    let showsInactiveOverlay: Bool

    var body: some View {
        BottomTabbedPaneView(
            model: model,
            workspace: workspace,
            activeTerminalSessionID: activeTerminalSessionID,
            showsInactiveOverlay: showsInactiveOverlay
        )
        .clipped()
    }
}

private struct TopPaneRowView: View {
    let model: AppModel
    let workspace: ProjectWorkspace
    let activeTerminalSessionID: UUID?
    let showsInactiveOverlay: Bool

    var body: some View {
        TopPaneSplitContainer(
            model: model,
            workspace: workspace,
            activeTerminalSessionID: activeTerminalSessionID,
            showsInactiveOverlay: showsInactiveOverlay
        )
    }
}

private struct TopPaneSplitContainer: NSViewControllerRepresentable {
    let model: AppModel
    let workspace: ProjectWorkspace
    let activeTerminalSessionID: UUID?
    let showsInactiveOverlay: Bool

    func makeNSViewController(context: Context) -> TopPaneSplitController {
        TopPaneSplitController(
            model: model,
            workspace: workspace,
            activeTerminalSessionID: activeTerminalSessionID,
            showsInactiveOverlay: showsInactiveOverlay,
            dividerColor: model.terminalDividerNSColor
        )
    }

    func updateNSViewController(_ controller: TopPaneSplitController, context: Context) {
        controller.update(
            workspace: workspace,
            activeTerminalSessionID: activeTerminalSessionID,
            showsInactiveOverlay: showsInactiveOverlay,
            dividerColor: model.terminalDividerNSColor
        )
    }
}

private final class TopPaneSplitController: NSViewController, NSSplitViewDelegate {
    private let model: AppModel
    private let logger = AppDebugLog.shared
    private let paneSplitView = DividerStyledHorizontalSplitView()
    private var paneHosts: [UUID: NSHostingController<TerminalPaneView>] = [:]
    private var lastRenderedSessionByID: [UUID: TerminalSession] = [:]
    private var currentSessionIDs: [UUID] = []
    private var currentWorkspace: ProjectWorkspace
    private var activeTerminalSessionID: UUID?
    private var lastRenderedFocusedSessionID: UUID?
    private var showsInactiveOverlay: Bool
    private var lastRenderedShowsInactiveOverlay: Bool
    private var dividerColor: NSColor
    private let minimumPaneWidth: CGFloat = 220
    private var isApplyingLayout = false
    private var needsEqualDistribution = false

    init(model: AppModel, workspace: ProjectWorkspace, activeTerminalSessionID: UUID?, showsInactiveOverlay: Bool, dividerColor: NSColor) {
        self.model = model
        self.currentWorkspace = workspace
        self.activeTerminalSessionID = activeTerminalSessionID
        self.showsInactiveOverlay = showsInactiveOverlay
        self.lastRenderedShowsInactiveOverlay = showsInactiveOverlay
        self.dividerColor = dividerColor
        super.init(nibName: nil, bundle: nil)
        logger.log(
            "startup-ui",
            "top-pane init project=\(workspace.projectID.uuidString) topSessions=\(workspace.topSessionIDs.count) bottomTabs=\(workspace.bottomTabSessionIDs.count)"
        )
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

    func update(workspace: ProjectWorkspace, activeTerminalSessionID: UUID?, showsInactiveOverlay: Bool, dividerColor: NSColor) {
        let sessionIDsChanged = workspace.topSessionIDs != currentWorkspace.topSessionIDs
        let previousSessionIDs = currentWorkspace.topSessionIDs
        currentWorkspace = workspace
        self.activeTerminalSessionID = activeTerminalSessionID
        self.showsInactiveOverlay = showsInactiveOverlay
        self.dividerColor = dividerColor
        paneSplitView.customDividerColor = dividerColor
        if sessionIDsChanged {
            let survivingSessionIDs = workspace.topSessionIDs.filter { previousSessionIDs.contains($0) }
            DmuxTerminalBackend.shared.registry.beginStructuralResizeTransition(for: survivingSessionIDs)
        }
        if sessionIDsChanged {
            rebuildPanes(for: workspace)
        }
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
        logger.log(
            "startup-ui",
            "top-pane rebuild project=\(workspace.projectID.uuidString) sessions=\(workspace.topSessionIDs.count)"
        )

        let activeSessionIDs = Set(workspace.topSessionIDs)
        let staleSessionIDs = Set(paneHosts.keys).subtracting(activeSessionIDs)
        for sessionID in staleSessionIDs {
            if let host = paneHosts.removeValue(forKey: sessionID) {
                detachPaneHostView(host.view)
                host.removeFromParent()
            }
            lastRenderedSessionByID.removeValue(forKey: sessionID)
        }

        let sessionsByID = Dictionary(uniqueKeysWithValues: workspace.sessions.map { ($0.id, $0) })

        for (index, sessionID) in workspace.topSessionIDs.enumerated() {
            guard let session = sessionsByID[sessionID] else { continue }
            logger.log(
                "startup-ui",
                "top-pane prepare-host session=\(session.id.uuidString) project=\(session.projectID.uuidString)"
            )

            let host: NSHostingController<TerminalPaneView>
            if let existing = paneHosts[sessionID] {
                host = existing
                host.rootView = makePaneView(session: session, sessionID: sessionID)
                lastRenderedSessionByID[sessionID] = session
            } else {
                host = NSHostingController(
                    rootView: makePaneView(session: session, sessionID: sessionID)
                )
                paneHosts[sessionID] = host
                lastRenderedSessionByID[sessionID] = session
                logger.log("startup-ui", "top-pane create-host session=\(session.id.uuidString)")
            }

            if host.parent == nil {
                addChild(host)
                logger.log("startup-ui", "top-pane add-child session=\(session.id.uuidString)")
            }

            logger.log("startup-ui", "top-pane access-host-view session=\(session.id.uuidString)")
            if paneSplitView.arrangedSubviews.contains(host.view) == false {
                paneSplitView.insertArrangedSubview(host.view, at: min(index, paneSplitView.arrangedSubviews.count))
                logger.log("startup-ui", "top-pane attached-host-view session=\(session.id.uuidString)")
            } else if index >= paneSplitView.arrangedSubviews.count || paneSplitView.arrangedSubviews[index] !== host.view {
                detachPaneHostView(host.view)
                paneSplitView.insertArrangedSubview(host.view, at: index)
                logger.log("startup-ui", "top-pane reordered-host-view session=\(session.id.uuidString) index=\(index)")
            }
            host.view.translatesAutoresizingMaskIntoConstraints = true
            host.view.autoresizingMask = [.width, .height]
            host.view.needsLayout = true
        }

        lastRenderedFocusedSessionID = activeTerminalSessionID

        paneSplitView.needsLayout = true
        paneSplitView.layoutSubtreeIfNeeded()
        for sessionID in workspace.topSessionIDs {
            paneHosts[sessionID]?.view.layoutSubtreeIfNeeded()
        }

        DispatchQueue.main.async { [weak self] in
            self?.applyRatiosIfNeeded()
        }
    }

    private func detachPaneHostView(_ view: NSView) {
        if paneSplitView.arrangedSubviews.contains(view) {
            paneSplitView.removeArrangedSubview(view)
        }
        view.removeFromSuperview()
    }

    private func updatePaneViews(for workspace: ProjectWorkspace) {
        let sessionsByID = Dictionary(uniqueKeysWithValues: workspace.sessions.map { ($0.id, $0) })
        var sessionsToUpdate = Set<UUID>()

        if lastRenderedShowsInactiveOverlay != showsInactiveOverlay {
            sessionsToUpdate.formUnion(workspace.topSessionIDs)
        } else {
            sessionsToUpdate.formUnion([lastRenderedFocusedSessionID, activeTerminalSessionID].compactMap { $0 })
        }

        for sessionID in workspace.topSessionIDs {
            guard let session = sessionsByID[sessionID] else { continue }
            if lastRenderedSessionByID[sessionID] != session {
                sessionsToUpdate.insert(sessionID)
            }
        }

        for sessionID in sessionsToUpdate {
            guard let session = sessionsByID[sessionID], let host = paneHosts[sessionID] else { continue }
            host.rootView = makePaneView(session: session, sessionID: sessionID)
            lastRenderedSessionByID[sessionID] = session
        }

        lastRenderedFocusedSessionID = activeTerminalSessionID
        lastRenderedShowsInactiveOverlay = showsInactiveOverlay
    }

    private func makePaneView(session: TerminalSession, sessionID: UUID) -> TerminalPaneView {
        TerminalPaneView(
            model: model,
            session: session,
            terminalBackgroundPreset: model.terminalBackgroundPreset,
            backgroundColorPreset: model.backgroundColorPreset,
            isFocused: sessionID == activeTerminalSessionID,
            isVisible: true,
            showsInactiveOverlay: showsInactiveOverlay,
            onSelect: { self.model.selectSession(sessionID) },
            onClose: { self.model.closeSession(sessionID) },
            onDetach: { self.model.detachSession(sessionID) },
            showsCloseButton: true
        )
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
        paneSplitView.needsLayout = true
        paneSplitView.layoutSubtreeIfNeeded()
        for sessionID in currentSessionIDs {
            paneHosts[sessionID]?.view.needsLayout = true
            paneHosts[sessionID]?.view.layoutSubtreeIfNeeded()
            paneHosts[sessionID]?.view.needsDisplay = true
        }
        DmuxTerminalBackend.shared.registry.reconcileGeometry(for: currentSessionIDs)
        paneSplitView.needsDisplay = true
        if needsEqualDistribution {
            model.updateTopPaneRatios(ratios)
            needsEqualDistribution = false
        }
    }

    func splitViewWillResizeSubviews(_ notification: Notification) {
        guard currentSessionIDs.count > 1,
              model.selectedProjectID == currentWorkspace.projectID else { return }
        DmuxTerminalBackend.shared.registry.beginStructuralResizeTransition(for: currentSessionIDs)
    }

    func splitViewDidResizeSubviews(_ notification: Notification) {
        guard !isApplyingLayout,
              currentSessionIDs.count > 1,
              model.selectedProjectID == currentWorkspace.projectID else { return }
        let widths = paneSplitView.subviews.prefix(currentSessionIDs.count).map { $0.frame.width }
        let total = widths.reduce(0, +)
        guard total > 0 else { return }
        model.updateTopPaneRatios(widths.map { $0 / total })
        DmuxTerminalBackend.shared.registry.reconcileGeometry(for: currentSessionIDs)
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
    let activeTerminalSessionID: UUID?
    let showsInactiveOverlay: Bool

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
                                title: String(format: String(localized: "workspace.tab_format", defaultValue: "Tab %@", bundle: .module), "\(index + 1)"),
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
                if let selectedBottomSessionID,
                   let session = workspace.sessions.first(where: { $0.id == selectedBottomSessionID }) {
                    TerminalPaneView(
                        model: model,
                        session: session,
                        terminalBackgroundPreset: model.terminalBackgroundPreset,
                        backgroundColorPreset: model.backgroundColorPreset,
                        isFocused: selectedBottomSessionID == activeTerminalSessionID,
                        isVisible: true,
                        showsInactiveOverlay: showsInactiveOverlay,
                        onSelect: { model.selectBottomTabSession(selectedBottomSessionID) },
                        onClose: { model.closeSession(selectedBottomSessionID) },
                        onDetach: { model.detachSession(selectedBottomSessionID) },
                        showsCloseButton: true
                    )
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
                .background(model.terminalUsesLightBackground ? Color.black.opacity(0.06) : Color.white.opacity(0.08))
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
    private var hasCommittedInitialBottomLayout = false
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
        if currentWorkspace.projectID != workspace.projectID {
            hasCommittedInitialBottomLayout = false
        }
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
            hasCommittedInitialBottomLayout = false
            splitView.setPosition(splitView.bounds.height, ofDividerAt: 0)
            splitView.adjustSubviews()
            return
        }

        let totalHeight = splitView.bounds.height
        let clampedBottomHeight = min(max(bottomHeight, minimumBottomHeight), max(minimumBottomHeight, totalHeight - minimumTopHeight))
        let dividerPosition = max(minimumTopHeight, totalHeight - clampedBottomHeight)
        splitView.setPosition(dividerPosition, ofDividerAt: 0)
        splitView.adjustSubviews()
        hasCommittedInitialBottomLayout = true
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
        guard !isApplyingLayout,
              hasBottomRegion,
              hasCommittedInitialBottomLayout,
              model.selectedProjectID == currentWorkspace.projectID,
              splitView.bounds.height > 0 else { return }
        let newBottomHeight = bottomHosting.view.frame.height
        guard newBottomHeight > 0 else { return }
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
        model.terminalUsesLightBackground ? Color.black.opacity(0.04) : Color.white.opacity(0.05)
    }

    private var selectedFill: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.07) : Color.white.opacity(0.11)
    }

    private var selectedStroke: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.12) : Color.white.opacity(0.1)
    }
}

struct TerminalPaneView: View {
    let model: AppModel
    let session: TerminalSession
    let terminalBackgroundPreset: AppTerminalBackgroundPreset
    let backgroundColorPreset: AppBackgroundColorPreset
    let isFocused: Bool
    let isVisible: Bool
    let showsInactiveOverlay: Bool
    let onSelect: () -> Void
    let onClose: () -> Void
    let onDetach: (() -> Void)?
    let showsCloseButton: Bool

    @State private var isHovered = false
    @State private var hasLoggedMount = false

    private let terminalEnvironmentService = AIRuntimeBridgeService()
    private var recoveryIssue: AppModel.TerminalRecoveryIssue? {
        model.terminalRecoveryIssue(for: session.id)
    }

    private var paneBackgroundColor: Color {
        return model.terminalChromeColor
    }

    private var hasPaneControls: Bool {
        showsCloseButton || onDetach != nil
    }

    private var terminalInsets: EdgeInsets {
        let base = CGFloat(10)
        guard hasPaneControls else {
            return EdgeInsets(top: base, leading: base, bottom: base, trailing: base)
        }
        return EdgeInsets(top: base, leading: base, bottom: base, trailing: 56)
    }

    var body: some View {
        ZStack {
            Rectangle()
                .fill(paneBackgroundColor)

            if showsInactiveOverlay && !isFocused {
                Rectangle()
                    .fill(Color(nsColor: model.terminalInactiveDimColor))
            }

            Group {
                if let recoveryIssue {
                    TerminalRecoveryFallbackView(
                        model: model,
                        session: session,
                        issue: recoveryIssue,
                        onRetry: { model.retryTerminalRecovery(session.id) }
                    )
                } else {
                    terminalHost
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(terminalInsets)

            if showsCloseButton || onDetach != nil {
                VStack {
                    HStack {
                        Spacer()

                        HStack(spacing: 6) {
                            if let onDetach {
                                Button(action: onDetach) {
                                    Image(systemName: "square.on.square")
                                        .font(.system(size: 10, weight: .bold))
                                        .foregroundStyle(model.terminalMutedTextColor)
                                        .frame(width: 20, height: 20)
                                        .background(model.terminalChromeColor.opacity(0.96))
                                        .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                                }
                                .buttonStyle(.plain)
                                .appCursor(.pointingHand)
                            }

                            if showsCloseButton {
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
                        }
                    }

                    Spacer()
                }
                .padding(10)
                .opacity(isHovered || isFocused ? 1 : 0.82)
            }
        }
        .contentShape(Rectangle())
        .onHover { hovering in
            isHovered = hovering
        }
        .onAppear {
            guard hasLoggedMount == false else {
                return
            }
            hasLoggedMount = true
            AppDebugLog.shared.log(
                "startup-ui",
                "terminal-pane appear session=\(session.id.uuidString) project=\(session.projectID.uuidString)"
            )
        }
    }

    @ViewBuilder
    private var terminalHost: some View {
        GhosttyTerminalHostView(
            session: session,
            environment: terminalEnvironment(),
            terminalBackgroundPreset: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset,
            terminalFontSize: model.appSettings.terminalFontSize,
            isFocused: isFocused,
            isVisible: isVisible,
            showsInactiveOverlay: showsInactiveOverlay,
            shouldFocus: model.terminalFocusRequestID == session.id,
            onInteraction: onSelect,
            onFocusConsumed: { model.consumeTerminalFocusRequest(session.id) },
            onStartupSucceeded: { model.noteTerminalStartupSucceeded(session.id) },
            onStartupFailure: { detail in model.noteTerminalStartupFailure(session.id, detail: detail) }
        )
        .id("terminal-\(session.id.uuidString)-\(model.terminalRecoveryRetryToken(for: session.id))-ghostty")
    }

    private func terminalEnvironment() -> [(String, String)] {
        let logger = AppDebugLog.shared
        let resolution = terminalEnvironmentService.environmentResolution(for: session)
        if resolution.isCacheHit == false {
            logger.log(
                "startup-ui",
                "terminal-pane host-build-start session=\(session.id.uuidString) project=\(session.projectID.uuidString)"
            )
            logger.log(
                "startup-ui",
                "terminal-pane host-build-env-ready session=\(session.id.uuidString) envCount=\(resolution.pairs.count)"
            )
        }
        return resolution.pairs
    }
}

@MainActor
enum DetachedTerminalWindowPresenter {
    private static var controllers: [UUID: NSWindowController] = [:]
    private static var delegates: [UUID: DetachedTerminalWindowDelegate] = [:]

    static func show(model: AppModel, sessionID: UUID) {
        guard model.isDetachedTerminal(sessionID),
              let session = model.terminalSession(for: sessionID) else {
            return
        }

        if let controller = controllers[sessionID],
           let window = controller.window,
           let hosting = controller.contentViewController as? NSHostingController<AnyView> {
            hosting.rootView = AnyView(DetachedTerminalWindowView(model: model, session: session))
            window.title = detachedWindowTitle(for: session)
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 980, height: 700),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.detachedTerminal(sessionID)
        applyStandardWindowChrome(window, title: detachedWindowTitle(for: session))
        window.minSize = NSSize(width: 540, height: 320)
        window.isReleasedWhenClosed = false

        let hosting = NSHostingController(
            rootView: AnyView(
                DetachedTerminalWindowView(model: model, session: session)
            )
        )
        window.contentViewController = hosting

        let delegate = DetachedTerminalWindowDelegate(model: model, sessionID: sessionID)
        window.delegate = delegate

        let controller = NSWindowController(window: window)
        delegates[sessionID] = delegate
        controllers[sessionID] = controller

        if let parentWindow = NSApp.keyWindow ?? NSApp.mainWindow {
            let origin = NSPoint(x: parentWindow.frame.minX + 36, y: parentWindow.frame.minY - 36)
            window.setFrameOrigin(origin)
        } else {
            window.center()
        }

        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
        model.requestTerminalFocus(sessionID)
    }

    static func dismiss(sessionID: UUID, restoreOnClose: Bool) {
        guard let controller = controllers[sessionID],
              let delegate = delegates[sessionID] else {
            return
        }

        delegate.shouldRestoreOnClose = restoreOnClose
        controller.window?.close()
        controllers[sessionID] = nil
        delegates[sessionID] = nil
    }

    static func clear(sessionID: UUID) {
        controllers[sessionID] = nil
        delegates[sessionID] = nil
    }

    private static func detachedWindowTitle(for session: TerminalSession) -> String {
        if session.title.isEmpty {
            return session.projectName
        }
        return "\(session.projectName) - \(session.title)"
    }
}

@MainActor
private final class DetachedTerminalWindowDelegate: NSObject, NSWindowDelegate {
    weak var model: AppModel?
    let sessionID: UUID
    var shouldRestoreOnClose = true
    private var hasScheduledRestore = false

    init(model: AppModel, sessionID: UUID) {
        self.model = model
        self.sessionID = sessionID
    }

    func windowDidBecomeKey(_ notification: Notification) {
        model?.requestTerminalFocus(sessionID)
    }

    func windowWillClose(_ notification: Notification) {
        guard hasScheduledRestore == false else {
            return
        }
        hasScheduledRestore = true
        let model = self.model
        let sessionID = self.sessionID
        let shouldRestoreOnClose = self.shouldRestoreOnClose
        DetachedTerminalWindowPresenter.clear(sessionID: sessionID)
        guard shouldRestoreOnClose else {
            return
        }
        DispatchQueue.main.async {
            model?.restoreDetachedSession(sessionID)
        }
    }
}

private struct DetachedTerminalWindowView: View {
    let model: AppModel
    let session: TerminalSession

    var body: some View {
        TerminalPaneView(
            model: model,
            session: session,
            terminalBackgroundPreset: model.terminalBackgroundPreset,
            backgroundColorPreset: model.backgroundColorPreset,
            isFocused: true,
            isVisible: true,
            showsInactiveOverlay: false,
            onSelect: { model.requestTerminalFocus(session.id) },
            onClose: {},
            onDetach: nil,
            showsCloseButton: false
        )
        .background(model.terminalChromeColor)
        .onAppear {
            model.requestTerminalFocus(session.id)
        }
    }
}

private struct TerminalRecoveryFallbackView: View {
    let model: AppModel
    let session: TerminalSession
    let issue: AppModel.TerminalRecoveryIssue
    let onRetry: () -> Void

    private var cardFill: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.035) : Color.white.opacity(0.05)
    }

    private var cardStroke: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.1) : Color.white.opacity(0.1)
    }

    private var accent: Color {
        model.terminalUsesLightBackground ? AppTheme.warning.opacity(0.88) : AppTheme.warning.opacity(0.92)
    }

    var body: some View {
        VStack {
            Spacer()

            VStack(spacing: 14) {
                Image(systemName: "exclamationmark.triangle")
                    .font(.system(size: 22, weight: .semibold))
                    .foregroundStyle(accent)

                VStack(spacing: 6) {
                    Text(issue.message)
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(model.terminalTextColor.opacity(0.92))
                        .multilineTextAlignment(.center)

                    Text(session.cwd)
                        .font(.system(size: 11, weight: .medium, design: .monospaced))
                        .foregroundStyle(model.terminalMutedTextColor.opacity(0.78))
                        .lineLimit(2)
                        .multilineTextAlignment(.center)

                    Text(issue.detail)
                        .font(.system(size: 12))
                        .foregroundStyle(model.terminalMutedTextColor.opacity(0.82))
                        .multilineTextAlignment(.center)
                }

                HStack(spacing: 10) {
                    Button(action: onRetry) {
                        Label(String(localized: "common.retry", defaultValue: "Retry", bundle: .module), systemImage: "arrow.clockwise")
                            .font(.system(size: 13, weight: .semibold))
                            .padding(.horizontal, 14)
                            .padding(.vertical, 6)
                    }
                    .buttonStyle(.borderedProminent)

                    Button(action: { model.openSelectedProjectInTerminal() }) {
                        Label(String(localized: "open.terminal", defaultValue: "Open in Terminal", bundle: .module), systemImage: "terminal")
                            .font(.system(size: 13, weight: .medium))
                            .padding(.horizontal, 14)
                            .padding(.vertical, 6)
                    }
                    .buttonStyle(.bordered)
                }
            }
            .padding(.horizontal, 22)
            .padding(.vertical, 24)
            .background(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .fill(cardFill)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 16, style: .continuous)
                    .stroke(cardStroke, lineWidth: 1)
            )

            Spacer()
        }
        .padding(20)
    }
}
