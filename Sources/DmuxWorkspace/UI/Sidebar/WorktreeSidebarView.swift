import AppKit
import SwiftUI

private let worktreeStatusOrder: [ProjectWorktreeTaskStatus] = [
    .running, .waiting, .ready, .review, .blocked, .done, .merged, .todo, .archived
]

private let worktreeActiveIndicatorColor = Color.orange

private enum WorktreeSidebarMetrics {
    static let taskRowHeight: CGFloat = 52
    static let statusColumnWidth: CGFloat = 16
    static let statusColumnHeight: CGFloat = 20
}

private struct WorktreeActiveIndicator: View {
    var body: some View {
        ZStack {
            Circle()
                .stroke(worktreeActiveIndicatorColor.opacity(0.28), lineWidth: 2)
                .frame(width: 10, height: 10)
            Circle()
                .fill(worktreeActiveIndicatorColor)
                .frame(width: 6, height: 6)
        }
        .frame(width: 12, height: 12)
    }
}

struct WorktreeSidebarView: View {
    let model: AppModel

    private var defaultWorktree: ProjectWorktree? {
        model.selectedProjectWorktrees.first { $0.isDefault }
    }

    private var taskWorktrees: [ProjectWorktree] {
        model.selectedProjectWorktrees
            .filter { !$0.isDefault }
            .sorted { lhs, rhs in
                let lhsStatus = model.effectiveWorktreeTaskStatus(for: lhs)
                let rhsStatus = model.effectiveWorktreeTaskStatus(for: rhs)
                let lhsRank = statusRank(lhsStatus)
                let rhsRank = statusRank(rhsStatus)
                if lhsRank != rhsRank {
                    return lhsRank < rhsRank
                }
                if lhs.updatedAt != rhs.updatedAt {
                    return lhs.updatedAt > rhs.updatedAt
                }
                return lhs.name.localizedStandardCompare(rhs.name) == .orderedAscending
            }
    }

    private func statusRank(_ status: ProjectWorktreeTaskStatus) -> Int {
        worktreeStatusOrder.firstIndex(of: status.visibleStatus) ?? worktreeStatusOrder.count
    }

    private var taskSectionTitle: String {
        String(localized: "worktree.sidebar.subtasks", defaultValue: "Subtasks", bundle: .module)
    }

    var body: some View {
        VStack(spacing: 0) {
            header
            Divider()
                .opacity(0.4)
                .padding(.horizontal, 12)

            if let defaultWorktree {
                VStack(alignment: .leading, spacing: 6) {
                    sectionLabel(String(localized: "worktree.sidebar.main_task", defaultValue: "Main Task", bundle: .module))
                    BaseWorkspaceCard(
                        model: model,
                        worktree: defaultWorktree,
                        isSelected: defaultWorktree.id == model.selectedWorktreeID
                    )
                }
                .padding(.horizontal, 10)
                .padding(.top, 12)
                .padding(.bottom, 10)

                taskSectionDivider
            }

            ScrollView {
                LazyVStack(spacing: 2) {
                    ForEach(taskWorktrees) { worktree in
                        WorktreeRowCompact(
                            model: model,
                            worktree: worktree,
                            isSelected: worktree.id == model.selectedWorktreeID
                        )
                    }
                }
                .padding(.horizontal, 8)
                .padding(.top, 4)
                .padding(.bottom, 10)
            }
        }
        .background(worktreeSidebarBackground)
        .onAppear {
            model.refreshWorktreeGitSummaries()
        }
        .onChange(of: model.selectedProjectID) { _, _ in
            model.refreshWorktreeGitSummaries()
        }
        .onChange(of: model.selectedWorktreeID) { _, _ in
            model.refreshWorktreeGitSummaries()
        }
        .onChange(of: model.worktrees) { _, _ in
            model.refreshWorktreeGitSummaries()
        }
        .overlay(alignment: .trailing) {
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.38))
                .frame(width: 1)
        }
    }

    private var header: some View {
        HStack(spacing: 8) {
            Text(String(localized: "worktree.sidebar.title", defaultValue: "Tasks", bundle: .module))
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
            Spacer(minLength: 0)
            Button {
                if let projectID = model.selectedProjectID {
                    model.createWorktree(for: projectID)
                }
            } label: {
                Image(systemName: "plus")
                    .font(.system(size: 11, weight: .bold))
                    .frame(width: 24, height: 24)
            }
            .buttonStyle(.plain)
            .appCursor(.pointingHand)
            .help(String(localized: "worktree.create.title", defaultValue: "New Worktree", bundle: .module))
        }
        .padding(.horizontal, 13)
        .frame(height: 42)
    }

    private var worktreeSidebarBackground: Color {
        model.terminalUsesLightBackground
            ? Color.black.opacity(0.025)
            : Color.white.opacity(0.025)
    }

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 10, weight: .semibold))
            .foregroundStyle(AppTheme.textMuted)
            .textCase(.uppercase)
            .tracking(0.6)
            .padding(.horizontal, 4)
    }

    private var taskSectionDivider: some View {
        HStack(spacing: 8) {
            Text(taskSectionTitle)
                .font(.system(size: 10, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)
                .textCase(.uppercase)
                .tracking(0.6)
            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(0.4))
                .frame(height: 1)
        }
        .padding(.horizontal, 14)
        .padding(.bottom, 6)
    }
}

