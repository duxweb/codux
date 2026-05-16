import XCTest
@testable import DmuxWorkspace

final class AgentMessageRenderingTests: XCTestCase {
    func testTimelinePresentationGroupsActivityBetweenUserAndAssistant() {
        let now = Date()
        let items = [
            timelineItem(id: "user", kind: .userPrompt, content: "hello", createdAt: now),
            timelineItem(id: "cmd", kind: .command, title: "pwd", content: "/tmp", createdAt: now.addingTimeInterval(1)),
            timelineItem(id: "tool", kind: .tool, title: "search", content: "result", createdAt: now.addingTimeInterval(2)),
            timelineItem(id: "assistant", kind: .assistantMessage, content: "done", createdAt: now.addingTimeInterval(3)),
        ]

        let entries = AgentTimelinePresentation.entries(from: items)

        XCTAssertEqual(entries.count, 3)
        guard case .item(let first) = entries[0],
              case .activity(let activity) = entries[1],
              case .item(let last) = entries[2] else {
            return XCTFail("Expected user, activity, assistant entries.")
        }
        XCTAssertEqual(first.kind, .userPrompt)
        XCTAssertEqual(activity.items.map(\.id), ["cmd", "tool"])
        XCTAssertEqual(activity.commandCount, 1)
        XCTAssertEqual(activity.toolCount, 1)
        XCTAssertEqual(activity.status, .completed)
        XCTAssertEqual(last.kind, .assistantMessage)
    }

    func testTimelinePresentationKeepsSeparateActivityRuns() {
        let now = Date()
        let items = [
            timelineItem(id: "user-1", kind: .userPrompt, content: "one", createdAt: now),
            timelineItem(id: "cmd-1", kind: .command, status: .completed, createdAt: now.addingTimeInterval(1)),
            timelineItem(id: "assistant-1", kind: .assistantMessage, content: "done", createdAt: now.addingTimeInterval(2)),
            timelineItem(id: "user-2", kind: .userPrompt, content: "two", createdAt: now.addingTimeInterval(3)),
            timelineItem(id: "cmd-2", kind: .command, status: .running, createdAt: now.addingTimeInterval(4)),
        ]

        let activityEntries = AgentTimelinePresentation.entries(from: items).compactMap { entry -> AgentActivityGroup? in
            if case .activity(let group) = entry { return group }
            return nil
        }

        XCTAssertEqual(activityEntries.count, 2)
        XCTAssertEqual(activityEntries[0].items.map(\.id), ["cmd-1"])
        XCTAssertEqual(activityEntries[0].status, .completed)
        XCTAssertEqual(activityEntries[1].items.map(\.id), ["cmd-2"])
        XCTAssertEqual(activityEntries[1].status, .running)
    }

    func testTimelinePresentationKeepsActivityAtOriginalStreamPosition() {
        let now = Date()
        let items = [
            timelineItem(id: "user", kind: .userPrompt, content: "analyze", createdAt: now),
            timelineItem(id: "assistant-1", kind: .assistantMessage, content: "I will inspect files.", createdAt: now.addingTimeInterval(1)),
            timelineItem(id: "cmd-1", kind: .command, status: .completed, createdAt: now.addingTimeInterval(2)),
            timelineItem(id: "cmd-2", kind: .command, status: .running, createdAt: now.addingTimeInterval(3)),
            timelineItem(id: "assistant-2", kind: .assistantMessage, content: "Found the entrypoint.", createdAt: now.addingTimeInterval(4)),
        ]

        let entries = AgentTimelinePresentation.entries(from: items)

        XCTAssertEqual(entries.count, 4)
        guard case .item(let first) = entries[0],
              case .item(let second) = entries[1],
              case .activity(let activity) = entries[2],
              case .item(let fourth) = entries[3] else {
            return XCTFail("Expected user, assistant, activity, assistant entries.")
        }
        XCTAssertEqual(first.id, "user")
        XCTAssertEqual(second.id, "assistant-1")
        XCTAssertEqual(activity.items.map(\.id), ["cmd-1", "cmd-2"])
        XCTAssertEqual(activity.status, .running)
        XCTAssertTrue(activity.shouldDefaultExpand)
        XCTAssertEqual(fourth.id, "assistant-2")
    }

