import SwiftUI

// MARK: - Pet Progression Model

struct PetProgressInfo {
    let level: Int
    let xpInLevel: Int
    let xpForLevel: Int
    let totalXP: Int
    let hatchTokens: Int
    let stage: PetStage

    static let hatchThreshold = 200_000_000
    static let xpBase = 7_500_000
    // Raised from 180_000 → 750_000 so the curve grows ~11x from Lv1 to Lv100
    // (was ~3.4x — almost flat, causing rapid level-ups at all stages).
    // Full run Lv1→100 now requires ~4.4B XP vs the old 1.6B.
    static let xpIncrement = 750_000
    static let maxLevel = 100
    static let postCapXP = xpBase + (maxLevel - 1) * xpIncrement
    static let infantRange = 1 ... 15
    static let childRange = 16 ... 35
    static let adultRange = 36 ... 60
    static let evoRange = 61 ... 85
    static let megaStartLevel = 86
    static let evoUnlockLevel = evoRange.lowerBound

    init(totalXP: Int, hatchTokens: Int, evoPath: PetEvoPath) {
        let growthXP = max(0, totalXP)
        let hatch = min(max(0, hatchTokens), Self.hatchThreshold)
        guard hatch >= Self.hatchThreshold else {
            self.level = 0
            self.xpInLevel = hatch
            self.xpForLevel = Self.hatchThreshold
            self.totalXP = 0
            self.hatchTokens = hatch
            self.stage = .egg
            return
        }

        let lvl = Self.levelFromXP(growthXP)
        let consumed = Self.totalXPRequired(toReach: lvl)
        self.level = lvl
        self.xpInLevel = max(0, growthXP - consumed)
        self.xpForLevel = Self.xpForLevel(lvl)
        self.totalXP = growthXP
        self.hatchTokens = hatch
        self.stage = PetStage.stage(for: lvl, evoPath: evoPath)
    }

    var xpProgress: Double {
        guard xpForLevel > 0 else { return 1.0 }
        return min(1.0, Double(xpInLevel) / Double(xpForLevel))
    }

    var hatchProgress: Double {
        min(1.0, Double(hatchTokens) / Double(Self.hatchThreshold))
    }

    var isHatching: Bool { stage == .egg }

    var hasUnlockedInheritance: Bool { level >= Self.maxLevel }

    static func xpForLevel(_ level: Int) -> Int {
        if level >= maxLevel {
            return postCapXP
        }
        return xpBase + max(0, level - 1) * xpIncrement
    }

    static func totalXPRequired(toReach level: Int) -> Int {
        guard level > 1 else {
            return 0
        }

        let cappedLevel = min(level, maxLevel)
        var total = 0
        for current in 1 ..< cappedLevel {
            total += xpBase + (current - 1) * xpIncrement
        }
        if level > maxLevel {
            total += (level - maxLevel) * postCapXP
        }
        return total
    }

    static func levelFromXP(_ totalXP: Int) -> Int {
        let total = max(0, totalXP)
        var level = 1
        var remaining = total

        while true {
            let needed = xpForLevel(level)
            if remaining < needed {
                break
            }
            remaining -= needed
            level += 1
        }

        return level
    }

    // MARK: - Daily pace limiter

    /// Number of calendar days from `hatchDate` to `now` (0-based: hatch day itself = 0).
    static func dayIndex(from hatchDate: Date, to now: Date = Date()) -> Int {
        let calendar = Calendar.current
        let start = calendar.startOfDay(for: hatchDate)
        let today = calendar.startOfDay(for: now)
        return max(0, calendar.dateComponents([.day], from: start, to: today).day ?? 0)
    }

    /// Expected level on day `dayIndex` based on a 30-day-to-level-100 pace.
    static func expectedLevel(forDayIndex dayIndex: Int) -> Int {
        let totalXPForCap = totalXPRequired(toReach: maxLevel + 1)
        let dailyBudget = Double(totalXPForCap) / 30.0
        let expectedXP = Int(Double(max(0, dayIndex)) * dailyBudget)
        return levelFromXP(expectedXP)
    }

