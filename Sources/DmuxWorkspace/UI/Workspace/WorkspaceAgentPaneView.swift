import AppKit
import QuartzCore
import SwiftUI

struct AgentPaneView: View {
    let model: AppModel
    let session: TerminalSession
    let isFocused: Bool
    let isVisible: Bool
    let showsInactiveOverlay: Bool
    let onSelect: () -> Void
    let onClose: () -> Void
    let showsCloseButton: Bool

    @State private var inputFocused = false
    @State private var inputComposing = false
    @State private var draftText = ""
    @State private var expandedContentIDs: Set<String> = []
    @State private var collapsedContentIDs: Set<String> = []
    @State private var virtualRowHeights: [String: CGFloat] = [:]
    @State private var messageScrollOffset: CGFloat = 0
    @State private var messageViewportHeight: CGFloat = 1
    @State private var lastAutoTailScrollAt = Date.distantPast

    private var state: AgentSessionState {
        model.agentState(for: session)
    }

    private var draft: Binding<String> {
        Binding(
            get: { draftText },
            set: { value in
                draftText = value
                model.updateAgentDraft(value, for: session.id)
            }
        )
    }

    private var canSend: Bool {
        state.runState != .running && !draftText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var supportedTool: AppSupportedAITool {
        state.tool.supportedAITool
    }

    private var permissionMode: AppAIToolPermissionMode {
        supportedTool.permissionMode(from: model.appSettings.ai.runtimeTools)
    }

    private var currentModel: String {
        model.appSettings.ai.runtimeTools.model(for: supportedTool)
    }

    private var presentationEntries: [AgentTimelinePresentationEntry] {
        AgentTimelinePresentation.entries(from: visibleTimelineItems)
    }

    private var messageListItems: [AgentMessageListItem] {
        let entries = presentationEntries
        var items = entries.map(AgentMessageListItem.entry)
        if state.fileChanges.isEmpty == false {
            items.append(.changes)
        }
        let hasRunningActivityGroup = entries.contains { entry in
            if case .activity(let group) = entry {
                return group.status == .running
            }
            return false
        }
        if state.runState == .running, hasRunningActivityGroup == false {
            items.append(.working)
        }
        return items
    }

    private var messageListItemIDs: [String] {
        messageListItems.map(\.id)
    }

    private var virtualListLayout: AgentVirtualListLayout {
        AgentVirtualListLayoutCalculator.layout(
            itemIDs: messageListItemIDs,
            measuredHeights: virtualRowHeights,
            spacing: 18,
            viewportHeight: messageViewportHeight,
            scrollOffset: messageScrollOffset
        )
    }

    private var isMessageListNearTail: Bool {
        messageScrollOffset + messageViewportHeight >= virtualListLayout.totalContentHeight - 180
    }

    var body: some View {
        VStack(spacing: 0) {
            header

            Divider().opacity(0.35)

            messageList

            inputBar
        }
        .background(model.terminalChromeColor)
        .overlay {
            if showsInactiveOverlay && !isFocused {
                Color(nsColor: model.terminalInactiveDimColor)
                    .allowsHitTesting(false)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            selectAgentPane()
        }
        .onAppear {
            draftText = model.agentDraft(for: session.id)
        }
        .onChange(of: session.id) { _, _ in
            draftText = model.agentDraft(for: session.id)
            virtualRowHeights.removeAll(keepingCapacity: true)
            messageScrollOffset = 0
            messageViewportHeight = 1
        }
        .onChange(of: isFocused) { _, focused in
            if !focused {
                inputFocused = false
            }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            Image(systemName: state.tool.symbolName)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AppTheme.focus)

            Text(state.tool.displayName)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AppTheme.textPrimary)

            AgentStatusDot(state: state.runState)

            Spacer(minLength: 0)

            if showsCloseButton {
                Button(action: onClose) {
                    Image(systemName: "xmark")
                        .font(.system(size: 10, weight: .bold))
                        .frame(width: 22, height: 22)
                }
                .buttonStyle(.plain)
                .foregroundStyle(AppTheme.textSecondary)
                .help(String(localized: "common.close", defaultValue: "Close", bundle: .module))
            }
        }
        .padding(.horizontal, 12)
        .frame(height: 42)
    }

