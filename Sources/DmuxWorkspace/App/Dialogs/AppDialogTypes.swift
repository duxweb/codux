import AppKit
import SwiftUI

enum GitInputDialogKind: Equatable {
    case createBranch
    case createBranchFromCommit(String)
    case editLastCommitMessage(headCommitPushed: Bool)
    case cloneRepository
    case renameAISession
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

enum ConfirmDialogResult {
    case primary
    case secondary
    case cancel
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
}

struct GitCredentialDialogState {
    var title: String
    var message: String
    var confirmTitle: String
    var cancelTitle: String
    var username: String = ""
    var password: String = ""
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

enum ConfirmDialogPresenter {
    @MainActor
    static func present(dialog: ConfirmDialogState, parentWindow: NSWindow, completion: @escaping (ConfirmDialogResult?) -> Void) {
        let controller = ConfirmDialogController(dialog: dialog)
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
