import AppKit
import ObjectiveC
import SwiftUI

private enum GitPanelFocusField: Hashable {
    case commitMessage
}

struct GitPanelView: View {
    let model: AppModel
    let gitStore: GitStore
    @State private var stagedExpanded = true
    @State private var changesExpanded = true
    @State private var untrackedExpanded = true
    @FocusState private var focusedField: GitPanelFocusField?
    @State private var filesScrollResetToken = UUID()

    var body: some View {
        VStack(spacing: 0) {
            if let gitState = gitStore.panelState.gitState {
                VStack(spacing: 0) {
                    GitPanelHeader(model: model)
                        .contentShape(Rectangle())
                        .onTapGesture {
                            focusedField = nil
                            NSApp.keyWindow?.makeFirstResponder(nil)
                        }

                    GitTopRegion(model: model, gitState: gitState, focusedField: $focusedField)

                    GitPanelSeparator()

                    GitFilesRegion(
                        model: model,
                        gitState: gitState,
                        stagedExpanded: $stagedExpanded,
                        changesExpanded: $changesExpanded,
                        untrackedExpanded: $untrackedExpanded,
                        scrollResetToken: filesScrollResetToken,
                        clearFocus: {
                            focusedField = nil
                            NSApp.keyWindow?.makeFirstResponder(nil)
                        }
                    )
                    .frame(maxWidth: .infinity, maxHeight: .infinity)

                    GitPanelSeparator()

                    GitHistoryRegion(model: model, history: gitStore.panelState.gitHistory, clearFocus: {
                        focusedField = nil
                        NSApp.keyWindow?.makeFirstResponder(nil)
                    })
                    .frame(height: 190)

                    GitPanelSeparator()

                    GitRemoteSyncBar(model: model, gitStore: gitStore)
                }
            } else {
                GitEmptyRepositoryView(model: model, gitStore: gitStore)
                .frame(maxWidth: .infinity, maxHeight: .infinity)
                .padding(24)
            }
        }
        .background(Color.clear)
        .onChange(of: stagedExpanded) { _, _ in
            filesScrollResetToken = UUID()
        }
        .onChange(of: changesExpanded) { _, _ in
            filesScrollResetToken = UUID()
        }
        .onChange(of: untrackedExpanded) { _, _ in
            filesScrollResetToken = UUID()
        }
    }
}

private struct GitEmptyRepositoryView: View {
    let model: AppModel
    let gitStore: GitStore

    private var isCheckingRepository: Bool {
        gitStore.panelState.isGitLoading && gitStore.panelState.gitState == nil
    }

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: isCheckingRepository ? "arrow.triangle.branch" : "point.topleft.down.curvedto.point.bottomright.up")
                .font(.system(size: 30, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)

            VStack(spacing: 6) {
                Text(isCheckingRepository ? String(localized: "git.empty.reading_status", defaultValue: "Reading Git Status", bundle: .module) : String(localized: "git.empty.not_repository", defaultValue: "Current Directory Is Not a Git Repository", bundle: .module))
                    .font(.system(size: 16, weight: .bold))
                    .foregroundStyle(AppTheme.textPrimary)
                Text(isCheckingRepository ? String(localized: "git.empty.loading_description", defaultValue: "Keep the current sidebar layout while repository status syncs in the background.", bundle: .module) : String(localized: "git.empty.description", defaultValue: "Initialize a repository or clone a remote repository to view commits, diffs, and branches here.", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 280)
            }

            if !isCheckingRepository {
                HStack(spacing: 10) {
                    Button(String(localized: "git.empty.initialize_repository", defaultValue: "Initialize Repository", bundle: .module), action: model.initializeGitRepository)
                        .buttonStyle(.borderedProminent)
                        .disabled(gitStore.panelState.isGitLoading)

                    Button(String(localized: "git.empty.clone_remote_repository", defaultValue: "Clone Remote Repository", bundle: .module), action: model.cloneGitRepository)
                        .buttonStyle(.bordered)
                        .disabled(gitStore.panelState.isGitLoading)

                    Button {
                        model.refreshGitState()
                    } label: {
                        Label(String(localized: "git.status.refresh", defaultValue: "Refresh Git Status", bundle: .module), systemImage: "arrow.clockwise")
                    }
                    .buttonStyle(.bordered)
                    .disabled(gitStore.panelState.isGitLoading)
                }
                .controlSize(.regular)
            }

            if let status = gitStore.panelState.gitOperationStatusText {
                VStack(alignment: .leading, spacing: 8) {
                    Text(status)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(AppTheme.textSecondary)
                        .frame(maxWidth: 280, alignment: .leading)

                    ProgressView(value: gitStore.panelState.gitOperationProgress ?? 0.05)
                        .tint(AppTheme.focus)
                        .frame(maxWidth: 280)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct GitRemoteSyncBar: View {
    let model: AppModel
    let gitStore: GitStore
    @State private var hoveredAction: GitRemoteOperation?

    private let activeOperationColor = AppTheme.focus

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 6) {
                if isRunningRemoteAction {
                    ProgressView()
                        .controlSize(.small)
                        .tint(.white)
                } else {
                    Image(systemName: statusIcon)
                        .font(.system(size: 12, weight: .semibold))
                }

                Text(statusText)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
            }
            .foregroundStyle(Color.white.opacity(0.92))
            .layoutPriority(0)

            Spacer(minLength: 8)

            HStack(spacing: 14) {
                remoteButton(
                    operation: .pull,
                    title: String(localized: "git.remote.pull", defaultValue: "Pull", bundle: .module),
                    help: pullHelp,
                    systemImage: "arrow.down",
                    isLoading: gitStore.panelState.activeGitRemoteOperation == .pull,
                    badge: gitStore.panelState.gitRemoteSyncState.hasUpstream && gitStore.panelState.gitRemoteSyncState.incomingCount > 0 ? gitStore.panelState.gitRemoteSyncState.incomingCount : nil,
                    action: model.pullGitBranch
                )

                remoteButton(
                    operation: .push,
                    title: String(localized: "git.remote.push", defaultValue: "Push", bundle: .module),
                    help: pushHelp,
                    systemImage: "arrow.up",
                    isLoading: gitStore.panelState.activeGitRemoteOperation == .push,
                    badge: gitStore.panelState.gitRemoteSyncState.hasUpstream && gitStore.panelState.gitRemoteSyncState.outgoingCount > 0 ? gitStore.panelState.gitRemoteSyncState.outgoingCount : nil,
                    action: model.pushGitBranch
                )
            }
            .fixedSize(horizontal: true, vertical: false)
            .layoutPriority(1)
        }
        .padding(.horizontal, 12)
        .frame(height: 32)
        .frame(maxWidth: .infinity)
        .background(statusBackground)
    }

    private var statusText: String {
        if gitStore.panelState.activeGitRemoteOperation == .pull {
            return String(localized: "git.remote.status.pulling", defaultValue: "Pulling Remote Updates", bundle: .module)
        }
        if gitStore.panelState.activeGitRemoteOperation == .push {
            return String(localized: "git.remote.status.pushing", defaultValue: "Pushing Current Branch", bundle: .module)
        }
        if gitStore.panelState.activeGitRemoteOperation == .forcePush {
            return String(localized: "git.remote.status.force_pushing", defaultValue: "Force Pushing Current Branch", bundle: .module)
        }

        let state = gitStore.panelState.gitRemoteSyncState
        if !state.hasUpstream {
            return String(localized: "git.remote.status.no_remote_branch", defaultValue: "No Remote Branch", bundle: .module)
        }
        if state.incomingCount == 0 && state.outgoingCount == 0 {
            return String(localized: "git.remote.status.synced", defaultValue: "Remote Is Synced", bundle: .module)
        }
        return String(localized: "git.remote.status.has_updates", defaultValue: "Remote Has Updates", bundle: .module)
    }

    private var pullHelp: String {
        if !gitStore.panelState.gitRemoteSyncState.hasUpstream {
            return String(localized: "git.remote.no_upstream_description", defaultValue: "The current branch does not have a remote branch yet.", bundle: .module)
        }
        return String(localized: "git.remote.pull_description", defaultValue: "Pull remote updates.", bundle: .module)
    }

    private var pushHelp: String {
        if !gitStore.panelState.gitRemoteSyncState.hasUpstream {
            return String(localized: "git.remote.no_upstream_description", defaultValue: "The current branch does not have a remote branch yet.", bundle: .module)
        }
        return String(localized: "git.remote.push_description", defaultValue: "Push the current branch to remote.", bundle: .module)
    }

    private var statusIcon: String {
        let state = gitStore.panelState.gitRemoteSyncState
        if !state.hasUpstream {
            return "arrow.triangle.branch"
        }
        if state.incomingCount == 0 && state.outgoingCount == 0 {
            return "checkmark.circle.fill"
        }
        return "arrow.triangle.2.circlepath"
    }

    private var isRunningRemoteAction: Bool {
        gitStore.panelState.activeGitRemoteOperation == .pull
            || gitStore.panelState.activeGitRemoteOperation == .push
            || gitStore.panelState.activeGitRemoteOperation == .forcePush
    }

    private var statusBackground: Color {
        if gitStore.panelState.activeGitRemoteOperation != nil {
            return activeOperationColor
        }
        return gitStore.panelState.gitRemoteSyncState.hasUpstream ? AppTheme.focus : AppTheme.textMuted.opacity(0.45)
    }

    @ViewBuilder
    private func remoteButton(
        operation: GitRemoteOperation,
        title: String,
        help: String,
        systemImage: String,
        isLoading: Bool,
        badge: Int?,
        action: @escaping () -> Void
    ) -> some View {
        Button(action: action) {
            HStack(spacing: 6) {
                Image(systemName: systemImage)
                    .font(.system(size: 12, weight: .bold))

                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)

                if let badge {
                    Text("\(badge)")
                        .font(.system(size: 10, weight: .bold, design: .rounded))
                        .monospacedDigit()
                        .foregroundStyle(statusBackground)
                        .padding(.horizontal, 6)
                        .padding(.vertical, 2)
                        .background(
                            Capsule()
                                .fill(Color.white)
                        )
                        .overlay(
                            Capsule()
                                .stroke(Color.black.opacity(0.08), lineWidth: 0.5)
                        )
                }
            }
            .foregroundStyle(Color.white.opacity(model.gitRemoteSyncState.hasUpstream ? 0.96 : 0.72))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(Color.white.opacity(hoveredAction == operation ? 0.22 : 0.001))
            )
        }
        .buttonStyle(.plain)
        .disabled(model.gitState == nil || !model.gitRemoteSyncState.hasUpstream || model.activeGitRemoteOperation != nil)
        .opacity(model.gitState == nil || !model.gitRemoteSyncState.hasUpstream || model.activeGitRemoteOperation != nil ? 0.72 : 1.0)
        .help(help)
        .onHover { hovering in
            hoveredAction = hovering ? operation : (hoveredAction == operation ? nil : hoveredAction)
        }
    }
}