    /// XP rate multiplier for today.
    /// Returns 1.0 if the pet is at or below the daily pace.
    /// Rate curve: on-pace = 100%, +1 level ahead = 50%, each further level
    /// -10 pp (min 5%). This gives an immediate meaningful brake the moment
    /// the pet outpaces the 30-day schedule, without a harsh cliff.
    static func dailyXPRate(currentLevel: Int, dayIndex: Int) -> Double {
        let expected = expectedLevel(forDayIndex: dayIndex)
        let levelsAhead = max(0, currentLevel - expected)
        guard levelsAhead > 0 else { return 1.0 }
        return max(0.05, 0.50 - Double(levelsAhead - 1) * 0.10)
    }
}

enum PetEvoPath: String, Codable {
    case pathA, pathB
}

enum PetStage: String {
    case egg
    case infant
    case child
    case adult
    case evoA = "evo_a"
    case evoB = "evo_b"
    case megaA = "mega_a"
    case megaB = "mega_b"

    static func stage(for level: Int, evoPath: PetEvoPath) -> PetStage {
        switch level {
        case PetProgressInfo.infantRange: return .infant
        case PetProgressInfo.childRange:  return .child
        case PetProgressInfo.adultRange:  return .adult
        case PetProgressInfo.evoRange:    return evoPath == .pathA ? .evoA : .evoB
        default:      return evoPath == .pathA ? .megaA : .megaB
        }
    }

    var displayName: String {
        switch self {
        case .egg:    return petL("pet.stage.egg", "Hatching")
        case .infant: return petL("pet.stage.infant", "Infant")
        case .child:  return petL("pet.stage.child", "Growing")
        case .adult:  return petL("pet.stage.adult", "Adult")
        case .evoA, .evoB:   return petL("pet.stage.awakened", "Awakened")
        case .megaA, .megaB: return petL("pet.stage.final_awakening", "Final Awakening")
        }
    }

    func speciesName(for species: PetSpecies, evoPath: PetEvoPath) -> String {
        switch species {
        case .voidcat:
            switch self {
            case .egg:    return petL("pet.species.voidcat.egg", "花花蛋")
            case .infant: return petL("pet.species.voidcat.infant", "Huahua")
            case .child:  return petL("pet.species.voidcat.child", "Shadow Cat")
            case .adult:  return petL("pet.species.voidcat.adult", "Voidcat")
            case .evoA:   return petL("pet.species.voidcat.evo_a", "Tomecat")
            case .evoB:   return petL("pet.species.voidcat.evo_b", "Shadecat")
            case .megaA:  return petL("pet.species.voidcat.mega_a", "Inkspirit")
            case .megaB:  return petL("pet.species.voidcat.mega_b", "Nightspirit")
            }
        case .rusthound:
            switch self {
            case .egg:    return petL("pet.species.rusthound.egg", "毛团蛋")
            case .infant: return petL("pet.species.rusthound.infant", "Furball")
            case .child:  return petL("pet.species.rusthound.child", "Flop-Eared Pup")
            case .adult:  return petL("pet.species.rusthound.adult", "Rusthound")
            case .evoA:   return petL("pet.species.rusthound.evo_a", "Blazehound")
            case .evoB:   return petL("pet.species.rusthound.evo_b", "Ironwolf")
            case .megaA:  return petL("pet.species.rusthound.mega_a", "Sunflare")
            case .megaB:  return petL("pet.species.rusthound.mega_b", "Bloodmoon")
            }
        case .goose:
            switch self {
            case .egg:    return petL("pet.species.goose.egg", "啾啾蛋")
            case .infant: return petL("pet.species.goose.infant", "Chirpy")
            case .child:  return petL("pet.species.goose.child", "Dozy")
            case .adult:  return petL("pet.species.goose.adult", "Goosey")
            case .evoA:   return petL("pet.species.goose.evo_a", "Dawnwing")
            case .evoB:   return petL("pet.species.goose.evo_b", "Windwing")
            case .megaA:  return petL("pet.species.goose.mega_a", "Wildfire")
            case .megaB:  return petL("pet.species.goose.mega_b", "Tempest")
            }
        case .chaossprite:
            switch self {
            case .egg:    return petL("pet.species.chaossprite.egg", "混沌蛋")
            case .infant: return petL("pet.species.chaossprite.infant", "Chaos")
            case .child:  return petL("pet.species.chaossprite.child", "Mischief")
            case .adult:  return petL("pet.species.chaossprite.adult", "Glimmer")
            case .evoA, .evoB:   return petL("pet.species.chaossprite.evo", "Chaos Wisp")
            case .megaA, .megaB: return petL("pet.species.chaossprite.mega", "Prism Core")
            }
        }
    }

