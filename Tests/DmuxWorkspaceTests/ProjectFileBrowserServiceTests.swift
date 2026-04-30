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

    func testPreviewRejectsBinaryAndHighlightsText() throws {
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
}
