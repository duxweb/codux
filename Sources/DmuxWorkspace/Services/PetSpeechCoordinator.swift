import AppKit
import Foundation
import Observation

@MainActor
@Observable
final class PetSpeechCoordinator {
    private let catalog: PetSpeechCatalog
    private let logger = AppDebugLog.shared

    private var settingsProvider: (() -> AppAIPetSettings)?
    private var aiSettingsProvider: (() -> AppAISettings)?
    private var petSettingsProvider: (() -> AppPetSettings)?
    private var petNameProvider: (() -> String)?
    private var activitySnapshotsProvider: (() -> [PetSpeechActivitySnapshot])?
    private var llmLineProvider: ((PetSpeechEvent, PetSpeechMode, AppAISettings) async -> String?)?
    private var expiryTask: Task<Void, Never>?
    private var periodicTimer: Timer?
    private var lastGlobalSpeechAt: Date?
    private var lastSpeechByEventKind: [PetSpeechEventKind: Date] = [:]
    private var lastTurnFamilySpeechAt: Date?
    private var lastAnyActivityAt: Date?
    private var currentIdleStartedAt: Date?
    private var didEmitIdleEnteredForCurrentIdle = false
    private var emittedNightDays: Set<String> = []
    private var multiToolStartedAt: Date?
    private var didEmitCurrentMultiToolStreak = false
    private var tokenBurstSamples: [(date: Date, tokens: Int, tool: String)] = []
    private var lastTokenBurstAt: Date?
    private var temporaryFrequencyOffset = 0
    private var temporaryFrequencyOffsetUntil: Date?
    private var reminderActiveStartedAt: Date?

    var currentLine: PetSpeechLine?

    init(catalog: PetSpeechCatalog = PetSpeechCatalog()) {
        self.catalog = catalog
    }

    func configure(
        settings: @escaping () -> AppAIPetSettings,
        aiSettings: (() -> AppAISettings)? = nil,
        petSettings: @escaping () -> AppPetSettings = { AppPetSettings() },
        petName: @escaping () -> String,
        activitySnapshots: @escaping () -> [PetSpeechActivitySnapshot],
        llmLineProvider: ((PetSpeechEvent, PetSpeechMode, AppAISettings) async -> String?)? = nil
    ) {
        settingsProvider = settings
        aiSettingsProvider = aiSettings
        petSettingsProvider = petSettings
        petNameProvider = petName
        activitySnapshotsProvider = activitySnapshots
        self.llmLineProvider = llmLineProvider
    }

    func start() {
        periodicTimer?.invalidate()
        periodicTimer = Timer.scheduledTimer(withTimeInterval: 60, repeats: true) { [weak self] _ in
            Task { @MainActor [weak self] in
                self?.runPeriodicChecks(now: Date())
            }
        }
    }

    func stop() {
        periodicTimer?.invalidate()
        periodicTimer = nil
        expiryTask?.cancel()
        expiryTask = nil
    }

    func notify(_ event: PetSpeechEvent) {
        let aiSettings = aiSettingsProvider?()
        let settings = aiSettings?.pet ?? settingsProvider?() ?? AppAIPetSettings()
        let isReminder = event.kind.isReminder
        guard settings.speechMode != .off || isReminder else {
            clearCurrentLine()
            return
        }

        var normalizedEvent = event
        normalizedEvent.payload = enrichedPayload(event.payload)

        recordTokenBurstCandidate(from: normalizedEvent)

        guard shouldSpeak(event: normalizedEvent, settings: settings) else {
            return
        }

        let mode = resolvedMode(settings.speechMode == .off && isReminder ? .encourage : settings.speechMode)
        let line = catalog.pickLine(mode: mode, event: normalizedEvent)
        guard line.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false else {
            return
        }

        currentLine = line
        lastGlobalSpeechAt = normalizedEvent.occurredAt
        lastSpeechByEventKind[normalizedEvent.kind] = normalizedEvent.occurredAt
        if normalizedEvent.kind.isTurnFamily {
            lastTurnFamilySpeechAt = normalizedEvent.occurredAt
        }
        scheduleExpiry(for: line)
        logger.log("pet-speech", "event=\(normalizedEvent.kind.rawValue) source=\(line.source.rawValue)")
        requestLLMReplacementIfNeeded(
            fallbackLine: line,
            event: normalizedEvent,
            mode: mode,
            aiSettings: aiSettings
        )
    }

