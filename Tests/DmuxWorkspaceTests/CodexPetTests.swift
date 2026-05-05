import XCTest
@testable import DmuxWorkspace

final class CodexPetAtlasSpecTests: XCTestCase {
    func testOfficialAtlasDimensionsAndRows() {
        XCTAssertEqual(CodexPetAtlasSpec.columns, 8)
        XCTAssertEqual(CodexPetAtlasSpec.rows, 9)
        XCTAssertEqual(CodexPetAtlasSpec.cellWidth, 192)
        XCTAssertEqual(CodexPetAtlasSpec.cellHeight, 208)
        XCTAssertEqual(CodexPetAtlasSpec.atlasWidth, 1536)
        XCTAssertEqual(CodexPetAtlasSpec.atlasHeight, 1872)

        XCTAssertEqual(CodexPetAtlasSpec.animation(for: .idle).row, 0)
        XCTAssertEqual(CodexPetAtlasSpec.animation(for: .waiting).row, 6)
        XCTAssertEqual(CodexPetAtlasSpec.animation(for: .review).row, 8)
        XCTAssertEqual(CodexPetAtlasSpec.animation(for: .runningRight).frameCount, 8)
    }

    func testPlaybackUsesCalmerCycleAndPreservesAtlasFrameHoldWeights() {
        let animation = CodexPetAtlasSpec.animation(for: .idle)
        let durations = CodexPetPlaybackPolicy.frameDurations(
            for: animation,
            activeFrameCount: animation.frameCount
        )

        XCTAssertEqual(durations.count, animation.frameCount)
        XCTAssertEqual(
            durations.reduce(0, +),
            CodexPetPlaybackPolicy.baseFrameDuration * Double(animation.frameCount),
            accuracy: 0.001
        )
        XCTAssertGreaterThan(durations[0], durations[1])
        XCTAssertGreaterThan(durations[durations.count - 1], durations[1])
        XCTAssertGreaterThan(durations[0], 2.0)
    }

    func testFeedbackAnimationsAreNotGloballySpedUp() {
        for state in CodexPetAnimationState.allCases where state != .waiting {
            XCTAssertGreaterThanOrEqual(
                CodexPetPlaybackPolicy.cycleDurationMultiplier(for: state),
                1.0
            )
        }
    }

    func testPlaybackUsesActiveFrameCountWithoutSpeedingUpShortRows() {
        let animation = CodexPetAtlasSpec.animation(for: .idle)
        let durations = CodexPetPlaybackPolicy.frameDurations(
            for: animation,
            activeFrameCount: animation.frameCount - 1
        )

        XCTAssertEqual(durations.count, animation.frameCount - 1)
        XCTAssertEqual(
            durations.reduce(0, +),
            CodexPetPlaybackPolicy.baseFrameDuration * Double(animation.frameCount),
            accuracy: 0.001
        )
        XCTAssertGreaterThan(durations[0], durations[1])
    }

    func testPlaybackUsesActiveFrameCountWithoutRushingLongRows() {
        let animation = CodexPetAtlasSpec.animation(for: .idle)
        let activeFrameCount = animation.frameCount + 1
        let durations = CodexPetPlaybackPolicy.frameDurations(
            for: animation,
            activeFrameCount: activeFrameCount
        )

        XCTAssertEqual(durations.count, activeFrameCount)
        XCTAssertEqual(
            durations.reduce(0, +),
            CodexPetPlaybackPolicy.baseFrameDuration * Double(activeFrameCount),
            accuracy: 0.001
        )
        XCTAssertGreaterThan(durations[0], durations[1])
    }

    func testWaitingPlaybackIsSlowerForSleepState() {
        let idle = CodexPetAtlasSpec.animation(for: .idle)
        let waiting = CodexPetAtlasSpec.animation(for: .waiting)
        let idleTotal = CodexPetPlaybackPolicy.frameDurations(
            for: idle,
            activeFrameCount: idle.frameCount
        ).reduce(0, +)
        let waitingTotal = CodexPetPlaybackPolicy.frameDurations(
            for: waiting,
            activeFrameCount: waiting.frameCount
        ).reduce(0, +)

        XCTAssertEqual(
            waitingTotal,
            CodexPetPlaybackPolicy.baseFrameDuration
                * Double(waiting.frameCount)
                * CodexPetPlaybackPolicy.cycleDurationMultiplier(for: .waiting),
            accuracy: 0.001
        )
        XCTAssertGreaterThan(waitingTotal, idleTotal)
    }

