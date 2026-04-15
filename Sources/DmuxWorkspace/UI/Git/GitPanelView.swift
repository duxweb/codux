import AppKit
import ObjectiveC
import SwiftUI

private enum GitPanelFocusField: Hashable {
    case commitMessage
}

struct GitPanelView: View {
    let model: AppModel
    @State private var stagedExpanded = true
    @State private var changesExpanded = true
    @State private var untrackedExpanded = true
    @FocusState private var focusedField: GitPanelFocusField?
    @State private var filesScrollResetToken = UUID()

    var body: some View {
        VStack(spacing: 0) {
            GitPanelHeader(model: model)
                .contentShape(Rectangle())
                .onTapGesture {
                    focusedField = nil
                    NSApp.keyWindow?.makeFirstResponder(nil)
                }

            if let gitState = model.gitPanelState.gitState {
                VStack(spacing: 0) {
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

                    GitHistoryRegion(model: model, history: model.gitPanelState.gitHistory, clearFocus: {
                        focusedField = nil
                        NSApp.keyWindow?.makeFirstResponder(nil)
                    })
                    .frame(height: 190)

                    GitPanelSeparator()

                    GitRemoteSyncBar(model: model)
                }
            } else {
                GitEmptyRepositoryView(model: model)
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

    private var isCheckingRepository: Bool {
        model.gitPanelState.isGitLoading && model.gitPanelState.gitState == nil
    }

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: isCheckingRepository ? "arrow.triangle.branch" : "point.topleft.down.curvedto.point.bottomright.up")
                .font(.system(size: 30, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)

            VStack(spacing: 6) {
                Text(isCheckingRepository ? model.i18n("git.empty.reading_status", fallback: "Reading Git Status") : model.i18n("git.empty.not_repository", fallback: "Current Directory Is Not a Git Repository"))
                    .font(.system(size: 16, weight: .bold))
                    .foregroundStyle(AppTheme.textPrimary)
                Text(isCheckingRepository ? model.i18n("git.empty.loading_description", fallback: "Keep the current sidebar layout while repository status syncs in the background.") : model.i18n("git.empty.description", fallback: "Initialize a repository or clone a remote repository to view commits, diffs, and branches here."))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 280)
            }

            if !isCheckingRepository {
                HStack(spacing: 10) {
                    Button(model.i18n("git.empty.initialize_repository", fallback: "Initialize Repository"), action: model.initializeGitRepository)
                        .buttonStyle(.borderedProminent)
                        .disabled(model.gitPanelState.isGitLoading)

                    Button(model.i18n("git.empty.clone_remote_repository", fallback: "Clone Remote Repository"), action: model.cloneGitRepository)
                        .buttonStyle(.bordered)
                        .disabled(model.gitPanelState.isGitLoading)
                }
                .controlSize(.regular)
            }

            if let status = model.gitPanelState.gitOperationStatusText {
                VStack(alignment: .leading, spacing: 8) {
                    Text(status)
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(AppTheme.textSecondary)
                        .frame(maxWidth: 280, alignment: .leading)

                    ProgressView(value: model.gitPanelState.gitOperationProgress ?? 0.05)
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
                    title: model.i18n("git.remote.pull", fallback: "Pull"),
                    help: pullHelp,
                    systemImage: "arrow.down",
                    isLoading: model.gitPanelState.activeGitRemoteOperation == .pull,
                    badge: model.gitPanelState.gitRemoteSyncState.hasUpstream && model.gitPanelState.gitRemoteSyncState.incomingCount > 0 ? model.gitPanelState.gitRemoteSyncState.incomingCount : nil,
                    action: model.pullGitBranch
                )

                remoteButton(
                    operation: .push,
                    title: model.i18n("git.remote.push", fallback: "Push"),
                    help: pushHelp,
                    systemImage: "arrow.up",
                    isLoading: model.gitPanelState.activeGitRemoteOperation == .push,
                    badge: model.gitPanelState.gitRemoteSyncState.hasUpstream && model.gitPanelState.gitRemoteSyncState.outgoingCount > 0 ? model.gitPanelState.gitRemoteSyncState.outgoingCount : nil,
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
        if model.gitPanelState.activeGitRemoteOperation == .pull {
            return model.i18n("git.remote.status.pulling", fallback: "Pulling Remote Updates")
        }
        if model.gitPanelState.activeGitRemoteOperation == .push {
            return model.i18n("git.remote.status.pushing", fallback: "Pushing Current Branch")
        }
        if model.gitPanelState.activeGitRemoteOperation == .forcePush {
            return model.i18n("git.remote.status.force_pushing", fallback: "Force Pushing Current Branch")
        }

        let state = model.gitPanelState.gitRemoteSyncState
        if !state.hasUpstream {
            return model.i18n("git.remote.status.no_remote_branch", fallback: "No Remote Branch")
        }
        if state.incomingCount == 0 && state.outgoingCount == 0 {
            return model.i18n("git.remote.status.synced", fallback: "Remote Is Synced")
        }
        return model.i18n("git.remote.status.has_updates", fallback: "Remote Has Updates")
    }

    private var pullHelp: String {
        if !model.gitPanelState.gitRemoteSyncState.hasUpstream {
            return model.i18n("git.remote.no_upstream_description", fallback: "The current branch does not have a remote branch yet.")
        }
        return model.i18n("git.remote.pull_description", fallback: "Pull remote updates.")
    }

    private var pushHelp: String {
        if !model.gitPanelState.gitRemoteSyncState.hasUpstream {
            return model.i18n("git.remote.no_upstream_description", fallback: "The current branch does not have a remote branch yet.")
        }
        return model.i18n("git.remote.push_description", fallback: "Push the current branch to remote.")
    }

    private var statusIcon: String {
        let state = model.gitPanelState.gitRemoteSyncState
        if !state.hasUpstream {
            return "arrow.triangle.branch"
        }
        if state.incomingCount == 0 && state.outgoingCount == 0 {
            return "checkmark.circle.fill"
        }
        return "arrow.triangle.2.circlepath"
    }

    private var isRunningRemoteAction: Bool {
        model.gitPanelState.activeGitRemoteOperation == .pull
            || model.gitPanelState.activeGitRemoteOperation == .push
            || model.gitPanelState.activeGitRemoteOperation == .forcePush
    }

    private var statusBackground: Color {
        if model.gitPanelState.activeGitRemoteOperation != nil {
            return activeOperationColor
        }
        return model.gitPanelState.gitRemoteSyncState.hasUpstream ? AppTheme.focus : AppTheme.textMuted.opacity(0.45)
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
        let branch = model.gitState?.branch ?? model.i18n("git.empty.no_repository", fallback: "No Repository")
        let width = (branch as NSString).size(withAttributes: [.font: NSFont.systemFont(ofSize: 15, weight: .bold)]).width
        return NSSize(width: width + 26, height: 24)
    }

    override func draw(_ dirtyRect: NSRect) {
        NSColor.clear.setFill()
        dirtyRect.fill()
        guard let model else { return }

        let branch = model.gitState?.branch ?? model.i18n("git.empty.no_repository", fallback: "No Repository")
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

        addAction(model.i18n("git.branch.new", fallback: "New Branch")) { model.createGitBranch() }
        addSeparator()

        let localMenu = NSMenu(title: model.i18n("git.branch.local", fallback: "Local Branches"))
        if model.gitBranches.isEmpty {
            let item = NSMenuItem(title: model.i18n("git.branch.local.empty", fallback: "No Local Branches"), action: nil, keyEquivalent: "")
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
        let localItem = NSMenuItem(title: model.i18n("git.branch.local", fallback: "Local Branches"), action: nil, keyEquivalent: "")
        menu.setSubmenu(localMenu, for: localItem)
        menu.addItem(localItem)

        let mergeMenu = NSMenu(title: model.i18n("git.branch.merge_current", fallback: "Merge into Current Branch"))
        let mergeCandidates = model.gitBranches.filter { $0 != model.gitState?.branch }
        if mergeCandidates.isEmpty {
            let item = NSMenuItem(title: model.i18n("git.branch.merge.empty", fallback: "No Branches Available to Merge"), action: nil, keyEquivalent: "")
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
        let mergeItem = NSMenuItem(title: model.i18n("git.branch.merge_current", fallback: "Merge into Current Branch"), action: nil, keyEquivalent: "")
        menu.setSubmenu(mergeMenu, for: mergeItem)
        menu.addItem(mergeItem)

        let remotesMenu = NSMenu(title: model.i18n("git.remote.remotes", fallback: "Remotes"))
        let defaultPushRemoteName = model.selectedProject?.gitDefaultPushRemoteName
        let addRemoteHandler = NativeContextMenuHandler(action: { model.addGitRemote() })
        handlers.append(addRemoteHandler)
        let addRemoteItem = NSMenuItem(title: model.i18n("git.remote.add", fallback: "Add Remote"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
        addRemoteItem.target = addRemoteHandler
        remotesMenu.addItem(addRemoteItem)
        remotesMenu.addItem(.separator())

        if model.gitRemotes.isEmpty {
            let item = NSMenuItem(title: model.i18n("git.remote.empty", fallback: "No Remotes"), action: nil, keyEquivalent: "")
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
                let toggleDefaultItem = NSMenuItem(title: model.i18n("git.remote.set_default", fallback: "Set as Default"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                toggleDefaultItem.target = toggleDefaultHandler
                toggleDefaultItem.state = isDefaultPushRemote ? .on : .off
                remoteSubmenu.addItem(toggleDefaultItem)

                remoteSubmenu.addItem(.separator())

                let copyURLHandler = NativeContextMenuHandler(action: {
                    NSPasteboard.general.clearContents()
                    NSPasteboard.general.setString(remote.url, forType: .string)
                    model.statusMessage = model.i18n("git.remote.copy_url.success", fallback: "Copied Remote Repository URL.")
                })
                handlers.append(copyURLHandler)
                let copyURLItem = NSMenuItem(title: model.i18n("git.remote.copy_url", fallback: "Copy URL"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                copyURLItem.target = copyURLHandler
                remoteSubmenu.addItem(copyURLItem)

                let removeHandler = NativeContextMenuHandler(action: { model.removeGitRemote(remote) })
                handlers.append(removeHandler)
                let removeItem = NSMenuItem(title: model.i18n("git.remote.remove", fallback: "Remove Remote"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
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

        let remotesItem = NSMenuItem(title: model.i18n("git.remote.remotes", fallback: "Remotes"), action: nil, keyEquivalent: "")
        menu.setSubmenu(remotesMenu, for: remotesItem)
        menu.addItem(remotesItem)

        let remoteMenu = NSMenu(title: model.i18n("git.remote.branches", fallback: "Remote Branches"))
        let refreshHandler = NativeContextMenuHandler(action: { model.refreshRemoteBranches() })
        handlers.append(refreshHandler)
        let refresh = NSMenuItem(title: model.i18n("git.remote.branches.refresh", fallback: "Refresh Remote Branches"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
        refresh.target = refreshHandler
        remoteMenu.addItem(refresh)
        remoteMenu.addItem(.separator())
        if model.gitRemoteBranches.isEmpty {
            let item = NSMenuItem(title: model.i18n("git.remote.branches.empty", fallback: "No Remote Branches"), action: nil, keyEquivalent: "")
            item.isEnabled = false
            remoteMenu.addItem(item)
        } else {
            for remote in model.gitRemotes {
                let branches = (remoteBranchGroups[remote.name] ?? []).sorted { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }
                let remoteSubmenu = NSMenu(title: remote.name)

                if branches.isEmpty {
                    let item = NSMenuItem(title: model.i18n("git.remote.branches.empty", fallback: "No Remote Branches"), action: nil, keyEquivalent: "")
                    item.isEnabled = false
                    remoteSubmenu.addItem(item)
                } else {
                    for branch in branches {
                        let branchMenu = NSMenu(title: branch)
                        let shortName = branch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).dropFirst().first.map(String.init) ?? branch
                        let checkoutHandler = NativeContextMenuHandler(action: { model.checkoutRemoteGitBranch(branch) })
                        handlers.append(checkoutHandler)
                        let checkout = NSMenuItem(title: model.i18n("git.remote.branch.checkout_local", fallback: "Checkout as Local Branch"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                        checkout.target = checkoutHandler
                        branchMenu.addItem(checkout)

                        let pushHandler = NativeContextMenuHandler(action: { model.pushCurrentLocalBranch(to: branch) })
                        handlers.append(pushHandler)
                        let pushItem = NSMenuItem(title: model.i18n("git.remote.branch.push_here", fallback: "Push to This Branch"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
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
                let otherMenu = NSMenu(title: model.i18n("git.misc.other", fallback: "Other"))
                for branch in ungroupedBranches.sorted(by: { $0.localizedCaseInsensitiveCompare($1) == .orderedAscending }) {
                    let branchMenu = NSMenu(title: branch)
                    let shortName = branch.split(separator: "/", maxSplits: 1, omittingEmptySubsequences: true).dropFirst().first.map(String.init) ?? branch
                    let checkoutHandler = NativeContextMenuHandler(action: { model.checkoutRemoteGitBranch(branch) })
                    handlers.append(checkoutHandler)
                    let checkout = NSMenuItem(title: model.i18n("git.remote.branch.checkout_local", fallback: "Checkout as Local Branch"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                    checkout.target = checkoutHandler
                    branchMenu.addItem(checkout)

                    let pushHandler = NativeContextMenuHandler(action: { model.pushCurrentLocalBranch(to: branch) })
                    handlers.append(pushHandler)
                    let pushItem = NSMenuItem(title: model.i18n("git.remote.branch.push_here", fallback: "Push to This Branch"), action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                    pushItem.target = pushHandler
                    branchMenu.addItem(pushItem)

                    let branchItem = NSMenuItem(title: shortName, action: nil, keyEquivalent: "")
                    branchItem.attributedTitle = remoteBranchAttributedTitle(shortName: shortName, fullName: branch)
                    otherMenu.setSubmenu(branchMenu, for: branchItem)
                    otherMenu.addItem(branchItem)
                }

                let otherItem = NSMenuItem(title: model.i18n("git.misc.other", fallback: "Other"), action: nil, keyEquivalent: "")
                remoteMenu.setSubmenu(otherMenu, for: otherItem)
                remoteMenu.addItem(otherItem)
            }
        }
        let remoteItem = NSMenuItem(title: model.i18n("git.remote.branches", fallback: "Remote Branches"), action: nil, keyEquivalent: "")
        menu.setSubmenu(remoteMenu, for: remoteItem)
        menu.addItem(remoteItem)

        addSeparator()
        addAction(model.i18n("git.remote.fetch", fallback: "Fetch")) { model.fetchGitBranch() }
        addAction(model.i18n("git.remote.pull", fallback: "Pull")) { model.pullGitBranch() }
        addAction(model.i18n("git.remote.push", fallback: "Push")) { model.pushGitBranch() }

        let pushRemoteMenu = NSMenu(title: model.i18n("git.remote.push_to", fallback: "Push To..."))
        if model.gitRemotes.isEmpty {
            let item = NSMenuItem(title: model.i18n("git.remote.empty", fallback: "No Remotes"), action: nil, keyEquivalent: "")
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
        let pushRemoteItem = NSMenuItem(title: model.i18n("git.remote.push_to", fallback: "Push To..."), action: nil, keyEquivalent: "")
        menu.setSubmenu(pushRemoteMenu, for: pushRemoteItem)
        menu.addItem(pushRemoteItem)

        addAction(model.i18n("git.remote.force_push", fallback: "Force Push")) { model.forcePushGitBranch() }
        addSeparator()
        addAction(model.i18n("git.history.undo_last_commit", fallback: "Undo Last Commit")) { model.undoLastGitCommit() }
        addAction(model.i18n("git.history.edit_last_commit_message", fallback: "Edit Last Commit Message")) { model.editLastGitCommitMessage() }
        addSeparator()
        addAction(model.i18n("git.repository.show_in_finder", fallback: "Show Repository in Finder")) { model.revealRepositoryInFinder() }

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
                .help(model.i18n("git.commit.generate_message", fallback: "Generate Commit Message"))

                Button {
                    model.refreshGitState()
                } label: {
                    Image(systemName: model.isGitLoading ? "hourglass" : "arrow.clockwise")
                        .font(.system(size: 13, weight: .semibold))
                }
                .buttonStyle(GitToolbarIconButtonStyle())
                .help(model.i18n("git.status.refresh", fallback: "Refresh Git Status"))
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
    @State private var isComposerHovered = false

    private let composerFont = NSFont.systemFont(ofSize: 14, weight: .medium)
    private let composerHorizontalInset: CGFloat = 14
    private let composerVerticalInset: CGFloat = 10

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            ZStack(alignment: .topLeading) {
                AppMultilineInputArea(
                    text: Binding(
                        get: { model.commitMessage },
                        set: { model.commitMessage = $0 }
                    ),
                    placeholder: model.i18n("git.commit.message.placeholder", fallback: "Enter Commit Message"),
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
                    .onHover { hovering in
                        isComposerHovered = hovering
                    }
            }

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

    var body: some View {
        HStack(spacing: 0) {
            Button(action: onSubmit) {
                Text(commitActionTitle(selectedAction))
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(CommitMainButtonStyle())

            Menu {
                ForEach(GitCommitAction.allCases, id: \.self) { action in
                    Button(commitActionTitle(action)) {
                        selectedAction = action
                    }
                }
            } label: {
                Image(systemName: "chevron.down")
                    .font(.system(size: 10, weight: .semibold))
                    .frame(width: 40, height: 32)
                    .foregroundStyle(AppTheme.textPrimary)
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .buttonStyle(CommitMenuButtonStyle())
        }
        .background(AppTheme.focus)
        .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .stroke(Color(nsColor: .separatorColor).opacity(0.3), lineWidth: 0.5)
        }
        .disabled(isDisabled)
        .opacity(isDisabled ? 0.5 : 1.0)
    }

    private func commitActionTitle(_ action: GitCommitAction) -> String {
        switch action {
        case .commit:
            return model.i18n("git.commit.action", fallback: "Commit")
        case .commitAndPush:
            return model.i18n("git.commit.action_push", fallback: "Commit and Push")
        case .commitAndSync:
            return model.i18n("git.commit.action_sync", fallback: "Commit and Sync")
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
                        title: model.i18n("git.files.staged", fallback: "Staged"),
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
                        title: model.i18n("git.files.changes", fallback: "Changes"),
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
                        title: model.i18n("git.files.untracked", fallback: "Untracked"),
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
    let title: String
    let entries: [GitFileEntry]
    let accent: Color
    @Binding var isExpanded: Bool
    let primaryIcon: String
    let primaryAction: (GitFileEntry) -> Void
    let secondaryIcon: String?
    let secondaryAction: ((GitFileEntry) -> Void)?
    let model: AppModel
    @State private var isHovered = false

    private var selectedEntries: [GitFileEntry] {
        entries.filter { model.isGitEntrySelected($0) }
    }

    private var usesSelectedEntries: Bool {
        model.selectedGitEntryIDs.count > 1 && !selectedEntries.isEmpty
    }

    private var shouldShowHeaderActions: Bool {
        isHovered || usesSelectedEntries
    }

    private var actionEntries: [GitFileEntry] {
        usesSelectedEntries ? selectedEntries : entries
    }

    private var headerActions: [GitSectionHeaderAction] {
        switch title {
        case "Staged":
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "minus", help: usesSelectedEntries ? model.i18n("git.files.unstage_selected", fallback: "Unstage Selected") : model.i18n("git.files.unstage_all", fallback: "Unstage All")) {
                    model.unstageEntries(actionEntries)
                }
            ]
        case "Changes":
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "plus", help: usesSelectedEntries ? model.i18n("git.files.stage_selected", fallback: "Stage Selected") : model.i18n("git.files.stage_all", fallback: "Stage All")) {
                    model.stageEntries(actionEntries)
                },
                GitSectionHeaderAction(icon: "discard", help: usesSelectedEntries ? model.i18n("git.files.discard_selected", fallback: "Discard Selected") : model.i18n("git.files.discard_all", fallback: "Discard All")) {
                    model.discardEntries(actionEntries)
                }
            ]
        case "Untracked":
            guard !actionEntries.isEmpty else { return [] }
            return [
                GitSectionHeaderAction(icon: "plus", help: usesSelectedEntries ? model.i18n("git.files.stage_selected", fallback: "Stage Selected") : model.i18n("git.files.stage_all", fallback: "Stage All")) {
                    model.stageEntries(actionEntries)
                },
                GitSectionHeaderAction(icon: "discard", help: usesSelectedEntries ? model.i18n("git.files.remove_selected", fallback: "Remove Selected") : model.i18n("git.files.remove_all", fallback: "Remove All")) {
                    model.discardEntries(actionEntries)
                }
            ]
        default:
            return []
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
                AppPinnedHeaderBackground()
                    .overlay(Color(nsColor: .shadowColor).opacity(0.06))
            }
            .overlay(alignment: .trailing) {
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
                    .padding(.leading, 12)
                    .padding(.trailing, 14)
                    .frame(height: 34)
                }
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(AppTheme.separator)
                    .frame(height: 1)
            }
            .zIndex(1)
            .onHover { hovering in
                isHovered = hovering
            }
        }
    }

    private var displayTitle: String {
        switch title {
        case "Staged":
            return model.i18n("git.files.staged", fallback: "Staged")
        case "Changes":
            return model.i18n("git.files.changes", fallback: "Changes")
        case "Untracked":
            return model.i18n("git.files.untracked", fallback: "Untracked")
        default:
            return title
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
            let shiftPressed = NSApp.currentEvent?.modifierFlags.contains(.shift) == true
            model.selectGitEntry(entry, extendingRange: shiftPressed)
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

                GitStatusBadge(entry: entry, accent: accent)
            }
            .padding(.leading, 10)
            .padding(.trailing, 14)
            .padding(.vertical, 4)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .background(rowBackground)
        .overlay(alignment: .trailing) {
            if isHovered {
                GitHoverActions(
                    primaryIcon: primaryIcon,
                    primaryAction: primaryAction,
                    secondaryIcon: secondaryIcon,
                    secondaryAction: secondaryAction
                )
                .padding(.trailing, 10)
            }
        }
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
            .overlay(alignment: .trailing) {
                if isHovered {
                    Color.clear.frame(width: 88)
                }
            }
    }

    private var baseRowColor: Color {
        if isSelected {
            return AppTheme.focus.opacity(0.14)
        }

        return isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear
    }
}

private struct GitStatusBadge: View {
    let entry: GitFileEntry
    let accent: Color

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
            .frame(width: 14, alignment: .trailing)
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
                Text(model.i18n("git.history.title", fallback: "Git History"))
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                    .foregroundStyle(AppTheme.textSecondary)
                Spacer()
            }
            .padding(.horizontal, 16)
            .frame(height: 34)
            .background {
                AppPinnedHeaderBackground()
                    .overlay(Color(nsColor: .shadowColor).opacity(0.06))
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(AppTheme.separator)
                    .frame(height: 1)
            }

            if history.isEmpty {
                Text(model.i18n("git.history.empty", fallback: "No Commit History"))
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
                    .help(model.i18n("git.history.revert_commit", fallback: "Revert This Commit"))

                    Button {
                        model.createBranch(from: item)
                    } label: {
                        Image(systemName: "point.topleft.down.curvedto.point.bottomright.up")
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .buttonStyle(GitHistoryActionButtonStyle())
                    .help(model.i18n("git.history.create_branch_from_commit", fallback: "Create Branch from This Commit"))
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

    actions.append(.action(selectedEntries.count > 1 ? model.i18n("git.files.copy_selected_paths", fallback: "Copy Selected Paths") : model.i18n("git.files.copy_path", fallback: "Copy Path")) {
        model.copyGitPaths(selectedEntries)
    })

    actions.append(.action(model.i18n("git.files.show_in_finder", fallback: "Show in Finder")) {
        model.revealGitEntriesInFinder(selectedEntries)
    })

    actions.append(.separator)

    if allStaged {
        actions.append(.action(selectedEntries.count > 1 ? model.i18n("git.files.unstage_selected", fallback: "Unstage Selected") : model.i18n("git.files.unstage", fallback: "Unstage")) {
            model.unstageEntries(selectedEntries)
        })
    } else {
        actions.append(.action(selectedEntries.count > 1 ? model.i18n("git.files.stage_selected", fallback: "Stage Selected") : model.i18n("git.files.stage", fallback: "Stage")) {
            model.stageEntries(selectedEntries)
        })
    }

    if hasNonStaged {
        actions.append(.action(selectedEntries.count > 1 ? model.i18n("git.files.discard_selected_changes", fallback: "Discard Selected Changes") : model.i18n("git.files.discard_changes", fallback: "Discard Changes")) {
            model.discardEntries(selectedEntries)
        })
    }

    if allUntracked {
        actions.append(.separator)

        actions.append(.action(model.i18n("git.ignore.add", fallback: "Add to .gitignore")) {
            model.addGitEntriesToIgnore(selectedEntries)
        })

        actions.append(.action(selectedEntries.count > 1 ? model.i18n("git.files.delete_selected_files", fallback: "Delete Selected Files") : model.i18n("git.files.delete_file", fallback: "Delete File")) {
            model.discardEntries(selectedEntries)
        })
    }

    return actions
}

@MainActor
private func buildGitCommitContextMenu(model: AppModel, commit: GitCommitEntry) -> [NativeContextMenuAction] {
    var actions: [NativeContextMenuAction] = [
        .action(model.i18n("git.history.copy_commit_hash", fallback: "Copy Commit Hash")) { model.copyGitCommitHash(commit) },
        .action(model.i18n("git.history.checkout_commit", fallback: "Checkout This Commit")) { model.checkoutGitCommit(commit) },
        .action(model.i18n("git.history.create_branch_from_commit", fallback: "Create Branch from This Commit")) { model.createBranch(from: commit) },
    ]

    if model.gitHistory.first?.hash == commit.hash {
        actions.append(.separator)
        actions.append(.action(model.i18n("git.history.undo_last_commit", fallback: "Undo Last Commit")) { model.undoLastGitCommit() })
        actions.append(.action(model.i18n("git.history.edit_last_commit_message", fallback: "Edit Last Commit Message")) { model.editLastGitCommitMessage() })
    }

    actions.append(.separator)
    actions.append(.action(model.i18n("git.history.revert_commit", fallback: "Revert This Commit")) { model.revertGitCommit(commit) })
    actions.append(.separator)
    actions.append(.action(model.i18n("git.history.restore_local", fallback: "Restore This Revision Locally")) { model.restoreGitCommit(commit, forceRemote: false) })
    actions.append(.action(model.i18n("git.history.restore_remote", fallback: "Restore This Revision Remotely")) { model.restoreGitCommit(commit, forceRemote: true) })

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
            .contentShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
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
            .frame(height: 32)
            .foregroundStyle(Color.white.opacity(isEnabled ? 0.98 : 0.78))
            .background(AppTheme.focus.opacity(isEnabled ? (configuration.isPressed ? 0.82 : 1.0) : 0.5))
    }
}

private struct CommitMenuButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .foregroundStyle(Color.white.opacity(isEnabled ? 0.96 : 0.76))
            .background(
                ZStack(alignment: .leading) {
                    AppTheme.focus.opacity(isEnabled ? (configuration.isPressed ? 0.82 : 1.0) : 0.5)
                    Rectangle()
                        .fill(Color.white.opacity(isEnabled ? 0.2 : 0.1))
                        .frame(width: 0.5)
                }
            )
    }
}
