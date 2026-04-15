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

                TerminalShellView(model: model)
                .frame(minWidth: 700, maxWidth: .infinity, maxHeight: .infinity)
            }
            .padding(.top, titlebarHeight)

            TitlebarOverlayView(model: model)
                .frame(height: titlebarHeight)
        }
        .background(Color.clear)
        .ignoresSafeArea(.container, edges: .top)
    }
}

private struct TitlebarOverlayView: View {
    let model: AppModel
    @State private var isShowingLevelPopover = false

    private let rowHeight: CGFloat = 24

    var body: some View {
        let _ = model.aiStatsStore.renderVersion

        ZStack {
            TitlebarZoomSurface()

            HStack(spacing: 0) {
                HStack(spacing: 10) {
                    TitlebarGlyphButton(symbol: model.isSidebarExpanded ? "sidebar.left" : "sidebar.right", help: model.i18n("titlebar.projects", fallback: "Projects")) {
                        model.toggleSidebarExpansion()
                    }

                    if model.appSettings.developer.showsNotificationTestButton {
                        TitlebarGlyphButton(symbol: "waveform.badge.magnifyingglass", help: model.i18n("titlebar.notification_test", fallback: "Notification Test")) {
                            model.triggerActivityTest()
                        }
                    }

                    TitlebarGlyphButton(symbol: "rectangle.split.2x1", help: model.i18n("titlebar.split", fallback: "Split")) {
                        model.splitSelectedPane(axis: .horizontal)
                    }
                }
                .padding(.leading, 86)
                .frame(height: rowHeight, alignment: .center)
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
                        TitlebarGlyphButton(symbol: "scroll", help: model.i18n("titlebar.debug_log", fallback: "Debug Log")) {
                            model.openDebugLog()
                        }
                    }

                    TitlebarGlyphButton(symbol: "terminal", help: model.i18n("titlebar.tab", fallback: "Tab")) {
                        model.createBottomTab()
                    }

                    TitlebarGlyphButton(symbol: "chart.bar.xaxis", help: model.i18n("titlebar.ai_assistant", fallback: "AI Assistant")) {
                        model.toggleRightPanel(.aiStats)
                    }

                    TitlebarGlyphButton(symbol: "point.3.filled.connected.trianglepath.dotted", help: model.i18n("titlebar.git", fallback: "Git")) {
                        model.toggleRightPanel(.git)
                    }
                }
                .padding(.trailing, 16)
                .frame(height: rowHeight, alignment: .center)
                .frame(maxHeight: .infinity, alignment: .center)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)

            ProjectTitleView(project: model.selectedProject)
                .frame(height: rowHeight, alignment: .center)
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

        let recognizer = NSClickGestureRecognizer(target: self, action: #selector(handleDoubleClick))
        recognizer.numberOfClicksRequired = 2
        addGestureRecognizer(recognizer)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    @objc
    private func handleDoubleClick() {
        window?.performZoom(nil)
    }
}

private struct TerminalShellView: View {
    let model: AppModel

    var body: some View {
        TerminalHorizontalSplitContainer(model: model)
        .background(model.terminalChromeColor)
        .clipShape(TerminalShellShape())
        .overlay(
            TerminalShellShape()
                .stroke(AppTheme.border, lineWidth: 1)
        )
    }
}

private struct RightPanelContainerView: View {
    let model: AppModel

