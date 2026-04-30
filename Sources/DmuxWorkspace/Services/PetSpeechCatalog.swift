import Foundation

@MainActor
final class PetSpeechCatalog {
    private let recentWindowSize = 5
    private var recentlyUsedByPool: [String: [String]] = [:]
    private var lastTemplateByPool: [String: String] = [:]

    func pickLine(mode requestedMode: PetSpeechMode, event: PetSpeechEvent) -> PetSpeechLine {
        let mode = resolvedMode(requestedMode)
        let template = pickTemplate(mode: mode, eventKind: event.kind)
        let text = render(template.text, payload: event.payload)
        return PetSpeechLine(
            text: text,
            source: template.source,
            eventKind: event.kind,
            createdAt: event.occurredAt,
            ttl: ttl(for: event.tier)
        )
    }

    func templateCount(mode requestedMode: PetSpeechMode, eventKind: PetSpeechEventKind) -> Int {
        let mode = requestedMode == .mixed ? .roast : requestedMode
        return templates(mode: mode, eventKind: eventKind).count
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

    private func pickTemplate(mode: PetSpeechMode, eventKind: PetSpeechEventKind) -> (text: String, source: PetSpeechLineSource) {
        let poolKey = "\(mode.rawValue)|\(eventKind.rawValue)"
        var pool = templates(mode: mode, eventKind: eventKind)
        var source: PetSpeechLineSource = .template
        if pool.isEmpty {
            pool = fallbackTemplates(mode: mode)
            source = .fallback
        }
        if pool.isEmpty {
            pool = [localizedFallbackLine()]
            source = .fallback
        }

        let recent = Set(recentlyUsedByPool[poolKey] ?? [])
        var candidates = pool.filter { recent.contains($0) == false }
        if candidates.isEmpty {
            candidates = pool
        }

        var selected = candidates.randomElement() ?? pool[0]
        if candidates.count > 1,
           let previous = lastTemplateByPool[poolKey],
           selected == previous {
            selected = candidates.first { $0 != previous } ?? selected
        }

        var nextRecent = recentlyUsedByPool[poolKey] ?? []
        nextRecent.append(selected)
        if nextRecent.count > recentWindowSize {
            nextRecent.removeFirst(nextRecent.count - recentWindowSize)
        }
        recentlyUsedByPool[poolKey] = nextRecent
        lastTemplateByPool[poolKey] = selected

        return (selected, source)
    }

    private func templates(mode: PetSpeechMode, eventKind: PetSpeechEventKind) -> [String] {
        guard let core = eventCore(mode: mode, eventKind: eventKind) else {
            return []
        }
        return openers(mode: mode).map { "\($0)\(core)" }
    }

    private func ttl(for tier: PetSpeechTier) -> TimeInterval {
        switch tier {
        case .daily: return 6
        case .rhythm: return 8
        case .milestone: return 10
        }
    }

    private func render(_ template: String, payload: [String: String]) -> String {
        var values = defaultPayload()
        for (key, value) in payload {
            let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                values[key] = trimmed
            }
        }

        var text = template
        for (key, value) in values {
            text = text.replacingOccurrences(of: "{\(key)}", with: value)
        }
        text = text.replacingOccurrences(
            of: #"\{[^}]+\}"#,
            with: "",
            options: .regularExpression
        )
        text = text.replacingOccurrences(
            of: #"\s+"#,
            with: " ",
            options: .regularExpression
        )
        .trimmingCharacters(in: .whitespacesAndNewlines)

        if text.isEmpty {
            text = localizedFallbackLine()
        }
        if text.count > 36 {
            let endIndex = text.index(text.startIndex, offsetBy: 35)
            text = String(text[..<endIndex]) + "…"
        }
        return text
    }

    private func defaultPayload() -> [String: String] {
        [
            "tokensK": petSpeechL("pet.speech.payload.tokens_k", "that last burst"),
            "durationMin": petSpeechL("pet.speech.payload.duration_min", "a while"),
            "durationSec": petSpeechL("pet.speech.payload.duration_sec", "a few seconds"),
            "tool": petSpeechL("pet.speech.payload.tool", "you"),
            "model": "AI",
            "project": petSpeechL("pet.speech.payload.project", "this task"),
            "petName": petSpeechL("pet.speech.payload.pet_name", "Little One"),
            "tokens": petSpeechL("pet.speech.payload.tokens", "that last burst"),
            "reqCount": "",
            "streakDays": "",
            "hourLabel": petSpeechL("pet.speech.payload.hour_label", "this hour"),
            "stat": "",
            "value": "",
            "level": "",
            "prevTool": "",
            "minutesAway": petSpeechL("pet.speech.payload.minutes_away", "a while"),
            "toolList": "",
            "newStage": "",
        ]
    }

