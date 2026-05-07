import XCTest
@testable import DmuxWorkspace

final class ProjectFileBrowserServiceTests: XCTestCase {
    func testDirectoryChildrenSortFoldersFirstAndKeepsHiddenDirectories() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try FileManager.default.createDirectory(at: root.appendingPathComponent(".git", isDirectory: true), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: root.appendingPathComponent("Sources", isDirectory: true), withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: root.appendingPathComponent("node_modules", isDirectory: true), withIntermediateDirectories: true)
        try "readme".write(to: root.appendingPathComponent("README.md"), atomically: true, encoding: .utf8)

        let project = Project(
            id: UUID(),
            name: "Demo",
            path: root.path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
        let service = ProjectFileBrowserService()
        let children = try service.children(of: service.rootItem(for: project), rootURL: root)

        XCTAssertEqual(children.map(\.name), [".git", "node_modules", "Sources", "README.md"])
        XCTAssertTrue(children[0].isDirectory)
        XCTAssertFalse(children[3].isDirectory)
    }

    func testPreviewRejectsBinaryAndReadsText() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let swiftFile = root.appendingPathComponent("Demo.swift")
        try "struct Demo { let value = 1 }".write(to: swiftFile, atomically: true, encoding: .utf8)
        let binaryFile = root.appendingPathComponent("image.bin")
        try Data([0, 1, 2, 3]).write(to: binaryFile)

        let service = ProjectFileBrowserService()
        if case let .text(text) = service.preview(for: swiftFile, rootURL: root).state {
            XCTAssertTrue(text.string.contains("struct Demo"))
            XCTAssertGreaterThan(text.length, 0)
        } else {
            XCTFail("Expected text preview")
        }

        if case let .message(message) = service.preview(for: binaryFile, rootURL: root).state {
            XCTAssertFalse(message.isEmpty)
        } else {
            XCTFail("Expected binary message")
        }
    }

    func testPreviewAllowsEmptyTextFiles() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let fileURL = root.appendingPathComponent("empty.txt")
        try Data().write(to: fileURL)

        if case let .text(text) = ProjectFileBrowserService().preview(for: fileURL, rootURL: root).state {
            XCTAssertEqual(text.string, "")
        } else {
            XCTFail("Expected empty text preview")
        }
    }

    func testPreviewKeepsMediumLargeTextEditable() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let fileURL = root.appendingPathComponent("Large.swift")
        try "struct Demo { let value = 1 }\n".write(to: fileURL, atomically: true, encoding: .utf8)

        let preview = ProjectFileBrowserService(maxPreviewBytes: 1_024)
            .preview(for: fileURL, rootURL: root)

        if case let .text(text) = preview.state {
            XCTAssertTrue(text.string.contains("struct Demo"))
        } else {
            XCTFail("Expected editable text preview")
        }
    }

    func testPreviewUsesVirtualReadOnlyPreviewAboveMaximumEditableSize() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let fileURL = root.appendingPathComponent("Huge.log")
        try "line 1\nline 2\nline 3\n".write(to: fileURL, atomically: true, encoding: .utf8)

        let preview = ProjectFileBrowserService(maxPreviewBytes: 8)
            .preview(for: fileURL, rootURL: root)

        if case let .largeText(metadata) = preview.state {
            XCTAssertGreaterThan(metadata.totalBytes, 8)
            XCTAssertGreaterThan(metadata.estimatedLineCount, 0)
        } else {
            XCTFail("Expected large text virtual preview")
        }
    }

    func testSaveTextWritesOriginalFileInsideProjectRoot() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let fileURL = root.appendingPathComponent("Notes.md")
        try "old".write(to: fileURL, atomically: true, encoding: .utf8)

        try ProjectFileBrowserService().saveText("new\ncontent", to: fileURL, rootURL: root)

        XCTAssertEqual(try String(contentsOf: fileURL, encoding: .utf8), "new\ncontent")
    }

    func testSaveTextRejectsFileOutsideProjectRoot() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        let outsideRoot = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: outsideRoot, withIntermediateDirectories: true)
        defer {
            try? FileManager.default.removeItem(at: root)
            try? FileManager.default.removeItem(at: outsideRoot)
        }

        let outsideFileURL = outsideRoot.appendingPathComponent("Notes.md")
        try "old".write(to: outsideFileURL, atomically: true, encoding: .utf8)

        XCTAssertThrowsError(
            try ProjectFileBrowserService().saveText("new", to: outsideFileURL, rootURL: root)
        )
        XCTAssertEqual(try String(contentsOf: outsideFileURL, encoding: .utf8), "old")
    }

    func testSaveTextRejectsDirectory() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        XCTAssertThrowsError(
            try ProjectFileBrowserService().saveText("new", to: root, rootURL: root)
        )
        XCTAssertTrue(FileManager.default.fileExists(atPath: root.path))
    }

    func testMarkdownPreviewUsesSplitLayoutForMarkdownExtensions() {
        XCTAssertTrue(ProjectFilePreviewLayoutPolicy.usesMarkdownSplitPreview(fileExtension: "md"))
        XCTAssertTrue(ProjectFilePreviewLayoutPolicy.usesMarkdownSplitPreview(fileExtension: "markdown"))
        XCTAssertTrue(ProjectFilePreviewLayoutPolicy.usesMarkdownSplitPreview(fileExtension: "MD"))
        XCTAssertFalse(ProjectFilePreviewLayoutPolicy.usesMarkdownSplitPreview(fileExtension: "swift"))
        XCTAssertFalse(ProjectFilePreviewLayoutPolicy.usesMarkdownSplitPreview(fileExtension: "txt"))
    }

    @MainActor
    func testDeleteSelectedItemMarksPendingDeleteBeforeConfirmation() throws {
        let root = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let fileURL = root.appendingPathComponent("README.md")
        try "readme".write(to: fileURL, atomically: true, encoding: .utf8)

        let project = Project(
            id: UUID(),
            name: "Demo",
            path: root.path,
            shell: "/bin/zsh",
            defaultCommand: "",
            badgeText: nil,
            badgeSymbol: nil,
            badgeColorHex: nil,
            gitDefaultPushRemoteName: nil
        )
        let store = ProjectFileBrowserStore()
        store.load(project: project)

        guard let item = store.visibleRows.first(where: { $0.item.name == "README.md" })?.item else {
            XCTFail("Expected file row")
            return
        }

        store.select(item)
        store.deleteSelectedItem()

        XCTAssertEqual(store.pendingDeletePaths, [item.id])
        XCTAssertTrue(FileManager.default.fileExists(atPath: fileURL.path))

        store.cancelPendingDeletes()

        XCTAssertTrue(store.pendingDeletePaths.isEmpty)
        XCTAssertTrue(FileManager.default.fileExists(atPath: fileURL.path))
    }
}
