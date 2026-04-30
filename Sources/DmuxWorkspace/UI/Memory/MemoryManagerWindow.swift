import AppKit
import Observation
import SwiftUI

@MainActor
enum MemoryManagerWindowPresenter {
    private static var controller: NSWindowController?

    static func show(model: AppModel) {
        if let window = controller?.window {
            if let hosting = controller?.contentViewController as? NSHostingController<AnyView> {
                hosting.rootView = AnyView(MemoryManagerWindowView(model: model))
            }
            window.makeKeyAndOrderFront(nil)
            NSApp.activate(ignoringOtherApps: true)
            return
        }

        let window = NSWindow(
            contentRect: NSRect(x: 0, y: 0, width: 940, height: 660),
            styleMask: [.titled, .closable, .miniaturizable, .resizable],
            backing: .buffered,
            defer: false
        )
        window.identifier = AppWindowIdentifier.memoryManager
        applyStandardWindowChrome(
            window,
            title: memoryL("memory.manager.window.title", "Memory Manager")
        )
        window.center()
        window.isReleasedWhenClosed = false
        window.minSize = NSSize(width: 820, height: 560)
        let hosting = NSHostingController(rootView: AnyView(MemoryManagerWindowView(model: model)))
        window.contentViewController = hosting
        let controller = NSWindowController(window: window)
        self.controller = controller
        window.makeKeyAndOrderFront(nil)
        NSApp.activate(ignoringOtherApps: true)
    }
}

private struct MemoryManagerWindowView: View {
    let model: AppModel

    @State private var manager = MemoryManagerViewModel()
    @State private var pendingDelete: MemoryManagerDeleteTarget?

    var body: some View {
        HStack(spacing: 0) {
            MemoryManagerSidebar(
                rows: manager.targetRows,
                selectedTarget: manager.selectedTarget,
                onSelect: { target in
                    manager.selectedTarget = target
                    manager.reload(projects: model.projects)
                }
            )
            .frame(width: 260)

            Divider()

            VStack(spacing: 0) {
                MemoryManagerHeader(
                    selectedTab: $manager.selectedTab,
                    selectedTitle: manager.selectedTargetTitle,
                    overview: manager.currentOverview,
                    onReload: { manager.reload(projects: model.projects) }
                )

                Divider()

                MemoryManagerContent(
                    tab: manager.selectedTab,
                    entries: manager.entries,
                    summaries: manager.summaries,
                    errorMessage: manager.errorMessage,
                    onArchiveEntry: { entryID in
                        manager.archiveEntry(entryID, projects: model.projects)
                    },
                    onDeleteEntry: { entryID in
                        pendingDelete = .entry(entryID)
                    },
                    onDeleteSummary: { summaryID in
                        pendingDelete = .summary(summaryID)
                    }
                )
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
        }
        .frame(minWidth: 820, minHeight: 560)
        .background(Color(nsColor: .windowBackgroundColor))
        .onAppear {
            manager.reload(projects: model.projects)
        }
        .onChange(of: manager.selectedTab) { _, _ in
            manager.reload(projects: model.projects)
        }
        .confirmationDialog(
            memoryL("memory.manager.delete.confirm.title", "Delete Memory"),
            isPresented: Binding(
                get: { pendingDelete != nil },
                set: { isPresented in
                    if !isPresented {
                        pendingDelete = nil
                    }
                }
            )
        ) {
            Button(memoryL("common.delete", "Delete"), role: .destructive) {
                if let pendingDelete {
                    manager.delete(pendingDelete, projects: model.projects)
                }
                pendingDelete = nil
            }
            Button(memoryL("common.cancel", "Cancel"), role: .cancel) {}
        } message: {
            Text(memoryL("memory.manager.delete.confirm.message", "This removes the selected memory from the local memory database."))
        }
    }
}

@MainActor
@Observable
private final class MemoryManagerViewModel {
    var targetRows: [MemoryManagerTargetRow] = []
    var selectedTarget: MemoryManagerTarget = .user
    var selectedTargetTitle = memoryL("memory.manager.user_memory", "User Memory")
    var selectedTab: MemoryManagerTab = .summary
    var entries: [MemoryEntry] = []
    var summaries: [MemorySummary] = []
    var currentOverview = MemoryScopeOverview(
        activeEntryCount: 0,
        archivedEntryCount: 0,
        mergedEntryCount: 0,
        summaryCount: 0,
        updatedAt: nil
    )
    var errorMessage: String?