private struct BaseWorkspaceCard: View {
    let model: AppModel
    let worktree: ProjectWorktree
    let isSelected: Bool

    @State private var isHovered = false

    private var summary: ProjectWorktreeGitSummary {
        model.worktreeGitSummary(worktree)
    }

    private var branchText: String {
        if worktree.branch.isEmpty {
            return String(localized: "worktree.branch.current", defaultValue: "current branch", bundle: .module)
        }
        return worktree.branch
    }

    private var mainTaskTitle: String {
        String(localized: "worktree.sidebar.main_task", defaultValue: "Main Task", bundle: .module)
    }

    private var isAgentActive: Bool {
        model.isWorktreeAIActive(for: worktree)
    }

    var body: some View {
        Button {
            model.selectWorktree(worktree.id)
        } label: {
            HStack(alignment: .center, spacing: 10) {
                statusIndicator
                    .frame(
                        width: WorktreeSidebarMetrics.statusColumnWidth,
                        height: WorktreeSidebarMetrics.statusColumnHeight
                    )
                VStack(alignment: .leading, spacing: 3) {
                    Text(mainTaskTitle)
                        .font(.system(size: 13, weight: .semibold, design: .rounded))
                        .foregroundStyle(AppTheme.textPrimary)
                        .lineLimit(1)
                    HStack(spacing: 8) {
                        Label(branchText, systemImage: "arrow.triangle.branch")
                            .lineLimit(1)
                        if summary.changes > 0 {
                            Text("Δ\(summary.changes)")
                        }
                        if summary.incoming > 0 {
                            Text("↓\(summary.incoming)")
                        }
                        if summary.outgoing > 0 {
                            Text("↑\(summary.outgoing)")
                        }
                    }
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 8)
            .frame(height: WorktreeSidebarMetrics.taskRowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(cardBackground)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(cardBorder, lineWidth: 0.5)
            )
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button(String(localized: "worktree.action.open_terminal", defaultValue: "Open Terminal", bundle: .module)) {
                model.selectWorktree(worktree.id)
                model.selectWorkspaceTerminal()
            }
            Button(String(localized: "sidebar.project.open_folder", defaultValue: "Open Folder", bundle: .module)) {
                model.openWorktreeDirectory(worktree.id)
            }
            Button(String(localized: "worktree.menu.review", defaultValue: "Review", bundle: .module)) {
                model.openWorktreeReview(worktree.id)
            }
            Menu(String(localized: "open.ide", defaultValue: "Open in IDE", bundle: .module)) {
                ForEach(ProjectOpenApplication.ideApplications) { application in
                    Button(application.localizedOpenTitle) {
                        model.selectWorktree(worktree.id)
                        model.openSelectedProject(in: application)
                    }
                }
            }
        }
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var cardBackground: Color {
        if isSelected {
            return AppTheme.focus.opacity(model.terminalUsesLightBackground ? 0.12 : 0.18)
        }
        if isHovered {
            return Color.primary.opacity(0.06)
        }
        return Color.primary.opacity(model.terminalUsesLightBackground ? 0.035 : 0.05)
    }

    private var cardBorder: Color {
        if isSelected {
            return AppTheme.focus.opacity(0.45)
        }
        return Color(nsColor: .separatorColor).opacity(0.35)
    }

    @ViewBuilder
    private var statusIndicator: some View {
        if isAgentActive {
            WorktreeActiveIndicator()
        } else {
            Circle()
                .fill(AppTheme.focus)
                .frame(width: 10, height: 10)
        }
    }
}

private struct WorktreeRowCompact: View {
    let model: AppModel
    let worktree: ProjectWorktree
    let isSelected: Bool

    @State private var isHovered = false

    private var task: WorktreeTask? {
        model.worktreeTask(worktree.id)
    }

    private var effectiveStatus: ProjectWorktreeTaskStatus {
        model.effectiveWorktreeTaskStatus(for: worktree)
    }

    private var isAgentActive: Bool {
        model.isWorktreeAIActive(for: worktree)
    }

    private var summary: ProjectWorktreeGitSummary {
        model.worktreeGitSummary(worktree)
    }

    private var taskTitleText: String {
        let title = (task?.title ?? worktree.name).trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? worktree.name : title
    }

    var body: some View {
        Button {
            model.selectWorktree(worktree.id)
        } label: {
            HStack(alignment: .center, spacing: 10) {
                statusIndicator
                    .frame(
                        width: WorktreeSidebarMetrics.statusColumnWidth,
                        height: WorktreeSidebarMetrics.statusColumnHeight
                    )

                VStack(alignment: .leading, spacing: 4) {
                    Text(taskTitleText)
                        .font(.system(size: 13, weight: .medium, design: .rounded))
                        .foregroundStyle(isSelected ? AppTheme.textPrimary : AppTheme.textSecondary)
                        .lineLimit(1)

                    HStack(spacing: 6) {
                        Label(branchText, systemImage: "arrow.triangle.branch")
                            .lineLimit(1)
                        if summary.changes > 0 {
                            Text("Δ\(summary.changes)")
                        }
                        if summary.incoming > 0 {
                            Text("↓\(summary.incoming)")
                        }
                        if summary.outgoing > 0 {
                            Text("↑\(summary.outgoing)")
                        }
                        Spacer(minLength: 0)
                    }
                    .font(.system(size: 10, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                }
                Spacer(minLength: 0)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 6)
            .frame(height: WorktreeSidebarMetrics.taskRowHeight)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(
                ZStack(alignment: .trailing) {
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(statusVisual.background)
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(rowInteractionBackground)
                    Text(statusVisual.watermark)
                        .font(.system(size: 32, weight: .black, design: .rounded))
                        .foregroundStyle(statusVisual.color.opacity(statusVisual.watermarkOpacity))
                        .lineLimit(1)
                        .padding(.trailing, 8)
                        .accessibilityHidden(true)
                        .allowsHitTesting(false)
                }
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
            )
            .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        .buttonStyle(.plain)
        .contextMenu {
            Button(String(localized: "worktree.action.open_terminal", defaultValue: "Open Terminal", bundle: .module)) {
                model.selectWorktree(worktree.id)
                model.selectWorkspaceTerminal()
            }
            Button(String(localized: "sidebar.project.open_folder", defaultValue: "Open Folder", bundle: .module)) {
                model.openWorktreeDirectory(worktree.id)
            }
            Button(String(localized: "worktree.menu.review", defaultValue: "Review", bundle: .module)) {
                model.openWorktreeReview(worktree.id)
            }
            Menu(String(localized: "open.ide", defaultValue: "Open in IDE", bundle: .module)) {
                ForEach(ProjectOpenApplication.ideApplications) { application in
                    Button(application.localizedOpenTitle) {
                        model.selectWorktree(worktree.id)
                        model.openSelectedProject(in: application)
                    }
                }
            }
            Divider()
            Button(String(localized: "worktree.menu.remove", defaultValue: "Remove", bundle: .module), role: .destructive) {
                model.removeWorktree(worktree.id)
            }
            .disabled(worktree.isDefault)
        }
        .onHover { hovering in
            isHovered = hovering
        }
    }

    private var branchText: String {
        if worktree.branch.isEmpty {
            return String(localized: "worktree.branch.current", defaultValue: "current branch", bundle: .module)
        }
        return worktree.branch
    }

    private var rowInteractionBackground: Color {
        if isSelected {
            return AppTheme.focus.opacity(model.terminalUsesLightBackground ? 0.08 : 0.12)
        }
        if isHovered {
            return Color.primary.opacity(0.05)
        }
        return Color.clear
    }

    private var statusVisual: StatusVisual {
        switch effectiveStatus.visibleStatus {
        case .todo:
            return StatusVisual(
                background: .clear,
                color: AppTheme.textMuted.opacity(0.72),
                watermark: "T",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.035 : 0.045
            )
        case .planning, .running, .waiting:
            return StatusVisual(
                background: AppTheme.warning.opacity(model.terminalUsesLightBackground ? 0.10 : 0.14),
                color: AppTheme.warning,
                watermark: effectiveStatus.visibleStatus == .waiting ? "W" : "R",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.095 : 0.12
            )
        case .ready:
            return StatusVisual(
                background: AppTheme.warning.opacity(model.terminalUsesLightBackground ? 0.07 : 0.10),
                color: AppTheme.warning,
                watermark: "Y",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.065 : 0.085
            )
        case .review:
            return StatusVisual(
                background: Color.blue.opacity(model.terminalUsesLightBackground ? 0.06 : 0.10),
                color: .blue,
                watermark: "V",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.065 : 0.085
            )
        case .blocked:
            return StatusVisual(
                background: Color.red.opacity(model.terminalUsesLightBackground ? 0.07 : 0.11),
                color: .red,
                watermark: "!",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.095 : 0.12
            )
        case .done:
            return StatusVisual(
                background: Color.blue.opacity(model.terminalUsesLightBackground ? 0.05 : 0.08),
                color: .blue,
                watermark: "P",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.055 : 0.075
            )
        case .merged:
            return StatusVisual(
                background: AppTheme.success.opacity(model.terminalUsesLightBackground ? 0.07 : 0.10),
                color: AppTheme.success,
                watermark: "M",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.065 : 0.085
            )
        case .archived:
            return StatusVisual(
                background: AppTheme.textMuted.opacity(model.terminalUsesLightBackground ? 0.035 : 0.055),
                color: AppTheme.textMuted.opacity(0.45),
                watermark: "A",
                watermarkOpacity: model.terminalUsesLightBackground ? 0.065 : 0.085
            )
        }
    }

    @ViewBuilder
    private var statusIndicator: some View {
        if isAgentActive {
            WorktreeActiveIndicator()
        } else {
            Circle()
                .fill(statusVisual.color)
                .frame(width: 10, height: 10)
        }
    }

}

private struct StatusVisual {
    let background: Color
    let color: Color
    let watermark: String
    let watermarkOpacity: Double
}
