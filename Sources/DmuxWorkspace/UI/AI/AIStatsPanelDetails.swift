import AppKit
import SwiftUI

struct AIStatsBreakdownCard: View {
    let model: AppModel
    let title: String
    let items: [AIUsageBreakdownItem]
    let displayMode: AppAIStatisticsDisplayMode

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(title)
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)

            if items.isEmpty {
                Text(String(localized: "common.no_data", defaultValue: "No Data", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.tertiary)
            } else {
                let sortedItems = items.sorted {
                    $0.displayedTotalTokens(mode: displayMode) > $1.displayedTotalTokens(mode: displayMode)
                }
                let total = max(sortedItems.reduce(0) { $0 + $1.displayedTotalTokens(mode: displayMode) }, 1)
                VStack(spacing: 0) {
                    ForEach(Array(sortedItems.enumerated()), id: \.element.id) { _, item in
                        let ratio = Double(item.displayedTotalTokens(mode: displayMode)) / Double(total)

                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(item.key)
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundStyle(.primary)
                                    .lineLimit(1)

                                Spacer()

                                Text(aiStatsFormatCompactToken(item.displayedTotalTokens(mode: displayMode)))
                                    .font(.system(size: 13, weight: .bold, design: .rounded))
                                    .foregroundStyle(.secondary)

                                Text(String(format: "%.0f%%", ratio * 100))
                                    .font(.system(size: 11, weight: .medium, design: .rounded))
                                    .foregroundStyle(.tertiary)
                                    .frame(width: 32, alignment: .trailing)
                            }

                            GeometryReader { proxy in
                                ZStack(alignment: .leading) {
                                    RoundedRectangle(cornerRadius: 2, style: .continuous)
                                        .fill(Color(nsColor: .quaternarySystemFill))
                                    RoundedRectangle(cornerRadius: 2, style: .continuous)
                                        .fill(AppTheme.focus.opacity(0.65))
                                        .frame(width: max(2, proxy.size.width * ratio))
                                }
                            }
                            .frame(height: 4)
                        }
                        .padding(.vertical, 6)
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

struct AIStatsSessionsCard: View {
    let model: AppModel
    let sessions: [AISessionSummary]
    let displayMode: AppAIStatisticsDisplayMode
    @State private var selectedSessionID: UUID?
    private let maxVisibleSessions = 20

    private var visibleSessions: [AISessionSummary] {
        Array(sessions.prefix(maxVisibleSessions))
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Text(String(localized: "ai.sessions.history", defaultValue: "Session History", bundle: .module))
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.secondary)

                if sessions.count > maxVisibleSessions {
                    Text(
                        String(
                            format: String(localized: "ai.sessions.recent_limit_format", defaultValue: "Recent %d", bundle: .module),
                            maxVisibleSessions
                        )
                    )
                    .font(.system(size: 11, weight: .medium))
                    .foregroundStyle(.tertiary)
                }
            }

            if sessions.isEmpty {
                Text(String(localized: "ai.sessions.empty", defaultValue: "No Session History", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(.tertiary)
            } else {
                LazyVStack(spacing: 0) {
                    ForEach(Array(visibleSessions.enumerated()), id: \.element.id) { index, session in
                        let capabilities = model.aiSessionCapabilities(for: session)
                        let isSelected = selectedSessionID == session.sessionID

                        HStack(alignment: .top) {
                            VStack(alignment: .leading, spacing: 4) {
                                Text(session.sessionTitle)
                                    .font(.system(size: 13, weight: .semibold))
                                    .foregroundStyle(.primary)
                                    .lineLimit(1)
                                VStack(alignment: .leading, spacing: 2) {
                                    Text(session.lastTool ?? "-")
                                        .font(.system(size: 12, weight: .medium))
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                    Text(session.lastModel ?? "-")
                                        .font(.system(size: 11, weight: .medium))
                                        .foregroundStyle(.tertiary)
                                        .lineLimit(1)
                                }
                            }
                            Spacer()
                            VStack(alignment: .trailing, spacing: 4) {
                                Text(sessionTimeLabel(session.lastSeenAt))
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundStyle(.secondary)
                                    .lineLimit(1)
                                Text(aiStatsFormatCompactToken(session.displayedTotalTokens(mode: displayMode)))
                                    .font(.system(size: 14, weight: .medium, design: .rounded))
                                    .foregroundStyle(.primary)
                                Text(String(format: String(localized: "common.today_format", defaultValue: "Today %@", bundle: .module), aiStatsFormatCompactToken(session.displayedTodayTokens(mode: displayMode))))
                                    .font(.system(size: 11, weight: .medium))
                                    .foregroundStyle(.tertiary)
                            }
                        }
                        .padding(.horizontal, 10)
                        .padding(.vertical, 8)
                        .frame(maxWidth: .infinity, alignment: .leading)
                        .background {
                            if isSelected {
                                RoundedRectangle(cornerRadius: 6, style: .continuous)
                                    .fill(AppTheme.focus.opacity(0.12))
                                    .overlay(alignment: .leading) {
                                        UnevenRoundedRectangle(
                                            cornerRadii: .init(topLeading: 6, bottomLeading: 6, bottomTrailing: 0, topTrailing: 0),
                                            style: .continuous
                                        )
                                        .fill(AppTheme.focus)
                                        .frame(width: 2)
                                    }
                            }
                        }
                        .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
                        .onTapGesture {
                            selectedSessionID = session.sessionID
                        }
                        .contextMenu {
                            Button(String(localized: "ai.session.open.title", defaultValue: "Open Session", bundle: .module)) {
                                model.openAISession(session)
                            }
                            .disabled(!capabilities.canOpen)

                            Button(String(localized: "ai.session.rename.title", defaultValue: "Rename Session", bundle: .module)) {
                                model.renameAISession(session)
                            }
                            .disabled(!capabilities.canRename)

                            Divider()

                            Button(String(localized: "ai.session.remove.title", defaultValue: "Remove Session", bundle: .module), role: .destructive) {
                                model.removeAISession(session)
                            }
                            .disabled(!capabilities.canRemove)
                        }

                        if index < visibleSessions.count - 1 {
                            Rectangle()
                                .fill(Color(nsColor: .separatorColor).opacity(0.4))
                                .frame(height: 0.5)
                                .padding(.horizontal, 6)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(14)
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private func sessionTimeLabel(_ date: Date) -> String {
        guard date.timeIntervalSince1970 > 0 else {
            return "-"
        }
        return String(format: String(localized: "common.last_format", defaultValue: "Last %@", bundle: .module), relativeSessionTime(date))
    }

    private func relativeSessionTime(_ date: Date) -> String {
        let formatter = RelativeDateTimeFormatter()
        formatter.unitsStyle = .short
        formatter.locale = aiStatsLocale(for: model.displayLanguage)
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

struct AIStatsIndexingBar: View {
    let model: AppModel
    let status: AIIndexingStatus
    let isShowingCachedState: Bool
    let isAutomaticRefreshInProgress: Bool
    let onRefresh: () -> Void
    let onCancel: () -> Void
    @State private var hoveredAction: AIStatsIndexingAction?

    private enum AIStatsIndexingAction {
        case refresh
        case cancel
    }

    private var canRetry: Bool {
        switch status {
        case .cancelled, .failed:
            return true
        default:
            return false
        }
    }

    private var isRunning: Bool {
        if case .indexing = status {
            return true
        }
        return false
    }

    private var isManualRunning: Bool {
        isRunning && !isAutomaticRefreshInProgress
    }

    private var shouldShowRefreshAction: Bool {
        !isRunning
    }

    private var statusText: String {
        if isAutomaticRefreshInProgress {
            switch status {
            case let .completed(detail):
                return detail
            case let .failed(detail):
                return detail
            case let .cancelled(detail):
                return detail
            case .idle, .indexing:
                return completedStatusText
            }
        }
        if isShowingCachedState, case .indexing = status {
            return completedStatusText
        }
        switch status {
        case .idle:
            return completedStatusText
        case let .indexing(_, detail):
            return detail
        case let .completed(detail):
            return detail
        case let .cancelled(detail):
            return detail
        case let .failed(detail):
            return detail
        }
    }

    private var completedStatusText: String {
        String(localized: "ai.indexing.complete", defaultValue: "Index complete.", bundle: .module)
    }

    private var progressValue: Double? {
        if case let .indexing(progress, _) = status {
            return progress
        }
        return nil
    }

    private var statusBackground: Color {
        if isAutomaticRefreshInProgress {
            switch status {
            case .failed:
                return AppTheme.warning
            case .cancelled:
                return AppTheme.textMuted.opacity(0.55)
            default:
                return AppTheme.focus
            }
        }
        switch status {
        case .cancelled:
            return AppTheme.textMuted.opacity(0.55)
        case .failed:
            return AppTheme.warning
        case .indexing:
            return AppTheme.focus
        case .completed:
            return AppTheme.focus
        case .idle:
            return AppTheme.focus
        }
    }

    var body: some View {
        HStack(spacing: 10) {
            HStack(spacing: 6) {
                if isAutomaticRefreshInProgress {
                    ProgressView()
                        .controlSize(.small)
                        .tint(.white)
                } else if isManualRunning {
                    ProgressView(value: progressValue)
                        .controlSize(.small)
                        .tint(.white)
                        .frame(width: 42)
                } else {
                    Image(systemName: statusIcon)
                        .font(.system(size: 12, weight: .semibold))
                }

                Text(statusText)
                    .font(.system(size: 12, weight: .medium))
                    .lineLimit(1)
                    .foregroundStyle(Color.white.opacity(0.92))
            }

            Spacer()

            if shouldShowRefreshAction {
                actionButton(
                    action: .refresh,
                    title: canRetry ? String(localized: "common.retry", defaultValue: "Retry", bundle: .module) : String(localized: "common.refresh", defaultValue: "Refresh", bundle: .module),
                    help: canRetry ? String(localized: "ai.action.reload_current_project", defaultValue: "Reload AI stats for the current project.", bundle: .module) : String(localized: "ai.action.refresh_current_project", defaultValue: "Refresh AI stats for the current project.", bundle: .module),
                    systemImage: "arrow.clockwise",
                    buttonAction: onRefresh
                )
            }

            if isManualRunning {
                actionButton(
                    action: .cancel,
                    title: String(localized: "common.stop", defaultValue: "Stop", bundle: .module),
                    help: String(localized: "ai.action.stop_refresh", defaultValue: "Stop the current AI stats refresh.", bundle: .module),
                    systemImage: "stop.fill",
                    buttonAction: onCancel
                )
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 32)
        .frame(maxWidth: .infinity)
        .background(statusBackground)
    }

    private var statusIcon: String {
        switch status {
        case .idle:
            return "checkmark.circle.fill"
        case .completed:
            return "checkmark.circle.fill"
        case .cancelled:
            return "stop.circle.fill"
        case .failed:
            return "exclamationmark.triangle.fill"
        case .indexing:
            return "arrow.triangle.2.circlepath"
        }
    }

    @ViewBuilder
    private func actionButton(
        action: AIStatsIndexingAction,
        title: String,
        help: String,
        systemImage: String,
        buttonAction: @escaping () -> Void
    ) -> some View {
        Button(action: buttonAction) {
            HStack(spacing: 6) {
                Image(systemName: systemImage)
                    .font(.system(size: 12, weight: .bold))

                Text(title)
                    .font(.system(size: 12, weight: .semibold))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .foregroundStyle(Color.white.opacity(0.96))
            .padding(.horizontal, 8)
            .padding(.vertical, 3)
            .contentShape(Rectangle())
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(Color.white.opacity(hoveredAction == action ? 0.22 : 0.001))
            )
        }
        .buttonStyle(.plain)
        .help(help)
        .onHover { hovering in
            hoveredAction = hovering ? action : (hoveredAction == action ? nil : hoveredAction)
        }
    }
}
