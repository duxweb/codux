import AppKit
import SwiftUI

struct AIStatsPanelView: View {
    let model: AppModel
    let store: AIStatsStore
    let currentProject: Project?
    let isAutomaticRefreshInProgress: Bool
    let onRefresh: () -> Void
    let onCancel: () -> Void
    @State private var showsDeferredDetails = false

    private var stateMatchesCurrentProject: Bool {
        guard let currentProject else {
            return true
        }
        guard let summary = store.state.projectSummary else {
            return false
        }
        return summary.projectID == currentProject.id
    }

    var body: some View {
        let _ = store.renderVersion
        let statisticsDisplayMode = model.appSettings.aiStatisticsDisplayMode
        VStack(spacing: 0) {
            AIStatsHeader(model: model)

            if stateMatchesCurrentProject, let summary = store.state.projectSummary {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        AIStatsLiveSessionsCard(model: model, snapshots: store.state.liveSnapshots, displayMode: statisticsDisplayMode)
                        AIStatsSummaryCards(
                            summary: summary,
                            displayMode: statisticsDisplayMode
                        )
                        if showsDeferredDetails {
                            AIStatsTodayUsageBarChart(model: model, buckets: store.state.todayTimeBuckets, displayMode: statisticsDisplayMode)
                            AIStatsHeatmapCard(model: model, days: store.state.heatmap, displayMode: statisticsDisplayMode)
                            AIStatsBreakdownCard(model: model, title: String(localized: "ai.breakdown.tool_ranking", defaultValue: "Tool Ranking", bundle: .module), items: store.state.toolBreakdown, displayMode: statisticsDisplayMode)
                            AIStatsBreakdownCard(model: model, title: String(localized: "ai.breakdown.model_ranking", defaultValue: "Model Ranking", bundle: .module), items: store.state.modelBreakdown, displayMode: statisticsDisplayMode)
                            AIStatsSessionsCard(model: model, sessions: store.state.sessions, displayMode: statisticsDisplayMode)
                        } else {
                            AIStatsDeferredSectionsPlaceholder()
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                }
            } else if case .indexing = store.state.indexingStatus {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        ForEach(0..<5, id: \.self) { index in
                            RoundedRectangle(cornerRadius: 10, style: .continuous)
                                .fill(Color(nsColor: .quaternaryLabelColor).opacity(0.3))
                                .frame(height: [110, 164, 112, 120, 140][index])
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.bottom, 16)
                }
            } else {
                AIStatsEmptyView(model: model)
            }

            if shouldShowIndexingBar {
                AIStatsIndexingBar(
                    model: model,
                    status: effectiveIndexingStatus,
                    isShowingCachedState: store.refreshState.isShowingCached,
                    isAutomaticRefreshInProgress: isAutomaticRefreshInProgress,
                    onRefresh: onRefresh,
                    onCancel: onCancel
                )
            }
        }
        .background(Color.clear)
        .task(id: currentProject?.id) {
            showsDeferredDetails = false
            if !stateMatchesCurrentProject {
                onRefresh()
            }
            await Task.yield()
            try? await Task.sleep(nanoseconds: 160_000_000)
            guard !Task.isCancelled else {
                return
            }
            withAnimation(.easeOut(duration: 0.14)) {
                showsDeferredDetails = true
            }
        }
    }

    private var effectiveIndexingStatus: AIIndexingStatus {
        if stateMatchesCurrentProject {
            return store.state.indexingStatus
        }
        return .indexing(progress: 0.0, detail: String(localized: "ai.state.switching_current_project", defaultValue: "Switching to Current Project", bundle: .module))
    }

    private var shouldShowIndexingBar: Bool {
        true
    }
}

private struct AIStatsDeferredSectionsPlaceholder: View {
    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(spacing: 8) {
                ProgressView()
                    .controlSize(.small)
                Text(String(localized: "ai.panel.loading_details", defaultValue: "Loading project details…", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.secondary)
            }

            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(nsColor: .quaternaryLabelColor).opacity(0.22))
                .frame(height: 76)

            RoundedRectangle(cornerRadius: 10, style: .continuous)
                .fill(Color(nsColor: .quaternaryLabelColor).opacity(0.18))
                .frame(height: 110)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

private struct AIStatsEmptyView: View {
    let model: AppModel

    var body: some View {
        VStack(spacing: 16) {
            Image(systemName: "chart.bar.xaxis")
                .font(.system(size: 30, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)

            VStack(spacing: 6) {
                Text(String(localized: "ai.empty.no_stats", defaultValue: "No AI Stats Yet", bundle: .module))
                    .font(.system(size: 16, weight: .bold))
                    .foregroundStyle(AppTheme.textPrimary)

                Text(String(localized: "ai.empty.description", defaultValue: "There are no AI tool usage records yet in this project's workspace terminals.", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .multilineTextAlignment(.center)
                    .frame(maxWidth: 280)
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct AIStatsHeader: View {
    let model: AppModel

    var body: some View {
        HStack {
            Text(String(localized: "ai.panel.title", defaultValue: "AI Assistant", bundle: .module))
                .font(.system(size: 14, weight: .bold))
                .foregroundStyle(AppTheme.textPrimary)

            Spacer()
        }
        .padding(.horizontal, 16)
        .padding(.top, 14)
        .padding(.bottom, 14)
    }
}
