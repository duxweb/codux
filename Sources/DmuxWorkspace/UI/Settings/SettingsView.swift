import AppKit
import SwiftUI

enum SettingsSectionTab: String, CaseIterable, Identifiable {
    case general
    case appearance
    case pet
    case tools
    case notifications
    case shortcuts
    case developer

    var id: String { rawValue }

    var symbol: String {
        switch self {
        case .general: return "gearshape"
        case .appearance: return "paintbrush"
        case .pet: return "pawprint"
        case .tools: return "terminal"
        case .notifications: return "bell.badge"
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
        case .tools:
            return 360
        case .notifications:
            return 620
        case .shortcuts:
            return 320
        case .developer:
            return 220
        }
    }
}

struct SettingsView: View {
    let model: AppModel
    @State private var selectedTab: SettingsSectionTab = .general

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

            ToolSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.tools", defaultValue: "Tools", bundle: .module), systemImage: SettingsSectionTab.tools.symbol)
                }
                .tag(SettingsSectionTab.tools)

            NotificationSettingsPane(model: model)
                .tabItem {
                    Label(String(localized: "settings.tab.notifications", defaultValue: "Notifications", bundle: .module), systemImage: SettingsSectionTab.notifications.symbol)
                }
                .tag(SettingsSectionTab.notifications)

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
        .frame(width: 640, height: selectedTab.preferredContentHeight)
        .background(Color(nsColor: .windowBackgroundColor))
        .background(
            SettingsWindowConfigurator(
                title: String(localized: "menu.settings", defaultValue: "Settings", bundle: .module),
                contentSize: NSSize(width: 640, height: selectedTab.preferredContentHeight)
            )
        )
    }
}
