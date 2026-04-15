import AppKit
import SwiftUI

struct SidebarView: View {
    let model: AppModel
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        let _ = model.activityRenderVersion

        VStack(spacing: 0) {
            if model.isSidebarExpanded {
                Text(model.i18n("sidebar.workspace", fallback: "Workspace"))
                    .font(.system(size: 17, weight: .bold, design: .rounded))
                    .foregroundStyle(AppTheme.textPrimary)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 18)
                .padding(.top, 18)
                .padding(.bottom, 12)
            }

            ScrollView {
                LazyVStack(spacing: 4) {
                    ForEach(model.projects) { project in
                        ProjectRow(
                            project: project,
                            isExpanded: model.isSidebarExpanded,
                            isSelected: project.id == model.selectedProjectID,
                            activityPhase: model.activityPhase(for: project.id)
                        ) {
                            model.selectProject(project.id)
                        }
                        .contextMenu {
                            Button(model.i18n("sidebar.project.open_folder", fallback: "Open Folder")) {
                                model.openProjectDirectory(project.id)
                            }
                            Divider()
                            Button(model.i18n("common.rename", fallback: "Rename")) {
                                model.editProject(project.id)
                            }
                            Button(model.i18n("sidebar.project.edit", fallback: "Edit Project")) {
                                model.editProject(project.id)
                            }
                            Divider()
                            Button(model.i18n("sidebar.project.remove", fallback: "Remove Project"), role: .destructive) {
                                model.removeProject(project.id)
                            }
                        }
                    }
                }
                .padding(.horizontal, model.isSidebarExpanded ? 12 : 16)
                .padding(.top, model.isSidebarExpanded ? 4 : 18)
                .padding(.bottom, 16)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)

            VStack(spacing: 6) {
                SidebarFooterButton(
                    symbol: "plus",
                    title: model.i18n("sidebar.footer.add_project", fallback: "Add Project"),
                    isExpanded: model.isSidebarExpanded
                ) {
                    model.addProject()
                }

                SidebarFooterButton(
                    symbol: "gearshape",
                    title: model.i18n("sidebar.footer.settings", fallback: "Settings"),
                    isExpanded: model.isSidebarExpanded
                ) {
                    openSettings()
                }

                SidebarHelpMenuButton(
                    model: model,
                    title: model.i18n("sidebar.footer.help", fallback: "Help"),
                    isExpanded: model.isSidebarExpanded
                ) {
                    AboutWindowPresenter.show(model: model)
                } openGitHub: {
                    model.openURL(AppSupportLinks.github)
                } openIssues: {
                    model.openURL(AppSupportLinks.issues)
                } openWebsite: {
                    model.openURL(AppSupportLinks.website)
                }
            }
            .frame(maxWidth: .infinity)
            .padding(.horizontal, model.isSidebarExpanded ? 12 : 0)
            .padding(.bottom, 16)
            .padding(.top, 12)
        }
        .background(Color.clear)
        .animation(.snappy(duration: 0.22), value: model.isSidebarExpanded)
    }
}

private struct SidebarHelpMenuButton: View {
    let model: AppModel
    let title: String
    let isExpanded: Bool
    let showAbout: () -> Void
    let openGitHub: () -> Void
    let openIssues: () -> Void
    let openWebsite: () -> Void
    @State private var isHovered = false
    @State private var menuAnchorView: NSView?

    var body: some View {
        Button {
            presentMenu()
        } label: {
            SidebarFooterLabel(
                symbol: "ellipsis.circle",
                title: title,
                isExpanded: isExpanded,
                isHovered: isHovered
            )
        }
        .buttonStyle(.plain)
        .frame(maxWidth: .infinity, alignment: isExpanded ? .leading : .center)
        .floatingTooltip(title, enabled: !isExpanded, placement: .right)
        .onHover { hovering in
            isHovered = hovering
        }
        .background(SidebarMenuAnchorView(anchorView: $menuAnchorView))
        .frame(height: 30)
    }

