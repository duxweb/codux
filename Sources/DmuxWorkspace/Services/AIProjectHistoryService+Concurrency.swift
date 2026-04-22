import Foundation

private struct IndexedTaskResult<Value: Sendable>: Sendable {
    let index: Int
    let value: Value
}

extension AIProjectHistoryService {
    func limitedConcurrentMap<Input: Sendable, Output: Sendable>(
        _ items: [Input],
        maxConcurrency: Int,
        priority: TaskPriority,
        operation: @Sendable @escaping (Int, Input) async -> Output
    ) async -> [Output] {
        guard items.isEmpty == false else {
            return []
        }

        let concurrency = min(max(1, maxConcurrency), items.count)
        var nextIndex = 0
        var orderedResults = Array<Output?>(repeating: nil, count: items.count)

        return await withTaskGroup(of: IndexedTaskResult<Output>.self) { group in
            func enqueueNext() {
                guard nextIndex < items.count else {
                    return
                }
                let index = nextIndex
                let item = items[index]
                nextIndex += 1
                group.addTask(priority: priority) {
                    let value = await operation(index, item)
                    return IndexedTaskResult(index: index, value: value)
                }
            }

            for _ in 0..<concurrency {
                enqueueNext()
            }

            while let result = await group.next() {
                orderedResults[result.index] = result.value
                enqueueNext()
            }

            return orderedResults.compactMap { $0 }
        }
    }
}