private struct GitPanelSeparator: View {
    var body: some View {
        Rectangle()
            .fill(AppTheme.separator)
            .frame(height: 1)
    }
}

private struct GitBranchMenuTrigger: NSViewRepresentable {
    let model: AppModel

    func makeNSView(context: Context) -> GitBranchMenuButton {
        let button = GitBranchMenuButton()
        button.model = model
        button.setContentHuggingPriority(.required, for: .horizontal)
        button.setContentCompressionResistancePriority(.required, for: .horizontal)
        return button
    }

    func updateNSView(_ nsView: GitBranchMenuButton, context: Context) {
        nsView.model = model
        nsView.invalidateIntrinsicContentSize()
        nsView.needsDisplay = true
    }
}

private final class GitBranchMenuButton: NSButton {
    weak var model: AppModel?
    private var handlers: [NativeContextMenuHandler] = []

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        isBordered = false
        bezelStyle = .regularSquare
        focusRingType = .none
        target = self
        action = #selector(openMenu)
        setButtonType(.momentaryChange)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override var intrinsicContentSize: NSSize {
        guard let model else { return NSSize(width: 96, height: 24) }
        let branch = model.gitState?.branch ?? String(localized: "git.empty.no_repository", defaultValue: "No Repository", bundle: .module)
        let width = (branch as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: 15, weight: .bold)]).width
        return NSSize(width: width + 26, height: 24)
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.clear.setFill()
        dirtyRect.fill()
        guard let model else { return }

        let branch = model.gitState?.branch ?? String(localized: "git.empty.no_repository", defaultValue: "No Repository", bundle: .module)
        let attributes: [NSAttributedString.Key: Any] = [
            .font: NSFont.systemFont(ofSize: 15, weight: .bold),
            .foregroundColor: NSColor(AppTheme.warning),
        ]
        let text = NSAttributedString(string: branch, attributes: attributes)
        let textSize = text.size()
        let textRect = NSRect(x: 0, y: floor((bounds.height - textSize.height) / 2), width: textSize.width, height: textSize.height)
        text.draw(in: textRect)

        let centerX = textRect.maxX + 11
        let centerY = floor(bounds.midY)
        let path = NSBezierPath()
        path.lineWidth = 1.8
        path.lineCapStyle = .round
        path.move(to: NSPoint(x: centerX - 4, y: centerY - 1))
        path.line(to: NSPoint(x: centerX, y: centerY + 3))
        path.line(to: NSPoint(x: centerX + 4, y: centerY - 1))
        NSColor.white.withAlphaComponent(0.92).setStroke()
        path.stroke()
    }

    @objc
    private func openMenu() {
        guard let model else { return }

        let menu = NSMenu()
        handlers.removeAll()
        let remoteBranchGroups = Dictionary(grouping: model.gitRemoteBranches) { branch in
            branch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).first.map(String.init) ?? branch
        }

        func addAction(_ title: String, _ action: @escaping () -> Void) {
            let handler = NativeContextMenuHandler(action: action)
            handlers.append(handler)
            let item = NSMenuItem(title: title, action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
            item.target = handler
            menu.addItem(item)
        }

        func addSeparator() {
            menu.addItem(.separator())
        }

        func remoteAttributedTitle(name: String, url: String) -> NSAttributedString {
            let title = NSMutableAttributedString(
                string: name,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 13, weight: .semibold),
                    .foregroundColor: NSColor.labelColor,
                ]
            )
            title.append(NSAttributedString(string: "\n"))
            title.append(
                NSAttributedString(
                    string: url,
                    attributes: [
                        .font: NSFont.systemFont(ofSize: 11, weight: .regular),
                        .foregroundColor: NSColor.secondaryLabelColor,
                    ]
                )
            )
            return title
        }

        func remoteBranchAttributedTitle(shortName: String, fullName: String) -> NSAttributedString {
            let title = NSMutableAttributedString(
                string: shortName,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 13, weight: .semibold),
                    .foregroundColor: NSColor.labelColor,
                ]
            )
            title.append(NSAttributedString(string: "\n"))
            title.append(
                NSAttributedString(
                    string: fullName,
                    attributes: [
                        .font: NSFont.systemFont(ofSize: 11, weight: .regular),
                        .foregroundColor: NSColor.secondaryLabelColor,
                    ]
                )
            )
            return title
        }

        func localBranchAttributedTitle(name: String, upstream: String?) -> NSAttributedString {
            let title = NSMutableAttributedString(
                string: name,
                attributes: [
                    .font: NSFont.systemFont(ofSize: 13, weight: .semibold),
                    .foregroundColor: NSColor.labelColor,
                ]
            )

            if let upstream, !upstream.isEmpty {
                title.append(NSAttributedString(string: "\n"))
                title.append(
                    NSAttributedString(
                        string: upstream,
                        attributes: [
                            .font: NSFont.systemFont(ofSize: 11, weight: .regular),
                            .foregroundColor: NSColor.secondaryLabelColor,
                        ]
                    )
                )
            }

            return title
        }

        addAction(String(localized: "git.branch.new", defaultValue: "New Branch", bundle: .module)) { model.createGitBranch() }
        addSeparator()

        let localMenu = NSMenu(title: String(localized: "git.branch.local", defaultValue: "Local Branches", bundle: .module))
        if model.gitBranches.isEmpty {
            let item = NSMenuItem(title: String(localized: "git.branch.local.empty", defaultValue: "No Local Branches", bundle: .module), action: nil, keyEquivalent: "")
            item.isEnabled = false
            localMenu.addItem(item)
        } else {
            for branch in model.gitBranches {
                let isCurrentBranch = branch == model.gitState?.branch
                let upstream = model.gitBranchUpstreams[branch]
                let branchItem: NSMenuItem

                if isCurrentBranch {
                    branchItem = NSMenuItem(title: branch, action: nil, keyEquivalent: "")
                    branchItem.state = .on
                    branchItem.isEnabled = false
                } else {
                    let checkoutHandler = NativeContextMenuHandler(action: { model.checkoutGitBranch(branch) })
                    handlers.append(checkoutHandler)
                    branchItem = NSMenuItem(title: branch, action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                    branchItem.target = checkoutHandler
                }
                branchItem.attributedTitle = localBranchAttributedTitle(name: branch, upstream: upstream)
                localMenu.addItem(branchItem)
            }
        }
        let localItem = NSMenuItem(title: String(localized: "git.branch.local", defaultValue: "Local Branches", bundle: .module), action: nil, keyEquivalent: "")
        menu.setSubmenu(localMenu, for: localItem)
        menu.addItem(localItem)

        let mergeMenu = NSMenu(title: String(localized: "git.branch.merge_current", defaultValue: "Merge into Current Branch", bundle: .module))
        let mergeCandidates = model.gitBranches.filter { $0 != model.gitState?.branch }
        if mergeCandidates.isEmpty {
            let item = NSMenuItem(title: String(localized: "git.branch.merge.empty", defaultValue: "No Branches Available to Merge", bundle: .module), action: nil, keyEquivalent: "")
            item.isEnabled = false
            mergeMenu.addItem(item)
        } else {
            for branch in mergeCandidates {
                let mergeHandler = NativeContextMenuHandler(action: { model.mergeBranchIntoCurrent(branch) })
                handlers.append(mergeHandler)
                let item = NSMenuItem(title: branch, action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                item.target = mergeHandler
                mergeMenu.addItem(item)
            }
        }
        let mergeItem = NSMenuItem(title: String(localized: "git.branch.merge_current", defaultValue: "Merge into Current Branch", bundle: .module), action: nil, keyEquivalent: "")
        menu.setSubmenu(mergeMenu, for: mergeItem)
        menu.addItem(mergeItem)

        let remotesMenu = NSMenu(title: String(localized: "git.remote.remotes", defaultValue: "Remotes", bundle: .module))
        let defaultPushRemoteName = model.selectedProject?.gitDefaultPushRemoteName
        let addRemoteHandler = NativeContextMenuHandler(action: { model.addGitRemote() })
        handlers.append(addRemoteHandler)
        let addRemoteItem = NSMenuItem(title: String(localized: "git.remote.add", defaultValue: "Add Remote", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
        addRemoteItem.target = addRemoteHandler
        remotesMenu.addItem(addRemoteItem)
        remotesMenu.addItem(.separator())

        if model.gitRemotes.isEmpty {
            let item = NSMenuItem(title: String(localized: "git.remote.empty", defaultValue: "No Remotes", bundle: .module), action: nil, keyEquivalent: "")
            item.isEnabled = false
            remotesMenu.addItem(item)
        } else {
            for remote in model.gitRemotes {
                let remoteSubmenu = NSMenu(title: remote.name)
                let isDefaultPushRemote = defaultPushRemoteName == remote.name

                let toggleDefaultHandler = NativeContextMenuHandler(action: {
                    if isDefaultPushRemote {
                        model.clearDefaultPushRemote()
                    } else {
                        model.setDefaultPushRemote(remote)
                    }
                })
                handlers.append(toggleDefaultHandler)
                let toggleDefaultItem = NSMenuItem(title: String(localized: "git.remote.set_default", defaultValue: "Set as Default", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                toggleDefaultItem.target = toggleDefaultHandler
                toggleDefaultItem.state = isDefaultPushRemote ? .on : .off
                remoteSubmenu.addItem(toggleDefaultItem)

                remoteSubmenu.addItem(.separator())

                let copyURLHandler = NativeContextMenuHandler(action: {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(remote.url, forType: .string)
                    model.statusMessage = String(localized: "git.remote.copy_url.success", defaultValue: "Copied Remote Repository URL.", bundle: .module)
                })
                handlers.append(copyURLHandler)
                let copyURLItem = NSMenuItem(title: String(localized: "git.remote.copy_url", defaultValue: "Copy URL", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                copyURLItem.target = copyURLHandler
                remoteSubmenu.addItem(copyURLItem)

                let removeHandler = NativeContextMenuHandler(action: { model.removeGitRemote(remote) })
                handlers.append(removeHandler)
                let removeItem = NSMenuItem(title: String(localized: "git.remote.remove", defaultValue: "Remove Remote", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                removeItem.target = removeHandler
                remoteSubmenu.addItem(removeItem)

                let remoteItem = NSMenuItem(title: remote.name, action: nil, keyEquivalent: "")
                remoteItem.state = isDefaultPushRemote ? .on : .off
                remoteItem.attributedTitle = remoteAttributedTitle(name: remote.name, url: remote.url)
                remoteItem.toolTip = remote.url
                remotesMenu.setSubmenu(remoteSubmenu, for: remoteItem)
                remotesMenu.addItem(remoteItem)
            }
        }

        let remotesItem = NSMenuItem(title: String(localized: "git.remote.remotes", defaultValue: "Remotes", bundle: .module), action: nil, keyEquivalent: "")
        menu.setSubmenu(remotesMenu, for: remotesItem)
        menu.addItem(remotesItem)

        let remoteMenu = NSMenu(title: String(localized: "git.remote.branches", defaultValue: "Remote Branches", bundle: .module))
        let refreshHandler = NativeContextMenuHandler(action: { model.refreshRemoteBranches() })
        handlers.append(refreshHandler)
        let refresh = NSMenuItem(title: String(localized: "git.remote.branches.refresh", defaultValue: "Refresh Remote Branches", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
        refresh.target = refreshHandler
        remoteMenu.addItem(refresh)
        remoteMenu.addItem(.separator())
        if model.gitRemoteBranches.isEmpty {
            let item = NSMenuItem(title: String(localized: "git.remote.branches.empty", defaultValue: "No Remote Branches", bundle: .module), action: nil, keyEquivalent: "")
            item.isEnabled = false
            remoteMenu.addItem(item)
        } else {
            for remote in model.gitRemotes {
                let branches = (remoteBranchGroups[remote.name] ?? []).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
                let remoteSubmenu = NSMenu(title: remote.name)

                if branches.isEmpty {
                    let item = NSMenuItem(title: String(localized: "git.remote.branches.empty", defaultValue: "No Remote Branches", bundle: .module), action: nil, keyEquivalent: "")
                    item.isEnabled = false
                    remoteSubmenu.addItem(item)
                } else {
                    for branch in branches {
                        let branchMenu = NSMenu(title: branch)
                        let shortName = branch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).dropFirst().first.map(String.init) ?? branch
                        let checkoutHandler = NativeContextMenuHandler(action: { model.checkoutRemoteGitBranch(branch) })
                        handlers.append(checkoutHandler)
                        let checkout = NSMenuItem(title: String(localized: "git.remote.branch.checkout_local", defaultValue: "Checkout as Local Branch", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                        checkout.target = checkoutHandler
                        branchMenu.addItem(checkout)

                        let pushHandler = NativeContextMenuHandler(action: { model.pushCurrentLocalBranch(to: branch) })
                        handlers.append(pushHandler)
                        let pushItem = NSMenuItem(title: String(localized: "git.remote.branch.push_here", defaultValue: "Push to This Branch", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                        pushItem.target = pushHandler
                        branchMenu.addItem(pushItem)

                        let branchItem = NSMenuItem(title: shortName, action: nil, keyEquivalent: "")
                        branchItem.attributedTitle = remoteBranchAttributedTitle(shortName: shortName, fullName: branch)
                        remoteSubmenu.setSubmenu(branchMenu, for: branchItem)
                        remoteSubmenu.addItem(branchItem)
                    }
                }

                let remoteItem = NSMenuItem(title: remote.name, action: nil, keyEquivalent: "")
                remoteItem.attributedTitle = remoteAttributedTitle(name: remote.name, url: remote.url)
                remoteMenu.setSubmenu(remoteSubmenu, for: remoteItem)
                remoteMenu.addItem(remoteItem)
            }

            let ungroupedBranches = model.gitRemoteBranches.filter { branch in
                let remoteName = branch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).first.map(String.init) ?? ""
                return !model.gitRemotes.contains(where: { $0.name == remoteName })
            }
            if !ungroupedBranches.isEmpty {
                let otherMenu = NSMenu(title: String(localized: "git.misc.other", defaultValue: "Other", bundle: .module))
                for branch in ungroupedBranches.sorted(by: { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }) {
                    let branchMenu = NSMenu(title: branch)
                    let shortName = branch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).dropFirst().first.map(String.init) ?? branch
                    let checkoutHandler = NativeContextMenuHandler(action: { model.checkoutRemoteGitBranch(branch) })
                    handlers.append(checkoutHandler)
                    let checkout = NSMenuItem(title: String(localized: "git.remote.branch.checkout_local", defaultValue: "Checkout as Local Branch", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                    checkout.target = checkoutHandler
                    branchMenu.addItem(checkout)

                    let pushHandler = NativeContextMenuHandler(action: { model.pushCurrentLocalBranch(to: branch) })
                    handlers.append(pushHandler)
                    let pushItem = NSMenuItem(title: String(localized: "git.remote.branch.push_here", defaultValue: "Push to This Branch", bundle: .module), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                    pushItem.target = pushHandler
                    branchMenu.addItem(pushItem)

                    let branchItem = NSMenuItem(title: shortName, action: nil, keyEquivalent: "")
                    branchItem.attributedTitle = remoteBranchAttributedTitle(shortName: shortName, fullName: branch)
                    otherMenu.setSubmenu(branchMenu, for: branchItem)
                    otherMenu.addItem(branchItem)
                }

                let otherItem = NSMenuItem(title: String(localized: "git.misc.other", defaultValue: "Other", bundle: .module), action: nil, keyEquivalent: "")
                remoteMenu.setSubmenu(otherMenu, for: otherItem)
                remoteMenu.addItem(otherItem)
            }
        }
        let remoteItem = NSMenuItem(title: String(localized: "git.remote.branches", defaultValue: "Remote Branches", bundle: .module), action: nil, keyEquivalent: "")
        menu.setSubmenu(remoteMenu, for: remoteItem)
        menu.addItem(remoteItem)

        addSeparator()
        addAction(String(localized: "git.remote.fetch", defaultValue: "Fetch", bundle: .module)) { model.fetchGitBranch() }
        addAction(String(localized: "git.remote.pull", defaultValue: "Pull", bundle: .module)) { model.pullGitBranch() }
        addAction(String(localized: "git.remote.push", defaultValue: "Push", bundle: .module)) { model.pushGitBranch() }

        let pushRemoteMenu = NSMenu(title: String(localized: "git.remote.push_to", defaultValue: "Push To...", bundle: .module))
        if model.gitRemotes.isEmpty {
            let item = NSMenuItem(title: String(localized: "git.remote.empty", defaultValue: "No Remotes", bundle: .module), action: nil, keyEquivalent: "")
            item.isEnabled = false
            pushRemoteMenu.addItem(item)
        } else {
            for remote in model.gitRemotes {
                let pushHandler = NativeContextMenuHandler(action: { model.pushGitBranch(to: remote) })
                handlers.append(pushHandler)
                let item = NSMenuItem(title: remote.name, action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                item.target = pushHandler
                item.attributedTitle = remoteAttributedTitle(name: remote.name, url: remote.url)
                item.state = defaultPushRemoteName == remote.name ? .on : .off
                pushRemoteMenu.addItem(item)
            }
        }
        let pushRemoteItem = NSMenuItem(title: String(localized: "git.remote.push_to", defaultValue: "Push To...", bundle: .module), action: nil, keyEquivalent: "")
        menu.setSubmenu(pushRemoteMenu, for: pushRemoteItem)
        menu.addItem(pushRemoteItem)

        addAction(String(localized: "git.remote.force_push", defaultValue: "Force Push", bundle: .module)) { model.forcePushGitBranch() }
        addSeparator()
        addAction(String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module)) { model.undoLastGitCommit() }
        addAction(String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module)) { model.editLastGitCommitMessage() }
        addSeparator()
        addAction(String(localized: "git.repository.show_in_finder", defaultValue: "Show Repository in Finder", bundle: .module)) { model.revealRepositoryInFinder() }

        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: bounds.height + 4), in: self)
    }
}

private struct GitPanelHeader: View {
    let model: AppModel

    var body: some View {
        HStack {
            GitBranchMenuTrigger(model: model)
                .frame(height: 24)

            Spacer(minLength: 12)

            HStack(spacing: 8) {
                Button {
                    model.generateCommitMessage()
                } label: {
                    Image(systemName: model.isGeneratingCommitMessage ? "sparkles.rectangle.stack.fill" : "sparkles")
                        .font(.system(size: 13, weight: .semibold))
                }
                .buttonStyle(GitToolbarIconButtonStyle())
                .help(String(localized: "git.commit.generate_message", defaultValue: "Generate Commit Message", bundle: .module))

                Button {
                    model.refreshGitState()
                } label: {
                    Image(systemName: model.isGitLoading ? "hourglass" : "arrow.clockwise")
                        .font(.system(size: 13, weight: .semibold))
                }
                .buttonStyle(GitToolbarIconButtonStyle())
                .help(String(localized: "git.status.refresh", defaultValue: "Refresh Git Status", bundle: .module))
            }
        }
        .padding(.horizontal, 18)
        .padding(.top, 10)
        .padding(.bottom, 6)
    }
}

private struct GitTopRegion: View {
    let model: AppModel
    let gitState: GitRepositoryState
    let focusedField: FocusState<GitPanelFocusField?>.Binding
    @State private var selectedCommitAction: GitCommitAction = .commit

    private let composerFont = NSFont.systemFont(ofSize: 14, weight: .medium)
    private let composerHorizontalInset: CGFloat = 14
    private let composerVerticalInset: CGFloat = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            AppMultilineInputArea(
                text: Binding(
                    get: { model.commitMessage },
                    set: { model.commitMessage = $0 }
                ),
                placeholder: String(localized: "git.commit.message.placeholder", defaultValue: "Enter Commit Message", bundle: .module),
                isFocused: Binding(
                    get: { focusedField.wrappedValue == .commitMessage },
                    set: { focusedField.wrappedValue = $0 ? .commitMessage : nil }
                ),
                font: composerFont,
                horizontalInset: composerHorizontalInset,
                verticalInset: composerVerticalInset,
                enablesSpellChecking: false
            )
            .frame(height: composerHeight)

            GitCommitSplitButton(
                model: model,
                selectedAction: $selectedCommitAction,
                isDisabled: !gitState.hasStagedChanges || model.commitMessage.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                onSubmit: { model.performCommitAction(selectedCommitAction) }
            )
        }
        .padding(.horizontal, 18)
        .padding(.top, 6)
        .padding(.bottom, 18)
    }
    private var composerHeight: CGFloat {
        (composerFont.ascender - composerFont.descender + composerFont.leading) * 3 + (composerVerticalInset * 2)
    }
}

private struct GitCommitSplitButton: View {
    let model: AppModel
    @Binding var selectedAction: GitCommitAction
    let isDisabled: Bool
    let onSubmit: () -> Void
    @State private var menuAnchorView: NSView?
    private let menuSegmentWidth: CGFloat = 30
    private let menuIconSize: CGFloat = 10

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onSubmit) {
                Text(commitActionTitle(selectedAction))
            }
            .frame(maxWidth: .infinity)
            .buttonStyle(CommitMainButtonStyle())

            Button(action: presentMenu) {
                ZStack {
                    Color.clear
                    Image(systemName: "chevron.down")
                        .font(.system(size: menuIconSize, weight: .semibold))
                        .foregroundStyle(AppTheme.textPrimary)
                }
                .frame(width: menuSegmentWidth, height: 32)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .background(GitCommitMenuAnchorView(anchorView: $menuAnchorView))
        }
        .background(AppTheme.focus)
        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .stroke(Color(nsColor: .separatorColor).opacity(0.3), lineWidth: 0.5)
        }
        .overlay(alignment: .trailing) {
            HStack(spacing: 0) {
                Rectangle()
                    .fill(Color.white.opacity(isDisabled ? 0.08 : 0.14))
                    .frame(width: 1)
                Color.clear
                    .frame(width: menuSegmentWidth)
            }
            .allowsHitTesting(false)
        }
        .disabled(isDisabled)
        .opacity(isDisabled ? 0.5 : 1.0)
    }

    private func commitActionTitle(_ action: GitCommitAction) -> String {
        switch action {
        case .commit:
            return String(localized: "git.commit.action", defaultValue: "Commit", bundle: .module)
        case .commitAndPush:
            return String(localized: "git.commit.action_push", defaultValue: "Commit and Push", bundle: .module)
        case .commitAndSync:
            return String(localized: "git.commit.action_sync", defaultValue: "Commit and Sync", bundle: .module)
        }
    }

    private func presentMenu() {
        guard let anchorView = menuAnchorView else {
            return
        }

        let menu = NSMenu()
        var handlers: [GitCommitMenuActionHandler] = []

        func addItem(for action: GitCommitAction) {
            let title = commitActionTitle(action)
            let handler = GitCommitMenuActionHandler {
                selectedAction = action
            }
            handlers.append(handler)

            let item = NSMenuItem(title: title, action: #selector(GitCommitMenuActionHandler.performAction), keyEquivalent: "")
            item.target = handler
            menu.addItem(item)
        }

        GitCommitAction.allCases.forEach(addItem)

        objc_setAssociatedObject(anchorView, Unmanaged.passUnretained(anchorView).toOpaque(), handlers, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: anchorView.bounds.height + 4), in: anchorView)
    }
}

private final class GitCommitMenuActionHandler: NSObject {
    private let action: () -> Void

    init(action: @escaping () -> Void) {
        self.action = action
    }

    @objc
    func performAction() {
        action()
    }
}

private struct GitCommitMenuAnchorView: NSViewRepresentable {
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

private struct GitFilesRegion: View {
    let model: AppModel
    let gitState: GitRepositoryState
    @Binding var stagedExpanded: Bool
    @Binding var changesExpanded: Bool
    @Binding var untrackedExpanded: Bool
    let scrollResetToken: UUID
    let clearFocus: () -> Void

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                Color.clear
                    .frame(height: 0)
                    .id("git-files-top")

                LazyVStack(alignment: .leading, spacing: 0, pinnedViews: [.sectionHeaders]) {
                    GitListSection(
                        kind: .staged,
                        entries: gitState.staged,
                        accent: AppTheme.success,
                        isExpanded: $stagedExpanded,
                        primaryIcon: "minus.circle",
                        primaryAction: { model.unstage($0) },
                        secondaryIcon: nil,
                        secondaryAction: nil,
                        model: model
                    )

                    GitListSection(
                        kind: .changed,
                        entries: gitState.changes,
                        accent: AppTheme.warning,
                        isExpanded: $changesExpanded,
                        primaryIcon: "plus.circle",
                        primaryAction: { model.stage($0) },
                        secondaryIcon: "arrow.uturn.backward",
                        secondaryAction: { model.discard($0) },
                        model: model
                    )

                    GitListSection(
                        kind: .untracked,
                        entries: gitState.untracked,
                        accent: AppTheme.textSecondary,
                        isExpanded: $untrackedExpanded,
                        primaryIcon: "plus.circle",
                        primaryAction: { model.stage($0) },
                        secondaryIcon: "trash",
                        secondaryAction: { model.discard($0) },
                        model: model
                    )
                }
            }
            .scrollIndicators(.automatic)
            .background(Color.clear)
            .contentShape(Rectangle())
            .onTapGesture {
                clearFocus()
            }
            .onChange(of: scrollResetToken) { _, _ in
                proxy.scrollTo("git-files-top", anchor: .top)
            }
        }
    }
}

private struct GitListSection: View {
    let kind: GitFileKind
    let entries: [GitFileEntry]
    let accent: Color
    @Binding var isExpanded: Bool
    let primaryIcon: String
    let primaryAction: (GitFileEntry) -> Void
    let secondaryIcon: String?
    let secondaryAction: ((GitFileEntry) -> Void)?
    let model: AppModel

    private var selectedEntries: [GitFileEntry] {
        entries.filter { model.isGitEntrySelected($0) }
    }

    private var shouldShowHeaderActions: Bool {
        !selectedEntries.isEmpty
    }

    private var actionEntries: [GitFileEntry] {
        selectedEntries
    }

    private var headerActions: [GitSectionHeaderAction] {
        switch kind {
        case .staged:
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "minus", help: String(localized: "git.files.unstage_selected", defaultValue: "Unstage Selected", bundle: .module)) {
                    model.unstageEntries(actionEntries)
                }
            ]
        case .changed:
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "plus", help: String(localized: "git.files.stage_selected", defaultValue: "Stage Selected", bundle: .module)) {
                    model.stageEntries(actionEntries)
                },
                GitSectionHeaderAction(icon: "discard", help: String(localized: "git.files.discard_selected", defaultValue: "Discard Selected", bundle: .module)) {
                    model.discardEntries(actionEntries)
                }
            ]
        case .untracked:
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "plus", help: String(localized: "git.files.stage_selected", defaultValue: "Stage Selected", bundle: .module)) {
                    model.stageEntries(actionEntries)
                }
            ]
        }
    }

    var body: some View {
        Section {
            if isExpanded {
                GitListSectionContent(
                    entries: entries,
                    accent: accent,
                    model: model,
                    primaryIcon: primaryIcon,
                    primaryAction: primaryAction,
                    secondaryIcon: secondaryIcon,
                    secondaryAction: secondaryAction
                )
            }
        } header: {
            Button {
                isExpanded.toggle()
            } label: {
                HStack(spacing: 8) {
                    Image(systemName: isExpanded ? "chevron.down" : "chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .foregroundStyle(AppTheme.textSecondary)
                        .frame(width: 12, alignment: .center)

                    Text(displayTitle)
                        .font(.system(size: 12, weight: .semibold, design: .rounded))
                        .foregroundStyle(AppTheme.textSecondary)

                    Spacer()

                    if shouldShowHeaderActions, !headerActions.isEmpty {
                        HStack(spacing: 2) {
                            ForEach(Array(headerActions.enumerated()), id: \.offset) { _, action in
                                Button(action: action.action) {
                                    HeaderActionIcon(symbol: action.icon)
                                }
                                .buttonStyle(GitHeaderIconButtonStyle())
                                .help(action.help)
                            }
                        }
                    }

                    Text("\(entries.count)")
                        .font(.system(size: 11, weight: .bold, design: .rounded))
                        .foregroundStyle(accent)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .leading)
                .padding(.leading, 10)
                .padding(.trailing, 14)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .frame(maxWidth: .infinity, alignment: .leading)
            .frame(height: 34)
            .background {
                RoundedRectangle(cornerRadius: 0, style: .continuous)
                    .fill(AppTheme.aiPanelCardBackground.opacity(0.6))
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(AppTheme.separator)
                    .frame(height: 1)
            }
            .zIndex(1)
        }
    }

    private var displayTitle: String {
        switch kind {
        case .staged:
            return String(localized: "git.files.staged", defaultValue: "Staged", bundle: .module)
        case .changed:
            return String(localized: "git.files.changes", defaultValue: "Changes", bundle: .module)
        case .untracked:
            return String(localized: "git.files.untracked", defaultValue: "Untracked", bundle: .module)
        }
    }
}

private struct GitListSectionContent: View {
    let entries: [GitFileEntry]
    let accent: Color
    let model: AppModel
    let primaryIcon: String
    let primaryAction: (GitFileEntry) -> Void
    let secondaryIcon: String?
    let secondaryAction: ((GitFileEntry) -> Void)?

    var body: some View {
        if entries.isEmpty {
            EmptyView()
        } else {
            VStack(spacing: 0) {
                ForEach(entries) { entry in
                    GitFileRow(
                        entry: entry,
                        accent: accent,
                        model: model,
                        primaryIcon: primaryIcon,
                        primaryAction: { primaryAction(entry) },
                        secondaryIcon: secondaryIcon,
                        secondaryAction: secondaryAction.map { action in { action(entry) } }
                    )
                }
            }
        }
    }
}

private struct GitFileRow: View {
    let entry: GitFileEntry
    let accent: Color
    let model: AppModel
    let primaryIcon: String
    let primaryAction: () -> Void
    let secondaryIcon: String?
    let secondaryAction: (() -> Void)?

    @State private var isHovered = false

    private var isSelected: Bool {
        model.isGitEntrySelected(entry)
    }

    var body: some View {
        Button {
            let modifierFlags = NSApp.currentEvent?.modifierFlags ?? NSEvent.modifierFlags
            let shiftPressed = modifierFlags.contains(.shift)
            let commandPressed = modifierFlags.contains(.command)

            if commandPressed {
                model.toggleGitEntrySelection(entry)
            } else {
                model.selectGitEntry(entry, extendingRange: shiftPressed)
            }
            model.loadDiff(for: entry)
        } label: {
            HStack(spacing: 8) {
                Color.clear
                    .frame(width: 12, height: 1)

                Text(entry.path)
                    .font(.system(size: 12, weight: .medium, design: .monospaced))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.tail)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .help(entry.path)

                trailingAccessorySlot
            }
            .frame(maxWidth: .infinity, alignment: .leading)
            .padding(.leading, 10)
            .padding(.trailing, 14)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(rowBackground)
        .overlay {
            NativeContextMenuRegion(
                onOpen: {
                    model.prepareGitEntryContextMenu(entry)
                },
                menuProvider: {
                    buildGitFileContextMenu(model: model, fallbackEntry: entry)
                }
            )
        }
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var rowBackground: some View {
        Rectangle()
            .fill(baseRowColor)
            .overlay(alignment: .leading) {
                if isSelected {
                    Rectangle()
                        .fill(accent)
                        .frame(width: 2)
                }
            }
    }

    private var baseRowColor: Color {
        if isSelected {
            return AppTheme.focus.opacity(0.14)
        }

        return isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear
    }

    private var actionSlotWidth: CGFloat {
        secondaryIcon == nil ? 44 : 68
    }

    private var trailingSlotWidth: CGFloat {
        max(actionSlotWidth, GitStatusBadge.width)
    }

    private var trailingAccessorySlot: some View {
        Color.clear
            .frame(width: trailingSlotWidth, height: GitStatusBadge.height)
            .overlay(alignment: .trailing) {
                if isHovered {
                    GitHoverActions(
                        primaryIcon: primaryIcon,
                        primaryAction: primaryAction,
                        secondaryIcon: secondaryIcon,
                        secondaryAction: secondaryAction
                    )
                } else {
                    GitStatusBadge(entry: entry, accent: accent)
                }
            }
    }
}

private struct GitStatusBadge: View {
    let entry: GitFileEntry
    let accent: Color

    static let width: CGFloat = 14
    static let height: CGFloat = 14

    private var label: String {
        switch entry.kind {
        case .staged:
            return "S"
        case .changed:
            return "M"
        case .untracked:
            return "U"
        }
    }

    var body: some View {
        Text(label)
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .foregroundStyle(accent)
            .frame(width: Self.width, height: Self.height, alignment: .trailing)
    }
}

private struct GitHoverActions: View {
    let primaryIcon: String
    let primaryAction: () -> Void
    let secondaryIcon: String?
    let secondaryAction: (() -> Void)?

    var body: some View {
        HStack(spacing: 2) {
            if let secondaryIcon, let secondaryAction {
                Button(action: secondaryAction) {
                    HoverActionIcon(symbol: secondaryIcon)
                }
                .buttonStyle(GitIconButtonStyle())
            }

            Button(action: primaryAction) {
                HoverActionIcon(symbol: primaryIcon)
            }
            .buttonStyle(GitIconButtonStyle())
        }
        .padding(.leading, 12)
    }
}

private struct GitSectionHeaderAction {
    let icon: String
    let help: String
    let action: () -> Void
}

private struct HeaderActionIcon: View {
    let symbol: String

    var body: some View {
        switch symbol {
        case "plus":
            Image(systemName: "plus")
                .font(.system(size: 10, weight: .semibold))
        case "minus":
            Image(systemName: "minus")
                .font(.system(size: 10, weight: .semibold))
        case "discard":
            Image(systemName: "arrow.uturn.backward")
                .font(.system(size: 10, weight: .semibold))
        default:
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
        }
    }
}

private struct HoverActionIcon: View {
    let symbol: String

    var body: some View {
        switch symbol {
        case "plus.circle":
            Image(systemName: "plus")
                .font(.system(size: 10, weight: .semibold))
        case "minus.circle":
            Image(systemName: "minus")
                .font(.system(size: 10, weight: .semibold))
        case "trash":
            Image(systemName: "trash")
                .font(.system(size: 10, weight: .semibold))
        case "arrow.uturn.backward":
            Image(systemName: "arrow.uturn.backward")
                .font(.system(size: 10, weight: .semibold))
        default:
            Image(systemName: symbol)
                .font(.system(size: 10, weight: .semibold))
        }
    }
}

private struct GitHistoryRegion: View {
    let model: AppModel
    let history: [GitCommitEntry]
    let clearFocus: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(String(localized: "git.history.title", defaultValue: "Git History", bundle: .module))
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                    .foregroundStyle(AppTheme.textSecondary)
                Spacer()
            }
            .padding(.horizontal, 16)
            .frame(height: 34)
            .background {
                RoundedRectangle(cornerRadius: 0, style: .continuous)
                    .fill(AppTheme.aiPanelCardBackground.opacity(0.6))
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(AppTheme.separator)
                    .frame(height: 1)
            }

            if history.isEmpty {
                Text(String(localized: "git.history.empty", defaultValue: "No Commit History", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .padding(.horizontal, 16)
                    .padding(.top, 12)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(history) { item in
                            GitHistoryRow(model: model, item: item)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                }
                .frame(maxHeight: .infinity, alignment: .top)
            }
        }
        .padding(.bottom, 14)
        .contentShape(Rectangle())
        .onTapGesture {
            clearFocus()
        }
    }
}

private struct GitHistoryRow: View {
    let model: AppModel
    let item: GitCommitEntry
    @State private var isHovered = false

    private var isSelected: Bool {
        model.selectedGitCommitHash == item.hash
    }

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            GitGraphPrefixView(prefix: item.graphPrefix)
                .frame(width: graphWidth)
                .frame(height: 26, alignment: .leading)

            Text(item.subject)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)

            Text(item.relativeDate)
                .font(.system(size: 12, weight: .medium, design: .rounded))
                .foregroundStyle(AppTheme.textMuted)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

            HStack(spacing: 4) {
                ForEach(Array(compactDecorations.enumerated()), id: \.offset) { index, decoration in
                    GitDecorationTag(text: decoration, color: tagColor(for: decoration, index: index))
                }

                if overflowDecorationCount > 0 {
                    GitDecorationTag(text: "+\(overflowDecorationCount)", color: AppTheme.textSecondary)
                }
            }
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, 6)
        .frame(minHeight: 26)
        .background(rowBackground)
        .overlay(alignment: .trailing) {
            if isSelected {
                HStack(spacing: 4) {
                    Button {
                        model.revertGitCommit(item)
                    } label: {
                        Image(systemName: "arrow.uturn.backward")
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .buttonStyle(GitHistoryActionButtonStyle())
                    .help(String(localized: "git.history.revert_commit", defaultValue: "Revert This Commit", bundle: .module))

                    Button {
                        model.createBranch(from: item)
                    } label: {
                        Image(systemName: "point.topleft.down.curvedto.point.bottomright.up")
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .buttonStyle(GitHistoryActionButtonStyle())
                    .help(String(localized: "git.history.create_branch_from_commit", defaultValue: "Create Branch from This Commit", bundle: .module))
                }
                .padding(.leading, 12)
                .padding(.trailing, 6)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            model.selectGitCommit(item)
        }
        .onHover { hovering in
            isHovered = hovering
        }
        .help("\(item.subject)\n\(item.author) · \(item.relativeDate)")
        .overlay {
            NativeContextMenuRegion(
                onOpen: {
                    model.prepareGitCommitContextMenu(item)
                },
                menuProvider: {
                    buildGitCommitContextMenu(model: model, commit: item)
                }
            )
        }
    }

    private var graphWidth: CGFloat {
        34
    }

    private var compactDecorations: [String] {
        Array(item.decorations.prefix(1)).map { decoration in
            decoration
                .replacingOccurrences(of: "HEAD -> ", with: "HEAD→")
                .replacingOccurrences(of: "origin/", with: "o/")
        }
    }

    private var overflowDecorationCount: Int {
        max(0, item.decorations.count - compactDecorations.count)
    }

    private var rowBackground: some View {
        ZStack(alignment: .leading) {
            if isSelected {
                AppTheme.focus.opacity(0.14)
            } else if isHovered {
                Color(nsColor: .quaternarySystemFill)
            } else {
                Color.clear
            }

            if isSelected {
                Rectangle()
                    .fill(AppTheme.focus)
                    .frame(width: 2)
            }
        }
    }

    private func tagColor(for decoration: String, index: Int) -> Color {
        if decoration.contains("HEAD") || decoration == "master" || decoration == "main" {
            return AppTheme.focus
        }

        let palette: [Color] = [AppTheme.success, AppTheme.warning, Color.pink, Color.orange]
        return palette[index % palette.count]
    }
}

@MainActor
private func buildGitFileContextMenu(model: AppModel, fallbackEntry: GitFileEntry) -> [NativeContextMenuAction] {
    let selectedEntries = model.selectedGitEntriesForContextMenu.isEmpty ? [fallbackEntry] : model.selectedGitEntriesForContextMenu
    let allStaged = !selectedEntries.isEmpty && selectedEntries.allSatisfy { $0.kind == .staged }
    let hasNonStaged = selectedEntries.contains { $0.kind != .staged }
    let allUntracked = !selectedEntries.isEmpty && selectedEntries.allSatisfy { $0.kind == .untracked }

    var actions: [NativeContextMenuAction] = []

    actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.copy_selected_paths", defaultValue: "Copy Selected Paths", bundle: .module) : String(localized: "git.files.copy_path", defaultValue: "Copy Path", bundle: .module)) {
        model.copyGitPaths(selectedEntries)
    })

    actions.append(.action(String(localized: "git.files.show_in_finder", defaultValue: "Show in Finder", bundle: .module)) {
        model.revealGitEntriesInFinder(selectedEntries)
    })

    actions.append(.separator)

    if allStaged {
        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.unstage_selected", defaultValue: "Unstage Selected", bundle: .module) : String(localized: "git.files.unstage", defaultValue: "Unstage", bundle: .module)) {
            model.unstageEntries(selectedEntries)
        })
    } else {
        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.stage_selected", defaultValue: "Stage Selected", bundle: .module) : String(localized: "git.files.stage", defaultValue: "Stage", bundle: .module)) {
            model.stageEntries(selectedEntries)
        })
    }

    if hasNonStaged {
        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.discard_selected_changes", defaultValue: "Discard Selected Changes", bundle: .module) : String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module)) {
            model.discardEntries(selectedEntries)
        })
    }

    if allUntracked {
        actions.append(.separator)

        actions.append(.action(String(localized: "git.ignore.add", defaultValue: "Add to .gitignore", bundle: .module)) {
            model.addGitEntriesToIgnore(selectedEntries)
        })

        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.delete_selected_files", defaultValue: "Delete Selected Files", bundle: .module) : String(localized: "git.files.delete_file", defaultValue: "Delete File", bundle: .module)) {
            model.discardEntries(selectedEntries)
        })
    }

    return actions
}

