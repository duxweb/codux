import SwiftUI
import AppKit
import Observation
import UserNotifications

enum TerminalKeyRoutingPolicy {
    static func shouldRouteToTerminal(
        isMainMenuShortcut: Bool,
        isReservedApplicationShortcut: Bool
    ) -> Bool {
        !isMainMenuShortcut && !isReservedApplicationShortcut
    }
}

enum AppWindowIdentifier {
    static let main = NSUserInterfaceItemIdentifier("dmux.main")
    static let settings = NSUserInterfaceItemIdentifier("dmux.settings")
    static let about = NSUserInterfaceItemIdentifier("dmux.about")
    static let agreement = NSUserInterfaceItemIdentifier("dmux.agreement")
    static let petDex = NSUserInterfaceItemIdentifier("dmux.petDex")
    static let desktopPet = NSUserInterfaceItemIdentifier("dmux.desktopPet")
    static let memoryManager = NSUserInterfaceItemIdentifier("dmux.memoryManager")
    static let detachedTerminalPrefix = "dmux.detached-terminal."

    static func detachedTerminal(_ sessionID: UUID) -> NSUserInterfaceItemIdentifier {
        NSUserInterfaceItemIdentifier("\(detachedTerminalPrefix)\(sessionID.uuidString)")
    }
}

@MainActor
func applyStandardWindowChrome(
    _ window: NSWindow,
    title: String? = nil,
    toolbarStyle: NSWindow.ToolbarStyle = .automatic,
    backgroundColor: NSColor = .windowBackgroundColor
) {
    if let title {
        window.title = title
    }
    window.styleMask.remove(.fullSizeContentView)
    window.titleVisibility = .visible
    window.titlebarAppearsTransparent = false
    window.isMovableByWindowBackground = false
    window.backgroundColor = backgroundColor
    if toolbarStyle != .preference {
        window.toolbar = nil
    }
    if #available(macOS 13.0, *) {
        window.toolbarStyle = toolbarStyle
    }
}

@MainActor
func isStandardChromeWindow(_ window: NSWindow) -> Bool {
    if let id = window.identifier?.rawValue {
        if id == AppWindowIdentifier.settings.rawValue
            || id == AppWindowIdentifier.about.rawValue
            || id == AppWindowIdentifier.agreement.rawValue
            || id == AppWindowIdentifier.petDex.rawValue
            || id == AppWindowIdentifier.memoryManager.rawValue {
            return true
        }
        if id.contains("Settings") || id.contains("settings") {
            return true
        }
    }
    return false
}

@MainActor
func isMainWorkspaceWindow(_ window: NSWindow) -> Bool {
    window.identifier == AppWindowIdentifier.main
}

@MainActor
func isDetachedTerminalWindow(_ window: NSWindow) -> Bool {
    window.identifier?.rawValue.hasPrefix(AppWindowIdentifier.detachedTerminalPrefix) == true
}

@MainActor
func applyImmersiveWindowChrome(_ window: NSWindow) {
    window.styleMask.insert(.fullSizeContentView)
    window.titleVisibility = .hidden
    window.titlebarAppearsTransparent = true
    window.isMovableByWindowBackground = false
    window.backgroundColor = .clear
    window.toolbar = nil
}

