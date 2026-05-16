import AppKit
import SwiftUI

enum GitInputDialogKind: Equatable {
    case createBranch
    case createBranchFromCommit(String)
    case editLastCommitMessage(headCommitPushed: Bool)
    case cloneRepository
    case renameAISession
    case renameTerminalTab
}

struct GitInputDialogState: Equatable {
    var kind: GitInputDialogKind
    var title: String
    var message: String
    var placeholder: String
    var confirmTitle: String
    var value: String
    var isMultiline: Bool
}

struct ProjectEditorDialogState: Equatable {
    var title: String
    var message: String
    var confirmTitle: String
    var name: String
    var path: String
    var badgeText: String
    var badgeSymbol: String?
    var badgeColorHex: String
}

struct WorktreeTaskDialogState: Equatable {
    var title: String
    var message: String
    var confirmTitle: String
    var baseBranches: [String]
    var baseBranch: String
    var branchName: String
    var taskTitle: String
}

struct WorktreeTaskDialogResult: Equatable {
    var baseBranch: String
    var branchName: String
    var taskTitle: String
}

struct AgentSplitDialogState: Equatable {
    var title: String
    var message: String
    var confirmTitle: String
    var tools: [AgentToolKind]
    var selectedTool: AgentToolKind
}

struct AgentSplitDialogResult: Equatable {
    var tool: AgentToolKind
}

enum ConfirmDialogResult {
    case primary
    case secondary
    case cancel
}

struct ConfirmDialogOptionState {
    var title: String
    var isOn: Bool = false
}

struct ConfirmDialogOptionResult {
    var action: ConfirmDialogResult
    var isOptionEnabled: Bool
}

struct ConfirmDialogState {
    var title: String
    var message: String
    var icon: String
    var iconColor: Color
    var primaryTitle: String
    var primaryTint: Color = AppTheme.focus
    var secondaryTitle: String?
    var cancelTitle: String?
    var option: ConfirmDialogOptionState? = nil
}

struct GitCredentialDialogState {
    var title: String
    var message: String
    var confirmTitle: String
    var cancelTitle: String
    var username: String = ""
    var password: String = ""
}

struct SSHProfileDialogState: Equatable {
    var title: String
    var message: String
    var confirmTitle: String
    var profile: SSHConnectionProfile
    var password: String
    var keyPassphrase: String
}

struct SSHProfileDialogResult: Equatable {
    var profile: SSHConnectionProfile
    var password: String
    var keyPassphrase: String
}

enum GitInputPanelPresenter {
    @MainActor
    static func present(dialog: GitInputDialogState, parentWindow: NSWindow, completion: @escaping (String?) -> Void) {
        let controller = GitInputPanelController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

enum ProjectEditorPanelPresenter {
    @MainActor
    static func present(dialog: ProjectEditorDialogState, parentWindow: NSWindow, completion: @escaping (ProjectEditorDialogState?) -> Void) {
        let controller = ProjectEditorPanelController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

enum WorktreeTaskPanelPresenter {
    @MainActor
    static func present(dialog: WorktreeTaskDialogState, parentWindow: NSWindow, completion: @escaping (WorktreeTaskDialogResult?) -> Void) {
        let controller = WorktreeTaskPanelController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

enum AgentSplitDialogPresenter {
    @MainActor
    static func present(dialog: AgentSplitDialogState, parentWindow: NSWindow, completion: @escaping (AgentSplitDialogResult?) -> Void) {
        let controller = AgentSplitPanelController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

enum ConfirmDialogPresenter {
    @MainActor
    static func present(dialog: ConfirmDialogState, parentWindow: NSWindow, completion: @escaping (ConfirmDialogResult?) -> Void) {
        let controller = ConfirmDialogController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }

    @MainActor
    static func presentWithOption(dialog: ConfirmDialogState, parentWindow: NSWindow, completion: @escaping (ConfirmDialogOptionResult?) -> Void) {
        let controller = ConfirmDialogOptionController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

enum GitCredentialDialogPresenter {
    @MainActor
    static func present(dialog: GitCredentialDialogState, parentWindow: NSWindow, completion: @escaping (GitCredential?) -> Void) {
        let controller = GitCredentialDialogController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

enum SSHProfileDialogPresenter {
    @MainActor
    static func present(dialog: SSHProfileDialogState, parentWindow: NSWindow, completion: @escaping (SSHProfileDialogResult?) -> Void) {
        let controller = SSHProfileDialogController(dialog: dialog)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}