    func clearCurrentLine() {
        expiryTask?.cancel()
        expiryTask = nil
        currentLine = nil
    }

    func skipCurrentLine() {
        clearCurrentLine()
    }

    func speakMoreTemporarily() {
        temporaryFrequencyOffset = min(1, temporaryFrequencyOffset + 1)
        temporaryFrequencyOffsetUntil = Date().addingTimeInterval(3600)
    }

    func speakLessTemporarily() {
        temporaryFrequencyOffset = max(-1, temporaryFrequencyOffset - 1)
        temporaryFrequencyOffsetUntil = Date().addingTimeInterval(3600)
    }

    private func shouldSpeak(event: PetSpeechEvent, settings: AppAIPetSettings) -> Bool {
        if event.isHardOverride {
            return true
        }

        if isMuted(settings: settings, now: event.occurredAt) {
            return false
        }

        let frequency = effectiveFrequency(settings: settings, now: event.occurredAt)
        let config = frequency.config

        if event.kind.isReminder == false {
            guard event.tier >= config.minimumTier else {
                return false
            }

            if event.tier == .daily,
               config.lv1SuppressRate > 0,
               Double.random(in: 0..<1) < config.lv1SuppressRate {
                return false
            }
        }

        if let lastGlobalSpeechAt,
           event.occurredAt.timeIntervalSince(lastGlobalSpeechAt) < config.globalCooldown {
            return false
        }

        if let lastEventSpeech = lastSpeechByEventKind[event.kind],
           event.occurredAt.timeIntervalSince(lastEventSpeech) < config.perEventCooldown {
            return false
        }

        if event.kind.isTurnFamily,
           let lastTurnFamilySpeechAt,
           event.occurredAt.timeIntervalSince(lastTurnFamilySpeechAt) < config.perEventCooldown {
            return false
        }

        return true
    }

    private func effectiveFrequency(settings: AppAIPetSettings, now: Date) -> PetSpeechFrequency {
        var frequency = settings.speechFrequency
        let calendar = Calendar.current
        let hour = calendar.component(.hour, from: now)
        let weekday = calendar.component(.weekday, from: now)
        if settings.speechQuietDuringWork,
           (2 ... 6).contains(weekday),
           (9 ..< 18).contains(hour) {
            frequency = frequency.lowered()
        }
        if settings.speechLouderAtNight,
           (hour >= 22 || hour < 6) {
            frequency = frequency.raised()
        }
        if let until = temporaryFrequencyOffsetUntil,
           until > now {
            if temporaryFrequencyOffset > 0 {
                frequency = frequency.raised()
            } else if temporaryFrequencyOffset < 0 {
                frequency = frequency.lowered()
            }
        } else {
            temporaryFrequencyOffset = 0
            temporaryFrequencyOffsetUntil = nil
        }
        return frequency
    }

    private func isMuted(settings: AppAIPetSettings, now: Date) -> Bool {
        if let muteUntil = settings.speechTemporaryMuteUntil,
           muteUntil > now {
            return true
        }
        if quietHoursActive(settings: settings, now: now) {
            return true
        }
        if settings.speechMuteOnFullscreen,
           NSApplication.shared.presentationOptions.contains(.fullScreen) {
            return true
        }
        return false
    }

    private func quietHoursActive(settings: AppAIPetSettings, now: Date) -> Bool {
        guard let start = settings.speechQuietHoursStart,
              let end = settings.speechQuietHoursEnd,
              start != end else {
            return false
        }
        let hour = Calendar.current.component(.hour, from: now)
        if start < end {
            return (start ..< end).contains(hour)
        }
        return hour >= start || hour < end
    }

    private func enrichedPayload(_ payload: [String: String]) -> [String: String] {
        var next = payload
        if next["petName"]?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty ?? true {
            let petName = petNameProvider?().trimmingCharacters(in: .whitespacesAndNewlines)
            next["petName"] = petName?.isEmpty == false
                ? petName
                : petSpeechL("pet.speech.payload.pet_name", "Little One")
        }
        return next
    }

    private func resolvedMode(_ mode: PetSpeechMode) -> PetSpeechMode {
        if mode == .mixed {
            return PetSpeechMode.concreteModes.randomElement() ?? .encourage
        }
        if PetSpeechMode.concreteModes.contains(mode) {
            return mode
        }
        return .encourage
    }

