import AppKit
import Foundation
import MarkdownUI
import SwiftUI

enum AgentTimelinePresentationEntry: Identifiable, Equatable {
    case item(AgentTimelineItem)
    case activity(AgentActivityGroup)

    var id: String {
        switch self {
        case .item(let item):
            return item.id
        case .activity(let group):
            return group.id
        }
    }
}

struct AgentActivityGroup: Identifiable, Equatable {
    let id: String
    let turnID: String?
    let items: [AgentTimelineItem]
    let createdAt: Date
    let updatedAt: Date

    var status: AgentTimelineStatus {
        if items.contains(where: { $0.status == .failed }) {
            return .failed
        }
        if items.contains(where: { $0.status == .running }) {
            return .running
        }
        return .completed
    }

    var durationSeconds: Int {
        max(0, Int(updatedAt.timeIntervalSince(createdAt)))
    }

    var commandCount: Int {
        items.filter { $0.kind == .command }.count
    }

    var toolCount: Int {
        items.filter { $0.kind == .tool }.count
    }

    var fileChangeCount: Int {
        items.filter { $0.kind == .fileChange }.count
    }

    var reasoningCount: Int {
        items.filter { $0.kind == .plan || $0.kind == .reasoning }.count
    }

    var isThinkingOnly: Bool {
        items.isEmpty == false && items.allSatisfy { item in
            item.kind == .reasoning || item.kind == .plan
        }
    }

    var shouldDefaultExpand: Bool {
        status == .running
    }
}

enum AgentTimelinePresentation {
    static func entries(from items: [AgentTimelineItem]) -> [AgentTimelinePresentationEntry] {
        var entries: [AgentTimelinePresentationEntry] = []
        var activityItems: [AgentTimelineItem] = []
        var groupIndex = 0

        func flushActivityItems() {
            guard activityItems.isEmpty == false else { return }
            entries.append(.activity(activityGroup(from: activityItems, index: groupIndex)))
            groupIndex += 1
            activityItems.removeAll(keepingCapacity: true)
        }

        for item in items {
            if item.kind.isAgentActivity {
                activityItems.append(item)
            } else {
                flushActivityItems()
                entries.append(.item(item))
            }
        }
        flushActivityItems()

        return entries
    }

    private static func activityGroup(from items: [AgentTimelineItem], index: Int) -> AgentActivityGroup {
        let createdAt = items.map(\.createdAt).min() ?? Date()
        let updatedAt = items.map(\.updatedAt).max() ?? createdAt
        let turnID = items.compactMap(\.turnID).first
        let firstID = items.first?.id ?? "\(index)"
        let id = "agent-activity-\(index)-\(turnID ?? firstID)"
        return AgentActivityGroup(
            id: id,
            turnID: turnID,
            items: items,
            createdAt: createdAt,
            updatedAt: updatedAt
        )
    }
}

extension AgentTimelineKind {
    var isAgentActivity: Bool {
        switch self {
        case .plan, .reasoning, .command, .fileChange, .tool:
            return true
        case .userPrompt, .assistantMessage, .error, .status:
            return false
        }
    }
}

enum AgentDurationFormatter {
    static func shortElapsedText(seconds: Int) -> String {
        if seconds < 60 {
            return "\(seconds)s"
        }
        return "\(seconds / 60)m \(seconds % 60)s"
    }
}

struct AgentVirtualListLayout: Equatable {
    let visibleRange: Range<Int>
    let topSpacerHeight: CGFloat
    let bottomSpacerHeight: CGFloat
    let totalContentHeight: CGFloat
}

enum AgentVirtualListLayoutCalculator {
    static let defaultEstimatedRowHeight: CGFloat = 96
    static let defaultSpacing: CGFloat = 18
    static let defaultOverscan: CGFloat = 1_200

    static func layout(
        itemIDs: [String],
        measuredHeights: [String: CGFloat],
        estimatedRowHeight: CGFloat = Self.defaultEstimatedRowHeight,
        spacing: CGFloat = Self.defaultSpacing,
        viewportHeight: CGFloat,
        scrollOffset: CGFloat,
        overscan: CGFloat = Self.defaultOverscan
    ) -> AgentVirtualListLayout {
        guard itemIDs.isEmpty == false else {
            return AgentVirtualListLayout(
                visibleRange: 0 ..< 0,
                topSpacerHeight: 0,
                bottomSpacerHeight: 0,
                totalContentHeight: 0
            )
        }

        let visibleStart = max(0, scrollOffset - overscan)
        let visibleEnd = scrollOffset + max(viewportHeight, 1) + overscan
        var firstVisible: Int?
        var lastVisible: Int?
        var firstVisibleTop: CGFloat = 0
        var lastVisibleBottom: CGFloat = 0
        var firstRowTop: CGFloat = 0
        var firstRowBottom: CGFloat = 0
        var lastRowTop: CGFloat = 0
        var lastRowBottom: CGFloat = 0
        var cursor: CGFloat = 0
        let lastIndex = itemIDs.index(before: itemIDs.endIndex)

        for index in itemIDs.indices {
            let height = max(1, measuredHeights[itemIDs[index]] ?? estimatedRowHeight)
            let top = cursor
            let bottom = top + height
            if index == itemIDs.startIndex {
                firstRowTop = top
                firstRowBottom = bottom
            }
            lastRowTop = top
            lastRowBottom = bottom

            if bottom >= visibleStart, top <= visibleEnd {
                if firstVisible == nil {
                    firstVisible = index
                    firstVisibleTop = top
                }
                lastVisible = index
                lastVisibleBottom = bottom
            }

            cursor = bottom + (index == lastIndex ? 0 : spacing)
        }

        let totalContentHeight = max(cursor, 0)
        if firstVisible == nil || lastVisible == nil {
            let fallback = visibleStart > totalContentHeight ? itemIDs.count - 1 : 0
            firstVisible = fallback
            lastVisible = fallback
            if fallback == 0 {
                firstVisibleTop = firstRowTop
                lastVisibleBottom = firstRowBottom
            } else {
                firstVisibleTop = lastRowTop
                lastVisibleBottom = lastRowBottom
            }
        }

        let first = firstVisible ?? 0
        let last = lastVisible ?? first
        return AgentVirtualListLayout(
            visibleRange: first ..< (last + 1),
            topSpacerHeight: max(firstVisibleTop, 0),
            bottomSpacerHeight: max(totalContentHeight - lastVisibleBottom, 0),
            totalContentHeight: totalContentHeight
        )
    }
}

enum AgentMarkdownMessageStyle: Equatable {
    case body
    case reasoning
}

enum AgentContentFoldState: Equatable {
    case automatic
    case expanded
    case collapsed

    func isExpanded(defaultExpanded: Bool) -> Bool {
        switch self {
        case .automatic:
            return defaultExpanded
        case .expanded:
            return true
        case .collapsed:
            return false
        }
    }
}

struct AgentMessageTextPreview: Equatable {
    static let characterThreshold = 12_000
    static let lineThreshold = 160
    static let previewLineCount = 80
    static let previewCharacterLimit = 8_000

    let fullText: String
    let previewText: String
    let omittedLineCount: Int
    let omittedCharacterCount: Int

