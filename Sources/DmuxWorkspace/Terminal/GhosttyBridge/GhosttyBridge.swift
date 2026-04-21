import AppKit
import Darwin
import Foundation
import GhosttyTerminal
import GhosttyTheme
import QuartzCore
import SwiftUI

enum GhosttyEmbeddedConfig {
    private struct ParsedThemeOverrides {
        var backgroundColor: NSColor?
        var foregroundColor: NSColor?
        var cursorColor: NSColor?
        var cursorTextColor: NSColor?
        var selectionBackgroundColor: NSColor?
        var selectionForegroundColor: NSColor?
        var palette: [Int: String] = [:]
    }

    struct ResolvedControllerConfig {
        let configSource: TerminalController.ConfigSource
        let userConfigPaths: [String]

        var prefersUserConfig: Bool {
            !userConfigPaths.isEmpty
        }

        var userConfigDescription: String? {
            guard !userConfigPaths.isEmpty else {
                return nil
            }
            return userConfigPaths.joined(separator: ", ")
        }
    }

    static let candidateRelativePaths = [
        ".config/ghostty/config.ghostty",
        ".config/ghostty/config",
        "Library/Application Support/com.mitchellh.ghostty/config.ghostty",
        "Library/Application Support/com.mitchellh.ghostty/config",
    ]

    static let candidateThemeDirectoryRelativePaths = [
        ".config/ghostty/themes",
        "Library/Application Support/com.mitchellh.ghostty/themes",
    ]

    static let fallbackEditingKeybinds = [
        "cmd+left=text:\\x01",
        "cmd+right=text:\\x05",
        "option+left=text:\\x1bb",
        "option+right=text:\\x1bf",
        "cmd+backspace=text:\\x15",
        "option+backspace=text:\\x17",
    ]

    static func resolvedUserConfigFileURLs(
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> [URL] {
        var urls: [URL] = []
        var seenPaths = Set<String>()
        for relativePath in candidateRelativePaths {
            let url = homeDirectoryURL.appendingPathComponent(relativePath, isDirectory: false)
            var isDirectory = ObjCBool(false)
            guard fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory),
                  isDirectory.boolValue == false,
                  seenPaths.insert(url.path).inserted else {
                continue
            }
            urls.append(url)
        }
        return urls
    }

