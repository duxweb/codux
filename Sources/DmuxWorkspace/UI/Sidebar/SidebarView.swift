import AppKit
import QuartzCore
import SwiftUI
import UniformTypeIdentifiers

private enum ProjectRowDragPayload {
    static let type = UTType.text.identifier

    static func provider(for projectID: UUID) -> NSItemProvider {
        NSItemProvider(object: projectID.uuidString as NSString)
    }
}

struct SidebarView: View {
    let model: AppModel
    @Environment(\.openSettings) private var openSettings
    @State private var draggingProjectID: UUID?
    @State private var didReorderProjects = false

    var body: some View {
        let _ = model.activityRenderVersion

        VStack(spacing: 0) {
            if model.isSidebarExpanded {
                Text(String(localized: "sidebar.workspace", defaultValue: "Workspace", bundle: .module))
                    .font(.system(size: 17, weight: .bold, design: .rounded))
                    .foregroundStyle(AppTheme.textPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 18)
                .padding(.top, 18)
                .padding(.bottom, 12)
            }

            ScrollView {
                LazyVStack(spacing: 4) {
                    ForEach(model.projects) { project in
                        let activityPhase = model.activityPhase(for: project.id)
                        ProjectRow(
                            project: project,
                            isExpanded: model.isSidebarExpanded,
                            isSelected: project.id == model.selectedProjectID,
                            activityPhase: activityPhase,
                            activityCount: model.activityIndicatorCount(for: project.id, phase: activityPhase),
                            worktreeSummary: model.worktreeStatusSummary(for: project.id),
                            draggingProjectID: $draggingProjectID,
                            onMove: { draggedProjectID in
                                model.moveProject(draggedProjectID, to: project.id, persists: false)
                            },
                            onFinishMove: {
                                guard didReorderProjects else { return }
                                didReorderProjects = false
                                model.scheduleProjectOrderPersist()
                            },
                            didMove: { didReorderProjects = true }
                        ) {
                            model.selectProject(project.id)
                        }
                        .contextMenu {
                            Button(String(localized: "worktree.create.title", defaultValue: "New Worktree", bundle: .module)) {
                                model.createWorktree(for: project.id)
                            }
                            Divider()
                            Button(String(localized: "sidebar.project.open_folder", defaultValue: "Open Folder", bundle: .module)) {
                                model.openProjectDirectory(project.id)
                            }
                            Menu(String(localized: "open.ide", defaultValue: "Open in IDE", bundle: .module)) {
                                ForEach(ProjectOpenApplication.ideApplications) { application in
                                    Button(application.localizedOpenTitle) {
                                        model.openProject(project.id, in: application)
                                    }
                                }
                            }
                            Divider()
                            Button(String(localized: "common.rename", defaultValue: "Rename", bundle: .module)) {
                                model.editProject(project.id)
                            }
                            Button(String(localized: "sidebar.project.edit", defaultValue: "Edit Project", bundle: .module)) {
                                model.editProject(project.id)
                            }
                            Divider()
                            Button(String(localized: "sidebar.project.remove", defaultValue: "Remove Project", bundle: .module), role: .destructive) {
                                model.removeProject(project.id)
                            }
                        }
                    }
                }
                .padding(.horizontal, model.isSidebarExpanded ? 12 : 16)
                .padding(.top, model.isSidebarExpanded ? 4 : 18)
                .padding(.bottom, 16)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            VStack(spacing: 6) {
                SidebarFooterButton(
                    symbol: "plus",
                    title: String(localized: "sidebar.footer.add_project", defaultValue: "Add Project", bundle: .module),
                    isExpanded: model.isSidebarExpanded
                ) {
                    model.addProject()
                }

                SidebarFooterButton(
                    symbol: "gearshape",
                    title: String(localized: "sidebar.footer.settings", defaultValue: "Settings", bundle: .module),
                    isExpanded: model.isSidebarExpanded
                ) {
                    openSettings()
                }

                SidebarHelpMenuButton(
                    model: model,
                    title: String(localized: "sidebar.footer.help", defaultValue: "Help", bundle: .module),
                    isExpanded: model.isSidebarExpanded
                ) {
                    AboutWindowPresenter.show(model: model)
                } openGitHub: {
                    model.openURL(AppSupportLinks.github)
                } openIssues: {
                    model.openURL(AppSupportLinks.issues)
                } openWebsite: {
                    model.openURL(AppSupportLinks.website)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, model.isSidebarExpanded ? 12 : 0)
            .padding(.bottom, 16)
            .padding(.top, 12)
        }
        .background(Color.clear)
        .onDrop(of: [UTType.fileURL.identifier], isTargeted: nil) { providers in
            ProjectDirectoryDropPayload.loadURLs(from: providers) { urls in
                model.importDroppedProjectDirectories(urls)
            }
            return true
        }
        .animation(.snappy(duration: 0.22), value: model.isSidebarExpanded)
    }
}

enum ProjectDirectoryDropPayload {
    static func loadURLs(from providers: [NSItemProvider], completion: @escaping @MainActor ([URL]) -> Void) {
        let collector = ProjectDirectoryDropURLCollector()
        let group = DispatchGroup()

        for provider in providers where provider.hasItemConformingToTypeIdentifier(UTType.fileURL.identifier) {
            group.enter()
            provider.loadItem(forTypeIdentifier: UTType.fileURL.identifier, options: nil) { item, _ in
                defer { group.leave() }
                let loadedURL: URL?
                if let url = item as? URL {
                    loadedURL = url
                } else if let data = item as? Data,
                          let value = String(data: data, encoding: .utf8),
                          let url = URL(string: value) {
                    loadedURL = url
                } else if let value = item as? String,
                          let url = URL(string: value) {
                    loadedURL = url
                } else {
                    loadedURL = nil
                }
                if let loadedURL {
                    collector.append(loadedURL)
                }
            }
        }

        group.notify(queue: .main) {
            let urls = collector.urls()
            Task { @MainActor in
                completion(urls)
            }
        }
    }
}

private final class ProjectDirectoryDropURLCollector: @unchecked Sendable {
    private let lock = NSLock()
    private var storedURLs: [URL] = []

    func append(_ url: URL) {
        lock.lock()
        storedURLs.append(url)
        lock.unlock()
    }

    func urls() -> [URL] {
        lock.lock()
        let urls = storedURLs
        lock.unlock()
        return urls
    }
}

private struct SidebarHelpMenuButton: View {
    let model: AppModel
    let title: String
    let isExpanded: Bool
    let showAbout: () -> Void
    let openGitHub: () -> Void
    let openIssues: () -> Void
    let openWebsite: () -> Void
    @State private var isHovered = false
    @State private var menuAnchorView: NSView?

    var body: some View {
        Button {
            presentMenu()
        } label: {
            SidebarFooterLabel(
                symbol: "ellipsis.circle",
                title: title,
                isExpanded: isExpanded,
                isHovered: isHovered
            )
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: isExpanded ? .leading : .center)
        .floatingTooltip(title, enabled: !isExpanded, placement: .right)
        .onHover { hovering in
            isHovered = hovering
        }
        .background(SidebarMenuAnchorView(anchorView: $menuAnchorView))
        .frame(height: 30)
    }

    private func presentMenu() {
        guard let anchorView = menuAnchorView else {
            return
        }

        let menu = NSMenu()
        var handlers: [SidebarMenuActionHandler] = []

        func addItem(_ title: String, action: @escaping () -> Void) {
            let handler = SidebarMenuActionHandler(action: action)
            handlers.append(handler)
            let item = NSMenuItem(title: title, action: #selector(SidebarMenuActionHandler.performAction), keyEquivalent: "")
            item.target = handler
            menu.addItem(item)
        }

        addItem(String(localized: "common.about", defaultValue: "About", bundle: .module), action: showAbout)
        menu.addItem(.separator())
        addItem(String(localized: "menu.help.github", defaultValue: "GitHub", bundle: .module), action: openGitHub)
        addItem(String(localized: "menu.help.github_issue", defaultValue: "GitHub Issue", bundle: .module), action: openIssues)
        addItem(String(localized: "menu.help.website", defaultValue: "Official Website", bundle: .module), action: openWebsite)

        objc_setAssociatedObject(anchorView, Unmanaged.passUnretained(anchorView).toOpaque(), handlers, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: anchorView.bounds.height + 4), in: anchorView)
    }
}

private struct SidebarMenuAnchorView: NSViewRepresentable {
    @Binding var anchorView: NSView?

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async {
            anchorView = view
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        if anchorView !== nsView {
            DispatchQueue.main.async {
                anchorView = nsView
            }
        }
    }
}

private final class SidebarMenuActionHandler: NSObject {
    private let action: () -> Void

    init(action: @escaping () -> Void) {
        self.action = action
    }

    @objc
    func performAction() {
        action()
    }
}

private struct ProjectRow: View {
    let project: Project
    let isExpanded: Bool
    let isSelected: Bool
    let activityPhase: ProjectActivityPhase
    let activityCount: Int?
    let worktreeSummary: String?
    @Binding var draggingProjectID: UUID?
    let onMove: (UUID) -> Void
    let onFinishMove: () -> Void
    let didMove: () -> Void
    let action: () -> Void

    @State private var isDropTarget = false

    private var isDragging: Bool {
        draggingProjectID == project.id
    }

    private var showsDropTarget: Bool {
        isDropTarget && draggingProjectID != nil && draggingProjectID != project.id
    }

    private var selectionContainer: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(showsDropTarget ? AppTheme.focus.opacity(0.14) : (isSelected ? AppTheme.sidebarSelectionFill : Color.clear))
    }

    var body: some View {
        Group {
            if isExpanded {
                HStack(spacing: 9) {
                    ProjectBadge(project: project, isSelected: isSelected, size: 38, activityPhase: activityPhase, activityCount: activityCount)

                    VStack(alignment: .leading, spacing: 4) {
                        Text(project.name)
                            .font(.system(size: 13, weight: .semibold, design: .rounded))
                            .foregroundStyle(isSelected ? AppTheme.textPrimary : AppTheme.textSecondary.opacity(0.9))
                            .lineLimit(1)

                        Text(worktreeSummary ?? project.path)
                            .font(.system(size: 11, weight: .medium, design: .default))
                            .foregroundStyle(isSelected ? AppTheme.textMuted : AppTheme.textMuted.opacity(0.75))
                            .lineLimit(1)
                    }

                    Spacer(minLength: 0)
                }
            } else {
                VStack(spacing: 6) {
                    ProjectBadge(project: project, isSelected: isSelected, size: 36, activityPhase: activityPhase, activityCount: activityCount)
                }
            }
        }
        .padding(isExpanded ? 8 : 6)
        .frame(maxWidth: .infinity)
        .background(selectionContainer)
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .stroke(showsDropTarget ? AppTheme.focus.opacity(0.72) : Color.clear, lineWidth: 1.2)
        }
        .opacity(isDragging ? 0.46 : (isSelected ? 1.0 : 0.8))
        .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onTapGesture(perform: action)
        .onDrag {
            draggingProjectID = project.id
            return ProjectRowDragPayload.provider(for: project.id)
        } preview: {
            Color.white.opacity(0.001).frame(width: 1, height: 1)
        }
        .onDrop(
            of: [ProjectRowDragPayload.type],
            delegate: ProjectRowReorderDropDelegate(
                targetProjectID: project.id,
                draggingProjectID: $draggingProjectID,
                isDropTarget: $isDropTarget,
                onMove: onMove,
                onFinishMove: onFinishMove,
                didMove: didMove
            )
        )
        .floatingTooltip(project.name, enabled: !isExpanded, placement: .right)
    }
}