    private var messageList: some View {
        ScrollViewReader { proxy in
            if visibleTimelineItems.isEmpty, state.fileChanges.isEmpty, state.runState != .running {
                ScrollView {
                    emptyState
                        .padding(.horizontal, 18)
                        .padding(.vertical, 16)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            } else {
                AgentMessageVirtualListView(
                    items: messageListItems,
                    measuredHeights: virtualRowHeights,
                    scrollOffset: messageScrollOffset,
                    viewportHeight: messageViewportHeight,
                    spacing: 18,
                    tailID: AgentMessageListConstants.tailID,
                    content: { item in
                        messageListRow(for: item)
                    }
                )
                .onPreferenceChange(AgentMessageScrollOffsetPreferenceKey.self) { value in
                    updateMessageScrollOffset(value)
                }
                .onPreferenceChange(AgentMessageViewportHeightPreferenceKey.self) { value in
                    updateMessageViewportHeight(value)
                }
                .onPreferenceChange(AgentMessageRowHeightPreferenceKey.self) { values in
                    updateVirtualRowHeights(values)
                }
                .onChange(of: messageListItemIDs) { _, _ in
                    pruneVirtualRowHeights()
                    lastAutoTailScrollAt = .distantPast
                    scrollToTail(proxy: proxy, animated: false)
                }
                .onChange(of: state.fileChanges.count) { _, _ in
                    lastAutoTailScrollAt = .distantPast
                    scrollToTail(proxy: proxy, animated: false)
                }
                .onChange(of: state.runState) { _, _ in
                    lastAutoTailScrollAt = .distantPast
                    scrollToTail(proxy: proxy, animated: false)
                }
                .onChange(of: state.updatedAt) { _, _ in
                    if state.runState == .running, isMessageListNearTail {
                        scrollToTailIfNeeded(proxy: proxy)
                    }
                }
            }
        }
    }

    @ViewBuilder
    private func messageListRow(for item: AgentMessageListItem) -> some View {
        switch item {
        case .entry(let entry):
            AgentTimelineEntryRow(
                entry: entry,
                runStartedAt: state.runStartedAt,
                runCompletedAt: state.runCompletedAt,
                expansionState: expansionBinding(for: entry.id),
                itemExpansionState: itemExpansionBinding,
                onOpenPath: openAgentPath
            )
        case .changes:
            AgentInlineChangesBlock(
                changes: state.fileChanges,
                expansionState: expansionBinding(for: AgentMessageListConstants.changesID),
                changeExpansionState: changeExpansionBinding,
                onReview: { model.reviewAgentChanges(session: session) },
                onReviewFile: { model.reviewAgentChanges(session: session, selectedPath: $0.path) },
                onDiscardAll: { model.discardAllAgentChanges(session: session) },
                onDiscardFile: { model.discardAgentFileChange($0, session: session) }
            )
        case .working:
            AgentWorkingIndicator(
                statusText: state.statusText,
                startedAt: state.runStartedAt
            )
        }
    }

    private func updateMessageScrollOffset(_ value: CGFloat) {
        guard value.isFinite else { return }
        let normalized = max(0, value)
        guard abs(messageScrollOffset - normalized) >= 72 else { return }
        messageScrollOffset = normalized
    }

    private func updateMessageViewportHeight(_ value: CGFloat) {
        guard value.isFinite, value > 0 else { return }
        guard abs(messageViewportHeight - value) >= 1 else { return }
        messageViewportHeight = value
    }

    private func updateVirtualRowHeights(_ values: [String: CGFloat]) {
        guard values.isEmpty == false else { return }
        var next = virtualRowHeights
        var didChange = false
        for (id, height) in values where height.isFinite && height > 0 {
            let normalized = ceil(height)
            guard abs((next[id] ?? 0) - normalized) >= 1 else { continue }
            next[id] = normalized
            didChange = true
        }
        if didChange {
            virtualRowHeights = next
        }
    }

    private func pruneVirtualRowHeights() {
        let ids = Set(messageListItemIDs)
        virtualRowHeights = virtualRowHeights.filter { ids.contains($0.key) }
    }

    private func expansionBinding(for id: String) -> Binding<AgentContentFoldState> {
        Binding(
            get: {
                if expandedContentIDs.contains(id) {
                    return .expanded
                }
                if collapsedContentIDs.contains(id) {
                    return .collapsed
                }
                return .automatic
            },
            set: { value in
                switch value {
                case .automatic:
                    expandedContentIDs.remove(id)
                    collapsedContentIDs.remove(id)
                case .expanded:
                    expandedContentIDs.insert(id)
                    collapsedContentIDs.remove(id)
                case .collapsed:
                    collapsedContentIDs.insert(id)
                    expandedContentIDs.remove(id)
                }
            }
        )
    }

    private func itemExpansionBinding(_ itemID: String) -> Binding<AgentContentFoldState> {
        expansionBinding(for: itemID)
    }

    private func changeExpansionBinding(_ changeID: String) -> Binding<AgentContentFoldState> {
        expansionBinding(for: changeID)
    }

    private func scrollToTail(proxy: ScrollViewProxy, animated: Bool = false) {
        guard animated else {
            proxy.scrollTo(AgentMessageListConstants.tailID, anchor: .bottom)
            return
        }
        withAnimation(.easeOut(duration: 0.12)) {
            proxy.scrollTo(AgentMessageListConstants.tailID, anchor: .bottom)
        }
    }

    private func scrollToTailIfNeeded(proxy: ScrollViewProxy) {
        let now = Date()
        guard now.timeIntervalSince(lastAutoTailScrollAt) >= 0.25 else { return }
        lastAutoTailScrollAt = now
        scrollToTail(proxy: proxy, animated: false)
    }

    private var visibleTimelineItems: [AgentTimelineItem] {
        let baseItems: [AgentTimelineItem]
        if state.timelineItems.isEmpty == false {
            baseItems = state.timelineItems
        } else {
            baseItems = state.messages.map { message in
                AgentTimelineItem(
                    id: message.id.uuidString,
                    turnID: nil,
                    itemID: nil,
                    kind: timelineKind(for: message.role),
                    role: message.role,
                    title: nil,
                    content: message.content,
                    detail: nil,
                    status: message.role == .error ? .failed : .completed,
                    createdAt: message.createdAt,
                    updatedAt: message.createdAt
                )
            }
        }
        return baseItems.filter { $0.kind != .status }
    }

    private func timelineKind(for role: AgentRole) -> AgentTimelineKind {
        switch role {
        case .user:
            return .userPrompt
        case .assistant:
            return .assistantMessage
        case .system:
            return .status
        case .tool:
            return .tool
        case .error:
            return .error
        }
    }

    private func openAgentPath(_ path: String) {
        let trimmedPath = path.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmedPath.isEmpty == false else { return }
        let rootPath = model.worktrees.first(where: { $0.id == session.projectID })?.path
            ?? model.selectedWorktree?.path
            ?? model.selectedProject?.path
            ?? session.cwd
        let rootURL = URL(fileURLWithPath: rootPath, isDirectory: true).standardizedFileURL
        let fileURL = URL(fileURLWithPath: trimmedPath, relativeTo: rootURL).standardizedFileURL
        guard FileManager.default.fileExists(atPath: fileURL.path) else {
            return
        }
        model.selectedWorktreeID = session.projectID
        model.openFileInWorkspace(fileURL, rootURL: rootURL)
    }

    private var emptyState: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(String(localized: "agent.empty.title", defaultValue: "Agent Ready", bundle: .module))
                .font(.system(size: 14, weight: .semibold))
                .foregroundStyle(AppTheme.textPrimary)

            Text(String(localized: "agent.empty.message", defaultValue: "Send a task. Codux will render structured events from the selected CLI driver here.", bundle: .module))
                .font(.system(size: 13, weight: .regular))
                .foregroundStyle(AppTheme.textSecondary)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(14)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .tertiarySystemFill).opacity(0.35))
        .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
    }

    private var inputBar: some View {
        VStack(spacing: 0) {
            AppMultilineEditor(
                text: draft,
                placeholder: String(localized: "agent.input.placeholder", defaultValue: "Ask the agent to work on this project...", bundle: .module),
                isFocused: $inputFocused,
                isComposing: $inputComposing,
                font: .systemFont(ofSize: 14, weight: .regular),
                horizontalInset: 14,
                verticalInset: 12,
                enablesSpellChecking: true,
                allowsProgrammaticFocus: false,
                resignsOnExternalMouseDown: true,
                onFocusRequest: selectAgentPane
            )
            .frame(height: 86)

            Divider().opacity(0.22)

            composerToolbar
        }
        .background(AppTheme.inputFill)
        .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .strokeBorder(AppTheme.inputBorder(isFocused: false, isHovered: false), lineWidth: 1)
        }
        .padding(12)
    }

    private var composerToolbar: some View {
        HStack(spacing: 6) {
            permissionMenu
            modelControl

            if state.tool == .codex {
                effortMenu
            }

            Spacer(minLength: 0)

            sendButton
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 8)
    }

    private var sendButton: some View {
        Button {
            if state.runState == .running {
                stopAgentRun()
            } else {
                sendDraft()
            }
        } label: {
            Image(systemName: state.runState == .running ? "stop.fill" : "paperplane.fill")
                .font(.system(size: 13, weight: .bold))
                .foregroundStyle(.white)
                .frame(width: 30, height: 30)
                .background(sendButtonFill)
                .clipShape(Circle())
        }
        .buttonStyle(.plain)
        .disabled(state.runState != .running && !canSend)
        .keyboardShortcut(.return, modifiers: .command)
        .appCursor((state.runState == .running || canSend) ? .pointingHand : .arrow)
        .help(sendButtonHelp)
    }

    private var sendButtonFill: Color {
        if state.runState == .running {
            return AppTheme.warning
        }
        return canSend ? AppTheme.focus : AppTheme.textMuted.opacity(0.55)
    }

    private var sendButtonHelp: String {
        state.runState == .running
            ? String(localized: "agent.stop", defaultValue: "Stop", bundle: .module)
            : String(localized: "agent.send", defaultValue: "Send", bundle: .module)
    }

    private var permissionMenu: some View {
        Menu {
            ForEach(AppAIToolPermissionMode.allCases) { mode in
                Button {
                    model.updateToolPermissionMode(mode, for: supportedTool)
                } label: {
                    if mode == permissionMode {
                        Label(mode.title, systemImage: "checkmark")
                    } else {
                        Text(mode.title)
                    }
                }
            }
        } label: {
            Label(permissionMode.title, systemImage: permissionMode == .fullAccess ? "lock.open.fill" : "lock.fill")
        }
        .menuStyle(.borderlessButton)
        .buttonStyle(AgentComposerPillButtonStyle(tint: permissionMode == .fullAccess ? AppTheme.warning : AppTheme.textSecondary))
        .help(String(localized: "agent.composer.permission.help", defaultValue: "Permission mode", bundle: .module))
    }

    private var modelControl: some View {
        Menu {
            Button {
                model.updateToolDefaultModel("", for: supportedTool)
            } label: {
                Label(
                    String(localized: "agent.composer.model.default", defaultValue: "Default Model", bundle: .module),
                    systemImage: currentModel.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ? "checkmark" : "circle"
                )
            }

            if state.tool.modelPresets.isEmpty == false {
                Divider()
                ForEach(state.tool.modelPresets, id: \.self) { preset in
                    Button {
                        model.updateToolDefaultModel(preset, for: supportedTool)
                    } label: {
                        if currentModel == preset {
                            Label(preset, systemImage: "checkmark")
                        } else {
                            Text(preset)
                        }
                    }
                }
            }
        } label: {
            Label(modelMenuTitle, systemImage: "cpu")
        }
        .menuStyle(.borderlessButton)
        .buttonStyle(AgentComposerPillButtonStyle(tint: AppTheme.focus))
        .help(String(localized: "agent.composer.model.label", defaultValue: "Model", bundle: .module))
    }

    private var modelMenuTitle: String {
        let trimmed = currentModel.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.isEmpty {
            return String(localized: "agent.composer.model.default", defaultValue: "Default Model", bundle: .module)
        }
        return trimmed
    }

    private var effortMenu: some View {
        Menu {
            ForEach(AppAICodexReasoningEffort.allCases) { effort in
                Button {
                    model.updateCodexReasoningEffort(effort)
                } label: {
                    if model.appSettings.ai.runtimeTools.codexEffort == effort {
                        Label(effort.title, systemImage: "checkmark")
                    } else {
                        Text(effort.title)
                    }
                }
            }
        } label: {
            Label(model.appSettings.ai.runtimeTools.codexEffort.title, systemImage: "dial.medium")
        }
        .menuStyle(.borderlessButton)
        .buttonStyle(AgentComposerPillButtonStyle(tint: AppTheme.focus))
        .help(String(localized: "agent.composer.effort.help", defaultValue: "Reasoning effort", bundle: .module))
    }

    private func sendDraft() {
        selectAgentPane()
        let prompt = draftText
        model.debugLog.log(
            "agent-driver",
            "send-click session=\(session.id.uuidString) tool=\(state.tool.rawValue) length=\(prompt.trimmingCharacters(in: .whitespacesAndNewlines).count)"
        )
        model.sendAgentMessage(
            session: session,
            prompt: prompt,
            model: currentModel,
            fullAccess: permissionMode == .fullAccess,
            reasoningEffort: state.tool == .codex ? model.appSettings.ai.runtimeTools.codexEffort : nil
        )
        draftText = model.agentDraft(for: session.id)
        inputFocused = true
    }

    private func stopAgentRun() {
        selectAgentPane()
        model.stopAgentRun(sessionID: session.id)
        inputFocused = false
    }

    private func selectAgentPane() {
        onSelect()
    }
}

private struct AgentComposerPillButtonStyle: ButtonStyle {
    let tint: Color

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(tint)
            .labelStyle(.titleAndIcon)
            .padding(.horizontal, 8)
            .frame(height: 26)
            .background(tint.opacity(configuration.isPressed ? 0.18 : 0.08))
            .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
            .appCursor(.pointingHand)
    }
}

