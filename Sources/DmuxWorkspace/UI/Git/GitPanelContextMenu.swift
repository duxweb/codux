import AppKit
import SwiftUI

@MainActor
func buildGitFileContextMenu(model: AppModel, fallbackEntry: GitFileEntry) -> [NativeContextMenuAction] {
    let selectedEntries = model.selectedGitEntriesForContextMenu.isEmpty ? [fallbackEntry] : model.selectedGitEntriesForContextMenu
    let allStaged = !selectedEntries.isEmpty && selectedEntries.allSatisfy { $0.kind == .staged }
    let hasNonStaged = selectedEntries.contains { $0.kind != .staged }
    let allUntracked = !selectedEntries.isEmpty && selectedEntries.allSatisfy { $0.kind == .untracked }

    var actions: [NativeContextMenuAction] = []

    actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.copy_selected_paths", defaultValue: "Copy Selected Paths", bundle: .module) : String(localized: "git.files.copy_path", defaultValue: "Copy Path", bundle: .module)) {
        model.copyGitPaths(selectedEntries)
    })

    actions.append(.action(String(localized: "git.files.show_in_finder", defaultValue: "Show in Finder", bundle: .module)) {
        model.revealGitEntriesInFinder(selectedEntries)
    })

    actions.append(.separator)

    if allStaged {
        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.unstage_selected", defaultValue: "Unstage Selected", bundle: .module) : String(localized: "git.files.unstage", defaultValue: "Unstage", bundle: .module)) {
            model.unstageEntries(selectedEntries)
        })
    } else {
        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.stage_selected", defaultValue: "Stage Selected", bundle: .module) : String(localized: "git.files.stage", defaultValue: "Stage", bundle: .module)) {
            model.stageEntries(selectedEntries)
        })
    }

    if hasNonStaged {
        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.discard_selected_changes", defaultValue: "Discard Selected Changes", bundle: .module) : String(localized: "git.files.discard_changes", defaultValue: "Discard Changes", bundle: .module)) {
            model.discardEntries(selectedEntries)
        })
    }

    if allUntracked {
        actions.append(.separator)

        actions.append(.action(String(localized: "git.ignore.add", defaultValue: "Add to .gitignore", bundle: .module)) {
            model.addGitEntriesToIgnore(selectedEntries)
        })

        actions.append(.action(selectedEntries.count > 1 ? String(localized: "git.files.delete_selected_files", defaultValue: "Delete Selected Files", bundle: .module) : String(localized: "git.files.delete_file", defaultValue: "Delete File", bundle: .module)) {
            model.discardEntries(selectedEntries)
        })
    }

    return actions
}

@MainActor
func buildGitCommitContextMenu(model: AppModel, commit: GitCommitEntry) -> [NativeContextMenuAction] {
    var actions: [NativeContextMenuAction] = [
        .action(String(localized: "git.history.copy_commit_hash", defaultValue: "Copy Commit Hash", bundle: .module)) { model.copyGitCommitHash(commit) },
        .action(String(localized: "git.history.checkout_commit", defaultValue: "Checkout This Commit", bundle: .module)) { model.checkoutGitCommit(commit) },
        .action(String(localized: "git.history.create_branch_from_commit", defaultValue: "Create Branch from This Commit", bundle: .module)) { model.createBranch(from: commit) },
    ]

    if model.gitHistory.first?.hash == commit.hash {
        actions.append(.separator)
        actions.append(.action(String(localized: "git.history.undo_last_commit", defaultValue: "Undo Last Commit", bundle: .module)) { model.undoLastGitCommit() })
        actions.append(.action(String(localized: "git.history.edit_last_commit_message", defaultValue: "Edit Last Commit Message", bundle: .module)) { model.editLastGitCommitMessage() })
    }

    actions.append(.separator)
    actions.append(.action(String(localized: "git.history.revert_commit", defaultValue: "Revert This Commit", bundle: .module)) { model.revertGitCommit(commit) })
    actions.append(.separator)
    actions.append(.action(String(localized: "git.history.restore_local", defaultValue: "Restore This Revision Locally", bundle: .module)) { model.restoreGitCommit(commit, forceRemote: false) })
    actions.append(.action(String(localized: "git.history.restore_remote", defaultValue: "Restore This Revision Remotely", bundle: .module)) { model.restoreGitCommit(commit, forceRemote: true) })

    return actions
}

enum NativeContextMenuAction {
    case separator
    case action(String, () -> Void)
}

@MainActor
struct NativeContextMenuRegion: NSViewRepresentable {
    let onOpen: () -> Void
    let menuProvider: () -> [NativeContextMenuAction]

    func makeNSView(context: Context) -> NativeContextMenuView {
        let view = NativeContextMenuView()
        view.onOpen = onOpen
        view.menuProvider = menuProvider
        return view
    }

    func updateNSView(_ nsView: NativeContextMenuView, context: Context) {
        nsView.onOpen = onOpen
        nsView.menuProvider = menuProvider
    }
}

@MainActor
final class NativeContextMenuView: NSView {
    var onOpen: (() -> Void)?
    var menuProvider: (() -> [NativeContextMenuAction])?
    private var handlers: [NativeContextMenuHandler] = []

    override var isOpaque: Bool { false }

    override func hitTest(_ point: NSPoint) -> NSView? {
        guard let event = NSApp.currentEvent else { return nil }
        switch event.type {
        case .rightMouseDown, .rightMouseUp, .otherMouseDown, .otherMouseUp:
            return self
        default:
            return nil
        }
    }

    override func rightMouseDown(with event: NSEvent) {
        onOpen?()
        let menu = NSMenu()
        handlers.removeAll()

        for action in menuProvider?() ?? [] {
            switch action {
            case .separator:
                menu.addItem(.separator())
            case let .action(title, callback):
                let handler = NativeContextMenuHandler(action: callback)
                handlers.append(handler)
                let item = NSMenuItem(title: title, action: #selector(NativeContextMenuHandler.performAction), keyEquivalent: "")
                item.target = handler
                menu.addItem(item)
            }
        }

        NSMenu.popUpContextMenu(menu, with: event, for: self)
    }
}

@MainActor
final class NativeContextMenuHandler: NSObject {
    let action: () -> Void

    init(action: @escaping () -> Void) {
        self.action = action
    }

    @objc
    func performAction() {
        action()
    }
}
