import CryptoKit
import Foundation

enum AIHistorySessionRole: String, Codable, Sendable {
    case user
    case assistant
}

struct AIHistorySessionKey: Hashable, Sendable {
    var source: String
    var sessionID: String
}

struct AIHistoryUsageEntry: Sendable {
    var key: AIHistorySessionKey
    var projectName: String
    var timestamp: Date
    var model: String
    var inputTokens: Int
    var outputTokens: Int
    var cachedInputTokens: Int
    var reasoningOutputTokens: Int

    var totalTokens: Int {
        inputTokens + outputTokens + reasoningOutputTokens
    }
}

struct AIHistorySessionEvent: Sendable {
    var key: AIHistorySessionKey
    var projectName: String
    var timestamp: Date
    var role: AIHistorySessionRole
}

struct AIHistorySessionMetadata: Sendable {
    var key: AIHistorySessionKey
    var externalSessionID: String?
    var sessionTitle: String?
    var model: String?
}

struct AIHistoryParseResult: Sendable {
    var entries: [AIHistoryUsageEntry]
    var events: [AIHistorySessionEvent]
    var metadataByKey: [AIHistorySessionKey: AIHistorySessionMetadata]

    static let empty = AIHistoryParseResult(entries: [], events: [], metadataByKey: [:])
}

struct AIHistoryExtractedSessionMetrics: Equatable, Sendable {
    var key: AIHistorySessionKey
    var projectName: String
    var firstMessageAt: Date
    var lastMessageAt: Date
    var durationSeconds: Int
    var activeSeconds: Int
    var messageCount: Int
    var userMessageCount: Int
    var userPromptHours: [Int]
}

struct AIHistoryAggregationService: Sendable {
    private let calendar = Calendar.autoupdatingCurrent

