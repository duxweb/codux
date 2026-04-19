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
        VStack(spacing: 0) {
            AIStatsHeader(model: model)

            if stateMatchesCurrentProject, let summary = store.state.projectSummary {
                ScrollView {
                    LazyVStack(alignment: .leading, spacing: 12) {
                        AIStatsLiveSessionsCard(model: model, snapshots: store.state.liveSnapshots)
                        AIStatsSummaryCards(
                            model: model,
                            summary: summary,
                            heatmap: store.state.heatmap,
                            todayTimeBuckets: store.state.todayTimeBuckets
                        )
                        if showsDeferredDetails {
                            AIStatsTodayUsageBarChart(model: model, buckets: store.state.todayTimeBuckets)
                            AIStatsHeatmapCard(model: model, days: store.state.heatmap)
                            AIStatsBreakdownCard(model: model, title: String(localized: "ai.breakdown.tool_ranking", defaultValue: "Tool Ranking", bundle: .module), items: store.state.toolBreakdown)
                            AIStatsBreakdownCard(model: model, title: String(localized: "ai.breakdown.model_ranking", defaultValue: "Model Ranking", bundle: .module), items: store.state.modelBreakdown)
                            AIStatsSessionsCard(model: model, sessions: store.state.sessions)
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

            AIStatsIndexingBar(
                model: model,
                status: effectiveIndexingStatus,
                isShowingCachedState: store.refreshState.isShowingCached,
                isAutomaticRefreshInProgress: isAutomaticRefreshInProgress,
                onRefresh: onRefresh,
                onCancel: onCancel
            )
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
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

// MARK: - Empty State

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

// MARK: - Header

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

// MARK: - Summary Cards

private struct AIStatsSummaryCards: View {
    let model: AppModel
    let summary: AIProjectUsageSummary
    let heatmap: [AIHeatmapDay]
    let todayTimeBuckets: [AITimeBucket]

    var body: some View {
        HStack(spacing: 10) {
            metricCard(
                title: String(localized: "ai.summary.current_project", defaultValue: "Current Project", bundle: .module),
                value: formatCompactToken(summary.projectTotalTokens),
                accent: AppTheme.focus
            )
            metricCard(
                title: String(localized: "ai.summary.today_total", defaultValue: "Today's Total", bundle: .module),
                value: formatCompactToken(displayedTodayTotalTokens),
                accent: AppTheme.success
            )
        }
    }

    private var displayedTodayTotalTokens: Int {
        let bucketTotal = todayTimeBuckets.reduce(0) { $0 + $1.totalTokens }
        if bucketTotal > 0 {
            return bucketTotal
        }

        let calendar = Calendar.autoupdatingCurrent
        let today = calendar.startOfDay(for: Date())
        if let heatmapToday = heatmap.first(where: { calendar.isDate($0.day, inSameDayAs: today) })?.totalTokens,
           heatmapToday > 0 {
            return heatmapToday
        }

        return summary.todayTotalTokens
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
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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

// MARK: - Live Sessions

private struct AIStatsLiveSessionsCard: View {
    let model: AppModel
    let snapshots: [AITerminalSessionSnapshot]

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
                            Text(formatLiveTokenValue(max(0, snapshot.currentTotalTokens)))
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
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

// MARK: - Heatmap

private struct AIStatsHeatmapCard: View {
    let model: AppModel
    let days: [AIHeatmapDay]
    @State private var hoveredDay: AIHeatmapDay?
    @State private var hoveredDayAnchor: CGPoint = .zero

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(String(localized: "ai.recent_usage", defaultValue: "Recent Usage", bundle: .module))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)

            AIRecentDaysHeatmapGrid(days: days, hoveredDay: $hoveredDay, hoveredAnchor: $hoveredDayAnchor)
        }
        .padding(14)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(alignment: .topLeading) {
            GeometryReader { proxy in
                if let hoveredDay {
                    AIChartTooltip(
                        title: formattedNumericDate(hoveredDay.day, language: model.displayLanguage),
                        primary: "\(String(localized: "ai.metric.token", defaultValue: "Token", bundle: .module)) \(formatCompactToken(hoveredDay.totalTokens))",
                        secondary: String(format: String(localized: "common.requests_format", defaultValue: "Requests %@", bundle: .module), "\(hoveredDay.requestCount)")
                    )
                    .position(chartTooltipPosition(anchor: hoveredDayAnchor, containerSize: proxy.size, tooltipSize: CGSize(width: 150, height: 72)))
                }
            }
        }
        .coordinateSpace(name: "heatmap-card")
    }
}

private struct AIRecentDaysHeatmapGrid: View {
    let days: [AIHeatmapDay]
    @Binding var hoveredDay: AIHeatmapDay?
    @Binding var hoveredAnchor: CGPoint

    var body: some View {
        let maxTokens = max(days.map(\.totalTokens).max() ?? 0, 1)
        let sortedNonZeroTokens = days
            .map(\.totalTokens)
            .filter { $0 > 0 }
            .sorted()

        VStack(alignment: .leading, spacing: 4) {
            GeometryReader { proxy in
                let spacing: CGFloat = 3
                let baseCellSize: CGFloat = 9
                let visibleColumnCount = max(2, Int(floor((proxy.size.width + spacing) / (baseCellSize + spacing))))
                let cellSize = max(8, min(10, floor((proxy.size.width - spacing * CGFloat(max(visibleColumnCount - 1, 0))) / CGFloat(visibleColumnCount))))
                let displayedColumns = gridColumns(columnCount: visibleColumnCount)
                let gridWidth = CGFloat(displayedColumns.count) * cellSize + CGFloat(max(displayedColumns.count - 1, 0)) * spacing
                let gridHeight = 7 * cellSize + 6 * spacing

                ZStack(alignment: .topLeading) {
                    HStack(alignment: .top, spacing: spacing) {
                        ForEach(Array(displayedColumns.enumerated()), id: \.offset) { _, column in
                            VStack(spacing: spacing) {
                                ForEach(Array(column.enumerated()), id: \.offset) { _, item in
                                    ZStack {
                                        if item == nil {
                                            RoundedRectangle(cornerRadius: 3, style: .continuous)
                                                .fill(Color(nsColor: .quaternaryLabelColor).opacity(0.32))
                                        } else {
                                            RoundedRectangle(cornerRadius: 3, style: .continuous)
                                                .fill(fillColor(for: item, maxTokens: maxTokens, sortedNonZeroTokens: sortedNonZeroTokens))
                                        }

                                        if item != nil, hoveredDay?.id == item?.id {
                                            RoundedRectangle(cornerRadius: 3, style: .continuous)
                                                .strokeBorder(Color(nsColor: .selectedContentBackgroundColor), lineWidth: 1)
                                        }
                                    }
                                    .frame(width: cellSize, height: cellSize)
                                }
                            }
                        }
                    }
                    .frame(width: gridWidth, height: gridHeight, alignment: .topLeading)

                        AIHoverTrackingOverlay(
                            onMove: { localLocation in
                                let nextHoveredDay = hoveredItem(
                                    at: localLocation,
                                    columns: displayedColumns,
                                    cellSize: cellSize,
                                    spacing: spacing,
                                    gridWidth: gridWidth,
                                    gridHeight: gridHeight
                                )
                                if hoveredDay?.id != nextHoveredDay?.id {
                                    hoveredDay = nextHoveredDay
                                    if let nextHoveredDay,
                                       let localAnchor = anchorForItem(
                                           nextHoveredDay,
                                           columns: displayedColumns,
                                           cellSize: cellSize,
                                           spacing: spacing
                                       ) {
                                        hoveredAnchor = localAnchor
                                    }
                                }
                            },
                            onExit: {
                                hoveredDay = nil
                            }
                        )
                            .frame(width: gridWidth, height: gridHeight)
                }
            }
            .frame(height: 7 * 10 + 6 * 3)
        }
    }

    private func gridColumns(columnCount: Int) -> [[AIHeatmapDay?]] {
        let calendar = Calendar.autoupdatingCurrent
        let endDate = calendar.startOfDay(for: Date())
        let minimumDayCount = max(14, columnCount * 7)
        let startDate = calendar.date(byAdding: .day, value: -(minimumDayCount - 1), to: endDate) ?? endDate
        let allDays = (0..<minimumDayCount).compactMap { calendar.date(byAdding: .day, value: $0, to: startDate) }
        let dayMap = Dictionary(uniqueKeysWithValues: days.map { (calendar.startOfDay(for: $0.day), $0) })

        return stride(from: 0, to: allDays.count, by: 7).map { offset in
            Array(allDays[offset ..< min(offset + 7, allDays.count)]).map { dayMap[calendar.startOfDay(for: $0)] }
        }
    }

    private func fillColor(for item: AIHeatmapDay?, maxTokens: Int, sortedNonZeroTokens: [Int]) -> Color {
        guard let item else { return .clear }
        let normalized = percentileRatio(value: item.totalTokens, maxValue: maxTokens, sortedNonZeroTokens: sortedNonZeroTokens)
        switch normalized {
        case 0..<0.10:
            return AppTheme.focus.opacity(0.14)
        case 0.10..<0.20:
            return AppTheme.focus.opacity(0.22)
        case 0.20..<0.32:
            return AppTheme.focus.opacity(0.30)
        case 0.32..<0.44:
            return AppTheme.focus.opacity(0.40)
        case 0.44..<0.56:
            return AppTheme.focus.opacity(0.52)
        case 0.56..<0.68:
            return AppTheme.focus.opacity(0.64)
        case 0.68..<0.80:
            return AppTheme.focus.opacity(0.76)
        case 0.80..<0.92:
            return AppTheme.focus.opacity(0.88)
        default:
            return AppTheme.focus.opacity(1.0)
        }
    }

    private func percentileRatio(value: Int, maxValue: Int, sortedNonZeroTokens: [Int]) -> Double {
        guard value > 0, maxValue > 0, !sortedNonZeroTokens.isEmpty else {
            return 0
        }
        guard sortedNonZeroTokens.count > 1 else {
            return 1
        }
        let upperBound = sortedNonZeroTokens.firstIndex(where: { $0 > value }) ?? sortedNonZeroTokens.count
        let clampedRank = max(0, upperBound - 1)
        return Double(clampedRank) / Double(sortedNonZeroTokens.count - 1)
    }

    private func hoveredItem(
        at location: CGPoint,
        columns: [[AIHeatmapDay?]],
        cellSize: CGFloat,
        spacing: CGFloat,
        gridWidth: CGFloat,
        gridHeight: CGFloat
    ) -> AIHeatmapDay? {
        guard location.x >= 0, location.y >= 0, location.x <= gridWidth, location.y <= gridHeight else {
            return nil
        }

        let step = cellSize + spacing
        let columnIndex = Int(location.x / step)
        let rowIndex = Int(location.y / step)
        let columnRemainder = location.x.truncatingRemainder(dividingBy: step)
        let rowRemainder = location.y.truncatingRemainder(dividingBy: step)

        guard columnRemainder <= cellSize, rowRemainder <= cellSize else {
            return nil
        }
        guard columns.indices.contains(columnIndex), columns[columnIndex].indices.contains(rowIndex) else {
            return nil
        }

        return columns[columnIndex][rowIndex]
    }

    private func anchorForItem(
        _ item: AIHeatmapDay,
        columns: [[AIHeatmapDay?]],
        cellSize: CGFloat,
        spacing: CGFloat
    ) -> CGPoint? {
        for (columnIndex, column) in columns.enumerated() {
            for (rowIndex, candidate) in column.enumerated() where candidate?.id == item.id {
                let step = cellSize + spacing
                return CGPoint(
                    x: CGFloat(columnIndex) * step + cellSize / 2,
                    y: CGFloat(rowIndex) * step + cellSize / 2
                )
            }
        }
        return nil
    }
}

// MARK: - Today Bar Chart

private struct AIStatsTodayUsageBarChart: View {
    let model: AppModel
    let buckets: [AITimeBucket]
    @State private var hoveredBucket: AITimeBucket?
    @State private var hoveredBucketAnchor: CGPoint = .zero

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(String(localized: "ai.today_usage", defaultValue: "Today's Usage", bundle: .module))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)

            let maxTokens = max(buckets.map(\.totalTokens).max() ?? 0, 1)
            GeometryReader { proxy in
                let spacing: CGFloat = 2
                let barCount = max(buckets.count, 1)
                let barWidth = max(3, floor((proxy.size.width - spacing * CGFloat(max(barCount - 1, 0))) / CGFloat(barCount)))
                let chartHeight: CGFloat = 78
                let chartWidth = CGFloat(barCount) * barWidth + CGFloat(max(barCount - 1, 0)) * spacing

                VStack(alignment: .leading, spacing: 6) {
                    ZStack(alignment: .bottomLeading) {
                        HStack(alignment: .bottom, spacing: spacing) {
                            ForEach(Array(buckets.enumerated()), id: \.offset) { _, bucket in
                                RoundedRectangle(cornerRadius: 2, style: .continuous)
                                    .fill(hoveredBucket?.id == bucket.id ? AppTheme.focus : AppTheme.focus.opacity(0.7))
                                    .frame(width: barWidth, height: max(6, CGFloat(bucket.totalTokens) / CGFloat(maxTokens) * 72))
                            }
                        }
                        .frame(width: chartWidth, height: chartHeight, alignment: .bottomLeading)

                        HStack(spacing: spacing) {
                            ForEach(Array(buckets.enumerated()), id: \.offset) { index, _ in
                                Rectangle()
                                    .fill(index == currentBucketIndex ? Color(nsColor: .labelColor).opacity(0.6) : gridLineColor(for: index))
                                    .frame(width: index == currentBucketIndex ? 1 : 0.5, height: 78)
                                    .frame(width: barWidth)
                            }
                        }
                        .frame(width: chartWidth, height: chartHeight, alignment: .bottomLeading)
                        .allowsHitTesting(false)

                        AIHoverTrackingOverlay(
                            onMove: { localLocation in
                                let nextHoveredIndex = hoveredBucketIndexAt(
                                    location: localLocation,
                                    barWidth: barWidth,
                                    spacing: spacing,
                                    chartWidth: chartWidth,
                                    chartHeight: chartHeight
                                )
                                let nextHoveredBucket = nextHoveredIndex.flatMap { buckets.indices.contains($0) ? buckets[$0] : nil }
                                if hoveredBucket?.id != nextHoveredBucket?.id {
                                    hoveredBucket = nextHoveredBucket
                                    if let nextHoveredIndex {
                                        let step = barWidth + spacing
                                        hoveredBucketAnchor = CGPoint(
                                            x: CGFloat(nextHoveredIndex) * step + barWidth / 2,
                                            y: chartHeight / 2
                                        )
                                    }
                                }
                            },
                            onExit: {
                                hoveredBucket = nil
                            }
                        )
                            .frame(width: chartWidth, height: chartHeight)
                    }

                    HStack(spacing: 0) {
                        ForEach(axisLabels(totalWidth: proxy.size.width), id: \.label) { item in
                            Text(item.label)
                                .lineLimit(1)
                                .fixedSize(horizontal: true, vertical: false)
                                .font(.system(size: 11, weight: .medium, design: .rounded))
                                .foregroundStyle(.tertiary)
                                .frame(width: item.width, alignment: .leading)
                        }
                    }
                }
            }
            .frame(height: 96)
        }
        .padding(14)
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(alignment: .topLeading) {
            GeometryReader { proxy in
                if let hoveredBucket {
                    AIChartTooltip(
                        title: bucketRangeLabel(hoveredBucket.start, hoveredBucket.end),
                        primary: "\(String(localized: "ai.metric.token", defaultValue: "Token", bundle: .module)) \(formatCompactToken(hoveredBucket.totalTokens))",
                        secondary: String(format: String(localized: "common.requests_format", defaultValue: "Requests %@", bundle: .module), "\(hoveredBucket.requestCount)")
                    )
                    .position(chartTooltipPosition(anchor: hoveredBucketAnchor, containerSize: proxy.size, tooltipSize: CGSize(width: 172, height: 72)))
                }
            }
        }
        .coordinateSpace(name: "today-bar-card")
    }

    private func bucketRangeLabel(_ start: Date, _ end: Date) -> String {
        let formatter = DateFormatter()
        formatter.timeZone = .autoupdatingCurrent
        formatter.locale = locale(for: model.displayLanguage)
        formatter.dateFormat = "HH:mm"
        return "\(formatter.string(from: start)) - \(formatter.string(from: end))"
    }

    private var currentBucketIndex: Int {
        let now = Date()
        let hour = Calendar.autoupdatingCurrent.component(.hour, from: now)
        let minute = Calendar.autoupdatingCurrent.component(.minute, from: now)
        return min(((hour * 60) + minute) / 30, max(buckets.count - 1, 0))
    }

    private func axisLabels(totalWidth: CGFloat) -> [(label: String, width: CGFloat)] {
        let segmentWidth = totalWidth / 4
        return [
            ("00:00", segmentWidth),
            ("06:00", segmentWidth),
            ("12:00", segmentWidth),
            ("18:00", segmentWidth),
        ]
    }

    private func gridLineColor(for index: Int) -> Color {
        index % 12 == 0 ? Color(nsColor: .separatorColor).opacity(0.5) : Color.clear
    }

    private func hoveredBucketIndexAt(
        location: CGPoint,
        barWidth: CGFloat,
        spacing: CGFloat,
        chartWidth: CGFloat,
        chartHeight: CGFloat
    ) -> Int? {
        guard location.x >= 0, location.y >= 0, location.x <= chartWidth, location.y <= chartHeight else {
            return nil
        }

        let step = barWidth + spacing
        let index = Int(location.x / step)
        let remainder = location.x.truncatingRemainder(dividingBy: step)

        guard remainder <= barWidth, buckets.indices.contains(index) else {
            return nil
        }

        return index
    }
}

// MARK: - Tooltip

private struct AIHoverTrackingOverlay: NSViewRepresentable {
    final class HoverView: NSView {
        var onMove: ((CGPoint) -> Void)?
        var onExit: (() -> Void)?
        private var currentTrackingArea: NSTrackingArea?

        override var isFlipped: Bool {
            true
        }

        override func updateTrackingAreas() {
            super.updateTrackingAreas()
            if let currentTrackingArea {
                removeTrackingArea(currentTrackingArea)
            }
            let nextTrackingArea = NSTrackingArea(
                rect: .zero,
                options: [.activeInKeyWindow, .inVisibleRect, .mouseMoved, .mouseEnteredAndExited],
                owner: self,
                userInfo: nil
            )
            addTrackingArea(nextTrackingArea)
            currentTrackingArea = nextTrackingArea
        }

        override func mouseEntered(with event: NSEvent) {
            mouseMoved(with: event)
        }

        override func mouseMoved(with event: NSEvent) {
            onMove?(convert(event.locationInWindow, from: nil))
        }

        override func mouseExited(with event: NSEvent) {
            onExit?()
        }
    }

    let onMove: (CGPoint) -> Void
    let onExit: () -> Void

    func makeNSView(context: Context) -> HoverView {
        let view = HoverView(frame: .zero)
        view.onMove = onMove
        view.onExit = onExit
        return view
    }

    func updateNSView(_ nsView: HoverView, context: Context) {
        nsView.onMove = onMove
        nsView.onExit = onExit
    }
}

private struct AIChartTooltip: View {
    let title: String
    let primary: String
    let secondary: String

    var body: some View {
        VStack(alignment: .leading, spacing: 4) {
            Text(title)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.white)
            Text(primary)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.white.opacity(0.75))
            Text(secondary)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.white.opacity(0.55))
        }
        .padding(8)
        .background(Color(white: 0.12, opacity: 0.92), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
        .allowsHitTesting(false)
    }
}

