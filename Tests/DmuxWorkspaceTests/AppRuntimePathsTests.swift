import XCTest
@testable import DmuxWorkspace

final class AppRuntimePathsTests: XCTestCase {
    func testAppSupportFolderNameUsesApplicationDisplayName() {
        XCTAssertEqual(
            AppRuntimePaths.appSupportFolderName(appDisplayName: "Codux"),
            "Codux"
        )
        XCTAssertEqual(
            AppRuntimePaths.appSupportFolderName(appDisplayName: "Codux-dev"),
            "Codux-dev"
        )
    }

    func testRuntimeOwnerIDSanitizesDisplayName() {
        XCTAssertEqual(
            AppRuntimePaths.runtimeOwnerID(appDisplayName: "Codux"),
            "codux"
        )
        XCTAssertEqual(
            AppRuntimePaths.runtimeOwnerID(appDisplayName: "Codux Dev"),
            "codux-dev"
        )
        XCTAssertEqual(
            AppRuntimePaths.runtimeOwnerID(appDisplayName: "Codux/dev"),
            "codux-dev"
        )
    }

    func testTemporaryRootUsesRuntimeOwner() {
        let fileManager = FileManager.default

        let releaseRoot = AppRuntimePaths.temporaryRootURL(
            fileManager: fileManager,
            appDisplayName: "Codux",
            bundleIdentifier: "com.duxweb.dmux"
        )
        let devRoot = AppRuntimePaths.temporaryRootURL(
            fileManager: fileManager,
            appDisplayName: "Codux-dev",
            bundleIdentifier: "com.duxweb.dmux.dev"
        )

        XCTAssertTrue(releaseRoot.lastPathComponent == "codux")
        XCTAssertTrue(devRoot.lastPathComponent == "codux-dev")
        XCTAssertNotEqual(releaseRoot, devRoot)
    }

    func testRunningUnderXCTestIsDetectedFromEnvironment() {
        XCTAssertTrue(AppRuntimePaths.isRunningUnderXCTest())
    }
}