    private let store = MemoryStore()

    func reload(projects: [Project]) {
        do {
            errorMessage = nil
            let projectByID = Dictionary(uniqueKeysWithValues: projects.map { ($0.id, $0) })
            let userOverview = try store.memoryScopeOverview(scope: .user)
            let projectOverviews = try store.projectOverviewsForManagement()

            var rows = [
                MemoryManagerTargetRow(
                    target: .user,
                    title: memoryL("memory.manager.user_memory", "User Memory"),
                    subtitle: memoryL("memory.manager.user_memory.subtitle", "Cross-project preferences"),
                    count: userOverview.totalCount,
                    updatedAt: userOverview.updatedAt
                )
            ]

            rows.append(contentsOf: projectOverviews.map { overview in
                let project = projectByID[overview.projectID]
                return MemoryManagerTargetRow(
                    target: .project(overview.projectID),
                    title: project?.name ?? String(format: memoryL("memory.manager.unknown_project_format", "Project %@"), overview.projectID.uuidString.prefix(8).description),
                    subtitle: project?.path ?? overview.projectID.uuidString,
                    count: overview.totalCount,
                    updatedAt: overview.updatedAt
                )
            })

            targetRows = rows
            if !targetRows.contains(where: { $0.target == selectedTarget }) {
                selectedTarget = .user
            }

            let scope = selectedTarget.scope
            let projectID = selectedTarget.projectID
            selectedTargetTitle = targetRows.first(where: { $0.target == selectedTarget })?.title
                ?? memoryL("memory.manager.user_memory", "User Memory")
            currentOverview = try store.memoryScopeOverview(scope: scope, projectID: projectID)

            switch selectedTab {
            case .summary:
                summaries = try store.listSummariesForManagement(scope: scope, projectID: projectID)
                entries = []
            case .core:
                summaries = []
                entries = try store.listEntriesForManagement(
                    scope: scope,
                    projectID: projectID,
                    tiers: [.core],
                    statuses: [.active],
                    limit: 500
                )
            case .working:
                summaries = []
                entries = try store.listEntriesForManagement(
                    scope: scope,
                    projectID: projectID,
                    tiers: [.working],
                    statuses: [.active],
                    limit: 500
                )
            case .archive:
                summaries = []
                entries = try store.listEntriesForManagement(
                    scope: scope,
                    projectID: projectID,
                    statuses: [.archived, .merged],
                    limit: 500
                )
            }
        } catch {
            errorMessage = error.localizedDescription
            entries = []
            summaries = []
        }
    }

    func archiveEntry(_ entryID: UUID, projects: [Project]) {
        do {
            try store.archiveEntry(entryID)
            reload(projects: projects)
        } catch {
            errorMessage = error.localizedDescription
        }
    }