    private func presentMenu() {
        guard let anchorView = menuAnchorView else {
            return
        }

        let menu = NSMenu()
        var handlers: [SidebarMenuActionHandler] = []

        func addItem(_ title: String, action: @escaping () -> Void) {
            let handler = SidebarMenuActionHandler(action: action)
            handlers.append(handler)
            let item = NSMenuItem(title: title, action: #selector(SidebarMenuActionHandler.performAction), keyEquivalent: "")
            item.target = handler
            menu.addItem(item)
        }

        addItem(model.i18n("common.about", fallback: "About"), action: showAbout)
        menu.addItem(.separator())
        addItem(model.i18n("menu.help.github", fallback: "GitHub"), action: openGitHub)
        addItem(model.i18n("menu.help.github_issue", fallback: "GitHub Issue"), action: openIssues)
        addItem(model.i18n("menu.help.website", fallback: "Official Website"), action: openWebsite)

        objc_setAssociatedObject(anchorView, Unmanaged.passUnretained(anchorView).toOpaque(), handlers, .OBJC_ASSOCIATION_RETAIN_NONATOMIC)
        menu.popUp(positioning: nil, at: NSPoint(x: 0, y: anchorView.bounds.height + 4), in: anchorView)
    }
}

private struct SidebarMenuAnchorView: NSViewRepresentable {
    @Binding var anchorView: NSView?

    func makeNSView(context: Context) -> NSView {
        let view = NSView(frame: .zero)
        DispatchQueue.main.async {
            anchorView = view
        }
        return view
    }

    func updateNSView(_ nsView: NSView, context: Context) {
        if anchorView !== nsView {
            DispatchQueue.main.async {
                anchorView = nsView
            }
        }
    }
}

private final class SidebarMenuActionHandler: NSObject {
    private let action: () -> Void

    init(action: @escaping () -> Void) {
        self.action = action
    }

    @objc
    func performAction() {
        action()
    }
}

private struct ProjectRow: View {
    let project: Project
    let isExpanded: Bool
    let isSelected: Bool
    let activityPhase: ProjectActivityPhase
    let action: () -> Void

    private var selectionContainer: some View {
        RoundedRectangle(cornerRadius: 12, style: .continuous)
            .fill(isSelected ? Color(nsColor: .quaternarySystemFill) : Color.clear)
    }

    var body: some View {
        Group {
            if isExpanded {
                HStack(spacing: 9) {
                    ProjectBadge(project: project, isSelected: isSelected, size: 38, activityPhase: activityPhase)

                    VStack(alignment: .leading, spacing: 4) {
                        Text(project.name)
                            .font(.system(size: 13, weight: .semibold, design: .rounded))
                            .foregroundStyle(isSelected ? AppTheme.textPrimary : AppTheme.textSecondary.opacity(0.9))
                            .lineLimit(1)

                        Text(project.path)
                            .font(.system(size: 11, weight: .medium, design: .default))
                            .foregroundStyle(isSelected ? AppTheme.textMuted : AppTheme.textMuted.opacity(0.75))
                            .lineLimit(1)
                    }

                    Spacer(minLength: 0)
                }
            } else {
                VStack(spacing: 6) {
                    ProjectBadge(project: project, isSelected: isSelected, size: 36, activityPhase: activityPhase)
                }
            }
        }
        .padding(isExpanded ? 8 : 6)
        .frame(maxWidth: .infinity)
        .background(selectionContainer)
        .opacity(isSelected ? 1.0 : 0.8)
        .contentShape(RoundedRectangle(cornerRadius: 12, style: .continuous))
        .onTapGesture(perform: action)
        .floatingTooltip(project.name, enabled: !isExpanded, placement: .right)
    }
}

private struct ProjectBadge: View {
    let project: Project
    let isSelected: Bool
    let size: CGFloat
    let activityPhase: ProjectActivityPhase

    private var initials: String {
        if let badgeText = project.badgeText?.trimmingCharacters(in: .whitespacesAndNewlines), !badgeText.isEmpty {
            return String(badgeText.prefix(2)).uppercased()
        }
        let words = project.name.split(separator: " ")
        let letters = words.prefix(2).compactMap { $0.first }
        let value = String(letters)
        return value.isEmpty ? String(project.name.prefix(1)).uppercased() : value.uppercased()
    }

    private var badgeColor: Color {
        Color(hexString: project.badgeColorHex ?? (isSelected ? "#6B2D73" : "#2E2236"))
    }