    var idleSpriteName: String { rawValue == "egg" ? "egg" : "\(rawValue)_idle" }
    var sleepSpriteName: String? {
        switch self {
        case .evoA, .evoB, .megaA, .megaB: return "\(rawValue)_sleep"
        default: return nil
        }
    }

    var idleFrameCount: Int {
        switch self {
        case .egg:             return 1
        case .infant, .child, .adult, .megaA, .megaB: return 8
        case .evoA, .evoB:     return 6
        }
    }

    var sleepFrameCount: Int { 8 }

    var nativeFrameSize: CGFloat {
        switch self {
        case .egg, .infant:    return 256
        case .child, .adult:   return 320
        case .evoA, .evoB:     return 384
        case .megaA, .megaB:   return 512
        }
    }

    var idleFrameDuration: TimeInterval {
        switch self {
        case .evoA, .evoB: return 0.600
        default:            return 0.625
        }
    }

    var accentColor: Color {
        switch self {
        case .egg:    return Color(hex: 0x888888)
        case .infant: return Color(hex: 0xC98663)
        case .child:  return Color(hex: 0xC8D1E3)
        case .adult:  return Color(hex: 0xE8AA34)
        case .evoA:   return Color(hex: 0x2A80FF)
        case .evoB:   return Color(hex: 0x9040FF)
        case .megaA:  return Color(hex: 0xE0C040)
        case .megaB:  return Color(hex: 0x6020CC)
        }
    }
}

extension PetStage {
    func resolvedIdentity(for species: PetSpecies, evoPath: PetEvoPath, customName: String) -> PetResolvedIdentity {
        let speciesName = speciesName(for: species, evoPath: evoPath)
        let trimmedName = customName.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmedName.isEmpty else {
            return PetResolvedIdentity(title: speciesName, subtitle: nil)
        }
        return PetResolvedIdentity(title: trimmedName, subtitle: speciesName)
    }
}

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

// MARK: - Popover

struct PetPopoverView: View {
    let model: AppModel
    let allTimeTokens: Int
    let sleeping: Bool
    let petStats: PetStats
    let onInheritConfirmed: (() -> Void)?
    @State private var isEditingName = false
    @State private var showsInheritanceConfirmation = false

    private var petStore: PetStore { model.petStore }
    private var species: PetSpecies { petStore.species }
    private var path: PetEvoPath { petStore.currentEvoPath() }
    private var currentXP: Int { petStore.currentExperienceTokens }
    private var hatchTokens: Int { petStore.currentHatchTokens }
    private var info: PetProgressInfo { PetProgressInfo(totalXP: currentXP, hatchTokens: hatchTokens, evoPath: path) }
    private var identity: PetResolvedIdentity { info.stage.resolvedIdentity(for: species, evoPath: path, customName: petStore.customName) }
    private var displayName: String { identity.title }
    private var hasLegacy: Bool { !petStore.legacy.isEmpty }
    private var maxStatValue: Int { max(1, petStore.currentStats.maxValue) }
    private var widestStatText: String { petStore.currentStats.widestCompactValueText }

    var body: some View {
        AnyView(
            mainContent
                .frame(width: 300)
                .alert(petL("pet.inherit.alert.title", "Inherit Current Pet"), isPresented: $showsInheritanceConfirmation) {
                    Button(petL("common.cancel", "Cancel"), role: .cancel) {}
                    Button(petL("pet.inherit.confirm", "Confirm Inheritance")) {
                        petStore.inheritCurrentPet()
                        onInheritConfirmed?()
                    }
                } message: {
                    Text(petL("pet.inherit.alert.message", "Inheritance will archive the current pet into the dex and return you to egg selection."))
                }
        )
    }