    func testActivityGroupRecognisesThinkingOnlyRuns() {
        let now = Date()
        let items = [
            timelineItem(id: "reasoning", kind: .reasoning, status: .running, createdAt: now),
            timelineItem(id: "plan", kind: .plan, status: .running, createdAt: now.addingTimeInterval(1)),
        ]

        let entries = AgentTimelinePresentation.entries(from: items)

        guard case .activity(let activity) = entries.first else {
            return XCTFail("Expected an activity group.")
        }
        XCTAssertTrue(activity.isThinkingOnly)
        XCTAssertEqual(activity.status, .running)
    }

    func testActivityGroupTreatsCommandsAsRunningWork() {
        let now = Date()
        let items = [
            timelineItem(id: "reasoning", kind: .reasoning, status: .running, createdAt: now),
            timelineItem(id: "command", kind: .command, status: .running, createdAt: now.addingTimeInterval(1)),
        ]

        let entries = AgentTimelinePresentation.entries(from: items)

        guard case .activity(let activity) = entries.first else {
            return XCTFail("Expected an activity group.")
        }
        XCTAssertFalse(activity.isThinkingOnly)
        XCTAssertEqual(activity.status, .running)
    }

    func testGitStatusListRecognisesPorcelainRows() {
        let status = AgentGitStatusList.parse(
            """
             M app/system/service/update.go
            ?? code.go
            ?? codex-file-write-test.md
            """
        )

        XCTAssertEqual(status?.entries.map(\.code), ["M", "??", "??"])
        XCTAssertEqual(status?.entries.map(\.path), [
            "app/system/service/update.go",
            "code.go",
            "codex-file-write-test.md",
        ])
    }

    func testGitStatusListIgnoresOrdinaryCommandOutput() {
        XCTAssertNil(AgentGitStatusList.parse("hello\nworld"))
        XCTAssertNil(AgentGitStatusList.parse(" M single-file.go\nhello"))
    }

    func testGitStatusListRecognisesSinglePorcelainRow() {
        let status = AgentGitStatusList.parse(" M single-file.go")

        XCTAssertEqual(status?.entries.map(\.code), ["M"])
        XCTAssertEqual(status?.entries.map(\.path), ["single-file.go"])
    }

    func testAgentPathRecognitionUsesShapeNotKnownFileNameList() {
        XCTAssertTrue("Sources/App.swift".isLikelyAgentFilePath)
        XCTAssertTrue("Package.swift".isLikelyAgentFilePath)
        XCTAssertFalse("Makefile".isLikelyAgentFilePath)
        XCTAssertFalse("README".isLikelyAgentFilePath)
        XCTAssertFalse("diff --git a/Sources/App.swift b/Sources/App.swift".isLikelyAgentFilePath)
        XCTAssertFalse("+added line".isLikelyAgentFilePath)
    }

    func testMarkdownBlockParserSplitsFencedCodeBlocks() {
        let segments = AgentMarkdownBlockParser.segments(
            from:
                """
                Before

                ```go
                package main
                ```

                After
                """
        )

        XCTAssertEqual(segments.count, 3)
        guard case .markdown(let before) = segments[0].kind,
              case .code(let language, let code) = segments[1].kind,
              case .markdown(let after) = segments[2].kind else {
            return XCTFail("Expected markdown, code, markdown segments.")
        }
        XCTAssertEqual(before.trimmingCharacters(in: .whitespacesAndNewlines), "Before")
        XCTAssertEqual(language, "go")
        XCTAssertEqual(code, "package main")
        XCTAssertEqual(after.trimmingCharacters(in: .whitespacesAndNewlines), "After")
    }

    func testMarkdownBlockParserKeepsUnclosedFenceAsMarkdown() {
        let segments = AgentMarkdownBlockParser.segments(
            from:
                """
                Before

                ```
                still streaming
                """
        )

        XCTAssertEqual(segments.count, 1)
        guard case .markdown(let markdown) = segments[0].kind else {
            return XCTFail("Expected unfinished fenced content to remain markdown.")
        }
        XCTAssertTrue(markdown.contains("still streaming"))
    }