    private func requestLLMReplacementIfNeeded(
        fallbackLine: PetSpeechLine,
        event: PetSpeechEvent,
        mode: PetSpeechMode,
        aiSettings: AppAISettings?
    ) {
        guard let aiSettings,
              aiSettings.pet.speechLLMEnabled,
              event.tier >= .rhythm,
              let llmLineProvider else {
            return
        }

        Task { @MainActor [weak self] in
            guard let self,
                  self.currentLine?.id == fallbackLine.id,
                  let text = await llmLineProvider(event, mode, aiSettings),
                  text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false,
                  self.currentLine?.id == fallbackLine.id else {
                return
            }
            var llmLine = fallbackLine
            llmLine.text = text
            llmLine.source = .llm
            self.currentLine = llmLine
            self.scheduleExpiry(for: llmLine)
            self.logger.log("pet-speech", "event=\(event.kind.rawValue) source=llm")
        }
    }

    private func scheduleExpiry(for line: PetSpeechLine) {
        expiryTask?.cancel()
        expiryTask = Task { @MainActor [weak self] in
            try? await Task.sleep(nanoseconds: UInt64(line.ttl * 1_000_000_000))
            guard let self,
                  !Task.isCancelled,
                  self.currentLine?.id == line.id else {
                return
            }
            self.currentLine = nil
        }
    }

    private func recordTokenBurstCandidate(from event: PetSpeechEvent) {
        guard event.kind != .tokensBurst,
              let rawTokens = event.payload["tokensInt"],
              let tokens = Int(rawTokens),
              tokens > 0 else {
            return
        }
        let now = event.occurredAt
        tokenBurstSamples.append((now, tokens, event.payload["tool"] ?? petSpeechL("pet.speech.payload.tool", "you")))
        tokenBurstSamples.removeAll { now.timeIntervalSince($0.date) > 1800 }
        let total = tokenBurstSamples.reduce(0) { $0 + $1.tokens }
        guard total >= 50_000,
              lastTokenBurstAt.map({ now.timeIntervalSince($0) >= 1800 }) != false else {
            return
        }
        lastTokenBurstAt = now
        let dominantTool = tokenBurstSamples.last?.tool
            ?? event.payload["tool"]
            ?? petSpeechL("pet.speech.payload.tool", "you")
        notify(
            PetSpeechEvent(
                kind: .tokensBurst,
                payload: [
                    "tool": dominantTool,
                    "tokensK": "\(max(1, total / 1000))K",
                ],
                occurredAt: now
            )
        )
    }

    private func runPeriodicChecks(now: Date) {
        emitNightEnteredIfNeeded(now: now)

        let snapshots = activitySnapshotsProvider?() ?? []
        let activeSnapshots = snapshots.filter { $0.state == "responding" || $0.state == "needsInput" }
        if activeSnapshots.isEmpty {
            reminderActiveStartedAt = nil
            handleIdleTick(now: now)
        } else {
            emitReminderEventsIfNeeded(activeSnapshots: activeSnapshots, now: now)
            handleActiveTick(activeSnapshots: activeSnapshots, now: now)
        }
    }

    private func emitNightEnteredIfNeeded(now: Date) {
        let calendar = Calendar.current
        guard calendar.component(.hour, from: now) >= 22 else {
            return
        }
        let components = calendar.dateComponents([.year, .month, .day], from: now)
        let dayKey = "\(components.year ?? 0)-\(components.month ?? 0)-\(components.day ?? 0)"
        guard emittedNightDays.contains(dayKey) == false else {
            return
        }
        emittedNightDays.insert(dayKey)
        notify(
            PetSpeechEvent(
                kind: .nightEntered,
                payload: [
                    "hourLabel": String(
                        format: petSpeechL("pet.speech.payload.hour_format", "%d:00"),
                        calendar.component(.hour, from: now)
                    )
                ],
                occurredAt: now
            )
        )
    }

    private func handleIdleTick(now: Date) {
        if currentIdleStartedAt == nil {
            currentIdleStartedAt = lastAnyActivityAt ?? now
            didEmitIdleEnteredForCurrentIdle = false
        }
        guard let idleStartedAt = currentIdleStartedAt,
              didEmitIdleEnteredForCurrentIdle == false,
              now.timeIntervalSince(idleStartedAt) >= 300 else {
            return
        }
        didEmitIdleEnteredForCurrentIdle = true
        notify(PetSpeechEvent(kind: .idleEntered, occurredAt: now))
    }

