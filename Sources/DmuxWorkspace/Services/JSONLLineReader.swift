import Foundation

enum JSONLLineReader {
    static func forEachLine(
        in fileURL: URL,
        startingAt offset: UInt64 = 0,
        dropPartialFirstLine: Bool = false,
        chunkSize: Int = 65_536,
        _ body: (Data) -> Bool
    ) {
        forEachLine(
            in: fileURL,
            startingAt: offset,
            dropPartialFirstLine: dropPartialFirstLine,
            chunkSize: chunkSize
        ) { line, _ in
            body(line)
        }
    }

    static func forEachLine(
        in fileURL: URL,
        startingAt offset: UInt64 = 0,
        dropPartialFirstLine: Bool = false,
        chunkSize: Int = 65_536,
        _ body: (Data, UInt64) -> Bool
    ) {
        guard let handle = try? FileHandle(forReadingFrom: fileURL) else {
            return
        }
        defer {
            try? handle.close()
        }

        do {
            try handle.seek(toOffset: offset)
        } catch {
            return
        }

        var remainder = Data()
        var shouldDropPartialFirstLine = dropPartialFirstLine
        var currentOffset = offset

        while true {
            let chunk = handle.readData(ofLength: chunkSize)
            if chunk.isEmpty {
                break
            }

            remainder.append(chunk)

            if shouldDropPartialFirstLine {
                guard let newlineIndex = remainder.firstIndex(of: UInt8(ascii: "\n")) else {
                    continue
                }
                let droppedCount = remainder.distance(from: remainder.startIndex, to: remainder.index(after: newlineIndex))
                currentOffset += UInt64(droppedCount)
                remainder.removeSubrange(...newlineIndex)
                shouldDropPartialFirstLine = false
            }

            while let newlineIndex = remainder.firstIndex(of: UInt8(ascii: "\n")) {
                let line = Data(remainder[..<newlineIndex])
                let consumedCount = remainder.distance(from: remainder.startIndex, to: remainder.index(after: newlineIndex))
                currentOffset += UInt64(consumedCount)
                remainder.removeSubrange(...newlineIndex)
                if line.isEmpty {
                    continue
                }
                if body(line, currentOffset) == false {
                    return
                }
            }
        }

        guard shouldDropPartialFirstLine == false,
              remainder.isEmpty == false else {
            return
        }
        _ = body(remainder, currentOffset + UInt64(remainder.count))
    }

    static func tailLines(in fileURL: URL, maxBytes: Int = 262_144) -> [Data] {
        let fileSize = currentFileSize(for: fileURL)
        let offset = fileSize > UInt64(maxBytes) ? fileSize - UInt64(maxBytes) : 0
        var lines: [Data] = []
        forEachLine(
            in: fileURL,
            startingAt: offset,
            dropPartialFirstLine: offset > 0
        ) { line in
            lines.append(line)
            return true
        }
        return lines
    }

    static func currentFileSize(for fileURL: URL) -> UInt64 {
        let size = (try? fileURL.resourceValues(forKeys: [.fileSizeKey]))?.fileSize ?? 0
        return UInt64(max(0, size))
    }
}
