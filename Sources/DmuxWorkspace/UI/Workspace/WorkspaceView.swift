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