    func testShortElapsedText() {
        XCTAssertEqual(AgentDurationFormatter.shortElapsedText(seconds: 8), "8s")
        XCTAssertEqual(AgentDurationFormatter.shortElapsedText(seconds: 65), "1m 5s")
    }

    func testVirtualListLayoutRendersOnlyVisibleWindowWithOverscan() {
        let ids = (0 ..< 100).map { "row-\($0)" }
        let layout = AgentVirtualListLayoutCalculator.layout(
            itemIDs: ids,
            measuredHeights: [:],
            estimatedRowHeight: 100,
            spacing: 10,
            viewportHeight: 300,
            scrollOffset: 2_000,
            overscan: 200
        )

        XCTAssertLessThan(layout.visibleRange.count, ids.count)
        XCTAssertEqual(layout.visibleRange, 16 ..< 23)
        XCTAssertEqual(layout.topSpacerHeight, 1_760, accuracy: 0.001)
        XCTAssertEqual(layout.totalContentHeight, 10_990, accuracy: 0.001)
        XCTAssertGreaterThan(layout.bottomSpacerHeight, 0)
    }

    func testVirtualListLayoutUsesMeasuredRowHeights() {
        let layout = AgentVirtualListLayoutCalculator.layout(
            itemIDs: ["a", "b", "c"],
            measuredHeights: ["a": 40, "b": 180, "c": 60],
            estimatedRowHeight: 100,
            spacing: 10,
            viewportHeight: 90,
            scrollOffset: 55,
            overscan: 0
        )

        XCTAssertEqual(layout.visibleRange, 1 ..< 2)
        XCTAssertEqual(layout.topSpacerHeight, 50, accuracy: 0.001)
        XCTAssertEqual(layout.bottomSpacerHeight, 70, accuracy: 0.001)
        XCTAssertEqual(layout.totalContentHeight, 300, accuracy: 0.001)
    }

    func testVirtualListLayoutHandlesEmptyList() {
        let layout = AgentVirtualListLayoutCalculator.layout(
            itemIDs: [],
            measuredHeights: [:],
            viewportHeight: 400,
            scrollOffset: 0
        )

        XCTAssertTrue(layout.visibleRange.isEmpty)
        XCTAssertEqual(layout.topSpacerHeight, 0)
        XCTAssertEqual(layout.bottomSpacerHeight, 0)
        XCTAssertEqual(layout.totalContentHeight, 0)
    }

    func testContentFoldStateUsesAutomaticDefault() {
        XCTAssertTrue(AgentContentFoldState.automatic.isExpanded(defaultExpanded: true))
        XCTAssertFalse(AgentContentFoldState.automatic.isExpanded(defaultExpanded: false))
        XCTAssertTrue(AgentContentFoldState.expanded.isExpanded(defaultExpanded: false))
        XCTAssertFalse(AgentContentFoldState.collapsed.isExpanded(defaultExpanded: true))
    }

    func testMessageTextPreviewDoesNotFoldShortContent() {
        let text = "hello\nworld"
        let preview = AgentMessageTextPreview(content: text)

        XCTAssertFalse(preview.shouldFold)
        XCTAssertEqual(preview.previewText, text)
        XCTAssertEqual(preview.omittedLineCount, 0)
        XCTAssertEqual(preview.omittedCharacterCount, 0)
    }

    func testMessageTextPreviewFoldsLineHeavyContent() {
        let total = AgentMessageTextPreview.lineThreshold + 12
        let text = (0 ..< total).map { "line \($0)" }.joined(separator: "\n")
        let preview = AgentMessageTextPreview(content: text)

        XCTAssertTrue(preview.shouldFold)
        XCTAssertEqual(preview.previewText.components(separatedBy: "\n").count, AgentMessageTextPreview.previewLineCount)
        XCTAssertEqual(preview.omittedLineCount, total - AgentMessageTextPreview.previewLineCount)
        XCTAssertGreaterThan(preview.omittedCharacterCount, 0)
        XCTAssertEqual(preview.hiddenSummary, "\(preview.omittedLineCount)")
    }

