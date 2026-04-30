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
            return "Tch. \nAgain? \nListen. \nFine. \nSure. \nWow. \nEyes on you. \nSame act. "
        case .encourage:
            return "Saw it. \nYep. \nNice. \nTake your time. \nI'm here. \nEasy now. \nPretty good. \nRight with you. "
        case .flirty:
            return "Hey. \nCaught you. \nMmm. \nNot bad. \nEyes on you. \nNoted. \nCloser now. \nAw. "
        case .chuunibyou:
            return "Witnessed. \nBehold. \nHear me. \nThe pact stirs. \nBy fate's pen, \nThe seal trembles. \nMortal, \nBy the black flame, "
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
            return "At it again, are we.\nLoud effort, mid result.\nBarely pulled that off.\nDon't go bragging yet.\nSaw that. Holding it in.\nWow, that worked? Bold.\nSame script as last time.\nPretending I didn't see? Saw.\nCreative move there.\nStop, I'll laugh.\nFine. Adding to your file.\nActing casual won't help."
        case .encourage:
            return "I'm watching. No rush.\nThat step felt solid.\nSet your own pace.\nStuck is fine. Pause.\nOff course? Just adjust.\nTaking a break is fine.\nYou're moving. That's enough.\nGetting here already counts.\nGood enough is good.\nSmall steps still count.\nI'll sit with you.\nBreathe if you need to."
        case .flirty:
            return "Stared a while, you didn't notice.\nThat move? Cute.\nDon't go, let me look.\nFocused you is the good you.\nMy heart did the thing.\nLogging that reaction.\nLook up. I'm waiting.\nYou set my mood, you know.\nThis vibe — I like it.\nKeep that up and I blush.\nThrow me a glance, please.\nSmooth rhythm. Show off."
        case .chuunibyou:
            return "Fate echoes once more.\nInscribed in the codex.\nThe pact still burns.\nStars shift quietly.\nSeal stable. For now.\nStorm has not passed.\nYour will, I perceive.\nNight parts for you.\nPower gathers at your hand.\nBattle continues.\nEmbers still shine.\nPrologue only. Onward."
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
            turn.started={tool} started. Don't ghost it.
            turn.completed={tool} done. Barely passes.
            turn.completedFast={tool} in {durationSec}? Lucky.
            turn.completedLong={tool} took {durationMin}min. Slow.
            turn.needsInput={tool} froze. Save it, hero.
            turn.interrupted={tool} cut off. Awkward.
            tool.switched={prevTool} out, {tool} in. Again.
            idle.entered=Five quiet min. Keyboard's cold.
            tokens.burst={tool} ate {tokensK}. Hungry.
            night.entered={hourLabel} and still up.
            idle.returned=Gone {minutesAway}min. Finally back.
            tool.multiStreak={toolList} all open. Showoff.
            pet.levelUp=Lv.{level}. Don't act surprised.
            pet.statBreakthrough={stat} past {value}. Wild.
            pet.evolution={newStage} with the entrance.
            usage.dailyRecord={tokensK} today. Glutton.
            reminder.hydration={durationMin}min in. Water? No?
            reminder.sedentary=Sat {durationMin}min. Legs gone.
            reminder.lateNight={hourLabel}. Sleep, drama queen.
            """
        case .encourage:
            return """
            turn.started={tool} is going. I'm here.
            turn.completed={tool} done. Solid step.
            turn.completedFast={tool} took {durationSec}. Snappy.
            turn.completedLong={tool} took {durationMin}min. Got there.
            turn.needsInput={tool} is waiting on you.
            turn.interrupted={tool} stopped. Breathe and retry.
            tool.switched={prevTool} to {tool}. Smooth.
            idle.entered=A pause is fine.
            tokens.burst={tool} pushed {tokensK}. Big push.
            night.entered={hourLabel} and still here.
            idle.returned=Back after {minutesAway}min.
            tool.multiStreak={toolList} all running.
            pet.levelUp=Lv.{level} unlocked.
            pet.statBreakthrough={stat} hit {value}.
            pet.evolution=You became {newStage}.
            usage.dailyRecord={tokensK} today. New high.
            reminder.hydration={durationMin}min and no water.
            reminder.sedentary=Sat for {durationMin}min. Stretch.
            reminder.lateNight={hourLabel}. Wrap it up soon.
            """
        case .flirty:
            return """
            turn.started={tool} started. Eyes on you.
            turn.completed={tool} done. That felt nice.
            turn.completedFast={tool} in {durationSec}. Charming.
            turn.completedLong={durationMin}min with you. Intense.
            turn.needsInput={tool} wants a word from you.
            turn.interrupted={tool} cut off. I paused too.
            tool.switched={prevTool} to {tool}. Nice taste.
            idle.entered=Quiet... missing me?
            tokens.burst={tool} burned {tokensK}. Hot hands.
            night.entered={hourLabel} and you stayed. Sweet.
            idle.returned=Gone {minutesAway}min. I waited.
            tool.multiStreak={toolList} all here. Popular.
            pet.levelUp=Lv.{level}. Getting fancy.
            pet.statBreakthrough={stat} past {value}. Wow.
            pet.evolution={newStage}. Even your eyes changed.
            usage.dailyRecord={tokensK} today. New high. Hot.
            reminder.hydration={durationMin}min with me. Water?
            reminder.sedentary=Sat too long. Stand for me.
            reminder.lateNight={hourLabel}. Sleep, please?
            """
        case .chuunibyou:
            return """
            turn.started={tool} ritual begins.
            turn.completed={tool} rite complete.
            turn.completedFast={tool} cut the mist in {durationSec}.
            turn.completedLong={durationMin}min trial crossed.
            turn.needsInput={tool} summons your verdict.
            turn.interrupted={tool} ritual broken.
            tool.switched={prevTool} falls. {tool} rises.
            idle.entered=Silence falls. Core sleeps.
            tokens.burst={tool} drank {tokensK}. Power surges.
            night.entered={hourLabel}. Night pact opens.
            idle.returned=After {minutesAway}min stillness, you return.
            tool.multiStreak={toolList} resonate.
            pet.levelUp=Lv.{level}. Seal weakens.
            pet.statBreakthrough={stat} past {value}.
            pet.evolution={newStage} manifests.
            usage.dailyRecord=Today {tokensK}. Codex rewrites.
            reminder.hydration={durationMin}min of ritual. Drink.
            reminder.sedentary=The seated seal forms. Rise.
            reminder.lateNight={hourLabel}. Night deep. Conserve.
            """
        case .off, .mixed:
            return defaultCores(mode: .encourage)
        }
    }
}
