import Foundation

struct CodexPetManifest: Codable, Equatable, Sendable {
    var id: String
    var displayName: String
    var description: String
    var spritesheetPath: String
}

enum CodexPetAnimationState: String, CaseIterable, Codable, Equatable, Sendable {
    case idle
    case runningRight = "running-right"
    case runningLeft = "running-left"
    case waving
    case jumping
    case failed
    case waiting
    case running
    case review
}

struct CodexPetAnimation: Equatable, Sendable {
    var state: CodexPetAnimationState
    var row: Int
    var frameDurationsMS: [Int]

    var frameCount: Int { frameDurationsMS.count }
}

enum CodexPetPlaybackPolicy {
    static let baseFrameDuration: TimeInterval = 1.875

    static func frameDuration(
        for animation: CodexPetAnimation,
        activeFrameCount: Int,
        frame: Int
    ) -> TimeInterval {
        let durations = frameDurations(for: animation, activeFrameCount: activeFrameCount)
        return durations[min(max(0, frame), durations.count - 1)]
    }

    static func frameDurations(
        for animation: CodexPetAnimation,
        activeFrameCount: Int
    ) -> [TimeInterval] {
        let frameCount = max(1, activeFrameCount)
        let sourceDurations = normalizedSourceDurations(for: animation, frameCount: frameCount)
            .enumerated()
            .map { index, duration in
                index == 0 ? duration * leadingFrameHoldMultiplier(for: animation.state) : duration
            }
        let sourceTotal = sourceDurations.reduce(0, +)
        guard sourceTotal > 0 else {
            return Array(repeating: baseFrameDuration, count: frameCount)
        }

        let targetTotal = baseFrameDuration
            * TimeInterval(max(1, max(animation.frameCount, activeFrameCount)))
            * cycleDurationMultiplier(for: animation.state)
        let scale = targetTotal / sourceTotal
        return sourceDurations.map { max(0.08, $0 * scale) }
    }

    private static func normalizedSourceDurations(
        for animation: CodexPetAnimation,
        frameCount: Int
    ) -> [TimeInterval] {
        let explicitDurations = animation.frameDurationsMS
            .prefix(frameCount)
            .map { TimeInterval(max(1, $0)) / 1_000 }

        guard explicitDurations.count < frameCount else {
            return explicitDurations
        }

        let fallback = explicitDurations.last ?? baseFrameDuration
        return explicitDurations + Array(repeating: fallback, count: frameCount - explicitDurations.count)
    }

    private static func leadingFrameHoldMultiplier(for state: CodexPetAnimationState) -> TimeInterval {
        switch state {
        case .idle, .waiting, .review:
            return 1.85
        case .runningRight, .runningLeft, .waving, .jumping, .failed, .running:
            return 1.35
        }
    }

    static func cycleDurationMultiplier(for state: CodexPetAnimationState) -> TimeInterval {
        switch state {
        case .waiting:
            return 1.45
        case .idle, .review:
            return 1.0
        case .runningRight, .runningLeft, .waving, .jumping, .failed, .running:
            return 1.0
        }
    }
}

enum CodexPetAtlasSpec {
    static let columns = 8
    static let rows = 9
    static let cellWidth = 192
    static let cellHeight = 208
    static let atlasWidth = columns * cellWidth
    static let atlasHeight = rows * cellHeight

    static func animation(for state: CodexPetAnimationState) -> CodexPetAnimation {
        switch state {
        case .idle:
            return CodexPetAnimation(state: state, row: 0, frameDurationsMS: [280, 110, 110, 140, 140, 320])
        case .runningRight:
            return CodexPetAnimation(state: state, row: 1, frameDurationsMS: [120, 120, 120, 120, 120, 120, 120, 220])
        case .runningLeft:
            return CodexPetAnimation(state: state, row: 2, frameDurationsMS: [120, 120, 120, 120, 120, 120, 120, 220])
        case .waving:
            return CodexPetAnimation(state: state, row: 3, frameDurationsMS: [140, 140, 140, 280])
        case .jumping:
            return CodexPetAnimation(state: state, row: 4, frameDurationsMS: [140, 140, 140, 140, 280])
        case .failed:
            return CodexPetAnimation(state: state, row: 5, frameDurationsMS: [140, 140, 140, 140, 140, 140, 140, 240])
        case .waiting:
            return CodexPetAnimation(state: state, row: 6, frameDurationsMS: [150, 150, 150, 150, 150, 260])
        case .running:
            return CodexPetAnimation(state: state, row: 7, frameDurationsMS: [120, 120, 120, 120, 120, 220])
        case .review:
            return CodexPetAnimation(state: state, row: 8, frameDurationsMS: [150, 150, 150, 150, 150, 280])
        }
    }
}

enum CodexPetActivityAnimationMapper {
    static func animationState(
        for phase: ProjectActivityPhase,
        sleeping: Bool,
        hasAnyRunningActivity: Bool
    ) -> CodexPetAnimationState {
        if sleeping {
            return .waiting
        }

        switch phase {
        case .loading, .running:
            return .running
        case .waitingInput:
            return .review
        case .completed(_, _, let exitCode):
            return exitCode == 0 || exitCode == nil ? .waving : .failed
        case .idle:
            return hasAnyRunningActivity ? .running : .idle
        }
    }
}