    private var mainContent: some View {
        return Group {
                if info.isHatching {
                    VStack(spacing: 0) {
                    // ── Header: egg · name+badge · percentage ──────────────────
                    HStack(alignment: .center, spacing: 12) {
                        PetSpriteView(
                            species: species,
                            stage: .egg,
                            staticMode: model.appSettings.pet.staticMode,
                            displaySize: 80
                        )
                        .frame(width: 80, height: 80)

                        VStack(alignment: .leading, spacing: 5) {
                            Text(displayName)
                                .font(.system(size: 16, weight: .bold, design: .rounded))
                                .foregroundStyle(.primary)
                            Text(petL("pet.stage.egg", "Hatching"))
                                .font(.system(size: 12, weight: .semibold, design: .rounded))
                                .foregroundStyle(info.stage.accentColor)
                                .padding(.horizontal, 8)
                                .padding(.vertical, 3)
                                .background(Capsule().fill(info.stage.accentColor.opacity(0.14)))
                            Text(petL("pet.hatch.description", "Hatches after reaching the token threshold"))
                                .font(.system(size: 12))
                                .foregroundStyle(.tertiary)
                        }

                        Spacer(minLength: 0)

                        // Percentage — right side, vertically centered
                        VStack(spacing: 2) {
                            Text("\(Int((info.hatchProgress * 100).rounded()))%")
                                .font(.system(size: 26, weight: .black, design: .rounded))
                                .foregroundStyle(.primary)
                                .contentTransition(.numericText())
                            Text(petL("pet.hatch.progress.short", "Hatch"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.tertiary)
                        }
                        .frame(minWidth: 64, alignment: .center)
                    }
                    .padding(14)

                    Divider()
                        .padding(.horizontal, 14)

                    // ── Progress bar ──────────────────────────────────────────
                    VStack(spacing: 6) {
                        HStack {
                            Text(petL("pet.hatch.progress.title", "Hatch Progress"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.secondary)
                            Spacer()
                            Text("\(petFormatCompactNumber(info.hatchTokens)) / \(petFormatCompactNumber(PetProgressInfo.hatchThreshold))")
                                .font(.system(size: 12, weight: .medium, design: .rounded))
                                .foregroundStyle(.secondary)
                        }

                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(info.stage.accentColor.opacity(0.15))
                                    .frame(height: 7)
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(
                                        LinearGradient(
                                            colors: [info.stage.accentColor, info.stage.accentColor.adjustingBrightness(-0.12)],
                                            startPoint: .leading, endPoint: .trailing
                                        )
                                    )
                                    .frame(width: max(0, geo.size.width * info.hatchProgress), height: 7)
                                    .animation(.easeInOut(duration: 0.4), value: info.hatchProgress)
                            }
                        }
                        .frame(height: 7)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)
                    .appCursor(.arrow)
                }
            } else {
                VStack(alignment: .leading, spacing: 0) {

                    // ── Header: large sprite + name + persona + level ──────────
                    VStack(spacing: 0) {
                        // Sprite — large, centered
                        PetSpriteView(
                            species: species,
                            stage: info.stage,
                            sleeping: sleeping,
                            staticMode: model.appSettings.pet.staticMode,
                            displaySize: 110
                        )
                        .frame(width: 110, height: 110)

                        Spacer().frame(height: 14)

                        // Name row
                        HStack(alignment: .firstTextBaseline, spacing: 6) {
                            if isEditingName {
                                TextField(
                                    petL("pet.name.placeholder", "Pet Name"),
                                    text: Binding(
                                        get: { petStore.customName },
                                        set: { petStore.rename($0) }
                                    )
                                )
                                .font(.system(size: 17, weight: .bold, design: .rounded))
                                .textFieldStyle(.roundedBorder)
                                .frame(maxWidth: 140)
                            } else {
                                Group {
                                    if let subtitle = identity.subtitle {
                                        Text(identity.title)
                                            .font(.system(size: 17, weight: .bold, design: .rounded))
                                            .foregroundStyle(.primary)
                                        + Text("  \(subtitle)")
                                            .font(.system(size: 14, weight: .medium, design: .rounded))
                                            .foregroundStyle(.secondary)
                                    } else {
                                        Text(identity.title)
                                            .font(.system(size: 17, weight: .bold, design: .rounded))
                                            .foregroundStyle(.primary)
                                    }
                                }
                                .onTapGesture { isEditingName = true }
                            }
                        }

                        Spacer().frame(height: 8)

                        // Persona tag only
                        Text(petStore.currentStats.personaTag)
                            .font(.system(size: 12, weight: .medium, design: .rounded))
                            .foregroundStyle(info.stage.accentColor)
                            .padding(.horizontal, 9)
                            .padding(.vertical, 4)
                            .background(Capsule().fill(info.stage.accentColor.opacity(0.14)))

                        Spacer().frame(height: 10)

                        // Level
                        Text("Lv.\(info.level)")
                            .font(.system(size: 26, weight: .black, design: .rounded))
                            .foregroundStyle(.primary)
                            .contentTransition(.numericText())
                    }
                    .frame(maxWidth: .infinity)
                    .padding(.top, 18)
                    .padding(.bottom, 14)
                    .padding(.horizontal, 14)

                    // Dex button — top-right corner
                    .overlay(alignment: .topTrailing) {
                        Button {
                            PetDexWindowPresenter.show(model: model)
                        } label: {
                            Image(systemName: "books.vertical.fill")
                                .font(.system(size: 13, weight: .semibold))
                                .foregroundStyle(info.stage.accentColor.opacity(0.8))
                                .frame(width: 28, height: 28)
                                .background(
                                    RoundedRectangle(cornerRadius: 7, style: .continuous)
                                        .fill(info.stage.accentColor.opacity(0.1))
                                )
                        }
                        .buttonStyle(.plain)
                        .help(petL("pet.dex.open", "Open Dex"))
                        .padding(14)
                    }

                    Divider().padding(.horizontal, 14)

                    // ── XP Bar ─────────────────────────────────────────────────
                    VStack(spacing: 6) {
                        HStack {
                            Text(petL("pet.xp.label", "Experience"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.secondary)
                            Spacer()
                            Text("\(petFormatCompactNumber(info.xpInLevel)) / \(petFormatCompactNumber(info.xpForLevel))")
                                .font(.system(size: 12, weight: .medium, design: .rounded))
                                .foregroundStyle(.secondary)
                        }
                        GeometryReader { geo in
                            ZStack(alignment: .leading) {
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(info.stage.accentColor.opacity(0.15))
                                    .frame(height: 7)
                                RoundedRectangle(cornerRadius: 4, style: .continuous)
                                    .fill(LinearGradient(
                                        colors: [info.stage.accentColor, info.stage.accentColor.adjustingBrightness(-0.15)],
                                        startPoint: .leading, endPoint: .trailing
                                    ))
                                    .frame(width: geo.size.width * info.xpProgress, height: 7)
                                    .animation(.easeInOut(duration: 0.4), value: info.xpProgress)
                            }
                        }
                        .frame(height: 7)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)

                    Divider().padding(.horizontal, 14)

                    // ── Pet Attributes ─────────────────────────────────────────
                    VStack(alignment: .leading, spacing: 7) {
                        Text(petL("pet.stats.title", "Traits"))
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.tertiary)
                        PetAttributeRow(emoji: "🧠", name: petL("pet.attribute.wisdom", "Wisdom"), value: petStore.currentStats.wisdom, maxValue: maxStatValue, color: info.stage.accentColor, widestValueText: widestStatText)
                        PetAttributeRow(emoji: "🔥", name: petL("pet.attribute.chaos", "Chaos"), value: petStore.currentStats.chaos, maxValue: maxStatValue, color: Color(hex: 0xFF6030), widestValueText: widestStatText)
                        PetAttributeRow(emoji: "🌙", name: petL("pet.attribute.night", "Night"), value: petStore.currentStats.night, maxValue: maxStatValue, color: Color(hex: 0x6060CC), widestValueText: widestStatText)
                        PetAttributeRow(emoji: "💪", name: petL("pet.attribute.stamina", "Stamina"), value: petStore.currentStats.stamina, maxValue: maxStatValue, color: Color(hex: 0x20A060), widestValueText: widestStatText)
                        PetAttributeRow(emoji: "🩹", name: petL("pet.attribute.empathy", "Empathy"), value: petStore.currentStats.empathy, maxValue: maxStatValue, color: Color(hex: 0xE060A0), widestValueText: widestStatText)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)

                    Divider().padding(.horizontal, 14)

                    // ── Stats Row (XP + next stage only) ───────────────────────
                    HStack(spacing: 0) {
                        PetStatCell(label: petL("pet.total_xp", "Total XP"), value: petFormatCompactNumber(info.totalXP))
                            .frame(maxWidth: .infinity)
                        Divider().frame(height: 32)
                        PetStatCell(label: petL("pet.next_stage", "Next Stage"), value: nextStageLabel(info: info))
                            .frame(maxWidth: .infinity)
                    }
                    .padding(.vertical, 10)

                    if info.hasUnlockedInheritance {
                        Divider().padding(.horizontal, 14)

                        HStack {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(petL("pet.inherit.unlocked", "Inheritance Unlocked"))
                                    .font(.system(size: 13, weight: .semibold, design: .rounded))
                                Text(petL("pet.inherit.unlocked.detail", "Inheritance unlocks at level 100, but levels can keep increasing."))
                                    .font(.system(size: 12))
                                    .foregroundStyle(.secondary)
                            }
                            Spacer()
                            Button(petL("pet.inherit.action", "Inherit")) {
                                showsInheritanceConfirmation = true
                            }
                            .buttonStyle(.borderedProminent)
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                    }

                    if hasLegacy {
                        Divider().padding(.horizontal, 14)

                        VStack(alignment: .leading, spacing: 8) {
                            Text(petL("pet.inherit.history", "Inheritance History"))
                                .font(.system(size: 12, weight: .medium))
                                .foregroundStyle(.tertiary)

                            ForEach(Array(petStore.legacy.prefix(2))) { record in
                                PetLegacyRow(
                                    record: record,
                                    stage: PetProgressInfo(
                                        totalXP: record.totalXP,
                                        hatchTokens: PetProgressInfo.hatchThreshold,
                                        evoPath: record.evoPath
                                    ).stage
                                )
                            }
                        }
                        .padding(.horizontal, 14)
                        .padding(.vertical, 10)
                    }
                }
            }
        }
    }

