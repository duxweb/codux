import AppKit
import SwiftUI

struct GitPanelHeader: View {
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