    init(content: String) {
        fullText = content
        let lines = content.components(separatedBy: "\n")
        let shouldPreview = content.count > Self.characterThreshold || lines.count > Self.lineThreshold
        guard shouldPreview else {
            previewText = content
            omittedLineCount = 0
            omittedCharacterCount = 0
            return
        }

        var previewLines: [String] = []
        var characterCount = 0
        for line in lines {
            let nextCount = characterCount + line.count + (previewLines.isEmpty ? 0 : 1)
            guard previewLines.count < Self.previewLineCount,
                  nextCount <= Self.previewCharacterLimit else {
                break
            }
            previewLines.append(line)
            characterCount = nextCount
        }

        if previewLines.isEmpty {
            previewText = String(content.prefix(Self.previewCharacterLimit))
            omittedLineCount = max(0, lines.count - 1)
            omittedCharacterCount = max(0, content.count - previewText.count)
        } else {
            previewText = previewLines.joined(separator: "\n")
            omittedLineCount = max(0, lines.count - previewLines.count)
            omittedCharacterCount = max(0, content.count - previewText.count)
        }
    }

    var shouldFold: Bool {
        omittedLineCount > 0 || omittedCharacterCount > 0
    }

    var hiddenSummary: String {
        if omittedLineCount > 0 {
            return "\(omittedLineCount)"
        }
        return "\(omittedCharacterCount)"
    }
}

final class AgentMessageTextPreviewCache: @unchecked Sendable {
    static let shared = AgentMessageTextPreviewCache(limit: 240)

    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: AgentMessageTextPreview] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func preview(for content: String) -> AgentMessageTextPreview {
        let key = AgentContentCacheKey.key(for: content)
        lock.lock()
        if let value = storage[key] {
            lock.unlock()
            return value
        }
        lock.unlock()

        let value = AgentMessageTextPreview(content: content)

        lock.lock()
        storage[key] = value
        keys.append(key)
        trimIfNeeded()
        lock.unlock()
        return value
    }

    private func trimIfNeeded() {
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
    }
}

enum AgentMarkdownRenderMode: Equatable {
    case markdown
    case plain
    case streamingMarkdown
}

struct AgentStreamingMarkdownParts: Equatable {
    let stableMarkdown: String
    let streamingPlainTail: String

    init(content: String) {
        guard content.isEmpty == false else {
            stableMarkdown = ""
            streamingPlainTail = ""
            return
        }

        guard content.hasSuffix("\n") == false,
              let lastLineBreak = content.lastIndex(of: "\n") else {
            stableMarkdown = content
            streamingPlainTail = ""
            return
        }

        stableMarkdown = String(content[..<lastLineBreak])
        streamingPlainTail = String(content[lastLineBreak...])
    }

    var isFullyStable: Bool {
        streamingPlainTail.isEmpty
    }
}

struct AgentMarkdownBlockSegment: Identifiable, Equatable {
    enum Kind: Equatable {
        case markdown(String)
        case code(language: String?, content: String)
    }

    let id: String
    let kind: Kind
}

enum AgentMarkdownBlockParser {
    static func segments(from content: String) -> [AgentMarkdownBlockSegment] {
        guard content.isEmpty == false else { return [] }

        let lines = content.components(separatedBy: "\n")
        var segments: [AgentMarkdownBlockSegment] = []
        var markdownLines: [String] = []
        var index = 0

        func flushMarkdown() {
            let markdown = markdownLines.joined(separator: "\n")
            markdownLines.removeAll(keepingCapacity: true)
            guard markdown.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else { return }
            segments.append(
                AgentMarkdownBlockSegment(
                    id: "markdown-\(segments.count)-\(markdown.count)",
                    kind: .markdown(markdown)
                )
            )
        }

        while index < lines.count {
            let line = lines[index]
            if let fence = fenceStart(in: line) {
                var cursor = index + 1
                var codeLines: [String] = []
                var didClose = false
                while cursor < lines.count {
                    if lines[cursor].trimmingCharacters(in: .whitespaces).hasPrefix(fence.marker) {
                        didClose = true
                        break
                    }
                    codeLines.append(lines[cursor])
                    cursor += 1
                }

                guard didClose else {
                    markdownLines.append(contentsOf: lines[index...])
                    break
                }

                flushMarkdown()
                let code = codeLines.joined(separator: "\n")
                segments.append(
                    AgentMarkdownBlockSegment(
                        id: "code-\(segments.count)-\(code.count)",
                        kind: .code(language: fence.language, content: code)
                    )
                )
                index = cursor + 1
                continue
            }

            markdownLines.append(line)
            index += 1
        }

        flushMarkdown()
        return segments
    }

    private static func fenceStart(in line: String) -> (marker: String, language: String?)? {
        let trimmed = line.trimmingCharacters(in: .whitespaces)
        let marker: String
        if trimmed.hasPrefix("```") {
            marker = "```"
        } else if trimmed.hasPrefix("~~~") {
            marker = "~~~"
        } else {
            return nil
        }
        let language = String(trimmed.dropFirst(marker.count))
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return (marker, language.isEmpty ? nil : language)
    }
}

struct AgentSelectableMarkdownText: NSViewRepresentable {
    let content: String
    let style: AgentMarkdownMessageStyle
    let cacheContent: Bool
    let renderMode: AgentMarkdownRenderMode
    let fitsContentWidth: Bool
    let maximumWidth: CGFloat?
    let onOpenLink: ((String) -> Void)?