private func chartTooltipPosition(anchor: CGPoint, containerSize: CGSize, tooltipSize: CGSize) -> CGPoint {
    let padding: CGFloat = 8
    let offset: CGFloat = 18
    var topLeftX = anchor.x + offset
    var topLeftY = anchor.y + offset

    if topLeftX + tooltipSize.width > containerSize.width - padding {
        topLeftX = anchor.x - tooltipSize.width - offset
    }
    if topLeftY + tooltipSize.height > containerSize.height - padding {
        topLeftY = anchor.y - tooltipSize.height - offset
    }

    let maxX = max(padding, containerSize.width - tooltipSize.width - padding)
    let maxY = max(padding, containerSize.height - tooltipSize.height - padding)

    topLeftX = min(max(topLeftX, padding), maxX)
    topLeftY = min(max(topLeftY, padding), maxY)

    return CGPoint(x: topLeftX + tooltipSize.width / 2, y: topLeftY + tooltipSize.height / 2)
}

// MARK: - Helpers

private func formatCompactToken(_ value: Int) -> String {
    if value >= 1_000_000 {
        return String(format: "%.1fM", Double(value) / 1_000_000)
    }
    if value >= 1_000 {
        return String(format: "%.1fK", Double(value) / 1_000)
    }
    return "\(value)"
}

