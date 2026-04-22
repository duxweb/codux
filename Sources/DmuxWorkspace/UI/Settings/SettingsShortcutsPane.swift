import AppKit
import SwiftUI

struct ShortcutSettingsPane: View {
    let model: AppModel

    var body: some View {
        Form {
            Section {
                shortcutRow(String(localized: "settings.shortcut.create_split", defaultValue: "Create Split", bundle: .module), target: .splitPane, value: model.appSettings.shortcuts.splitPane)
                shortcutRow(String(localized: "settings.shortcut.create_tab", defaultValue: "Create Tab", bundle: .module), target: .createTab, value: model.appSettings.shortcuts.createTab)
                shortcutRow(String(localized: "settings.shortcut.open_git_panel", defaultValue: "Git Panel", bundle: .module), target: .toggleGitPanel, value: model.appSettings.shortcuts.toggleGitPanel)
                shortcutRow(String(localized: "settings.shortcut.open_ai_panel", defaultValue: "AI Panel", bundle: .module), target: .toggleAIPanel, value: model.appSettings.shortcuts.toggleAIPanel)
            }

            Section(String(localized: "settings.shortcut.project_switch", defaultValue: "Project Switch Shortcuts", bundle: .module)) {
                Text(String(localized: "settings.shortcut.project_switch_hint", defaultValue: "Use ⌘1-⌘9 to switch projects in sidebar order.", bundle: .module))
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .scrollContentBackground(.hidden)
        .background(Color(nsColor: .windowBackgroundColor))
    }

    @ViewBuilder
    private func shortcutRow(_ title: String, target: AppShortcutTarget, value: AppKeyboardShortcut?) -> some View {
        LabeledContent(title) {
            ShortcutRecorderField(
                value: value,
                placeholder: String(localized: "settings.shortcut.record", defaultValue: "Record Shortcut", bundle: .module)
            ) { shortcut in
                model.updateShortcut(shortcut, for: target)
            }
        }
    }
}

private struct ShortcutRecorderField: View {
    let value: AppKeyboardShortcut?
    let placeholder: String
    let onChange: (AppKeyboardShortcut?) -> Void
    @State private var isRecording = false

    var body: some View {
        HStack(spacing: 6) {
            ShortcutRecorderRepresentable(
                isRecording: $isRecording,
                onRecord: onChange
            )
            .frame(width: 0, height: 0)

            Button {
                isRecording = true
            } label: {
                HStack(spacing: 6) {
                    Text(isRecording ? "..." : (value?.title ?? placeholder))
                        .font(.system(size: 12, design: .rounded))
                        .foregroundStyle(value == nil && !isRecording ? .tertiary : .primary)

                    Image(systemName: "keyboard")
                        .font(.system(size: 10))
                        .foregroundStyle(.tertiary)
                }
                .padding(.horizontal, 10)
                .padding(.vertical, 5)
                .background(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .fill(Color(nsColor: .controlBackgroundColor))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 6, style: .continuous)
                        .stroke(isRecording ? Color.accentColor : Color(nsColor: .separatorColor), lineWidth: isRecording ? 1.5 : 0.5)
                )
            }
            .buttonStyle(.plain)

            if value != nil {
                Button {
                    onChange(nil)
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 12))
                        .foregroundStyle(.tertiary)
                }
                .buttonStyle(.plain)
            }
        }
    }
}

private struct ShortcutRecorderRepresentable: NSViewRepresentable {
    @Binding var isRecording: Bool
    let onRecord: (AppKeyboardShortcut?) -> Void

    func makeNSView(context: Context) -> ShortcutRecorderNSView {
        let view = ShortcutRecorderNSView()
        view.onRecord = onRecord
        view.onCancel = { isRecording = false }
        return view
    }

    func updateNSView(_ nsView: ShortcutRecorderNSView, context: Context) {
        nsView.onRecord = { value in
            onRecord(value)
            isRecording = false
        }
        nsView.onCancel = {
            isRecording = false
        }
        if isRecording, nsView.window?.firstResponder !== nsView {
            DispatchQueue.main.async {
                nsView.window?.makeFirstResponder(nsView)
            }
        }
    }
}

private final class ShortcutRecorderNSView: NSView {
    var onRecord: ((AppKeyboardShortcut?) -> Void)?
    var onCancel: (() -> Void)?

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        switch event.keyCode {
        case 53:
            onCancel?()
            return
        case 51, 117:
            onRecord?(nil)
            return
        default:
            break
        }

        let modifiers = AppShortcutModifiers.from(eventModifiers: event.modifierFlags)
        guard !modifiers.isEmpty else {
            NSSound.beep()
            return
        }

        let cleaned = (event.charactersIgnoringModifiers ?? "")
            .trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard let character = cleaned.first, character.isLetter || character.isNumber else {
            NSSound.beep()
            return
        }

        onRecord?(AppKeyboardShortcut(key: String(character), modifiers: modifiers))
    }
}