    static func resolvedControllerConfig(
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> ResolvedControllerConfig {
        let urls = resolvedUserConfigFileURLs(
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
        guard !urls.isEmpty else {
            return ResolvedControllerConfig(
                configSource: .generated(fallbackEditingConfigContents()),
                userConfigPaths: []
            )
        }

        let mergedContents = mergedUserConfigContents(
            urls.map { url in
                (url, (try? String(contentsOf: url, encoding: .utf8)) ?? "")
            }
        )
        return ResolvedControllerConfig(
            configSource: .generated(mergedContents),
            userConfigPaths: urls.map(\.path)
        )
    }

    static func fallbackEditingConfigContents() -> String {
        fallbackEditingKeybinds
            .map { "keybind = \($0)" }
            .joined(separator: "\n") + "\n"
    }

    static func mergedUserConfigContents(_ userConfigEntries: [(URL, String)]) -> String {
        var sections = [fallbackEditingConfigContents().trimmingCharacters(in: .whitespacesAndNewlines)]

        for (url, contents) in userConfigEntries {
            let normalizedContents = sanitizedEmbeddedUserConfigContents(contents)
                .trimmingCharacters(in: .whitespacesAndNewlines)
            guard !normalizedContents.isEmpty else {
                continue
            }
            sections.append("# Source: \(url.path)\n\(normalizedContents)")
        }

        return sections.joined(separator: "\n\n") + "\n"
    }

    private static func sanitizedEmbeddedUserConfigContents(_ contents: String) -> String {
        contents
            .components(separatedBy: .newlines)
            .filter { rawLine in
                let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
                guard line.isEmpty == false, line.hasPrefix("#") == false else {
                    return true
                }

                guard let separatorIndex = line.firstIndex(of: "=") else {
                    return true
                }

                let key = line[..<separatorIndex]
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                    .lowercased()

                // Embedded Ghostty uses a different host/composition path than the
                // standalone app. Cursor thickness amplification is visually unstable
                // here, so keep the cursor style/opacity but ignore explicit thickness.
                return key != "adjust-cursor-thickness"
            }
            .joined(separator: "\n")
    }

    static func resolvedAutomaticTerminalAppearance(
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser,
        prefersDarkAppearance: Bool = true
    ) -> AppEffectiveTerminalAppearance {
        let fallbackBase = AppTerminalBackgroundPreset
            .automaticFallbackPreset(prefersDarkAppearance: prefersDarkAppearance)
            .effectiveAppearance(backgroundColorPreset: .automatic)
        let fallback = AppEffectiveTerminalAppearance(
            backgroundColor: AppBackgroundColorPreset.base950.swatchColor ?? fallbackBase.backgroundColor,
            foregroundColor: fallbackBase.foregroundColor,
            cursorColor: fallbackBase.cursorColor,
            cursorTextColor: fallbackBase.cursorTextColor,
            selectionBackgroundColor: fallbackBase.selectionBackgroundColor,
            selectionForegroundColor: fallbackBase.selectionForegroundColor,
            paletteHexStrings: fallbackBase.paletteHexStrings,
            isLight: false,
            minimumContrast: fallbackBase.minimumContrast
        )

        let urls = resolvedUserConfigFileURLs(
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
        guard !urls.isEmpty else {
            return fallback
        }

        let entries = urls.map { url in
            (url, (try? String(contentsOf: url, encoding: .utf8)) ?? "")
        }
        return automaticTerminalAppearance(
            from: entries,
            fallback: fallback,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
    }

    static func automaticTerminalAppearance(
        from userConfigEntries: [(URL, String)],
        fallback: AppEffectiveTerminalAppearance,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> AppEffectiveTerminalAppearance {
        var appearance = fallback

        for (_, contents) in userConfigEntries {
            for rawLine in contents.components(separatedBy: .newlines) {
                let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
                guard line.isEmpty == false, line.hasPrefix("#") == false,
                      let separatorIndex = line.firstIndex(of: "=") else {
                    continue
                }

                let key = line[..<separatorIndex]
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                    .lowercased()
                let rawValue = line[line.index(after: separatorIndex)...]
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                guard rawValue.isEmpty == false else {
                    continue
                }

                if key == "theme",
                   let themeName = Self.unquoted(rawValue) as String? {
                    if let themeAppearance = Self.themeAppearance(
                        named: themeName,
                        fileManager: fileManager,
                        homeDirectoryURL: homeDirectoryURL
                    ) {
                        appearance = themeAppearance
                        continue
                    }
                    if let themePreset = AppTerminalBackgroundPreset.automaticMatch(
                        forGhosttyThemeName: themeName
                    ) {
                        appearance = themePreset.effectiveAppearance(backgroundColorPreset: .automatic)
                        continue
                    }
                }

                if key == "palette",
                   let paletteIndex = Self.parsePaletteIndex(from: rawValue),
                   let paletteColor = Self.parseHexColorString(from: rawValue) {
                    var palette = appearance.paletteHexStrings
                    while palette.count <= paletteIndex {
                        palette.append(appearance.backgroundColor.ghosttyHexString)
                    }
                    palette[paletteIndex] = paletteColor
                    appearance = AppEffectiveTerminalAppearance(
                        backgroundColor: appearance.backgroundColor,
                        foregroundColor: appearance.foregroundColor,
                        cursorColor: appearance.cursorColor,
                        cursorTextColor: appearance.cursorTextColor,
                        selectionBackgroundColor: appearance.selectionBackgroundColor,
                        selectionForegroundColor: appearance.selectionForegroundColor,
                        paletteHexStrings: palette,
                        isLight: appearance.isLight,
                        minimumContrast: appearance.minimumContrast
                    )
                    continue
                }

                var overrides = ParsedThemeOverrides()

                switch key {
                case "background":
                    overrides.backgroundColor = Self.parseColor(from: rawValue)
                case "foreground":
                    overrides.foregroundColor = Self.parseColor(from: rawValue)
                case "cursor-color":
                    overrides.cursorColor = Self.parseColor(from: rawValue)
                case "cursor-text":
                    overrides.cursorTextColor = Self.parseColor(from: rawValue)
                case "selection-background":
                    overrides.selectionBackgroundColor = Self.parseColor(from: rawValue)
                case "selection-foreground":
                    overrides.selectionForegroundColor = Self.parseColor(from: rawValue)
                default:
                    continue
                }

                appearance = apply(overrides: overrides, to: appearance)
            }
        }

        return appearance
    }

    private static func apply(
        overrides: ParsedThemeOverrides,
        to appearance: AppEffectiveTerminalAppearance
    ) -> AppEffectiveTerminalAppearance {
        let backgroundColor = overrides.backgroundColor ?? appearance.backgroundColor
        let isLight = backgroundColor.dmuxPerceivedBrightness >= 0.72
        return AppEffectiveTerminalAppearance(
            backgroundColor: backgroundColor,
            foregroundColor: overrides.foregroundColor ?? appearance.foregroundColor,
            cursorColor: overrides.cursorColor ?? appearance.cursorColor,
            cursorTextColor: overrides.cursorTextColor ?? appearance.cursorTextColor,
            selectionBackgroundColor: overrides.selectionBackgroundColor ?? appearance.selectionBackgroundColor,
            selectionForegroundColor: overrides.selectionForegroundColor ?? appearance.selectionForegroundColor,
            paletteHexStrings: overrides.palette.isEmpty ? appearance.paletteHexStrings : mergedPalette(overrides.palette, into: appearance.paletteHexStrings),
            isLight: isLight,
            minimumContrast: isLight ? 1.05 : 1.0
        )
    }

    private static func themeAppearance(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> AppEffectiveTerminalAppearance? {
        let definition = resolvedThemeDefinition(
            named: name,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        )
        return definition.map { appearance(from: $0) }
    }

    private static func resolvedThemeDefinition(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> GhosttyThemeDefinition? {
        if let userTheme = resolvedUserThemeDefinition(
            named: name,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        ) {
            return userTheme
        }

        if let exact = GhosttyThemeCatalog.theme(named: name) {
            return exact
        }

        let normalizedQuery = normalizeThemeName(name)
        guard normalizedQuery.isEmpty == false else {
            return nil
        }

        return GhosttyThemeCatalog
            .search(name)
            .first(where: { normalizeThemeName($0.name) == normalizedQuery })
    }

    private static func resolvedUserThemeDefinition(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> GhosttyThemeDefinition? {
        guard let url = resolvedUserThemeFileURL(
            named: name,
            fileManager: fileManager,
            homeDirectoryURL: homeDirectoryURL
        ),
        let contents = try? String(contentsOf: url, encoding: .utf8) else {
            return nil
        }

        return parseUserThemeDefinition(
            name: url.lastPathComponent,
            contents: contents
        )
    }

    private static func resolvedUserThemeFileURL(
        named name: String,
        fileManager: FileManager = .default,
        homeDirectoryURL: URL = FileManager.default.homeDirectoryForCurrentUser
    ) -> URL? {
        let candidates = [name, name.lowercased(), name.capitalized]
        for relativePath in candidateThemeDirectoryRelativePaths {
            let directory = homeDirectoryURL.appendingPathComponent(relativePath, isDirectory: true)
            for candidate in candidates {
                let url = directory.appendingPathComponent(candidate, isDirectory: false)
                var isDirectory = ObjCBool(false)
                if fileManager.fileExists(atPath: url.path, isDirectory: &isDirectory),
                   isDirectory.boolValue == false {
                    return url
                }
            }
        }
        return nil
    }

    private static func parseUserThemeDefinition(
        name: String,
        contents: String
    ) -> GhosttyThemeDefinition? {
        var background: String?
        var foreground: String?
        var cursorColor: String?
        var cursorText: String?
        var selectionBackground: String?
        var selectionForeground: String?
        var palette: [Int: String] = [:]

        for rawLine in contents.components(separatedBy: .newlines) {
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            guard line.isEmpty == false,
                  line.hasPrefix("#") == false,
                  let separatorIndex = line.firstIndex(of: "=") else {
                continue
            }

            let key = line[..<separatorIndex]
                .trimmingCharacters(in: .whitespacesAndNewlines)
                .lowercased()
            let rawValue = line[line.index(after: separatorIndex)...]
                .trimmingCharacters(in: .whitespacesAndNewlines)

            switch key {
            case "background":
                background = parseHexColorString(from: rawValue)
            case "foreground":
                foreground = parseHexColorString(from: rawValue)
            case "cursor-color":
                cursorColor = parseHexColorString(from: rawValue)
            case "cursor-text":
                cursorText = parseHexColorString(from: rawValue)
            case "selection-background":
                selectionBackground = parseHexColorString(from: rawValue)
            case "selection-foreground":
                selectionForeground = parseHexColorString(from: rawValue)
            case "palette":
                if let index = parsePaletteIndex(from: rawValue),
                   let color = parseHexColorString(from: rawValue) {
                    palette[index] = String(color.dropFirst())
                }
            default:
                continue
            }
        }

        guard let background, let foreground else {
            return nil
        }

        return GhosttyThemeDefinition(
            name: name,
            background: background,
            foreground: foreground,
            cursorColor: cursorColor,
            cursorText: cursorText,
            selectionBackground: selectionBackground,
            selectionForeground: selectionForeground,
            palette: palette
        )
    }

    private static func appearance(from definition: GhosttyThemeDefinition) -> AppEffectiveTerminalAppearance {
        let backgroundColor = parseColor(from: definition.background) ?? .black
        let foregroundColor = parseColor(from: definition.foreground)
            ?? (backgroundColor.dmuxPerceivedBrightness >= 0.72 ? .black : .white)
        let selectionBackgroundColor = parseColor(from: definition.selectionBackground)
            ?? (backgroundColor.blended(withFraction: 0.2, of: foregroundColor) ?? backgroundColor)
        let selectionForegroundColor = parseColor(from: definition.selectionForeground)
            ?? foregroundColor
        let cursorColor = parseColor(from: definition.cursorColor) ?? foregroundColor
        let cursorTextColor = parseColor(from: definition.cursorText) ?? backgroundColor
        let isLight = backgroundColor.dmuxPerceivedBrightness >= 0.72
        let paletteHexStrings = (0..<16).map { index in
            definition.palette[index]?.uppercased() ?? backgroundColor.ghosttyHexString
        }

        return AppEffectiveTerminalAppearance(
            backgroundColor: backgroundColor,
            foregroundColor: foregroundColor,
            cursorColor: cursorColor,
            cursorTextColor: cursorTextColor,
            selectionBackgroundColor: selectionBackgroundColor,
            selectionForegroundColor: selectionForegroundColor,
            paletteHexStrings: paletteHexStrings,
            isLight: isLight,
            minimumContrast: isLight ? 1.05 : 1.0
        )
    }

    private static func mergedPalette(
        _ overrides: [Int: String],
        into palette: [String]
    ) -> [String] {
        var palette = palette
        for (index, color) in overrides {
            while palette.count <= index {
                palette.append("#000000")
            }
            palette[index] = color
        }
        return palette
    }

    private static func parsePaletteIndex(from rawValue: String) -> Int? {
        guard let match = rawValue.range(
            of: #"^\s*(\d+)\s*="#,
            options: .regularExpression
        ) else {
            return nil
        }
        let prefix = rawValue[match]
        let digits = prefix.replacingOccurrences(of: #"[^0-9]"#, with: "", options: .regularExpression)
        return Int(digits)
    }

    private static func parseColor(from rawValue: String) -> NSColor? {
        guard let hex = parseHexColorString(from: rawValue),
              let rgb = UInt(hex.dropFirst(), radix: 16) else {
            return nil
        }
        return NSColor(
            calibratedRed: CGFloat((rgb >> 16) & 0xFF) / 255,
            green: CGFloat((rgb >> 8) & 0xFF) / 255,
            blue: CGFloat(rgb & 0xFF) / 255,
            alpha: 1
        )
    }

    private static func parseColor(from rawValue: String?) -> NSColor? {
        guard let rawValue else {
            return nil
        }
        return parseColor(from: rawValue)
    }

    private static func parseHexColorString(from rawValue: String) -> String? {
        guard let match = rawValue.range(
            of: #"#([0-9a-fA-F]{6}|[0-9a-fA-F]{8})"#,
            options: .regularExpression
        ) else {
            return nil
        }
        let value = String(rawValue[match])
        if value.count == 9 {
            return "#\(value.dropFirst().prefix(6))"
        }
        return value.uppercased()
    }

    private static func unquoted<S: StringProtocol>(_ value: S) -> String {
        let string = String(value).trimmingCharacters(in: .whitespacesAndNewlines)
        guard string.count >= 2 else {
            return string
        }
        if (string.hasPrefix("\"") && string.hasSuffix("\"")) || (string.hasPrefix("'") && string.hasSuffix("'")) {
            return String(string.dropFirst().dropLast())
        }
        return string
    }

    private static func normalizeThemeName(_ value: String) -> String {
        value
            .lowercased()
            .replacingOccurrences(
                of: #"[^a-z0-9]+"#,
                with: "",
                options: .regularExpression
            )
    }

    static func terminalConfiguration() -> TerminalConfiguration {
        TerminalConfiguration { builder in
            builder.withBackgroundOpacity(1.0)
            builder.withBackgroundBlur(0)
            builder.withWindowPaddingX(0)
            builder.withWindowPaddingY(0)
        }
    }
}

private enum GhosttyWaitStatus {
    static func didExit(_ status: Int32) -> Bool {
        (status & 0x7f) == 0
    }

    static func exitStatus(_ status: Int32) -> Int32 {
        (status >> 8) & 0xff
    }

    static func didSignal(_ status: Int32) -> Bool {
        let code = status & 0x7f
        return code != 0 && code != 0x7f
    }

    static func termSignal(_ status: Int32) -> Int32 {
        status & 0x7f
    }
}

private extension NSColor {
    var ghosttyHexString: String {
        let converted = usingColorSpace(.deviceRGB) ?? self
        let red = Int(round(converted.redComponent * 255.0))
        let green = Int(round(converted.greenComponent * 255.0))
        let blue = Int(round(converted.blueComponent * 255.0))
        return String(format: "#%02X%02X%02X", red, green, blue)
    }

    var dmuxPerceivedBrightness: CGFloat {
        let resolved = usingColorSpace(.deviceRGB) ?? self
        return (resolved.redComponent * 0.299) + (resolved.greenComponent * 0.587) + (resolved.blueComponent * 0.114)
    }
}

struct GhosttyResizeTransitionState {
    private(set) var isSuspended = false
    private var pendingViewport: InMemoryTerminalViewport?

    mutating func begin() {
        isSuspended = true
    }

    mutating func capture(_ viewport: InMemoryTerminalViewport) -> InMemoryTerminalViewport? {
        guard isSuspended else {
            return viewport
        }
        pendingViewport = viewport
        return nil
    }

    mutating func end() -> InMemoryTerminalViewport? {
        isSuspended = false
        defer { pendingViewport = nil }
        return pendingViewport
    }
}

private final class GhosttyPTYProcessBridge: @unchecked Sendable {
    let sessionID: UUID
    let processInstanceID = UUID().uuidString.lowercased()
    lazy var terminalSession = InMemoryTerminalSession(
        write: { [weak self] data in
            self?.writeToProcess(data)
        },
        resize: { [weak self] viewport in
            self?.resizeProcess(viewport)
        }
    )

    var onFirstOutput: (() -> Void)?
    var onProcessTerminated: ((Int32?) -> Void)?

    private let logger = AppDebugLog.shared
    private let lock = NSLock()
    private let ioQueue = DispatchQueue(label: "codux.ghostty.pty", qos: .userInitiated)
    private var masterFD: Int32 = -1
    private var closeMasterFDOnCancel = true
    private var shellPID: Int32 = 0
    private var readSource: DispatchSourceRead?
    private var processSource: DispatchSourceProcess?
    private var launchDate = Date()
    private var hasObservedOutput = false
    private var lastViewport = InMemoryTerminalViewport(columns: 80, rows: 24)
    private var resizeTransitionState = GhosttyResizeTransitionState()

    init(sessionID: UUID) {
        self.sessionID = sessionID
    }

    deinit {
        terminateProcessTree()
    }

    func start(
        shell: String,
        shellName: String,
        command: String,
        cwd: String,
        environment: [(String, String)]
    ) {
        guard currentShellPID == nil else {
            return
        }

        var winsizeValue = winsize(
            ws_row: lastViewport.rows,
            ws_col: lastViewport.columns,
            ws_xpixel: UInt16(min(lastViewport.widthPixels, UInt32(UInt16.max))),
            ws_ypixel: UInt16(min(lastViewport.heightPixels, UInt32(UInt16.max)))
        )
        var master: Int32 = -1
        let launch = shellLaunchConfiguration(shellName: shellName, command: command)
        let pid = forkpty(&master, nil, nil, &winsizeValue)

        if pid < 0 {
            logger.log(
                "ghostty-process",
                "start-failed session=\(sessionID.uuidString) reason=forkpty errno=\(errno)"
            )
            onProcessTerminated?(nil)
            return
        }

        if pid == 0 {
            _ = chdir(cwd)
            var env = Dictionary(uniqueKeysWithValues: environment)
            env["DMUX_SESSION_INSTANCE_ID"] = processInstanceID
            if env["TERM"] == nil {
                env["TERM"] = "xterm-256color"
            }

            let execArguments = [launch.execName] + launch.args
            _ = execArguments.withCStringArray { argv in
                env
                    .map { "\($0.key)=\($0.value)" }
                    .withCStringArray { envp in
                        execve(shell, argv, envp)
                    }
            }

            let message = "Codux Ghostty exec failed: \(String(cString: strerror(errno)))\n"
            message.withCString { ptr in
                _ = write(STDERR_FILENO, ptr, strlen(ptr))
            }
            _exit(127)
        }

        configureParentAfterFork(masterFD: master, childPID: pid)
    }

    func resetOutputObservation() {
        lock.lock()
        hasObservedOutput = false
        lock.unlock()
    }

    func beginStructuralResizeTransition() {
        lock.lock()
        resizeTransitionState.begin()
        lock.unlock()
    }

    func endStructuralResizeTransition() {
        let viewport: InMemoryTerminalViewport?
        lock.lock()
        viewport = resizeTransitionState.end()
        lock.unlock()

        guard let viewport else {
            return
        }
        resizeProcess(viewport)
    }

    var currentShellPID: Int32? {
        lock.lock()
        defer { lock.unlock() }
        return shellPID > 0 ? shellPID : nil
    }

    func terminateProcessTree() {
        let pid: Int32
        let fd: Int32
        let shouldCloseOnCancel: Bool
        let readSource: DispatchSourceRead?
        let processSource: DispatchSourceProcess?

        lock.lock()
        pid = shellPID
        fd = masterFD
        shouldCloseOnCancel = closeMasterFDOnCancel
        readSource = self.readSource
        processSource = self.processSource
        shellPID = 0
        masterFD = -1
        closeMasterFDOnCancel = false
        self.readSource = nil
        self.processSource = nil
        lock.unlock()

        readSource?.cancel()
        processSource?.cancel()
        if fd >= 0, shouldCloseOnCancel {
            close(fd)
        }

        guard pid > 0 else {
            return
        }

        kill(-pid, SIGTERM)
        kill(pid, SIGTERM)

        ioQueue.asyncAfter(deadline: .now() + 1.0) {
            guard kill(pid, 0) == 0 else {
                return
            }
            kill(-pid, SIGKILL)
            kill(pid, SIGKILL)
        }
    }

    func sendText(_ text: String) {
        guard let data = text.data(using: .utf8), !data.isEmpty else {
            return
        }
        writeToProcess(data)
    }

    func sendInterrupt() {
        writeToProcess(Data([0x03]))
    }

    func sendEscape() {
        writeToProcess(Data([0x1b]))
    }

    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut) {
        writeToProcess(Data(shortcut.bytes))
    }

    func sendNativeCommandArrow(keyCode: UInt16) -> Bool {
        guard let shortcut = TerminalEditingShortcut.match(
            keyCode: keyCode,
            modifiers: [.command]
        ) else {
            return false
        }
        sendEditingShortcut(shortcut)
        return true
    }

    private func configureParentAfterFork(masterFD: Int32, childPID: Int32) {
        _ = fcntl(masterFD, F_SETFL, O_NONBLOCK)

        lock.lock()
        self.masterFD = masterFD
        closeMasterFDOnCancel = true
        shellPID = childPID
        launchDate = Date()
        readSource = DispatchSource.makeReadSource(fileDescriptor: masterFD, queue: ioQueue)
        processSource = DispatchSource.makeProcessSource(identifier: childPID, eventMask: .exit, queue: ioQueue)
        let readSource = self.readSource
        let processSource = self.processSource
        lock.unlock()

        logger.log(
            "ghostty-process",
            "started session=\(sessionID.uuidString) shellPID=\(childPID) instance=\(processInstanceID)"
        )

        readSource?.setEventHandler { [weak self] in
            self?.consumeReadableOutput()
        }
        readSource?.setCancelHandler {
            self.lock.lock()
            let shouldClose = self.closeMasterFDOnCancel
            self.closeMasterFDOnCancel = false
            self.lock.unlock()
            if shouldClose {
                _ = close(masterFD)
            }
        }
        readSource?.resume()

        processSource?.setEventHandler { [weak self] in
            self?.handleProcessExit(expectedPID: childPID)
        }
        processSource?.resume()

        resizeProcess(lastViewport)
    }

    private func consumeReadableOutput() {
        let fd: Int32
        lock.lock()
        fd = masterFD
        lock.unlock()

        guard fd >= 0 else {
            return
        }

        var buffer = [UInt8](repeating: 0, count: 16384)
        while true {
            let count = read(fd, &buffer, buffer.count)
            if count > 0 {
                let data = Data(buffer.prefix(count))
                var fireFirstOutput = false

                lock.lock()
                if hasObservedOutput == false {
                    hasObservedOutput = true
                    fireFirstOutput = true
                }
                lock.unlock()

                DmuxTerminalOutputEventEmitter.shared.noteOutput(sessionID: sessionID)
                terminalSession.receive(data)

                if fireFirstOutput {
                    DispatchQueue.main.async { [weak self] in
                        self?.onFirstOutput?()
                    }
                }
                continue
            }

            if count == 0 {
                return
            }

            if errno == EAGAIN || errno == EWOULDBLOCK {
                return
            }

            if errno == EINTR {
                continue
            }

            return
        }
    }

    private func handleProcessExit(expectedPID: Int32) {
        var status: Int32 = 0
        let waitedPID = waitpid(expectedPID, &status, 0)
        let exitCode: Int32?
        if waitedPID > 0, GhosttyWaitStatus.didExit(status) {
            exitCode = GhosttyWaitStatus.exitStatus(status)
        } else if waitedPID > 0, GhosttyWaitStatus.didSignal(status) {
            exitCode = 128 + GhosttyWaitStatus.termSignal(status)
        } else {
            exitCode = nil
        }

        let runtimeMs = max(0, UInt64(Date().timeIntervalSince(launchDate) * 1000))
        terminalSession.finish(
            exitCode: UInt32(max(0, exitCode ?? 0)),
            runtimeMilliseconds: runtimeMs
        )

        lock.lock()
        let readSource = self.readSource
        self.readSource = nil
        self.processSource = nil
        shellPID = 0
        masterFD = -1
        closeMasterFDOnCancel = false
        lock.unlock()

        readSource?.cancel()
        logger.log(
            "ghostty-process",
            "exited session=\(sessionID.uuidString) exit=\(exitCode.map(String.init) ?? "nil")"
        )

        DispatchQueue.main.async { [weak self] in
            self?.onProcessTerminated?(exitCode)
        }
    }

    private func writeToProcess(_ data: Data) {
        let fd: Int32
        lock.lock()
        fd = masterFD
        lock.unlock()

        guard fd >= 0, !data.isEmpty else {
            return
        }

        if data.contains(0x03) {
            DispatchQueue.main.async {
                NotificationCenter.default.post(
                    name: .dmuxTerminalInterruptDidSend,
                    object: self.sessionID
                )
            }
        }

        ioQueue.async {
            data.withUnsafeBytes { buffer in
                guard var base = buffer.baseAddress?.assumingMemoryBound(to: UInt8.self) else {
                    return
                }
                var remaining = buffer.count
                while remaining > 0 {
                    let written = write(fd, base, remaining)
                    if written > 0 {
                        remaining -= written
                        base = base.advanced(by: written)
                        continue
                    }
                    if written < 0, errno == EINTR {
                        continue
                    }
                    break
                }
            }
        }
    }

    private func resizeProcess(_ viewport: InMemoryTerminalViewport) {
        let viewportToApply: InMemoryTerminalViewport?
        lock.lock()
        lastViewport = viewport
        viewportToApply = resizeTransitionState.capture(viewport)
        let fd = masterFD
        let pid = shellPID
        lock.unlock()

        guard let viewportToApply else {
            return
        }
        guard fd >= 0 else {
            return
        }

        ioQueue.async {
            var winsizeValue = winsize(
                ws_row: viewportToApply.rows,
                ws_col: viewportToApply.columns,
                ws_xpixel: UInt16(min(viewportToApply.widthPixels, UInt32(UInt16.max))),
                ws_ypixel: UInt16(min(viewportToApply.heightPixels, UInt32(UInt16.max)))
            )
            _ = ioctl(fd, TIOCSWINSZ, &winsizeValue)
            if pid > 0 {
                kill(pid, SIGWINCH)
                kill(-pid, SIGWINCH)
            }
        }
    }

    private func shellLaunchConfiguration(shellName: String, command: String) -> (args: [String], execName: String) {
        switch shellName {
        case "zsh", "bash", "fish":
            if command == shellName || command.hasSuffix("/\(shellName)") {
                return (["-i", "-l"], shellName)
            }
            return (["-i", "-l", "-c", command], shellName)
        default:
            return command == shellName ? ([], shellName) : (["-lc", command], "-\(shellName)")
        }
    }
}

@MainActor
struct GhosttyTerminalHostView: NSViewRepresentable {
    let session: TerminalSession
    let environment: [(String, String)]
    let terminalBackgroundPreset: AppTerminalBackgroundPreset
    let backgroundColorPreset: AppBackgroundColorPreset
    let terminalFontSize: Int
    let isFocused: Bool
    let isVisible: Bool
    let showsInactiveOverlay: Bool
    let shouldFocus: Bool
    var onInteraction: (() -> Void)? = nil
    var onFocusConsumed: (() -> Void)? = nil
    var onStartupSucceeded: (() -> Void)? = nil
    var onStartupFailure: ((String) -> Void)? = nil

    func makeCoordinator() -> Coordinator {
        Coordinator()
    }

    func makeNSView(context: Context) -> HostContainerView {
        let container = HostContainerView(frame: .zero)
        container.wantsLayer = false
        return container
    }

    func updateNSView(_ nsView: HostContainerView, context: Context) {
        let view = GhosttyTerminalRegistry.shared.containerView(
            for: session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset,
            terminalFontSize: terminalFontSize,
            isFocused: isFocused,
            isVisible: isVisible,
            showsInactiveOverlay: showsInactiveOverlay,
            onInteraction: onInteraction,
            onFocusConsumed: onFocusConsumed,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        let coordinator = context.coordinator
        coordinator.containerView = view
        coordinator.bindGeneration &+= 1
        let generation = coordinator.bindGeneration
        let bindVisibility = isVisible
        coordinator.boundAnchorId = ObjectIdentifier(nsView)

        let bindHostedView: (Bool) -> Void = { [weak nsView, weak view, weak coordinator] synchronizeAnchor in
            guard let nsView, let view, let coordinator else { return }
            guard coordinator.bindGeneration == generation else { return }
            guard nsView.window != nil else {
                GhosttyTerminalPortalRegistry.detach(
                    hostedView: view,
                    ifOwnedByAnchorId: coordinator.boundAnchorId
                )
                return
            }
            GhosttyTerminalPortalRegistry.bind(hostedView: view, to: nsView, visibleInUI: bindVisibility)
            if synchronizeAnchor {
                GhosttyTerminalPortalRegistry.synchronizeForAnchor(nsView)
            }
        }

        nsView.onDidMoveToWindow = {
            bindHostedView(false)
        }
        nsView.onGeometryChanged = {
            bindHostedView(true)
        }

        if nsView.window != nil {
            GhosttyTerminalPortalRegistry.bind(hostedView: view, to: nsView, visibleInUI: isVisible)
            GhosttyTerminalPortalRegistry.synchronizeForAnchor(nsView)
        } else {
            GhosttyTerminalPortalRegistry.updateEntryVisibility(for: view, visibleInUI: isVisible)
        }

        if shouldFocus, coordinator.lastShouldFocus == false {
            DispatchQueue.main.async {
                view.focusTerminal()
            }
        }
        coordinator.lastShouldFocus = shouldFocus
    }

    final class Coordinator {
        weak var containerView: GhosttyTerminalContainerView?
        var bindGeneration: UInt64 = 0
        var lastShouldFocus = false
        var boundAnchorId: ObjectIdentifier?
    }

    final class HostContainerView: NSView {
        var onDidMoveToWindow: (() -> Void)?
        var onGeometryChanged: (() -> Void)?
        private var lastGeometrySignature: String = ""
        private var geometryNotificationScheduled = false

        override func viewDidMoveToWindow() {
            super.viewDidMoveToWindow()
            onDidMoveToWindow?()
            notifyGeometryChangedIfNeeded()
        }

        override func viewDidMoveToSuperview() {
            super.viewDidMoveToSuperview()
            notifyGeometryChangedIfNeeded()
        }

        override func layout() {
            super.layout()
            notifyGeometryChangedIfNeeded()
        }

        override func setFrameOrigin(_ newOrigin: NSPoint) {
            super.setFrameOrigin(newOrigin)
            notifyGeometryChangedIfNeeded()
        }

        override func setFrameSize(_ newSize: NSSize) {
            super.setFrameSize(newSize)
            notifyGeometryChangedIfNeeded()
        }

        private func notifyGeometryChangedIfNeeded() {
            let signature = "\(frame.debugDescription)|\(bounds.debugDescription)|\(window?.windowNumber ?? -1)|\(superview.map { ObjectIdentifier($0).debugDescription } ?? "nil")"
            guard signature != lastGeometrySignature else {
                return
            }
            lastGeometrySignature = signature
            guard geometryNotificationScheduled == false else {
                return
            }
            geometryNotificationScheduled = true
            DispatchQueue.main.async { [weak self] in
                guard let self else {
                    return
                }
                self.geometryNotificationScheduled = false
                self.onGeometryChanged?()
            }
        }
    }

    static func dismantleNSView(_ nsView: HostContainerView, coordinator: Coordinator) {
        coordinator.bindGeneration &+= 1
        nsView.onDidMoveToWindow = nil
        nsView.onGeometryChanged = nil
        if let containerView = coordinator.containerView {
            GhosttyTerminalPortalRegistry.detach(
                hostedView: containerView,
                ifOwnedByAnchorId: coordinator.boundAnchorId
            )
        }
        coordinator.containerView = nil
        coordinator.boundAnchorId = nil
    }
}

fileprivate struct GhosttyTerminalSessionResources {
    let processBridge: GhosttyPTYProcessBridge
    let controller: TerminalController
    let hasStartedProcess: Bool
    let hasReceivedInitialOutput: Bool
    let pendingFocusRequest: Bool
    let hasReportedStartupFailure: Bool
}

@MainActor
enum GhosttyPortalHostRegistry {
    private final class WeakHostBox {
        weak var view: NSView?

        init(view: NSView) {
            self.view = view
        }
    }

    private static var hostViewByWindowId: [ObjectIdentifier: WeakHostBox] = [:]

    static func register(hostView: NSView, for window: NSWindow) {
        hostViewByWindowId[ObjectIdentifier(window)] = WeakHostBox(view: hostView)
    }

    static func unregister(hostView: NSView, for window: NSWindow) {
        let key = ObjectIdentifier(window)
        guard let existing = hostViewByWindowId[key]?.view, existing === hostView else {
            return
        }
        hostViewByWindowId[key] = nil
    }

    static func hostView(for window: NSWindow) -> NSView? {
        let key = ObjectIdentifier(window)
        if let view = hostViewByWindowId[key]?.view {
            return view
        }
        hostViewByWindowId[key] = nil
        return nil
    }
}

@MainActor
private final class GhosttyTerminalPortalOverlayView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        for subview in subviews.reversed() where !subview.isHidden && subview.alphaValue > 0.001 {
            guard subview.frame.contains(point) else {
                continue
            }
            let converted = convert(point, to: subview)
            if let hit = subview.hitTest(converted) {
                return hit
            }
        }
        return nil
    }
}

@MainActor
private final class GhosttyTerminalPortalMountedView: NSView {
    let hostedView: GhosttyTerminalContainerView

    init(hostedView: GhosttyTerminalContainerView) {
        self.hostedView = hostedView
        super.init(frame: .zero)
        translatesAutoresizingMaskIntoConstraints = true
        autoresizingMask = []
        wantsLayer = true
        layer?.masksToBounds = true
        layer?.backgroundColor = NSColor.clear.cgColor
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.001 else {
            return nil
        }
        guard bounds.contains(point) else {
            return nil
        }

        if hostedView.isReadyForInteraction, let event = NSApp.currentEvent {
            switch event.type {
            case .leftMouseDown, .rightMouseDown, .otherMouseDown:
                hostedView.prepareForPointerInteraction()
            default:
                break
            }
        }

        let converted = convert(point, to: hostedView)
        return hostedView.hitTest(converted) ?? hostedView
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }

    override func mouseDown(with event: NSEvent) {
        hostedView.forwardMouseDown(event)
    }

    override func rightMouseDown(with event: NSEvent) {
        hostedView.forwardRightMouseDown(event)
    }

    override func otherMouseDown(with event: NSEvent) {
        hostedView.forwardOtherMouseDown(event)
    }

    override func layout() {
        super.layout()
        if hostedView.superview !== self {
            addSubview(hostedView)
        }
        if hostedView.frame != bounds {
            hostedView.frame = bounds
        }
    }
}

@MainActor
private final class GhosttyWindowPortal {
    struct Entry {
        weak var anchorView: NSView?
        let hostedView: GhosttyTerminalContainerView
        let mountedView: GhosttyTerminalPortalMountedView
        var visibleInUI: Bool
    }

    weak var window: NSWindow?
    weak var hostView: NSView?
    let overlayView: GhosttyTerminalPortalOverlayView
    private var entries: [ObjectIdentifier: Entry] = [:]
    private var closeObserver: Any?

    init(window: NSWindow, hostView: NSView) {
        self.window = window
        self.hostView = hostView
        let overlayView = GhosttyTerminalPortalOverlayView(frame: .zero)
        overlayView.translatesAutoresizingMaskIntoConstraints = true
        overlayView.autoresizingMask = [.width, .height]
        overlayView.frame = hostView.bounds
        overlayView.wantsLayer = false
        self.overlayView = overlayView

        hostView.addSubview(overlayView, positioned: .above, relativeTo: nil)

        closeObserver = NotificationCenter.default.addObserver(
            forName: NSWindow.willCloseNotification,
            object: window,
            queue: .main
        ) { [weak window] _ in
            guard let window else { return }
            Task { @MainActor in
                GhosttyTerminalPortalRegistry.removePortal(for: window)
            }
        }
    }

    func tearDown(retainedHostedIds: Set<ObjectIdentifier>) {
        if let closeObserver {
            NotificationCenter.default.removeObserver(closeObserver)
            self.closeObserver = nil
        }
        for (hostedId, entry) in entries {
            if retainedHostedIds.contains(hostedId) {
                entry.hostedView.isHidden = true
                entry.hostedView.removeFromSuperview()
            }
            entry.mountedView.isHidden = true
            entry.mountedView.removeFromSuperview()
        }
        entries.removeAll()
        overlayView.removeFromSuperview()
    }

    func bind(hostedView: GhosttyTerminalContainerView, to anchorView: NSView, visibleInUI: Bool) {
        let hostedId = ObjectIdentifier(hostedView)
        let mountedView: GhosttyTerminalPortalMountedView
        if let existing = entries[hostedId]?.mountedView {
            mountedView = existing
        } else {
            mountedView = GhosttyTerminalPortalMountedView(hostedView: hostedView)
        }
        entries[hostedId] = Entry(
            anchorView: anchorView,
            hostedView: hostedView,
            mountedView: mountedView,
            visibleInUI: visibleInUI
        )

        if mountedView.superview !== overlayView {
            mountedView.removeFromSuperview()
            overlayView.addSubview(mountedView)
        }

        if hostedView.superview !== mountedView {
            hostedView.removeFromSuperview()
            mountedView.addSubview(hostedView)
        }

        synchronizeHostedView(withId: hostedId)
    }

    func detachHostedView(withId hostedId: ObjectIdentifier) {
        guard var entry = entries[hostedId] else { return }
        entry.anchorView = nil
        entry.visibleInUI = false
        // Do NOT hide hostedView — it will be rebound by the next bind() call.
        // Only hide mountedView so the overlay stops showing a stale frame.
        entry.mountedView.isHidden = true
        entries[hostedId] = entry
    }

    func updateEntryVisibility(for hostedId: ObjectIdentifier, visibleInUI: Bool) {
        guard var entry = entries[hostedId] else { return }
        entry.visibleInUI = visibleInUI
        entries[hostedId] = entry
        synchronizeHostedView(withId: hostedId)
    }

    func synchronizeHostedViewForAnchor(_ anchorView: NSView) {
        for hostedId in entries.keys where entries[hostedId]?.anchorView === anchorView {
            synchronizeHostedView(withId: hostedId)
        }
    }

    private func synchronizeHostedView(withId hostedId: ObjectIdentifier) {
        guard let window, let hostView, let entry = entries[hostedId] else {
            return
        }

        if overlayView.superview !== hostView {
            overlayView.removeFromSuperview()
            hostView.addSubview(overlayView, positioned: .above, relativeTo: nil)
        }
        overlayView.frame = hostView.bounds

        guard entry.visibleInUI,
              let anchorView = entry.anchorView,
              anchorView.window === window,
              anchorView.superview != nil,
              !anchorView.bounds.isEmpty,
              !anchorView.dmuxHasHiddenAncestor else {
            entry.hostedView.isHidden = true
            entry.mountedView.isHidden = true
            return
        }

        let frameInHost = anchorView.convert(anchorView.bounds, to: hostView).integral
        guard frameInHost.width > 1, frameInHost.height > 1 else {
            entry.hostedView.isHidden = true
            entry.mountedView.isHidden = true
            return
        }

        if entry.mountedView.superview !== overlayView {
            overlayView.addSubview(entry.mountedView)
        }
        if entry.hostedView.superview !== entry.mountedView {
            entry.hostedView.removeFromSuperview()
            entry.mountedView.addSubview(entry.hostedView)
        }
        if entry.mountedView.frame != frameInHost {
            entry.mountedView.frame = frameInHost
        }
        entry.mountedView.needsLayout = true
        entry.mountedView.layoutSubtreeIfNeeded()
        entry.mountedView.isHidden = false
        entry.hostedView.isHidden = false
        entry.hostedView.layoutSubtreeIfNeeded()
    }
}

@MainActor
private enum GhosttyTerminalPortalRegistry {
    private static var portalsByWindowId: [ObjectIdentifier: GhosttyWindowPortal] = [:]
    private static var hostedToWindowId: [ObjectIdentifier: ObjectIdentifier] = [:]
    private static var hostedToAnchorId: [ObjectIdentifier: ObjectIdentifier] = [:]

    static func bind(
        hostedView: GhosttyTerminalContainerView,
        to anchorView: NSView,
        visibleInUI: Bool
    ) {
        guard let window = anchorView.window else {
            detach(hostedView: hostedView)
            return
        }

        let hostedId = ObjectIdentifier(hostedView)
        let windowId = ObjectIdentifier(window)

        if let oldWindowId = hostedToWindowId[hostedId], oldWindowId != windowId {
            portalsByWindowId[oldWindowId]?.detachHostedView(withId: hostedId)
        }

        let portal = portal(for: window)
        portal.bind(hostedView: hostedView, to: anchorView, visibleInUI: visibleInUI)
        hostedToWindowId[hostedId] = windowId
        hostedToAnchorId[hostedId] = ObjectIdentifier(anchorView)
    }

    static func detach(hostedView: GhosttyTerminalContainerView) {
        let hostedId = ObjectIdentifier(hostedView)
        if let windowId = hostedToWindowId[hostedId] {
            portalsByWindowId[windowId]?.detachHostedView(withId: hostedId)
        }
    }

    static func detach(
        hostedView: GhosttyTerminalContainerView,
        ifOwnedByAnchorId ownerAnchorId: ObjectIdentifier?
    ) {
        guard let ownerAnchorId else {
            return
        }
        let hostedId = ObjectIdentifier(hostedView)
        guard hostedToAnchorId[hostedId] == ownerAnchorId else {
            return
        }
        guard let windowId = hostedToWindowId[hostedId] else {
            return
        }
        portalsByWindowId[windowId]?.detachHostedView(withId: hostedId)
    }

    static func updateEntryVisibility(for hostedView: GhosttyTerminalContainerView, visibleInUI: Bool) {
        let hostedId = ObjectIdentifier(hostedView)
        guard let windowId = hostedToWindowId[hostedId] else {
            hostedView.isHidden = !visibleInUI
            return
        }
        portalsByWindowId[windowId]?.updateEntryVisibility(for: hostedId, visibleInUI: visibleInUI)
    }

    static func synchronizeForAnchor(_ anchorView: NSView) {
        guard let window = anchorView.window else { return }
        portal(for: window).synchronizeHostedViewForAnchor(anchorView)
    }

    static func removePortal(for window: NSWindow) {
        let windowId = ObjectIdentifier(window)
        let retainedHostedIds = Set(
            hostedToWindowId.compactMap { hostedId, mappedWindowId in
                mappedWindowId == windowId ? hostedId : nil
            }
        )
        portalsByWindowId.removeValue(forKey: windowId)?.tearDown(retainedHostedIds: retainedHostedIds)
        hostedToWindowId = hostedToWindowId.filter { $0.value != windowId }
        hostedToAnchorId = hostedToAnchorId.filter { hostedId, _ in
            hostedToWindowId[hostedId] != nil
        }
    }

    private static func portal(for window: NSWindow) -> GhosttyWindowPortal {
        let windowId = ObjectIdentifier(window)
        guard let hostView = GhosttyPortalHostRegistry.hostView(for: window) ?? window.contentView else {
            fatalError("Ghostty portal host unavailable")
        }
        if let existing = portalsByWindowId[windowId] {
            if existing.hostView !== hostView {
                let retainedHostedIds = Set(
                    hostedToWindowId.compactMap { hostedId, mappedWindowId in
                        mappedWindowId == windowId ? hostedId : nil
                    }
                )
                existing.tearDown(retainedHostedIds: retainedHostedIds)
                let replacement = GhosttyWindowPortal(window: window, hostView: hostView)
                portalsByWindowId[windowId] = replacement
                return replacement
            }
            return existing
        }
        let created = GhosttyWindowPortal(window: window, hostView: hostView)
        portalsByWindowId[windowId] = created
        return created
    }
}

private extension NSView {
    var dmuxHasHiddenAncestor: Bool {
        var current: NSView? = self
        while let view = current {
            if view.isHidden || view.alphaValue <= 0.001 {
                return true
            }
            current = view.superview
        }
        return false
    }
}

@MainActor
private final class GhosttyTerminalDimOverlayView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        nil
    }
}

@MainActor
private final class GhosttyTerminalLoadingShieldView: NSView {
    override func hitTest(_ point: NSPoint) -> NSView? {
        guard !isHidden, alphaValue > 0.001, bounds.contains(point) else {
            return nil
        }
        return self
    }

    override func acceptsFirstMouse(for event: NSEvent?) -> Bool {
        true
    }
}

@MainActor
final class GhosttyTerminalContainerView: NSView, TerminalSurfaceFocusDelegate, TerminalSurfaceCloseDelegate {
    private var terminalView = AppTerminalView(frame: .zero)
    private let inactiveOverlayView = GhosttyTerminalDimOverlayView(frame: .zero)
    private let loadingShieldView = GhosttyTerminalLoadingShieldView(frame: .zero)
    private var configuredSession: TerminalSession
    private var configuredEnvironment: [(String, String)]
    private var terminalBackgroundPreset: AppTerminalBackgroundPreset
    private var backgroundColorPreset: AppBackgroundColorPreset
    private var terminalFontSize: Int
    private var onInteraction: (() -> Void)?
    private var onFocusConsumed: (() -> Void)?
    private var onStartupSucceeded: (() -> Void)?
    private var onStartupFailure: ((String) -> Void)?
    private var hasStartedProcess = false
    private var hasReceivedInitialOutput = false
    private var pendingFocusRequest = false
    private var hasReportedStartupFailure = false
    private var pendingStartWorkItem: DispatchWorkItem?
    private var startupWatchdogWorkItem: DispatchWorkItem?
    private var structuralResizeRestoreWorkItem: DispatchWorkItem?
    private var geometryReconcileGeneration: UInt64 = 0
    private var geometryReconcileScheduled = false
    private var pendingGeometryReconcilePasses = 0
    private var lastAppliedFocusedState: Bool?
    private var lastAppliedVisibleState: Bool?
    private var lastShowsInactiveOverlay = false
    private let startupDelay: TimeInterval = 0.18
    private let startupWatchdogDelay: TimeInterval = 3.5
    private let logger = AppDebugLog.shared

    private let processBridge: GhosttyPTYProcessBridge
    private let controller: TerminalController

    fileprivate var isReadyForInteraction: Bool {
        hasReceivedInitialOutput
    }

    fileprivate init(
        session: TerminalSession,
        environment: [(String, String)],
        terminalFontSize: Int,
        sessionResources: GhosttyTerminalSessionResources? = nil,
        onInteraction: (() -> Void)?,
        onFocusConsumed: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) {
        configuredSession = session
        configuredEnvironment = environment
        terminalBackgroundPreset = .flexokiDark
        backgroundColorPreset = .automatic
        self.terminalFontSize = terminalFontSize
        self.onInteraction = onInteraction
        self.onFocusConsumed = onFocusConsumed
        self.onStartupSucceeded = onStartupSucceeded
        self.onStartupFailure = onStartupFailure
        if let sessionResources {
            processBridge = sessionResources.processBridge
            controller = sessionResources.controller
            hasStartedProcess = sessionResources.hasStartedProcess
            hasReceivedInitialOutput = sessionResources.hasReceivedInitialOutput
            pendingFocusRequest = sessionResources.pendingFocusRequest
            hasReportedStartupFailure = sessionResources.hasReportedStartupFailure
        } else {
            processBridge = GhosttyPTYProcessBridge(sessionID: session.id)
            controller = Self.makeController(
                backgroundPreset: .flexokiDark,
                backgroundColorPreset: .automatic,
                logger: AppDebugLog.shared
            )
        }
        super.init(frame: .zero)
        setup()
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }

    func updateSession(
        _ session: TerminalSession,
        environment: [(String, String)],
        terminalBackgroundPreset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset,
        terminalFontSize: Int,
        isFocused: Bool,
        isVisible: Bool,
        showsInactiveOverlay: Bool,
        onInteraction: (() -> Void)?,
        onFocusConsumed: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) {
        configuredEnvironment = environment
        self.onInteraction = onInteraction
        self.onFocusConsumed = onFocusConsumed
        self.onStartupSucceeded = onStartupSucceeded
        self.onStartupFailure = onStartupFailure
        lastShowsInactiveOverlay = showsInactiveOverlay

        if self.terminalBackgroundPreset != terminalBackgroundPreset || self.backgroundColorPreset != backgroundColorPreset {
            self.backgroundColorPreset = backgroundColorPreset
            applyTerminalBackgroundPreset(terminalBackgroundPreset)
        }

        if self.terminalFontSize != terminalFontSize {
            self.terminalFontSize = terminalFontSize
            terminalView.configuration = surfaceOptions()
        }

        if configuredSession.id != session.id {
            cancelDeferredLifecycleWork()
            DmuxTerminalOutputEventEmitter.shared.clear(sessionID: configuredSession.id)
            processBridge.terminateProcessTree()
            configuredSession = session
            hasStartedProcess = false
            hasReceivedInitialOutput = false
            hasReportedStartupFailure = false
            pendingFocusRequest = false
            lastAppliedFocusedState = nil
            lastAppliedVisibleState = nil
            updateLoadingShieldVisibility()
            terminalView.configuration = surfaceOptions()
        } else {
            configuredSession = session
        }

        let focusChanged = lastAppliedFocusedState != isFocused
        let visibilityChanged = lastAppliedVisibleState != isVisible
        lastAppliedFocusedState = isFocused
        lastAppliedVisibleState = isVisible

        if isVisible {
            scheduleProcessStartIfPossible(reason: isFocused ? "update-focused" : "update-visible")
            if focusChanged || visibilityChanged {
                scheduleGeometryReconcile()
            }
        }

        let showsDimOverlay = showsInactiveOverlay && isVisible && !isFocused
        setInactiveOverlay(visible: showsDimOverlay)
        applyEffectiveBackgroundColor()

        if isFocused && focusChanged {
            focusTerminal()
        }
    }

    func applyTerminalBackgroundPreset(_ preset: AppTerminalBackgroundPreset) {
        guard terminalBackgroundPreset != preset else {
            if controller.setTheme(Self.theme(for: preset, backgroundColorPreset: backgroundColorPreset)) == false {
                return
            }
            inactiveOverlayView.layer?.backgroundColor = Self.inactiveOverlayColor(for: preset, backgroundColorPreset: backgroundColorPreset).cgColor
            applyEffectiveBackgroundColor()
            return
        }
        terminalBackgroundPreset = preset
        let themeChanged = controller.setTheme(Self.theme(for: preset, backgroundColorPreset: backgroundColorPreset))
        inactiveOverlayView.layer?.backgroundColor = Self.inactiveOverlayColor(for: preset, backgroundColorPreset: backgroundColorPreset).cgColor

        let isFocused = lastAppliedFocusedState ?? false
        let isVisible = lastAppliedVisibleState ?? false
        let showsDimOverlay = lastShowsInactiveOverlay && isVisible && !isFocused
        setInactiveOverlay(visible: showsDimOverlay)
        applyEffectiveBackgroundColor()

        guard themeChanged, window != nil else {
            return
        }

        scheduleGeometryReconcile()
    }

    func focusTerminal() {
        guard hasReceivedInitialOutput else {
            pendingFocusRequest = true
            if let window,
               let responder = window.firstResponder,
               GhosttyTerminalRegistry.shared.ownsResponder(responder),
               ownsResponder(responder) == false {
                window.makeFirstResponder(nil)
                GhosttyTerminalRegistry.shared.clearFocusedSession()
            }
            return
        }
        if window?.firstResponder === terminalView {
            GhosttyTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            onFocusConsumed?()
            return
        }
        if window?.makeFirstResponder(terminalView) == true,
           window?.firstResponder === terminalView {
            GhosttyTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            onFocusConsumed?()
        }
    }

    func beginStructuralResizeTransition(duration: TimeInterval = 0.22) {
        structuralResizeRestoreWorkItem?.cancel()
        processBridge.beginStructuralResizeTransition()

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.structuralResizeRestoreWorkItem = nil
            self.layoutSubtreeIfNeeded()
            self.terminalView.layoutSubtreeIfNeeded()
            self.terminalView.fitToSize()
            self.processBridge.endStructuralResizeTransition()
        }
        structuralResizeRestoreWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + duration, execute: workItem)
    }

    func reconcileGeometry() {
        guard bounds.width > 0, bounds.height > 0 else {
            return
        }
        scheduleGeometryReconcile()
    }

    var terminalShellPID: Int32? {
        processBridge.currentShellPID
    }

    var terminalProjectID: UUID {
        configuredSession.projectID
    }

    var terminalProcessInstanceID: String {
        processBridge.processInstanceID
    }

    func terminateProcessTree() {
        processBridge.terminateProcessTree()
    }

    func prepareForPermanentRemoval() {
        logger.log(
            "ghostty-lifecycle",
            "prepare-remove session=\(configuredSession.id.uuidString) instance=\(processBridge.processInstanceID)"
        )
        cancelDeferredLifecycleWork()
        processBridge.terminateProcessTree()
        tearDownTerminalView()
    }

    var isTerminalFocused: Bool {
        guard let responder = window?.firstResponder else {
            return false
        }
        if responder === terminalView {
            return true
        }
        if let view = responder as? NSView, view.isDescendant(of: terminalView) {
            return true
        }

        var next: NSResponder? = responder.nextResponder
        while let current = next {
            if current === terminalView {
                return true
            }
            next = current.nextResponder
        }
        return false
    }

    override func layout() {
        super.layout()
        if hasStartedProcess == false {
            scheduleProcessStartIfPossible(reason: "layout")
        }
    }

    override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        if window == nil {
            cancelDeferredLifecycleWork()
            return
        }
        scheduleProcessStartIfPossible(reason: "window-attached")
        scheduleGeometryReconcile()
    }

    override func viewDidMoveToSuperview() {
        super.viewDidMoveToSuperview()
        guard superview != nil, window != nil else {
            return
        }
        scheduleGeometryReconcile()
    }

    func sendText(_ text: String) {
        notifyInteraction()
        processBridge.sendText(text)
    }

    func sendInterrupt() {
        notifyInteraction()
        processBridge.sendInterrupt()
    }

    func sendEscape() {
        notifyInteraction()
        processBridge.sendEscape()
    }

    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut) {
        notifyInteraction()
        processBridge.sendEditingShortcut(shortcut)
    }

    func sendNativeCommandArrow(keyCode: UInt16) -> Bool {
        notifyInteraction()
        return processBridge.sendNativeCommandArrow(keyCode: keyCode)
    }

    func forwardKeyDown(_ event: NSEvent) -> Bool {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return false
        }
        notifyInteraction()
        focusTerminal()
        terminalView.keyDown(with: event)
        return true
    }