    func testMessageTextPreviewFoldsLongSingleLineContent() {
        let text = String(repeating: "a", count: AgentMessageTextPreview.characterThreshold + 500)
        let preview = AgentMessageTextPreview(content: text)

        XCTAssertTrue(preview.shouldFold)
        XCTAssertEqual(preview.previewText.count, AgentMessageTextPreview.previewCharacterLimit)
        XCTAssertEqual(preview.omittedLineCount, 0)
        XCTAssertEqual(preview.omittedCharacterCount, text.count - AgentMessageTextPreview.previewCharacterLimit)
        XCTAssertEqual(preview.hiddenSummary, "\(preview.omittedCharacterCount)")
    }

    func testStreamingMarkdownKeepsCompletedLinesStableAndTailPlain() {
        let parts = AgentStreamingMarkdownParts(content: "# Title\nstreaming **tail")

        XCTAssertEqual(parts.stableMarkdown, "# Title")
        XCTAssertEqual(parts.streamingPlainTail, "\nstreaming **tail")
        XCTAssertFalse(parts.isFullyStable)
    }

    func testStreamingMarkdownTreatsTrailingNewlineAsStableMarkdown() {
        let parts = AgentStreamingMarkdownParts(content: "# Title\n\n- item\n")

        XCTAssertEqual(parts.stableMarkdown, "# Title\n\n- item\n")
        XCTAssertEqual(parts.streamingPlainTail, "")
        XCTAssertTrue(parts.isFullyStable)
    }

    func testStreamingMarkdownRendererFormatsStablePrefixOnly() {
        let parts = AgentStreamingMarkdownParts(content: "# Title\nstreaming **tail")
        let text = AgentMarkdownAttributedRenderer.streamingAttributedString(for: parts, style: .body).string

        XCTAssertEqual(text, "Title\nstreaming **tail")
    }

    func testSelectableMarkdownRendererPreservesMultilineText() {
        let markdown = "# Title\n\nhello **world**\n\n- first\n- second"
        let text = AgentMarkdownAttributedRenderer.visibleText(for: markdown)

        XCTAssertTrue(text.contains("Title\n"))
        XCTAssertTrue(text.contains("hello world"))
        XCTAssertTrue(text.contains("• first"))
        XCTAssertTrue(text.contains("• second"))
    }

    func testSelectableMarkdownRendererStripsInlineMarkers() {
        let markdown = "Use `cmd` with **strong** and *emphasis*."
        let text = AgentMarkdownAttributedRenderer.visibleText(for: markdown)

        XCTAssertEqual(text.trimmingCharacters(in: .whitespacesAndNewlines), "Use cmd with strong and emphasis.")
    }

    func testFoldedContentDoesNotFoldShortContent() {
        let text = (0 ..< 10).map { "line \($0)" }.joined(separator: "\n")
        let fold = AgentFoldedContent(content: text)

        XCTAssertEqual(fold.totalLineCount, 10)
        XCTAssertFalse(fold.isFolded)
        XCTAssertEqual(fold.hiddenLineCount, 0)
        XCTAssertEqual(fold.previewLines.count, 10)
        XCTAssertEqual(fold.previewText, text)
    }

    func testFoldedContentDoesNotFoldAtThreshold() {
        let text = (0 ..< AgentFoldedContent.foldThreshold).map { "line \($0)" }.joined(separator: "\n")
        let fold = AgentFoldedContent(content: text)

        XCTAssertEqual(fold.totalLineCount, AgentFoldedContent.foldThreshold)
        XCTAssertFalse(fold.isFolded)
    }

    func testFoldedContentFoldsAboveThreshold() {
        let total = AgentFoldedContent.foldThreshold + 5
        let text = (0 ..< total).map { "line \($0)" }.joined(separator: "\n")
        let fold = AgentFoldedContent(content: text)

        XCTAssertTrue(fold.isFolded)
        XCTAssertEqual(fold.previewLines.count, AgentFoldedContent.previewLineCount)
        XCTAssertEqual(fold.hiddenLineCount, total - AgentFoldedContent.previewLineCount)
        XCTAssertEqual(fold.fullText, text)
    }