    private func nextStageLabel(info: PetProgressInfo) -> String {
        let next: PetStage
        switch info.stage {
        case .egg:    next = .infant
        case .infant: next = .child
        case .child:  next = .adult
        case .adult:  next = path == .pathA ? .evoA : .evoB
        case .evoA, .evoB: next = path == .pathA ? .megaA : .megaB
        case .megaA, .megaB: return petL("pet.stage.awakened_complete", "Awakened")
        }
        return next.displayName
    }
}

private struct PetLegacyRow: View {
    let record: PetLegacyRecord
    let stage: PetStage

    private var identity: PetResolvedIdentity {
        record.resolvedIdentity(for: stage)
    }

    private var subtitleText: String {
        let parts = [identity.subtitle, "\(petFormatCompactNumber(record.totalXP)) XP"]
            .compactMap { $0 }
        return parts.joined(separator: " · ")
    }

    var body: some View {
        HStack(spacing: 8) {
            Image(systemName: record.species.placeholderSymbol)
                .font(.system(size: 13, weight: .semibold))
                .foregroundStyle(.secondary)
                .frame(width: 16)

            VStack(alignment: .leading, spacing: 2) {
                Text(identity.title)
                    .font(.system(size: 12, weight: .semibold, design: .rounded))
                Text(subtitleText)
                    .font(.system(size: 12))
                    .foregroundStyle(.secondary)
            }

            Spacer()
        }
        .padding(.vertical, 4)
    }
}

