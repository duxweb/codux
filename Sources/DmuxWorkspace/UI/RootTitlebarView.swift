import AppKit
import SwiftUI

struct TitlebarOverlayView: View {
    let model: AppModel
    @State private var isShowingLevelPopover = false
    @State private var isShowingPetPopover = false
    @State private var isShowingRemotePopover = false

    var body: some View {
        ZStack {
            TitlebarZoomSurface()

            HStack(spacing: 0) {
                HStack(spacing: 10) {
                    TitlebarGlyphButton(symbol: model.isSidebarExpanded ? "sidebar.left" : "sidebar.right", help: String(localized: "titlebar.projects", defaultValue: "Projects", bundle: .module)) {
                        model.toggleSidebarExpansion()
                    }

                    TitlebarGlyphButton(symbol: "rectangle.split.2x1", help: String(localized: "titlebar.split", defaultValue: "Split", bundle: .module)) {
                        model.splitSelectedPane(axis: .horizontal)
                    }

                    if model.appSettings.developer.showsPerformanceMonitor {
                        TitlebarPerformanceMonitorView(model: model)
                    }

                    TitlebarMemoryStatusView(snapshot: model.memoryExtractionStatus)
                    TitlebarRemoteStatusButton(
                        model: model,
                        isShowingPopover: $isShowingRemotePopover
                    )
                }
                .padding(.leading, 86)
                .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
                .frame(maxHeight: .infinity, alignment: .leading)

                Spacer(minLength: 0)

                HStack(spacing: 10) {
                    let hasVSCode = ApplicationIconAsset.isInstalled("com.microsoft.VSCode")

                    if model.appSettings.pet.enabled {
                        TitlebarPetButtonContainer(
                            model: model,
                            isShowingPopover: $isShowingPetPopover
                        )
                    }

                    if model.selectedProject != nil {
                        TitlebarAITodayLevelButtonContainer(
                            model: model,
                            isShowingPopover: $isShowingLevelPopover
                        )
                    }

                    TitlebarOpenSplitButton(
                        model: model,
                        isEnabled: model.selectedProject != nil,
                        prefersVSCode: hasVSCode,
                        primaryAction: {
                            if hasVSCode {
                                model.openSelectedProjectInVSCode()
                            } else {
                                model.revealSelectedProjectInFinder()
                            }
                        },
                        revealInFinder: { model.revealSelectedProjectInFinder() },
                        openInVSCode: { model.openSelectedProjectInVSCode() },
                        openInTerminal: { model.openSelectedProjectInTerminal() },
                        openInITerm2: { model.openSelectedProjectInITerm2() },
                        openInGhostty: { model.openSelectedProjectInGhostty() },
                        openInXcode: { model.openSelectedProjectInXcode() }
                    )

                    TitlebarGlyphButton(symbol: "terminal", help: String(localized: "titlebar.tab", defaultValue: "Tab", bundle: .module)) {
                        model.createBottomTab()
                    }

                    TitlebarGlyphButton(symbol: "chart.bar.xaxis", help: String(localized: "titlebar.ai_assistant", defaultValue: "AI Assistant", bundle: .module)) {
                        model.toggleRightPanel(.aiStats)
                    }

                    TitlebarGlyphButton(symbol: "point.3.filled.connected.trianglepath.dotted", help: String(localized: "titlebar.git", defaultValue: "Git", bundle: .module)) {
                        model.toggleRightPanel(.git)
                    }
                }
                .padding(.trailing, 16)
                .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
                .frame(maxHeight: .infinity, alignment: .center)
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)

            ProjectTitleView(project: model.selectedProject)
                .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
                .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .center)
        .background(Color.clear)
        .onChange(of: model.selectedProjectID) { _, _ in
            isShowingLevelPopover = false
        }
    }
}

private struct TitlebarPetButtonContainer: View {
    let model: AppModel
    @Binding var isShowingPopover: Bool

    var body: some View {
        TitlebarPetButton(
            model: model,
            isShowingPopover: $isShowingPopover
        )
    }
}

private struct TitlebarAITodayLevelButtonContainer: View {
    let model: AppModel
    @Binding var isShowingPopover: Bool

    private var totalTodayTokens: Int {
        model.aiStatsStore.titlebarTodayLevelTokens()
    }

