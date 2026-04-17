import AppKit
import SwiftUI

struct AppCommands: Commands {
    let model: AppModel

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button(String(localized: "menu.file.new_project", defaultValue: "New Project", bundle: .module)) {
                model.addProject()
            }
            .keyboardShortcut("n", modifiers: [.command])

            Button(String(localized: "menu.file.open_folder", defaultValue: "Open Folder…", bundle: .module)) {
                model.openProjectFolder()
            }
            .keyboardShortcut("o", modifiers: [.command])

            Divider()

            Button(closeCommandTitle) {
                handleCloseCommand()
            }
            .disabled(!canHandleCloseCommand)
            .keyboardShortcut("w", modifiers: [.command])

            Button(String(localized: "menu.file.close_current_project", defaultValue: "Close Current Project", bundle: .module)) {
                model.closeCurrentProject()
            }
            .disabled(model.selectedProject == nil)

            Button(String(localized: "menu.file.close_all_projects", defaultValue: "Close All Projects…", bundle: .module)) {
                model.closeAllProjects()
            }
            .disabled(model.projects.isEmpty)
        }

        CommandGroup(replacing: .saveItem) {}
        CommandGroup(replacing: .importExport) {}
        CommandGroup(replacing: .toolbar) {}
        CommandGroup(replacing: .windowArrangement) {}

        CommandGroup(replacing: .appInfo) {
            Button(String(format: String(localized: "menu.app.about_format", defaultValue: "About %@", bundle: .module), model.appDisplayName)) {
                AboutWindowPresenter.show(model: model)
            }

            Divider()

            Button(model.isCheckingForUpdates ? String(localized: "about.checking_updates", defaultValue: "Checking...", bundle: .module) : String(localized: "about.updates", defaultValue: "Updates", bundle: .module)) {
                model.checkForUpdates()
            }
            .disabled(model.isCheckingForUpdates)
        }

        CommandGroup(replacing: .help) {
            Button(String(localized: "menu.help.export_diagnostics", defaultValue: "Export Diagnostics…", bundle: .module)) {
                model.exportDiagnosticsArchive()
            }

            Divider()

            Button(String(localized: "menu.help.github", defaultValue: "GitHub", bundle: .module)) {
                model.openURL(AppSupportLinks.github)
            }

            Button(String(localized: "menu.help.github_issue", defaultValue: "GitHub Issue", bundle: .module)) {
                model.openURL(AppSupportLinks.issues)
            }

            Button(String(localized: "menu.help.website", defaultValue: "Official Website", bundle: .module)) {
                model.openURL(AppSupportLinks.website)
            }
        }

        CommandGroup(after: .sidebar) {
            ShortcutCommandButton(
                title: String(localized: "menu.view.create_split", defaultValue: "Create Split", bundle: .module),
                shortcut: model.appSettings.shortcuts.splitPane
            ) {
                model.splitSelectedPane(axis: .horizontal)
            }

            ShortcutCommandButton(
                title: String(localized: "menu.view.create_tab", defaultValue: "Create Tab", bundle: .module),
                shortcut: model.appSettings.shortcuts.createTab
            ) {
                model.createBottomTab()
            }

            Divider()

            ShortcutCommandButton(
                title: String(localized: "menu.view.open_git_panel", defaultValue: "Open Git Panel", bundle: .module),
                shortcut: model.appSettings.shortcuts.toggleGitPanel
            ) {
                model.toggleRightPanel(.git)
            }

            ShortcutCommandButton(
                title: String(localized: "menu.view.open_ai_panel", defaultValue: "Open AI Panel", bundle: .module),
                shortcut: model.appSettings.shortcuts.toggleAIPanel
            ) {
                model.toggleRightPanel(.aiStats)
            }

            Divider()

            Button(String(localized: "menu.view.toggle_full_screen", defaultValue: "Toggle Full Screen", bundle: .module)) {
                (NSApp.keyWindow ?? NSApp.mainWindow)?.toggleFullScreen(nil)
            }
            .keyboardShortcut("f", modifiers: [.command, .option])

            Divider()

            WorkspaceSwitchCommandButton(model: model, index: 0)
            WorkspaceSwitchCommandButton(model: model, index: 1)
            WorkspaceSwitchCommandButton(model: model, index: 2)
            WorkspaceSwitchCommandButton(model: model, index: 3)
            WorkspaceSwitchCommandButton(model: model, index: 4)
            WorkspaceSwitchCommandButton(model: model, index: 5)
            WorkspaceSwitchCommandButton(model: model, index: 6)
            WorkspaceSwitchCommandButton(model: model, index: 7)
            WorkspaceSwitchCommandButton(model: model, index: 8)
        }

    }

    private var activeWindow: NSWindow? {
        NSApp.keyWindow ?? NSApp.mainWindow
    }

    private var isClosingStandardWindow: Bool {
        guard let activeWindow else {
            return false
        }
        return isStandardChromeWindow(activeWindow)
    }

    private var canHandleCloseCommand: Bool {
        if isClosingStandardWindow {
            return activeWindow?.styleMask.contains(.closable) ?? true
        }
        return activeWindow != nil
    }

    private var closeCommandTitle: String {
        if isClosingStandardWindow {
            return String(localized: "menu.file.close_window", defaultValue: "Close Window", bundle: .module)
        }
        return String(localized: "menu.file.close_current_split", defaultValue: "Close Current Split", bundle: .module)
    }

    private func handleCloseCommand() {
        if isClosingStandardWindow {
            activeWindow?.performClose(nil)
            return
        }
        guard focusedTerminalSessionID != nil else {
            return
        }
        model.confirmCloseSelectedSession()
    }

    private var focusedTerminalSessionID: UUID? {
        guard let focusedSessionID = SwiftTermTerminalRegistry.shared.focusedSessionID(),
              model.selectedSessionID == focusedSessionID else {
            return nil
        }
        return focusedSessionID
    }
}

private struct ShortcutCommandButton: View {
    let title: String
    let shortcut: AppKeyboardShortcut?
    let action: () -> Void

    var body: some View {
        if let shortcut {
            Button(title, action: action)
                .keyboardShortcut(shortcut.keyEquivalent, modifiers: shortcut.eventModifiers)
        } else {
            Button(title, action: action)
        }
    }
}

private struct WorkspaceSwitchCommandButton: View {
    let model: AppModel
    let index: Int

    var body: some View {
        Button(String(format: String(localized: "menu.view.workspace_format", defaultValue: "Workspace %@", bundle: .module), "\(index + 1)")) {
            model.selectProject(atSidebarIndex: index)
        }
        .keyboardShortcut(KeyEquivalent(Character(String(index + 1))), modifiers: [.command])
        .disabled(!model.projects.indices.contains(index))
    }
}