private func formatLiveTokenValue(_ value: Int) -> String {
    if value >= 1_000_000 {
        return String(format: "%.1fM", Double(value) / 1_000_000)
    }
    if value >= 10_000 {
        return String(format: "%.1fK", Double(value) / 1_000)
    }
    return value.formatted(.number.grouping(.automatic))
}

private func formattedNumericDate(_ date: Date, language: AppLanguage = .system) -> String {
    let formatter = DateFormatter()
    formatter.timeZone = .autoupdatingCurrent
    formatter.locale = locale(for: language.resolved)
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter.string(from: date)
}

// MARK: - Breakdown

private struct AIStatsBreakdownCard: View {
    let model: AppModel
    let title: String
    let items: [AIUsageBreakdownItem]

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
                let total = max(items.reduce(0) { $0 + $1.totalTokens }, 1)
                VStack(spacing: 0) {
                    ForEach(Array(items.enumerated()), id: \.element.id) { index, item in
                        let ratio = Double(item.totalTokens) / Double(total)

                        VStack(alignment: .leading, spacing: 6) {
                            HStack {
                                Text(item.key)
                                    .font(.system(size: 13, weight: .medium))
                                    .foregroundStyle(.primary)
                                    .lineLimit(1)

                                Spacer()

                                Text(formatCompactToken(item.totalTokens))
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
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
    }
}

// MARK: - Sessions

private struct AIStatsSessionsCard: View {
    let model: AppModel
    let sessions: [AISessionSummary]
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
                                Text(formatCompactToken(session.totalTokens))
                                    .font(.system(size: 14, weight: .medium, design: .rounded))
                                    .foregroundStyle(.primary)
                                Text(String(format: String(localized: "common.today_format", defaultValue: "Today %@", bundle: .module), formatCompactToken(session.todayTokens)))
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
        .background(Color(nsColor: .controlBackgroundColor).opacity(0.7), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
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
        formatter.locale = locale(for: model.displayLanguage)
        return formatter.localizedString(for: date, relativeTo: Date())
    }
}

// MARK: - Indexing Bar

private struct AIStatsIndexingBar: View {
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
                return String(localized: "ai.status.ready", defaultValue: "AI Stats Ready", bundle: .module)
            }
        }
        if isShowingCachedState, case .indexing = status {
            return String(localized: "ai.status.cached_refreshing", defaultValue: "Showing recent results, updating in background", bundle: .module)
        }
        switch status {
        case .idle:
            return String(localized: "ai.status.ready", defaultValue: "AI Stats Ready", bundle: .module)
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

private func locale(for language: AppLanguage) -> Locale {
    switch language {
    case .system:
        return .autoupdatingCurrent
    case .traditionalChinese:
        return Locale(identifier: "zh_TW")
    case .english:
        return Locale(identifier: "en_US")
    case .simplifiedChinese:
        return Locale(identifier: "zh_CN")
    case .japanese:
        return Locale(identifier: "ja_JP")
    case .korean:
        return Locale(identifier: "ko_KR")
    case .french:
        return Locale(identifier: "fr_FR")
    case .german:
        return Locale(identifier: "de_DE")
    case .spanish:
        return Locale(identifier: "es_ES")
    case .portugueseBrazil:
        return Locale(identifier: "pt_BR")
    case .russian:
        return Locale(identifier: "ru_RU")
    }
}