    var body: some View {
        let _ = model.aiStatsStore.renderVersion
        TitlebarAITodayLevelButton(
            model: model,
            tokens: totalTodayTokens,
            isShowingPopover: $isShowingPopover
        )
    }
}

private enum TitlebarControlMetrics {
    static let rowHeight: CGFloat = 30
    static let iconButtonSize: CGFloat = 30
    static let glyphIconSize: CGFloat = 15
    static let pillHeight: CGFloat = 26
    static let glyphCornerRadius: CGFloat = 7
    static let pillCornerRadius: CGFloat = 9
}

private struct TitlebarPerformanceMonitorView: View {
    let model: AppModel

    private var cpuColor: Color {
        if model.performanceMonitor.cpuPercent >= 85 {
            return AppTheme.warning
        }
        if model.performanceMonitor.cpuPercent >= 60 {
            return AppTheme.warning.opacity(0.78)
        }
        return AppTheme.textSecondary
    }

    var body: some View {
        let _ = model.performanceMonitor.renderVersion
        HStack(spacing: 8) {
            HStack(spacing: 4) {
                Image(systemName: "cpu")
                    .font(.system(size: 11, weight: .semibold))
                Text(model.performanceMonitor.formattedCPU)
            }
            .foregroundStyle(cpuColor)

            Rectangle()
                .fill(AppTheme.titlebarControlBorder)
                .frame(width: 0.5, height: 12)

            HStack(spacing: 4) {
                Image(systemName: "memorychip")
                    .font(.system(size: 11, weight: .semibold))
                Text(model.performanceMonitor.formattedMemory)
            }
            .foregroundStyle(AppTheme.textSecondary)

            Rectangle()
                .fill(AppTheme.titlebarControlBorder)
                .frame(width: 0.5, height: 12)

            HStack(spacing: 4) {
                Image(systemName: "display")
                    .font(.system(size: 11, weight: .semibold))
                Text(model.performanceMonitor.formattedGraphics)
            }
            .foregroundStyle(AppTheme.textSecondary)
        }
        .font(.system(size: 11.5, weight: .medium, design: .monospaced))
        .environment(\.symbolVariants, .none)
        .padding(.leading, 9)
        .padding(.trailing, 10)
        .frame(height: TitlebarControlMetrics.pillHeight)
        .background(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .fill(AppTheme.emphasizedControlFill)
        )
        .overlay(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .stroke(AppTheme.titlebarControlBorder, lineWidth: 0.5)
        )
        .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
        .floatingTooltip(
            String(localized: "settings.developer.performance_monitor", defaultValue: "Performance Monitor HUD", bundle: .module),
            placement: .below
        )
    }
}

private struct TitlebarMemoryStatusView: View {
    let snapshot: MemoryExtractionStatusSnapshot

    private var isActive: Bool {
        snapshot.status == .queued || snapshot.status == .processing
    }

    private var statusColor: Color {
        switch snapshot.status {
        case .processing:
            return AppTheme.focus
        case .queued:
            return AppTheme.warning
        case .failed:
            return Color(hex: 0xFF5E6C)
        case .idle:
            return AppTheme.textSecondary
        }
    }

    private var statusText: String {
        switch snapshot.status {
        case .processing:
            return String(localized: "memory.status.processing", defaultValue: "Remembering", bundle: .module)
        case .queued:
            return String(localized: "memory.status.queued", defaultValue: "Memory queued", bundle: .module)
        case .failed:
            return String(localized: "memory.status.failed", defaultValue: "Memory failed", bundle: .module)
        case .idle:
            return String(localized: "memory.status.idle", defaultValue: "Memory idle", bundle: .module)
        }
    }

    var body: some View {
        ZStack {
            Image(systemName: "brain.head.profile")
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(statusColor)

            if isActive {
                ProgressView()
                    .controlSize(.small)
                    .scaleEffect(0.58)
                    .frame(width: 22, height: 22)
                    .offset(x: 9, y: -9)
            }
        }
        .frame(width: TitlebarControlMetrics.pillHeight, height: TitlebarControlMetrics.pillHeight)
        .frame(height: TitlebarControlMetrics.pillHeight)
        .background(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .fill(statusColor.opacity(isActive ? 0.12 : 0.07))
        )
        .overlay(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .stroke(statusColor.opacity(isActive ? 0.28 : 0.14), lineWidth: 0.5)
        )
        .floatingTooltip(tooltipText, placement: .below)
        .accessibilityLabel(statusText)
    }

