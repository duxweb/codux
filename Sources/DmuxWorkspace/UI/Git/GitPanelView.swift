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

                    GitHistoryRegion(
                        model: model,
                        history: gitStore.panelState.gitHistory,
                        clearFocus: {
                            focusedField = nil
                            NSApp.keyWindow?.makeFirstResponder(nil)
                        }
                    )
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
                Text(
                    isCheckingRepository
                        ? String(localized: "git.empty.reading_status", defaultValue: "Reading Git Status", bundle: .module)
                        : String(localized: "git.empty.not_repository", defaultValue: "Current Directory Is Not a Git Repository", bundle: .module)
                )
                .font(.system(size: 16, weight: .bold))
                .foregroundStyle(AppTheme.textPrimary)

                Text(
                    isCheckingRepository
                        ? String(localized: "git.empty.loading_description", defaultValue: "Keep the current sidebar layout while repository status syncs in the background.", bundle: .module)
                        : String(localized: "git.empty.description", defaultValue: "Initialize a repository or clone a remote repository to view commits, diffs, and branches here.", bundle: .module)
                )
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
                    badge: gitStore.panelState.gitRemoteSyncState.hasUpstream && gitStore.panelState.gitRemoteSyncState.incomingCount > 0 ? gitStore.panelState.gitRemoteSyncState.incomingCount : nil,
                    action: model.pullGitBranch
                )

                remoteButton(
                    operation: .push,
                    title: String(localized: "git.remote.push", defaultValue: "Push", bundle: .module),
                    help: pushHelp,
                    systemImage: "arrow.up",
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
                        .background {
                            Capsule()
                                .fill(Color.white)
                        }
                        .overlay {
                            Capsule()
                                .stroke(Color.black.opacity(0.08), lineWidth: 0.5)
                        }
                }
            }
            .foregroundStyle(Color.white.opacity(model.gitRemoteSyncState.hasUpstream ? 0.96 : 0.72))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .contentShape(Rectangle())
            .background {
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(Color.white.opacity(hoveredAction == operation ? 0.22 : 0.001))
            }
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