    init(
        content: String,
        style: AgentMarkdownMessageStyle,
        cacheContent: Bool,
        renderMode: AgentMarkdownRenderMode = .markdown,
        fitsContentWidth: Bool = false,
        maximumWidth: CGFloat? = nil,
        onOpenLink: ((String) -> Void)? = nil
    ) {
        self.content = content
        self.style = style
        self.cacheContent = cacheContent
        self.renderMode = renderMode
        self.fitsContentWidth = fitsContentWidth
        self.maximumWidth = maximumWidth
        self.onOpenLink = onOpenLink
    }

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context _: Context) -> AgentSelectableMarkdownTextContainerView {
        AgentSelectableMarkdownTextContainerView()
    }

    func updateNSView(_ view: AgentSelectableMarkdownTextContainerView, context: Context) {
        context.coordinator.onOpenLink = onOpenLink
        view.linkHandler = { [weak coordinator = context.coordinator] link in
            coordinator?.onOpenLink?(link)
        }
        let previousStyle = context.coordinator.lastStyle
        let previousRenderMode = context.coordinator.lastRenderMode
        let previousStreamingStableMarkdown = context.coordinator.lastStreamingStableMarkdown
        let previousStreamingPlainTail = context.coordinator.lastStreamingPlainTail
        guard context.coordinator.lastContent != content ||
            context.coordinator.lastStyle != style ||
            context.coordinator.lastRenderMode != renderMode else {
            view.configureSizing(fitsContentWidth: fitsContentWidth, maximumWidth: maximumWidth)
            return
        }

        if renderMode == .streamingMarkdown {
            let parts = AgentStreamingMarkdownParts(content: content)
            if previousRenderMode == .streamingMarkdown,
               previousStyle == style,
               previousStreamingStableMarkdown == parts.stableMarkdown,
               let previousStreamingPlainTail,
               parts.streamingPlainTail.hasPrefix(previousStreamingPlainTail),
               parts.streamingPlainTail.count > previousStreamingPlainTail.count {
                let suffixStart = parts.streamingPlainTail.index(
                    parts.streamingPlainTail.startIndex,
                    offsetBy: previousStreamingPlainTail.count
                )
                let suffix = String(parts.streamingPlainTail[suffixStart...])
                context.coordinator.lastContent = content
                context.coordinator.lastStyle = style
                context.coordinator.lastRenderMode = renderMode
                context.coordinator.lastStreamingStableMarkdown = parts.stableMarkdown
                context.coordinator.lastStreamingPlainTail = parts.streamingPlainTail
                view.appendAttributedSuffix(
                    AgentMarkdownAttributedRenderer.plainAttributedString(for: suffix, style: style),
                    fitsContentWidth: fitsContentWidth,
                    maximumWidth: maximumWidth
                )
                return
            }
        }

        context.coordinator.lastContent = content
        context.coordinator.lastStyle = style
        context.coordinator.lastRenderMode = renderMode
        let attributedString: NSAttributedString
        let streamingStableMarkdown: String?
        let streamingPlainTail: String?
        switch renderMode {
        case .markdown:
            attributedString = cacheContent
                ? AgentMarkdownAttributedCache.shared.attributedString(for: content, style: style)
                : AgentMarkdownAttributedRenderer.attributedString(for: content, style: style)
            streamingStableMarkdown = nil
            streamingPlainTail = nil
        case .plain:
            attributedString = AgentMarkdownAttributedRenderer.plainAttributedString(for: content, style: style)
            streamingStableMarkdown = nil
            streamingPlainTail = nil
        case .streamingMarkdown:
            let parts = AgentStreamingMarkdownParts(content: content)
            attributedString = AgentMarkdownAttributedRenderer.streamingAttributedString(for: parts, style: style)
            streamingStableMarkdown = parts.stableMarkdown
            streamingPlainTail = parts.streamingPlainTail
        }
        context.coordinator.lastStreamingStableMarkdown = streamingStableMarkdown
        context.coordinator.lastStreamingPlainTail = streamingPlainTail
        view.configure(
            attributedString: attributedString,
            fitsContentWidth: fitsContentWidth,
            maximumWidth: maximumWidth,
            heightCacheContent: cacheContent ? content : nil,
            heightCacheStyle: cacheContent ? style : nil,
            forceUpdate: previousStyle != style || previousRenderMode != renderMode,
            allowsIncrementalAppend: renderMode == .plain || (
                renderMode == .streamingMarkdown &&
                    previousStreamingStableMarkdown == streamingStableMarkdown
            )
        )
    }

    func sizeThatFits(
        _ proposal: ProposedViewSize,
        nsView: AgentSelectableMarkdownTextContainerView,
        context _: Context
    ) -> CGSize? {
        let proposedWidth = proposal.width.map { max($0, 1) }
        let width = nsView.preferredWidth(proposedWidth: proposedWidth)
        return CGSize(
            width: width,
            height: nsView.height(
                for: width,
                content: cacheContent ? content : nil,
                style: cacheContent ? style : nil
            )
        )
    }

    final class Coordinator {
        var lastContent: String?
        var lastStyle: AgentMarkdownMessageStyle?
        var lastRenderMode: AgentMarkdownRenderMode?
        var lastStreamingStableMarkdown: String?
        var lastStreamingPlainTail: String?
        var onOpenLink: ((String) -> Void)?
    }
}

final class AgentSelectableMarkdownTextContainerView: NSView {
    private let textView = AgentSelectableMarkdownTextView()
    private var attributedString = NSAttributedString()
    private var fitsContentWidth = false
    private var maximumWidth: CGFloat?
    private var heightCacheContent: String?
    private var heightCacheStyle: AgentMarkdownMessageStyle?
    private var lastMeasuredWidth: CGFloat = 0
    private var lastMeasuredHeight: CGFloat = 1
    private var lastMeasuredContentLength = 0
    var linkHandler: ((String) -> Void)?