    private var tooltipText: String {
        if snapshot.status == .failed, let error = snapshot.lastError, !error.isEmpty {
            return "\(statusText): \(error)"
        }
        if snapshot.pendingCount > 0 || snapshot.runningCount > 0 {
            let format = String(localized: "memory.status.detail", defaultValue: "Memory queue: %lld pending, %lld running", bundle: .module)
            return String(format: format, Int64(snapshot.pendingCount), Int64(snapshot.runningCount))
        }
        return statusText
    }
}

private struct TitlebarRemoteStatusButton: View {
    let model: AppModel
    @ObservedObject private var remoteHostService: RemoteHostService
    @Binding var isShowingPopover: Bool
    @State private var isHovered = false

    init(model: AppModel, isShowingPopover: Binding<Bool>) {
        self.model = model
        self.remoteHostService = model.remoteHostService
        self._isShowingPopover = isShowingPopover
    }

    private var statusColor: Color {
        if model.appSettings.remote.isEnabled == false
            || model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        {
            return AppTheme.textSecondary
        }
        switch remoteHostService.snapshot.status {
        case .connected:
            return Color(hex: 0x39D98A)
        case .registering, .connecting:
            return AppTheme.warning
        case .failed:
            return Color(hex: 0xFF5E6C)
        case .stopped:
            return AppTheme.textSecondary
        }
    }

    private var statusText: String {
        if model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            return String(localized: "remote.status.not_configured", defaultValue: "Remote not configured", bundle: .module)
        }
        if model.appSettings.remote.isEnabled == false {
            return String(localized: "remote.status.disabled", defaultValue: "Remote disabled", bundle: .module)
        }
        switch remoteHostService.snapshot.status {
        case .connected:
            return String(localized: "remote.status.connected_short", defaultValue: "Remote connected", bundle: .module)
        case .registering, .connecting:
            return String(localized: "remote.status.connecting_short", defaultValue: "Remote connecting", bundle: .module)
        case .failed:
            return String(localized: "remote.status.failed_short", defaultValue: "Remote failed", bundle: .module)
        case .stopped:
            return String(localized: "remote.status.stopped_short", defaultValue: "Remote stopped", bundle: .module)
        }
    }

    private var compactStatusText: String {
        if model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || model.appSettings.remote.isEnabled == false
        {
            return String(localized: "remote.status.off_label", defaultValue: "Off", bundle: .module)
        }
        switch remoteHostService.snapshot.status {
        case .connected:
            return String(localized: "remote.status.connected_label", defaultValue: "Connected", bundle: .module)
        case .registering, .connecting:
            return String(localized: "remote.status.connecting_label", defaultValue: "Connecting", bundle: .module)
        case .failed:
            return String(localized: "remote.status.failed_label", defaultValue: "Error", bundle: .module)
        case .stopped:
            return String(localized: "remote.status.off_label", defaultValue: "Off", bundle: .module)
        }
    }

    private var onlineDeviceCount: Int {
        remoteHostService.snapshot.devices.filter { $0.online == true }.count
    }

    private var hasMeaningfulStatus: Bool {
        guard model.appSettings.remote.isEnabled,
              !model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        else { return false }
        switch remoteHostService.snapshot.status {
        case .connected, .registering, .connecting, .failed: return true
        case .stopped: return false
        }
    }

    private var iconColor: Color {
        if hasMeaningfulStatus { return statusColor }
        return isHovered || isShowingPopover ? AppTheme.textPrimary : AppTheme.textSecondary
    }

    var body: some View {
        Button {
            isShowingPopover.toggle()
            if isShowingPopover {
                remoteHostService.refreshDevices()
            }
        } label: {
            HStack(spacing: 6) {
                Image(systemName: "antenna.radiowaves.left.and.right")
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(iconColor)
                    .frame(width: 14, height: TitlebarControlMetrics.pillHeight)

                Text(compactStatusText)
                    .font(.system(size: 11.5, weight: .semibold))
                    .foregroundStyle(isHovered || isShowingPopover ? AppTheme.textPrimary : AppTheme.textSecondary)
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .padding(.horizontal, 9)
            .frame(height: TitlebarControlMetrics.pillHeight)
            .background(
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                    .fill(statusColor.opacity(isHovered || isShowingPopover ? 0.13 : 0.07))
            )
            .overlay(
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                    .stroke(statusColor.opacity(isHovered || isShowingPopover ? 0.3 : 0.14), lineWidth: 0.5)
            )
        }
        .buttonStyle(.plain)
        .frame(height: TitlebarControlMetrics.rowHeight)
        .floatingTooltip(tooltipText, enabled: !isShowingPopover, placement: .below)
        .popover(isPresented: $isShowingPopover, attachmentAnchor: .rect(.bounds), arrowEdge: .top) {
            RemoteStatusPopover(model: model, service: remoteHostService)
        }
        .onHover { isHovered = $0 }
        .accessibilityLabel(statusText)
    }

    private var tooltipText: String {
        if onlineDeviceCount > 0 {
            let format = String(
                localized: "remote.devices.online_count_format",
                defaultValue: "%lld mobile device(s) online",
                bundle: .module)
            return "\(statusText) · \(String(format: format, Int64(onlineDeviceCount)))"
        }
        return statusText
    }
}

