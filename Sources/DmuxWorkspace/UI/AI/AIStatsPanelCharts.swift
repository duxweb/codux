import AppKit
import SwiftUI

struct AIStatsHeatmapCard: View {
    let model: AppModel
    let days: [AIHeatmapDay]
    let displayMode: AppAIStatisticsDisplayMode
    @State private var hoveredDay: AIHeatmapDay?
    @State private var hoveredDayAnchor: CGPoint = .zero

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(String(localized: "ai.recent_usage", defaultValue: "Recent Usage", bundle: .module))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)

            AIRecentDaysHeatmapGrid(days: days, hoveredDay: $hoveredDay, hoveredAnchor: $hoveredDayAnchor, displayMode: displayMode)
        }
        .padding(14)
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(alignment: .topLeading) {
            GeometryReader { proxy in
                if let hoveredDay {
                    AIChartTooltip(
                        title: aiStatsFormattedNumericDate(hoveredDay.day, language: model.displayLanguage),
                        primary: "\(String(localized: "ai.metric.token", defaultValue: "Token", bundle: .module)) \(aiStatsFormatCompactToken(hoveredDay.displayedTotalTokens(mode: displayMode)))",
                        secondary: String(format: String(localized: "common.requests_format", defaultValue: "Requests %@", bundle: .module), "\(hoveredDay.requestCount)")
                    )
                    .position(aiStatsChartTooltipPosition(anchor: hoveredDayAnchor, containerSize: proxy.size, tooltipSize: CGSize(width: 150, height: 72)))
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
    let displayMode: AppAIStatisticsDisplayMode

    var body: some View {
        let maxTokens = max(days.map { $0.displayedTotalTokens(mode: displayMode) }.max() ?? 0, 1)
        let sortedNonZeroTokens = days
            .map { $0.displayedTotalTokens(mode: displayMode) }
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
        let normalized = percentileRatio(
            value: item.displayedTotalTokens(mode: displayMode),
            maxValue: maxTokens,
            sortedNonZeroTokens: sortedNonZeroTokens
        )
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

struct AIStatsTodayUsageBarChart: View {
    let model: AppModel
    let buckets: [AITimeBucket]
    let displayMode: AppAIStatisticsDisplayMode
    @State private var hoveredBucket: AITimeBucket?
    @State private var hoveredBucketAnchor: CGPoint = .zero

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text(String(localized: "ai.today_usage", defaultValue: "Today's Usage", bundle: .module))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(.secondary)

            let maxTokens = max(buckets.map { $0.displayedTotalTokens(mode: displayMode) }.max() ?? 0, 1)
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
                                    .frame(width: barWidth, height: max(6, CGFloat(bucket.displayedTotalTokens(mode: displayMode)) / CGFloat(maxTokens) * 72))
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
        .background(AppTheme.aiPanelCardBackground.opacity(0.6), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
        .overlay(alignment: .topLeading) {
            GeometryReader { proxy in
                if let hoveredBucket {
                    AIChartTooltip(
                        title: bucketRangeLabel(hoveredBucket.start, hoveredBucket.end),
                        primary: "\(String(localized: "ai.metric.token", defaultValue: "Token", bundle: .module)) \(aiStatsFormatCompactToken(hoveredBucket.displayedTotalTokens(mode: displayMode)))",
                        secondary: String(format: String(localized: "common.requests_format", defaultValue: "Requests %@", bundle: .module), "\(hoveredBucket.requestCount)")
                    )
                    .position(aiStatsChartTooltipPosition(anchor: hoveredBucketAnchor, containerSize: proxy.size, tooltipSize: CGSize(width: 172, height: 72)))
                }
            }
        }
        .coordinateSpace(name: "today-bar-card")
    }

    private func bucketRangeLabel(_ start: Date, _ end: Date) -> String {
        let formatter = DateFormatter()
        formatter.timeZone = .autoupdatingCurrent
        formatter.locale = aiStatsLocale(for: model.displayLanguage)
        formatter.dateFormat = "HH:mm"
        return "\(formatter.string(from: start)) - \(formatter.string(from: end))"
    }

    private var currentBucketIndex: Int {
        let now = Date()
        let hour = Calendar.autoupdatingCurrent.component(.hour, from: now)
        return min(hour, max(buckets.count - 1, 0))
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
        index % 6 == 0 ? Color(nsColor: .separatorColor).opacity(0.5) : Color.clear
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

func aiStatsChartTooltipPosition(anchor: CGPoint, containerSize: CGSize, tooltipSize: CGSize) -> CGPoint {
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

func aiStatsFormatCompactToken(_ value: Int) -> String {
    if value >= 1_000_000 {
        let precision = value < 10_000_000 ? "%.2fM" : "%.1fM"
        return String(format: precision, Double(value) / 1_000_000)
    }
    if value >= 1_000 {
        let precision = value < 100_000 ? "%.2fK" : "%.1fK"
        return String(format: precision, Double(value) / 1_000)
    }
    return "\(value)"
}

func aiStatsFormatLiveTokenValue(_ value: Int) -> String {
    if value >= 1_000_000 {
        return String(format: "%.1fM", Double(value) / 1_000_000)
    }
    if value >= 10_000 {
        return String(format: "%.1fK", Double(value) / 1_000)
    }
    return value.formatted(.number.grouping(.automatic))
}

func aiStatsFormattedNumericDate(_ date: Date, language: AppLanguage = .system) -> String {
    let formatter = DateFormatter()
    formatter.timeZone = .autoupdatingCurrent
    formatter.locale = aiStatsLocale(for: language.resolved)
    formatter.dateFormat = "yyyy-MM-dd"
    return formatter.string(from: date)
}

func aiStatsLocale(for language: AppLanguage) -> Locale {
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