    override var isFlipped: Bool { true }

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        translatesAutoresizingMaskIntoConstraints = false
        setupTextView()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        nil
    }

    override var intrinsicContentSize: NSSize {
        let width = preferredWidth(proposedWidth: bounds.width > 0 ? bounds.width : nil)
        return NSSize(width: width, height: height(for: width))
    }

    override func layout() {
        super.layout()
        let width = max(bounds.width, 1)
        textView.frame = NSRect(
            x: 0,
            y: 0,
            width: width,
            height: height(for: width, content: heightCacheContent, style: heightCacheStyle)
        )
    }

    func configure(
        attributedString: NSAttributedString,
        fitsContentWidth: Bool,
        maximumWidth: CGFloat?,
        heightCacheContent: String?,
        heightCacheStyle: AgentMarkdownMessageStyle?,
        forceUpdate: Bool,
        allowsIncrementalAppend: Bool
    ) {
        configureSizing(fitsContentWidth: fitsContentWidth, maximumWidth: maximumWidth)
        self.heightCacheContent = heightCacheContent
        self.heightCacheStyle = heightCacheStyle
        if forceUpdate ||
            self.attributedString.length != attributedString.length ||
            self.attributedString.string != attributedString.string {
            updateTextStorage(with: attributedString, allowsIncrementalAppend: allowsIncrementalAppend)
            lastMeasuredWidth = 0
            lastMeasuredContentLength = attributedString.length
        }
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    func configureSizing(fitsContentWidth: Bool, maximumWidth: CGFloat?) {
        if self.fitsContentWidth != fitsContentWidth || self.maximumWidth != maximumWidth {
            self.fitsContentWidth = fitsContentWidth
            self.maximumWidth = maximumWidth
            invalidateIntrinsicContentSize()
            needsLayout = true
        }
    }

    func appendAttributedSuffix(
        _ suffix: NSAttributedString,
        fitsContentWidth: Bool,
        maximumWidth: CGFloat?
    ) {
        configureSizing(fitsContentWidth: fitsContentWidth, maximumWidth: maximumWidth)
        guard suffix.length > 0, let textStorage = textView.textStorage else {
            return
        }
        let next = NSMutableAttributedString(attributedString: attributedString)
        next.append(suffix)
        textStorage.append(suffix)
        attributedString = next
        lastMeasuredWidth = 0
        lastMeasuredContentLength = attributedString.length
        invalidateIntrinsicContentSize()
        needsLayout = true
    }

    func preferredWidth(proposedWidth: CGFloat?) -> CGFloat {
        if fitsContentWidth {
            let limit = max(1, maximumWidth ?? proposedWidth ?? 560)
            let naturalWidth = min(naturalTextWidth(limit: limit), limit)
            if let proposedWidth {
                return min(max(naturalWidth, 1), proposedWidth)
            }
            return max(naturalWidth, 1)
        }
        return max(proposedWidth ?? bounds.width, 1)
    }

    func height(for width: CGFloat, content: String? = nil, style: AgentMarkdownMessageStyle? = nil) -> CGFloat {
        let width = max(width, 1)
        if abs(lastMeasuredWidth - width) < 0.5,
           lastMeasuredContentLength == attributedString.length {
            return lastMeasuredHeight
        }
        let cachedContent = content ?? heightCacheContent
        let cachedStyle = style ?? heightCacheStyle
        if let cachedContent, let cachedStyle,
           let height = AgentMarkdownHeightCache.shared.height(for: cachedContent, style: cachedStyle, width: width) {
            lastMeasuredWidth = width
            lastMeasuredHeight = height
            lastMeasuredContentLength = attributedString.length
            return height
        }
        guard let layoutManager = textView.layoutManager,
              let textContainer = textView.textContainer else {
            return 0
        }
        textContainer.containerSize = NSSize(width: width, height: CGFloat.greatestFiniteMagnitude)
        textContainer.widthTracksTextView = false
        layoutManager.ensureLayout(for: textContainer)
        let height = max(ceil(layoutManager.usedRect(for: textContainer).height), 1)
        lastMeasuredWidth = width
        lastMeasuredHeight = height
        lastMeasuredContentLength = attributedString.length
        if let cachedContent, let cachedStyle {
            AgentMarkdownHeightCache.shared.store(height, for: cachedContent, style: cachedStyle, width: width)
        }
        return height
    }

    private func setupTextView() {
        textView.drawsBackground = false
        textView.isEditable = false
        textView.isSelectable = true
        textView.isRichText = true
        textView.importsGraphics = false
        textView.usesFontPanel = false
        textView.allowsUndo = false
        textView.isHorizontallyResizable = false
        textView.isVerticallyResizable = true
        textView.textContainerInset = .zero
        textView.textContainer?.lineFragmentPadding = 0
        textView.textContainer?.heightTracksTextView = false
        textView.textContainer?.widthTracksTextView = false
        textView.delegate = self
        textView.linkClickHandler = { [weak self] link in
            self?.handleLink(link) ?? false
        }
        textView.linkTextAttributes = [
            .foregroundColor: NSColor.dmuxHex(0x5AA7FF),
            .underlineStyle: NSUnderlineStyle.single.rawValue,
        ]
        addSubview(textView)
    }

    private func updateTextStorage(with next: NSAttributedString, allowsIncrementalAppend: Bool) {
        guard let textStorage = textView.textStorage else {
            attributedString = next
            return
        }
        if allowsIncrementalAppend,
           next.length > attributedString.length,
           next.string.hasPrefix(attributedString.string) {
            let appendRange = NSRange(location: attributedString.length, length: next.length - attributedString.length)
            textStorage.append(next.attributedSubstring(from: appendRange))
            attributedString = next
            return
        }

        attributedString = next
        textStorage.setAttributedString(next)
    }

    private func naturalTextWidth(limit: CGFloat) -> CGFloat {
        let boundingRect = attributedString.boundingRect(
            with: NSSize(width: limit, height: CGFloat.greatestFiniteMagnitude),
            options: [.usesLineFragmentOrigin, .usesFontLeading]
        )
        return ceil(boundingRect.width)
    }
}

extension AgentSelectableMarkdownTextContainerView: NSTextViewDelegate {
    func textView(
        _ textView: NSTextView,
        clickedOnLink link: Any,
        at charIndex: Int
    ) -> Bool {
        if let url = link as? URL {
            return handleLink(url.absoluteString)
        }
        if let string = link as? String {
            return handleLink(string)
        }
        return false
    }

    private func handleLink(_ link: Any) -> Bool {
        if let url = link as? URL {
            return handleLink(url.absoluteString)
        }
        if let string = link as? String {
            return handleLink(string)
        }
        return false
    }

    private func handleLink(_ value: String) -> Bool {
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.isEmpty == false else { return true }
        if let url = URL(string: trimmed),
           let scheme = url.scheme?.lowercased(),
           ["http", "https", "mailto"].contains(scheme) {
            NSWorkspace.shared.open(url)
            return true
        }
        linkHandler?(trimmed)
        return true
    }
}

private final class AgentSelectableMarkdownTextView: NSTextView {
    var linkClickHandler: ((Any) -> Bool)?

    override func resetCursorRects() {
        addCursorRect(visibleRect, cursor: .iBeam)
    }

    override func mouseDown(with event: NSEvent) {
        if event.clickCount == 1,
           let link = link(at: event.locationInWindow),
           linkClickHandler?(link) == true {
            return
        }
        super.mouseDown(with: event)
    }

    private func link(at windowPoint: NSPoint) -> Any? {
        guard let layoutManager, let textContainer, let textStorage else { return nil }
        let point = convert(windowPoint, from: nil)
        let textPoint = NSPoint(
            x: point.x - textContainerInset.width,
            y: point.y - textContainerInset.height
        )
        guard textPoint.x >= 0, textPoint.y >= 0 else { return nil }
        let glyphIndex = layoutManager.glyphIndex(for: textPoint, in: textContainer)
        let characterIndex = layoutManager.characterIndexForGlyph(at: glyphIndex)
        guard characterIndex >= 0, characterIndex < textStorage.length else { return nil }
        return textStorage.attribute(.link, at: characterIndex, effectiveRange: nil)
    }
}

enum AgentMarkdownAttributedRenderer {
    static func plainAttributedString(for content: String, style: AgentMarkdownMessageStyle) -> NSAttributedString {
        NSAttributedString(string: content, attributes: Builder.baseAttributes(for: style))
    }

    static func streamingAttributedString(for parts: AgentStreamingMarkdownParts, style: AgentMarkdownMessageStyle) -> NSAttributedString {
        if parts.streamingPlainTail.isEmpty {
            return AgentMarkdownAttributedCache.shared.attributedString(for: parts.stableMarkdown, style: style)
        }
        if parts.stableMarkdown.isEmpty {
            return plainAttributedString(for: parts.streamingPlainTail, style: style)
        }

        let output = NSMutableAttributedString(
            attributedString: AgentMarkdownAttributedCache.shared.attributedString(for: parts.stableMarkdown, style: style)
        )
        output.append(plainAttributedString(for: parts.streamingPlainTail, style: style))
        return output
    }

    static func attributedString(for markdown: String, style: AgentMarkdownMessageStyle) -> NSAttributedString {
        let builder = Builder(style: style)
        builder.render(markdown)
        return builder.result()
    }

    static func visibleText(for markdown: String, style: AgentMarkdownMessageStyle = .body) -> String {
        attributedString(for: markdown, style: style).string
    }