    func delete(_ target: MemoryManagerDeleteTarget, projects: [Project]) {
        do {
            switch target {
            case let .entry(entryID):
                try store.deleteEntry(entryID)
            case let .summary(summaryID):
                try store.deleteSummary(summaryID)
            }
            reload(projects: projects)
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

private struct MemoryManagerSidebar: View {
    let rows: [MemoryManagerTargetRow]
    let selectedTarget: MemoryManagerTarget
    let onSelect: (MemoryManagerTarget) -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            VStack(alignment: .leading, spacing: 4) {
                HStack(spacing: 8) {
                    Image(systemName: "brain.head.profile")
                        .font(.system(size: 15, weight: .semibold))
                    Text(memoryL("memory.manager.title", "Memory"))
                        .font(.system(size: 17, weight: .bold))
                }
                Text(memoryL("memory.manager.subtitle", "Browse and clean extracted memories"))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
            .padding(16)

            Divider()

            ScrollView {
                VStack(spacing: 6) {
                    ForEach(rows) { row in
                        Button {
                            onSelect(row.target)
                        } label: {
                            MemoryManagerSidebarRow(
                                row: row,
                                isSelected: row.target == selectedTarget
                            )
                        }
                        .buttonStyle(.plain)
                        .contentShape(Rectangle())
                    }
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(10)
            }
        }
        .background(Color.primary.opacity(0.025))
    }
}

private struct MemoryManagerSidebarRow: View {
    let row: MemoryManagerTargetRow
    let isSelected: Bool

    var body: some View {
        HStack(spacing: 10) {
            Image(systemName: row.target == .user ? "person.crop.circle" : "folder")
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(isSelected ? AppTheme.focus : .secondary)
                .frame(width: 18)

            VStack(alignment: .leading, spacing: 2) {
                Text(row.title)
                    .font(.system(size: 13, weight: .semibold))
                    .lineLimit(1)
                Text(row.subtitle)
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
                    .truncationMode(.middle)
            }

            Spacer(minLength: 8)

            Text("\(row.count)")
                .font(.system(size: 11, weight: .semibold, design: .rounded))
                .foregroundStyle(isSelected ? AppTheme.focus : .secondary)
                .monospacedDigit()
                .padding(.horizontal, 7)
                .padding(.vertical, 3)
                .background(
                    Capsule().fill((isSelected ? AppTheme.focus : Color.primary).opacity(0.10))
                )
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 9)
        .background(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .fill(isSelected ? AppTheme.focus.opacity(0.10) : Color.clear)
        )
        .overlay(
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .stroke(isSelected ? AppTheme.focus.opacity(0.22) : Color.clear, lineWidth: 0.5)
        )
        .frame(maxWidth: .infinity, alignment: .leading)
        .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

private struct MemoryManagerHeader: View {
    @Binding var selectedTab: MemoryManagerTab
    let selectedTitle: String
    let overview: MemoryScopeOverview
    let onReload: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(spacing: 12) {
                VStack(alignment: .leading, spacing: 3) {
                    Text(selectedTitle)
                        .font(.system(size: 20, weight: .bold))
                    Text(overviewText)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }

                Spacer()

                Button {
                    onReload()
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .buttonStyle(.borderless)
                .help(memoryL("common.refresh", "Refresh"))
            }

            HStack(spacing: 12) {
                Picker("", selection: $selectedTab) {
                    ForEach(MemoryManagerTab.allCases) { tab in
                        Text(tab.title).tag(tab)
                    }
                }
                .pickerStyle(.segmented)
                .frame(maxWidth: 520)

                Spacer(minLength: 0)
            }
        }
        .padding(18)
    }

    private var overviewText: String {
        let format = memoryL(
            "memory.manager.overview_format",
            "%lld active, %lld archived, %lld summaries"
        )
        return String(
            format: format,
            Int64(overview.activeEntryCount),
            Int64(overview.archivedEntryCount + overview.mergedEntryCount),
            Int64(overview.summaryCount)
        )
    }
}

private struct MemoryManagerContent: View {
    let tab: MemoryManagerTab
    let entries: [MemoryEntry]
    let summaries: [MemorySummary]
    let errorMessage: String?
    let onArchiveEntry: (UUID) -> Void
    let onDeleteEntry: (UUID) -> Void
    let onDeleteSummary: (UUID) -> Void

    var body: some View {
        Group {
            if let errorMessage {
                MemoryManagerEmptyState(
                    symbol: "exclamationmark.triangle",
                    title: memoryL("memory.manager.error", "Memory could not be loaded"),
                    detail: errorMessage
                )
            } else if tab == .summary {
                if summaries.isEmpty {
                    MemoryManagerEmptyState(
                        symbol: "doc.text",
                        title: memoryL("memory.manager.empty.summary", "No summary memory"),
                        detail: memoryL("memory.manager.empty.summary.detail", "Summaries appear after extraction has enough useful context.")
                    )
                } else {
                    ScrollView {
                        LazyVStack(spacing: 12) {
                            ForEach(summaries) { summary in
                                MemorySummaryCard(
                                    summary: summary,
                                    onDelete: { onDeleteSummary(summary.id) }
                                )
                            }
                        }
                        .padding(18)
                    }
                }
            } else if entries.isEmpty {
                MemoryManagerEmptyState(
                    symbol: "tray",
                    title: memoryL("memory.manager.empty.entries", "No memories in this view"),
                    detail: memoryL("memory.manager.empty.entries.detail", "Try another type tab or wait for memory extraction to complete.")
                )
            } else {
                ScrollView {
                    LazyVStack(spacing: 12) {
                        ForEach(entries) { entry in
                            MemoryEntryCard(
                                entry: entry,
                                canArchive: entry.status == .active,
                                onArchive: { onArchiveEntry(entry.id) },
                                onDelete: { onDeleteEntry(entry.id) }
                            )
                        }
                    }
                    .padding(18)
                }
            }
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color.primary.opacity(0.012))
    }
}

private struct MemorySummaryCard: View {
    let summary: MemorySummary
    let onDelete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 10) {
                MemoryManagerBadge(
                    text: String(format: memoryL("memory.manager.summary.version_format", "v%lld"), Int64(summary.version)),
                    color: AppTheme.focus
                )
                MemoryManagerBadge(
                    text: String(format: memoryL("memory.manager.summary.tokens_format", "%lld tokens"), Int64(summary.tokenEstimate)),
                    color: Color(nsColor: .secondaryLabelColor)
                )
                Spacer()
                Text(summary.updatedAt.formatted(date: .abbreviated, time: .shortened))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)
                Button(role: .destructive) {
                    onDelete()
                } label: {
                    Image(systemName: "trash")
                }
                .buttonStyle(.borderless)
                .help(memoryL("common.delete", "Delete"))
            }

            Text(summary.content)
                .font(.system(size: 13))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)

            if !summary.sourceEntryIDs.isEmpty {
                Text(String(format: memoryL("memory.manager.summary.sources_format", "%lld source entries"), Int64(summary.sourceEntryIDs.count)))
                    .font(.system(size: 11))
                    .foregroundStyle(.secondary)
            }
        }
        .padding(14)
        .background(MemoryManagerCardBackground())
    }
}

