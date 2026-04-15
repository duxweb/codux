import SwiftUI

struct WorkspaceEmptyStateView: View {
    let model: AppModel

    var body: some View {
        VStack(spacing: 0) {
            Spacer()

            VStack(spacing: 20) {
                Image(nsImage: model.appIconImage)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
                    .frame(width: 72, height: 72)
                    .shadow(color: .black.opacity(0.08), radius: 6, y: 3)

                VStack(spacing: 6) {
                    Text(String(format: model.i18n("welcome.title_format", fallback: "Welcome to %@"), model.appDisplayName))
                        .font(.system(size: 22, weight: .bold))
                        .foregroundStyle(.primary)

                    Text(model.i18n("welcome.subtitle", fallback: "Create a project in the sidebar to get started"))
                    .font(.system(size: 13))
                    .foregroundStyle(.secondary)
                }

                Button {
                    model.addProject()
                } label: {
                    Label(model.i18n("menu.file.new_project", fallback: "New Project"), systemImage: "plus")
                        .font(.system(size: 14, weight: .semibold))
                        .padding(.horizontal, 12)
                        .padding(.vertical, 4)
                }
                .buttonStyle(.borderedProminent)
                .controlSize(.large)
            }

            Spacer()

            WelcomeShortcutHintsView(model: model)
                .padding(.bottom, 20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct WelcomeShortcutHintsView: View {
    let model: AppModel

    var body: some View {
        HStack(spacing: 24) {
            if let shortcut = model.appSettings.shortcuts.splitPane {
                shortcutHint(
                    symbol: "rectangle.split.2x1",
                    label: model.i18n("titlebar.split", fallback: "Split"),
                    keys: shortcut.title
                )
            }
            if let shortcut = model.appSettings.shortcuts.createTab {
                shortcutHint(
                    symbol: "plus.rectangle.on.rectangle",
                    label: model.i18n("titlebar.tab", fallback: "Tab"),
                    keys: shortcut.title
                )
            }
            if let shortcut = model.appSettings.shortcuts.toggleGitPanel {
                shortcutHint(
                    symbol: "arrow.triangle.branch",
                    label: model.i18n("titlebar.git", fallback: "Git"),
                    keys: shortcut.title
                )
            }
            if let shortcut = model.appSettings.shortcuts.toggleAIPanel {
                shortcutHint(
                    symbol: "sparkles",
                    label: model.i18n("titlebar.ai", fallback: "AI"),
                    keys: shortcut.title
                )
            }
        }
    }

    private func shortcutHint(symbol: String, label: String, keys: String) -> some View {
        VStack(spacing: 4) {
            Image(systemName: symbol)
                .font(.system(size: 13))
                .foregroundStyle(.tertiary)

            Text(label)
                .font(.system(size: 11))
                .foregroundStyle(.secondary)

            Text(keys)
                .font(.system(size: 10, design: .rounded))
                .foregroundStyle(.tertiary)
        }
    }
}