private struct ProjectRowReorderDropDelegate: DropDelegate {
    let targetProjectID: UUID
    @Binding var draggingProjectID: UUID?
    @Binding var isDropTarget: Bool
    let onMove: (UUID) -> Void
    let onFinishMove: () -> Void
    let didMove: () -> Void

    func dropEntered(info: DropInfo) {
        guard let draggingProjectID,
              draggingProjectID != targetProjectID else {
            return
        }
        isDropTarget = true
        move(draggingProjectID)
    }

    private func move(_ draggedProjectID: UUID) {
        withAnimation(.snappy(duration: 0.16)) {
            onMove(draggedProjectID)
        }
        didMove()
    }

    func dropUpdated(info: DropInfo) -> DropProposal? {
        DropProposal(operation: .move)
    }

    func dropExited(info: DropInfo) {
        isDropTarget = false
    }

    func performDrop(info: DropInfo) -> Bool {
        var transaction = Transaction()
        transaction.disablesAnimations = true
        withTransaction(transaction) {
            isDropTarget = false
            draggingProjectID = nil
        }
        onFinishMove()
        return true
    }
}

private struct ProjectBadge: View {
    let project: Project
    let isSelected: Bool
    let size: CGFloat
    let activityPhase: ProjectActivityPhase
    let activityCount: Int?

