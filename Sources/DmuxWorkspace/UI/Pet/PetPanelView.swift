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
}

// MARK: - Titlebar Button

struct TitlebarPetButton: View {
    let model: AppModel
    let allTimeTokens: Int
    @Binding var isShowingPopover: Bool
    @AppStorage("pet.last_level") private var lastLevel: Int = 0
    @AppStorage("pet.did_first_open_bubble") private var didFirstOpenBubble: Bool = false
    @AppStorage("pet.showed_max_level_effect") private var showedMaxLevelEffect: Bool = false
    @State private var isHovered = false
    @State private var bubbleText: String? = nil
    @State private var bubbleTask: Task<Void, Never>? = nil
    @State private var appIsActive = NSApplication.shared.isActive
    @State private var recentActivityTick = Date()
    @State private var anyRunningSince: Date?
    @State private var lastCompletionXP: Int = 0
    @State private var bubbleCooldowns: [String: Date] = [:]
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
    private var liveComputedStats: PetStats {
        model.aiStatsStore.petStatsAcrossProjects(model.projects, claimedAt: petStore.claimedAt)
    }
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
        return Date().timeIntervalSince(recentActivityTick) > 300
    }

    private func showBubble(_ text: String, duration: TimeInterval = 3.0) {
        bubbleTask?.cancel()
        withAnimation(.easeInOut(duration: 0.2)) {
            bubbleText = text
        }
        bubbleTask = Task {
            try? await Task.sleep(nanoseconds: UInt64(duration * 1_000_000_000))
            if !Task.isCancelled {
                withAnimation(.easeInOut(duration: 0.3)) {
                    bubbleText = nil
                }
            }
        }
    }

    private func canTriggerBubble(_ trigger: String, cooldown: TimeInterval) -> Bool {
        let now = Date()
        if let last = bubbleCooldowns[trigger], now.timeIntervalSince(last) < cooldown {
            return false
        }
        bubbleCooldowns[trigger] = now
        return true
    }

    private func triggerBubble(_ trigger: String, cooldown: TimeInterval, duration: TimeInterval = 3.0) {
        guard canTriggerBubble(trigger, cooldown: cooldown) else {
            return
        }
        showBubble(bubbleTextForSpecies(species.rawValue, trigger: trigger), duration: duration)
    }

    private func bubbleTextForSpecies(_ species: String, trigger: String) -> String {
        switch (species, trigger) {
        case ("voidcat", "running"): return petL("pet.bubble.voidcat.running", "Lost in thought…")
        case ("voidcat", "hydration"): return petL("pet.bubble.voidcat.hydration", "A sip of water might help the thoughts flow")
        case ("voidcat", "error"): return petL("pet.bubble.voidcat.error", "Even the whiskers frowned")
        case ("voidcat", "complete"): return petL("pet.bubble.voidcat.complete", "Done already")
        case ("voidcat", "bigSession"): return petL("pet.bubble.voidcat.big_session", "That was a long train of thought")
        case ("voidcat", "activeSession"): return petL("pet.bubble.voidcat.active_session", "You've been working for ages")
        case ("voidcat", "lateNight"): return petL("pet.bubble.voidcat.late_night", "Still thinking deep into the night")
        case ("voidcat", "firstOpen"): return petL("pet.bubble.voidcat.first_open", "Good morning")

        case ("rusthound", "running"): return petL("pet.bubble.rusthound.running", "What are we thinking about?")
        case ("rusthound", "hydration"): return petL("pet.bubble.rusthound.hydration", "Water break, then straight back in")
        case ("rusthound", "error"): return petL("pet.bubble.rusthound.error", "Let's try that again")
        case ("rusthound", "complete"): return petL("pet.bubble.rusthound.complete", "Nailed it!")
        case ("rusthound", "bigSession"): return petL("pet.bubble.rusthound.big_session", "That was a huge chunk of work!")
        case ("rusthound", "activeSession"): return petL("pet.bubble.rusthound.active_session", "Maybe take a short break")
        case ("rusthound", "lateNight"): return petL("pet.bubble.rusthound.late_night", "It's getting late")
        case ("rusthound", "firstOpen"): return petL("pet.bubble.rusthound.first_open", "*big stretch*")

        case ("goose", "running"): return petL("pet.bubble.goose.running", "*stares quietly*")
        case ("goose", "hydration"): return petL("pet.bubble.goose.hydration", "Maybe drink some water first?")
        case ("goose", "error"): return petL("pet.bubble.goose.error", "Looks stuck")
        case ("goose", "complete"): return petL("pet.bubble.goose.complete", "*nods*")
        case ("goose", "bigSession"): return petL("pet.bubble.goose.big_session", "*big yawn*")
        case ("goose", "activeSession"): return petL("pet.bubble.goose.active_session", "Getting tired?")
        case ("goose", "lateNight"): return petL("pet.bubble.goose.late_night", "It's midnight already…")
        case ("goose", "firstOpen"): return petL("pet.bubble.goose.first_open", "A brand-new day again")

        case ("chaossprite", "running"): return petL("pet.bubble.chaossprite.running", "The ideas are about to burst")
        case ("chaossprite", "hydration"): return petL("pet.bubble.chaossprite.hydration", "Hydrate before the sparks get too wild")
        case ("chaossprite", "error"): return petL("pet.bubble.chaossprite.error", "The chaos is getting rough")
        case ("chaossprite", "complete"): return petL("pet.bubble.chaossprite.complete", "Chaos restored to order")
        case ("chaossprite", "bigSession"): return petL("pet.bubble.chaossprite.big_session", "That energy was intense")
        case ("chaossprite", "activeSession"): return petL("pet.bubble.chaossprite.active_session", "You've been burning bright for a while")
        case ("chaossprite", "lateNight"): return petL("pet.bubble.chaossprite.late_night", "Night is the best time to glow")
        case ("chaossprite", "firstOpen"): return petL("pet.bubble.chaossprite.first_open", "Time to sparkle again today")

        default:
            return "❤️"
        }
    }

    private func handlePhaseChange(_ phase: ProjectActivityPhase) {
        guard petStore.isClaimed else {
            return
        }
        switch phase {
        case .running:
            triggerBubble("running", cooldown: 300)
        case .waitingInput:
            break
        case .completed(_, _, let exitCode):
            let trigger = (exitCode ?? 0) == 0 ? "complete" : "error"
            triggerBubble(trigger, cooldown: trigger == "complete" ? 300 : 600)
            if currentXP - lastCompletionXP >= 500_000 {
                triggerBubble("bigSession", cooldown: 1800)
            }
            lastCompletionXP = currentXP
        case .idle:
            break
        }
    }

    private func syncRunningBaselines(now: Date = Date()) {
        if hasAnyRunningActivity {
            if anyRunningSince == nil {
                anyRunningSince = now
            }
        } else {
            anyRunningSince = nil
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
                    allTimeTokens: allTimeTokens,
                    sleeping: isSleeping,
                    petStats: petStats,
                    onInheritConfirmed: {
                        pendingEvoFrom = nil
                        showMaxLevelEffect = false
                        showLevelUpEffect = false
                        bubbleText = nil
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
        }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didResignActiveNotification)) { _ in
            appIsActive = false
            recentActivityTick = Date()
        }
        .onAppear {
            syncRunningBaselines()
            if petStore.isClaimed, !didFirstOpenBubble {
                didFirstOpenBubble = true
                triggerBubble("firstOpen", cooldown: 86_400)
            }
            petStore.refreshDerivedState(currentAllTimeTokens: allTimeTokens, computedStats: liveComputedStats)
            if petStore.isClaimed, info.level > 0, lastLevel == 0 {
                lastLevel = info.level
            }
            lastCompletionXP = currentXP
            recentActivityTick = Date()
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
        .onChange(of: currentPhase) { _, phase in
            recentActivityTick = Date()
            handlePhaseChange(phase)
            syncRunningBaselines()
        }
        .onChange(of: model.activityRenderVersion) { _, _ in
            petStore.refreshDerivedState(currentAllTimeTokens: allTimeTokens, computedStats: liveComputedStats)
            syncRunningBaselines()
            if hasAnyRunningActivity {
                recentActivityTick = Date()
            }
        }
        .onReceive(Timer.publish(every: 30, on: .main, in: .common).autoconnect()) { now in
            petStore.refreshDerivedState(
                currentAllTimeTokens: allTimeTokens,
                computedStats: liveComputedStats,
                now: now
            )
            syncRunningBaselines(now: now)
            if hasAnyRunningActivity {
                recentActivityTick = now
            }
            if model.appSettings.pet.sedentaryReminderEnabled,
               hasAnyRunningActivity,
               let anyRunningSince,
               now.timeIntervalSince(anyRunningSince) > model.appSettings.pet.sedentaryReminderInterval {
                triggerBubble("activeSession", cooldown: model.appSettings.pet.sedentaryReminderInterval)
            }
            if model.appSettings.pet.hydrationReminderEnabled,
               hasAnyRunningActivity,
               let anyRunningSince,
               now.timeIntervalSince(anyRunningSince) > model.appSettings.pet.hydrationReminderInterval {
                triggerBubble("hydration", cooldown: model.appSettings.pet.hydrationReminderInterval)
            }
            let hour = Calendar.current.component(.hour, from: now)
            if model.appSettings.pet.lateNightReminderEnabled,
               hasAnyRunningActivity,
               (hour >= 23 || hour < 6) {
                triggerBubble("lateNight", cooldown: model.appSettings.pet.lateNightReminderInterval)
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
        // Bubble overlay outside button hit area to prevent tap-through
        .overlay(alignment: .bottom) {
            if let text = bubbleText {
                PetBubbleView(
                    text: text,
                    onDismiss: { bubbleText = nil }
                )
                .offset(y: 40)
                .transition(.asymmetric(
                    insertion: .opacity.combined(with: .offset(y: -4)),
                    removal: .opacity.combined(with: .offset(y: -4))
                ))
                .zIndex(100)
            }
        }
        #endif
    }

    private var titleText: String {
        petStore.isClaimed
            ? (info.isHatching
                ? String(format: petL("pet.title.hatching_percent", "Hatching %@%%"), "\(Int((info.hatchProgress * 100).rounded()))")
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
                totalTokens: allTimeTokens,
                option: result.option,
                customName: result.customName,
                hiddenSpeciesChance: hiddenSpeciesChance
            )
            petStore.refreshDerivedState(
                currentAllTimeTokens: allTimeTokens,
                computedStats: liveComputedStats
            )
        }
    }
}