@MainActor
private func buildGitCommitContextMenu(model: AppModel, commit: GitCommitEntry) -> [NativeContextMenuAction] {
    var actions: [NativeContextMenuAction] = [
        .action(String(localized: "git.history.copy_commit_hash", defaultValue: "Copy Commit Hash", bundle: .module)) { model.copyGitCommitHash(commit) },
        .action(String(localized: "git.history.checkout_commit", defaultValue: "Checkout This Commit", bundle: .module)) { model.checkoutGitCommit(commit) },
        .action(String(localized: "git.history.create_branch_from_commit", defaultValue: "Create Branch from This Commit", bundle: .module)) { model.createBranch(from: commit) },
    ]

    if model.gitHistory.first?.hash == commit.hash {
        actions.append(.separator)
        actions.append(.action(String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module)) { model.undoLastGitCommit() })
        actions.append(.action(String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module)) { model.editLastGitCommitMessage() })
    }

    actions.append(.separator)
    actions.append(.action(String(localized: "git.history.revert_commit", defaultValue: "Revert This Commit", bundle: .module)) { model.revertGitCommit(commit) })
    actions.append(.separator)
    actions.append(.action(String(localized: "git.history.restore_local", defaultValue: "Restore This Revision Locally", bundle: .module)) { model.restoreGitCommit(commit, forceRemote: false) })
    actions.append(.action(String(localized: "git.history.restore_remote", defaultValue: "Restore This Revision Remotely", bundle: .module)) { model.restoreGitCommit(commit, forceRemote: true) })

    return actions
}

