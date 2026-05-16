import AppKit
import SwiftUI

struct WorkspaceView: View {
    let model: AppModel

    var body: some View {
        Group {
            if let workspace = model.selectedWorkspace {
                WorkspaceTabbedContentView(model: model, workspace: workspace)
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
        .onChange(of: model.selectedWorktreeID) { _, _ in
            model.noteWorkspaceViewAppeared()
        }
    }
}

private struct WorkspaceTabbedContentView: View {
    let model: AppModel
    let workspace: ProjectWorkspace

    private var viewMode: WorkspacePrimaryViewMode {
        model.workspacePrimaryViewMode(for: workspace.projectID)
    }

    private var isTerminalSelected: Bool {
        viewMode == .terminal
    }

    private var isFilesSelected: Bool {
        viewMode == .files
    }

    private var isReviewSelected: Bool {
        viewMode == .review
    }

    var body: some View {
        ZStack {
            WorkspaceProjectView(model: model, workspace: workspace, isVisible: isTerminalSelected)
                .opacity(isTerminalSelected ? 1 : 0)
                .allowsHitTesting(isTerminalSelected)

            WorkspaceFilesContentView(model: model, workspaceID: workspace.projectID)
                .opacity(isFilesSelected ? 1 : 0)
                .allowsHitTesting(isFilesSelected)

            WorktreeReviewPanelView(model: model)
                .opacity(isReviewSelected ? 1 : 0)
                .allowsHitTesting(isReviewSelected)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(model.terminalChromeColor)
    }
}

private struct WorkspaceFilesContentView: View {
    let model: AppModel
    let workspaceID: UUID

    private var selectedFileTab: WorkspaceFileTab? {
        model.selectedWorkspaceFileTab(for: workspaceID)
    }

    private var tabs: [WorkspaceFileTab] {
        model.workspaceFileTabs(for: workspaceID)
    }

    var body: some View {
        VStack(spacing: 0) {
            if tabs.isEmpty == false {
                WorkspaceFileTabBar(model: model, workspaceID: workspaceID, tabs: tabs)
            }

            Group {
                if tabs.isEmpty == false {
                    ZStack {
                        ForEach(tabs) { tab in
                            let isSelected = selectedFileTab?.id == tab.id
                            WorkspaceFileEditorView(model: model, tab: tab, isSelected: isSelected)
                                .id(tab.id)
                                .opacity(isSelected ? 1 : 0)
                                .allowsHitTesting(isSelected)
                        }
                    }
                    .clipped()
                } else {
                    WorkspaceFilesEmptyStateView(model: model)
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .clipped()
        }
        .background(model.terminalChromeColor)
    }
}

private struct WorkspaceFileTabBar: View {
    let model: AppModel
    let workspaceID: UUID
    let tabs: [WorkspaceFileTab]

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: 6) {
                ForEach(tabs) { tab in
                    WorkspaceFileTabPill(
                        title: tab.title,
                        symbol: "doc.text",
                        isSelected: isSelected(tab),
                        isDirty: model.isWorkspaceFileTabDirty(tab.id),
                        close: { model.closeWorkspaceFileTab(tab) },
                        action: { model.selectWorkspaceFileTab(tab.id) }
                    )
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 6)
        }
        .frame(height: 40)
        .background(tabBarBackground)
        .overlay(alignment: .bottom) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.35))
                .frame(height: 1)
        }
    }

    private func isSelected(_ tab: WorkspaceFileTab) -> Bool {
        if case .file(let tabID) = model.workspaceContentSelection(for: workspaceID) {
            return tabID == tab.id
        }
        return false
    }

    private var tabBarBackground: Color {
        model.terminalUsesLightBackground ? Color.black.opacity(0.035) : Color.white.opacity(0.035)
    }
}