    fileprivate final class Builder {
        private let style: AgentMarkdownMessageStyle
        private let output = NSMutableAttributedString()
        private var isFirstBlock = true

        init(style: AgentMarkdownMessageStyle) {
            self.style = style
        }

        func render(_ markdown: String) {
            let lines = markdown.components(separatedBy: .newlines)
            var index = 0
            while index < lines.count {
                let line = lines[index]
                let trimmed = line.trimmingCharacters(in: .whitespaces)

                if let fence = fenceMarker(in: trimmed) {
                    var codeLines: [String] = []
                    index += 1
                    while index < lines.count {
                        let candidate = lines[index]
                        if candidate.trimmingCharacters(in: .whitespaces).hasPrefix(fence) {
                            break
                        }
                        codeLines.append(candidate)
                        index += 1
                    }
                    appendBlockSeparator()
                    appendCodeBlock(codeLines.joined(separator: "\n"))
                    index += 1
                    continue
                }

                if trimmed.isEmpty {
                    appendBlankLine()
                    index += 1
                    continue
                }

                appendBlockSeparator()
                appendLine(line)
                index += 1
            }
        }

        func result() -> NSAttributedString {
            if output.string.hasSuffix("\n") {
                output.deleteCharacters(in: NSRange(location: output.length - 1, length: 1))
            }
            return output
        }

        private func appendLine(_ line: String) {
            let parsed = parseLine(line)
            appendInline(parsed.text, attributes: attributes(for: parsed.kind))
            output.append(NSAttributedString(string: "\n", attributes: attributes(for: .body)))
        }

        private func appendBlankLine() {
            guard output.length > 0,
                  output.string.hasSuffix("\n\n") == false else {
                return
            }
            output.append(NSAttributedString(string: "\n", attributes: attributes(for: .body)))
            isFirstBlock = true
        }

        private func appendBlockSeparator() {
            if isFirstBlock {
                isFirstBlock = false
                return
            }
            if output.length > 0, output.string.hasSuffix("\n") == false {
                output.append(NSAttributedString(string: "\n", attributes: attributes(for: .body)))
            }
        }

        private func appendCodeBlock(_ code: String) {
            let text = code.isEmpty ? " " : code
            output.append(NSAttributedString(string: text, attributes: attributes(for: .codeBlock)))
            output.append(NSAttributedString(string: "\n", attributes: attributes(for: .body)))
        }

        private func appendInline(_ text: String, attributes currentAttributes: [NSAttributedString.Key: Any]) {
            let chars = Array(text)
            var index = 0
            var plainStart = 0

            func flushPlain(upTo end: Int) {
                guard end > plainStart else { return }
                output.append(NSAttributedString(string: String(chars[plainStart ..< end]), attributes: currentAttributes))
            }

            while index < chars.count {
                if chars[index] == "`",
                   let close = nextIndex(of: "`", in: chars, after: index + 1) {
                    flushPlain(upTo: index)
                    let code = String(chars[(index + 1) ..< close])
                    output.append(NSAttributedString(string: code, attributes: attributes(for: .inlineCode, base: currentAttributes)))
                    index = close + 1
                    plainStart = index
                    continue
                }

                if hasPrefix("**", in: chars, at: index),
                   let close = nextIndex(of: "**", in: chars, after: index + 2) {
                    flushPlain(upTo: index)
                    let strongText = String(chars[(index + 2) ..< close])
                    appendInline(strongText, attributes: attributes(for: .strong, base: currentAttributes))
                    index = close + 2
                    plainStart = index
                    continue
                }

                if chars[index] == "*",
                   let close = nextIndex(of: "*", in: chars, after: index + 1) {
                    flushPlain(upTo: index)
                    let emphasizedText = String(chars[(index + 1) ..< close])
                    appendInline(emphasizedText, attributes: attributes(for: .emphasis, base: currentAttributes))
                    index = close + 1
                    plainStart = index
                    continue
                }

                if chars[index] == "[",
                   let labelClose = nextIndex(of: "]", in: chars, after: index + 1),
                   labelClose + 1 < chars.count,
                   chars[labelClose + 1] == "(",
                   let destinationClose = nextIndex(of: ")", in: chars, after: labelClose + 2) {
                    flushPlain(upTo: index)
                    let label = String(chars[(index + 1) ..< labelClose])
                    let destination = String(chars[(labelClose + 2) ..< destinationClose])
                    var linkAttributes = attributes(for: .link, base: currentAttributes)
                    linkAttributes[.link] = destination
                    appendInline(label, attributes: linkAttributes)
                    index = destinationClose + 1
                    plainStart = index
                    continue
                }

                index += 1
            }

            flushPlain(upTo: chars.count)
        }

        private func parseLine(_ line: String) -> (text: String, kind: LineKind) {
            let leadingWhitespaceCount = line.prefix { $0 == " " || $0 == "\t" }.count
            let indent = String(repeating: " ", count: leadingWhitespaceCount)
            let trimmed = line.trimmingCharacters(in: .whitespaces)

            if trimmed.hasPrefix("### ") {
                return (String(trimmed.dropFirst(4)), .heading3)
            }
            if trimmed.hasPrefix("## ") {
                return (String(trimmed.dropFirst(3)), .heading2)
            }
            if trimmed.hasPrefix("# ") {
                return (String(trimmed.dropFirst(2)), .heading1)
            }
            if trimmed.hasPrefix("> ") {
                return ("│ " + String(trimmed.dropFirst(2)), .quote)
            }
            if trimmed.hasPrefix("- ") || trimmed.hasPrefix("* ") || trimmed.hasPrefix("+ ") {
                return (indent + "• " + String(trimmed.dropFirst(2)), .body)
            }
            if let orderedPrefixEnd = orderedListPrefixEnd(in: trimmed) {
                return (indent + String(trimmed[..<orderedPrefixEnd]) + " " + String(trimmed[orderedPrefixEnd...]).trimmingCharacters(in: .whitespaces), .body)
            }
            return (line, .body)
        }

        private func attributes(
            for kind: LineKind,
            base: [NSAttributedString.Key: Any]? = nil
        ) -> [NSAttributedString.Key: Any] {
            var attributes = base ?? baseAttributes
            switch kind {
            case .body:
                attributes[.font] = style == .reasoning ? Self.reasoningFont : Self.bodyFont
                attributes[.foregroundColor] = style == .reasoning ? NSColor.secondaryLabelColor : NSColor.labelColor
            case .heading1:
                attributes[.font] = NSFont.systemFont(ofSize: 18, weight: .bold)
                attributes[.foregroundColor] = NSColor.labelColor
            case .heading2:
                attributes[.font] = NSFont.systemFont(ofSize: 16, weight: .semibold)
                attributes[.foregroundColor] = NSColor.labelColor
            case .heading3:
                attributes[.font] = NSFont.systemFont(ofSize: 15, weight: .semibold)
                attributes[.foregroundColor] = NSColor.labelColor
            case .quote:
                attributes[.font] = style == .reasoning ? Self.reasoningFont : Self.bodyFont
                attributes[.foregroundColor] = NSColor.secondaryLabelColor
            case .strong:
                let font = (attributes[.font] as? NSFont) ?? Self.bodyFont
                attributes[.font] = NSFontManager.shared.convert(font, toHaveTrait: .boldFontMask)
            case .emphasis:
                let font = (attributes[.font] as? NSFont) ?? Self.bodyFont
                attributes[.font] = NSFontManager.shared.convert(font, toHaveTrait: .italicFontMask)
            case .inlineCode:
                attributes[.font] = NSFont.monospacedSystemFont(ofSize: 13, weight: .regular)
                attributes[.foregroundColor] = NSColor.dmuxHex(0xD7DEE8)
                attributes.removeValue(forKey: .backgroundColor)
            case .codeBlock:
                attributes[.font] = NSFont.monospacedSystemFont(ofSize: 12.5, weight: .regular)
                attributes[.foregroundColor] = NSColor.labelColor
                attributes[.backgroundColor] = NSColor.black.withAlphaComponent(0.16)
            case .link:
                attributes[.foregroundColor] = NSColor.dmuxHex(0x5AA7FF)
                attributes[.underlineStyle] = NSUnderlineStyle.single.rawValue
            }
            attributes[.paragraphStyle] = paragraphStyle(for: kind)
            return attributes
        }

        static func baseAttributes(for style: AgentMarkdownMessageStyle) -> [NSAttributedString.Key: Any] {
            Builder(style: style).baseAttributes
        }

        private var baseAttributes: [NSAttributedString.Key: Any] {
            attributes(for: .body, base: [
                .font: style == .reasoning ? Self.reasoningFont : Self.bodyFont,
                .foregroundColor: style == .reasoning ? NSColor.secondaryLabelColor : NSColor.labelColor,
            ])
        }

        private func paragraphStyle(for kind: LineKind) -> NSMutableParagraphStyle {
            let paragraphStyle = NSMutableParagraphStyle()
            paragraphStyle.lineSpacing = style == .reasoning ? 3 : 4
            paragraphStyle.paragraphSpacing = {
                switch kind {
                case .heading1, .heading2:
                    return 6
                case .heading3:
                    return 4
                case .codeBlock:
                    return 4
                default:
                    return style == .reasoning ? 4 : 5
                }
            }()
            paragraphStyle.lineBreakMode = .byWordWrapping
            return paragraphStyle
        }

        private func fenceMarker(in trimmedLine: String) -> String? {
            if trimmedLine.hasPrefix("```") {
                return "```"
            }
            if trimmedLine.hasPrefix("~~~") {
                return "~~~"
            }
            return nil
        }

        private func orderedListPrefixEnd(in trimmed: String) -> String.Index? {
            guard let dotIndex = trimmed.firstIndex(of: "."),
                  dotIndex > trimmed.startIndex else {
                return nil
            }
            let number = trimmed[..<dotIndex]
            guard number.allSatisfy(\.isNumber) else {
                return nil
            }
            let nextIndex = trimmed.index(after: dotIndex)
            guard nextIndex < trimmed.endIndex,
                  trimmed[nextIndex].isWhitespace else {
                return nil
            }
            return nextIndex
        }

        private func hasPrefix(_ prefix: String, in chars: [Character], at index: Int) -> Bool {
            let prefixChars = Array(prefix)
            guard index + prefixChars.count <= chars.count else {
                return false
            }
            return Array(chars[index ..< (index + prefixChars.count)]) == prefixChars
        }

        private func nextIndex(of marker: String, in chars: [Character], after start: Int) -> Int? {
            guard start < chars.count else {
                return nil
            }
            var index = start
            while index < chars.count {
                if hasPrefix(marker, in: chars, at: index) {
                    return index
                }
                index += 1
            }
            return nil
        }

        private enum LineKind {
            case body
            case heading1
            case heading2
            case heading3
            case quote
            case strong
            case emphasis
            case inlineCode
            case codeBlock
            case link
        }

        private static var bodyFont: NSFont {
            NSFont.systemFont(ofSize: 14, weight: .regular)
        }

        private static var reasoningFont: NSFont {
            NSFont.systemFont(ofSize: 13, weight: .regular)
        }
    }
}