private struct PetEggSelectionDialogState: Equatable {
    var selectedOption: PetClaimOption
}


struct PetEggClaimResult {
    let option: PetClaimOption
    let customName: String
}

@MainActor
private enum PetEggSelectionDialogPresenter {
    static func present(
        dialog: PetEggSelectionDialogState,
        staticMode: Bool,
        parentWindow: NSWindow,
        completion: @escaping (PetEggClaimResult?) -> Void
    ) {
        let controller = PetEggSelectionDialogController(dialog: dialog, staticMode: staticMode)
        controller.beginSheet(for: parentWindow, completion: completion)
    }
}

@MainActor
private final class PetEggSelectionDialogViewModel: ObservableObject {
    @Published var selectedOption: PetClaimOption
    @Published var customName: String = ""

    init(dialog: PetEggSelectionDialogState) {
        self.selectedOption = dialog.selectedOption
    }
}

private struct PetEggSelectionDialogView: View {
    static let dialogWidth: CGFloat = 640
    static let dialogHeight: CGFloat = 420

    @ObservedObject var viewModel: PetEggSelectionDialogViewModel
    let staticMode: Bool
    let onCancel: () -> Void
    let onConfirm: () -> Void

    private var selectedOption: PetClaimOption { viewModel.selectedOption }
    private var accentColor: Color {
        switch selectedOption {
        case .voidcat:  return Color(hex: 0x2A80FF)
        case .rusthound: return Color(hex: 0xFF6030)
        case .goose:    return Color(hex: 0xF5DEB3)
        case .random:   return .purple
        }
    }
    private var description: String {
        switch selectedOption {
        case .voidcat:   return petL("pet.claim.voidcat.description", "A black cat that loves long thoughts in the middle of the night.")
        case .rusthound: return petL("pet.claim.rusthound.description", "It falls over, gets up again, and keeps going anyway.")
        case .goose:     return petL("pet.claim.goose.description", "It doesn't fully get what's happening, but it also doesn't mind.")
        case .random:    return petL("pet.claim.random.description", "Leave it to fate. A random egg can hatch the hidden species.")
        }
    }

