import AppKit
import SwiftUI

struct GitPanelSeparator: View {
    var body: some View {
        Rectangle()
            .fill(AppTheme.separator)
            .frame(height: 1)
    }
}

struct GitToolbarIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitToolbarIconButtonBody(configuration: configuration)
    }
}

struct GitToolbarIconButtonBody: View {
    let configuration: ButtonStyle.Configuration
    @State private var isHovered = false

    var body: some View {
        configuration.label
            .foregroundStyle(
                isHovered || configuration.isPressed
                    ? AppTheme.textPrimary
                    : AppTheme.textSecondary
            )
            .frame(width: 28, height: 28)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(backgroundColor)
            )
            .contentShape(RoundedRectangle(cornerRadius: 8, style: .continuous))
            .onHover { hovering in
                isHovered = hovering
            }
    }

    private var backgroundColor: Color {
        if configuration.isPressed {
            return Color(nsColor: .tertiarySystemFill)
        }
        if isHovered {
            return Color(nsColor: .quaternarySystemFill)
        }
        return Color.clear
    }
}

struct GitIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitSecondaryHoverIconButtonBody(configuration: configuration)
    }
}

struct GitHeaderIconButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitSecondaryHoverIconButtonBody(configuration: configuration)
    }
}

struct GitSecondaryHoverIconButtonBody: View {
    let configuration: ButtonStyle.Configuration
    @State private var isHovered = false

    var body: some View {
        configuration.label
            .foregroundStyle(
                (isHovered || configuration.isPressed)
                    ? AppTheme.textPrimary
                    : AppTheme.textSecondary
            )
            .frame(width: 22, height: 22)
            .background(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .fill(backgroundColor)
            )
            .overlay(
                RoundedRectangle(cornerRadius: 4, style: .continuous)
                    .stroke(borderColor, lineWidth: 0.5)
            )
            .contentShape(RoundedRectangle(cornerRadius: 4, style: .continuous))
            .onHover { hovering in
                isHovered = hovering
            }
    }

    private var backgroundColor: Color {
        if configuration.isPressed {
            return AppTheme.card.opacity(0.9)
        }
        return AppTheme.panel.opacity(0.88)
    }

    private var borderColor: Color {
        if configuration.isPressed {
            return AppTheme.separator.opacity(0.5)
        }
        if isHovered {
            return AppTheme.separator.opacity(0.45)
        }
        return AppTheme.separator.opacity(0.3)
    }
}

struct CommitMainButtonStyle: ButtonStyle {
    @Environment(\.isEnabled) private var isEnabled

    func makeBody(configuration: Configuration) -> some View {
        configuration.label
            .font(.system(size: 14, weight: .semibold, design: .rounded))
            .padding(.horizontal, 12)
            .frame(maxWidth: .infinity)
            .frame(height: 32)
            .foregroundStyle(Color.white.opacity(isEnabled ? 0.98 : 0.78))
            .background(Color.white.opacity(isEnabled && configuration.isPressed ? 0.08 : 0))
    }
}