    private var initials: String {
        if let badgeText = project.badgeText?.trimmingCharacters(in: .whitespacesAndNewlines), !badgeText.isEmpty {
            return String(badgeText.prefix(2)).uppercased()
        }
        let words = project.name.split(separator: " ")
        let letters = words.prefix(2).compactMap { $0.first }
        let value = String(letters)
        return value.isEmpty ? String(project.name.prefix(1)).uppercased() : value.uppercased()
    }

    private var badgeColor: Color {
        Color(hexString: project.badgeColorHex ?? (isSelected ? "#6B2D73" : "#2E2236"))
    }

    private var badgeGradient: LinearGradient {
        LinearGradient(
            colors: isSelected
                ? [badgeColor.opacity(0.94), badgeColor.opacity(0.80)]
                : [badgeColor.opacity(0.80), badgeColor.opacity(0.66)],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                .fill(badgeGradient)
                .shadow(color: Color.black.opacity(isSelected ? 0.12 : 0.05), radius: isSelected ? 8 : 4, y: 1)

            if let symbol = project.badgeSymbol {
                Image(systemName: symbol)
                    .font(.system(size: size * 0.34, weight: .bold))
                    .foregroundStyle(Color.white.opacity(isSelected ? 0.96 : 0.88))
            } else {
                Text(initials)
                    .font(.system(size: size * 0.34, weight: .bold, design: .rounded))
                    .foregroundStyle(Color.white.opacity(isSelected ? 0.96 : 0.88))
            }
        }
        .frame(width: size, height: size)
        .overlay(alignment: .topTrailing) {
            ActivityBadgeView(phase: activityPhase, count: activityCount)
                .offset(x: 4, y: -4)
        }
    }
}