private enum NativeContextMenuAction {
    case separator
    case action(String, () -> Void)
}

@MainActor
private struct NativeContextMenuRegion: NSViewRepresentable {
    let onOpen: () -> Void
    let menuProvider: () -> [NativeContextMenuAction]

    func makeNSView(context: Context) -> NativeContextMenuView {
        let view = NativeContextMenuView()
        view.onOpen = onOpen
        view.menuProvider = menuProvider
        return view
    }

    func updateNSView(_ nsView: NativeContextMenuView, context: Context) {
        nsView.onOpen = onOpen
        nsView.menuProvider = menuProvider
    }
}

@MainActor
private final class NativeContextMenuView: NSView {
    var onOpen: (() -> Void)?
    var menuProvider: (() -> [NativeContextMenuAction])?
    private var handlers: [NativeContextMenuHandler] = []

    override var isOpaque: Bool { false }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let event = NSApp.currentEvent else { return nil }
        switch event.type {
        case .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp:
            return self
        default:
            return nil
        }
    }

    override func rightMouseDown(with event: NSEvent) {
        onOpen?()
        let menu = NSMenu()
        handlers.removeAll()

        for action in menuProvider?() ?? [] {
            switch action {
            case .separator:
                menu.addItem(.separator())
            case let .action(title, callback):
                let handler = NativeContextMenuHandler(action: callback)
                handlers.append(handler)
                let item = NSMenuItem(title: title, action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                item.target = handler
                menu.addItem(item)
            }
        }

        NSMenu.popUpContextMenu(menu, with: event, for: self)
    }
}