private struct RemoteStatusPopover: View {
    let model: AppModel
    @ObservedObject var service: RemoteHostService
    @Environment(\.openSettings) private var openSettings

    var body: some View {
        Group {
            if service.snapshot.devices.isEmpty {
                emptyState
            } else {
                ScrollView {
                    VStack(spacing: 6) {
                        ForEach(service.snapshot.devices) { device in
                            RemoteDeviceStatusRow(device: device)
                        }
                    }
                    .padding(10)
                }
                .frame(width: 280)
                .frame(maxHeight: 280)
            }
        }
    }

    private var emptyState: some View {
        VStack(spacing: 14) {
            ZStack {
                Circle()
                    .fill(AppTheme.focus.opacity(0.12))
                    .frame(width: 56, height: 56)
                Image(systemName: "iphone.gen3.radiowaves.left.and.right")
                    .font(.system(size: 22, weight: .regular))
                    .foregroundStyle(AppTheme.focus)
            }

            VStack(spacing: 4) {
                Text(String(localized: "remote.devices.empty", defaultValue: "No paired devices", bundle: .module))
                    .font(.system(size: 13, weight: .semibold))
                    .foregroundStyle(AppTheme.textPrimary)
                Text(String(localized: "remote.devices.empty_hint", defaultValue: "Pair a phone to control terminals on the go.", bundle: .module))
                    .font(.system(size: 11.5))
                    .foregroundStyle(AppTheme.textSecondary)
                    .multilineTextAlignment(.center)
                    .fixedSize(horizontal: false, vertical: true)
            }

            Button {
                SettingsNavigationRequest.request(.remote)
                openSettings()
            } label: {
                Label(
                    String(localized: "remote.devices.add", defaultValue: "Add Device", bundle: .module),
                    systemImage: "qrcode.viewfinder"
                )
                .font(.system(size: 12.5, weight: .semibold))
                .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .controlSize(.regular)
        }
        .padding(.horizontal, 18)
        .padding(.top, 22)
        .padding(.bottom, 18)
        .frame(width: 280)
    }
}

private struct RemoteDeviceStatusRow: View {
    let device: RemoteHostDevice

    private var isOnline: Bool {
        device.online == true
    }