private struct WorkspaceFileTabPill: View {
    let title: String
    let symbol: String
    let isSelected: Bool
    let isDirty: Bool
    let close: (() -> Void)?
    let action: () -> Void

    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            HStack(spacing: 7) {
                Image(systemName: symbol)
                    .font(.system(size: 11, weight: .semibold))
                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .lineLimit(1)
                    .truncationMode(.middle)
                if isDirty {
                    Circle()
                        .fill(AppTheme.warning)
                        .frame(width: 6, height: 6)
                }
                if let close {
                    Button(action: close) {
                        Image(systemName: "xmark")
                            .font(.system(size: 8, weight: .bold))
                            .frame(width: 14, height: 14)
                    }
                    .buttonStyle(.plain)
                    .opacity(isHovered || isSelected ? 1 : 0.36)
                }
            }
            .foregroundStyle(isSelected ? AppTheme.textPrimary : AppTheme.textSecondary)
            .padding(.horizontal, 10)
            .frame(minWidth: 84, maxWidth: 210, minHeight: 28)
            .background(backgroundColor)
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(isSelected ? AppTheme.focus.opacity(0.35) : Color.clear, lineWidth: 1)
            }
            .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var backgroundColor: Color {
        if isSelected {
            return AppTheme.focus.opacity(0.16)
        }
        if isHovered {
            return Color(nsColor: .quaternarySystemFill).opacity(0.88)
        }
        return Color.clear
    }
}

private struct WorkspaceFilesEmptyStateView: View {
    let model: AppModel

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: "doc.text.magnifyingglass")
                .font(.system(size: 30, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)

            Text(String(localized: "workspace.files.empty.title", defaultValue: "No File Open", bundle: .module))
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AppTheme.textSecondary)

            Button {
                model.toggleRightPanel(.files)
            } label: {
                Label(String(localized: "workspace.files.empty.open_files", defaultValue: "Open Files", bundle: .module), systemImage: "folder")
                    .font(.system(size: 12, weight: .semibold))
                    .padding(.horizontal, 10)
                    .frame(height: 28)
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.textPrimary)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color(nsColor: .quaternarySystemFill).opacity(0.85))
            )
            .overlay {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(Color(nsColor: .separatorColor).opacity(0.38), lineWidth: 0.5)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(model.terminalChromeColor)
    }
}

private struct WorkspaceProjectView: View {
    let model: AppModel
    let workspace: ProjectWorkspace
    let isVisible: Bool

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
            hasBottomRegion: true,
            bottomHeight: workspace.hasBottomTabs ? workspace.bottomPaneHeight : BottomTabbedPaneView.statusBarHeight,
            isVisible: isVisible,
            top: {
                TopPaneRowView(
                    model: model,
                    workspace: workspace,
                    activeTerminalSessionID: activeTerminalSessionID,
                    showsInactiveOverlay: hasMultipleVisibleTerminalPanes,
                    isVisible: isVisible
                )
                    .frame(minHeight: 220, maxHeight: .infinity)
            },
            bottom: {
                AnyView(
                    Group {
                        WorkspaceBottomRegion(
                            model: model,
                            workspace: workspace,
                            activeTerminalSessionID: activeTerminalSessionID,
                            showsInactiveOverlay: hasMultipleVisibleTerminalPanes,
                            isVisible: isVisible
                        )
                        .frame(
                            minHeight: workspace.hasBottomTabs ? ProjectWorkspace.minimumBottomPaneHeight : BottomTabbedPaneView.statusBarHeight,
                            idealHeight: workspace.hasBottomTabs ? workspace.bottomPaneHeight : BottomTabbedPaneView.statusBarHeight,
                            maxHeight: .infinity
                        )
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
    let isVisible: Bool

    var body: some View {
        BottomTabbedPaneView(
            model: model,
            workspace: workspace,
            activeTerminalSessionID: activeTerminalSessionID,
            showsInactiveOverlay: showsInactiveOverlay,
            isVisible: isVisible
        )
        .clipped()
    }
}