@MainActor
private final class NativeContextMenuHandler: NSObject {
    let action: () -> Void

    init(action: @escaping () -> Void) {
        self.action = action
    }

    @objc
    func performAction() {
        action()
    }
}

private struct GitHistoryActionButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitSecondaryHoverIconButtonBody(configuration: configuration)
    }
}

private struct GitDecorationTag: View {
    let text: String
    let color: Color

    var body: some View {
        Text(text)
            .font(.system(size: 9, weight: .semibold, design: .rounded))
            .foregroundStyle(color)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(color.opacity(0.14))
            .clipShape(Capsule())
    }
}

private struct GitGraphPrefixView: View {
    let prefix: String

    private let columnWidth: CGFloat = 8
    private let strokeWidth: CGFloat = 1.25
    private let palette: [Color] = [AppTheme.focus, AppTheme.success, AppTheme.warning, Color.pink, Color.orange]

    var body: some View {
        Canvas { context, size in
            let chars = Array(prefix)
            let startX = max(0, size.width - CGFloat(chars.count) * columnWidth)

            for (index, char) in chars.enumerated() {
                let centerX = startX + CGFloat(index) * columnWidth + columnWidth / 2
                let color = palette[index % palette.count]
                var path = Path()

                switch char {
                case "|":
                    path.move(to: CGPoint(x: centerX, y: -8))
                    path.addLine(to: CGPoint(x: centerX, y: size.height + 8))
                    context.stroke(path, with: .color(color), lineWidth: strokeWidth)
                case "/":
                    path.move(to: CGPoint(x: centerX + 2.5, y: -8))
                    path.addLine(to: CGPoint(x: centerX - 2.5, y: size.height + 8))
                    context.stroke(path, with: .color(color), lineWidth: strokeWidth)
                case "\\":
                    path.move(to: CGPoint(x: centerX - 2.5, y: -8))
                    path.addLine(to: CGPoint(x: centerX + 2.5, y: size.height + 8))
                    context.stroke(path, with: .color(color), lineWidth: strokeWidth)
                case "*", "o":
                    path.move(to: CGPoint(x: centerX, y: -8))
                    path.addLine(to: CGPoint(x: centerX, y: size.height + 8))
                    context.stroke(path, with: .color(color.opacity(0.5)), lineWidth: 1)

                    let nodeRect = CGRect(x: centerX - 3.5, y: size.height / 2 - 3.5, width: 7, height: 7)
                    context.fill(Path(ellipseIn: nodeRect), with: .color(color))
                default:
                    continue
                }
            }
        }
    }
}