private enum AgentMessageListConstants {
    static let changesID = "agent-changes-block"
    static let workingID = "agent-working-indicator"
    static let tailID = "agent-message-list-tail"
}

private enum AgentMessageListItem: Identifiable, Equatable {
    case entry(AgentTimelinePresentationEntry)
    case changes
    case working

    var id: String {
        switch self {
        case .entry(let entry):
            return entry.id
        case .changes:
            return AgentMessageListConstants.changesID
        case .working:
            return AgentMessageListConstants.workingID
        }
    }
}

private struct AgentMessageVirtualListView<RowContent: View>: View {
    static var coordinateSpaceName: String { "agent-message-virtual-list" }

    let items: [AgentMessageListItem]
    let measuredHeights: [String: CGFloat]
    let scrollOffset: CGFloat
    let viewportHeight: CGFloat
    let spacing: CGFloat
    let tailID: String
    @ViewBuilder let content: (AgentMessageListItem) -> RowContent

    private var layout: AgentVirtualListLayout {
        AgentVirtualListLayoutCalculator.layout(
            itemIDs: items.map(\.id),
            measuredHeights: measuredHeights,
            spacing: spacing,
            viewportHeight: viewportHeight,
            scrollOffset: scrollOffset
        )
    }

    var body: some View {
        ScrollView {
            GeometryReader { proxy in
                Color.clear.preference(
                    key: AgentMessageScrollOffsetPreferenceKey.self,
                    value: max(0, -proxy.frame(in: .named(Self.coordinateSpaceName)).minY)
                )
            }
            .frame(height: 0)

            LazyVStack(alignment: .leading, spacing: 0) {
                Color.clear
                    .frame(height: layout.topSpacerHeight)

                ForEach(items[layout.visibleRange], id: \.id) { item in
                    content(item)
                        .id(item.id)
                        .background(AgentMessageRowHeightReader(id: item.id))
                        .padding(.bottom, item.id == items[layout.visibleRange].last?.id ? 0 : spacing)
                }

                Color.clear
                    .frame(height: layout.bottomSpacerHeight)

                Color.clear
                    .frame(height: 1)
                    .id(tailID)
            }
            .padding(.horizontal, 18)
            .padding(.vertical, 16)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .background(
            GeometryReader { proxy in
                Color.clear.preference(
                    key: AgentMessageViewportHeightPreferenceKey.self,
                    value: proxy.size.height
                )
            }
        )
        .coordinateSpace(name: Self.coordinateSpaceName)
    }
}

private struct AgentMessageRowHeightReader: View {
    let id: String

    var body: some View {
        GeometryReader { proxy in
            Color.clear.preference(
                key: AgentMessageRowHeightPreferenceKey.self,
                value: [id: proxy.size.height]
            )
        }
    }
}

private struct AgentMessageRowHeightPreferenceKey: PreferenceKey {
    static let defaultValue: [String: CGFloat] = [:]

    static func reduce(value: inout [String: CGFloat], nextValue: () -> [String: CGFloat]) {
        value.merge(nextValue()) { _, new in new }
    }
}

private struct AgentMessageScrollOffsetPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct AgentMessageViewportHeightPreferenceKey: PreferenceKey {
    static let defaultValue: CGFloat = 0

    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value = nextValue()
    }
}

private struct AgentTimelineEntryRow: View {
    let entry: AgentTimelinePresentationEntry
    let runStartedAt: Date?
    let runCompletedAt: Date?
    @Binding var expansionState: AgentContentFoldState
    let itemExpansionState: (String) -> Binding<AgentContentFoldState>
    let onOpenPath: (String) -> Void

    var body: some View {
        switch entry {
        case .item(let item):
            AgentTimelineRow(
                item: item,
                expansionState: $expansionState,
                itemExpansionState: itemExpansionState,
                onOpenPath: onOpenPath
            )
        case .activity(let group):
            AgentActivityGroupView(
                group: group,
                runStartedAt: runStartedAt,
                runCompletedAt: runCompletedAt,
                expansionState: $expansionState,
                itemExpansionState: itemExpansionState,
                onOpenPath: onOpenPath
            )
        }
    }
}

private struct AgentTimelineRow: View {
    let item: AgentTimelineItem
    @Binding var expansionState: AgentContentFoldState
    let itemExpansionState: (String) -> Binding<AgentContentFoldState>
    let onOpenPath: (String) -> Void

    var body: some View {
        switch item.kind {
        case .userPrompt:
            AgentUserMessage(
                content: item.content,
                createdAt: item.createdAt,
                onOpenPath: onOpenPath,
                expansionState: $expansionState
            )
        case .assistantMessage:
            AgentAssistantMessage(
                content: item.content,
                completedAt: item.status == .completed ? item.updatedAt : nil,
                isStreaming: item.status == .running,
                onOpenPath: onOpenPath,
                expansionState: $expansionState
            )
        case .plan:
            AgentLabeledMarkdown(
                label: String(localized: "agent.timeline.plan", defaultValue: "Plan", bundle: .module),
                symbol: "checklist",
                tint: AppTheme.warning,
                content: item.content,
                expansionState: itemExpansionState("\(item.id):plan")
            )
        case .reasoning:
            AgentReasoningBlock(
                content: item.content,
                expansionState: itemExpansionState("\(item.id):reasoning"),
                contentExpansionState: itemExpansionState("\(item.id):reasoning-content")
            )
        case .command:
            AgentCommandBlock(title: item.title, detail: item.detail, output: item.content, expansionState: itemExpansionState("\(item.id):command-output"))
        case .tool:
            AgentToolBlock(title: item.title, content: item.content, expansionState: itemExpansionState("\(item.id):tool-output"))
        case .fileChange:
            AgentInlineNotice(
                symbol: "doc.badge.gearshape",
                tint: AppTheme.focus,
                title: item.title ?? String(localized: "agent.timeline.file_change", defaultValue: "File Change", bundle: .module),
                detail: item.detail,
                content: item.content,
                onOpenPath: onOpenPath
            )
        case .error:
            AgentErrorBlock(content: item.content)
        case .status:
            EmptyView()
        }
    }
}

private struct AgentUserMessage: View {
    let content: String
    let createdAt: Date
    let onOpenPath: (String) -> Void
    @Binding var expansionState: AgentContentFoldState

    var body: some View {
        HStack(spacing: 0) {
            Spacer(minLength: 56)

            VStack(alignment: .trailing, spacing: 6) {
                AgentFoldableMarkdownMessage(
                    content: content,
                    style: .body,
                    cacheContent: true,
                    fitsContentWidth: true,
                    maximumWidth: 560,
                    onOpenPath: onOpenPath,
                    expansionState: $expansionState
                )
                    .padding(.vertical, 9)
                    .padding(.horizontal, 14)
                    .background(
                        RoundedRectangle(cornerRadius: 14, style: .continuous)
                            .fill(Color.white.opacity(0.06))
                    )

                AgentMessageHoverActions(content: content, date: createdAt, showsDate: false)
            }
        }
        .frame(maxWidth: .infinity, alignment: .trailing)
    }
}

