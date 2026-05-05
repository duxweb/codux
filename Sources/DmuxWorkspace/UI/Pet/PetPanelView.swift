import SwiftUI

// MARK: - Sprite Animation View

struct PetSpriteView: View {
    let species: PetSpecies
    let stage: PetStage
    var sleeping: Bool = false
    var animationState: CodexPetAnimationState? = nil
    var staticMode = false
    let displaySize: CGFloat

    @State private var frame: Int = 0
    @State private var loadedCodexAtlas: NSImage? = nil
    @State private var activeFrameCount: Int = 1

    private var codexAnimationState: CodexPetAnimationState {
        animationState ?? (sleeping ? .waiting : .idle)
    }

    var body: some View {
        ZStack {
            if let atlas = loadedCodexAtlas {
                codexAtlasFrame(atlas)
            } else {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(stage.accentColor.opacity(0.12))
                    .frame(width: displaySize, height: displaySize)
                Image(systemName: species.placeholderSymbol)
                    .font(.system(size: displaySize * 0.34, weight: .semibold))
                    .foregroundStyle(stage.accentColor.opacity(0.7))
            }
        }
        .task(id: "\(species.rawValue)-\(codexAnimationState.rawValue)-\(staticMode)") {
            frame = 0
            let codexAtlas = loadCodexAtlas()
            loadedCodexAtlas = codexAtlas
            let animation = CodexPetAtlasSpec.animation(for: codexAnimationState)
            let resolvedActiveFrameCount = codexAtlas.map {
                codexAtlasActiveFrameCount($0, row: animation.row, fallback: animation.frameCount)
            } ?? animation.frameCount
            activeFrameCount = resolvedActiveFrameCount
            guard !staticMode else {
                return
            }
            while !Task.isCancelled {
                let delay = CodexPetPlaybackPolicy.frameDuration(
                    for: animation,
                    activeFrameCount: resolvedActiveFrameCount,
                    frame: frame
                )
                try? await Task.sleep(nanoseconds: UInt64(delay * 1_000_000_000))
                frame = (frame + 1) % max(1, resolvedActiveFrameCount)
            }
        }
    }

    private func codexAtlasFrame(_ atlas: NSImage) -> some View {
        let animation = CodexPetAtlasSpec.animation(for: codexAnimationState)
        let cellWidth = CGFloat(CodexPetAtlasSpec.cellWidth)
        let cellHeight = CGFloat(CodexPetAtlasSpec.cellHeight)
        let scale = displaySize / cellHeight
        let scaledCellWidth = cellWidth * scale
        let scaledCellHeight = cellHeight * scale
        let atlasWidth = CGFloat(CodexPetAtlasSpec.atlasWidth) * scale
        let atlasHeight = CGFloat(CodexPetAtlasSpec.atlasHeight) * scale
        return Image(nsImage: atlas)
            .resizable()
            .interpolation(.medium)
            .frame(width: atlasWidth, height: atlasHeight, alignment: .topLeading)
            .offset(
                x: -scaledCellWidth * CGFloat(min(frame, max(0, activeFrameCount - 1))),
                y: -scaledCellHeight * CGFloat(animation.row)
            )
            .frame(width: scaledCellWidth, height: scaledCellHeight, alignment: .topLeading)
            .clipped()
            .frame(width: displaySize, height: displaySize)
    }

    private func loadCodexAtlas() -> NSImage? {
        guard species.isImplemented else {
            return nil
        }
        for fileExtension in ["webp", "png"] {
            if let url = Bundle.module.url(forResource: "spritesheet", withExtension: fileExtension, subdirectory: "Pets/\(species.assetFolder)"),
               let image = NSImage(contentsOf: url) {
                return image
            }
        }
        return nil
    }

    private func codexAtlasActiveFrameCount(_ atlas: NSImage, row: Int, fallback: Int) -> Int {
        guard
            let tiff = atlas.tiffRepresentation,
            let bitmap = NSBitmapImageRep(data: tiff),
            bitmap.pixelsWide >= CodexPetAtlasSpec.atlasWidth,
            bitmap.pixelsHigh >= CodexPetAtlasSpec.atlasHeight
        else {
            return fallback
        }

        var count = 0
        for column in 0..<CodexPetAtlasSpec.columns {
            if codexAtlasCellHasContent(bitmap, row: row, column: column) {
                count = column + 1
            }
        }
        return max(1, min(count, fallback))
    }