    func forwardMouseDown(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.mouseDown(with: event)
    }

    func forwardRightMouseDown(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.rightMouseDown(with: event)
    }

    func forwardOtherMouseDown(_ event: NSEvent) {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        prepareForPointerInteraction()
        terminalView.otherMouseDown(with: event)
    }

    func ownsResponder(_ responder: NSResponder?) -> Bool {
        guard let responder else {
            return false
        }
        if responder === terminalView {
            return true
        }
        if let view = responder as? NSView, view.isDescendant(of: terminalView) {
            return true
        }
        var next: NSResponder? = responder.nextResponder
        while let current = next {
            if current === terminalView {
                return true
            }
            next = current.nextResponder
        }
        return false
    }

    func terminalDidChangeFocus(_ focused: Bool) {
        if focused {
            notifyInteraction()
            GhosttyTerminalRegistry.shared.markFocused(sessionID: configuredSession.id)
            onFocusConsumed?()
        }
    }

    func terminalDidClose(processAlive: Bool) {
        logger.log(
            "ghostty-process",
            "surface-closed session=\(configuredSession.id.uuidString) processAlive=\(processAlive)"
        )
    }

    private func setup() {
        wantsLayer = true
        let appearance = Self.resolvedAppearance(
            for: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset
        )
        layer?.backgroundColor = appearance.backgroundColor.cgColor
        layerContentsRedrawPolicy = .duringViewResize

        loadingShieldView.translatesAutoresizingMaskIntoConstraints = false
        loadingShieldView.wantsLayer = true
        loadingShieldView.layer?.backgroundColor = appearance.backgroundColor.cgColor
        inactiveOverlayView.translatesAutoresizingMaskIntoConstraints = false
        inactiveOverlayView.wantsLayer = true
        inactiveOverlayView.layer?.backgroundColor = Self.inactiveOverlayColor(for: terminalBackgroundPreset, backgroundColorPreset: backgroundColorPreset).cgColor
        inactiveOverlayView.isHidden = true

        configureTerminalView(terminalView)
        addSubview(terminalView)
        addSubview(loadingShieldView)
        addSubview(inactiveOverlayView)
        pinTerminalView(terminalView)
        pinInactiveOverlayView()
        pinLoadingShieldView()

        bindProcessCallbacks()
        updateLoadingShieldVisibility()
    }