private struct AgentActivityGroupView: View {
    let group: AgentActivityGroup
    let runStartedAt: Date?
    let runCompletedAt: Date?
    @Binding var expansionState: AgentContentFoldState
    let itemExpansionState: (String) -> Binding<AgentContentFoldState>
    let onOpenPath: (String) -> Void

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: group.shouldDefaultExpand)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            Button {
                toggleExpanded()
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .rotationEffect(.degrees(effectiveExpanded ? 90 : 0))
                        .frame(width: 12)

                    if group.status == .running {
                        AgentStatusShimmerText(activityTitle)
                            .fixedSize(horizontal: true, vertical: false)
                            .lineLimit(1)
                            .layoutPriority(2)
                    } else {
                        Text(activityTitle)
                            .font(.system(size: 13, weight: .semibold))
                            .foregroundStyle(activityColor)
                            .lineLimit(1)
                            .layoutPriority(2)
                    }

                    activitySubtitle

                    Spacer(minLength: 0)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(activityColor)
            .appCursor(.pointingHand)

            if effectiveExpanded {
                VStack(alignment: .leading, spacing: 5) {
                    ForEach(group.items) { item in
                        AgentActivityItemRow(
                            item: item,
                            expansionState: itemExpansionState(item.id),
                            contentExpansionState: itemExpansionState("\(item.id):content"),
                            nestedExpansionState: itemExpansionState,
                            onOpenPath: onOpenPath,
                            parentIsRunning: group.status == .running
                        )
                    }
                }
                .padding(.leading, 19)
                .transition(.opacity)
            }

            if group.status == .completed {
                Divider()
                    .opacity(0.22)
                    .padding(.leading, 19)
            }
        }
        .padding(.top, group.status == .completed ? 0 : 4)
        .padding(.bottom, 2)
        .frame(maxWidth: .infinity, alignment: .leading)
        .onChange(of: group.status) { _, status in
            if status == .completed, expansionState == .automatic {
                withAnimation(.easeOut(duration: 0.12)) {
                    expansionState = .collapsed
                }
            }
        }
    }

    private func toggleExpanded() {
        withAnimation(.easeOut(duration: 0.12)) {
            expansionState = effectiveExpanded ? .collapsed : .expanded
        }
    }

    private var activityTitle: String {
        switch group.status {
        case .running:
            if group.isThinkingOnly {
                return agentThinkingStatusLabel()
            }
            return agentRunningStatusLabel()
        case .completed:
            return String(localized: "agent.activity.processed", defaultValue: "Processed", bundle: .module)
        case .failed:
            return String(localized: "agent.activity.failed", defaultValue: "Failed", bundle: .module)
        }
    }

    @ViewBuilder
    private var activitySubtitle: some View {
        let parts = activitySubtitleParts
        if group.status == .running {
            AgentRunningActivitySubtitle(
                parts: parts,
                startedAt: runStartedAt,
                fallbackSeconds: group.durationSeconds
            )
        } else {
            Text(activitySubtitleText(duration: completedDurationText, parts: parts))
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(AppTheme.textMuted)
                .lineLimit(1)
        }
    }

    private var activitySubtitleParts: [String] {
        var parts: [String] = []
        if group.reasoningCount > 0 {
            parts.append(
                String(
                    format: String(localized: "agent.activity.reasoning_count_format", defaultValue: "%@ notes", bundle: .module),
                    "\(group.reasoningCount)"
                )
            )
        }
        if group.commandCount > 0 {
            parts.append(
                String(
                    format: String(localized: "agent.activity.command_count_format", defaultValue: "%@ commands", bundle: .module),
                    "\(group.commandCount)"
                )
            )
        }
        if group.toolCount > 0 {
            parts.append(
                String(
                    format: String(localized: "agent.activity.tool_count_format", defaultValue: "%@ tools", bundle: .module),
                    "\(group.toolCount)"
                )
            )
        }
        if group.fileChangeCount > 0 {
            parts.append(
                String(
                    format: String(localized: "agent.activity.file_count_format", defaultValue: "%@ files", bundle: .module),
                    "\(group.fileChangeCount)"
                )
            )
        }

        return parts
    }

    private var completedDurationText: String {
        let seconds: Int
        if let runCompletedAt {
            seconds = max(group.durationSeconds, Int(runCompletedAt.timeIntervalSince(group.createdAt)))
        } else {
            seconds = group.durationSeconds
        }
        return AgentDurationFormatter.shortElapsedText(seconds: seconds)
    }

    private var activityColor: Color {
        switch group.status {
        case .running:
            return AppTheme.warning
        case .completed:
            return AppTheme.textMuted
        case .failed:
            return Color(nsColor: .systemRed)
        }
    }
}

private struct AgentRunningActivitySubtitle: View {
    let parts: [String]
    let startedAt: Date?
    let fallbackSeconds: Int

    @ObservedObject private var clock = AgentElapsedClock.shared

    var body: some View {
        Text(activitySubtitleText(duration: durationText, parts: parts))
            .font(.system(size: 12, weight: .medium))
            .foregroundStyle(AppTheme.textMuted)
            .lineLimit(1)
    }

    private var durationText: String {
        let seconds: Int
        if let startedAt {
            seconds = max(0, Int(clock.now.timeIntervalSince(startedAt)))
        } else {
            seconds = fallbackSeconds
        }
        return AgentDurationFormatter.shortElapsedText(seconds: seconds)
    }
}

private func activitySubtitleText(duration: String, parts: [String]) -> String {
    if parts.isEmpty {
        return duration
    }
    return "\(duration) · \(parts.joined(separator: " · "))"
}

private func agentRunningStatusLabel() -> String {
    String(localized: "agent.status.running_live", defaultValue: "Running...", bundle: .module)
}

private func agentThinkingStatusLabel() -> String {
    String(localized: "agent.status.thinking_live", defaultValue: "Thinking...", bundle: .module)
}

private struct AgentActivityItemRow: View {
    let item: AgentTimelineItem
    @Binding var expansionState: AgentContentFoldState
    @Binding var contentExpansionState: AgentContentFoldState
    let nestedExpansionState: (String) -> Binding<AgentContentFoldState>
    let onOpenPath: (String) -> Void
    let parentIsRunning: Bool

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: parentIsRunning && item.status == .running)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 5) {
            Button {
                withAnimation(.easeOut(duration: 0.12)) {
                    expansionState = effectiveExpanded ? .collapsed : .expanded
                }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 9, weight: .bold))
                        .rotationEffect(.degrees(effectiveExpanded ? 90 : 0))
                        .frame(width: 10)

                    Text(summary)
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(AppTheme.textMuted)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Spacer(minLength: 0)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .foregroundStyle(AppTheme.textMuted)
            .appCursor(.pointingHand)

            if effectiveExpanded {
                AgentActivityItemDetail(
                    item: item,
                    expansionState: $contentExpansionState,
                    contentExpansionState: nestedExpansionState("\(item.id):detail-content"),
                    onOpenPath: onOpenPath
                )
                    .padding(.leading, 30)
                    .transition(.opacity)
            }
        }
        .onChange(of: item.status) { _, status in
            if status == .completed, expansionState == .automatic {
                withAnimation(.easeOut(duration: 0.12)) {
                    expansionState = .collapsed
                }
            }
        }
    }

    private var summary: String {
        switch item.kind {
        case .command:
            return String(
                format: String(localized: "agent.activity.command_summary_format", defaultValue: "Ran %@", bundle: .module),
                normalizedNonEmptyString(item.title) ?? String(localized: "agent.timeline.command", defaultValue: "Command", bundle: .module)
            )
        case .tool:
            return String(
                format: String(localized: "agent.activity.tool_summary_format", defaultValue: "Used %@", bundle: .module),
                normalizedNonEmptyString(item.title) ?? String(localized: "agent.timeline.tool", defaultValue: "Tool", bundle: .module)
            )
        case .fileChange:
            let content = normalizedNonEmptyString(item.content)
            return String(
                format: String(localized: "agent.activity.file_summary_format", defaultValue: "Edited %@", bundle: .module),
                normalizedNonEmptyString(item.title) ?? content?.components(separatedBy: "\n").first ?? String(localized: "agent.timeline.file_change", defaultValue: "File Change", bundle: .module)
            )
        case .plan:
            return String(localized: "agent.activity.plan_summary", defaultValue: "Updated plan", bundle: .module)
        case .reasoning:
            return String(localized: "agent.activity.reasoning_summary", defaultValue: "Thought through context", bundle: .module)
        default:
            return normalizedNonEmptyString(item.title)
                ?? normalizedNonEmptyString(item.content)
                ?? String(localized: "agent.timeline.tool", defaultValue: "Tool", bundle: .module)
        }
    }
}

private struct AgentActivityItemDetail: View {
    let item: AgentTimelineItem
    @Binding var expansionState: AgentContentFoldState
    @Binding var contentExpansionState: AgentContentFoldState
    let onOpenPath: (String) -> Void

    var body: some View {
        switch item.kind {
        case .plan:
            AgentLabeledMarkdown(
                label: String(localized: "agent.timeline.plan", defaultValue: "Plan", bundle: .module),
                symbol: "checklist",
                tint: AppTheme.textMuted,
                content: item.content,
                expansionState: $expansionState
            )
        case .reasoning:
            AgentReasoningBlock(content: item.content, expansionState: $expansionState, contentExpansionState: $contentExpansionState)
        case .command:
            AgentCommandBlock(title: item.title, detail: item.detail, output: item.content, expansionState: $expansionState)
        case .tool:
            AgentToolBlock(title: item.title, content: item.content, expansionState: $expansionState)
        case .fileChange:
            AgentInlineNotice(
                symbol: "doc.badge.gearshape",
                tint: AppTheme.textMuted,
                title: item.title ?? String(localized: "agent.timeline.file_change", defaultValue: "File Change", bundle: .module),
                detail: item.detail,
                content: item.content,
                onOpenPath: onOpenPath
            )
        default:
            AgentTimelineRow(
                item: item,
                expansionState: $expansionState,
                itemExpansionState: { _ in $expansionState },
                onOpenPath: onOpenPath
            )
        }
    }
}

private struct AgentAssistantMessage: View {
    let content: String
    let completedAt: Date?
    let isStreaming: Bool
    let onOpenPath: (String) -> Void
    @Binding var expansionState: AgentContentFoldState
    @State private var isHovering = false