private struct MemoryEntryCard: View {
    let entry: MemoryEntry
    let canArchive: Bool
    let onArchive: () -> Void
    let onDelete: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack(alignment: .top, spacing: 8) {
                MemoryManagerBadge(text: entry.kind.title, color: entry.kind.color)
                MemoryManagerBadge(text: entry.tier.title, color: entry.tier.color)
                MemoryManagerBadge(text: entry.status.title, color: entry.status.color)

                if let sourceTool = entry.sourceTool, !sourceTool.isEmpty {
                    MemoryManagerBadge(text: sourceTool, color: Color(nsColor: .secondaryLabelColor))
                }

                Spacer()

                Text(entry.updatedAt.formatted(date: .abbreviated, time: .shortened))
                    .font(.system(size: 11))
                    .foregroundStyle(.tertiary)

                if canArchive {
                    Button {
                        onArchive()
                    } label: {
                        Image(systemName: "archivebox")
                    }
                    .buttonStyle(.borderless)
                    .help(memoryL("memory.manager.archive", "Archive"))
                }

                Button(role: .destructive) {
                    onDelete()
                } label: {
                    Image(systemName: "trash")
                }
                .buttonStyle(.borderless)
                .help(memoryL("common.delete", "Delete"))
            }

            Text(entry.content)
                .font(.system(size: 13))
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)

            if let rationale = entry.rationale, !rationale.isEmpty {
                Text(rationale)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .fixedSize(horizontal: false, vertical: true)
            }
        }
        .padding(14)
        .background(MemoryManagerCardBackground())
    }
}

private struct MemoryManagerBadge: View {
    let text: String
    let color: Color

    var body: some View {
        Text(text)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(color)
            .lineLimit(1)
            .padding(.horizontal, 7)
            .padding(.vertical, 3)
            .background(Capsule().fill(color.opacity(0.11)))
    }
}

