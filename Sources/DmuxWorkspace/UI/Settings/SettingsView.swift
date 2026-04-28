import AppKit
import SwiftUI

enum SettingsSectionTab: String, CaseIterable, Identifiable {
    case general
    case appearance
    case pet
    case ai
    case notifications
    case remote
    case shortcuts
    case developer

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .general: return "gearshape"
        case .appearance: return "paintbrush"
        case .pet: return "pawprint"
        case .ai: return "brain.head.profile"
        case .notifications: return "bell.badge"
        case .remote: return "iphone.radiowaves.left.and.right"
        case .shortcuts: return "keyboard"
        case .developer: return "wrench.and.screwdriver"
        }
    }

    var preferredContentHeight: CGFloat {
        switch self {
        case .general:
            return 430
        case .appearance:
            return 760
        case .pet:
            return 430
        case .ai:
            return 640
        case .notifications:
            return 620
        case .remote:
            return 640
        case .shortcuts:
            return 320
        case .developer:
            return 220
        }
    }
}

extension Notification.Name {
    static let coduxSettingsTabRequested = Notification.Name("coduxSettingsTabRequested")
}

@MainActor
enum SettingsNavigationRequest {
    private static var pendingTab: SettingsSectionTab?

    static func request(_ tab: SettingsSectionTab) {
        pendingTab = tab
        NotificationCenter.default.post(name: .coduxSettingsTabRequested, object: tab.rawValue)
    }

    static func consume() -> SettingsSectionTab? {
        defer { pendingTab = nil }
        return pendingTab
    }
}

struct SettingsView: View {
    let model: AppModel
    @State private var selectedTab: SettingsSectionTab
    @ObservedObject private var remoteHostService: RemoteHostService

    init(model: AppModel) {
        self.model = model
        self.remoteHostService = model.remoteHostService
        self._selectedTab = State(initialValue: SettingsNavigationRequest.consume() ?? .general)
    }

    private var currentContentHeight: CGFloat {
        if selectedTab == .remote {
            let devicesCount = remoteHostService.snapshot.devices.count
            let urlConfigured = !model.appSettings.remote.serverURL
                .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            // Server section + buttons row baseline.
            var height: CGFloat = 280
            if urlConfigured {
                if devicesCount == 0 {
                    // "no_devices" placeholder + section header.
                    height += 100
                } else {
                    // Devices section header + each row ~50pt, capped so the window stays sensible.
                    height += 60 + min(CGFloat(devicesCount), 6) * 52
                }
            } else {
                // Configure-hint section.
                height += 80
            }
            return height
        }
        return selectedTab.preferredContentHeight
    }

    var body: some View {
        TabView(selection: $selectedTab) {
            GeneralSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.general", defaultValue: "General", bundle: .module), systemImage: SettingsSectionTab.general.symbol)
                }
                .tag(SettingsSectionTab.general)

            AppearanceSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.appearance", defaultValue: "Appearance", bundle: .module), systemImage: SettingsSectionTab.appearance.symbol)
                }
                .tag(SettingsSectionTab.appearance)

            PetSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.pet", defaultValue: "Pet", bundle: .module), systemImage: SettingsSectionTab.pet.symbol)
                }
                .tag(SettingsSectionTab.pet)

            AISettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.ai", defaultValue: "AI", bundle: .module), systemImage: SettingsSectionTab.ai.symbol)
                }
                .tag(SettingsSectionTab.ai)

            NotificationSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.notifications", defaultValue: "Notifications", bundle: .module), systemImage: SettingsSectionTab.notifications.symbol)
                }
                .tag(SettingsSectionTab.notifications)

            RemoteSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.remote", defaultValue: "Remote", bundle: .module), systemImage: SettingsSectionTab.remote.symbol)
                }
                .tag(SettingsSectionTab.remote)

            ShortcutSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.shortcuts", defaultValue: "Shortcuts", bundle: .module), systemImage: SettingsSectionTab.shortcuts.symbol)
                }
                .tag(SettingsSectionTab.shortcuts)

            DeveloperSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.developer", defaultValue: "Developer", bundle: .module), systemImage: SettingsSectionTab.developer.symbol)
                }
                .tag(SettingsSectionTab.developer)
        }
        .frame(width: 640, height: currentContentHeight)
        .background(Color(nsColor: .windowBackgroundColor))
        .background(
            SettingsWindowConfigurator(
                title: String(localized: "menu.settings", defaultValue: "Settings", bundle: .module),
                contentSize: NSSize(width: 640, height: currentContentHeight)
            )
        )
        .onReceive(NotificationCenter.default.publisher(for: .coduxSettingsTabRequested)) { notification in
            guard let rawValue = notification.object as? String,
                  let tab = SettingsSectionTab(rawValue: rawValue)
            else { return }
            selectedTab = tab
        }
    }
}