    var body: some View {
        AgentFoldableMarkdownMessage(
            content: content,
            style: .body,
            cacheContent: !isStreaming,
            onOpenPath: onOpenPath,
            expansionState: $expansionState
        )
        .padding(.bottom, 26)
        .overlay(alignment: .bottomLeading) {
            if !isStreaming {
                AgentMessageHoverActions(content: content, date: completedAt, showsDate: completedAt != nil)
                    .opacity(shouldShowActions ? 1 : 0)
                    .allowsHitTesting(shouldShowActions)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .onHover { hovering in
            guard isHovering != hovering else { return }
            isHovering = hovering
        }
    }

    private var shouldShowActions: Bool {
        isHovering && !isStreaming
    }
}

private struct AgentMessageHoverActions: View {
    let content: String
    let date: Date?
    let showsDate: Bool

    var body: some View {
        HStack(spacing: 8) {
            AgentCopyButton {
                copyAgentMessageContent(content)
            }

            if showsDate, let date {
                Text(AgentMessageTimestampFormatter.string(from: date))
                    .font(.system(size: 11, weight: .medium, design: .rounded))
                    .foregroundStyle(AppTheme.textMuted)
                    .lineLimit(1)
            }
        }
    }
}

private func copyAgentMessageContent(_ content: String) {
    NSPasteboard.general.clearContents()
    NSPasteboard.general.setString(content, forType: .string)
}

private struct AgentCopyButton: View {
    let action: () -> Void

    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @State private var isHovering = false

    var body: some View {
        Button(action: action) {
            Image(systemName: "square.on.square")
                .font(.system(size: 11, weight: .semibold))
                .frame(width: 22, height: 22)
                .contentShape(RoundedRectangle(cornerRadius: 6, style: .continuous))
        }
        .buttonStyle(.plain)
        .foregroundStyle(isHovering ? AppTheme.focus : AppTheme.textMuted)
        .background(
            RoundedRectangle(cornerRadius: 6, style: .continuous)
                .fill(AppTheme.focus.opacity(isHovering ? 0.12 : 0))
        )
        .animation(reduceMotion ? nil : .easeOut(duration: 0.12), value: isHovering)
        .help(String(localized: "agent.code.copy", defaultValue: "Copy", bundle: .module))
        .appCursor(.pointingHand)
        .onHover { hovering in
            guard isHovering != hovering else { return }
            isHovering = hovering
        }
    }
}

private enum AgentMessageTimestampFormatter {
    static func string(from date: Date) -> String {
        date.formatted(date: .omitted, time: .shortened)
    }
}

private struct AgentFoldableMarkdownMessage: View {
    let content: String
    let style: AgentMarkdownMessageStyle
    let cacheContent: Bool
    let fitsContentWidth: Bool
    let maximumWidth: CGFloat?
    let onOpenPath: ((String) -> Void)?
    @Binding var expansionState: AgentContentFoldState
    var defaultExpanded = true

    init(
        content: String,
        style: AgentMarkdownMessageStyle,
        cacheContent: Bool,
        fitsContentWidth: Bool = false,
        maximumWidth: CGFloat? = nil,
        onOpenPath: ((String) -> Void)? = nil,
        expansionState: Binding<AgentContentFoldState>,
        defaultExpanded: Bool = true
    ) {
        self.content = content
        self.style = style
        self.cacheContent = cacheContent
        self.fitsContentWidth = fitsContentWidth
        self.maximumWidth = maximumWidth
        self.onOpenPath = onOpenPath
        self._expansionState = expansionState
        self.defaultExpanded = defaultExpanded
    }

    private var preview: AgentMessageTextPreview {
        AgentMessageTextPreviewCache.shared.preview(for: content)
    }

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: defaultExpanded && !preview.shouldFold)
    }

    private var displayedText: String {
        preview.shouldFold && !effectiveExpanded ? preview.previewText : preview.fullText
    }

    private var displayedSegments: [AgentMarkdownBlockSegment] {
        AgentMarkdownBlockParser.segments(from: displayedText)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            ForEach(displayedSegments) { segment in
                switch segment.kind {
                case .markdown(let markdown):
                    AgentSelectableMarkdownText(
                        content: markdown,
                        style: style,
                        cacheContent: cacheContent,
                        renderMode: cacheContent ? .markdown : .streamingMarkdown,
                        fitsContentWidth: fitsContentWidth,
                        maximumWidth: maximumWidth,
                        onOpenLink: onOpenPath
                    )
                case .code(let language, let code):
                    AgentMarkdownCodeSegmentBlock(
                        content: code,
                        language: language
                    )
                }
            }

            if preview.shouldFold {
                foldToggle
            }
        }
        .frame(maxWidth: fitsContentWidth ? nil : .infinity, alignment: .leading)
        .onChange(of: content) { _, _ in
            if expansionState != .automatic, !AgentMessageTextPreviewCache.shared.preview(for: content).shouldFold {
                expansionState = .automatic
            }
        }
    }

    private var foldToggle: some View {
        Button {
            withAnimation(.easeOut(duration: 0.12)) {
                expansionState = effectiveExpanded ? .collapsed : .expanded
            }
        } label: {
            HStack(spacing: 5) {
                Image(systemName: effectiveExpanded ? "chevron.up" : "chevron.down")
                    .font(.system(size: 10, weight: .bold))
                Text(foldLabel)
                    .font(.system(size: 12, weight: .medium))
            }
            .foregroundStyle(AppTheme.focus)
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .appCursor(.pointingHand)
    }

    private var foldLabel: String {
        if effectiveExpanded {
            return String(localized: "agent.code.show_less", defaultValue: "Show less", bundle: .module)
        }
        if preview.omittedLineCount > 0 {
            return String(
                format: String(localized: "agent.code.show_more_format", defaultValue: "Show %@ more lines", bundle: .module),
                preview.hiddenSummary
            )
        }
        return String(
            format: String(localized: "agent.message.show_more_characters_format", defaultValue: "Show %@ more characters", bundle: .module),
            preview.hiddenSummary
        )
    }
}

private struct AgentMarkdownCodeSegmentBlock: View {
    let content: String
    let language: String?

    @State private var expansionState: AgentContentFoldState = .automatic

    var body: some View {
        AgentFoldableCodeBlock(
            content: content,
            language: language,
            expansionState: $expansionState,
            defaultExpanded: true
        )
    }
}

private struct AgentLabeledMarkdown: View {
    let label: String
    let symbol: String
    let tint: Color
    let content: String
    @Binding var expansionState: AgentContentFoldState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Image(systemName: symbol)
                    .font(.system(size: 11, weight: .semibold))
                Text(label)
                    .font(.system(size: 12, weight: .semibold))
                    .textCase(.uppercase)
                    .tracking(0.4)
            }
            .foregroundStyle(tint)

            AgentFoldableMarkdownMessage(
                content: content,
                style: .body,
                cacheContent: true,
                expansionState: $expansionState
            )
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct AgentReasoningBlock: View {
    let content: String
    @Binding var expansionState: AgentContentFoldState
    @Binding var contentExpansionState: AgentContentFoldState

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: false)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button {
                withAnimation(.easeOut(duration: 0.12)) {
                    expansionState = effectiveExpanded ? .collapsed : .expanded
                }
            } label: {
                HStack(spacing: 6) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .rotationEffect(.degrees(effectiveExpanded ? 90 : 0))
                    Image(systemName: "brain.head.profile")
                        .font(.system(size: 11, weight: .semibold))
                    Text(String(localized: "agent.timeline.reasoning", defaultValue: "Reasoning", bundle: .module))
                        .font(.system(size: 12, weight: .semibold))
                        .textCase(.uppercase)
                        .tracking(0.4)
                }
                .foregroundStyle(AppTheme.textMuted)
                .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .appCursor(.pointingHand)

            if effectiveExpanded {
                AgentFoldableMarkdownMessage(
                    content: content,
                    style: .reasoning,
                    cacheContent: true,
                    expansionState: $contentExpansionState,
                    defaultExpanded: true
                )
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }
}

private struct AgentCommandBlock: View {
    let title: String?
    let detail: String?
    let output: String
    @Binding var expansionState: AgentContentFoldState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text(commandTitle)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.middle)

                if let detail, detail.isEmpty == false {
                    Text(detail)
                        .font(.system(size: 12, weight: .medium, design: .rounded))
                        .foregroundStyle(AppTheme.textMuted)
                        .lineLimit(1)
                }

                Spacer(minLength: 0)
            }

            if output.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false {
                if let gitStatus = AgentGitStatusList.parse(output) {
                    AgentGitStatusListView(status: gitStatus)
                } else {
                    AgentFoldableCodeBlock(content: output, language: nil, expansionState: $expansionState)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var commandTitle: String {
        if let title, title.isEmpty == false {
            return "$ \(title)"
        }
        return String(localized: "agent.timeline.command", defaultValue: "Command", bundle: .module)
    }
}

private struct AgentToolBlock: View {
    let title: String?
    let content: String
    @Binding var expansionState: AgentContentFoldState

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack(spacing: 6) {
                Text(toolTitle)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.middle)

                Spacer(minLength: 0)
            }

            if content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false {
                if let gitStatus = AgentGitStatusList.parse(content) {
                    AgentGitStatusListView(status: gitStatus)
                } else {
                    AgentFoldableCodeBlock(content: content, language: nil, expansionState: $expansionState)
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var toolTitle: String {
        if let title, title.isEmpty == false {
            return title
        }
        return String(localized: "agent.timeline.tool", defaultValue: "Tool", bundle: .module)
    }
}

private struct AgentGitStatusListView: View {
    let status: AgentGitStatusList

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ForEach(status.entries) { entry in
                HStack(spacing: 10) {
                    Text(entry.code)
                        .font(.system(size: 12, weight: .bold, design: .monospaced))
                        .foregroundStyle(entry.tint)
                        .frame(width: 26, alignment: .center)

                    Text(entry.path)
                        .font(.system(size: 12.5, weight: .semibold, design: .monospaced))
                        .foregroundStyle(AppTheme.textPrimary)
                        .lineLimit(1)
                        .truncationMode(.middle)

                    Spacer(minLength: 0)
                }
                .padding(.horizontal, 10)
                .frame(height: 28)

                if entry.id != status.entries.last?.id {
                    Divider().opacity(0.18)
                }
            }
        }
        .background(Color.black.opacity(0.18))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(AppTheme.separator.opacity(0.24), lineWidth: 1)
        }
    }
}

