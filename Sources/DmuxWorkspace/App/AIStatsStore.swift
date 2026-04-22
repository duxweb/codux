import Darwin
import Foundation
import Observation

extension Notification.Name {
    static let dmuxAIRuntimeActivityPulse = Notification.Name("dmuxAIRuntimeActivityPulse")
}

@MainActor
@Observable
final class AIStatsStore {
    enum RefreshTrigger: Equatable {
        case initial
        case manual
        case automatic
        case background
    }

    enum LiveRefreshReason: String {
        case runtimeBridge = "runtime-bridge"
        case runtimePoll = "runtime-poll"
        case terminalFocus = "terminal-focus"
    }

    typealias LiveSnapshotContext = (
        display: [AITerminalSessionSnapshot],
        summary: [AITerminalSessionSnapshot],
        current: AITerminalSessionSnapshot?
    )

    var state = AIStatsPanelState.empty
    var refreshState: PanelRefreshState = .idle
    var isAutomaticRefreshInProgress = false
    var renderVersion: UInt64 = 0

    let aiUsageService = AIUsageService()
    let aiUsageStore = AIUsageStore()
    let runtimeIngressService = AIRuntimeIngressService.shared
    let aiSessionStore = AISessionStore.shared
    let logger = AppDebugLog.shared
    var refreshTasks: [UUID: Task<Void, Never>] = [:]
    var indexingStatusByProjectID: [UUID: AIIndexingStatus] = [:]
    var panelStateByProjectID: [UUID: AIStatsPanelState] = [:]
    var refreshStateByProjectID: [UUID: PanelRefreshState] = [:]
    var automaticRefreshInProgressByProjectID: [UUID: Bool] = [:]
    var manualRefreshInProgressByProjectID: [UUID: Bool] = [:]
    var lastCompletedRefreshAtByProjectID: [UUID: Date] = [:]
    var openedProjectIDsThisLaunch: Set<UUID> = []
    var cachedPanels = RecentProjectCache<AIStatsPanelState>()
    var refreshTimer: Timer?
    var backgroundRefreshTimer: Timer?
    var runtimeBridgeObserver: NSObjectProtocol?
    var terminalFocusObserver: NSObjectProtocol?
    var pendingLiveRefreshTask: Task<Void, Never>?
    var pendingLiveRefreshReason: LiveRefreshReason?
    var currentProjectID: UUID?
    var currentSelectedSessionID: UUID?
    var currentProjects: [Project] = []
    var panelVisibilityProvider: (@MainActor () -> Bool)?
    var selectedProjectProvider: (@MainActor () -> Project?)?
    var selectedSessionIDProvider: (@MainActor () -> UUID?)?
    var projectsProvider: (@MainActor () -> [Project])?
    var automaticRefreshInterval: TimeInterval = 180
    var backgroundRefreshInterval: TimeInterval = 600
    let debugAIFocus = ProcessInfo.processInfo.environment["DMUX_DEBUG_AI_FOCUS"] == "1"

    func effectiveSessionID(_ selectedSessionID: UUID?) -> UUID? {
        let focusedSessionID = DmuxTerminalBackend.shared.registry.focusedSessionID()
        let resolvedSessionID = focusedSessionID ?? selectedSessionID
        if debugAIFocus {
            let registrySnapshot = DmuxTerminalBackend.shared.registry.debugSnapshot()
            print("[AIStats] focusedSessionID=\(focusedSessionID?.uuidString ?? "nil") selectedSessionID=\(selectedSessionID?.uuidString ?? "nil") resolvedSessionID=\(resolvedSessionID?.uuidString ?? "nil") registry=[\(registrySnapshot)]")
        }
        return resolvedSessionID
    }
}