private final class AgentMarkdownAttributedCache: @unchecked Sendable {
    static let shared = AgentMarkdownAttributedCache(limit: 240)

    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: NSAttributedString] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func attributedString(for markdown: String, style: AgentMarkdownMessageStyle) -> NSAttributedString {
        let key = AgentContentCacheKey.key(for: markdown, style: style)
        lock.lock()
        if let value = storage[key] {
            lock.unlock()
            return value
        }
        lock.unlock()

        let value = AgentMarkdownAttributedRenderer.attributedString(for: markdown, style: style)

        lock.lock()
        storage[key] = value
        keys.append(key)
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
        lock.unlock()
        return value
    }
}

private final class AgentMarkdownHeightCache: @unchecked Sendable {
    static let shared = AgentMarkdownHeightCache(limit: 360)

    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: CGFloat] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func height(for markdown: String, style: AgentMarkdownMessageStyle, width: CGFloat) -> CGFloat? {
        let key = cacheKey(markdown: markdown, style: style, width: width)
        lock.lock()
        defer { lock.unlock() }
        return storage[key]
    }

    func store(_ height: CGFloat, for markdown: String, style: AgentMarkdownMessageStyle, width: CGFloat) {
        let key = cacheKey(markdown: markdown, style: style, width: width)
        lock.lock()
        storage[key] = height
        keys.append(key)
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
        lock.unlock()
    }

    private func cacheKey(markdown: String, style: AgentMarkdownMessageStyle, width: CGFloat) -> String {
        "\(Int(width.rounded()))|\(AgentContentCacheKey.key(for: markdown, style: style))"
    }
}

private enum AgentContentCacheKey {
    static func key(for content: String, style: AgentMarkdownMessageStyle? = nil) -> String {
        let stylePart = style.map { "\($0)|" } ?? ""
        return "\(stylePart)\(content.count)|\(content.hashValue)"
    }
}

extension String {
    var isLikelyAgentFilePath: Bool {
        let trimmed = trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.isEmpty == false,
              trimmed.count < 260,
              trimmed.hasPrefix("http://") == false,
              trimmed.hasPrefix("https://") == false else {
            return false
        }
        let blockedPrefixes = ["diff --git", "index ", "@@", "--- ", "+++ ", "+", "-"]
        guard blockedPrefixes.contains(where: { trimmed.hasPrefix($0) }) == false else {
            return false
        }

        let hasSeparator = trimmed.contains("/") || trimmed.contains("\\")
        let hasWhitespace = trimmed.rangeOfCharacter(from: .whitespaces) != nil
        let fileName = URL(fileURLWithPath: trimmed).lastPathComponent
        let hasFileExtension = fileName.contains(".") && fileName.hasSuffix(".") == false
        return hasSeparator || (hasWhitespace == false && hasFileExtension)
    }
}

struct AgentFoldedContent: Equatable {
    static let foldThreshold = 18
    static let previewLineCount = 12

    let allLines: [String]

    init(content: String) {
        allLines = content.components(separatedBy: "\n")
    }

    var totalLineCount: Int { allLines.count }
    var isFolded: Bool { totalLineCount > Self.foldThreshold }
    var hiddenLineCount: Int { isFolded ? totalLineCount - Self.previewLineCount : 0 }
    var previewLines: [String] { isFolded ? Array(allLines.prefix(Self.previewLineCount)) : allLines }
    var previewText: String { previewLines.joined(separator: "\n") }
    var fullText: String { allLines.joined(separator: "\n") }
}

final class AgentFoldedContentCache: @unchecked Sendable {
    static let shared = AgentFoldedContentCache(limit: 160)

    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: AgentFoldedContent] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func foldedContent(for content: String) -> AgentFoldedContent {
        let key = AgentContentCacheKey.key(for: content)
        lock.lock()
        if let value = storage[key] {
            lock.unlock()
            return value
        }
        lock.unlock()

        let value = AgentFoldedContent(content: content)

        lock.lock()
        storage[key] = value
        keys.append(key)
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
        lock.unlock()
        return value
    }
}