private struct MemoryManagerCardBackground: View {
    var body: some View {
        RoundedRectangle(cornerRadius: 8, style: .continuous)
            .fill(Color(nsColor: .controlBackgroundColor))
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(Color.primary.opacity(0.07), lineWidth: 0.5)
            )
    }
}

private struct MemoryManagerEmptyState: View {
    let symbol: String
    let title: String
    let detail: String

    var body: some View {
        VStack(spacing: 10) {
            Image(systemName: symbol)
                .font(.system(size: 34, weight: .semibold))
                .foregroundStyle(.secondary)
            Text(title)
                .font(.system(size: 14, weight: .semibold))
            Text(detail)
                .font(.system(size: 12))
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 360)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .padding(30)
    }
}

private struct MemoryManagerTargetRow: Identifiable, Equatable {
    var target: MemoryManagerTarget
    var title: String
    var subtitle: String
    var count: Int
    var updatedAt: Date?

    var id: String { target.id }
}

private enum MemoryManagerTarget: Hashable {
    case user
    case project(UUID)

    var id: String {
        switch self {
        case .user:
            return "user"
        case let .project(projectID):
            return "project-\(projectID.uuidString)"
        }
    }

    var scope: MemoryScope {
        switch self {
        case .user:
            return .user
        case .project:
            return .project
        }
    }

    var projectID: UUID? {
        switch self {
        case .user:
            return nil
        case let .project(projectID):
            return projectID
        }
    }
}

private enum MemoryManagerTab: String, CaseIterable, Identifiable {
    case summary
    case core
    case working
    case archive

    var id: String { rawValue }

    var title: String {
        switch self {
        case .summary:
            return memoryL("memory.manager.tab.summary", "Summary")
        case .core:
            return memoryL("memory.manager.tab.core", "Core")
        case .working:
            return memoryL("memory.manager.tab.working", "Working")
        case .archive:
            return memoryL("memory.manager.tab.archive", "Archive")
        }
    }
}

private enum MemoryManagerDeleteTarget: Identifiable {
    case entry(UUID)
    case summary(UUID)

    var id: String {
        switch self {
        case let .entry(id):
            return "entry-\(id.uuidString)"
        case let .summary(id):
            return "summary-\(id.uuidString)"
        }
    }
}

private extension MemoryTier {
    var title: String {
        switch self {
        case .core:
            return memoryL("memory.tier.core", "Core")
        case .working:
            return memoryL("memory.tier.working", "Working")
        case .archive:
            return memoryL("memory.tier.archive", "Archive")
        }
    }

    var color: Color {
        switch self {
        case .core:
            return AppTheme.focus
        case .working:
            return Color(hex: 0x2E9B5F)
        case .archive:
            return Color(nsColor: .secondaryLabelColor)
        }
    }
}

private extension MemoryKind {
    var title: String {
        switch self {
        case .preference:
            return memoryL("memory.kind.preference", "Preference")
        case .convention:
            return memoryL("memory.kind.convention", "Convention")
        case .decision:
            return memoryL("memory.kind.decision", "Decision")
        case .fact:
            return memoryL("memory.kind.fact", "Fact")
        case .bugLesson:
            return memoryL("memory.kind.bug_lesson", "Bug Lesson")
        }
    }

    var color: Color {
        switch self {
        case .preference:
            return Color(hex: 0x8C6FF7)
        case .convention:
            return Color(hex: 0x2F7FBD)
        case .decision:
            return Color(hex: 0xB8781D)
        case .fact:
            return Color(hex: 0x337A6B)
        case .bugLesson:
            return Color(hex: 0xC25555)
        }
    }
}

private extension MemoryEntryStatus {
    var title: String {
        switch self {
        case .active:
            return memoryL("memory.status.active", "Active")
        case .merged:
            return memoryL("memory.status.merged", "Merged")
        case .archived:
            return memoryL("memory.status.archived", "Archived")
        }
    }

    var color: Color {
        switch self {
        case .active:
            return Color(hex: 0x2E9B5F)
        case .merged:
            return Color(hex: 0x6E6E8B)
        case .archived:
            return Color(nsColor: .secondaryLabelColor)
        }
    }
}
