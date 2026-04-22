import Foundation

struct AIProjectHistoryIndexingProfile: Sendable {
    let sourceConcurrency: Int
    let fileConcurrency: Int
    let interFileDelayMilliseconds: UInt64
    let taskPriority: TaskPriority

    init(
        sourceConcurrency: Int,
        fileConcurrency: Int,
        interFileDelayMilliseconds: UInt64 = 0,
        taskPriority: TaskPriority
    ) {
        self.sourceConcurrency = max(1, sourceConcurrency)
        self.fileConcurrency = max(1, fileConcurrency)
        self.interFileDelayMilliseconds = interFileDelayMilliseconds
        self.taskPriority = taskPriority
    }

    static let foreground = AIProjectHistoryIndexingProfile(
        sourceConcurrency: 4,
        fileConcurrency: 2,
        taskPriority: .utility
    )

    static let background = AIProjectHistoryIndexingProfile(
        sourceConcurrency: 1,
        fileConcurrency: 1,
        interFileDelayMilliseconds: 60,
        taskPriority: .background
    )
}
