import SwiftUI

// MARK: - Sprite Animation View

struct PetSpriteView: View {
    let species: PetSpecies
    let stage: PetStage
    var sleeping: Bool = false
    var staticMode = false
    let displaySize: CGFloat

    @State private var frame: Int = 0
    @State private var loadedImage: NSImage? = nil
    @State private var eggRocking = false

    private var spriteName: String {
        if species == .chaossprite {
            if sleeping, stage.sleepSpriteName != nil {
                return stage == .megaA || stage == .megaB ? "mega_sleep" : "evo_sleep"
            }
            switch stage {
            case .egg:
                return "egg"
            case .infant:
                return "infant_idle"
            case .child:
                return "child_idle"
            case .adult:
                return "adult_idle"
            case .evoA, .evoB:
                return "evo_idle"
            case .megaA, .megaB:
                return "mega_idle"
            }
        }
        if sleeping, let s = stage.sleepSpriteName { return s }
        return stage.idleSpriteName
    }

    private var frameCount: Int {
        sleeping && stage.sleepSpriteName != nil ? stage.sleepFrameCount : stage.idleFrameCount
    }

    private var frameDuration: TimeInterval {
        sleeping ? 0.625 : stage.idleFrameDuration
    }

    var body: some View {
        let size = stage.nativeFrameSize
        let scale = displaySize / size
        let sheetWidth = size * CGFloat(frameCount) * scale

        ZStack {
            if let img = loadedImage {
                Image(nsImage: img)
                    .resizable()
                    .interpolation(.medium)
                    .frame(width: sheetWidth, height: displaySize)
                    .offset(x: -displaySize * CGFloat(frame))
                    .frame(width: displaySize, height: displaySize, alignment: .leading)
                    .clipped()
                    .rotationEffect(
                        stage == .egg && !staticMode ? .degrees(eggRocking ? 8 : -8) : .zero,
                        anchor: .bottom
                    )
            } else {
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(stage.accentColor.opacity(0.12))
                    .frame(width: displaySize, height: displaySize)
                Image(systemName: species.placeholderSymbol)
                    .font(.system(size: displaySize * 0.34, weight: .semibold))
                    .foregroundStyle(stage.accentColor.opacity(0.7))
            }
            if sleeping, stage.sleepSpriteName == nil {
                sleepFallbackIndicator
            }
        }
        .task(id: "\(spriteName)-\(staticMode)") {
            frame = 0
            loadedImage = loadSprite(spriteName)
            if stage == .egg && !staticMode {
                withAnimation(.easeInOut(duration: 0.55).repeatForever(autoreverses: true)) {
                    eggRocking = true
                }
            }
            guard !staticMode else {
                return
            }
            while !Task.isCancelled {
                try? await Task.sleep(nanoseconds: UInt64(frameDuration * 1_000_000_000))
                frame = (frame + 1) % max(1, frameCount)
            }
        }
    }

    private func loadSprite(_ name: String) -> NSImage? {
        guard species.isImplemented else {
            return nil
        }
        if let url = Bundle.module.url(forResource: name, withExtension: "png", subdirectory: "Pets/\(species.assetFolder)") {
            return NSImage(contentsOf: url)
        }
        return nil
    }

    private var sleepFallbackIndicator: some View {
        Text("Z")
            .font(.system(size: max(14, displaySize * 0.18), weight: .heavy, design: .rounded))
            .foregroundStyle(stage.accentColor)
            .shadow(color: .black.opacity(0.18), radius: 1, y: 1)
            .offset(x: displaySize * 0.32, y: -displaySize * 0.36)
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
    @State private var pendingEvoFrom: PetStage? = nil
    @State private var lastKnownStage: PetStage? = nil
    @State private var showMaxLevelEffect = false
    @State private var showLevelUpEffect = false
    @State private var levelUpTarget: Int = 0
    @State private var showHatchEffect = false

    private var petStore: PetStore { model.petStore }
    private var species: PetSpecies { petStore.species }
    private var evoPath: PetEvoPath { petStore.currentEvoPath() }
    private var currentXP: Int { petStore.currentExperienceTokens }
    private var hatchTokens: Int { petStore.currentHatchTokens }
    private var info: PetProgressInfo { PetProgressInfo(totalXP: currentXP, hatchTokens: hatchTokens, evoPath: evoPath) }
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
            case .running, .waitingInput:
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

    var body: some View {
        #if SWIFT_PACKAGE
        EmptyView()
        #else
        Button {
            if petStore.isClaimed {
                isShowingPopover.toggle()
            } else {
                presentEggSelectionDialog()
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
                        pendingEvoFrom = nil
                        showMaxLevelEffect = false
                        showLevelUpEffect = false
                        isShowingPopover = false
                    }
                )
                if let fromStage = pendingEvoFrom {
                    PetEvolutionEffectView(
                        species: species,
                        evoPath: evoPath,
                        fromStage: fromStage,
                        toStage: info.stage,
                        onComplete: { pendingEvoFrom = nil }
                    )
                    .transition(.opacity)
                }
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
                if showHatchEffect {
                    PetHatchEffectView(
                        species: species,
                        onComplete: { showHatchEffect = false }
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
        .onChange(of: info.stage) { oldStage, newStage in
            guard petStore.isClaimed, oldStage != newStage else {
                lastKnownStage = newStage
                return
            }
            if oldStage == .egg, newStage == .infant {
                // Egg just hatched
                isShowingPopover = true
                showHatchEffect = true
            } else if oldStage != .egg, newStage != .egg {
                // Evolution
                pendingEvoFrom = oldStage
                isShowingPopover = true
            }
            lastKnownStage = newStage
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
            ? (info.isHatching
                ? String(format: petL("pet.title.hatching_percent", "Hatching %@%%"), info.hatchPercentText)
                : "Lv.\(info.level)")
            : petL("pet.title.claim", "Claim")
    }

    private var tooltipText: String {
        petStore.isClaimed ? displayName : petL("pet.tooltip.egg", "Pet Egg")
    }

    private var titlebarPill: some View {
        HStack(alignment: .center, spacing: 5) {
            PetTitlebarBadge(stage: info.stage, size: 19, isMaxLevel: info.hasUnlockedInheritance)

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

    private func presentEggSelectionDialog() {
        guard let parentWindow = NSApp.keyWindow ?? NSApp.mainWindow else {
            return
        }
        PetEggSelectionDialogPresenter.present(
            dialog: PetEggSelectionDialogState(selectedOption: .voidcat),
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

struct PetClaimEggPreview: View {
    let option: PetClaimOption
    let staticMode: Bool
    @State private var randomEggImage: NSImage? = nil

    var body: some View {
        ZStack {
            RoundedRectangle(cornerRadius: 12, style: .continuous)
                .fill(Color.clear)

            if let species = option.previewSpecies {
                PetSpriteView(
                    species: species,
                    stage: .egg,
                    staticMode: true,
                    displaySize: 60
                )
            } else {
                if let randomEggImage {
                    Image(nsImage: randomEggImage)
                        .resizable()
                        .interpolation(.none)
                        .frame(width: 60, height: 60)
                } else {
                    RoundedRectangle(cornerRadius: 10, style: .continuous)
                        .stroke(Color.primary.opacity(0.12), style: StrokeStyle(lineWidth: 1, dash: [4, 4]))
                        .padding(10)
                }
            }
        }
        .task {
            guard option == .random else {
                randomEggImage = nil
                return
            }
            if let url = Bundle.module.url(forResource: "egg", withExtension: "png", subdirectory: "Pets/random") {
                randomEggImage = NSImage(contentsOf: url)
            }
        }
    }
}