private struct AgentInlineNotice: View {
    let symbol: String
    let tint: Color
    let title: String
    let detail: String?
    let content: String
    let onOpenPath: (String) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: symbol)
                .font(.system(size: 11, weight: .semibold))
                .foregroundStyle(tint)
                .padding(.top, 2)

            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(title)
                        .font(.system(size: 12, weight: .semibold))
                        .foregroundStyle(AppTheme.textPrimary)
                    if let detail, detail.isEmpty == false {
                        Text(detail)
                            .font(.system(size: 12, weight: .medium, design: .rounded))
                            .foregroundStyle(AppTheme.textMuted)
                    }
                }
                if content.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false {
                    VStack(alignment: .leading, spacing: 4) {
                        ForEach(filePathRows, id: \.self) { path in
                            Button {
                                onOpenPath(path)
                            } label: {
                                HStack(spacing: 5) {
                                    Image(systemName: "doc.text")
                                        .font(.system(size: 10, weight: .semibold))
                                    Text(path)
                                        .font(.system(size: 12, weight: .semibold, design: .monospaced))
                                        .lineLimit(1)
                                        .truncationMode(.middle)
                                }
                                .foregroundStyle(AppTheme.focus)
                                .contentShape(Rectangle())
                            }
                            .buttonStyle(.plain)
                            .appCursor(.pointingHand)
                            .help(path)
                        }

                        if let selectableText {
                            Text(selectableText)
                                .font(.system(size: 13, weight: .regular))
                                .foregroundStyle(AppTheme.textSecondary)
                                .fixedSize(horizontal: false, vertical: true)
                                .textSelection(.enabled)
                        }
                    }
                }
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var filePathRows: [String] {
        content
            .components(separatedBy: .newlines)
            .compactMap { line in
                let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
                guard trimmed.isLikelyAgentFilePath else { return nil }
                return trimmed
            }
    }

    private var selectableText: String? {
        let pathSet = Set(filePathRows)
        let lines = content.components(separatedBy: .newlines).filter { line in
            let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
            return pathSet.contains(trimmed) == false
        }
        return normalizedNonEmptyString(lines.joined(separator: "\n"))
    }
}

private struct AgentErrorBlock: View {
    let content: String

    var body: some View {
        HStack(alignment: .top, spacing: 8) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(Color(nsColor: .systemRed))
                .padding(.top, 1)

            Text(content)
                .font(.system(size: 13, weight: .regular))
                .foregroundStyle(AppTheme.textPrimary)
                .textSelection(.enabled)
                .fixedSize(horizontal: false, vertical: true)
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color(nsColor: .systemRed).opacity(0.08))
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(Color(nsColor: .systemRed).opacity(0.55))
                .frame(width: 2)
        }
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
    }
}

private struct AgentWorkingIndicator: View {
    let statusText: String?
    let startedAt: Date?

    @ObservedObject private var clock = AgentElapsedClock.shared

    var body: some View {
        HStack(spacing: 8) {
            AgentStatusShimmerText(workingLabel)
                .fixedSize(horizontal: true, vertical: false)
                .lineLimit(1)
                .truncationMode(.tail)
                .layoutPriority(2)

            if let elapsed = elapsedText {
                Text(elapsed)
                    .font(.system(size: 12, weight: .medium, design: .rounded))
                    .foregroundStyle(AppTheme.textMuted)
                    .monospacedDigit()
                    .lineLimit(1)
            }

            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var workingLabel: String {
        if isThinkingStatus(statusText) {
            return agentThinkingStatusLabel()
        }
        return agentRunningStatusLabel()
    }

    private var elapsedText: String? {
        guard let startedAt else { return nil }
        let seconds = max(0, Int(clock.now.timeIntervalSince(startedAt)))
        if seconds < 60 {
            return String(format: String(localized: "agent.elapsed.seconds_format", defaultValue: "Elapsed %@s", bundle: .module), "\(seconds)")
        }
        return String(
            format: String(localized: "agent.elapsed.minutes_format", defaultValue: "Elapsed %@m %@s", bundle: .module),
            "\(seconds / 60)",
            "\(seconds % 60)"
        )
    }
}

private func isThinkingStatus(_ statusText: String?) -> Bool {
    guard let statusText else { return false }
    let normalized = statusText.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
    return normalized.contains("think") || normalized.contains("思考")
}

@MainActor
private final class AgentElapsedClock: ObservableObject {
    static let shared = AgentElapsedClock()

    @Published private(set) var now = Date()
    private var timer: Timer?

    private init() {
        let timer = Timer(timeInterval: 1, repeats: true) { _ in
            Task { @MainActor in
                AgentElapsedClock.shared.tick()
            }
        }
        timer.tolerance = 0.2
        RunLoop.main.add(timer, forMode: .common)
        self.timer = timer
    }

    private func tick() {
        now = Date()
    }
}

private struct AgentStatusShimmerText: NSViewRepresentable {
    let text: String

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    init(_ text: String) {
        self.text = text
    }

    func makeNSView(context: Context) -> AgentStatusShimmerLabelView {
        let view = AgentStatusShimmerLabelView()
        view.configure(text: text, reduceMotion: reduceMotion)
        return view
    }

    func updateNSView(_ view: AgentStatusShimmerLabelView, context: Context) {
        view.configure(text: text, reduceMotion: reduceMotion)
    }
}

private final class AgentStatusShimmerLabelView: NSView {
    private static let warningColor = NSColor(calibratedRed: 244 / 255, green: 184 / 255, blue: 90 / 255, alpha: 1)
    private static let animationKey = "agent-status-shimmer"
    private static let sweepSpeed: CGFloat = 42

    private let baseLabel = AgentStatusLabelField()
    private let sweepLabel = AgentStatusLabelField()
    private let sweepMask = CAGradientLayer()
    private var reduceMotion = false
    private var lastAnimationDistance: CGFloat = 0

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setupLabel(baseLabel, color: Self.warningColor.withAlphaComponent(0.84))
        setupLabel(sweepLabel, color: .white.withAlphaComponent(0.94))
        addSubview(baseLabel)
        addSubview(sweepLabel)

        wantsLayer = true
        sweepLabel.wantsLayer = true
        sweepMask.colors = [
            NSColor.clear.cgColor,
            NSColor.white.cgColor,
            NSColor.clear.cgColor,
        ]
        sweepMask.locations = [0, 0.5, 1]
        sweepMask.startPoint = CGPoint(x: 0, y: 0.5)
        sweepMask.endPoint = CGPoint(x: 1, y: 0.5)
        sweepMask.anchorPoint = .zero
        sweepLabel.layer?.mask = sweepMask
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        let size = measuredTextSize()
        return NSSize(width: ceil(size.width), height: ceil(size.height))
    }

    override func layout() {
        super.layout()
        baseLabel.frame = bounds
        sweepLabel.frame = bounds
        updateSweepMaskFrame()
        updateAnimationIfNeeded()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updateAnimationIfNeeded()
    }

    func configure(text: String, reduceMotion: Bool) {
        if baseLabel.stringValue != text {
            baseLabel.stringValue = text
            sweepLabel.stringValue = text
            invalidateIntrinsicContentSize()
            needsLayout = true
        }
        if self.reduceMotion != reduceMotion {
            self.reduceMotion = reduceMotion
            lastAnimationDistance = 0
            updateAnimationIfNeeded()
        }
    }

    private func setupLabel(_ label: NSTextField, color: NSColor) {
        label.font = .systemFont(ofSize: 13, weight: .semibold)
        label.textColor = color
        label.lineBreakMode = .byClipping
        label.maximumNumberOfLines = 1
        label.usesSingleLineMode = true
        label.setContentHuggingPriority(.required, for: .horizontal)
        label.setContentCompressionResistancePriority(.required, for: .horizontal)
    }

    private func measuredTextSize() -> NSSize {
        let font = baseLabel.font ?? .systemFont(ofSize: 13, weight: .semibold)
        let attributes: [NSAttributedString.Key: Any] = [.font: font]
        let size = (baseLabel.stringValue as NSString).size(withAttributes: attributes)
        return NSSize(width: size.width + 1, height: max(size.height, font.ascender - font.descender))
    }

    private func updateSweepMaskFrame() {
        let width = max(bounds.width, 1)
        let height = max(bounds.height, intrinsicContentSize.height, 1)
        let sweepWidth = min(max(width * 0.45, 34), 72)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        sweepMask.bounds = CGRect(x: 0, y: 0, width: sweepWidth, height: height)
        sweepMask.position = CGPoint(x: -sweepWidth, y: 0)
        CATransaction.commit()
    }

    private func updateAnimationIfNeeded() {
        guard reduceMotion == false, window != nil, bounds.width > 0 else {
            sweepMask.removeAnimation(forKey: Self.animationKey)
            lastAnimationDistance = 0
            return
        }
        let sweepDistance = bounds.width + sweepMask.bounds.width * 2
        guard abs(lastAnimationDistance - sweepDistance) > 0.5
            || sweepMask.animation(forKey: Self.animationKey) == nil else {
            return
        }

        lastAnimationDistance = sweepDistance
        sweepMask.removeAnimation(forKey: Self.animationKey)
        let animation = CABasicAnimation(keyPath: "position.x")
        animation.fromValue = -sweepMask.bounds.width
        animation.toValue = bounds.width + sweepMask.bounds.width
        animation.duration = CFTimeInterval(sweepDistance / Self.sweepSpeed)
        animation.repeatCount = .infinity
        animation.timingFunction = CAMediaTimingFunction(name: .linear)
        sweepMask.add(animation, forKey: Self.animationKey)
    }
}

private final class AgentStatusLabelField: NSTextField {
    init() {
        super.init(frame: .zero)
        isEditable = false
        isSelectable = false
        isBordered = false
        drawsBackground = false
        focusRingType = .none
        refusesFirstResponder = true
        cell = AgentStatusLabelCell(textCell: "")
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override func resetCursorRects() {}

    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}

private final class AgentStatusLabelCell: NSTextFieldCell {
    override func resetCursorRect(_ cellFrame: NSRect, in controlView: NSView) {}
}

private struct AgentRunningDot: NSViewRepresentable {
    let size: CGFloat

    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    func makeNSView(context: Context) -> AgentRunningDotView {
        let view = AgentRunningDotView()
        view.configure(size: size, reduceMotion: reduceMotion)
        return view
    }