    private func codexAtlasCellHasContent(_ bitmap: NSBitmapImageRep, row: Int, column: Int) -> Bool {
        let startX = column * CodexPetAtlasSpec.cellWidth
        let startY = row * CodexPetAtlasSpec.cellHeight
        let endX = min(startX + CodexPetAtlasSpec.cellWidth, bitmap.pixelsWide)
        let endY = min(startY + CodexPetAtlasSpec.cellHeight, bitmap.pixelsHigh)

        var y = startY
        while y < endY {
            var x = startX
            while x < endX {
                if (bitmap.colorAt(x: x, y: y)?.alphaComponent ?? 0) > 0.01 {
                    return true
                }
                x += 4
            }
            y += 4
        }
        return false
    }
}

// MARK: - Titlebar Button

struct TitlebarPetButton: View {
    let model: AppModel
    @Binding var isShowingPopover: Bool
    @AppStorage("pet.last_level") private var lastLevel: Int = 0
    @AppStorage("pet.showed_max_level_effect") private var showedMaxLevelEffect: Bool = false
    @State private var isHovered = false
    @State private var appIsActive = NSApplication.shared.isActive
    @State private var recentActivityTick = Date()
    @State private var sleepClock = Date()
    @State private var lastKnownStage: PetStage? = nil
    @State private var showMaxLevelEffect = false
    @State private var showLevelUpEffect = false
    @State private var levelUpTarget: Int = 0

    private var petStore: PetStore { model.petStore }
    private var species: PetSpecies { petStore.species }
    private var evoPath: PetEvoPath { petStore.currentEvoPath() }
    private var currentXP: Int { petStore.currentExperienceTokens }
    private var info: PetProgressInfo { PetProgressInfo(totalXP: currentXP) }
    private var petStats: PetStats { petStore.currentStats }
    private var displayName: String {
        petStore.customName.isEmpty ? info.stage.speciesName(for: species, evoPath: evoPath) : petStore.customName
    }
    private var currentPhase: ProjectActivityPhase {
        guard let project = model.selectedProject else {
            return .idle
        }
        return model.activityPhase(for: project.id)
    }
    private var hasAnyRunningActivity: Bool {
        model.activityByProjectID.values.contains {
            switch $0 {
            case .loading, .running, .waitingInput:
                return true
            default:
                return false
            }
        }
    }
    private var isSleeping: Bool {
        if !appIsActive {
            return true
        }
        if hasAnyRunningActivity {
            return false
        }
        return sleepClock.timeIntervalSince(recentActivityTick) >= 30
    }

    private var titlebarAnimationState: CodexPetAnimationState {
        if isSleeping {
            return .waiting
        }
        switch currentPhase {
        case .loading:
            return .running
        case .running:
            return .running
        case .waitingInput:
            return .review
        case .completed(_, _, let exitCode):
            return exitCode == 0 || exitCode == nil ? .waving : .failed
        case .idle:
            return hasAnyRunningActivity ? .running : .idle
        }
    }