struct DmuxWorkspaceApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = AppModel.bootstrap()

    var body: some Scene {
        let _ = appDelegate.configure(model: model)

        WindowGroup {
            RootView(model: model)
        }
        .defaultSize(width: 1460, height: 920)
        .windowStyle(.hiddenTitleBar)
        .commands {
            AppCommands(model: model)
        }

        Settings {
            SettingsView(model: model)
        }
        .windowResizability(.contentSize)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    @MainActor
    private static var isRelaunching = false
    private var trafficLightBaseY: [ObjectIdentifier: CGFloat] = [:]
    private weak var model: AppModel?
    private var localKeyMonitor: Any?

    @MainActor
    static func scheduleRelaunch(at url: URL) {
        guard !isRelaunching else { return }
        isRelaunching = true

        let task = Process()
        task.executableURL = URL(fileURLWithPath: "/bin/sh")
        task.arguments = ["-c", "sleep 1 && /usr/bin/open -n -- \"$RELAUNCH_PATH\""]
        task.environment = ["RELAUNCH_PATH": url.path]

        do {
            try task.run()
            NSApp.terminate(nil)
        } catch {
            isRelaunching = false
        }
    }

    @MainActor
    func configure(model: AppModel) {
        self.model = model
        installLocalKeyMonitorIfNeeded()
    }

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.regular)
        NSApp.activate(ignoringOtherApps: true)
        NSWindow.allowsAutomaticWindowTabbing = false
        UNUserNotificationCenter.current().delegate = self

        NotificationCenter.default.addObserver(
            self,
            selector: #selector(windowDidResize(_:)),
            name: NSWindow.didResizeNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(windowDidBecomeKey(_:)),
            name: NSWindow.didBecomeKeyNotification,
            object: nil
        )
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(windowDidDeminiaturize(_:)),
            name: NSWindow.didDeminiaturizeNotification,
            object: nil
        )

        for window in NSApp.windows {
            configure(window)
        }

        Task { @MainActor [weak self] in
            self?.model?.reconcileManagedAIProcessState(reason: "did-finish-launching")
            self?.model?.presentStartupRecoveryIfNeeded()
        }
    }

    func applicationShouldOpenUntitledFile(_ sender: NSApplication) -> Bool {
        false
    }

    func applicationShouldSaveApplicationState(_ app: NSApplication) -> Bool {
        false
    }

    func applicationShouldRestoreApplicationState(_ app: NSApplication) -> Bool {
        false
    }

    func applicationSupportsSecureRestorableState(_ app: NSApplication) -> Bool {
        false
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        willPresent notification: UNNotification
    ) async -> UNNotificationPresentationOptions {
        if #available(macOS 11.0, *) {
            return [.banner, .list, .sound, .badge]
        }
        return [.sound, .badge]
    }

    func userNotificationCenter(
        _ center: UNUserNotificationCenter,
        didReceive response: UNNotificationResponse
    ) async {
    }

    @MainActor
    private func installLocalKeyMonitorIfNeeded() {
        guard localKeyMonitor == nil else {
            return
        }
        localKeyMonitor = NSEvent.addLocalMonitorForEvents(matching: .keyDown) { [weak self] event in
            guard let self else {
                return event
            }
            return self.handleLocalKeyDown(event)
        }
    }

    @MainActor
    private func handleLocalKeyDown(_ event: NSEvent) -> NSEvent? {
        let modifiers = normalizedShortcutModifiers(for: event)
        let mainMenuHandled = handleMainMenuShortcutIfNeeded(event, modifiers: modifiers)
        if mainMenuHandled {
            return nil
        }

        let reservedApplicationShortcut = isReservedApplicationShortcut(event, modifiers: modifiers)
        if reservedApplicationShortcut {
            return event
        }

        guard modifiers == [.command],
              event.charactersIgnoringModifiers?.lowercased() == "w" else {
            if routeKeyDownToTerminalIfNeeded(
                event,
                modifiers: modifiers,
                isMainMenuShortcut: mainMenuHandled,
                isReservedApplicationShortcut: reservedApplicationShortcut
            ) {
                return nil
            }
            return event
        }

        guard let window = NSApp.keyWindow ?? NSApp.mainWindow else {
            return event
        }

        if isStandardChromeWindow(window) || isDetachedTerminalWindow(window) {
            window.performClose(nil)
            return nil
        }

        guard let model, model.selectedSessionID != nil else {
            return event
        }

        model.confirmCloseSelectedSession()
        return nil
    }

    @MainActor
    private func handleMainMenuShortcutIfNeeded(_ event: NSEvent, modifiers: NSEvent.ModifierFlags) -> Bool {
        guard modifiers.contains(.command) else {
            return false
        }
        guard let mainMenu = NSApp.mainMenu else {
            return false
        }
        return mainMenu.performKeyEquivalent(with: event)
    }

    @MainActor
    private func normalizedShortcutModifiers(for event: NSEvent) -> NSEvent.ModifierFlags {
        var modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        modifiers.remove(.numericPad)
        modifiers.remove(.function)
        return modifiers
    }

    @MainActor
    private func routeKeyDownToTerminalIfNeeded(
        _ event: NSEvent,
        modifiers: NSEvent.ModifierFlags,
        isMainMenuShortcut: Bool,
        isReservedApplicationShortcut: Bool
    ) -> Bool {
        guard TerminalKeyRoutingPolicy.shouldRouteToTerminal(
            isMainMenuShortcut: isMainMenuShortcut,
            isReservedApplicationShortcut: isReservedApplicationShortcut
        ) else {
            return false
        }

        guard let window = NSApp.keyWindow ?? NSApp.mainWindow,
              !isStandardChromeWindow(window) else {
            return false
        }

        let responder = window.firstResponder
        if responder is NSTextView,
           DmuxTerminalBackend.shared.registry.ownsResponder(responder) == false {
            return false
        }

        return DmuxTerminalBackend.shared.registry.forwardKeyDown(event, responder: responder)
    }

    @MainActor
    private func isReservedApplicationShortcut(_ event: NSEvent, modifiers: NSEvent.ModifierFlags) -> Bool {
        modifiers == [.command] && event.charactersIgnoringModifiers?.lowercased() == ","
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        for window in NSApp.windows {
            configure(window)
        }
        Task { @MainActor [weak self] in
            self?.model?.reconcileManagedAIProcessState(reason: "did-become-active")
            self?.model?.presentStartupRecoveryIfNeeded()
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        AppDebugLog.shared.log("ghostty-lifecycle", "application-will-terminate")
        SleepPreventionService.shared.stop()
        DmuxTerminalBackend.shared.registry.terminateAll()
    }

    @objc
    @MainActor
    private func windowDidResize(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }

        guard !isStandardChromeWindow(window),
              !isDetachedTerminalWindow(window) else {
            return
        }

        repositionTrafficLights(in: window)
    }

    @objc
    @MainActor
    private func windowDidBecomeKey(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }
        restoreTerminalFocusIfNeeded(for: window)
    }

    @objc
    @MainActor
    private func windowDidDeminiaturize(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }
        restoreTerminalFocusIfNeeded(for: window)
    }

    @MainActor
    private func configure(_ window: NSWindow) {
        guard !(window is NSPanel) else {
            return
        }

        window.isRestorable = false

        if isStandardChromeWindow(window) {
            applyStandardWindowChrome(window, toolbarStyle: .preference)
            return
        }

        if isDetachedTerminalWindow(window) {
            applyStandardWindowChrome(window)
            return
        }

        guard isMainWorkspaceWindow(window) else {
            return
        }

        applyImmersiveWindowChrome(window)

        repositionTrafficLights(in: window)
    }

    @MainActor
    private func repositionTrafficLights(in window: NSWindow) {
        guard !(window is NSPanel) else {
            return
        }

        guard let closeButton = window.standardWindowButton(.closeButton),
              let miniaturizeButton = window.standardWindowButton(.miniaturizeButton),
              let zoomButton = window.standardWindowButton(.zoomButton) else {
            return
        }

        let buttons = [closeButton, miniaturizeButton, zoomButton]
        let downwardOffset: CGFloat = 5
        let windowID = ObjectIdentifier(window)

        let baseY: CGFloat
        if let storedBaseY = trafficLightBaseY[windowID] {
            baseY = storedBaseY
        } else {
            baseY = closeButton.frame.origin.y
            trafficLightBaseY[windowID] = baseY
        }

        for button in buttons {
            var frame = button.frame
            frame.origin.y = baseY - downwardOffset
            button.setFrameOrigin(frame.origin)
        }
    }

    @MainActor
    private func restoreTerminalFocusIfNeeded(for window: NSWindow) {
        guard !(window is NSPanel),
              !isStandardChromeWindow(window),
              !isDetachedTerminalWindow(window) else {
            return
        }
        model?.restoreSelectedTerminalFocusIfNeeded()
    }

    deinit {
        if let localKeyMonitor {
            NSEvent.removeMonitor(localKeyMonitor)
        }
        NotificationCenter.default.removeObserver(self)
    }
}
