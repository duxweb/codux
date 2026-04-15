import SwiftUI
import AppKit
import Observation
import UserNotifications

enum AppWindowIdentifier {
    static let main = NSUserInterfaceItemIdentifier("dmux.main")
    static let settings = NSUserInterfaceItemIdentifier("dmux.settings")
    static let about = NSUserInterfaceItemIdentifier("dmux.about")
    static let agreement = NSUserInterfaceItemIdentifier("dmux.agreement")
}

@MainActor
func applyStandardWindowChrome(_ window: NSWindow, title: String? = nil, toolbarStyle: NSWindow.ToolbarStyle = .automatic) {
    if let title {
        window.title = title
    }
    window.styleMask.remove(.fullSizeContentView)
    window.titleVisibility = .visible
    window.titlebarAppearsTransparent = false
    window.isMovableByWindowBackground = false
    window.backgroundColor = NSColor.windowBackgroundColor
    window.toolbar = nil
    if #available(macOS 13.0, *) {
        window.toolbarStyle = toolbarStyle
    }
}

@MainActor
func isStandardChromeWindow(_ window: NSWindow) -> Bool {
    if let id = window.identifier?.rawValue {
        if id == AppWindowIdentifier.settings.rawValue
            || id == AppWindowIdentifier.about.rawValue
            || id == AppWindowIdentifier.agreement.rawValue {
            return true
        }
        if id.contains("Settings") || id.contains("settings") {
            return true
        }
    }
    return false
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

@main
struct DmuxWorkspaceApp: App {
    @NSApplicationDelegateAdaptor(AppDelegate.self) private var appDelegate
    @State private var model = AppModel.bootstrap()

    var body: some Scene {
        let _ = appDelegate.configure(model: model)
        let languageToken = "\(model.appSettings.language.rawValue)-\(model.appSettings.language.resolved.rawValue)"
        let appLocale = Locale(identifier: model.appSettings.language.localeIdentifier)

        WindowGroup {
            RootView(model: model)
                .id(languageToken)
                .environment(\.locale, appLocale)
                .preferredColorScheme(model.appSettings.themeMode.colorScheme)
        }
        .defaultSize(width: 1460, height: 920)
        .windowStyle(.hiddenTitleBar)
        .commands {
            let _ = languageToken
            AppCommands(model: model)
        }

        Settings {
            SettingsView(model: model)
                .id(languageToken)
                .environment(\.locale, appLocale)
                .preferredColorScheme(model.appSettings.themeMode.colorScheme)
        }
        .windowResizability(.contentSize)
    }
}

final class AppDelegate: NSObject, NSApplicationDelegate, UNUserNotificationCenterDelegate {
    private var trafficLightBaseY: [ObjectIdentifier: CGFloat] = [:]
    private weak var model: AppModel?
    private var localKeyMonitor: Any?

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

        for window in NSApp.windows {
            configure(window)
        }
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
        let modifiers = event.modifierFlags.intersection(.deviceIndependentFlagsMask)
        guard modifiers == [.command],
              event.charactersIgnoringModifiers?.lowercased() == "w" else {
            return event
        }

        guard let window = NSApp.keyWindow ?? NSApp.mainWindow else {
            return event
        }

        if isStandardChromeWindow(window) {
            window.performClose(nil)
            return nil
        }

        guard let model, model.selectedSessionID != nil else {
            return event
        }

        model.confirmCloseSelectedSession()
        return nil
    }

    func applicationDidBecomeActive(_ notification: Notification) {
        for window in NSApp.windows {
            configure(window)
        }
    }

    func applicationWillTerminate(_ notification: Notification) {
        SwiftTermTerminalRegistry.shared.terminateAll()
    }

    @objc
    @MainActor
    private func windowDidResize(_ notification: Notification) {
        guard let window = notification.object as? NSWindow else {
            return
        }

        guard !isStandardChromeWindow(window) else {
            return
        }

        repositionTrafficLights(in: window)
    }

    @MainActor
    private func configure(_ window: NSWindow) {
        guard !(window is NSPanel) else {
            return
        }

        if isStandardChromeWindow(window) {
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

    deinit {
        if let localKeyMonitor {
            NSEvent.removeMonitor(localKeyMonitor)
        }
        NotificationCenter.default.removeObserver(self)
    }
}