    var body: some View {
        HStack(spacing: 9) {
            Circle()
                .fill(isOnline ? Color(hex: 0x39D98A) : AppTheme.textSecondary.opacity(0.55))
                .frame(width: 7, height: 7)
            VStack(alignment: .leading, spacing: 2) {
                Text(device.name.isEmpty ? device.id : device.name)
                    .font(.system(size: 12.5, weight: .medium))
                    .foregroundStyle(AppTheme.textPrimary)
                    .lineLimit(1)
                HStack(spacing: 6) {
                    Text(isOnline
                        ? String(localized: "remote.devices.online", defaultValue: "Online", bundle: .module)
                        : String(localized: "remote.devices.offline", defaultValue: "Offline", bundle: .module))
                    Text("·")
                    Text(String(localized: "remote.devices.last_seen", defaultValue: "Last seen", bundle: .module))
                    Text(device.lastSeen, style: .relative)
                }
                .font(.system(size: 10.5))
                .foregroundStyle(AppTheme.textSecondary)
            }
            Spacer()
        }
        .padding(.horizontal, 9)
        .padding(.vertical, 7)
        .background(
            RoundedRectangle(cornerRadius: 9, style: .continuous)
                .fill(isOnline ? Color(hex: 0x39D98A).opacity(0.08) : Color(nsColor: .quaternarySystemFill).opacity(0.55))
        )
    }
}

private struct TitlebarGlyphButton: View {
    let symbol: String
    let help: String
    let action: () -> Void
    @State private var isHovered = false

    private var opticalOffset: CGFloat {
        switch symbol {
        case "terminal":
            return -0.5
        case "rectangle.split.2x1", "rectangle.split.1x2":
            return 0
        case "sidebar.left", "sidebar.right":
            return 0.25
        case "scroll":
            return -0.25
        default:
            return 0
        }
    }

    var body: some View {
        Button(action: action) {
            ZStack {
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.glyphCornerRadius, style: .continuous)
                    .fill(isHovered ? Color(nsColor: .quaternarySystemFill) : Color.clear)

                Image(systemName: symbol)
                    .font(.system(size: TitlebarControlMetrics.glyphIconSize, weight: .regular))
                    .foregroundStyle(isHovered ? AppTheme.textPrimary : AppTheme.textSecondary)
                    .offset(y: opticalOffset)
                    .frame(width: TitlebarControlMetrics.glyphIconSize, height: TitlebarControlMetrics.glyphIconSize)
            }
            .frame(width: TitlebarControlMetrics.iconButtonSize, height: TitlebarControlMetrics.iconButtonSize)
            .contentShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.glyphCornerRadius, style: .continuous))
        }
        .frame(width: TitlebarControlMetrics.iconButtonSize, height: TitlebarControlMetrics.iconButtonSize)
        .buttonStyle(.plain)
        .floatingTooltip(help, placement: .below)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}

private struct TitlebarOpenSplitButton: View {
    let model: AppModel
    let isEnabled: Bool
    let prefersVSCode: Bool
    let primaryAction: () -> Void
    let revealInFinder: () -> Void
    let openInVSCode: () -> Void
    let openInTerminal: () -> Void
    let openInITerm2: () -> Void
    let openInGhostty: () -> Void
    let openInXcode: () -> Void

    @State private var isHovered = false

    private let primaryWidth: CGFloat = 30
    private let menuWidth: CGFloat = 22