    func buildExternalFileSummary(
        source: String,
        filePath: String,
        fileModifiedAt: Double,
        project: Project,
        parseResult: AIHistoryParseResult
    ) -> AIExternalFileSummary {
        let sessions = sortSessions(buildSessions(project: project, parseResults: [parseResult]))
        return AIExternalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            projectPath: project.path,
            sessions: sessions,
            dayUsage: buildHeatmap(parseResult.entries, events: parseResult.events),
            timeBuckets: buildTodayTimeBuckets(parseResult.entries, events: parseResult.events)
        )
    }

    func extractSessions(_ events: [AIHistorySessionEvent]) -> [AIHistoryExtractedSessionMetrics] {
        let grouped = Dictionary(grouping: events, by: \.key)
        var sessions: [AIHistoryExtractedSessionMetrics] = []

        for (key, sessionEvents) in grouped {
            let orderedEvents = sessionEvents.sorted { $0.timestamp < $1.timestamp }
            guard let first = orderedEvents.first,
                  let last = orderedEvents.last else {
                continue
            }

            let durationSeconds = max(0, Int(last.timestamp.timeIntervalSince(first.timestamp).rounded()))
            var activeSeconds = 0
            var turnStart: Date?
            var turnEnd: Date?
            var waitingForFirstResponse = false

            for event in orderedEvents {
                switch event.role {
                case .user:
                    if let turnStart, let turnEnd, turnEnd > turnStart {
                        activeSeconds += max(0, Int(turnEnd.timeIntervalSince(turnStart).rounded()))
                    }
                    turnStart = nil
                    turnEnd = nil
                    waitingForFirstResponse = true

                case .assistant:
                    if waitingForFirstResponse {
                        turnStart = event.timestamp
                        turnEnd = event.timestamp
                        waitingForFirstResponse = false
                    } else if turnStart != nil {
                        turnEnd = event.timestamp
                    }
                }
            }

            if let turnStart, let turnEnd, turnEnd > turnStart {
                activeSeconds += max(0, Int(turnEnd.timeIntervalSince(turnStart).rounded()))
            }

            var userPromptHours = Array(repeating: 0, count: 24)
            var userMessageCount = 0
            for event in orderedEvents where event.role == .user {
                userMessageCount += 1
                userPromptHours[calendar.component(.hour, from: event.timestamp)] += 1
            }

            sessions.append(
                AIHistoryExtractedSessionMetrics(
                    key: key,
                    projectName: first.projectName,
                    firstMessageAt: first.timestamp,
                    lastMessageAt: last.timestamp,
                    durationSeconds: durationSeconds,
                    activeSeconds: activeSeconds,
                    messageCount: orderedEvents.count,
                    userMessageCount: userMessageCount,
                    userPromptHours: userPromptHours
                )
            )
        }

        return sessions.sorted { $0.lastMessageAt > $1.lastMessageAt }
    }

    func buildProjectSummary(
        project: Project,
        parseResults: [AIHistoryParseResult]
    ) -> AIProjectDirectorySourceSummary {
        let entries = parseResults.flatMap(\.entries)
        let events = parseResults.flatMap(\.events)
        return makeProjectSummary(
            project: project,
            sessions: buildSessions(project: project, parseResults: parseResults),
            heatmap: buildHeatmap(entries, events: events),
            todayTimeBuckets: buildTodayTimeBuckets(entries, events: events)
        )
    }

    func buildProjectSummary(
        project: Project,
        fileSummaries: [AIExternalFileSummary]
    ) -> AIProjectDirectorySourceSummary {
        makeProjectSummary(
            project: project,
            sessions: mergeSessions(fileSummaries.flatMap(\.sessions)),
            heatmap: mergeHeatmap(fileSummaries.flatMap(\.dayUsage)),
            todayTimeBuckets: mergeTimeBuckets(fileSummaries.flatMap(\.timeBuckets))
        )
    }

    private func buildSessions(
        project: Project,
        parseResults: [AIHistoryParseResult]
    ) -> [AISessionSummary] {
        let entries = parseResults.flatMap(\.entries)
        let events = parseResults.flatMap(\.events)
        let metadataByKey = mergeMetadata(parseResults.map(\.metadataByKey))
        let usageByKey = aggregateUsage(entries)
        let activityByKey = Dictionary(
            uniqueKeysWithValues: extractSessions(events).map { ($0.key, $0) }
        )

        let allKeys = Set(usageByKey.keys)
            .union(activityByKey.keys)
            .union(metadataByKey.keys)

        return allKeys.compactMap { key in
            makeSessionSummary(
                project: project,
                key: key,
                usage: usageByKey[key],
                activity: activityByKey[key],
                metadata: metadataByKey[key]
            )
        }
    }

    private func makeProjectSummary(
        project: Project,
        sessions rawSessions: [AISessionSummary],
        heatmap: [AIHeatmapDay],
        todayTimeBuckets: [AITimeBucket]
    ) -> AIProjectDirectorySourceSummary {
        let sessions = sortSessions(rawSessions)
        let toolBreakdown = breakdown(items:
            sessions.map { ($0.lastTool ?? keyLabelUnknownTool, $0.totalTokens, max($0.requestCount, 1)) }
        )
        let modelBreakdown = breakdown(items:
            sessions.compactMap {
                guard let model = normalizedNonEmptyString($0.lastModel) else {
                    return nil
                }
                return (model, $0.totalTokens, max($0.requestCount, 1))
            }
        )

        return AIProjectDirectorySourceSummary(
            sessions: sessions,
            heatmap: heatmap,
            todayTimeBuckets: todayTimeBuckets,
            toolBreakdown: toolBreakdown,
            modelBreakdown: modelBreakdown
        )
    }

    private func mergeSessions(_ sessions: [AISessionSummary]) -> [AISessionSummary] {
        var merged: [String: AISessionSummary] = [:]
        for session in sessions {
            let key = "\(session.lastTool ?? "unknown")|\(session.externalSessionID ?? session.sessionID.uuidString)"
            if let existing = merged[key] {
                merged[key] = AISessionSummary(
                    sessionID: existing.sessionID,
                    externalSessionID: existing.externalSessionID ?? session.externalSessionID,
                    projectID: existing.projectID,
                    projectName: existing.projectName,
                    sessionTitle: preferredTitle(existing.sessionTitle, session.sessionTitle) ?? existing.sessionTitle,
                    firstSeenAt: min(existing.firstSeenAt, session.firstSeenAt),
                    lastSeenAt: max(existing.lastSeenAt, session.lastSeenAt),
                    lastTool: existing.lastTool ?? session.lastTool,
                    lastModel: existing.lastModel ?? session.lastModel,
                    requestCount: max(existing.requestCount, session.requestCount),
                    totalInputTokens: max(existing.totalInputTokens, session.totalInputTokens),
                    totalOutputTokens: max(existing.totalOutputTokens, session.totalOutputTokens),
                    totalTokens: max(existing.totalTokens, session.totalTokens),
                    maxContextUsagePercent: max(existing.maxContextUsagePercent ?? 0, session.maxContextUsagePercent ?? 0),
                    activeDurationSeconds: max(existing.activeDurationSeconds, session.activeDurationSeconds),
                    todayTokens: max(existing.todayTokens, session.todayTokens)
                )
            } else {
                merged[key] = session
            }
        }
        return Array(merged.values)
    }

    private func mergeHeatmap(_ days: [AIHeatmapDay]) -> [AIHeatmapDay] {
        var map: [Date: AIHeatmapDay] = [:]
        for day in days {
            if var existing = map[day.day] {
                existing.totalTokens += day.totalTokens
                existing.requestCount += day.requestCount
                map[day.day] = existing
            } else {
                map[day.day] = day
            }
        }
        return map.values.sorted { $0.day < $1.day }
    }

    private func mergeTimeBuckets(_ buckets: [AITimeBucket]) -> [AITimeBucket] {
        var map: [Date: AITimeBucket] = [:]
        for bucket in buckets {
            if var existing = map[bucket.start] {
                existing.totalTokens += bucket.totalTokens
                existing.requestCount += bucket.requestCount
                map[bucket.start] = existing
            } else {
                map[bucket.start] = bucket
            }
        }
        return map.values.sorted { $0.start < $1.start }
    }

    private func mergeMetadata(
        _ maps: [[AIHistorySessionKey: AIHistorySessionMetadata]]
    ) -> [AIHistorySessionKey: AIHistorySessionMetadata] {
        var merged: [AIHistorySessionKey: AIHistorySessionMetadata] = [:]
        for map in maps {
            for (key, metadata) in map {
                if var existing = merged[key] {
                    existing.externalSessionID = normalizedNonEmptyString(existing.externalSessionID)
                        ?? normalizedNonEmptyString(metadata.externalSessionID)
                    existing.sessionTitle = preferredTitle(existing.sessionTitle, metadata.sessionTitle)
                    existing.model = normalizedNonEmptyString(existing.model)
                        ?? normalizedNonEmptyString(metadata.model)
                    merged[key] = existing
                } else {
                    merged[key] = metadata
                }
            }
        }
        return merged
    }

    private func preferredTitle(_ lhs: String?, _ rhs: String?) -> String? {
        normalizedNonEmptyString(lhs) ?? normalizedNonEmptyString(rhs)
    }

    private func aggregateUsage(
        _ entries: [AIHistoryUsageEntry]
    ) -> [AIHistorySessionKey: (inputTokens: Int, outputTokens: Int, totalTokens: Int, model: String?, firstSeenAt: Date, lastSeenAt: Date, todayTokens: Int, requestCount: Int)] {
        let now = Date()
        let startOfToday = calendar.startOfDay(for: now)
        var map: [AIHistorySessionKey: (inputTokens: Int, outputTokens: Int, totalTokens: Int, model: String?, firstSeenAt: Date, lastSeenAt: Date, todayTokens: Int, requestCount: Int)] = [:]

        for entry in entries {
            let inputTokens = entry.inputTokens
            let outputTokens = entry.outputTokens
            let todayTokens = calendar.startOfDay(for: entry.timestamp) == startOfToday ? entry.totalTokens : 0

            if var existing = map[entry.key] {
                existing.inputTokens += inputTokens
                existing.outputTokens += outputTokens
                existing.totalTokens += entry.totalTokens
                if normalizedNonEmptyString(existing.model) == nil {
                    existing.model = normalizedNonEmptyString(entry.model)
                }
                existing.firstSeenAt = min(existing.firstSeenAt, entry.timestamp)
                existing.lastSeenAt = max(existing.lastSeenAt, entry.timestamp)
                existing.todayTokens += todayTokens
                map[entry.key] = existing
            } else {
                map[entry.key] = (
                    inputTokens,
                    outputTokens,
                    entry.totalTokens,
                    normalizedNonEmptyString(entry.model),
                    entry.timestamp,
                    entry.timestamp,
                    todayTokens,
                    0
                )
            }
        }

        return map
    }

    private func makeSessionSummary(
        project: Project,
        key: AIHistorySessionKey,
        usage: (inputTokens: Int, outputTokens: Int, totalTokens: Int, model: String?, firstSeenAt: Date, lastSeenAt: Date, todayTokens: Int, requestCount: Int)?,
        activity: AIHistoryExtractedSessionMetrics?,
        metadata: AIHistorySessionMetadata?
    ) -> AISessionSummary? {
        let firstSeenAt = activity?.firstMessageAt ?? usage?.firstSeenAt
        let lastSeenAt = activity?.lastMessageAt ?? usage?.lastSeenAt
        guard let firstSeenAt, let lastSeenAt else {
            return nil
        }

        let externalSessionID = normalizedNonEmptyString(metadata?.externalSessionID) ?? key.sessionID
        let requestCount = max(
            activity?.userMessageCount ?? 0,
            1
        )
        let model = normalizedNonEmptyString(metadata?.model) ?? usage?.model
        let sessionTitle = normalizedNonEmptyString(metadata?.sessionTitle) ?? project.name
        let activeDuration = activity?.activeSeconds
            ?? max(0, Int(lastSeenAt.timeIntervalSince(firstSeenAt)))

        return AISessionSummary(
            sessionID: deterministicUUID(from: "\(key.source):\(externalSessionID)"),
            externalSessionID: externalSessionID,
            projectID: project.id,
            projectName: project.name,
            sessionTitle: sessionTitle,
            firstSeenAt: firstSeenAt,
            lastSeenAt: lastSeenAt,
            lastTool: key.source,
            lastModel: model,
            requestCount: requestCount,
            totalInputTokens: usage?.inputTokens ?? 0,
            totalOutputTokens: usage?.outputTokens ?? 0,
            totalTokens: usage?.totalTokens ?? 0,
            maxContextUsagePercent: nil,
            activeDurationSeconds: activeDuration,
            todayTokens: usage?.todayTokens ?? 0
        )
    }

    private func buildHeatmap(_ entries: [AIHistoryUsageEntry], events: [AIHistorySessionEvent]) -> [AIHeatmapDay] {
        var map: [Date: AIHeatmapDay] = [:]
        for entry in entries {
            let day = calendar.startOfDay(for: entry.timestamp)
            if var existing = map[day] {
                existing.totalTokens += entry.totalTokens
                map[day] = existing
            } else {
                map[day] = AIHeatmapDay(day: day, totalTokens: entry.totalTokens, requestCount: 0)
            }
        }
        for event in events where event.role == .user {
            let day = calendar.startOfDay(for: event.timestamp)
            if var existing = map[day] {
                existing.requestCount += 1
                map[day] = existing
            } else {
                map[day] = AIHeatmapDay(day: day, totalTokens: 0, requestCount: 1)
            }
        }
        return map.values.sorted { $0.day < $1.day }
    }

    private func buildTodayTimeBuckets(_ entries: [AIHistoryUsageEntry], events: [AIHistorySessionEvent]) -> [AITimeBucket] {
        let startOfToday = calendar.startOfDay(for: Date())
        var bucketMap: [Date: AITimeBucket] = [:]

        for entry in entries where calendar.startOfDay(for: entry.timestamp) == startOfToday {
            let bucketStart = roundToHour(entry.timestamp)
            let bucketEnd = calendar.date(byAdding: .hour, value: 1, to: bucketStart) ?? bucketStart
            if var existing = bucketMap[bucketStart] {
                existing.totalTokens += entry.totalTokens
                bucketMap[bucketStart] = existing
            } else {
                bucketMap[bucketStart] = AITimeBucket(
                    start: bucketStart,
                    end: bucketEnd,
                    totalTokens: entry.totalTokens,
                    requestCount: 0
                )
            }
        }

        for event in events where event.role == .user && calendar.startOfDay(for: event.timestamp) == startOfToday {
            let bucketStart = roundToHour(event.timestamp)
            let bucketEnd = calendar.date(byAdding: .hour, value: 1, to: bucketStart) ?? bucketStart
            if var existing = bucketMap[bucketStart] {
                existing.requestCount += 1
                bucketMap[bucketStart] = existing
            } else {
                bucketMap[bucketStart] = AITimeBucket(
                    start: bucketStart,
                    end: bucketEnd,
                    totalTokens: 0,
                    requestCount: 1
                )
            }
        }

        return stride(from: 0, to: 24, by: 1).map { hourOffset in
            let bucketStart = calendar.date(byAdding: .hour, value: hourOffset, to: startOfToday)!
            let bucketEnd = calendar.date(byAdding: .hour, value: 1, to: bucketStart)!
            return bucketMap[bucketStart] ?? AITimeBucket(
                start: bucketStart,
                end: bucketEnd,
                totalTokens: 0,
                requestCount: 0
            )
        }
    }

    private func breakdown(items: [(String, Int, Int)]) -> [AIUsageBreakdownItem] {
        var map: [String: AIUsageBreakdownItem] = [:]
        for item in items {
            if var existing = map[item.0] {
                existing.totalTokens += item.1
                existing.requestCount += item.2
                map[item.0] = existing
            } else {
                map[item.0] = AIUsageBreakdownItem(key: item.0, totalTokens: item.1, requestCount: item.2)
            }
        }
        return map.values.sorted { $0.totalTokens > $1.totalTokens }
    }

    private func roundToHour(_ date: Date) -> Date {
        let components = calendar.dateComponents([.year, .month, .day, .hour], from: date)
        return calendar.date(from: DateComponents(
            year: components.year,
            month: components.month,
            day: components.day,
            hour: components.hour
        )) ?? date
    }

    private func sortSessions(_ sessions: [AISessionSummary]) -> [AISessionSummary] {
        sessions.sorted { lhs, rhs in
            if lhs.lastSeenAt != rhs.lastSeenAt {
                return lhs.lastSeenAt > rhs.lastSeenAt
            }
            return lhs.sessionTitle.localizedStandardCompare(rhs.sessionTitle) == .orderedAscending
        }
    }

    private func deterministicUUID(from value: String) -> UUID {
        let digest = SHA256.hash(data: Data(value.utf8))
        let bytes = Array(digest.prefix(16))
        let uuidBytes: uuid_t = (
            bytes[0], bytes[1], bytes[2], bytes[3],
            bytes[4], bytes[5], bytes[6], bytes[7],
            bytes[8], bytes[9], bytes[10], bytes[11],
            bytes[12], bytes[13], bytes[14], bytes[15]
        )
        return UUID(uuid: uuidBytes)
    }

    private func normalizedNonEmptyString(_ value: String?) -> String? {
        guard let value = value?.trimmingCharacters(in: .whitespacesAndNewlines),
              !value.isEmpty else {
            return nil
        }
        return value
    }

    private var keyLabelUnknownTool: String {
        String(localized: "ai.unknown_tool", defaultValue: "Unknown Tool", bundle: .module)
    }
}