    var body: some View {
        #if SWIFT_PACKAGE
        EmptyView()
        #else
        Button {
            if petStore.isClaimed {
                isShowingPopover.toggle()
            } else {
                presentClaimDialog()
            }
        } label: {
            titlebarPill
        }
        .buttonStyle(.plain)
        .floatingTooltip(
            tooltipText,
            enabled: !isShowingPopover,
            placement: .below
        )
        .popover(isPresented: $isShowingPopover, attachmentAnchor: .rect(.bounds), arrowEdge: .top) {
            ZStack {
                PetPopoverView(
                    model: model,
                    sleeping: isSleeping,
                    petStats: petStats,
                    onInheritConfirmed: {
                        showMaxLevelEffect = false
                        showLevelUpEffect = false
                        isShowingPopover = false
                    }
                )
                if showMaxLevelEffect {
                    PetMaxLevelEffectView(
                        species: species,
                        stage: info.stage,
                        onComplete: { showMaxLevelEffect = false }
                    )
                    .transition(.opacity)
                }
                if showLevelUpEffect {
                    PetLevelUpEffectView(
                        level: levelUpTarget,
                        accentColor: info.stage.accentColor,
                        onComplete: { showLevelUpEffect = false }
                    )
                    .transition(.opacity)
                }
            }
        }
        .frame(height: TitlebarPetMetrics.rowHeight, alignment: .center)
        .onHover { isHovered = $0 }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            appIsActive = true
            recentActivityTick = Date()
            sleepClock = recentActivityTick
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in
            appIsActive = false
            recentActivityTick = Date()
            sleepClock = recentActivityTick
        }
        .onAppear {
            if petStore.isClaimed, info.level > 0, lastLevel == 0 {
                lastLevel = info.level
            }
            recentActivityTick = Date()
            sleepClock = recentActivityTick
            lastKnownStage = info.stage
        }
        .onChange(of: currentPhase) { _, _ in
            recentActivityTick = Date()
            sleepClock = recentActivityTick
        }
        .onChange(of: model.activityRenderVersion) { _, _ in
            if hasAnyRunningActivity {
                recentActivityTick = Date()
                sleepClock = recentActivityTick
            }
        }
        .onReceive(Timer.publish(every: 5, on: .main, in: .common).autoconnect()) { now in
            sleepClock = now
            if hasAnyRunningActivity {
                recentActivityTick = now
            }
        }
        .onChange(of: info.level) { _, level in
            guard petStore.isClaimed, level > lastLevel else {
                if level > 0, lastLevel == 0 {
                    lastLevel = level
                }
                return
            }
            lastLevel = level
            if level >= PetProgressInfo.maxLevel, !showedMaxLevelEffect {
                showedMaxLevelEffect = true
                isShowingPopover = true
                showMaxLevelEffect = true
            } else {
                levelUpTarget = level
                showLevelUpEffect = true
                isShowingPopover = true
            }
        }
        #endif
    }

    private var titleText: String {
        petStore.isClaimed
            ? "Lv.\(info.level)"
            : petL("pet.title.claim", "Claim")
    }

    private var tooltipText: String {
        petStore.isClaimed ? displayName : petL("pet.tooltip.pet", "Pet")
    }

    private var titlebarPill: some View {
        HStack(alignment: .center, spacing: 5) {
            PetTitlebarBadge(stage: info.stage, size: 19, isMaxLevel: info.hasUnlockedArchive)

            Text(titleText)
                .font(.system(size: 12.5, weight: .semibold, design: .rounded))
                .foregroundStyle(AppTheme.textPrimary.opacity(isShowingPopover || isHovered ? 1 : 0.9))
                .lineLimit(1)
                .fixedSize(horizontal: true, vertical: false)
        }
        .padding(.leading, 8)
        .padding(.trailing, 10)
        .frame(height: TitlebarPetMetrics.pillHeight)
        .background(
            RoundedRectangle(cornerRadius: TitlebarPetMetrics.pillCornerRadius, style: .continuous)
                .fill(
                    isShowingPopover
                    ? info.stage.accentColor.opacity(0.2)
                    : (isHovered ? info.stage.accentColor.opacity(0.13) : AppTheme.emphasizedControlFill)
                )
        )
        .overlay(
            RoundedRectangle(cornerRadius: TitlebarPetMetrics.pillCornerRadius, style: .continuous)
                .stroke(
                    isShowingPopover
                    ? info.stage.accentColor.opacity(0.3)
                    : (isHovered ? AppTheme.titlebarControlHoverBorder : AppTheme.titlebarControlBorder),
                    lineWidth: 0.5
                )
        )
        .clipShape(RoundedRectangle(cornerRadius: TitlebarPetMetrics.pillCornerRadius, style: .continuous))
        .contentShape(RoundedRectangle(cornerRadius: TitlebarPetMetrics.pillCornerRadius, style: .continuous))
        .fixedSize(horizontal: true, vertical: false)
    }

    private func presentClaimDialog() {
        guard let parentWindow = NSApp.keyWindow ?? NSApp.mainWindow else {
            return
        }
        PetClaimDialogPresenter.present(
            dialog: PetClaimDialogState(selectedOption: .voidcat),
            staticMode: model.appSettings.pet.staticMode,
            parentWindow: parentWindow
        ) { result in
            guard let result else { return }
            let hiddenSpeciesChance = model.aiStatsStore.hiddenPetSpeciesChanceAcrossProjects(model.projects)
            petStore.claim(
                option: result.option,
                customName: result.customName,
                hiddenSpeciesChance: hiddenSpeciesChance
            )
            model.petRefreshCoordinator.refreshNow(reason: .claim)
        }
    }
}

