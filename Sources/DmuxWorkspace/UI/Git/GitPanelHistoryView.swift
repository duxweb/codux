import AppKit
import SwiftUI

struct GitHistoryRegion: View {
    let model: AppModel
    let history: [GitCommitEntry]
    let clearFocus: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            HStack {
                Text(String(localized: "git.history.title", defaultValue: "Git History", bundle: .module))
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                    .foregroundStyle(AppTheme.textSecondary)
                Spacer()
            }
            .padding(.horizontal, 16)
            .frame(height: 34)
            .background {
                RoundedRectangle(cornerRadius: 0, style: .continuous)
                    .fill(AppTheme.aiPanelCardBackground.opacity(0.6))
            }
            .overlay(alignment: .bottom) {
                Rectangle()
                    .fill(AppTheme.separator)
                    .frame(height: 1)
            }

            if history.isEmpty {
                Text(String(localized: "git.history.empty", defaultValue: "No Commit History", bundle: .module))
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(AppTheme.textMuted)
                    .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
                    .padding(.horizontal, 16)
                    .padding(.top, 12)
            } else {
                ScrollView {
                    LazyVStack(spacing: 0) {
                        ForEach(history) { item in
                            GitHistoryRow(model: model, item: item)
                        }
                    }
                    .frame(maxWidth: .infinity, alignment: .topLeading)
                }
                .frame(maxHeight: .infinity, alignment: .top)
            }
        }
        .padding(.bottom, 14)
        .contentShape(Rectangle())
        .onTapGesture {
            clearFocus()
        }
    }
}

private struct GitHistoryRow: View {
    let model: AppModel
    let item: GitCommitEntry
    @State private var isHovered = false

    private var isSelected: Bool {
        model.selectedGitCommitHash == item.hash
    }

    var body: some View {
        HStack(alignment: .center, spacing: 6) {
            GitGraphPrefixView(prefix: item.graphPrefix)
                .frame(width: graphWidth)
                .frame(height: 26, alignment: .leading)

            Text(item.subject)
                .font(.system(size: 12, weight: .semibold))
                .foregroundStyle(AppTheme.textPrimary)
                .lineLimit(1)
                .truncationMode(.tail)
                .frame(maxWidth: .infinity, alignment: .leading)

            Text(item.relativeDate)
                .font(.system(size: 12, weight: .medium, design: .rounded))
                .foregroundStyle(AppTheme.textMuted)
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)

            HStack(spacing: 4) {
                ForEach(Array(compactDecorations.enumerated()), id: \.offset) { index, decoration in
                    GitDecorationTag(text: decoration, color: tagColor(for: decoration, index: index))
                }

                if overflowDecorationCount > 0 {
                    GitDecorationTag(text: "+\(overflowDecorationCount)", color: AppTheme.textSecondary)
                }
            }
            .lineLimit(1)
            .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.horizontal, 6)
        .frame(minHeight: 26)
        .background(rowBackground)
        .overlay(alignment: .trailing) {
            if isSelected {
                HStack(spacing: 4) {
                    Button {
                        model.revertGitCommit(item)
                    } label: {
                        Image(systemName: "arrow.uturn.backward")
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .buttonStyle(GitHistoryActionButtonStyle())
                    .help(String(localized: "git.history.revert_commit", defaultValue: "Revert This Commit", bundle: .module))

                    Button {
                        model.createBranch(from: item)
                    } label: {
                        Image(systemName: "point.topleft.down.curvedto.point.bottomright.up")
                            .font(.system(size: 10, weight: .semibold))
                    }
                    .buttonStyle(GitHistoryActionButtonStyle())
                    .help(String(localized: "git.history.create_branch_from_commit", defaultValue: "Create Branch from This Commit", bundle: .module))
                }
                .padding(.leading, 12)
                .padding(.trailing, 6)
            }
        }
        .contentShape(Rectangle())
        .onTapGesture {
            model.selectGitCommit(item)
        }
        .onHover { hovering in
            isHovered = hovering
        }
        .help("\(item.subject)\n\(item.author) · \(item.relativeDate)")
        .overlay {
            NativeContextMenuRegion(
                onOpen: {
                    model.prepareGitCommitContextMenu(item)
                },
                menuProvider: {
                    buildGitCommitContextMenu(model: model, commit: item)
                }
            )
        }
    }