struct AgentGitStatusList: Equatable {
    let entries: [Entry]

    struct Entry: Identifiable, Equatable {
        let code: String
        let path: String

        var id: String { "\(code)|\(path)" }
    }

    static func parse(_ content: String) -> AgentGitStatusList? {
        let lines = content
            .components(separatedBy: .newlines)
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { $0.isEmpty == false }
        let entries = lines.compactMap(Entry.parse)
        guard entries.isEmpty == false, entries.count == lines.count else { return nil }
        return AgentGitStatusList(entries: entries)
    }
}

private extension AgentGitStatusList.Entry {
    static func parse(_ line: String) -> AgentGitStatusList.Entry? {
        let trimmed = line.trimmingCharacters(in: .whitespacesAndNewlines)
        guard trimmed.count >= 4 else { return nil }

        let code = String(trimmed.prefix(2)).trimmingCharacters(in: .whitespaces)
        let path = String(trimmed.dropFirst(2)).trimmingCharacters(in: .whitespaces)
        guard ["M", "A", "D", "R", "C", "??"].contains(code),
              path.isEmpty == false else {
            return nil
        }
        return AgentGitStatusList.Entry(code: code, path: path)
    }
}

extension AgentGitStatusList.Entry {
    var tint: Color {
        switch code {
        case "??", "A":
            return AppTheme.success
        case "M":
            return AppTheme.warning
        case "D":
            return Color(nsColor: .systemRed)
        default:
            return AppTheme.textMuted
        }
    }
}

enum AgentDiffLineKind: Equatable {
    case addition
    case deletion
    case hunk
    case meta
    case context

    static func classify(_ line: String) -> AgentDiffLineKind {
        if line.hasPrefix("@@") {
            return .hunk
        }
        if line.hasPrefix("+++") || line.hasPrefix("---")
            || line.hasPrefix("diff --git") || line.hasPrefix("index ")
            || line.hasPrefix("new file") || line.hasPrefix("deleted file") {
            return .meta
        }
        if line.hasPrefix("+") {
            return .addition
        }
        if line.hasPrefix("-") {
            return .deletion
        }
        return .context
    }
}

struct AgentCodeHighlighter: CodeSyntaxHighlighter {
    static let shared = AgentCodeHighlighter()
    private static let tokenCache = AgentCodeTokenCache(limit: 96)

    enum TokenKind: String, Equatable, Sendable {
        case plain
        case keyword
        case string
        case comment
        case number

        var color: Color {
            switch self {
            case .plain:
                return Color(nsColor: nsColor)
            case .keyword:
                return Color(nsColor: nsColor)
            case .string:
                return Color(nsColor: nsColor)
            case .comment:
                return Color(nsColor: nsColor)
            case .number:
                return Color(nsColor: nsColor)
            }
        }

        var nsColor: NSColor {
            switch self {
            case .plain:
                return .labelColor
            case .keyword:
                return .dmuxHex(0xC678DD)
            case .string:
                return .dmuxHex(0x98C379)
            case .comment:
                return .secondaryLabelColor
            case .number:
                return .dmuxHex(0xD19A66)
            }
        }
    }

    struct Token: Equatable, Sendable {
        let kind: TokenKind
        let text: String
    }

    static let keywords: Set<String> = [
        "func", "let", "var", "const", "if", "else", "return", "import", "from",
        "struct", "class", "enum", "extension", "protocol", "public", "private",
        "internal", "static", "final", "switch", "case", "default", "for", "while",
        "do", "try", "catch", "throw", "throws", "guard", "async", "await",
        "true", "false", "nil", "null", "None", "True", "False", "self", "this",
        "function", "def", "lambda", "package", "interface", "type", "yield",
        "break", "continue", "new", "delete", "typeof", "instanceof",
        "fn", "pub", "use", "mut", "impl", "match", "where", "as", "in", "is",
        "and", "or", "not", "with", "echo", "export", "module"
    ]

    func highlightCode(_ code: String, language _: String?) -> Text {
        let tokens = Self.cachedTokens(for: code)
        return tokens.reduce(Text("")) { partial, token in
            partial + Text(token.text).foregroundColor(token.kind.color)
        }
    }

    static func cachedTokens(for code: String) -> [Token] {
        if let tokens = tokenCache.tokens(for: code) {
            return tokens
        }
        let tokens = tokenize(code)
        tokenCache.store(tokens, for: code)
        return tokens
    }

    static func tokenize(_ code: String) -> [Token] {
        var tokens: [Token] = []
        let chars = Array(code)
        let count = chars.count
        var index = 0
        var plainStart = 0

        func flushPlain(upTo end: Int) {
            if end > plainStart {
                tokens.append(Token(kind: .plain, text: String(chars[plainStart ..< end])))
            }
        }

        while index < count {
            let current = chars[index]

            if current == "/", index + 1 < count, chars[index + 1] == "/" {
                flushPlain(upTo: index)
                var cursor = index + 2
                while cursor < count, chars[cursor] != "\n" { cursor += 1 }
                tokens.append(Token(kind: .comment, text: String(chars[index ..< cursor])))
                index = cursor
                plainStart = cursor
                continue
            }

            if current == "/", index + 1 < count, chars[index + 1] == "*" {
                flushPlain(upTo: index)
                var cursor = index + 2
                while cursor + 1 < count, !(chars[cursor] == "*" && chars[cursor + 1] == "/") {
                    cursor += 1
                }
                let end = min(cursor + 2, count)
                tokens.append(Token(kind: .comment, text: String(chars[index ..< end])))
                index = end
                plainStart = end
                continue
            }

            if current == "#", isAtLineStart(chars: chars, index: index) {
                flushPlain(upTo: index)
                var cursor = index
                while cursor < count, chars[cursor] != "\n" { cursor += 1 }
                tokens.append(Token(kind: .comment, text: String(chars[index ..< cursor])))
                index = cursor
                plainStart = cursor
                continue
            }

            if current == "\"" || current == "'" || current == "`" {
                flushPlain(upTo: index)
                let quote = current
                var cursor = index + 1
                while cursor < count {
                    if chars[cursor] == "\\", cursor + 1 < count {
                        cursor += 2
                        continue
                    }
                    if chars[cursor] == quote {
                        cursor += 1
                        break
                    }
                    if chars[cursor] == "\n", quote != "`" {
                        break
                    }
                    cursor += 1
                }
                tokens.append(Token(kind: .string, text: String(chars[index ..< cursor])))
                index = cursor
                plainStart = cursor
                continue
            }

            if current.isNumber, isAtWordBoundary(chars: chars, index: index) {
                flushPlain(upTo: index)
                var cursor = index
                while cursor < count, chars[cursor].isNumber || chars[cursor] == "." {
                    cursor += 1
                }
                tokens.append(Token(kind: .number, text: String(chars[index ..< cursor])))
                index = cursor
                plainStart = cursor
                continue
            }

            if (current.isLetter || current == "_"), isAtWordBoundary(chars: chars, index: index) {
                var cursor = index
                while cursor < count, chars[cursor].isLetter || chars[cursor].isNumber || chars[cursor] == "_" {
                    cursor += 1
                }
                let word = String(chars[index ..< cursor])
                if Self.keywords.contains(word) {
                    flushPlain(upTo: index)
                    tokens.append(Token(kind: .keyword, text: word))
                    plainStart = cursor
                }
                index = cursor
                continue
            }

            index += 1
        }
        flushPlain(upTo: count)
        return tokens
    }

