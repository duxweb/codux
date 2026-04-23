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

private struct AIUsageBucketGroupKey: Hashable {
    var source: String
    var sessionKey: String
    var modelKey: String
    var bucketStart: Date
}

struct AIHistoryAggregationService: Sendable {
    private let calendar = Calendar.autoupdatingCurrent
    private let timeBucketIntervalMinutes = 30

    func buildExternalFileSummary(
        source: String,
        filePath: String,
        fileModifiedAt: Double,
        project: Project,
        parseResult: AIHistoryParseResult
    ) -> AIExternalFileSummary {
        let usageBuckets = buildUsageBuckets(project: project, parseResult: parseResult)
        return externalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            projectPath: project.path,
            usageBuckets: usageBuckets
        )
    }

    func externalFileSummary(
        source: String,
        filePath: String,
        fileModifiedAt: Double,
        projectPath: String,
        usageBuckets: [AIUsageBucket]
    ) -> AIExternalFileSummary {
        let project = usageBuckets.first.map {
            Project(
                id: $0.projectID,
                name: $0.projectName,
                path: projectPath,
                shell: "/bin/zsh",
                defaultCommand: "",
                badgeText: nil,
                badgeSymbol: nil,
                badgeColorHex: nil,
                gitDefaultPushRemoteName: nil
            )
        } ?? Project(
            id: UUID(),
            name: URL(fileURLWithPath: projectPath).lastPathComponent,
            path: projectPath,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )

        let summary = makeProjectSummary(project: project, usageBuckets: usageBuckets)
        return AIExternalFileSummary(
            source: source,
            filePath: filePath,
            fileModifiedAt: fileModifiedAt,
            projectPath: projectPath,
            usageBuckets: usageBuckets,
            sessions: summary.sessions,
            dayUsage: summary.heatmap,
            timeBuckets: summary.todayTimeBuckets
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
        let usageBuckets = parseResults.flatMap { buildUsageBuckets(project: project, parseResult: $0) }
        return makeProjectSummary(project: project, usageBuckets: usageBuckets)
    }

    func buildProjectSummary(
        project: Project,
        fileSummaries: [AIExternalFileSummary]
    ) -> AIProjectDirectorySourceSummary {
        makeProjectSummary(project: project, usageBuckets: fileSummaries.flatMap(\.usageBuckets))
    }

    func buildUsageBuckets(
        project: Project,
        parseResult: AIHistoryParseResult
    ) -> [AIUsageBucket] {
        let metadataByKey = parseResult.metadataByKey
        let activityByKey = Dictionary(
            uniqueKeysWithValues: extractSessions(parseResult.events).map { ($0.key, $0) }
        )
        var map: [AIUsageBucketGroupKey: AIUsageBucket] = [:]

        for entry in parseResult.entries {
            let metadata = metadataByKey[entry.key]
            let bucketStart = roundToTimeBucket(entry.timestamp)
            let key = AIUsageBucketGroupKey(
                source: entry.key.source,
                sessionKey: entry.key.sessionID,
                modelKey: normalizedNonEmptyString(entry.model) ?? "",
                bucketStart: bucketStart
            )
            let externalSessionID = normalizedNonEmptyString(metadata?.externalSessionID) ?? entry.key.sessionID
            let sessionTitle = preferredTitle(metadata?.sessionTitle, project.name) ?? project.name
            let bucketEnd = calendar.date(byAdding: .minute, value: timeBucketIntervalMinutes, to: bucketStart) ?? bucketStart

            if var bucket = map[key] {
                bucket.inputTokens += entry.inputTokens
                bucket.outputTokens += entry.outputTokens
                bucket.totalTokens += entry.totalTokens
                bucket.cachedInputTokens += entry.cachedInputTokens
                bucket.firstSeenAt = min(bucket.firstSeenAt, entry.timestamp)
                bucket.lastSeenAt = max(bucket.lastSeenAt, entry.timestamp)
                map[key] = bucket
            } else {
                map[key] = AIUsageBucket(
                    source: entry.key.source,
                    sessionKey: entry.key.sessionID,
                    externalSessionID: externalSessionID,
                    sessionTitle: sessionTitle,
                    model: normalizedNonEmptyString(entry.model),
                    projectID: project.id,
                    projectName: project.name,
                    bucketStart: bucketStart,
                    bucketEnd: bucketEnd,
                    inputTokens: entry.inputTokens,
                    outputTokens: entry.outputTokens,
                    totalTokens: entry.totalTokens,
                    cachedInputTokens: entry.cachedInputTokens,
                    requestCount: 0,
                    activeDurationSeconds: 0,
                    firstSeenAt: entry.timestamp,
                    lastSeenAt: entry.timestamp
                )
            }
        }

        for event in parseResult.events {
            let metadata = metadataByKey[event.key]
            let bucketStart = roundToTimeBucket(event.timestamp)
            let key = AIUsageBucketGroupKey(
                source: event.key.source,
                sessionKey: event.key.sessionID,
                modelKey: "",
                bucketStart: bucketStart
            )
            let externalSessionID = normalizedNonEmptyString(metadata?.externalSessionID) ?? event.key.sessionID
            let sessionTitle = preferredTitle(metadata?.sessionTitle, project.name) ?? project.name
            let bucketEnd = calendar.date(byAdding: .minute, value: timeBucketIntervalMinutes, to: bucketStart) ?? bucketStart

            if var bucket = map[key] {
                if event.role == .user {
                    bucket.requestCount += 1
                }
                bucket.firstSeenAt = min(bucket.firstSeenAt, event.timestamp)
                bucket.lastSeenAt = max(bucket.lastSeenAt, event.timestamp)
                map[key] = bucket
            } else {
                map[key] = AIUsageBucket(
                    source: event.key.source,
                    sessionKey: event.key.sessionID,
                    externalSessionID: externalSessionID,
                    sessionTitle: sessionTitle,
                    model: nil,
                    projectID: project.id,
                    projectName: project.name,
                    bucketStart: bucketStart,
                    bucketEnd: bucketEnd,
                    inputTokens: 0,
                    outputTokens: 0,
                    totalTokens: 0,
                    cachedInputTokens: 0,
                    requestCount: event.role == .user ? 1 : 0,
                    activeDurationSeconds: 0,
                    firstSeenAt: event.timestamp,
                    lastSeenAt: event.timestamp
                )
            }
        }

        for (sessionKey, activity) in activityByKey {
            let metadata = metadataByKey[sessionKey]
            let bucketStart = roundToTimeBucket(activity.lastMessageAt)
            let key = AIUsageBucketGroupKey(
                source: sessionKey.source,
                sessionKey: sessionKey.sessionID,
                modelKey: "",
                bucketStart: bucketStart
            )
            let externalSessionID = normalizedNonEmptyString(metadata?.externalSessionID) ?? sessionKey.sessionID
            let sessionTitle = preferredTitle(metadata?.sessionTitle, project.name) ?? project.name
            let bucketEnd = calendar.date(byAdding: .minute, value: timeBucketIntervalMinutes, to: bucketStart) ?? bucketStart

            if var bucket = map[key] {
                bucket.activeDurationSeconds += activity.activeSeconds
                bucket.firstSeenAt = min(bucket.firstSeenAt, activity.firstMessageAt)
                bucket.lastSeenAt = max(bucket.lastSeenAt, activity.lastMessageAt)
                map[key] = bucket
            } else {
                map[key] = AIUsageBucket(
                    source: sessionKey.source,
                    sessionKey: sessionKey.sessionID,
                    externalSessionID: externalSessionID,
                    sessionTitle: sessionTitle,
                    model: nil,
                    projectID: project.id,
                    projectName: project.name,
                    bucketStart: bucketStart,
                    bucketEnd: bucketEnd,
                    inputTokens: 0,
                    outputTokens: 0,
                    totalTokens: 0,
                    cachedInputTokens: 0,
                    requestCount: 0,
                    activeDurationSeconds: activity.activeSeconds,
                    firstSeenAt: activity.firstMessageAt,
                    lastSeenAt: activity.lastMessageAt
                )
            }
        }

        return map.values.sorted {
            if $0.bucketStart != $1.bucketStart {
                return $0.bucketStart < $1.bucketStart
            }
            if $0.source != $1.source {
                return $0.source < $1.source
            }
            if $0.sessionKey != $1.sessionKey {
                return $0.sessionKey < $1.sessionKey
            }
            return ($0.model ?? "") < ($1.model ?? "")
        }
    }

    private func makeProjectSummary(
        project: Project,
        usageBuckets: [AIUsageBucket]
    ) -> AIProjectDirectorySourceSummary {
        let sessions = sortSessions(buildSessions(project: project, usageBuckets: usageBuckets))
        let heatmap = buildHeatmap(usageBuckets)
        let todayTimeBuckets = buildTodayTimeBuckets(usageBuckets)
        let toolBreakdown = breakdown(
            items: usageBuckets.map {
                ($0.source, $0.totalTokens, $0.cachedInputTokens, $0.requestCount)
            }
        )
        let modelBreakdown = breakdown(
            items: usageBuckets.compactMap {
                guard let model = normalizedNonEmptyString($0.model) else {
                    return nil
                }
                return (model, $0.totalTokens, $0.cachedInputTokens, $0.requestCount)
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

    private func buildSessions(
        project: Project,
        usageBuckets: [AIUsageBucket]
    ) -> [AISessionSummary] {
        let startOfToday = calendar.startOfDay(for: Date())
        var map: [String: AISessionSummary] = [:]

        for bucket in usageBuckets {
            let groupingKey = "\(bucket.source)|\(bucket.externalSessionID ?? bucket.sessionKey)"
            if var existing = map[groupingKey] {
                let previousLastSeenAt = existing.lastSeenAt
                existing.sessionTitle = preferredTitle(bucket.sessionTitle, existing.sessionTitle) ?? existing.sessionTitle
                existing.firstSeenAt = min(existing.firstSeenAt, bucket.firstSeenAt)
                existing.lastSeenAt = max(existing.lastSeenAt, bucket.lastSeenAt)
                existing.lastTool = bucket.source
                if bucket.lastSeenAt >= previousLastSeenAt,
                   let model = normalizedNonEmptyString(bucket.model) {
                    existing.lastModel = model
                } else if existing.lastModel == nil {
                    existing.lastModel = normalizedNonEmptyString(bucket.model)
                }
                existing.requestCount += bucket.requestCount
                existing.totalInputTokens += bucket.inputTokens
                existing.totalOutputTokens += bucket.outputTokens
                existing.totalTokens += bucket.totalTokens
                existing.cachedInputTokens += bucket.cachedInputTokens
                existing.activeDurationSeconds += bucket.activeDurationSeconds
                if calendar.startOfDay(for: bucket.bucketStart) == startOfToday {
                    existing.todayTokens += bucket.totalTokens
                    existing.todayCachedInputTokens += bucket.cachedInputTokens
                }
                map[groupingKey] = existing
            } else {
                map[groupingKey] = AISessionSummary(
                    sessionID: deterministicUUID(from: "\(bucket.source):\(bucket.externalSessionID ?? bucket.sessionKey)"),
                    externalSessionID: bucket.externalSessionID ?? bucket.sessionKey,
                    projectID: project.id,
                    projectName: project.name,
                    sessionTitle: bucket.sessionTitle,
                    firstSeenAt: bucket.firstSeenAt,
                    lastSeenAt: bucket.lastSeenAt,
                    lastTool: bucket.source,
                    lastModel: normalizedNonEmptyString(bucket.model),
                    requestCount: bucket.requestCount,
                    totalInputTokens: bucket.inputTokens,
                    totalOutputTokens: bucket.outputTokens,
                    totalTokens: bucket.totalTokens,
                    cachedInputTokens: bucket.cachedInputTokens,
                    maxContextUsagePercent: nil,
                    activeDurationSeconds: bucket.activeDurationSeconds,
                    todayTokens: calendar.startOfDay(for: bucket.bucketStart) == startOfToday ? bucket.totalTokens : 0,
                    todayCachedInputTokens: calendar.startOfDay(for: bucket.bucketStart) == startOfToday ? bucket.cachedInputTokens : 0
                )
            }
        }

        return Array(map.values)
    }

    private func buildHeatmap(_ usageBuckets: [AIUsageBucket]) -> [AIHeatmapDay] {
        var map: [Date: AIHeatmapDay] = [:]
        for bucket in usageBuckets {
            let day = calendar.startOfDay(for: bucket.bucketStart)
            if var existing = map[day] {
                existing.totalTokens += bucket.totalTokens
                existing.cachedInputTokens += bucket.cachedInputTokens
                existing.requestCount += bucket.requestCount
                map[day] = existing
            } else {
                map[day] = AIHeatmapDay(
                    day: day,
                    totalTokens: bucket.totalTokens,
                    cachedInputTokens: bucket.cachedInputTokens,
                    requestCount: bucket.requestCount
                )
            }
        }
        return map.values.sorted { $0.day < $1.day }
    }

    private func buildTodayTimeBuckets(_ usageBuckets: [AIUsageBucket]) -> [AITimeBucket] {
        let startOfToday = calendar.startOfDay(for: Date())
        var bucketMap: [Date: AITimeBucket] = [:]

        for bucket in usageBuckets where calendar.startOfDay(for: bucket.bucketStart) == startOfToday {
            if var existing = bucketMap[bucket.bucketStart] {
                existing.totalTokens += bucket.totalTokens
                existing.cachedInputTokens += bucket.cachedInputTokens
                existing.requestCount += bucket.requestCount
                bucketMap[bucket.bucketStart] = existing
            } else {
                bucketMap[bucket.bucketStart] = AITimeBucket(
                    start: bucket.bucketStart,
                    end: bucket.bucketEnd,
                    totalTokens: bucket.totalTokens,
                    cachedInputTokens: bucket.cachedInputTokens,
                    requestCount: bucket.requestCount
                )
            }
        }

        return stride(from: 0, to: 24 * 60, by: timeBucketIntervalMinutes).map { minuteOffset in
            let bucketStart = calendar.date(byAdding: .minute, value: minuteOffset, to: startOfToday)!
            let bucketEnd = calendar.date(byAdding: .minute, value: timeBucketIntervalMinutes, to: bucketStart)!
            return bucketMap[bucketStart] ?? AITimeBucket(
                start: bucketStart,
                end: bucketEnd,
                totalTokens: 0,
                cachedInputTokens: 0,
                requestCount: 0
            )
        }
    }

    private func breakdown(items: [(String, Int, Int, Int)]) -> [AIUsageBreakdownItem] {
        var map: [String: AIUsageBreakdownItem] = [:]
        for item in items {
            if var existing = map[item.0] {
                existing.totalTokens += item.1
                existing.cachedInputTokens += item.2
                existing.requestCount += item.3
                map[item.0] = existing
            } else {
                map[item.0] = AIUsageBreakdownItem(
                    key: item.0,
                    totalTokens: item.1,
                    cachedInputTokens: item.2,
                    requestCount: item.3
                )
            }
        }
        return map.values.sorted { $0.totalTokens > $1.totalTokens }
    }

    private func preferredTitle(_ lhs: String?, _ rhs: String?) -> String? {
        normalizedNonEmptyString(lhs) ?? normalizedNonEmptyString(rhs)
    }

    private func roundToTimeBucket(_ date: Date) -> Date {
        let components = calendar.dateComponents([.year, .month, .day, .hour, .minute], from: date)
        let roundedMinute = ((components.minute ?? 0) / timeBucketIntervalMinutes) * timeBucketIntervalMinutes
        return calendar.date(from: DateComponents(
            year: components.year,
            month: components.month,
            day: components.day,
            hour: components.hour,
            minute: roundedMinute
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

}