    private var graphWidth: CGFloat {
        34
    }

    private var compactDecorations: [String] {
        Array(item.decorations.prefix(1)).map { decoration in
            decoration
                .replacingOccurrences(of: "HEAD -> ", with: "HEAD→")
                .replacingOccurrences(of: "origin/", with: "o/")
        }
    }

    private var overflowDecorationCount: Int {
        max(0, item.decorations.count - compactDecorations.count)
    }

    private var rowBackground: some View {
        ZStack(alignment: .leading) {
            if isSelected {
                AppTheme.focus.opacity(0.14)
            } else if isHovered {
                Color(nsColor: .quaternarySystemFill)
            } else {
                Color.clear
            }

            if isSelected {
                Rectangle()
                    .fill(AppTheme.focus)
                    .frame(width: 2)
            }
        }
    }

    private func tagColor(for decoration: String, index: Int) -> Color {
        if decoration.contains("HEAD") || decoration == "master" || decoration == "main" {
            return AppTheme.focus
        }

        let palette: [Color] = [AppTheme.success, AppTheme.warning, Color.pink, Color.orange]
        return palette[index % palette.count]
    }
}

struct GitHistoryActionButtonStyle: ButtonStyle {
    func makeBody(configuration: Configuration) -> some View {
        GitSecondaryHoverIconButtonBody(configuration: configuration)
    }
}

private struct GitDecorationTag: View {
    let text: String
    let color: Color

    var body: some View {
        Text(text)
            .font(.system(size: 9, weight: .semibold, design: .rounded))
            .foregroundStyle(color)
            .padding(.horizontal, 5)
            .padding(.vertical, 1)
            .background(color.opacity(0.14))
            .clipShape(Capsule())
    }
}

private struct GitGraphPrefixView: View {
    let prefix: String

    private let columnWidth: CGFloat = 8
    private let strokeWidth: CGFloat = 1.25
    private let palette: [Color] = [AppTheme.focus, AppTheme.success, AppTheme.warning, Color.pink, Color.orange]

    var body: some View {
        Canvas { context, size in
            let chars = Array(prefix)
            let startX = max(0, size.width - CGFloat(chars.count) * columnWidth)

            for (index, char) in chars.enumerated() {
                let centerX = startX + CGFloat(index) * columnWidth + columnWidth / 2
                let color = palette[index % palette.count]
                var path = Path()

                switch char {
                case "|":
                    path.move(to: CGPoint(x: centerX, y: -8))
                    path.addLine(to: CGPoint(x: centerX, y: size.height + 8))
                    context.stroke(path, with: .color(color), lineWidth: strokeWidth)
                case "/":
                    path.move(to: CGPoint(x: centerX + 2.5, y: -8))
                    path.addLine(to: CGPoint(x: centerX - 2.5, y: size.height + 8))
                    context.stroke(path, with: .color(color), lineWidth: strokeWidth)
                case "\\":
                    path.move(to: CGPoint(x: centerX - 2.5, y: -8))
                    path.addLine(to: CGPoint(x: centerX + 2.5, y: size.height + 8))
                    context.stroke(path, with: .color(color), lineWidth: strokeWidth)
                case "*", "o":
                    path.move(to: CGPoint(x: centerX, y: -8))
                    path.addLine(to: CGPoint(x: centerX, y: size.height + 8))
                    context.stroke(path, with: .color(color.opacity(0.5)), lineWidth: 1)

                    let nodeRect = CGRect(x: centerX - 3.5, y: size.height / 2 - 3.5, width: 7, height: 7)
                    context.fill(Path(ellipseIn: nodeRect), with: .color(color))
                default:
                    continue
                }
            }
        }
    }
}