private struct GitTagButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 10, weight: .bold, design: .rounded))
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
            .foregroundStyle(AppTheme.textPrimary)
            .background(AppTheme.panel.opacity(configuration.isPressed ? 0.7 : 1.0))
            .clipShape(Capsule())
    }
}

private struct GitToolbarIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitToolbarIconButtonBody(configuration: configuration)
    }
}

private struct GitToolbarIconButtonBody: View {
    let configuration: ButtonStyle.Configuration
    @State private var isHovered = false

    var body: some View {
        configuration.label
            .foregroundStyle(
                isHovered || configuration.isPressed
                    ? AppTheme.textPrimary
                    : AppTheme.textSecondary
            )
            .frame(width: 28, height: 28)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(backgroundColor)
            )
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .onHover { hovering in
                isHovered = hovering
            }
    }

    private var backgroundColor: Color {
        if configuration.isPressed {
            return Color(nsColor: .tertiarySystemFill)
        }
        if isHovered {
            return Color(nsColor: .quaternarySystemFill)
        }
        return Color.clear
    }
}

private struct GitIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitSecondaryHoverIconButtonBody(configuration: configuration)
    }
}

private struct GitHeaderIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitSecondaryHoverIconButtonBody(configuration: configuration)
    }
}

private struct GitSecondaryHoverIconButtonBody: View {
    let configuration: ButtonStyle.Configuration
    @State private var isHovered = false

