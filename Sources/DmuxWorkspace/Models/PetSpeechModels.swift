import Foundation

func petSpeechL(_ key: String, _ defaultValue: String) -> String {
    Bundle.module.localizedString(forKey: key, value: defaultValue, table: nil)
}

enum PetSpeechMode: String, Codable, CaseIterable, Identifiable, Sendable {
    case off
    case roast
    case encourage
    case flirty
    case chuunibyou
    case mixed

    var id: String { rawValue }

    var title: String {
        switch self {
        case .off: return petL("pet.speech.mode.off", "Off")
        case .roast: return petL("pet.speech.mode.roast", "Roast")
        case .encourage: return petL("pet.speech.mode.encourage", "Encourage")
        case .flirty: return petL("pet.speech.mode.flirty", "Playful")
        case .chuunibyou: return petL("pet.speech.mode.chuunibyou", "Chuunibyou")
        case .mixed: return petL("pet.speech.mode.mixed", "Mixed")
        }
    }

    static let concreteModes: [PetSpeechMode] = [.roast, .encourage, .flirty, .chuunibyou]
}

enum PetSpeechFrequency: String, Codable, CaseIterable, Identifiable, Sendable {
    case quiet
    case normal
    case lively
    case chatterbox

    var id: String { rawValue }

    var title: String {
        switch self {
        case .quiet: return petL("pet.speech.frequency.quiet", "Quiet")
        case .normal: return petL("pet.speech.frequency.normal", "Normal")
        case .lively: return petL("pet.speech.frequency.lively", "Lively")
        case .chatterbox: return petL("pet.speech.frequency.chatterbox", "Chatty")
        }
    }

    var config: PetSpeechFrequencyConfig {
        switch self {
        case .quiet:
            return PetSpeechFrequencyConfig(
                globalCooldown: 300,
                perEventCooldown: 1800,
                minimumTier: .milestone,
                lv1SuppressRate: 1,
                estimatedHourlyCount: "0-1"
            )
        case .normal:
            return PetSpeechFrequencyConfig(
                globalCooldown: 60,
                perEventCooldown: 300,
                minimumTier: .rhythm,
                lv1SuppressRate: 0.5,
                estimatedHourlyCount: "1-3"
            )
        case .lively:
            return PetSpeechFrequencyConfig(
                globalCooldown: 30,
                perEventCooldown: 120,
                minimumTier: .daily,
                lv1SuppressRate: 0,
                estimatedHourlyCount: "3-8"
            )
        case .chatterbox:
            return PetSpeechFrequencyConfig(
                globalCooldown: 30,
                perEventCooldown: 60,
                minimumTier: .daily,
                lv1SuppressRate: 0,
                estimatedHourlyCount: "8-15"
            )
        }
    }

    func lowered() -> PetSpeechFrequency {
        switch self {
        case .quiet: return .quiet
        case .normal: return .quiet
        case .lively: return .normal
        case .chatterbox: return .lively
        }
    }

    func raised() -> PetSpeechFrequency {
        switch self {
        case .quiet: return .normal
        case .normal: return .lively
        case .lively: return .chatterbox
        case .chatterbox: return .chatterbox
        }
    }
}

struct PetSpeechFrequencyConfig: Equatable, Sendable {
    var globalCooldown: TimeInterval
    var perEventCooldown: TimeInterval
    var minimumTier: PetSpeechTier
    var lv1SuppressRate: Double
    var estimatedHourlyCount: String
}

enum PetSpeechTier: Int, Codable, Comparable, Sendable {
    case daily = 1
    case rhythm = 2
    case milestone = 3

    static func < (lhs: PetSpeechTier, rhs: PetSpeechTier) -> Bool {
        lhs.rawValue < rhs.rawValue
    }
}

enum PetSpeechEventKind: String, Codable, CaseIterable, Sendable {
    case turnStarted = "turn.started"
    case turnCompleted = "turn.completed"
    case turnCompletedFast = "turn.completedFast"
    case turnCompletedLong = "turn.completedLong"
    case turnNeedsInput = "turn.needsInput"
    case turnInterrupted = "turn.interrupted"
    case toolSwitched = "tool.switched"
    case idleEntered = "idle.entered"
    case tokensBurst = "tokens.burst"
    case nightEntered = "night.entered"
    case idleReturned = "idle.returned"
    case toolMultiStreak = "tool.multiStreak"
    case petLevelUp = "pet.levelUp"
    case petStatBreakthrough = "pet.statBreakthrough"
    case petEvolution = "pet.evolution"
    case usageDailyRecord = "usage.dailyRecord"
    case reminderHydration = "reminder.hydration"
    case reminderSedentary = "reminder.sedentary"
    case reminderLateNight = "reminder.lateNight"