// MARK: - Pet Bubble

struct PetBubbleView: View {
    let text: String
    let onDismiss: () -> Void

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: "bubble.left")
                .font(.system(size: 14, weight: .medium))
                .foregroundStyle(Color(NSColor.secondaryLabelColor))

            Text(text)
                .font(.system(size: 12, weight: .medium, design: .rounded))
                .foregroundStyle(.primary)
                .multilineTextAlignment(.leading)
                .fixedSize(horizontal: false, vertical: true)
                .frame(minWidth: 60)
        }
        .padding(.horizontal, 12)
        .padding(.top, 18)
        .padding(.bottom, 10)
        .frame(minWidth: 150, maxWidth: 250, alignment: .leading)
        // Single unified shape: rounded rect + upward arrow at top-center
        .background(
            SpeechBubbleShape(cornerRadius: 11, arrowWidth: 14, arrowHeight: 8)
                .fill(Color(NSColor.windowBackgroundColor))
        )
        .overlay(
            SpeechBubbleShape(cornerRadius: 11, arrowWidth: 14, arrowHeight: 8)
                .stroke(Color(NSColor.separatorColor), lineWidth: 1)
        )
        .padding(.top, 8)
        .shadow(color: .black.opacity(0.12), radius: 8, y: 4)
        .contentShape(Rectangle())
        .onTapGesture { onDismiss() }
        .simultaneousGesture(TapGesture())
    }
}