    func testDiffLineKindClassification() {
        XCTAssertEqual(AgentDiffLineKind.classify("+added line"), .addition)
        XCTAssertEqual(AgentDiffLineKind.classify("-removed line"), .deletion)
        XCTAssertEqual(AgentDiffLineKind.classify("@@ -1,3 +1,4 @@"), .hunk)
        XCTAssertEqual(AgentDiffLineKind.classify("diff --git a/x b/x"), .meta)
        XCTAssertEqual(AgentDiffLineKind.classify("index 0000..1111"), .meta)
        XCTAssertEqual(AgentDiffLineKind.classify("--- a/x"), .meta)
        XCTAssertEqual(AgentDiffLineKind.classify("+++ b/x"), .meta)
        XCTAssertEqual(AgentDiffLineKind.classify("new file mode 100644"), .meta)
        XCTAssertEqual(AgentDiffLineKind.classify("deleted file mode 100644"), .meta)
        XCTAssertEqual(AgentDiffLineKind.classify(" context line"), .context)
        XCTAssertEqual(AgentDiffLineKind.classify(""), .context)
    }

    func testHighlighterRecognisesKeyword() {
        let tokens = AgentCodeHighlighter.tokenize("let value = 1")

        XCTAssertEqual(tokens.first?.kind, .keyword)
        XCTAssertEqual(tokens.first?.text, "let")
        XCTAssertTrue(tokens.contains { $0.kind == .number && $0.text == "1" })
    }

    func testHighlighterRecognisesString() {
        let tokens = AgentCodeHighlighter.tokenize(#"let name = "codux""#)

        let stringToken = tokens.first { $0.kind == .string }
        XCTAssertEqual(stringToken?.text, "\"codux\"")
    }

    func testHighlighterRecognisesBacktickStringSpanningLines() {
        let code = "const tpl = `a\nb`"
        let tokens = AgentCodeHighlighter.tokenize(code)
        let stringToken = tokens.first { $0.kind == .string }

        XCTAssertEqual(stringToken?.text, "`a\nb`")
    }

    func testHighlighterRecognisesLineCommentWithSlash() {
        let tokens = AgentCodeHighlighter.tokenize("let x = 1 // trailing")
        let comment = tokens.first { $0.kind == .comment }

        XCTAssertEqual(comment?.text, "// trailing")
    }

    func testHighlighterRecognisesHashCommentAtLineStart() {
        let tokens = AgentCodeHighlighter.tokenize("# header\necho hi")
        let comment = tokens.first { $0.kind == .comment }

        XCTAssertEqual(comment?.text, "# header")
    }

    func testHighlighterIgnoresHashInsideLine() {
        let tokens = AgentCodeHighlighter.tokenize("call(a, #ref)")

        XCTAssertFalse(tokens.contains { $0.kind == .comment })
    }

    func testHighlighterRecognisesBlockComment() {
        let tokens = AgentCodeHighlighter.tokenize("a /* note */ b")
        let comment = tokens.first { $0.kind == .comment }

        XCTAssertEqual(comment?.text, "/* note */")
    }

    func testHighlighterDoesNotMatchKeywordPrefix() {
        let tokens = AgentCodeHighlighter.tokenize("letter")

        XCTAssertFalse(tokens.contains { $0.kind == .keyword })
    }

    func testHighlighterPreservesOriginalText() {
        let code = "func add(a: Int, b: Int) -> Int { return a + b } // sum"
        let tokens = AgentCodeHighlighter.tokenize(code)
        let reconstructed = tokens.map(\.text).joined()

        XCTAssertEqual(reconstructed, code)
    }

    func testHighlighterRecognisesNumber() {
        let tokens = AgentCodeHighlighter.tokenize("answer = 42.5")
        let number = tokens.first { $0.kind == .number }

        XCTAssertEqual(number?.text, "42.5")
    }

    private func timelineItem(
        id: String,
        kind: AgentTimelineKind,
        title: String? = nil,
        content: String = "",
        status: AgentTimelineStatus = .completed,
        createdAt: Date
    ) -> AgentTimelineItem {
        AgentTimelineItem(
            id: id,
            turnID: nil,
            itemID: id,
            kind: kind,
            role: nil,
            title: title,
            content: content,
            detail: nil,
            status: status,
            createdAt: createdAt,
            updatedAt: createdAt
        )
    }
}