    var body: some View {
        HStack(spacing: 0) {
            Button(action: primaryAction) {
                PrimaryOpenIconView(prefersVSCode: prefersVSCode)
                    .frame(width: 16, height: 16)
                    .frame(width: primaryWidth, height: TitlebarControlMetrics.pillHeight)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(!isEnabled)

            Rectangle()
                .fill(Color(nsColor: .separatorColor).opacity(isHovered ? 0.42 : 0.3))
                .frame(width: 0.5, height: 16)

            Menu {
                Button {
                    openInVSCode()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.vscode", defaultValue: "Open in VS Code", bundle: .module),
                        icon: .bundle("com.microsoft.VSCode", fallbackSystemName: "chevron.left.forwardslash.chevron.right")
                    )
                }

                Button {
                    revealInFinder()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.finder", defaultValue: "Open in Finder", bundle: .module),
                        icon: .bundle("com.apple.finder", fallbackSystemName: "folder")
                    )
                }

                Button {
                    openInTerminal()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.terminal", defaultValue: "Open in Terminal", bundle: .module),
                        icon: .bundle("com.apple.Terminal", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInITerm2()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.iterm2", defaultValue: "Open in iTerm2", bundle: .module),
                        icon: .bundle("com.googlecode.iterm2", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInGhostty()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.ghostty", defaultValue: "Open in Ghostty", bundle: .module),
                        icon: .bundle("com.mitchellh.ghostty", fallbackSystemName: "terminal")
                    )
                }

                Button {
                    openInXcode()
                } label: {
                    AppLauncherMenuLabel(
                        title: String(localized: "open.xcode", defaultValue: "Open in Xcode", bundle: .module),
                        icon: .bundle("com.apple.dt.Xcode", fallbackSystemName: "hammer")
                    )
                }
            } label: {
                Image(systemName: "chevron.down")
                    .font(.system(size: 9, weight: .bold))
                    .foregroundStyle(isEnabled ? AppTheme.textSecondary : AppTheme.textMuted)
                    .frame(width: menuWidth, height: TitlebarControlMetrics.pillHeight)
                    .contentShape(Rectangle())
            }
            .menuStyle(.borderlessButton)
            .menuIndicator(.hidden)
            .disabled(!isEnabled)
        }
        .frame(width: primaryWidth + menuWidth + 1, height: TitlebarControlMetrics.pillHeight)
        .background(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .fill(isHovered ? AppTheme.titlebarControlHoverFill : AppTheme.emphasizedControlFill)
        )
        .overlay(
            RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                .stroke(isHovered ? AppTheme.titlebarControlHoverBorder : AppTheme.titlebarControlBorder, lineWidth: 0.5)
        )
        .clipShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
        .floatingTooltip(
            prefersVSCode
            ? String(localized: "open.project.vscode", defaultValue: "Open Project in VS Code", bundle: .module)
            : String(localized: "open.project.finder", defaultValue: "Open Project in Finder", bundle: .module),
            placement: .below
        )
        .onHover { hovering in
            isHovered = hovering
        }
        .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
        .opacity(isEnabled ? 1 : 0.6)
    }
}

private struct TitlebarAITodayLevelButton: View {
    let model: AppModel
    let tokens: Int
    @Binding var isShowingPopover: Bool
    @State private var isHovered = false

    private var level: AITodayLevelTier {
        AITodayLevelTier.current(for: tokens)
    }

    var body: some View {
        Button {
            isShowingPopover.toggle()
        } label: {
            HStack(alignment: .center, spacing: 6) {
                AITodayLevelBadge(level: level, size: 19, compact: true)

                Text(level.localizedTitle(using: model))
                    .font(.system(size: 12.5, weight: .semibold, design: .rounded))
                    .foregroundStyle(AppTheme.textPrimary.opacity(isShowingPopover || isHovered ? 1 : 0.9))
                    .lineLimit(1)
                    .fixedSize(horizontal: true, vertical: false)
            }
            .padding(.leading, 8)
            .padding(.trailing, 10)
            .frame(height: TitlebarControlMetrics.pillHeight)
            .background(
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                    .fill(
                        isShowingPopover
                        ? level.accent.opacity(0.2)
                        : (isHovered ? level.accent.opacity(0.13) : AppTheme.emphasizedControlFill)
                    )
            )
            .overlay(
                RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous)
                    .stroke(
                        isShowingPopover
                        ? level.accent.opacity(0.3)
                        : (isHovered ? AppTheme.titlebarControlHoverBorder : AppTheme.titlebarControlBorder),
                        lineWidth: 0.5
                    )
            )
            .clipShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
            .contentShape(RoundedRectangle(cornerRadius: TitlebarControlMetrics.pillCornerRadius, style: .continuous))
        }
        .fixedSize(horizontal: true, vertical: false)
        .buttonStyle(.plain)
        .popover(isPresented: $isShowingPopover, attachmentAnchor: .rect(.bounds), arrowEdge: .top) {
            AITodayLevelPopover(model: model, tokens: tokens, currentLevel: level)
        }
        .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
        .floatingTooltip(String(localized: "ai.today_level", defaultValue: "Today's Level", bundle: .module), enabled: !isShowingPopover, placement: .below)
        .onHover { hovering in
            isHovered = hovering
        }
    }
}

private struct AITodayLevelPopover: View {
    let model: AppModel
    let tokens: Int
    let currentLevel: AITodayLevelTier