// Single path: rounded rect with upward-pointing arrow at top-center
private struct SpeechBubbleShape: Shape {
    let cornerRadius: CGFloat
    let arrowWidth: CGFloat
    let arrowHeight: CGFloat

    func path(in rect: CGRect) -> Path {
        let r = cornerRadius
        let ah = arrowHeight
        // Body rect sits below the arrow
        let bodyMinY = rect.minY + ah
        let bodyMaxY = rect.maxY
        let minX = rect.minX
        let maxX = rect.maxX
        let midX = rect.midX

        var p = Path()
        // Start at arrow tip (top center)
        p.move(to: CGPoint(x: midX, y: rect.minY))
        // Arrow right slope → top-right of body
        p.addLine(to: CGPoint(x: midX + arrowWidth / 2, y: bodyMinY))
        p.addLine(to: CGPoint(x: maxX - r, y: bodyMinY))
        p.addQuadCurve(to: CGPoint(x: maxX, y: bodyMinY + r),
                       control: CGPoint(x: maxX, y: bodyMinY))
        // Right edge → bottom-right
        p.addLine(to: CGPoint(x: maxX, y: bodyMaxY - r))
        p.addQuadCurve(to: CGPoint(x: maxX - r, y: bodyMaxY),
                       control: CGPoint(x: maxX, y: bodyMaxY))
        // Bottom edge → bottom-left
        p.addLine(to: CGPoint(x: minX + r, y: bodyMaxY))
        p.addQuadCurve(to: CGPoint(x: minX, y: bodyMaxY - r),
                       control: CGPoint(x: minX, y: bodyMaxY))
        // Left edge → top-left
        p.addLine(to: CGPoint(x: minX, y: bodyMinY + r))
        p.addQuadCurve(to: CGPoint(x: minX + r, y: bodyMinY),
                       control: CGPoint(x: minX, y: bodyMinY))
        // Top-left → arrow left slope → tip
        p.addLine(to: CGPoint(x: midX - arrowWidth / 2, y: bodyMinY))
        p.closeSubpath()
        return p
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

    private var ratio: CGFloat {
        guard maxValue > 0 else {
            return 0
        }
        return CGFloat(value) / CGFloat(maxValue)
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
