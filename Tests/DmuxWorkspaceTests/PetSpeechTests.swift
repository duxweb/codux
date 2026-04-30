import XCTest
@testable import DmuxWorkspace

@MainActor
final class PetSpeechCatalogTests: XCTestCase {
    func testConcreteModesHaveEightTemplatesForEveryEvent() {
        let catalog = PetSpeechCatalog()
        for mode in PetSpeechMode.concreteModes {
            for kind in PetSpeechEventKind.allCases {
                XCTAssertGreaterThanOrEqual(
                    catalog.templateCount(mode: mode, eventKind: kind),
                    8,
                    "\(mode.rawValue) \(kind.rawValue)"
                )
            }
        }
    }

    func testMissingPayloadNeverReturnsEmptyOrRawPlaceholder() {
        let catalog = PetSpeechCatalog()
        for mode in PetSpeechMode.concreteModes {
            for kind in PetSpeechEventKind.allCases {
                let line = catalog.pickLine(
                    mode: mode,
                    event: PetSpeechEvent(kind: kind)
                )
                XCTAssertFalse(line.text.isEmpty, "\(mode.rawValue) \(kind.rawValue)")
                XCTAssertFalse(line.text.contains("{"), line.text)
                XCTAssertFalse(line.text.contains("}"), line.text)
                XCTAssertLessThanOrEqual(line.text.count, 36, line.text)
            }
        }
    }

    func testTemplatePickerKeepsBasicVariety() {
        let catalog = PetSpeechCatalog()
        var lines = Set<String>()
        for _ in 0 ..< 100 {
            lines.insert(
                catalog.pickLine(
                    mode: .encourage,
                    event: PetSpeechEvent(
                        kind: .turnCompletedLong,
                        payload: ["durationMin": "42", "tool": "codex", "tokensK": "12K"]
                    )
                ).text
            )
        }
        XCTAssertGreaterThanOrEqual(lines.count, 6)
    }
}

@MainActor
final class PetSpeechCoordinatorTests: XCTestCase {
    func testModeOffClearsAndSuppressesSpeech() {
        let coordinator = PetSpeechCoordinator()
        var settings = AppAIPetSettings()
        settings.speechMode = .encourage
        settings.speechFrequency = .lively
        coordinator.configure(
            settings: { settings },
            petName: { "测试宠" },
            activitySnapshots: { [] }
        )
        coordinator.notify(PetSpeechEvent(kind: .petLevelUp, payload: ["level": "2"]))
        XCTAssertNotNil(coordinator.currentLine)

        settings.speechMode = .off
        coordinator.notify(PetSpeechEvent(kind: .petLevelUp, payload: ["level": "3"]))
        XCTAssertNil(coordinator.currentLine)
    }

    func testModeOffStillAllowsReminderEvents() {
        let coordinator = PetSpeechCoordinator()
        let settings = AppAIPetSettings()
        coordinator.configure(
            settings: { settings },
            petName: { "测试宠" },
            activitySnapshots: { [] }
        )

        coordinator.notify(PetSpeechEvent(kind: .reminderSedentary, payload: ["durationMin": "30"]))
        XCTAssertNotNil(coordinator.currentLine)
        XCTAssertEqual(coordinator.currentLine?.eventKind, .reminderSedentary)
    }

    func testQuietFrequencySuppressesDailyButAllowsMilestone() {
        let coordinator = PetSpeechCoordinator()
        var settings = AppAIPetSettings()
        settings.speechMode = .encourage
        settings.speechFrequency = .quiet
        coordinator.configure(
            settings: { settings },
            petName: { "测试宠" },
            activitySnapshots: { [] }
        )

        coordinator.notify(PetSpeechEvent(kind: .turnCompletedFast, payload: ["tool": "codex"]))
        XCTAssertNil(coordinator.currentLine)

        coordinator.notify(PetSpeechEvent(kind: .petLevelUp, payload: ["level": "2"]))
        XCTAssertNotNil(coordinator.currentLine)
    }

    func testReminderEventsBypassSpeechFrequencyTier() {
        let coordinator = PetSpeechCoordinator()
        var settings = AppAIPetSettings()
        settings.speechMode = .encourage
        settings.speechFrequency = .quiet
        coordinator.configure(
            settings: { settings },
            petName: { "测试宠" },
            activitySnapshots: { [] }
        )

        coordinator.notify(PetSpeechEvent(kind: .reminderHydration, payload: ["durationMin": "120"]))
        XCTAssertNotNil(coordinator.currentLine)
        XCTAssertEqual(coordinator.currentLine?.eventKind, .reminderHydration)
    }

    func testLLMReplacementOnlyRunsForEligibleEvents() async {
        let coordinator = PetSpeechCoordinator()
        var aiSettings = AppAISettings()
        aiSettings.pet.speechMode = .encourage
        aiSettings.pet.speechFrequency = .normal
        aiSettings.pet.speechLLMEnabled = true
        aiSettings.pet.speechQuietDuringWork = false
        var requestedKinds: [PetSpeechEventKind] = []
        coordinator.configure(
            settings: { aiSettings.pet },
            aiSettings: { aiSettings },
            petName: { "测试宠" },
            activitySnapshots: { [] },
            llmLineProvider: { event, _, _ in
                requestedKinds.append(event.kind)
                return "LLM 台词"
            }
        )

        coordinator.notify(PetSpeechEvent(kind: .tokensBurst, payload: ["tokensK": "60K"]))
        XCTAssertEqual(coordinator.currentLine?.source, .template)

        try? await Task.sleep(for: .milliseconds(50))
        XCTAssertEqual(coordinator.currentLine?.text, "LLM 台词")
        XCTAssertEqual(coordinator.currentLine?.source, .llm)
        XCTAssertEqual(requestedKinds, [.tokensBurst])
    }
}

final class PetSpeechLLMServiceTests: XCTestCase {
    func testAuditPromptUsesMetadataOnly() {
        let prompt = PetSpeechLLMService.auditPrompt(
            event: PetSpeechEvent(
                kind: .turnCompletedLong,
                payload: [
                    "tool": "codex",
                    "model": "gpt-test",
                    "tokens": "12000",
                    "durationMin": "42",
                    "project": "demo",
                    "message": "不要泄露这段正文",
                    "body": "secret transcript",
                    "content": "private answer",
                ]
            ),
            mode: .roast
        )

        let combined = "\(prompt.systemPrompt)\n\(prompt.userPrompt)"
        XCTAssertTrue(combined.contains("codex"))
        XCTAssertTrue(combined.contains("gpt-test"))
        XCTAssertTrue(combined.contains("12000"))
        XCTAssertTrue(combined.contains("42"))
        XCTAssertTrue(combined.contains("demo"))
        XCTAssertFalse(combined.contains("不要泄露这段正文"))
        XCTAssertFalse(combined.contains("secret transcript"))
        XCTAssertFalse(combined.contains("private answer"))
    }
}
