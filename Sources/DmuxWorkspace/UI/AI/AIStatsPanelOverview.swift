import AppKit
import SwiftUI

struct AIStatsSummaryCards: View {
    let summary: AIProjectUsageSummary
    let displayMode: AppAIStatisticsDisplayMode

    var body: some View {
        HStack(spacing: 10) {
            metricCard(
                title: String(localized: "ai.summary.current_project", defaultValue: "Current Project", bundle: .module),
                value: aiStatsFormatCompactToken(summary.displayedProjectTotalTokens(mode: displayMode)),
                accent: AppTheme.focus
            )
            metricCard(
                title: String(localized: "ai.summary.today_total", defaultValue: "Today's Total", bundle: .module),
                value: aiStatsFormatCompactToken(displayedTodayTotalTokens),
                accent: AppTheme.success
            )
        }
    }

    private var displayedTodayTotalTokens: Int {
        summary.displayedTodayTotalTokens(mode: displayMode)
    }

    private func metricCard(title: String, value: String, accent: Color) -> some View {
        VStack(alignment: .leading, spacing: 5) {
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)

            Text(value)
                .font(.system(size: 16, weight: .bold, design: .rounded))
                .foregroundStyle(.primary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(alignment: .top) {
            UnevenRoundedRectangle(
                cornerRadii: .init(topLeading: 10, bottomLeading: 0, bottomTrailing: 0, topTrailing: 10),
                style: .continuous
            )
            .fill(accent)
            .frame(height: 2)
        }
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

struct AIStatsLiveSessionsCard: View {
    let model: AppModel
    let snapshots: [AITerminalSessionSnapshot]
    let displayMode: AppAIStatisticsDisplayMode

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(String(localized: "ai.live_sessions", defaultValue: "Live Sessions", bundle: .module))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)

            if snapshots.isEmpty {
                Text(String(localized: "ai.live_sessions.empty", defaultValue: "There are no active AI sessions right now", bundle: .module))
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(.tertiary)
                    .frame(maxWidth: .infinity, minHeight: 48, alignment: .center)
            } else {
                ForEach(snapshots) { snapshot in
                    HStack(alignment: .top, spacing: 12) {
                        VStack(alignment: .leading, spacing: 4) {
                            Text(snapshot.tool ?? "-")
                                .font(.system(size: 14, weight: .semibold))
                                .foregroundStyle(.primary)
                                .lineLimit(1)
                            Text(snapshot.model ?? "-")
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                                .truncationMode(.tail)
                                .help(snapshot.sessionTitle)
                        }

                        Spacer(minLength: 8)

                        VStack(alignment: .trailing, spacing: 4) {
                            Text(aiStatsFormatLiveTokenValue(displayTokens(for: snapshot)))
                                .font(.system(size: 16, weight: .bold, design: .rounded))
                                .foregroundStyle(.primary)
                                .lineLimit(1)
                            Text(String(localized: "ai.metric.token", defaultValue: "Token", bundle: .module))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.tertiary)
                                .lineLimit(1)
                        }
                    }
                    .padding(10)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(Color(nsColor: .quaternarySystemFill), in: RoundedRectangle(cornerRadius: 8, style: .continuous))
                }
            }
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func displayTokens(for snapshot: AITerminalSessionSnapshot) -> Int {
        max(0, snapshot.displayedCurrentTotalTokens(mode: displayMode))
    }
}