    private func openers(mode: PetSpeechMode) -> [String] {
        let resolvedMode = PetSpeechMode.concreteModes.contains(mode) ? mode : .encourage
        return localizedLines(
            key: "pet.speech.catalog.\(resolvedMode.rawValue).openers",
            defaultValue: defaultOpeners(mode: resolvedMode)
        )
    }

    private func defaultOpeners(mode: PetSpeechMode) -> String {
        switch mode {
        case .roast:
            return "Tch,\nFine,\nSure,\nI see it,\nWild, but fine,\nDo not act relaxed,\nI am watching,\nReally?"
        case .encourage:
            return "Steady,\nNice,\nKeep going,\nGood rhythm,\nSolid step,\nHold it,\nProgress,\nI see it,"
        case .flirty:
            return "Oh,\nNot bad,\nLet me look closer,\nSmooth,\nDo not be shy,\nI noticed,\nThat feel,\nThere you go,"
        case .chuunibyou:
            return "I witness,\nAt this moment,\nThe pact rings,\nFate records it,\nThe stars shift,\nThe seal trembles,\nThe storm whispers,\nAwaken,"
        case .off, .mixed:
            return defaultOpeners(mode: .encourage)
        }
    }

    private func fallbackTemplates(mode: PetSpeechMode) -> [String] {
        let resolvedMode = PetSpeechMode.concreteModes.contains(mode) ? mode : .encourage
        return localizedLines(
            key: "pet.speech.catalog.\(resolvedMode.rawValue).fallbacks",
            defaultValue: defaultFallbacks(mode: resolvedMode)
        )
    }

    private func defaultFallbacks(mode: PetSpeechMode) -> String {
        switch mode {
        case .roast:
            return "Fine, it happened again.\nI saw that. Do not float away.\nThis one barely passes.\nDo not get proud yet.\nThat was not quiet.\nAcceptable. Barely.\nI am writing that down.\nKeep going. Do not stop.\nThis is getting interesting.\nI say nothing, but I know.\nYou are at it again.\nThat almost counts as progress."
        case .encourage:
            return "I saw it. Keep going.\nThat step was steady.\nKeep this rhythm.\nProgress is showing.\nThat was solid work.\nSteady is enough.\nGood state.\nNo rush. The direction is right.\nOne more step forward.\nThis rhythm works.\nSmall steps still count.\nI am watching with you."
        case .flirty:
            return "I saw that.\nThat was pretty smooth.\nDo not stop. It looks good.\nYou are good when focused.\nI liked that one.\nLet me look closer.\nThere is something there.\nYou earned one glance.\nYour state is nice.\nI will remember that.\nKeep showing me.\nThat rhythm has charm."
        case .chuunibyou:
            return "Fate has echoed.\nI have recorded this moment.\nThe pact still burns.\nThe stars acknowledge you.\nThe seal is stable for now.\nThis enters the grimoire.\nThe storm has not stopped.\nProceed onward.\nPower is condensing.\nNight makes way for you.\nThe ritual is not over.\nThe embers still shine."
        case .off, .mixed:
            return defaultFallbacks(mode: .encourage)
        }
    }

    private func eventCore(mode: PetSpeechMode, eventKind: PetSpeechEventKind) -> String? {
        let resolvedMode = PetSpeechMode.concreteModes.contains(mode) ? mode : .encourage
        let cores = localizedCoreMap(
            key: "pet.speech.catalog.\(resolvedMode.rawValue).cores",
            defaultValue: defaultCores(mode: resolvedMode)
        )
        return cores[eventKind.rawValue]
    }

    private func localizedLines(key: String, defaultValue: String) -> [String] {
        petSpeechL(key, defaultValue)
            .components(separatedBy: "\n")
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
    }

    private func localizedCoreMap(key: String, defaultValue: String) -> [String: String] {
        var result: [String: String] = [:]
        for line in localizedLines(key: key, defaultValue: defaultValue) {
            guard let separator = line.firstIndex(of: "=") else {
                continue
            }
            let rawKey = line[..<separator].trimmingCharacters(in: .whitespacesAndNewlines)
            let rawValue = line[line.index(after: separator)...].trimmingCharacters(in: .whitespacesAndNewlines)
            if !rawKey.isEmpty, !rawValue.isEmpty {
                result[String(rawKey)] = String(rawValue)
            }
        }
        return result
    }

    private func localizedFallbackLine() -> String {
        petSpeechL("pet.speech.catalog.fallback_line", "I saw it. Keep going.")
    }

