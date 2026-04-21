import SwiftUI

struct WorkspaceEmptyStateView: View {
    let model: AppModel

    private var titleColor: Color {
        model.terminalUsesLightBackground
            ? model.terminalTextColor.opacity(0.9)
            : model.terminalTextColor.opacity(0.82)
    }

    private var subtitleColor: Color {
        model.terminalUsesLightBackground
            ? model.terminalMutedTextColor.opacity(0.84)
            : model.terminalMutedTextColor.opacity(0.78)
    }

    private var tertiaryColor: Color {
        model.terminalUsesLightBackground
            ? model.terminalMutedTextColor.opacity(0.72)
            : model.terminalMutedTextColor.opacity(0.74)
    }

    private var secondaryButtonFill: Color {
        model.terminalUsesLightBackground
            ? Color.black.opacity(0.07)
            : Color.white.opacity(0.09)
    }

    private var secondaryButtonPressedFill: Color {
        model.terminalUsesLightBackground
            ? Color.black.opacity(0.11)
            : Color.white.opacity(0.13)
    }

    private var secondaryButtonStroke: Color {
        model.terminalUsesLightBackground
            ? Color.black.opacity(0.12)
            : Color.white.opacity(0.11)
    }

    private var primaryButtonFill: Color {
        AppTheme.focus
    }

    private var primaryButtonPressedFill: Color {
        AppTheme.focus.opacity(0.82)
    }

    private var primaryButtonStroke: Color {
        model.terminalUsesLightBackground
            ? Color.white.opacity(0.18)
            : Color.white.opacity(0.12)
    }

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
                    Text(String(format: String(localized: "welcome.title_format", defaultValue: "Welcome to %@", bundle: .module), model.appDisplayName))
                        .font(.system(size: 22, weight: .bold))
                        .foregroundStyle(titleColor)

                    Text(String(localized: "welcome.subtitle", defaultValue: "Create a project in the sidebar to get started", bundle: .module))
                    .font(.system(size: 13))
                    .foregroundStyle(subtitleColor)
                }

                VStack(spacing: 10) {
                    Button {
                        model.addProject()
                    } label: {
                        WelcomeActionButtonLabel(
                            title: String(localized: "menu.file.new_project", defaultValue: "New Project", bundle: .module),
                            systemImage: "plus",
                            foreground: .white
                        )
                    }
                    .buttonStyle(
                        WelcomeFilledButtonStyle(
                            fill: primaryButtonFill,
                            pressedFill: primaryButtonPressedFill,
                            stroke: primaryButtonStroke
                        )
                    )

                    Button {
                        model.openProjectFolder()
                    } label: {
                        WelcomeActionButtonLabel(
                            title: String(localized: "welcome.open_project", defaultValue: "Open Project", bundle: .module),
                            systemImage: "folder",
                            foreground: titleColor
                        )
                    }
                    .buttonStyle(
                        WelcomeFilledButtonStyle(
                            fill: secondaryButtonFill,
                            pressedFill: secondaryButtonPressedFill,
                            stroke: secondaryButtonStroke
                        )
                    )
                }
            }

            Spacer()

            WelcomeShortcutHintsView(model: model)
                .foregroundStyle(subtitleColor)
                .padding(.bottom, 20)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

private struct WelcomeActionButtonLabel: View {
    let title: String
    let systemImage: String
    let foreground: Color

    var body: some View {
        HStack(spacing: 9) {
            Image(systemName: systemImage)
                .font(.system(size: 14, weight: .semibold))
                .frame(width: 14, height: 14)

            Text(title)
                .font(.system(size: 15, weight: .medium))
                .lineLimit(1)
        }
        .foregroundStyle(foreground)
        .frame(minWidth: 136)
        .padding(.horizontal, 16)
        .padding(.vertical, 10)
        .contentShape(Rectangle())
    }
}

private struct WelcomeFilledButtonStyle: ButtonStyle {
    let fill: Color
    let pressedFill: Color
    let stroke: Color

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .background(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .fill(configuration.isPressed ? pressedFill : fill)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 9, style: .continuous)
                    .stroke(stroke, lineWidth: 1)
            )
            .shadow(color: .black.opacity(configuration.isPressed ? 0.04 : 0.08), radius: configuration.isPressed ? 2 : 5, y: configuration.isPressed ? 1 : 2)
            .scaleEffect(configuration.isPressed ? 0.985 : 1)
            .animation(.easeOut(duration: 0.12), value: configuration.isPressed)
    }
}

private struct WelcomeShortcutHintsView: View {
    let model: AppModel

    private var subtitleColor: Color {
        model.terminalUsesLightBackground
            ? model.terminalMutedTextColor.opacity(0.84)
            : model.terminalMutedTextColor.opacity(0.78)
    }

    private var tertiaryColor: Color {
        model.terminalUsesLightBackground
            ? model.terminalMutedTextColor.opacity(0.72)
            : model.terminalMutedTextColor.opacity(0.74)
    }

    var body: some View {
        HStack(spacing: 24) {
            if let shortcut = model.appSettings.shortcuts.splitPane {
                shortcutHint(
                    symbol: "rectangle.split.2x1",
                    label: String(localized: "titlebar.split", defaultValue: "Split", bundle: .module),
                    keys: shortcut.title
                )
            }
            if let shortcut = model.appSettings.shortcuts.createTab {
                shortcutHint(
                    symbol: "plus.rectangle.on.rectangle",
                    label: String(localized: "titlebar.tab", defaultValue: "Tab", bundle: .module),
                    keys: shortcut.title
                )
            }
            if let shortcut = model.appSettings.shortcuts.toggleGitPanel {
                shortcutHint(
                    symbol: "arrow.triangle.branch",
                    label: String(localized: "titlebar.git", defaultValue: "Git", bundle: .module),
                    keys: shortcut.title
                )
            }
            if let shortcut = model.appSettings.shortcuts.toggleAIPanel {
                shortcutHint(
                    symbol: "sparkles",
                    label: String(localized: "titlebar.ai", defaultValue: "AI", bundle: .module),
                    keys: shortcut.title
                )
            }
        }
    }

    private func shortcutHint(symbol: String, label: String, keys: String) -> some View {
        VStack(spacing: 4) {
            Image(systemName: symbol)
                .font(.system(size: 13))
                .foregroundStyle(tertiaryColor)

            Text(label)
                .font(.system(size: 11))
                .foregroundStyle(subtitleColor)

            Text(keys)
                .font(.system(size: 10, design: .rounded))
                .foregroundStyle(tertiaryColor)
        }
    }
}