    func updateNSView(_ view: AgentRunningDotView, context: Context) {
        view.configure(size: size, reduceMotion: reduceMotion)
    }

    static func dismantleNSView(_ nsView: AgentRunningDotView, coordinator: ()) {
        nsView.stopAnimation()
    }
}

private final class AgentRunningDotView: NSView {
    private static let warningColor = NSColor(calibratedRed: 244 / 255, green: 184 / 255, blue: 90 / 255, alpha: 1)
    private static let animationKey = "agent-dot-sweep"
    private static let sweepSpeed: CGFloat = 6.5

    private let sweepLayer = CAGradientLayer()
    private var dotSize: CGFloat = 7
    private var reduceMotion = false
    private var lastAnimationDistance: CGFloat = 0

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setContentHuggingPriority(.required, for: .horizontal)
        setContentHuggingPriority(.required, for: .vertical)
        setContentCompressionResistancePriority(.required, for: .horizontal)
        setContentCompressionResistancePriority(.required, for: .vertical)
        wantsLayer = true
        layer?.masksToBounds = true
        layer?.backgroundColor = Self.warningColor.cgColor

        sweepLayer.colors = [
            NSColor.clear.cgColor,
            NSColor.white.withAlphaComponent(0.76).cgColor,
            NSColor.clear.cgColor,
        ]
        sweepLayer.locations = [0, 0.5, 1]
        sweepLayer.startPoint = CGPoint(x: 0, y: 0.5)
        sweepLayer.endPoint = CGPoint(x: 1, y: 0.5)
        sweepLayer.anchorPoint = .zero
        layer?.addSublayer(sweepLayer)
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        NSSize(width: dotSize, height: dotSize)
    }

    override var fittingSize: NSSize {
        intrinsicContentSize
    }

    override func layout() {
        super.layout()
        if bounds.size != intrinsicContentSize {
            frame.size = intrinsicContentSize
        }
        layer?.cornerRadius = min(bounds.width, bounds.height) / 2
        let sweepWidth = max(bounds.width * 0.85, 6)
        CATransaction.begin()
        CATransaction.setDisableActions(true)
        sweepLayer.bounds = CGRect(x: 0, y: 0, width: sweepWidth, height: max(bounds.height, 1))
        sweepLayer.position = CGPoint(x: -sweepWidth, y: 0)
        CATransaction.commit()
        updateAnimationIfNeeded()
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        updateAnimationIfNeeded()
    }

    func configure(size: CGFloat, reduceMotion: Bool) {
        if abs(dotSize - size) > 0.5 {
            dotSize = size
            invalidateIntrinsicContentSize()
            needsLayout = true
        }
        if self.reduceMotion != reduceMotion {
            self.reduceMotion = reduceMotion
            lastAnimationDistance = 0
            updateAnimationIfNeeded()
        }
    }

    func stopAnimation() {
        sweepLayer.removeAnimation(forKey: Self.animationKey)
        lastAnimationDistance = 0
    }

    private func updateAnimationIfNeeded() {
        guard reduceMotion == false, window != nil, bounds.width > 0 else {
            sweepLayer.removeAnimation(forKey: Self.animationKey)
            lastAnimationDistance = 0
            return
        }
        let sweepDistance = bounds.width + sweepLayer.bounds.width * 2
        guard abs(lastAnimationDistance - sweepDistance) > 0.5
            || sweepLayer.animation(forKey: Self.animationKey) == nil else {
            return
        }

        lastAnimationDistance = sweepDistance
        sweepLayer.removeAnimation(forKey: Self.animationKey)
        let animation = CABasicAnimation(keyPath: "position.x")
        animation.fromValue = -sweepLayer.bounds.width
        animation.toValue = bounds.width + sweepLayer.bounds.width
        animation.duration = CFTimeInterval(sweepDistance / Self.sweepSpeed)
        animation.repeatCount = .infinity
        animation.timingFunction = CAMediaTimingFunction(name: .linear)
        sweepLayer.add(animation, forKey: Self.animationKey)
    }
}

private struct AgentFoldableCodeBlock: View {
    let content: String
    let language: String?
    @Binding var expansionState: AgentContentFoldState
    var defaultExpanded = false

    private var fold: AgentFoldedContent {
        AgentFoldedContentCache.shared.foldedContent(for: content.trimmingCharacters(in: .newlines))
    }

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: defaultExpanded)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            Divider().opacity(0.32)
            codeArea
            if fold.isFolded {
                foldToggle
            }
        }
        .background(Color.black.opacity(0.20))
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(AppTheme.separator.opacity(0.26), lineWidth: 1)
        }
    }

    private var header: some View {
        HStack(spacing: 6) {
            if let label = languageLabel {
                Text(label)
                    .font(.system(size: 11, weight: .semibold, design: .monospaced))
                    .foregroundStyle(AppTheme.textMuted)
            }
            Spacer(minLength: 0)
            AgentCopyButton {
                NSPasteboard.general.clearContents()
                NSPasteboard.general.setString(fold.fullText, forType: .string)
            }
        }
        .padding(.horizontal, 10)
        .frame(height: 26)
    }

    private var codeArea: some View {
        AgentSelectableCodeTextView(text: displayedText, language: language)
            .frame(minHeight: codeAreaHeight, maxHeight: codeAreaHeight)
            .padding(10)
    }

    private var foldToggle: some View {
        Button {
            withAnimation(.easeOut(duration: 0.12)) {
                expansionState = effectiveExpanded ? .collapsed : .expanded
            }
        } label: {
            HStack(spacing: 4) {
                Image(systemName: effectiveExpanded ? "chevron.up" : "chevron.down")
                    .font(.system(size: 10, weight: .bold))
                Text(foldLabel)
                    .font(.system(size: 12, weight: .medium))
            }
            .foregroundStyle(AppTheme.focus)
            .frame(maxWidth: .infinity, alignment: .center)
            .padding(.vertical, 6)
        }
        .buttonStyle(.plain)
        .background(Color.white.opacity(0.04))
        .appCursor(.pointingHand)
    }

    private var languageLabel: String? {
        guard let language else { return nil }
        let trimmed = language.trimmingCharacters(in: .whitespacesAndNewlines).lowercased()
        return trimmed.isEmpty ? nil : trimmed
    }

    private var displayedText: String {
        effectiveExpanded ? fold.fullText : fold.previewText
    }

    private var codeAreaHeight: CGFloat {
        let lineCount = max(1, effectiveExpanded ? fold.totalLineCount : fold.previewLines.count)
        return min(CGFloat(lineCount * 17 + 2), 320)
    }

    private var foldLabel: String {
        if effectiveExpanded {
            return String(localized: "agent.code.show_less", defaultValue: "Show less", bundle: .module)
        }
        return String(
            format: String(localized: "agent.code.show_more_format", defaultValue: "Show %@ more lines", bundle: .module),
            "\(fold.hiddenLineCount)"
        )
    }
}

private struct AgentDiffBlock: View {
    let diff: String
    @Binding var expansionState: AgentContentFoldState