    fileprivate func transplantSessionResources() -> GhosttyTerminalSessionResources {
        cancelDeferredLifecycleWork()
        let shouldRefocus = pendingFocusRequest || isTerminalFocused
        processBridge.onFirstOutput = nil
        processBridge.onProcessTerminated = nil

        tearDownTerminalView()

        return GhosttyTerminalSessionResources(
            processBridge: processBridge,
            controller: controller,
            hasStartedProcess: hasStartedProcess,
            hasReceivedInitialOutput: hasReceivedInitialOutput,
            pendingFocusRequest: shouldRefocus,
            hasReportedStartupFailure: hasReportedStartupFailure
        )
    }

    private func bindProcessCallbacks() {
        processBridge.onFirstOutput = { [weak self] in
            self?.markTerminalReady(reason: "initial-output")
        }
        processBridge.onProcessTerminated = { [weak self] exitCode in
            self?.handleProcessTermination(exitCode: exitCode)
        }
    }

    private func cancelDeferredLifecycleWork() {
        pendingStartWorkItem?.cancel()
        pendingStartWorkItem = nil
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
        structuralResizeRestoreWorkItem?.cancel()
        structuralResizeRestoreWorkItem = nil
    }

    private func scheduleProcessStartIfPossible(reason: String) {
        guard hasStartedProcess == false else {
            return
        }
        guard window != nil, bounds.width > 0, bounds.height > 0 else {
            return
        }
        guard pendingStartWorkItem == nil else {
            return
        }

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.pendingStartWorkItem = nil
            self.startProcessIfPossible(trigger: reason)
        }
        pendingStartWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + startupDelay, execute: workItem)
    }

    private func startProcessIfPossible(trigger: String) {
        guard hasStartedProcess == false else {
            return
        }
        guard window != nil, bounds.width > 0, bounds.height > 0 else {
            scheduleProcessStartIfPossible(reason: "\(trigger)-awaiting-layout")
            return
        }

        let shell = configuredSession.shell
        let shellName = URL(fileURLWithPath: shell).lastPathComponent
        hasStartedProcess = true
        hasReportedStartupFailure = false
        processBridge.resetOutputObservation()
        terminalView.configuration = surfaceOptions()
        processBridge.start(
            shell: shell,
            shellName: shellName,
            command: configuredSession.command,
            cwd: configuredSession.cwd,
            environment: configuredEnvironment
        )

        installStartupWatchdog()
        logger.log(
            "ghostty-process",
            "start session=\(configuredSession.id.uuidString) project=\(configuredSession.projectID.uuidString) reason=\(trigger) shell=\(shell)"
        )
    }

    private func installStartupWatchdog() {
        startupWatchdogWorkItem?.cancel()
        let workItem = DispatchWorkItem { [weak self] in
            guard let self else {
                return
            }
            self.startupWatchdogWorkItem = nil
            guard self.hasReceivedInitialOutput == false else {
                return
            }
            guard self.terminalShellPID == nil else {
                self.markTerminalReady(reason: "alive-without-output")
                return
            }
            self.reportStartupFailureIfNeeded("shell pid not available after \(Int(self.startupWatchdogDelay * 1000))ms")
        }
        startupWatchdogWorkItem = workItem
        DispatchQueue.main.asyncAfter(deadline: .now() + startupWatchdogDelay, execute: workItem)
    }

    private func markTerminalReady(reason: String) {
        guard hasReceivedInitialOutput == false else {
            return
        }
        hasReceivedInitialOutput = true
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
        hasReportedStartupFailure = false
        updateLoadingShieldVisibility()
        logger.log(
            "ghostty-process",
            "ready session=\(configuredSession.id.uuidString) reason=\(reason)"
        )
        onStartupSucceeded?()
        if pendingFocusRequest {
            pendingFocusRequest = false
            focusTerminal()
        }
    }

    private func handleProcessTermination(exitCode: Int32?) {
        startupWatchdogWorkItem?.cancel()
        startupWatchdogWorkItem = nil
        if hasReceivedInitialOutput == false {
            hasReceivedInitialOutput = true
            updateLoadingShieldVisibility()
            reportStartupFailureIfNeeded(
                "process exited before terminal became ready (exit=\(exitCode.map(String.init) ?? "nil"))"
            )
        }
        pendingFocusRequest = false
    }

    private func reportStartupFailureIfNeeded(_ detail: String) {
        guard hasReportedStartupFailure == false else {
            return
        }
        hasReportedStartupFailure = true
        logger.log(
            "terminal-recovery",
            "ghostty-container-failed session=\(configuredSession.id.uuidString) detail=\(detail)"
        )
        onStartupFailure?(detail)
    }

    private func updateLoadingShieldVisibility() {
        loadingShieldView.isHidden = hasReceivedInitialOutput
        terminalView.alphaValue = hasReceivedInitialOutput ? 1 : 0
        if hasReceivedInitialOutput == false,
           let responder = window?.firstResponder,
           ownsResponder(responder) {
            window?.makeFirstResponder(nil)
        }
    }

    private func notifyInteraction() {
        onInteraction?()
    }

    private func tearDownTerminalView() {
        if window?.firstResponder === terminalView {
            window?.makeFirstResponder(nil)
        }
        terminalView.delegate = nil
        terminalView.controller = nil
    }

    func prepareForPointerInteraction() {
        guard isReadyForInteraction else {
            pendingFocusRequest = true
            return
        }
        notifyInteraction()
        focusTerminal()
    }

    private func scheduleGeometryReconcile(passCount: Int = 1) {
        pendingGeometryReconcilePasses = max(pendingGeometryReconcilePasses, max(1, passCount))
        guard geometryReconcileScheduled == false else {
            return
        }

        geometryReconcileGeneration &+= 1
        let generation = geometryReconcileGeneration
        geometryReconcileScheduled = true
        DispatchQueue.main.async { [weak self] in
            self?.runGeometryReconcilePass(generation: generation)
        }
    }

    private func runGeometryReconcilePass(generation: UInt64) {
        guard generation == geometryReconcileGeneration else {
            geometryReconcileScheduled = false
            return
        }

        let remainingPasses = max(1, pendingGeometryReconcilePasses)
        pendingGeometryReconcilePasses = 0

        guard window != nil, bounds.width > 0, bounds.height > 0 else {
            if remainingPasses > 1 {
                pendingGeometryReconcilePasses = max(pendingGeometryReconcilePasses, remainingPasses - 1)
                DispatchQueue.main.async { [weak self] in
                    self?.runGeometryReconcilePass(generation: generation)
                }
            } else {
                geometryReconcileScheduled = false
            }
            return
        }

        layoutSubtreeIfNeeded()
        terminalView.layoutSubtreeIfNeeded()
        _ = terminalView.reconcileGeometryNow()
        alignTerminalSurfaceGeometry()
        terminalView.fitToSize()

        let nextPendingPasses = max(pendingGeometryReconcilePasses, remainingPasses - 1)
        if nextPendingPasses > 0 {
            pendingGeometryReconcilePasses = nextPendingPasses
            DispatchQueue.main.async { [weak self] in
                self?.runGeometryReconcilePass(generation: generation)
            }
        } else {
            geometryReconcileScheduled = false
        }
    }

    private func alignTerminalSurfaceGeometry() {
        guard bounds.width > 0, bounds.height > 0 else {
            return
        }

        CATransaction.begin()
        CATransaction.setDisableActions(true)

        if terminalView.frame != bounds {
            terminalView.frame = bounds
        }
        CATransaction.commit()
    }

    private func configureTerminalView(_ view: AppTerminalView) {
        view.translatesAutoresizingMaskIntoConstraints = false
        view.delegate = self
        view.controller = controller
        view.configuration = surfaceOptions()
    }

    private func pinTerminalView(_ view: AppTerminalView) {
        NSLayoutConstraint.activate([
            view.leadingAnchor.constraint(equalTo: leadingAnchor),
            view.trailingAnchor.constraint(equalTo: trailingAnchor),
            view.topAnchor.constraint(equalTo: topAnchor),
            view.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    private func pinLoadingShieldView() {
        NSLayoutConstraint.activate([
            loadingShieldView.leadingAnchor.constraint(equalTo: leadingAnchor),
            loadingShieldView.trailingAnchor.constraint(equalTo: trailingAnchor),
            loadingShieldView.topAnchor.constraint(equalTo: topAnchor),
            loadingShieldView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    private func pinInactiveOverlayView() {
        NSLayoutConstraint.activate([
            inactiveOverlayView.leadingAnchor.constraint(equalTo: leadingAnchor),
            inactiveOverlayView.trailingAnchor.constraint(equalTo: trailingAnchor),
            inactiveOverlayView.topAnchor.constraint(equalTo: topAnchor),
            inactiveOverlayView.bottomAnchor.constraint(equalTo: bottomAnchor),
        ])
    }

    private static func makeController(
        backgroundPreset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset,
        logger: AppDebugLog
    ) -> TerminalController {
        let resolvedConfig = GhosttyEmbeddedConfig.resolvedControllerConfig()
        logger.log(
            "ghostty-config",
            "source=\(resolvedConfig.prefersUserConfig ? "user" : "embedded") path=\(resolvedConfig.userConfigDescription ?? Self.configSourceDescription(resolvedConfig.configSource))"
        )
        return TerminalController(
            configSource: resolvedConfig.configSource,
            theme: Self.theme(for: backgroundPreset, backgroundColorPreset: backgroundColorPreset),
            terminalConfiguration: GhosttyEmbeddedConfig.terminalConfiguration()
        )
    }

    private static func resolvedAppearance(
        for preset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset
    ) -> AppEffectiveTerminalAppearance {
        let automaticAppearance = GhosttyEmbeddedConfig.resolvedAutomaticTerminalAppearance(
            prefersDarkAppearance: true
        )
        return preset.effectiveAppearance(
            backgroundColorPreset: backgroundColorPreset,
            automaticAppearance: automaticAppearance
        )
    }

    private static func theme(for preset: AppTerminalBackgroundPreset, backgroundColorPreset: AppBackgroundColorPreset) -> TerminalTheme {
        let appearance = resolvedAppearance(
            for: preset,
            backgroundColorPreset: backgroundColorPreset
        )
        return theme(from: appearance)
    }

    private static func theme(from appearance: AppEffectiveTerminalAppearance) -> TerminalTheme {
        let configuration = TerminalConfiguration { builder in
            builder.withBackground(appearance.backgroundColor.ghosttyHexString)
            builder.withForeground(appearance.foregroundColor.ghosttyHexString)
            builder.withSelectionBackground(appearance.selectionBackgroundColor.ghosttyHexString)
            builder.withSelectionForeground(appearance.selectionForegroundColor.ghosttyHexString)
            builder.withCursorColor(appearance.cursorColor.ghosttyHexString)
            builder.withCursorText(appearance.cursorTextColor.ghosttyHexString)
            builder.withBoldColor(appearance.foregroundColor.ghosttyHexString)
            builder.withMinimumContrast(appearance.minimumContrast)
            builder.withBackgroundOpacity(1.0)

            for (index, color) in appearance.paletteHexStrings.enumerated() {
                builder.withPalette(index, color: color)
            }
        }
        return TerminalTheme(light: configuration, dark: configuration)
    }

    private static func inactiveOverlayColor(for preset: AppTerminalBackgroundPreset, backgroundColorPreset: AppBackgroundColorPreset) -> NSColor {
        resolvedAppearance(
            for: preset,
            backgroundColorPreset: backgroundColorPreset
        ).inactiveDimColor
    }

    private func applyEffectiveBackgroundColor() {
        let backgroundColor = effectiveBackgroundColor()
        layer?.backgroundColor = backgroundColor.cgColor
        loadingShieldView.layer?.backgroundColor = backgroundColor.cgColor
    }

    private func effectiveBackgroundColor() -> NSColor {
        return Self.resolvedAppearance(
            for: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset
        ).backgroundColor
    }

    private func setInactiveOverlay(visible: Bool) {
        inactiveOverlayView.isHidden = !visible
    }

    private func surfaceOptions() -> TerminalSurfaceOptions {
        TerminalSurfaceOptions(
            backend: .inMemory(processBridge.terminalSession),
            fontSize: Float(terminalFontSize),
            workingDirectory: configuredSession.cwd,
            context: .window
        )
    }

    private static func configSourceDescription(_ source: TerminalController.ConfigSource) -> String {
        switch source {
        case .none:
            return "none"
        case let .file(path):
            return path
        case let .generated(contents):
            return "generated:\(contents.count)"
        }
    }
}

@MainActor
final class GhosttyTerminalRegistry {
    static let shared = GhosttyTerminalRegistry()

    private var containers: [UUID: GhosttyTerminalContainerView] = [:]
    private var explicitFocusedSessionID: UUID?

    func containerView(
        for session: TerminalSession,
        environment: [(String, String)],
        terminalBackgroundPreset: AppTerminalBackgroundPreset,
        backgroundColorPreset: AppBackgroundColorPreset,
        terminalFontSize: Int,
        isFocused: Bool,
        isVisible: Bool,
        showsInactiveOverlay: Bool,
        onInteraction: (() -> Void)?,
        onFocusConsumed: (() -> Void)?,
        onStartupSucceeded: (() -> Void)?,
        onStartupFailure: ((String) -> Void)?
    ) -> GhosttyTerminalContainerView {
        if let existing = containers[session.id] {
            existing.updateSession(
                session,
                environment: environment,
                terminalBackgroundPreset: terminalBackgroundPreset,
                backgroundColorPreset: backgroundColorPreset,
                terminalFontSize: terminalFontSize,
                isFocused: isFocused,
                isVisible: isVisible,
                showsInactiveOverlay: showsInactiveOverlay,
                onInteraction: onInteraction,
                onFocusConsumed: onFocusConsumed,
                onStartupSucceeded: onStartupSucceeded,
                onStartupFailure: onStartupFailure
            )
            return existing
        }

        let created = GhosttyTerminalContainerView(
            session: session,
            environment: environment,
            terminalFontSize: terminalFontSize,
            onInteraction: onInteraction,
            onFocusConsumed: onFocusConsumed,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        created.updateSession(
            session,
            environment: environment,
            terminalBackgroundPreset: terminalBackgroundPreset,
            backgroundColorPreset: backgroundColorPreset,
            terminalFontSize: terminalFontSize,
            isFocused: isFocused,
            isVisible: isVisible,
            showsInactiveOverlay: showsInactiveOverlay,
            onInteraction: onInteraction,
            onFocusConsumed: onFocusConsumed,
            onStartupSucceeded: onStartupSucceeded,
            onStartupFailure: onStartupFailure
        )
        containers[session.id] = created
        return created
    }

    func release(sessionID: UUID) {
        guard let container = containers.removeValue(forKey: sessionID) else {
            AppDebugLog.shared.log(
                "ghostty-lifecycle",
                "release-miss session=\(sessionID.uuidString)"
            )
            return
        }
        AppDebugLog.shared.log(
            "ghostty-lifecycle",
            "release session=\(sessionID.uuidString) remaining=\(containers.count)"
        )
        GhosttyTerminalPortalRegistry.detach(hostedView: container)
        container.prepareForPermanentRemoval()
        DmuxTerminalOutputEventEmitter.shared.clear(sessionID: sessionID)
        container.removeFromSuperviewWithoutNeedingDisplay()
        if explicitFocusedSessionID == sessionID {
            explicitFocusedSessionID = nil
            NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: sessionID)
        }
    }

    func beginStructuralResizeTransition(for sessionIDs: [UUID]) {
        for sessionID in sessionIDs {
            containers[sessionID]?.beginStructuralResizeTransition()
        }
    }

    func reconcileGeometry(for sessionIDs: [UUID]) {
        for sessionID in sessionIDs {
            containers[sessionID]?.reconcileGeometry()
        }
    }

    func terminateAll() {
        let sessionIDs = Array(containers.keys)
        for sessionID in sessionIDs {
            release(sessionID: sessionID)
        }
    }

    func shellPID(for sessionID: UUID) -> Int32? {
        containers[sessionID]?.terminalShellPID
    }

    func projectID(for sessionID: UUID) -> UUID? {
        containers[sessionID]?.terminalProjectID
    }

    func sessionInstanceID(for sessionID: UUID) -> String? {
        containers[sessionID]?.terminalProcessInstanceID
    }

    func sendText(_ text: String, to sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.sendText(text)
        return true
    }

    func sendInterrupt(to sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.sendInterrupt()
        return true
    }

    func sendEscape(to sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.sendEscape()
        return true
    }

    @discardableResult
    func focus(sessionID: UUID) -> Bool {
        guard let container = containers[sessionID] else {
            return false
        }
        container.focusTerminal()
        return true
    }

    func sendEditingShortcut(_ shortcut: TerminalEditingShortcut, responder: NSResponder?) -> Bool {
        if let responder,
           let container = containers.values.first(where: { $0.ownsResponder(responder) }) {
            container.sendEditingShortcut(shortcut)
            return true
        }
        if let sessionID = focusedSessionID(),
           let container = containers[sessionID] {
            container.sendEditingShortcut(shortcut)
            return true
        }
        return false
    }

    func sendNativeCommandArrow(keyCode: UInt16, responder: NSResponder?) -> Bool {
        if let responder,
           let container = containers.values.first(where: { $0.ownsResponder(responder) }) {
            return container.sendNativeCommandArrow(keyCode: keyCode)
        }
        if let sessionID = focusedSessionID(),
           let container = containers[sessionID] {
            return container.sendNativeCommandArrow(keyCode: keyCode)
        }
        return false
    }

    func forwardKeyDown(_ event: NSEvent, responder: NSResponder?) -> Bool {
        if let responder,
           let container = containers.values.first(where: { $0.ownsResponder(responder) }) {
            return container.forwardKeyDown(event)
        }
        if let sessionID = focusedSessionID(),
           let container = containers[sessionID] {
            return container.forwardKeyDown(event)
        }
        return false
    }

    func focusedSessionID() -> UUID? {
        if let explicitFocusedSessionID, containers[explicitFocusedSessionID] != nil {
            return explicitFocusedSessionID
        }
        return containers.first(where: { $0.value.isTerminalFocused })?.key
    }

    func markFocused(sessionID: UUID) {
        guard containers[sessionID] != nil else {
            return
        }
        guard explicitFocusedSessionID != sessionID else {
            return
        }
        explicitFocusedSessionID = sessionID
        NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: sessionID)
    }

    func clearFocusedSession() {
        guard explicitFocusedSessionID != nil else {
            return
        }
        explicitFocusedSessionID = nil
        NotificationCenter.default.post(name: .dmuxTerminalFocusDidChange, object: nil)
    }

    func ownsResponder(_ responder: NSResponder?) -> Bool {
        containers.values.contains { $0.ownsResponder(responder) }
    }

    func debugSnapshot() -> String {
        containers
            .map { sessionID, container in
                let shellPID = container.terminalShellPID.map(String.init) ?? "nil"
                let focused = container.isTerminalFocused ? "focused" : "blurred"
                return "\(sessionID.uuidString):\(shellPID):\(focused)"
            }
            .sorted()
            .joined(separator: ", ")
    }
}

extension GhosttyTerminalRegistry: DmuxTerminalBackendRegistry {}

private extension Array where Element == String {
    func withCStringArray<T>(_ body: ([UnsafeMutablePointer<CChar>?]) -> T) -> T {
        let allocated = map { strdup($0) }
        defer {
            for ptr in allocated {
                free(ptr)
            }
        }
        return body(allocated + [nil])
    }
}