    private var badgeGradient: LinearGradient {
        LinearGradient(
            colors: isSelected
                ? [badgeColor.opacity(0.94), badgeColor.opacity(0.80)]
                : [badgeColor.opacity(0.80), badgeColor.opacity(0.66)],
            startPoint: .topLeading,
            endPoint: .bottomTrailing
        )
    }

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: size * 0.22, style: .continuous)
                .fill(badgeGradient)
                .shadow(color: Color.black.opacity(isSelected ? 0.12 : 0.05), radius: isSelected ? 8 : 4, y: 1)

            if let symbol = project.badgeSymbol {
                Image(systemName: symbol)
                    .font(.system(size: size * 0.34, weight: .bold))
                    .foregroundStyle(Color.white.opacity(isSelected ? 0.96 : 0.88))
            } else {
                Text(initials)
                    .font(.system(size: size * 0.34, weight: .bold, design: .rounded))
                    .foregroundStyle(Color.white.opacity(isSelected ? 0.96 : 0.88))
            }
        }
        .frame(width: size, height: size)
        .overlay(alignment: .topTrailing) {
            ActivityBadgeView(phase: activityPhase)
                .offset(x: 4, y: -4)
        }
    }
}

private struct ActivityBadgeView: View {
    let phase: ProjectActivityPhase
    @State private var rotation = 0.0

    var body: some View {
        Group {
            switch phase {
            case .idle:
                EmptyView()
            case .running:
                Circle()
                    .stroke(Color.white.opacity(0.16), lineWidth: 1.6)
                    .overlay(
                        Circle()
                            .trim(from: 0.14, to: 0.76)
                            .stroke(Color.white, style: StrokeStyle(lineWidth: 1.8, lineCap: .round))
                            .rotationEffect(.degrees(rotation))
                    )
                    .overlay(
                        Circle()
                            .fill(AppTheme.warning)
                            .frame(width: 5, height: 5)
                    )
                    .frame(width: 12, height: 12)
                    .background(Color.black.opacity(0.28))
                    .clipShape(Circle())
                    .onAppear {
                        rotation = 0
                        withAnimation(.linear(duration: 0.9).repeatForever(autoreverses: false)) {
                            rotation = 360
                        }
                    }
            case .completed:
                Circle()
                    .fill(Color(hex: 0x31C46B))
                    .frame(width: 10, height: 10)
                    .overlay(
                        Circle().stroke(Color.white.opacity(0.18), lineWidth: 1)
                    )
            }
        }
    }
}

private struct SidebarFooterButton: View {
    let symbol: String
    let title: String
    let isExpanded: Bool
    let action: () -> Void
    @State private var isHovered = false

    var body: some View {
        Button(action: action) {
            SidebarFooterLabel(
                symbol: symbol,
                title: title,
                isExpanded: isExpanded,
                isHovered: isHovered
            )
        }
        .frame(maxWidth: .infinity, alignment: isExpanded ? .leading : .center)
        .buttonStyle(.plain)
        .floatingTooltip(title, enabled: !isExpanded, placement: .right)
        .onHover { hovering in
            isHovered = hovering
        }
        .frame(height: 30)
    }
}

private struct SidebarFooterLabel: View {
    let symbol: String
    let title: String
    let isExpanded: Bool
    let isHovered: Bool

    var body: some View {
        if isExpanded {
            HStack(spacing: 10) {
                Image(systemName: symbol)
                    .font(.system(size: 16, weight: .regular))
                    .frame(width: 18, height: 18)

                Text(title)
                    .font(.system(size: 13, weight: .medium, design: .rounded))
                    .lineLimit(1)

                Spacer(minLength: 0)
            }
            .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
            .padding(.horizontal, 14)
            .frame(height: 30)
            .frame(maxWidth: .infinity, alignment: .leading)
            .background(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)
            .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            .contentShape(Rectangle())
        } else {
            Image(systemName: symbol)
                .font(.system(size: 16, weight: .regular))
                .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
                .frame(width: 28, height: 28)
                .background(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)
                .clipShape(RoundedRectangle(cornerRadius: 7, style: .continuous))
                .contentShape(Rectangle())
        }
    }
}