    private func defaultCores(mode: PetSpeechMode) -> String {
        switch mode {
        case .roast:
            return """
            turn.started={tool} started. Do not wander off halfway.
            turn.completed={tool} is done. Barely acceptable.
            turn.completedFast={tool} finished in {durationSec}. Efficient, somehow.
            turn.completedLong={tool} took {durationMin} minutes and finally produced it.
            turn.needsInput={tool} is stuck. Your rescue scene is up.
            turn.interrupted={tool} got interrupted. Awkward little scene.
            tool.switched={prevTool} to {tool}. New flavor again.
            idle.entered=Five quiet minutes. Even the keyboard cooled down.
            tokens.burst={tool} burned {tokensK} in half an hour. Hungry thing.
            night.entered=Working at {hourLabel}. Your schedule is rebellious.
            idle.returned=Gone for {minutesAway} minutes, finally back.
            tool.multiStreak={toolList} all running. Busy little stage.
            pet.levelUp=Lv.{level}. Do not pretend you missed it.
            pet.statBreakthrough={stat} passed {value}. Slightly absurd.
            pet.evolution={newStage} appeared. Dramatic entrance granted.
            usage.dailyRecord={tokensK} today. Feeding the machine, are we?
            reminder.hydration={durationMin} minutes in. That cup is decoration.
            reminder.sedentary={durationMin} minutes seated. Reboot your legs.
            reminder.lateNight={hourLabel} and still pushing. Do not crash first.
            """
        case .encourage:
            return """
            turn.started={tool} started processing. Stay steady.
            turn.completed={tool} finished. This step moved forward.
            turn.completedFast={tool} finished in {durationSec}. Sharp response.
            turn.completedLong={durationMin} minutes held steady. Good rhythm.
            turn.needsInput={tool} is waiting for you. One step will move it.
            turn.interrupted={tool} stopped. Try another angle.
            tool.switched=From {prevTool} to {tool}. Nice handoff.
            idle.entered=A pause is fine. Leave room for thought.
            tokens.burst={tool} pushed {tokensK}. Solid output.
            night.entered=Still online at {hourLabel}. Keep some energy.
            idle.returned=Back after {minutesAway} minutes. Good timing.
            tool.multiStreak={toolList} in parallel. Stable scheduling.
            pet.levelUp=Reached Lv.{level}. Growth is visible.
            pet.statBreakthrough={stat} reached {value}. The work is adding up.
            pet.evolution=Evolved into {newStage}. The effort echoed back.
            usage.dailyRecord={tokensK} today, a new high. Solid work.
            reminder.hydration={durationMin} minutes busy. Drink water, then continue.
            reminder.sedentary={durationMin} minutes straight. Stand up for a bit.
            reminder.lateNight=Still working at {hourLabel}. Wrap up when you can.
            """
        case .flirty:
            return """
            turn.started={tool} started. I am watching.
            turn.completed={tool} finished. That felt nice.
            turn.completedFast={tool} finished in {durationSec}. Pretty charming.
            turn.completedLong=I stayed with you for {durationMin} minutes. Kind of intense.
            turn.needsInput={tool} is waiting for one word from you.
            turn.interrupted={tool} got cut off. I paused too.
            tool.switched={prevTool} to {tool}. Good taste.
            idle.entered=Suddenly quiet. I almost thought you missed me.
            tokens.burst={tool} rushed through {tokensK}. Warm hands.
            night.entered=Still with me at {hourLabel}. That is sweet.
            idle.returned=Away for {minutesAway} minutes. I waited.
            tool.multiStreak={toolList} all here. Popular, aren't you?
            pet.levelUp=Lv.{level}. Getting better and better.
            pet.statBreakthrough={stat} passed {value}. Hard to hide now.
            pet.evolution={newStage} arrived. Even the eyes changed.
            usage.dailyRecord={tokensK} today, a new high. Smooth with machines.
            reminder.hydration=I stayed {durationMin} minutes. Drink water.
            reminder.sedentary=Too long seated. Stand up and let me see.
            reminder.lateNight=Still awake at {hourLabel}. I might worry.
            """
        case .chuunibyou:
            return """
            turn.started={tool} ritual begins. The runes are lit.
            turn.completed={tool} ritual complete. The afterglow remains.
            turn.completedFast={tool} cut the mist in {durationSec}.
            turn.completedLong=The {durationMin}-minute trial has been crossed.
            turn.needsInput={tool} summons your judgment.
            turn.interrupted={tool} ritual broke. The ripple remains.
            tool.switched={prevTool} exits. {tool} takes the blade.
            idle.entered=Silence descends. The core sleeps.
            tokens.burst={tool} consumed {tokensK}. Magic surges.
            night.entered={hourLabel}. The night pact opens.
            idle.returned=After {minutesAway} minutes of stillness, you return.
            tool.multiStreak={toolList} resonate. The array unfolds.
            pet.levelUp=Lv.{level} awakens. The seal loosens.
            pet.statBreakthrough={stat} crossed the {value} boundary.
            pet.evolution={newStage} manifests. The stars bear witness.
            usage.dailyRecord=Today {tokensK} broke the record. The grimoire rewrites itself.
            reminder.hydration=After {durationMin} minutes of ritual, drink to restore magic.
            reminder.sedentary=The sitting seal has formed. Rise at once.
            reminder.lateNight={hourLabel}. Deep night. Preserve your strength.
            """
        case .off, .mixed:
            return defaultCores(mode: .encourage)
        }
    }
}