private enum TitlebarPetMetrics {
    static let rowHeight: CGFloat = 30
    static let pillHeight: CGFloat = 26
    static let pillCornerRadius: CGFloat = 8
}

// MARK: - Titlebar Badge (paw icon + stage color)

struct PetTitlebarBadge: View {
    let stage: PetStage
    let size: CGFloat
    var isMaxLevel: Bool = false

    var body: some View {
        ZStack {
            Circle()
                .fill(
                    LinearGradient(
                        colors: isMaxLevel
                            ? [Color(hex: 0xFFD700), Color(hex: 0xCC8800)]
                            : [stage.accentColor, stage.accentColor.adjustingBrightness(-0.25)],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            if isMaxLevel {
                Circle()
                    .stroke(Color(hex: 0xFFD700).opacity(0.7), lineWidth: 1.5)
                    .blur(radius: 1.5)
            }
            Image(systemName: isMaxLevel ? "crown.fill" : "pawprint.fill")
                .font(.system(size: size * 0.42, weight: .bold))
                .foregroundStyle(.white.opacity(0.95))
        }
        .frame(width: size, height: size)
        .clipShape(Circle())
        .shadow(color: isMaxLevel ? Color(hex: 0xFFD700).opacity(0.5) : .clear, radius: 3)
    }
}

// MARK: - Attribute Row

struct PetAttributeRow: View {
    let emoji: String
    let name: String
    let value: Int
    let maxValue: Int
    let color: Color
    let widestValueText: String
    var helpText: String? = nil

    private var ratio: CGFloat {
        guard maxValue > 0 else {
            return 0
        }
        return min(1, max(0, CGFloat(value) / CGFloat(maxValue)))
    }

    private var valueText: String {
        petFormatCompactNumber(value)
    }

    var body: some View {
        HStack(spacing: 6) {
            Text(emoji)
                .font(.system(size: 12))
                .frame(width: 16, alignment: .center)

            Text(name)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.secondary)
                .frame(width: 32, alignment: .leading)

            GeometryReader { geo in
                ZStack(alignment: .leading) {
                    RoundedRectangle(cornerRadius: 3, style: .continuous)
                        .fill(color.opacity(0.12))
                        .frame(height: 5)

                    RoundedRectangle(cornerRadius: 3, style: .continuous)
                        .fill(color.opacity(0.75))
                        .frame(width: geo.size.width * ratio, height: 5)
                        .animation(.easeOut(duration: 0.5), value: ratio)
                }
            }
            .frame(height: 5)

            ZStack(alignment: .trailing) {
                Text(widestValueText)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .hidden()
                Text(valueText)
                    .font(.system(size: 12, weight: .semibold, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .contentTransition(.numericText())
            }
        }
        .contentShape(Rectangle())
        .floatingTooltip(helpText ?? "", enabled: !(helpText ?? "").isEmpty, placement: .right)
    }
}

// MARK: - Stat Cell

struct PetStatCell: View {
    let label: String
    let value: String

    var body: some View {
        VStack(spacing: 2) {
            Text(label)
                .font(.system(size: 12, weight: .medium))
                .foregroundStyle(.tertiary)
            Text(value)
                .font(.system(size: 13, weight: .semibold, design: .rounded))
                .foregroundStyle(.primary)
                .lineLimit(1)
        }
    }
}

struct PetClaimPreview: View {
    let option: PetClaimOption
    let staticMode: Bool

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color.clear)

            if let species = option.previewSpecies {
                PetSpriteView(
                    species: species,
                    stage: .companion,
                    staticMode: true,
                    displaySize: 60
                )
            } else {
                Image(systemName: "sparkles")
                    .font(.system(size: 24, weight: .semibold))
                    .foregroundStyle(Color.secondary.opacity(0.72))
            }
        }
    }
}