    var body: some View {
        Group {
            switch model.rightPanel {
            case .git:
                GitPanelView(model: model)
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
    private let rightPanelContainer = NSView()
    private var rightPanelWidthConstraint: NSLayoutConstraint?
    private var isApplyingLayout = false
    private var collapsedRightPanelSize = NSSize(width: 0, height: 0)

    init(model: AppModel) {
        self.model = model
        self.workspaceHosting = NSHostingController(rootView: WorkspaceView(model: model))
        self.rightPanelHosting = NSHostingController(rootView: RightPanelContainerView(model: model))
        super.init(nibName: nil, bundle: nil)
        splitView.dividerStyle = .thin
        splitView.isVertical = true
        splitView.delegate = self
        splitView.customDividerColor = model.terminalDividerNSColor
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

        addChild(workspaceHosting)
        addChild(rightPanelHosting)
        splitView.addArrangedSubview(workspaceHosting.view)
        splitView.addArrangedSubview(rightPanelContainer)

        workspaceHosting.view.translatesAutoresizingMaskIntoConstraints = false
        rightPanelContainer.translatesAutoresizingMaskIntoConstraints = false
        workspaceHosting.view.setContentHuggingPriority(.defaultLow, for: .horizontal)
        rightPanelContainer.setContentCompressionResistancePriority(.required, for: .horizontal)
        splitView.setHoldingPriority(.defaultLow, forSubviewAt: 0)
        splitView.setHoldingPriority(.dragThatCanResizeWindow, forSubviewAt: 1)
        collapsedRightPanelSize = NSSize(width: 0, height: splitView.bounds.height)

        rightPanelContainer.addSubview(rightPanelHosting.view)
        rightPanelHosting.view.translatesAutoresizingMaskIntoConstraints = false
        NSLayoutConstraint.activate([
            rightPanelHosting.view.leadingAnchor.constraint(equalTo: rightPanelContainer.leadingAnchor),
            rightPanelHosting.view.trailingAnchor.constraint(equalTo: rightPanelContainer.trailingAnchor),
            rightPanelHosting.view.topAnchor.constraint(equalTo: rightPanelContainer.topAnchor),
            rightPanelHosting.view.bottomAnchor.constraint(equalTo: rightPanelContainer.bottomAnchor),
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

        splitView.customDividerColor = model.terminalDividerNSColor
        let isPanelVisible = rightPanel != nil
        splitView.showsCustomDivider = isPanelVisible
        rightPanelHosting.view.alphaValue = isPanelVisible ? 1 : 0

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
        view === workspaceHosting.view
    }

    func splitView(_ splitView: NSSplitView, constrainMinCoordinate proposedMinimumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        let totalWidth = splitView.bounds.width
        return max(520, totalWidth - 560)
    }

    func splitView(_ splitView: NSSplitView, constrainMaxCoordinate proposedMaximumPosition: CGFloat, ofSubviewAt dividerIndex: Int) -> CGFloat {
        splitView.bounds.width - 280
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
    private let buttonSize: CGFloat = 30
    private let iconSize: CGFloat = 15

    private var opticalOffset: CGFloat {
        switch symbol {
        case "terminal":
            return -0.5
        case "rectangle.split.2x1", "rectangle.split.1x2":
            return 0.5
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
                RoundedRectangle(cornerRadius: 7, style: .continuous)
                    .fill(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)

                Image(systemName: symbol)
                    .font(.system(size: iconSize, weight: .regular))
                    .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
                    .offset(y: opticalOffset)
                    .frame(width: iconSize, height: iconSize)
            }
            .frame(width: buttonSize, height: buttonSize)
            .contentShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
        }
        .frame(width: buttonSize, height: buttonSize)
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

    private let controlHeight: CGFloat = 26
    private let primaryWidth: CGFloat = 30
    private let menuWidth: CGFloat = 22

    var body: some View {
        HStack(spacing: 0) {
            Button(action: primaryAction) {
                PrimaryOpenIconView(prefersVSCode: prefersVSCode)
                    .frame(width: 16, height: 16)
                    .frame(width: primaryWidth, height: controlHeight)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!isEnabled)

            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(isHovered ? 0.6 : 0.4))
                .frame(width: 0.5, height: 16)

            Menu {
                Button {
                    openInVSCode()
                } label: {
                    AppLauncherMenuLabel(
                        title: model.i18n("open.vscode", fallback: "Open in VS Code"),
                        icon: .bundle("com.microsoft.VSCode", fallbackSystemName: "chevron.left.forwardslash.chevron.right")
                    )
                }

                Button {
                    revealInFinder()
                } label: {
                    AppLauncherMenuLabel(
                        title: model.i18n("open.finder", fallback: "Open in Finder"),
                        icon: .bundle("com.apple.finder", fallbackSystemName: "folder")
                    )
                }

                Button {
                    openInTerminal()
                } label: {
                    AppLauncherMenuLabel(
                        title: model.i18n("open.terminal", fallback: "Open in Terminal"),
                        icon: .bundle("com.apple.Terminal", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInITerm2()
                } label: {
                    AppLauncherMenuLabel(
                        title: model.i18n("open.iterm2", fallback: "Open in iTerm2"),
                        icon: .bundle("com.googlecode.iterm2", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInGhostty()
                } label: {
                    AppLauncherMenuLabel(
                        title: model.i18n("open.ghostty", fallback: "Open in Ghostty"),
                        icon: .bundle("com.mitchellh.ghostty", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInXcode()
                } label: {
                    AppLauncherMenuLabel(
                        title: model.i18n("open.xcode", fallback: "Open in Xcode"),
                        icon: .bundle("com.apple.dt.Xcode", fallbackSystemName: "hammer")
                    )
                }
            } label: {
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(isEnabled ? AppTheme.textSecondary : AppTheme.textMuted)
                    .frame(width: menuWidth, height: controlHeight)
                    .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .disabled(!isEnabled)
        }
        .frame(width: primaryWidth + menuWidth + 1, height: controlHeight)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(isHovered ? Color(nsColor: .quaternarySystemFill) : Color(nsColor: .quaternarySystemFill).opacity(0.5))
        )
        .overlay(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .stroke(Color(nsColor: .separatorColor).opacity(isHovered ? 0.4 : 0.2), lineWidth: 0.5)
        )
        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        .floatingTooltip(
            prefersVSCode
            ? model.i18n("open.project.vscode", fallback: "Open Project in VS Code")
            : model.i18n("open.project.finder", fallback: "Open Project in Finder"),
            placement: .below
        )
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
                    .offset(y: 0.4)

                Text(level.localizedTitle(using: model))
                    .font(.system(size: 12.5, weight: .semibold, design: .rounded))
                    .foregroundStyle(AppTheme.textPrimary.opacity(isShowingPopover || isHovered ? 1 : 0.9))
                    .lineLimit(1)
                    .offset(y: 0.4)
            }
            .padding(.leading, 8)
            .padding(.trailing, 10)
            .frame(height: 26)
            .background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(
                        isShowingPopover
                        ? level.accent.opacity(0.15)
                        : (isHovered ? level.accent.opacity(0.09) : Color(nsColor: .quaternarySystemFill).opacity(0.5))
                    )
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .stroke(
                        isShowingPopover
                        ? level.accent.opacity(0.24)
                        : Color(nsColor: .separatorColor).opacity(isHovered ? 0.4 : 0.2),
                        lineWidth: 0.5
                    )
            )
            .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        }
        .buttonStyle(.plain)
        .floatingTooltip(model.i18n("ai.today_level", fallback: "Today's Level"), enabled: !isShowingPopover, placement: .below)
        .popover(isPresented: $isShowingPopover, attachmentAnchor: .rect(.bounds), arrowEdge: .top) {
            AITodayLevelPopover(model: model, tokens: tokens, currentLevel: level)
        }
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
                    Text(model.i18n("ai.today_level", fallback: "Today's Level"))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.tertiary)

                    Text(currentLevel.localizedTitle(using: model))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)
                }

                Spacer(minLength: 12)

                VStack(alignment: .trailing, spacing: 2) {
                    Text(model.i18n("ai.today_tokens", fallback: "Today's Tokens"))
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

                            Text(String(format: model.i18n("common.need_format", fallback: "Need %@"), formatCompactToken(level.minimumTokens)))
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(.tertiary)
                        }

                        Spacer(minLength: 12)

                        if isCurrent {
                            Text(model.i18n("common.current", fallback: "Current"))
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
        case .blankSlate: return model.i18n("rank.iron", fallback: "Iron")
        case .bronze: return model.i18n("rank.bronze", fallback: "Bronze")
        case .silver: return model.i18n("rank.silver", fallback: "Silver")
        case .gold: return model.i18n("rank.gold", fallback: "Gold")
        case .platinum: return model.i18n("rank.platinum", fallback: "Platinum")
        case .diamond: return model.i18n("rank.diamond", fallback: "Diamond")
        case .master: return model.i18n("rank.master", fallback: "Master")
        case .grandmaster: return model.i18n("rank.grandmaster", fallback: "Grandmaster")
        }
    }

    var minimumTokens: Int {
        switch self {
        case .blankSlate: return 0
        case .bronze: return 1_000_000
        case .silver: return 5_000_000
        case .gold: return 15_000_000
        case .platinum: return 40_000_000
        case .diamond: return 80_000_000
        case .master: return 150_000_000
        case .grandmaster: return 300_000_000
        }
    }

    var accent: Color {
        switch self {
        case .blankSlate: return Color(hex: 0x5B616D)
        case .bronze: return Color(hex: 0xC98663)
        case .silver: return Color(hex: 0xC8D1E3)
        case .gold: return Color(hex: 0xF4C44C)
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
        Text(project?.name ?? "dmux")
            .font(.system(size: 13, weight: .semibold, design: .rounded))
            .foregroundStyle(AppTheme.textPrimary)
            .lineLimit(1)
            .frame(maxWidth: 260)
            .frame(height: 28, alignment: .center)
    }
}
