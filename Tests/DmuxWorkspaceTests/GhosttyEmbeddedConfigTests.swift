import AppKit
import XCTest
@testable import DmuxWorkspace

final class GhosttyEmbeddedConfigTests: XCTestCase {
    func testCollectsGhosttyConfigFilesInPriorityOrder() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let xdg = root.appendingPathComponent(
            ".config/ghostty/config.ghostty",
            isDirectory: false
        )
        let legacy = root.appendingPathComponent(
            "Library/Application Support/com.mitchellh.ghostty/config",
            isDirectory: false
        )
        let modern = root.appendingPathComponent(
            "Library/Application Support/com.mitchellh.ghostty/config.ghostty",
            isDirectory: false
        )
        try FileManager.default.createDirectory(
            at: xdg.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try FileManager.default.createDirectory(
            at: modern.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try "xdg".write(to: xdg, atomically: true, encoding: .utf8)
        try "legacy".write(to: legacy, atomically: true, encoding: .utf8)
        try "modern".write(to: modern, atomically: true, encoding: .utf8)

        let resolved = GhosttyEmbeddedConfig.resolvedUserConfigFileURLs(
            homeDirectoryURL: root
        )
        XCTAssertEqual(resolved.map(\.path), [xdg.path, modern.path, legacy.path])
    }

    func testFallsBackToLegacyGhosttyConfigFileWhenNeeded() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let legacy = root.appendingPathComponent(
            "Library/Application Support/com.mitchellh.ghostty/config",
            isDirectory: false
        )
        try FileManager.default.createDirectory(
            at: legacy.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try "legacy".write(to: legacy, atomically: true, encoding: .utf8)

        let resolved = GhosttyEmbeddedConfig.resolvedUserConfigFileURLs(
            homeDirectoryURL: root
        )
        XCTAssertEqual(resolved.map(\.path), [legacy.path])
    }

    func testEmbeddedDefaultConfigurationIncludesMacEditingBindings() {
        let rendered = GhosttyEmbeddedConfig.fallbackEditingConfigContents()

        XCTAssertTrue(rendered.contains("keybind = cmd+left=text:\\x01"))
        XCTAssertTrue(rendered.contains("keybind = cmd+right=text:\\x05"))
        XCTAssertTrue(rendered.contains("keybind = option+left=text:\\x1bb"))
        XCTAssertTrue(rendered.contains("keybind = option+right=text:\\x1bf"))
        XCTAssertTrue(rendered.contains("keybind = cmd+backspace=text:\\x15"))
        XCTAssertTrue(rendered.contains("keybind = option+backspace=text:\\x17"))
    }

    func testMergedUserConfigPrependsFallbackEditingBindings() {
        let url = URL(fileURLWithPath: "/tmp/ghostty-config-test")
        let rendered = GhosttyEmbeddedConfig
            .mergedUserConfigContents([(url, "font-size = 13\nadjust-cursor-thickness = 10\nkeybind = cmd+left=text:\\x02")])

        XCTAssertTrue(rendered.contains("keybind = cmd+left=text:\\x01"))
        XCTAssertTrue(rendered.contains("keybind = option+left=text:\\x1bb"))
        XCTAssertTrue(rendered.contains("# Source: /tmp/ghostty-config-test"))
        XCTAssertTrue(rendered.contains("font-size = 13"))
        XCTAssertTrue(rendered.contains("keybind = cmd+left=text:\\x02"))
        XCTAssertFalse(rendered.contains("adjust-cursor-thickness = 10"))
    }

    func testResolvedControllerConfigUsesGeneratedMergedSourceForUserConfig() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent(UUID().uuidString, isDirectory: true)
        try FileManager.default.createDirectory(at: root, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        let legacy = root.appendingPathComponent(
            "Library/Application Support/com.mitchellh.ghostty/config",
            isDirectory: false
        )
        try FileManager.default.createDirectory(
            at: legacy.deletingLastPathComponent(),
            withIntermediateDirectories: true
        )
        try "font-size = 14".write(to: legacy, atomically: true, encoding: .utf8)

        let resolved = GhosttyEmbeddedConfig.resolvedControllerConfig(homeDirectoryURL: root)

        XCTAssertTrue(resolved.prefersUserConfig)
        XCTAssertEqual(resolved.userConfigPaths, [legacy.path])
        guard case let .generated(contents) = resolved.configSource else {
            return XCTFail("expected generated config source")
        }
        XCTAssertTrue(contents.contains("keybind = cmd+left=text:\\x01"))
        XCTAssertTrue(contents.contains("font-size = 14"))
    }

    func testAutomaticAppearanceUsesKnownGhosttyThemeName() {
        let fallback = AppTerminalBackgroundPreset.tokyoNightStorm
            .effectiveAppearance(backgroundColorPreset: .automatic)

        let appearance = GhosttyEmbeddedConfig.automaticTerminalAppearance(
            from: [(URL(fileURLWithPath: "/tmp/ghostty-config"), "theme = Dracula")],
            fallback: fallback
        )

        XCTAssertFalse(appearance.isLight)
        XCTAssertGreaterThan(
            colorDistance(appearance.backgroundColor, fallback.backgroundColor),
            0.02
        )
    }

    func testAutomaticAppearanceAppliesExplicitGhosttyColorOverrides() {
        let fallback = AppTerminalBackgroundPreset.tokyoNightStorm
            .effectiveAppearance(backgroundColorPreset: .automatic)

        let config = """
        theme = Dracula
        background = #FDF6E3
        foreground = #073642
        selection-background = #EEE8D5
        """

        let appearance = GhosttyEmbeddedConfig.automaticTerminalAppearance(
            from: [(URL(fileURLWithPath: "/tmp/ghostty-config"), config)],
            fallback: fallback
        )

        XCTAssertColor(appearance.backgroundColor, approximatelyHex: 0xFDF6E3, tolerance: 20)
        XCTAssertColor(appearance.foregroundColor, approximatelyHex: 0x073642, tolerance: 20)
        XCTAssertColor(appearance.selectionBackgroundColor, approximatelyHex: 0xEEE8D5, tolerance: 20)
        XCTAssertTrue(appearance.isLight)
    }

    func testEmbeddedTerminalConfigurationDoesNotOverrideUserCursorPreferences() {
        let rendered = GhosttyEmbeddedConfig.terminalConfiguration().rendered

        XCTAssertFalse(rendered.contains("cursor-style ="))
        XCTAssertFalse(rendered.contains("cursor-style-blink ="))
        XCTAssertTrue(rendered.contains("background-opacity = 1"))
        XCTAssertTrue(rendered.contains("background-blur = 0"))
        XCTAssertTrue(rendered.contains("window-padding-x = 0"))
        XCTAssertTrue(rendered.contains("window-padding-y = 0"))
    }
}

private func XCTAssertColor(
    _ color: NSColor,
    approximatelyHex expectedHex: UInt,
    tolerance: Int,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    let resolved = color.usingColorSpace(.deviceRGB) ?? color
    let actual = (
        red: Int(round(resolved.redComponent * 255)),
        green: Int(round(resolved.greenComponent * 255)),
        blue: Int(round(resolved.blueComponent * 255))
    )
    let expected = (
        red: Int((expectedHex >> 16) & 0xFF),
        green: Int((expectedHex >> 8) & 0xFF),
        blue: Int(expectedHex & 0xFF)
    )

    XCTAssertLessThanOrEqual(abs(actual.red - expected.red), tolerance, file: file, line: line)
    XCTAssertLessThanOrEqual(abs(actual.green - expected.green), tolerance, file: file, line: line)
    XCTAssertLessThanOrEqual(abs(actual.blue - expected.blue), tolerance, file: file, line: line)
}

private func colorDistance(_ lhs: NSColor, _ rhs: NSColor) -> CGFloat {
    let left = lhs.usingColorSpace(.deviceRGB) ?? lhs
    let right = rhs.usingColorSpace(.deviceRGB) ?? rhs
    let red = left.redComponent - right.redComponent
    let green = left.greenComponent - right.greenComponent
    let blue = left.blueComponent - right.blueComponent
    return sqrt((red * red) + (green * green) + (blue * blue))
}