private struct ActivityBadgeView: View {
    let phase: ProjectActivityPhase
    let count: Int?

    var body: some View {
        Group {
            switch phase {
            case .idle:
                EmptyView()
            case .loading, .running:
                runningBadge
            case .waitingInput:
                solidBadge(color: AppTheme.warning)
            case .completed:
                solidBadge(color: Color(hex: 0x31C46B))
            }
        }
    }

    private var displayCount: String? {
        guard let count, count > 1 else {
            return nil
        }
        return count > 9 ? "9+" : "\(count)"
    }

    private var badgeSize: CGFloat {
        displayCount == nil ? 12 : 14
    }

    private var solidBadgeSize: CGFloat {
        displayCount == nil ? 12 : 14
    }

    private var countFontSize: CGFloat {
        displayCount?.count == 2 ? 7.2 : 8.0
    }

    private var runningBadge: some View {
        RunningActivityBadgeIndicator(
            displayCount: displayCount,
            countFontSize: countFontSize
        )
            .frame(width: badgeSize, height: badgeSize)
    }

    private func solidBadge(color: Color) -> some View {
        Circle()
            .fill(color)
            .frame(width: solidBadgeSize, height: solidBadgeSize)
            .overlay(
                Circle().stroke(Color.white.opacity(0.18), lineWidth: 1)
            )
            .overlay {
                if let displayCount {
                    Text(displayCount)
                        .font(.system(size: countFontSize, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(Color.white)
                        .minimumScaleFactor(0.7)
                }
            }
    }
}

private struct RunningActivityBadgeIndicator: NSViewRepresentable {
    let displayCount: String?
    let countFontSize: CGFloat

    func makeNSView(context: Context) -> RunningActivityBadgeView {
        let view = RunningActivityBadgeView(frame: .zero)
        view.configure(displayCount: displayCount, countFontSize: countFontSize)
        return view
    }

    func updateNSView(_ nsView: RunningActivityBadgeView, context: Context) {
        nsView.configure(displayCount: displayCount, countFontSize: countFontSize)
    }
}

private final class RunningActivityBadgeView: NSView {
    private static let rotationAnimationKey = "codux-sidebar-activity-rotation"

    private let backgroundLayer = CAShapeLayer()
    private let trackLayer = CAShapeLayer()
    private let arcLayer = CAShapeLayer()
    private let dotLayer = CAShapeLayer()
    private let countLabel = CATextLayer()
    private var displayCount: String?
    private var countFontSize: CGFloat = 6.2

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        wantsLayer = true
        layer?.masksToBounds = false

        backgroundLayer.fillColor = NSColor.black.withAlphaComponent(0.28).cgColor
        trackLayer.fillColor = NSColor.clear.cgColor
        trackLayer.strokeColor = NSColor.white.withAlphaComponent(0.16).cgColor
        trackLayer.lineWidth = 1.6
        arcLayer.fillColor = NSColor.clear.cgColor
        arcLayer.strokeColor = NSColor.white.cgColor
        arcLayer.lineWidth = 1.8
        arcLayer.lineCap = .round
        dotLayer.fillColor = NSColor(calibratedRed: 1.0, green: 0.64, blue: 0.18, alpha: 1).cgColor
        countLabel.alignmentMode = .center
        countLabel.contentsScale = NSScreen.main?.backingScaleFactor ?? 2
        countLabel.foregroundColor = NSColor.white.cgColor
        countLabel.truncationMode = .end

        layer?.addSublayer(backgroundLayer)
        layer?.addSublayer(trackLayer)
        layer?.addSublayer(arcLayer)
        layer?.addSublayer(dotLayer)
        layer?.addSublayer(countLabel)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func configure(displayCount: String?, countFontSize: CGFloat) {
        guard self.displayCount != displayCount || self.countFontSize != countFontSize else {
            return
        }
        self.displayCount = displayCount
        self.countFontSize = countFontSize
        needsLayout = true
    }

    override func layout() {
        super.layout()
        CATransaction.begin()
        CATransaction.setDisableActions(true)

        let circleRect = bounds.insetBy(dx: 0.5, dy: 0.5)
        let center = CGPoint(x: bounds.midX, y: bounds.midY)
        let radius = max(1, min(circleRect.width, circleRect.height) / 2)

        backgroundLayer.frame = bounds
        backgroundLayer.path = CGPath(ellipseIn: bounds, transform: nil)

        trackLayer.frame = bounds
        trackLayer.path = CGPath(ellipseIn: circleRect.insetBy(dx: 1.1, dy: 1.1), transform: nil)

        let arcPath = CGMutablePath()
        arcPath.addArc(
            center: center,
            radius: max(1, radius - 1.4),
            startAngle: CGFloat.pi * 0.15,
            endAngle: CGFloat.pi * 1.52,
            clockwise: false
        )
        arcLayer.frame = bounds
        arcLayer.path = arcPath

        if let displayCount {
            backgroundLayer.fillColor = NSColor(calibratedRed: 1.0, green: 0.64, blue: 0.18, alpha: 1).cgColor
            trackLayer.isHidden = true
            arcLayer.isHidden = true
            dotLayer.isHidden = true
            countLabel.isHidden = false
            countLabel.string = displayCount
            countLabel.font = NSFont.monospacedDigitSystemFont(ofSize: countFontSize, weight: .bold)
            countLabel.fontSize = countFontSize
            countLabel.frame = CGRect(x: bounds.minX, y: bounds.midY - countFontSize * 0.68, width: bounds.width, height: countFontSize * 1.5)
        } else {
            backgroundLayer.fillColor = NSColor.black.withAlphaComponent(0.28).cgColor
            trackLayer.isHidden = false
            arcLayer.isHidden = false
            dotLayer.isHidden = false
            countLabel.isHidden = true
            dotLayer.frame = CGRect(x: bounds.midX - 2.5, y: bounds.midY - 2.5, width: 5, height: 5)
            dotLayer.path = CGPath(ellipseIn: dotLayer.bounds, transform: nil)
        }

        CATransaction.commit()
        ensureRotationAnimation()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            arcLayer.removeAnimation(forKey: Self.rotationAnimationKey)
        } else {
            ensureRotationAnimation()
        }
    }

    private func ensureRotationAnimation() {
        guard window != nil, bounds.width > 0, arcLayer.animation(forKey: Self.rotationAnimationKey) == nil else {
            return
        }
        let animation = CABasicAnimation(keyPath: "transform.rotation.z")
        animation.fromValue = 0
        animation.toValue = Double.pi * 2
        animation.duration = 1.1
        animation.repeatCount = .infinity
        animation.isRemovedOnCompletion = false
        arcLayer.add(animation, forKey: Self.rotationAnimationKey)
    }
}

private struct SidebarFooterButton: View {
    let symbol: String
    let title: String
    let isExpanded: Bool
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            SidebarFooterLabel(
                symbol: symbol,
                title: title,
                isExpanded: isExpanded,
                isHovered: isHovered
            )
        }
        .frame(maxWidth: .infinity, alignment: isExpanded ? .leading : .center)
        .buttonStyle(.plain)
        .floatingTooltip(title, enabled: !isExpanded, placement: .right)
        .onHover { hovering in
            isHovered = hovering
        }
        .frame(height: 30)
    }
}

private struct SidebarFooterLabel: View {
    let symbol: String
    let title: String
    let isExpanded: Bool
    let isHovered: Bool

    var body: some View {
        if isExpanded {
            HStack(spacing: 10) {
                Image(systemName: symbol)
                    .font(.system(size: 16, weight: .regular))
                    .frame(width: 18, height: 18)

                Text(title)
                    .font(.system(size: 13, weight: .medium, design: .rounded))
                    .lineLimit(1)

                Spacer(minLength: 0)
            }
            .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
            .padding(.horizontal, 14)
            .frame(height: 30)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
        } else {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .regular))
                .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
                .frame(width: 28, height: 28)
                .background(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                .contentShape(Rectangle())
        }
    }
}
