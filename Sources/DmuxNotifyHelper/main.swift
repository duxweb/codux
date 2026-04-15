import Foundation

enum NotifyHelperError: LocalizedError {
    case missingValue(String)

    var errorDescription: String? {
        switch self {
        case .missingValue(let flag):
            return "Missing value for \(flag)"
        }
    }
}

struct NotifyPayload {
    var title: String = ""
    var message: String = ""
    var subtitle: String?

    init(arguments: [String]) throws {
        var iterator = arguments.makeIterator()
        while let argument = iterator.next() {
            switch argument {
            case "--title":
                guard let value = iterator.next() else { throw NotifyHelperError.missingValue(argument) }
                title = value
            case "--message":
                guard let value = iterator.next() else { throw NotifyHelperError.missingValue(argument) }
                message = value
            case "--subtitle":
                guard let value = iterator.next() else { throw NotifyHelperError.missingValue(argument) }
                subtitle = value
            default:
                continue
            }
        }
    }
}

@main
struct DmuxNotifyHelper {
    static func main() {
        do {
            let payload = try NotifyPayload(arguments: Array(CommandLine.arguments.dropFirst()))
            guard !payload.title.isEmpty, !payload.message.isEmpty else {
                fputs("usage: dmux-notify-helper --title <title> --message <message> [--subtitle <subtitle>]\n", stderr)
                Foundation.exit(64)
            }

            let process = Process()
            process.executableURL = URL(fileURLWithPath: "/usr/bin/osascript")
            process.arguments = appleScriptArguments(for: payload)
            try process.run()
            process.waitUntilExit()
            Foundation.exit(process.terminationStatus)
        } catch {
            fputs("dmux-notify-helper error: \(error.localizedDescription)\n", stderr)
            Foundation.exit(1)
        }
    }

    private static func appleScriptArguments(for payload: NotifyPayload) -> [String] {
        let script = """
        on run argv
            set notificationMessage to item 1 of argv
            set notificationTitle to item 2 of argv
            if (count of argv) > 2 then
                set notificationSubtitle to item 3 of argv
                display notification notificationMessage with title notificationTitle subtitle notificationSubtitle
            else
                display notification notificationMessage with title notificationTitle
            end if
        end run
        """

        var arguments = ["-e", script, payload.message, payload.title]
        if let subtitle = payload.subtitle, !subtitle.isEmpty {
            arguments.append(subtitle)
        }
        return arguments
    }
}