    private func handleActiveTick(activeSnapshots: [PetSpeechActivitySnapshot], now: Date) {
        if let idleStartedAt = currentIdleStartedAt,
           now.timeIntervalSince(idleStartedAt) >= 1800 {
            notify(
                PetSpeechEvent(
                    kind: .idleReturned,
                    payload: ["minutesAway": "\(Int(now.timeIntervalSince(idleStartedAt) / 60))"],
                    occurredAt: now
                )
            )
        }
        currentIdleStartedAt = nil
        didEmitIdleEnteredForCurrentIdle = false
        lastAnyActivityAt = now

        let tools = Array(Set(activeSnapshots.map(\.tool))).sorted()
        if tools.count >= 2 {
            if multiToolStartedAt == nil {
                multiToolStartedAt = now
                didEmitCurrentMultiToolStreak = false
            }
            if let startedAt = multiToolStartedAt,
               didEmitCurrentMultiToolStreak == false,
               now.timeIntervalSince(startedAt) >= 600 {
                didEmitCurrentMultiToolStreak = true
                notify(
                    PetSpeechEvent(
                        kind: .toolMultiStreak,
                        payload: ["toolList": ListFormatter.localizedString(byJoining: tools)],
                        occurredAt: now
                    )
                )
            }
        } else {
            multiToolStartedAt = nil
            didEmitCurrentMultiToolStreak = false
        }
    }

    private func emitReminderEventsIfNeeded(
        activeSnapshots: [PetSpeechActivitySnapshot],
        now: Date
    ) {
        let petSettings = petSettingsProvider?() ?? AppPetSettings()
        guard petSettings.enabled else {
            return
        }

        if reminderActiveStartedAt == nil {
            reminderActiveStartedAt = activeSnapshots
                .compactMap(\.activeStartedAt)
                .min() ?? now
        }

        guard let activeStartedAt = reminderActiveStartedAt else {
            return
        }

        let activeDuration = now.timeIntervalSince(activeStartedAt)
        let durationMin = "\(max(1, Int(activeDuration / 60)))"
        let payload = [
            "durationMin": durationMin,
            "tool": activeSnapshots.first?.tool ?? petSpeechL("pet.speech.payload.tool", "you"),
            "project": activeSnapshots.first?.projectName ?? petSpeechL("pet.speech.payload.project", "this task"),
            "hourLabel": String(
                format: petSpeechL("pet.speech.payload.hour_format", "%d:00"),
                Calendar.current.component(.hour, from: now)
            ),
        ]

        if petSettings.sedentaryReminderEnabled,
           activeDuration >= petSettings.sedentaryReminderInterval,
           reminderCooldownAllows(
               kind: .reminderSedentary,
               interval: petSettings.sedentaryReminderInterval,
               now: now
           ) {
            notify(
                PetSpeechEvent(
                    kind: .reminderSedentary,
                    payload: payload,
                    occurredAt: now
                )
            )
        }

        if petSettings.hydrationReminderEnabled,
           activeDuration >= petSettings.hydrationReminderInterval,
           reminderCooldownAllows(
               kind: .reminderHydration,
               interval: petSettings.hydrationReminderInterval,
               now: now
           ) {
            notify(
                PetSpeechEvent(
                    kind: .reminderHydration,
                    payload: payload,
                    occurredAt: now
                )
            )
        }

        let hour = Calendar.current.component(.hour, from: now)
        if petSettings.lateNightReminderEnabled,
           hour >= 23 || hour < 6,
           reminderCooldownAllows(
               kind: .reminderLateNight,
               interval: petSettings.lateNightReminderInterval,
               now: now
           ) {
            notify(
                PetSpeechEvent(
                    kind: .reminderLateNight,
                    payload: payload,
                    occurredAt: now
                )
            )
        }
    }

    private func reminderCooldownAllows(
        kind: PetSpeechEventKind,
        interval: TimeInterval,
        now: Date
    ) -> Bool {
        guard let lastSpeechAt = lastSpeechByEventKind[kind] else {
            return true
        }
        return now.timeIntervalSince(lastSpeechAt) >= interval
    }
}