    var body: some View {
        configuration.label
            .foregroundStyle(
                (isHovered || configuration.isPressed)
                    ? AppTheme.textPrimary
                    : AppTheme.textSecondary
            )
            .frame(width: 22, height: 22)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(backgroundColor)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .stroke(borderColor, lineWidth: 0.5)
            )
            .contentShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
            .onHover { hovering in
                isHovered = hovering
            }
    }

    private var backgroundColor: Color {
        if configuration.isPressed {
            return AppTheme.card.opacity(0.9)
        }
        return AppTheme.panel.opacity(0.88)
    }

    private var borderColor: Color {
        if configuration.isPressed {
            return AppTheme.separator.opacity(0.5)
        }
        if isHovered {
            return AppTheme.separator.opacity(0.45)
        }
        return AppTheme.separator.opacity(0.3)
    }
}

private struct CommitButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .semibold, design: .rounded))
            .frame(maxWidth: .infinity)
            .padding(.vertical, 12)
            .foregroundStyle(AppTheme.textPrimary)
            .background(AppTheme.focus.opacity(configuration.isPressed ? 0.75 : 1.0))
            .clipShape(RoundedRectangle(cornerRadius: 14, style: .continuous))
    }
}

private struct CommitMainButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .semibold, design: .rounded))
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity)
            .frame(height: 32)
            .foregroundStyle(Color.white.opacity(isEnabled ? 0.98 : 0.78))
            .background(Color.white.opacity(isEnabled && configuration.isPressed ? 0.08 : 0))
    }
}