    var body: some View {
        VStack(alignment: .leading, spacing: 14) {
            HStack(alignment: .center, spacing: 10) {
                AITodayLevelBadge(level: currentLevel, size: 34)

                VStack(alignment: .leading, spacing: 2) {
                    Text(String(localized: "ai.today_level", defaultValue: "Today's Level", bundle: .module))
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(.tertiary)

                    Text(currentLevel.localizedTitle(using: model))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)
                }

                Spacer(minLength: 12)

                VStack(alignment: .trailing, spacing: 2) {
                    Text(String(localized: "ai.today_tokens", defaultValue: "Today's Tokens", bundle: .module))
                        .font(.system(size: 11, weight: .medium))
                        .foregroundStyle(.tertiary)

                    Text(formatCompactToken(tokens))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)
                }
            }

            VStack(spacing: 6) {
                ForEach(AITodayLevelTier.allCases) { level in
                    let isCurrent = level == currentLevel

                    HStack(spacing: 10) {
                        AITodayLevelBadge(level: level, size: 24)

                        VStack(alignment: .leading, spacing: 1) {
                            Text(level.localizedTitle(using: model))
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(.primary)

                            Text(String(format: String(localized: "common.need_format", defaultValue: "Need %@", bundle: .module), formatCompactToken(level.minimumTokens)))
                                .font(.system(size: 11, weight: .medium))
                                .foregroundStyle(.tertiary)
                        }

                        Spacer(minLength: 12)

                        if isCurrent {
                            Text(String(localized: "common.current", defaultValue: "Current", bundle: .module))
                                .font(.system(size: 11, weight: .bold, design: .rounded))
                                .foregroundStyle(level.accent)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(
                                    Capsule()
                                        .fill(level.accent.opacity(0.16))
                                )
                        }
                    }
                    .padding(.horizontal, 10)
                    .padding(.vertical, 8)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .background(
                        RoundedRectangle(cornerRadius: 10, style: .continuous)
                            .fill(isCurrent ? level.accent.opacity(0.1) : Color.clear)
                    )
                }
            }
        }
        .padding(14)
        .frame(width: 280)
    }
}

private struct AITodayLevelBadge: View {
    let level: AITodayLevelTier
    let size: CGFloat
    var compact = false

