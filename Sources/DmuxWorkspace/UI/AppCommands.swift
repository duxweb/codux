import AppKit
import SwiftUI

struct AppCommands: Commands {
    let model: AppModel

    var body: some Commands {
        CommandGroup(replacing: .newItem) {
            Button(model.i18n("menu.file.new_project", fallback: "New Project")) {
                model.addProject()
            }
            .keyboardShortcut("n", modifiers: [.command])

            Button(model.i18n("menu.file.open_folder", fallback: "Open Folder…")) {
                model.openProjectFolder()
            }
            .keyboardShortcut("o", modifiers: [.command])

            Divider()

            Button(closeCommandTitle) {
                handleCloseCommand()
            }
            .disabled(!canHandleCloseCommand)
            .keyboardShortcut("w", modifiers: [.command])

            Button(model.i18n("menu.file.close_current_project", fallback: "Close Current Project")) {
                model.closeCurrentProject()
            }
            .disabled(model.selectedProject == nil)

            Button(model.i18n("menu.file.close_all_projects", fallback: "Close All Projects…")) {
                model.closeAllProjects()
            }
            .disabled(model.projects.isEmpty)
        }

        CommandGroup(replacing: .saveItem) {}
        CommandGroup(replacing: .importExport) {}
        CommandGroup(replacing: .toolbar) {}
        CommandGroup(replacing: .windowArrangement) {}

        CommandGroup(replacing: .appInfo) {
            Button(String(format: model.i18n("menu.app.about_format", fallback: "About %@"), model.appDisplayName)) {
                AboutWindowPresenter.show(model: model)
            }
        }

        CommandGroup(replacing: .help) {
            Button(model.i18n("menu.help.github", fallback: "GitHub")) {
                model.openURL(AppSupportLinks.github)
            }

            Button(model.i18n("menu.help.github_issue", fallback: "GitHub Issue")) {
                model.openURL(AppSupportLinks.issues)
            }

            Button(model.i18n("menu.help.website", fallback: "Official Website")) {
                model.openURL(AppSupportLinks.website)
            }
        }

        CommandGroup(after: .sidebar) {
            ShortcutCommandButton(
                title: model.i18n("menu.view.create_split", fallback: "Create Split"),
                shortcut: model.appSettings.shortcuts.splitPane
            ) {
                model.splitSelectedPane(axis: .horizontal)
            }

            ShortcutCommandButton(
                title: model.i18n("menu.view.create_tab", fallback: "Create Tab"),
                shortcut: model.appSettings.shortcuts.createTab
            ) {
                model.createBottomTab()
            }

            Divider()

            ShortcutCommandButton(
                title: model.i18n("menu.view.open_git_panel", fallback: "Open Git Panel"),
                shortcut: model.appSettings.shortcuts.toggleGitPanel
            ) {
                model.toggleRightPanel(.git)
            }

            ShortcutCommandButton(
                title: model.i18n("menu.view.open_ai_panel", fallback: "Open AI Panel"),
                shortcut: model.appSettings.shortcuts.toggleAIPanel
            ) {
                model.toggleRightPanel(.aiStats)
            }

            Divider()

            Button(model.i18n("menu.view.toggle_full_screen", fallback: "Toggle Full Screen")) {
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
            return model.i18n("menu.file.close_window", fallback: "Close Window")
        }
        return model.i18n("menu.file.close_current_split", fallback: "Close Current Split")
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
        Button(String(format: model.i18n("menu.view.workspace_format", fallback: "Workspace %@"), "\(index + 1)")) {
            model.selectProject(atSidebarIndex: index)
        }
        .keyboardShortcut(KeyEquivalent(Character(String(index + 1))), modifiers: [.command])
        .disabled(!model.projects.indices.contains(index))
    }
}
