import XCTest
@testable import DmuxWorkspace

final class AppTerminalBackgroundPresetTests: XCTestCase {
    func testBackgroundColorPresetsIncludeAutomaticAndFlexokiSwatches() {
        XCTAssertEqual(AppBackgroundColorPreset.allCases.first, .automatic)
        XCTAssertTrue(AppBackgroundColorPreset.allCases.contains(.black))
        XCTAssertTrue(AppBackgroundColorPreset.allCases.contains(.paper))
        XCTAssertTrue(AppBackgroundColorPreset.allCases.contains(.red600))
        XCTAssertTrue(AppBackgroundColorPreset.allCases.contains(.blue400))
        XCTAssertTrue(AppBackgroundColorPreset.allCases.contains(.magenta400))
    }

    func testCuratedThemesAreExposedInSettings() {
        XCTAssertEqual(AppTerminalBackgroundPreset.allCases.count, 28)
        XCTAssertEqual(AppTerminalBackgroundPreset.allCases.first, .automatic)
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.catppuccinMocha))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.dracula))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.flexokiDark))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.flexokiLight))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.githubDark))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.rosePine))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.nightOwl))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.kanagawaLotus))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.githubLight))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.ayuLight))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.draculaPlus))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.rosePineMoon))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.tokyoNightStorm))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.materialOcean))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.gruvboxMaterialLight))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.atomOneLight))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.nordLight))
        XCTAssertTrue(AppTerminalBackgroundPreset.allCases.contains(.ayuMirage))
    }

    func testFlexokiPresetsKeepExpectedLightDarkSemantics() {
        XCTAssertFalse(AppTerminalBackgroundPreset.flexokiDark.isLight)
        XCTAssertTrue(AppTerminalBackgroundPreset.flexokiLight.isLight)

        XCTAssertLessThan(
            AppTerminalBackgroundPreset.flexokiDark.backgroundColor.perceivedBrightness,
            AppTerminalBackgroundPreset.flexokiLight.backgroundColor.perceivedBrightness
        )
        XCTAssertLessThan(
            AppTerminalBackgroundPreset.flexokiDark.backgroundColor.perceivedBrightness,
            0.15
        )
        XCTAssertGreaterThan(
            AppTerminalBackgroundPreset.flexokiLight.backgroundColor.perceivedBrightness,
            0.95
        )
    }

    func testLegacyPresetNamesMigrateToCuratedThemes() throws {
        XCTAssertEqual(try decodePreset("auto"), .automatic)
        XCTAssertEqual(try decodePreset("obsidian"), .flexokiDark)
        XCTAssertEqual(try decodePreset("midnight"), .tokyoNight)
        XCTAssertEqual(try decodePreset("sand"), .gruvboxLight)
        XCTAssertEqual(try decodePreset("mist"), .catppuccinLatte)
    }

    func testExpandedCatalogStillCoversLightAndDarkFamilies() {
        let lightThemes = AppTerminalBackgroundPreset.allCases.filter(\.isLight)
        let darkThemes = AppTerminalBackgroundPreset.allCases.filter { !$0.isLight }

        XCTAssertGreaterThanOrEqual(lightThemes.count, 10)
        XCTAssertGreaterThanOrEqual(darkThemes.count, 14)
        XCTAssertTrue(lightThemes.contains(.rosePineDawn))
        XCTAssertTrue(lightThemes.contains(.kanagawaLotus))
        XCTAssertTrue(lightThemes.contains(.atomOneLight))
        XCTAssertTrue(lightThemes.contains(.nordLight))
        XCTAssertTrue(darkThemes.contains(.nightOwl))
        XCTAssertTrue(darkThemes.contains(.everforestDarkHard))
        XCTAssertTrue(darkThemes.contains(.draculaPlus))
        XCTAssertTrue(darkThemes.contains(.materialOcean))
        XCTAssertTrue(darkThemes.contains(.ayuMirage))
    }

    func testAutomaticBackgroundOverridePreservesThemeBackground() {
        let automatic = AppTerminalBackgroundPreset.flexokiDark
            .effectiveAppearance(backgroundColorPreset: .automatic)
        let overridden = AppTerminalBackgroundPreset.flexokiDark
            .effectiveAppearance(backgroundColorPreset: .paper)

        XCTAssertColor(automatic.backgroundColor, approximatelyHex: 0x100F0F, tolerance: 6)
        XCTAssertColor(overridden.backgroundColor, approximatelyHex: 0xFFFCF0, tolerance: 6)
        XCTAssertTrue(overridden.isLight)
    }

    private func decodePreset(_ rawValue: String) throws -> AppTerminalBackgroundPreset {
        let data = Data("\"\(rawValue)\"".utf8)
        return try JSONDecoder().decode(AppTerminalBackgroundPreset.self, from: data)
    }
}

private extension NSColor {
    var perceivedBrightness: CGFloat {
        let resolved = usingColorSpace(.deviceRGB) ?? self
        return (resolved.redComponent * 0.299) + (resolved.greenComponent * 0.587) + (resolved.blueComponent * 0.114)
    }

    var rgbComponents255: (red: Int, green: Int, blue: Int) {
        let resolved = usingColorSpace(.deviceRGB) ?? self
        let red = Int(round(resolved.redComponent * 255))
        let green = Int(round(resolved.greenComponent * 255))
        let blue = Int(round(resolved.blueComponent * 255))
        return (red, green, blue)
    }
}

private func XCTAssertColor(
    _ color: NSColor,
    approximatelyHex expectedHex: UInt,
    tolerance: Int,
    file: StaticString = #filePath,
    line: UInt = #line
) {
    let actual = color.rgbComponents255
    let expected = (
        red: Int((expectedHex >> 16) & 0xFF),
        green: Int((expectedHex >> 8) & 0xFF),
        blue: Int(expectedHex & 0xFF)
    )

    XCTAssertLessThanOrEqual(abs(actual.red - expected.red), tolerance, file: file, line: line)
    XCTAssertLessThanOrEqual(abs(actual.green - expected.green), tolerance, file: file, line: line)
    XCTAssertLessThanOrEqual(abs(actual.blue - expected.blue), tolerance, file: file, line: line)
}