    var body: some View {
        ZStack {
            Circle()
                .fill(
                    LinearGradient(
                        colors: [level.accent, level.accent.adjustingBrightness(-0.25)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )

            Image(systemName: level.symbol)
                .font(.system(size: size * 0.4, weight: .bold))
                .foregroundStyle(.white.opacity(0.95))
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
    }
}

private enum AITodayLevelTier: String, CaseIterable, Identifiable {
    case blankSlate
    case bronze
    case silver
    case gold
    case platinum
    case diamond
    case master
    case grandmaster

    var id: String { rawValue }
    static let topThreshold = 50_000_000

    @MainActor
    func localizedTitle(using model: AppModel) -> String {
        switch self {
        case .blankSlate: return String(localized: "rank.iron", defaultValue: "Idle", bundle: .module)
        case .bronze: return String(localized: "rank.bronze", defaultValue: "Light", bundle: .module)
        case .silver: return String(localized: "rank.silver", defaultValue: "Active", bundle: .module)
        case .gold: return String(localized: "rank.gold", defaultValue: "Focus", bundle: .module)
        case .platinum: return String(localized: "rank.platinum", defaultValue: "Intense", bundle: .module)
        case .diamond: return String(localized: "rank.diamond", defaultValue: "Grind", bundle: .module)
        case .master: return String(localized: "rank.master", defaultValue: "Limit", bundle: .module)
        case .grandmaster: return String(localized: "rank.grandmaster", defaultValue: "Godlike", bundle: .module)
        }
    }

    var minimumTokens: Int {
        switch self {
        case .blankSlate: return 0
        case .bronze: return 1_000_000
        case .silver: return 3_000_000
        case .gold: return 6_000_000
        case .platinum: return 10_000_000
        case .diamond: return 18_000_000
        case .master: return 30_000_000
        case .grandmaster: return Self.topThreshold
        }
    }

    var accent: Color {
        switch self {
        case .blankSlate: return Color(hex: 0x5B616D)
        case .bronze: return Color(hex: 0xC98663)
        case .silver: return Color(hex: 0xC8D1E3)
        case .gold:
            return Color(
                nsColor: NSColor(name: nil) { appearance in
                    let isDark = appearance.bestMatch(from: [.darkAqua, .aqua]) == .darkAqua
                    if isDark {
                        return NSColor(calibratedRed: 232 / 255, green: 170 / 255, blue: 52 / 255, alpha: 1)
                    }
                    return NSColor(calibratedRed: 221 / 255, green: 126 / 255, blue: 27 / 255, alpha: 1)
                }
            )
        case .platinum: return Color(hex: 0x7ED6D8)
        case .diamond: return Color(hex: 0x59A7FF)
        case .master: return Color(hex: 0x9A72FF)
        case .grandmaster: return Color(hex: 0xFF5E8E)
        }
    }

    var symbol: String {
        switch self {
        case .blankSlate: return "minus"
        case .bronze: return "bolt.fill"
        case .silver: return "shield.fill"
        case .gold: return "star.fill"
        case .platinum: return "star.circle.fill"
        case .diamond: return "diamond.fill"
        case .master: return "crown.fill"
        case .grandmaster: return "crown.fill"
        }
    }

    static func current(for tokens: Int) -> AITodayLevelTier {
        let clamped = max(0, tokens)
        return allCases.last(where: { clamped >= $0.minimumTokens }) ?? .blankSlate
    }
}

private func formatCompactToken(_ value: Int) -> String {
    let clamped = max(0, value)
    if clamped >= 1_000_000 {
        let formatted = Double(clamped) / 1_000_000
        return formatted >= 10 ? "\(Int(formatted.rounded(.down)))M" : String(format: "%.1fM", formatted)
    }
    if clamped >= 1_000 {
        let formatted = Double(clamped) / 1_000
        return formatted >= 10 ? "\(Int(formatted.rounded(.down)))K" : String(format: "%.1fK", formatted)
    }
    return "\(clamped)"
}

private enum AppLauncherMenuIcon {
    case bundle(String, fallbackSystemName: String)
}

private struct AppLauncherMenuLabel: View {
    let title: String
    let icon: AppLauncherMenuIcon

    var body: some View {
        HStack(spacing: 10) {
            switch icon {
            case let .bundle(bundleIdentifier, fallbackSystemName):
                ApplicationIconView(bundleIdentifier: bundleIdentifier, fallbackSystemName: fallbackSystemName)
            }

            Text(title)
        }
    }
}

private struct ApplicationIconView: View {
    let bundleIdentifier: String
    let fallbackSystemName: String

    var body: some View {
        Group {
            if let image = ApplicationIconAsset.image(for: bundleIdentifier) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
            } else {
                Image(systemName: fallbackSystemName)
                    .font(.system(size: 12, weight: .semibold))
                    .foregroundStyle(AppTheme.textSecondary)
            }
        }
        .frame(width: 18, height: 18)
    }
}

private enum ApplicationIconAsset {
    static func image(for bundleIdentifier: String, size: CGFloat = 18) -> NSImage? {
        guard let appURL = NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) else {
            return nil
        }
        let image = NSWorkspace.shared.icon(forFile: appURL.path)
        image.size = NSSize(width: size, height: size)
        return image
    }

    static func isInstalled(_ bundleIdentifier: String) -> Bool {
        NSWorkspace.shared.urlForApplication(withBundleIdentifier: bundleIdentifier) != nil
    }
}

private struct PrimaryOpenIconView: View {
    let prefersVSCode: Bool

    var body: some View {
        Group {
            if prefersVSCode, let image = ApplicationIconAsset.image(for: "com.microsoft.VSCode", size: 16) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
            } else if let image = ApplicationIconAsset.image(for: "com.apple.finder", size: 16) {
                Image(nsImage: image)
                    .resizable()
                    .interpolation(.high)
                    .antialiased(true)
            } else {
                Image(systemName: "folder")
                    .font(.system(size: 12, weight: .semibold))
            }
        }
    }
}

private struct ProjectTitleView: View {
    let project: Project?

    var body: some View {
        Text(project?.name ?? "Codux")
            .font(.system(size: 13, weight: .semibold, design: .rounded))
            .foregroundStyle(AppTheme.textPrimary)
            .lineLimit(1)
            .frame(maxWidth: 260)
            .frame(height: TitlebarControlMetrics.rowHeight, alignment: .center)
    }
}