    var tier: PetSpeechTier {
        switch self {
        case .turnStarted, .turnCompleted, .turnCompletedFast, .turnCompletedLong, .turnNeedsInput, .turnInterrupted, .toolSwitched, .idleEntered:
            return .daily
        case .tokensBurst, .nightEntered, .idleReturned, .toolMultiStreak, .reminderHydration, .reminderSedentary, .reminderLateNight:
            return .rhythm
        case .petLevelUp, .petStatBreakthrough, .petEvolution, .usageDailyRecord:
            return .milestone
        }
    }

    var isReminder: Bool {
        switch self {
        case .reminderHydration, .reminderSedentary, .reminderLateNight:
            return true
        default:
            return false
        }
    }
}

struct PetSpeechEvent: Equatable, Sendable {
    var kind: PetSpeechEventKind
    var payload: [String: String]
    var occurredAt: Date

    init(
        kind: PetSpeechEventKind,
        payload: [String: String] = [:],
        occurredAt: Date = Date()
    ) {
        self.kind = kind
        self.payload = payload
        self.occurredAt = occurredAt
    }

    var tier: PetSpeechTier { kind.tier }
    var isHardOverride: Bool { tier == .milestone }
}

enum PetSpeechLineSource: String, Codable, Sendable {
    case template
    case fallback
    case llm
}

struct PetSpeechLine: Identifiable, Equatable, Sendable {
    let id = UUID()
    var text: String
    var source: PetSpeechLineSource
    var eventKind: PetSpeechEventKind
    var createdAt: Date
    var ttl: TimeInterval

    var expiresAt: Date {
        createdAt.addingTimeInterval(ttl)
    }
}

struct PetSpeechActivitySnapshot: Equatable, Sendable {
    var tool: String
    var model: String?
    var projectName: String
    var state: String
    var updatedAt: Date
    var activeStartedAt: Date?
    var totalTokens: Int
}

struct AppAIPetSettings: Codable, Equatable, Sendable {
    static let automaticSpeechProviderID = "automatic"

    var speechMode: PetSpeechMode = .off
    var speechFrequency: PetSpeechFrequency = .normal
    var speechLLMEnabled = false
    var speechProviderID = Self.automaticSpeechProviderID
    var speechQuietDuringWork = true
    var speechLouderAtNight = false
    var speechMuteOnFullscreen = true
    var speechQuietHoursStart: Int?
    var speechQuietHoursEnd: Int?
    var speechTemporaryMuteUntil: Date?

    init() {}

    enum CodingKeys: String, CodingKey {
        case speechMode
        case speechFrequency
        case speechLLMEnabled
        case speechProviderID
        case speechQuietDuringWork
        case speechLouderAtNight
        case speechMuteOnFullscreen
        case speechQuietHoursStart
        case speechQuietHoursEnd
        case speechTemporaryMuteUntil
    }

    init(from decoder: Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        speechMode = try container.decodeIfPresent(PetSpeechMode.self, forKey: .speechMode) ?? .off
        speechFrequency = try container.decodeIfPresent(PetSpeechFrequency.self, forKey: .speechFrequency) ?? .normal
        speechLLMEnabled = try container.decodeIfPresent(Bool.self, forKey: .speechLLMEnabled) ?? false
        let decodedProviderID = try container.decodeIfPresent(String.self, forKey: .speechProviderID) ?? Self.automaticSpeechProviderID
        speechProviderID = decodedProviderID.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            ? Self.automaticSpeechProviderID
            : decodedProviderID
        speechQuietDuringWork = try container.decodeIfPresent(Bool.self, forKey: .speechQuietDuringWork) ?? true
        speechLouderAtNight = try container.decodeIfPresent(Bool.self, forKey: .speechLouderAtNight) ?? false
        speechMuteOnFullscreen = try container.decodeIfPresent(Bool.self, forKey: .speechMuteOnFullscreen) ?? true
        speechQuietHoursStart = Self.normalizedHour(try container.decodeIfPresent(Int.self, forKey: .speechQuietHoursStart))
        speechQuietHoursEnd = Self.normalizedHour(try container.decodeIfPresent(Int.self, forKey: .speechQuietHoursEnd))
        speechTemporaryMuteUntil = try container.decodeIfPresent(Date.self, forKey: .speechTemporaryMuteUntil)
    }

    static func normalizedHour(_ hour: Int?) -> Int? {
        guard let hour else {
            return nil
        }
        return min(23, max(0, hour))
    }
}