    @ViewBuilder
    private func optionRow(_ option: PetClaimOption) -> some View {
        let isSelected = viewModel.selectedOption == option
        Button {
            withAnimation(.easeInOut(duration: 0.15)) { viewModel.selectedOption = option }
        } label: {
            HStack(spacing: 10) {
                PetClaimEggPreview(option: option, staticMode: staticMode)
                    .frame(width: 44, height: 44)
                VStack(alignment: .leading, spacing: 2) {
                    Text(option.title)
                        .font(.system(size: 13, weight: .semibold, design: .rounded))
                        .foregroundStyle(.primary)
                    Text(option.subtitle)
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Spacer()
                if isSelected {
                    Image(systemName: "checkmark.circle.fill")
                        .foregroundStyle(Color.accentColor)
                        .font(.system(size: 14))
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .fill(isSelected ? Color.accentColor.opacity(0.1) : Color.primary.opacity(0.035))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 10, style: .continuous)
                    .stroke(isSelected ? Color.accentColor.opacity(0.6) : Color.clear, lineWidth: 1)
            )
        }
        .buttonStyle(.plain)
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack(spacing: 12) {
                ZStack {
                    Circle()
                        .fill(LinearGradient(
                            colors: [accentColor.opacity(0.3), accentColor.opacity(0.1)],
                            startPoint: .topLeading, endPoint: .bottomTrailing))
                    Image(systemName: "pawprint.fill")
                        .font(.system(size: 18, weight: .bold))
                        .foregroundStyle(accentColor)
                }
                .frame(width: 40, height: 40)
                VStack(alignment: .leading, spacing: 2) {
                    Text(petL("pet.claim.dialog.title", "Choose a Pet Egg"))
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                    Text(petL("pet.claim.dialog.subtitle", "A tiny coding partner that hatches by your side"))
                        .font(.system(size: 12))
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .padding(.horizontal, 20)
            .padding(.top, 20)
            .padding(.bottom, 16)

            Divider()

            HStack(alignment: .top, spacing: 0) {
                // Left: grid of options
                VStack(spacing: 8) {
                    ForEach(PetClaimOption.allCases) { option in
                        optionRow(option)
                    }
                }
                .padding(14)
                .frame(width: 220)

                Divider()

                // Right: big preview + description + name
                VStack(alignment: .center, spacing: 12) {
                    // Big egg preview
                    ZStack {
                        Circle()
                            .fill(accentColor.opacity(0.08))
                            .frame(width: 100, height: 100)
                        if let species = selectedOption.previewSpecies {
                            PetSpriteView(species: species, stage: .egg,
                                          staticMode: true, displaySize: 84)
                                .frame(width: 84, height: 84)
                        } else {
                            Image(systemName: "sparkles")
                                .font(.system(size: 32, weight: .semibold))
                                .foregroundStyle(accentColor.opacity(0.7))
                        }
                    }
                    .animation(.easeInOut(duration: 0.2), value: selectedOption)

                    Text(selectedOption.title)
                        .font(.system(size: 15, weight: .bold, design: .rounded))
                        .foregroundStyle(.primary)

                    Text(description)
                        .font(.system(size: 12, design: .rounded))
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                        .lineLimit(3)
                        .frame(maxWidth: .infinity)

                    Spacer(minLength: 0)

                    Divider()

                    // Name field
                    VStack(alignment: .leading, spacing: 6) {
                        Text(petL("pet.claim.name.label", "Give it a name (optional)"))
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(.secondary)
                        TextField(petL("pet.claim.name.placeholder", "Leave empty to use the species name"),
                                  text: Binding(get: { viewModel.customName },
                                               set: { viewModel.customName = $0 }))
                            .textFieldStyle(.roundedBorder)
                            .font(.system(size: 13))
                    }
                }
                .padding(16)
                .frame(maxWidth: .infinity)
            }

            Divider()

            // Actions
            HStack {
                Button(petL("common.cancel", "Cancel")) { onCancel() }
                    .keyboardShortcut(.escape)
                Spacer()
                Button(petL("pet.claim.confirm", "Confirm Claim")) { onConfirm() }
                    .buttonStyle(.borderedProminent)
                    .keyboardShortcut(.return)
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 14)
        }
        .frame(width: Self.dialogWidth, height: Self.dialogHeight)
    }
}

@MainActor
private final class PetEggSelectionDialogController: AppDialogController<PetEggClaimResult> {
    private let viewModel: PetEggSelectionDialogViewModel

    init(dialog: PetEggSelectionDialogState, staticMode: Bool) {
        self.viewModel = PetEggSelectionDialogViewModel(dialog: dialog)

        let panel = AppDialogPanel(
            contentRect: NSRect(x: 0, y: 0, width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight),
            styleMask: [.titled, .closable],
            backing: .buffered,
            defer: false
        )
        panel.title = petL("pet.claim.window.title", "Claim Pet")
        panel.isReleasedWhenClosed = false
        panel.isFloatingPanel = false
        panel.hidesOnDeactivate = false
        panel.level = .modalPanel
        panel.collectionBehavior = [.fullScreenAuxiliary]
        panel.standardWindowButton(.miniaturizeButton)?.isHidden = true
        panel.standardWindowButton(.zoomButton)?.isHidden = true
        panel.setContentSize(NSSize(width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight))
        panel.minSize = NSSize(width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight)
        panel.maxSize = NSSize(width: PetEggSelectionDialogView.dialogWidth, height: PetEggSelectionDialogView.dialogHeight)

        super.init(panel: panel)

        let contentView = PetEggSelectionDialogView(
            viewModel: viewModel,
            staticMode: staticMode,
            onCancel: { [weak self] in
                self?.finish(with: .abort)
            },
            onConfirm: { [weak self] in
                guard let self else { return }
                let result = PetEggClaimResult(
                    option: viewModel.selectedOption,
                    customName: viewModel.customName.trimmingCharacters(in: .whitespacesAndNewlines)
                )
                self.finish(with: .continue, value: result)
            }
        )
        let hostingController = NSHostingController(rootView: contentView)
        hostingController.view.frame = NSRect(
            x: 0,
            y: 0,
            width: PetEggSelectionDialogView.dialogWidth,
            height: PetEggSelectionDialogView.dialogHeight
        )
        hostingController.view.autoresizingMask = [.width, .height]
        panel.contentViewController = hostingController
    }

    @available(*, unavailable)
    required init?(coder: NSCoder) {
        fatalError("init(coder:) has not been implemented")
    }
}

// MARK: - Attribute Row

private struct PetAttributeRow: View {
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

private struct PetStatCell: View {
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

private struct PetClaimEggPreview: View {
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