    private var fold: AgentFoldedContent {
        AgentFoldedContentCache.shared.foldedContent(for: diff)
    }

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: false)
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            AgentSelectableDiffTextView(text: displayedText)
                .frame(minHeight: diffAreaHeight, maxHeight: diffAreaHeight)
                .padding(.horizontal, 10)
                .padding(.vertical, 6)

            if fold.isFolded {
                Button {
                    withAnimation(.easeOut(duration: 0.12)) {
                        expansionState = effectiveExpanded ? .collapsed : .expanded
                    }
                } label: {
                    HStack(spacing: 4) {
                        Image(systemName: effectiveExpanded ? "chevron.up" : "chevron.down")
                            .font(.system(size: 10, weight: .bold))
                        Text(foldLabel)
                            .font(.system(size: 12, weight: .medium))
                    }
                    .foregroundStyle(AppTheme.focus)
                    .frame(maxWidth: .infinity, alignment: .center)
                    .padding(.vertical, 6)
                }
                .buttonStyle(.plain)
                .background(Color.white.opacity(0.04))
                .appCursor(.pointingHand)
            }
        }
        .background(Color.black.opacity(0.18))
    }

    private var displayedLines: [String] {
        effectiveExpanded ? fold.allLines : fold.previewLines
    }

    private var displayedText: String {
        displayedLines.joined(separator: "\n")
    }

    private var diffAreaHeight: CGFloat {
        min(CGFloat(max(1, displayedLines.count) * 18 + 2), 360)
    }

    private var foldLabel: String {
        if effectiveExpanded {
            return String(localized: "agent.code.show_less", defaultValue: "Show less", bundle: .module)
        }
        return String(
            format: String(localized: "agent.code.show_more_format", defaultValue: "Show %@ more lines", bundle: .module),
            "\(fold.hiddenLineCount)"
        )
    }

}

private struct AgentInlineChangesBlock: View {
    let changes: [AgentFileChange]
    @Binding var expansionState: AgentContentFoldState
    let changeExpansionState: (String) -> Binding<AgentContentFoldState>
    let onReview: () -> Void
    let onReviewFile: (AgentFileChange) -> Void
    let onDiscardAll: () -> Void
    let onDiscardFile: (AgentFileChange) -> Void

    private var effectiveExpanded: Bool {
        expansionState.isExpanded(defaultExpanded: false)
    }

    private var additions: Int { changes.reduce(0) { $0 + $1.additions } }
    private var deletions: Int { changes.reduce(0) { $0 + $1.deletions } }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            summaryRow

            Divider().opacity(0.24)

            Button {
                withAnimation(.easeOut(duration: 0.12)) {
                    expansionState = effectiveExpanded ? .collapsed : .expanded
                }
            } label: {
                HStack(spacing: 7) {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 10, weight: .bold))
                        .rotationEffect(.degrees(effectiveExpanded ? 90 : 0))
                        .frame(width: 12)
                    Text(String(localized: "agent.files.details", defaultValue: "Details", bundle: .module))
                        .font(.system(size: 12, weight: .semibold))
                    Spacer(minLength: 0)
                }
                .foregroundStyle(AppTheme.textMuted)
                .contentShape(Rectangle())
                .padding(.horizontal, 12)
                .frame(height: 32)
            }
            .buttonStyle(.plain)
            .appCursor(.pointingHand)

            if effectiveExpanded {
                VStack(alignment: .leading, spacing: 10) {
                    ForEach(changes) { change in
                        AgentInlineFileCard(change: change, diffExpansionState: changeExpansionState("agent-change:\(change.path):diff")) {
                            onReviewFile(change)
                        } onDiscard: {
                            onDiscardFile(change)
                        }
                    }
                }
                .padding(.horizontal, 10)
                .padding(.bottom, 10)
                .transition(.opacity)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(Color.black.opacity(0.18))
        .clipShape(RoundedRectangle(cornerRadius: 9, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .strokeBorder(AppTheme.separator.opacity(0.28), lineWidth: 1)
        }
    }

    private var summaryRow: some View {
        HStack(spacing: 10) {
            Image(systemName: "doc.text")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(AppTheme.textMuted)
                .frame(width: 22, height: 22)
                .background(Color.white.opacity(0.06))
                .clipShape(RoundedRectangle(cornerRadius: 6, style: .continuous))

            VStack(alignment: .leading, spacing: 3) {
                Text(summaryTitle)
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                    .truncationMode(.middle)

                HStack(spacing: 7) {
                    Text("+\(additions)")
                        .foregroundStyle(AppTheme.success)
                    Text("-\(deletions)")
                        .foregroundStyle(Color(nsColor: .systemRed))
                }
                .font(.system(size: 12, weight: .bold, design: .monospaced))
            }

            Spacer(minLength: 0)

            Button(action: onDiscardAll) {
                Text(String(localized: "agent.files.revert", defaultValue: "Revert", bundle: .module))
            }
            .buttonStyle(AgentSummaryButtonStyle(tint: AppTheme.warning))

            Button(action: onReview) {
                Text(String(localized: "worktree.menu.review", defaultValue: "Review", bundle: .module))
            }
            .buttonStyle(AgentSummaryButtonStyle(tint: AppTheme.focus))
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 11)
    }

    private var summaryTitle: String {
        if changes.count == 1, let first = changes.first {
            return String(
                format: String(localized: "agent.files.edited_file_format", defaultValue: "Edited %@", bundle: .module),
                first.path
            )
        }
        return String(
            format: String(localized: "agent.files.edited_format", defaultValue: "Edited %@ files", bundle: .module),
            "\(changes.count)"
        )
    }
}

private struct AgentInlineFileCard: View {
    let change: AgentFileChange
    @Binding var diffExpansionState: AgentContentFoldState
    let onReview: () -> Void
    let onDiscard: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack(spacing: 8) {
                Image(systemName: "doc.text")
                    .font(.system(size: 11, weight: .semibold))
                    .foregroundStyle(AppTheme.textMuted)

                Button(action: onReview) {
                    Text(change.path)
                        .font(.system(size: 12, weight: .semibold, design: .monospaced))
                        .foregroundStyle(AppTheme.textPrimary)
                        .lineLimit(1)
                        .truncationMode(.middle)
                        .contentShape(Rectangle())
                }
                .buttonStyle(.plain)
                .appCursor(.pointingHand)
                .help(change.summary ?? change.path)

                Text(change.status.displayName)
                    .font(.system(size: 10, weight: .bold))
                    .foregroundStyle(statusColor)
                    .padding(.horizontal, 5)
                    .padding(.vertical, 2)
                    .background(statusColor.opacity(0.12))
                    .clipShape(RoundedRectangle(cornerRadius: 5, style: .continuous))

                Spacer(minLength: 0)

                HStack(spacing: 6) {
                    Text("+\(change.additions)")
                        .foregroundStyle(AppTheme.success)
                    Text("-\(change.deletions)")
                        .foregroundStyle(Color(nsColor: .systemRed))
                }
                .font(.system(size: 12, weight: .bold, design: .monospaced))

                Button(action: onDiscard) {
                    Image(systemName: "arrow.uturn.backward")
                        .font(.system(size: 10, weight: .bold))
                        .frame(width: 22, height: 22)
                }
                .buttonStyle(.plain)
                .foregroundStyle(AppTheme.warning)
                .help(String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module))
            }
            .padding(.horizontal, 10)
            .frame(height: 30)
            .background(Color(nsColor: .tertiarySystemFill).opacity(0.32))

            if let diffText {
                Divider().opacity(0.28)
                AgentDiffBlock(diff: diffText, expansionState: $diffExpansionState)
            }
        }
        .clipShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
        .overlay {
            RoundedRectangle(cornerRadius: 8, style: .continuous)
                .strokeBorder(AppTheme.separator.opacity(0.26), lineWidth: 1)
        }
    }

    private var diffText: String? {
        normalizedNonEmptyString(change.diff) ?? normalizedNonEmptyString(change.summary)
    }

    private var statusColor: Color {
        switch change.status {
        case .added:
            return AppTheme.success
        case .modified, .typeChanged, .unknown:
            return AppTheme.focus
        case .deleted:
            return Color(nsColor: .systemRed)
        case .renamed, .copied:
            return AppTheme.warning
        }
    }
}

private struct AgentSummaryButtonStyle: ButtonStyle {
    let tint: Color

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 11, weight: .bold))
            .foregroundStyle(tint)
            .padding(.horizontal, 8)
            .padding(.vertical, 5)
            .background(tint.opacity(configuration.isPressed ? 0.18 : 0.10))
            .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
            .appCursor(.pointingHand)
    }
}

private struct AgentStatusDot: View {
    let state: AgentRunState

    var body: some View {
        if state == .running {
            AgentRunningDot(size: 7)
                .frame(width: 7, height: 7)
        } else {
            Circle()
                .fill(color)
                .frame(width: 7, height: 7)
        }
    }

    private var color: Color {
        switch state {
        case .idle:
            return AppTheme.success
        case .running:
            return AppTheme.warning
        case .failed:
            return Color(nsColor: .systemRed)
        }
    }
}