    func testBundledAtlasesUseFlatSpeciesPackages() {
        for subdirectory in PetSpecies.allCases.map({ "Pets/\($0.assetFolder)" }) {
            XCTAssertNotNil(
                Bundle.module.url(
                    forResource: "spritesheet",
                    withExtension: "png",
                    subdirectory: subdirectory
                ),
                "Missing spritesheet for \(subdirectory)"
            )
            XCTAssertNotNil(
                Bundle.module.url(
                    forResource: "pet",
                    withExtension: "json",
                    subdirectory: subdirectory
                ),
                "Missing manifest for \(subdirectory)"
            )
        }
    }

    func testBundledPetResourcesExposeOnlyFlatPackages() throws {
        for species in PetSpecies.allCases.map(\.assetFolder) {
            let speciesURL = try XCTUnwrap(
                Bundle.module.url(forResource: species, withExtension: nil, subdirectory: "Pets"),
                "Missing bundled pet resource directory for \(species)"
            )
            let resourceURLs = try FileManager.default.contentsOfDirectory(
                at: speciesURL,
                includingPropertiesForKeys: [.isRegularFileKey, .isDirectoryKey],
                options: [.skipsHiddenFiles]
            )

            XCTAssertEqual(Set(resourceURLs.map(\.lastPathComponent)), Set(["pet.json", "spritesheet.png"]))

            XCTAssertTrue(
                resourceURLs.allSatisfy { url in
                    (try? url.resourceValues(forKeys: [.isRegularFileKey]).isRegularFile) == true
                },
                "Bundled pet runtime must expose only flat package files."
            )
        }
    }

    func testBundledPetManifestsUseStableNonLocalizedNames() throws {
        for species in PetSpecies.allCases {
            let manifestURL = try XCTUnwrap(
                Bundle.module.url(
                    forResource: "pet",
                    withExtension: "json",
                    subdirectory: "Pets/\(species.assetFolder)"
                )
            )
            let manifest = try JSONDecoder().decode(
                CodexPetManifest.self,
                from: Data(contentsOf: manifestURL)
            )

            XCTAssertEqual(manifest.displayName, species.assetFolder)
        }
    }
}

final class CodexPetActivityAnimationMapperTests: XCTestCase {
    func testMapsAIActivityPhasesToPetAnimationStates() {
        let finishedAt = Date()

        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .loading,
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .running
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .running(tool: "codex"),
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .running
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .waitingInput(tool: "codex"),
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .review
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .completed(tool: "codex", finishedAt: finishedAt, exitCode: 0),
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .waving
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .completed(tool: "codex", finishedAt: finishedAt, exitCode: nil),
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .waving
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .completed(tool: "codex", finishedAt: finishedAt, exitCode: 1),
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .failed
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .idle,
                sleeping: false,
                hasAnyRunningActivity: true
            ),
            .running
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .idle,
                sleeping: false,
                hasAnyRunningActivity: false
            ),
            .idle
        )
        XCTAssertEqual(
            CodexPetActivityAnimationMapper.animationState(
                for: .running(tool: "codex"),
                sleeping: true,
                hasAnyRunningActivity: true
            ),
            .waiting
        )
    }
}

final class CodexPetPackageServiceTests: XCTestCase {
    func testLoadsValidPackageOnly() throws {
        let root = FileManager.default.temporaryDirectory
            .appendingPathComponent("codex-pet-package-\(UUID().uuidString)", isDirectory: true)
        let valid = root.appendingPathComponent("demo", isDirectory: true)
        let invalid = root.appendingPathComponent("broken", isDirectory: true)
        try FileManager.default.createDirectory(at: valid, withIntermediateDirectories: true)
        try FileManager.default.createDirectory(at: invalid, withIntermediateDirectories: true)
        defer { try? FileManager.default.removeItem(at: root) }

        try Data("{}".utf8).write(to: valid.appendingPathComponent("spritesheet.webp"))
        try Data(
            #"{"id":"demo","displayName":"Demo","description":"One line.","spritesheetPath":"spritesheet.webp"}"#.utf8
        ).write(to: valid.appendingPathComponent("pet.json"))
        try Data(
            #"{"id":"broken","displayName":"Broken","description":"Missing image.","spritesheetPath":"spritesheet.webp"}"#.utf8
        ).write(to: invalid.appendingPathComponent("pet.json"))

        let packages = CodexPetPackageService().packages(rootURL: root)

        XCTAssertEqual(packages.map(\.manifest.id), ["demo"])
        XCTAssertEqual(packages.first?.spritesheetURL.lastPathComponent, "spritesheet.webp")
    }
}