    private static func isAtLineStart(chars: [Character], index: Int) -> Bool {
        var cursor = index - 1
        while cursor >= 0 {
            let character = chars[cursor]
            if character == " " || character == "\t" {
                cursor -= 1
                continue
            }
            return character == "\n"
        }
        return true
    }

    private static func isAtWordBoundary(chars: [Character], index: Int) -> Bool {
        guard index > 0 else { return true }
        let character = chars[index - 1]
        return !(character.isLetter || character.isNumber || character == "_")
    }
}

struct AgentSelectableCodeTextView: NSViewRepresentable {
    let text: String
    let language: String?

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context _: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder

        let textView = NSTextView()
        textView.drawsBackground = false
        textView.isEditable = false
        textView.isSelectable = true
        textView.isRichText = true
        textView.importsGraphics = false
        textView.usesFontPanel = false
        textView.allowsUndo = false
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = false
        textView.textContainerInset = NSSize(width: 0, height: 0)
        textView.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        textView.textColor = .labelColor
        scrollView.documentView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else { return }
        guard context.coordinator.lastText != text else { return }
        context.coordinator.lastText = text
        textView.textStorage?.setAttributedString(
            AgentCodeAttributedCache.shared.attributedString(for: text)
        )
    }

    final class Coordinator {
        var lastText: String?
    }
}

struct AgentSelectableDiffTextView: NSViewRepresentable {
    let text: String

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context _: Context) -> NSScrollView {
        let scrollView = NSScrollView()
        scrollView.drawsBackground = false
        scrollView.hasVerticalScroller = true
        scrollView.hasHorizontalScroller = true
        scrollView.autohidesScrollers = true
        scrollView.borderType = .noBorder

        let textView = NSTextView()
        textView.drawsBackground = false
        textView.isEditable = false
        textView.isSelectable = true
        textView.isRichText = true
        textView.importsGraphics = false
        textView.usesFontPanel = false
        textView.allowsUndo = false
        textView.isHorizontallyResizable = true
        textView.isVerticallyResizable = true
        textView.textContainer?.containerSize = NSSize(width: CGFloat.greatestFiniteMagnitude, height: CGFloat.greatestFiniteMagnitude)
        textView.textContainer?.widthTracksTextView = false
        textView.textContainerInset = NSSize(width: 0, height: 0)
        textView.font = .monospacedSystemFont(ofSize: 12, weight: .regular)
        scrollView.documentView = textView
        return scrollView
    }

    func updateNSView(_ scrollView: NSScrollView, context: Context) {
        guard let textView = scrollView.documentView as? NSTextView else { return }
        guard context.coordinator.lastText != text else { return }
        context.coordinator.lastText = text
        textView.textStorage?.setAttributedString(
            AgentDiffAttributedCache.shared.attributedString(for: text)
        )
    }

    final class Coordinator {
        var lastText: String?
    }
}

private final class AgentCodeAttributedCache: @unchecked Sendable {
    static let shared = AgentCodeAttributedCache(limit: 128)

    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: NSAttributedString] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func attributedString(for code: String) -> NSAttributedString {
        let key = AgentContentCacheKey.key(for: code)
        lock.lock()
        if let value = storage[key] {
            lock.unlock()
            return value
        }
        lock.unlock()

        let attributedText = NSMutableAttributedString()
        let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        for token in AgentCodeHighlighter.cachedTokens(for: code) {
            attributedText.append(
                NSAttributedString(
                    string: token.text,
                    attributes: [
                        .font: font,
                        .foregroundColor: token.kind.nsColor,
                    ]
                )
            )
        }

        lock.lock()
        storage[key] = attributedText
        keys.append(key)
        trimIfNeeded()
        lock.unlock()
        return attributedText
    }

    private func trimIfNeeded() {
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
    }
}

private final class AgentDiffAttributedCache: @unchecked Sendable {
    static let shared = AgentDiffAttributedCache(limit: 128)

    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: NSAttributedString] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func attributedString(for diff: String) -> NSAttributedString {
        let key = AgentContentCacheKey.key(for: diff)
        lock.lock()
        if let value = storage[key] {
            lock.unlock()
            return value
        }
        lock.unlock()

        let attributedText = NSMutableAttributedString()
        let font = NSFont.monospacedSystemFont(ofSize: 12, weight: .regular)
        let lines = diff.components(separatedBy: "\n")
        for (index, line) in lines.enumerated() {
            let kind = AgentDiffLineKind.classify(line)
            let displayLine = line.isEmpty ? " " : line
            attributedText.append(
                NSAttributedString(
                    string: displayLine + (index == lines.indices.last ? "" : "\n"),
                    attributes: [
                        .font: font,
                        .foregroundColor: AgentDiffTextAttributes.foreground(for: kind),
                        .backgroundColor: AgentDiffTextAttributes.background(for: kind),
                    ]
                )
            )
        }

        lock.lock()
        storage[key] = attributedText
        keys.append(key)
        trimIfNeeded()
        lock.unlock()
        return attributedText
    }

    private func trimIfNeeded() {
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
    }
}

private enum AgentDiffTextAttributes {
    static func foreground(for kind: AgentDiffLineKind) -> NSColor {
        switch kind {
        case .addition:
            return NSColor.systemGreen
        case .deletion:
            return NSColor.systemRed
        case .hunk:
            return NSColor.systemOrange
        case .meta:
            return NSColor.tertiaryLabelColor
        case .context:
            return NSColor.secondaryLabelColor
        }
    }

    static func background(for kind: AgentDiffLineKind) -> NSColor {
        switch kind {
        case .addition:
            return NSColor.systemGreen.withAlphaComponent(0.08)
        case .deletion:
            return NSColor.systemRed.withAlphaComponent(0.08)
        default:
            return .clear
        }
    }
}

private final class AgentCodeTokenCache: @unchecked Sendable {
    private let limit: Int
    private let lock = NSLock()
    private var storage: [String: [AgentCodeHighlighter.Token]] = [:]
    private var keys: [String] = []

    init(limit: Int) {
        self.limit = max(1, limit)
    }

    func tokens(for code: String) -> [AgentCodeHighlighter.Token]? {
        lock.lock()
        defer { lock.unlock() }
        return storage[code]
    }

    func store(_ tokens: [AgentCodeHighlighter.Token], for code: String) {
        lock.lock()
        defer { lock.unlock() }
        if storage[code] == nil {
            keys.append(code)
        }
        storage[code] = tokens
        while keys.count > limit, let oldest = keys.first {
            keys.removeFirst()
            storage.removeValue(forKey: oldest)
        }
    }
}
