import type {
  AIHookEventMetadata,
  AIHookEventPayload,
  AIProjectPhase,
  AIRuntimeBridgeSnapshot,
  AIRuntimeContextSnapshot,
  AISessionSnapshot,
  AIState,
} from "./types";
import { aiToolDriverFactory, type AIToolDriverFactory } from "./toolDrivers";
import { canonicalToolName, normalize, numberOr, projectPathContains, statusForState } from "./utils";

export type SessionStoreListener = () => void;

const RUNNING_STALE_MS = 90_000;

export class AISessionStore {
  private sessions = new Map<string, AISessionSnapshot>();
  private logicalBaselines = new Map<string, number>();
  private dismissedCompletedAt = new Map<string, number>();
  private latestActiveStartedAtByProject = new Map<string, number>();
  private listeners = new Set<SessionStoreListener>();

  constructor(private readonly toolDriverFactory: AIToolDriverFactory = aiToolDriverFactory) {}

  subscribe(listener: SessionStoreListener) {
    this.listeners.add(listener);
    return () => {
      this.listeners.delete(listener);
    };
  }

  snapshots(projectId?: string) {
    return [...this.sessions.values()]
      .filter((session) => (projectId ? session.projectId === projectId : true))
      .sort((left, right) => right.updatedAt - left.updatedAt);
  }

  sessionForTerminal(terminalId: string) {
    return this.sessions.get(terminalId);
  }

  runtimeTrackedSessions() {
    return [...this.sessions.values()].filter((session) => {
      if (session.state === "responding" || session.state === "needsInput") return true;
      return !session.hasCompletedTurn;
    });
  }

  projectPhase(projectId: string): AIProjectPhase {
    const sessions = this.snapshots(projectId);
    const needsInput = sessions.find((session) => session.state === "needsInput");
    if (needsInput) return { kind: "needsInput", tool: needsInput.tool };
    const running = sessions.find((session) => session.state === "responding");
    if (running) return { kind: "running", tool: running.tool };
    return { kind: "idle" };
  }

  completedPhase(projectId: string): AIProjectPhase {
    const sessions = this.snapshots(projectId);
    if (sessions.some((session) => session.state === "needsInput" || session.state === "responding")) {
      return { kind: "idle" };
    }
    const latestActiveStartedAt = this.latestActiveStartedAtByProject.get(projectId) ?? 0;
    const completed = sessions.find(
      (session) =>
        session.state === "idle" &&
        (session.hasCompletedTurn || session.wasInterrupted) &&
        session.updatedAt >= latestActiveStartedAt,
    );
    if (completed && !this.isCompletionDismissed(projectId, completed.updatedAt)) {
      return {
        kind: "completed",
        tool: completed.tool,
        wasInterrupted: completed.wasInterrupted,
        updatedAt: completed.updatedAt,
      };
    }
    return { kind: "idle" };
  }

  projectTotals(projectId?: string) {
    const sessions = this.snapshots(projectId);
    return sessions.reduce(
      (total, session) => ({
        totalTokens: total.totalTokens + Math.max(0, session.totalTokens - session.baselineTotalTokens),
        cachedInputTokens: total.cachedInputTokens + Math.max(0, session.cachedInputTokens),
        running: total.running + (session.state === "responding" ? 1 : 0),
        needsInput: total.needsInput + (session.state === "needsInput" ? 1 : 0),
        completed: total.completed + (session.hasCompletedTurn ? 1 : 0),
      }),
      { totalTokens: 0, cachedInputTokens: 0, running: 0, needsInput: 0, completed: 0 },
    );
  }

  dismissCompletion(projectId: string) {
    const completed = this.completedPhase(projectId);
    if (completed.kind !== "completed") return false;

    const previous = this.dismissedCompletedAt.get(projectId) ?? 0;
    const nextDismissedAt = Math.max(previous, completed.updatedAt);
    if (nextDismissedAt === previous) return false;

    this.dismissedCompletedAt.set(projectId, nextDismissedAt);
    this.emit();
    return true;
  }

  applyHook(event: AIHookEventPayload) {
    const terminalId = normalize(event.terminalID);
    const tool = this.toolDriverFactory.canonicalToolName(event.tool);
    if (!terminalId || !tool) return false;

    if (!projectPathContains(event.projectPath, event.metadata?.cwd)) return false;
    const previous = this.sessions.get(terminalId);
    const terminalInstanceId = normalize(event.terminalInstanceID);
    if (
      previous?.terminalInstanceId &&
      terminalInstanceId &&
      previous.terminalInstanceId !== terminalInstanceId &&
      event.updatedAt < previous.updatedAt
    ) {
      return false;
    }
    if (isToolActivityWithoutLoading(event, previous)) return false;

    const now = event.updatedAt || Date.now() / 1000;
    const shouldReset =
      previous &&
      (previous.tool !== tool ||
        (previous.terminalInstanceId && terminalInstanceId && previous.terminalInstanceId !== terminalInstanceId) ||
        (previous.aiSessionId &&
          normalize(event.aiSessionID) &&
          previous.aiSessionId !== normalize(event.aiSessionID)));
    const base = shouldReset ? undefined : previous;
    const aiSessionId = normalize(event.aiSessionID) ?? base?.aiSessionId;
    const logicalKey = aiSessionId ? `${tool}:${aiSessionId}` : undefined;
    const totalTokens = numberOr(base?.totalTokens, event.totalTokens);
    const baselineTotalTokens =
      base?.baselineTotalTokens ?? (logicalKey ? (this.logicalBaselines.get(logicalKey) ?? totalTokens) : 0);
    if (logicalKey && !this.logicalBaselines.has(logicalKey)) {
      this.logicalBaselines.set(logicalKey, baselineTotalTokens);
    }

    const state = nextState(event.kind, event.metadata);
    const wasInterrupted =
      event.kind === "turnCompleted" || event.kind === "sessionEnded"
        ? Boolean(event.metadata?.wasInterrupted ?? false)
        : (base?.wasInterrupted ?? false);
    const hasCompletedTurn =
      event.kind === "turnCompleted"
        ? event.metadata?.hasCompletedTurn !== false
        : event.kind === "sessionEnded"
          ? (base?.hasCompletedTurn ?? false)
          : (base?.hasCompletedTurn ?? false);

    if (event.kind === "sessionEnded" && base && !base.hasCompletedTurn) {
      this.sessions.delete(terminalId);
      this.emit();
      return true;
    }

    const activeTurnStartedAt =
      state === "responding"
        ? (base?.activeTurnStartedAt ?? now)
        : state === "needsInput"
          ? (base?.activeTurnStartedAt ?? now)
          : undefined;
    const didUpdateActiveStartedAt =
      activeTurnStartedAt != null && this.noteLatestActiveStartedAt(event.projectID, activeTurnStartedAt);

    const next: AISessionSnapshot = {
      terminalId,
      terminalInstanceId: terminalInstanceId ?? base?.terminalInstanceId,
      projectId: event.projectID,
      projectName: event.projectName || base?.projectName || "Workspace",
      projectPath: normalize(event.projectPath) ?? base?.projectPath,
      sessionTitle: event.sessionTitle || base?.sessionTitle || "Terminal",
      tool,
      aiSessionId,
      model: normalize(event.model) ?? base?.model,
      state,
      status: statusForState(state),
      isRunning: state === "responding",
      inputTokens: numberOr(base?.inputTokens, event.inputTokens),
      outputTokens: numberOr(base?.outputTokens, event.outputTokens),
      cachedInputTokens: numberOr(base?.cachedInputTokens, event.cachedInputTokens),
      totalTokens,
      baselineTotalTokens,
      startedAt: base?.startedAt ?? now,
      updatedAt: Math.max(base?.updatedAt ?? 0, now),
      activeTurnStartedAt,
      runtimeTurnStartedAt: state === "responding" ? base?.runtimeTurnStartedAt : undefined,
      hasCompletedTurn,
      wasInterrupted,
      transcriptPath: normalize(event.metadata?.transcriptPath) ?? base?.transcriptPath,
      notificationType: normalize(event.metadata?.notificationType),
      targetToolName: normalize(event.metadata?.targetToolName),
      message: normalize(event.metadata?.message),
      latestAssistantPreview: state === "idle" ? undefined : base?.latestAssistantPreview,
    };

    if (shallowEqualSession(base, next)) {
      if (didUpdateActiveStartedAt) this.emit();
      return didUpdateActiveStartedAt;
    }
    this.sessions.set(terminalId, next);
    this.emit();
    return true;
  }

  applyRuntimeSnapshot(terminalId: string, snapshot: AIRuntimeContextSnapshot) {
    const session = this.sessions.get(terminalId);
    if (!session) return false;

    const responseState = snapshot.responseState;
    const snapshotUpdatedAt = Math.max(snapshot.updatedAt || 0, session.updatedAt);
    let state = session.state;
    let wasInterrupted = session.wasInterrupted;
    let hasCompletedTurn = session.hasCompletedTurn;
    let activeTurnStartedAt = session.activeTurnStartedAt;
    let runtimeTurnStartedAt = session.runtimeTurnStartedAt;
    const snapshotIsNewer = (snapshot.updatedAt || 0) > session.updatedAt;
    const promptTurnStartedAt = session.activeTurnStartedAt ?? session.startedAt ?? session.updatedAt;

    if (responseState === "responding") {
      const snapshotStartedAfterPrompt =
        snapshot.startedAt != null && snapshot.startedAt >= promptTurnStartedAt;
      if (
        session.state === "responding" ||
        (!session.wasInterrupted && !session.hasCompletedTurn && (snapshotIsNewer || session.state === "idle")) ||
        snapshotStartedAfterPrompt
      ) {
        state = "responding";
        wasInterrupted = false;
        hasCompletedTurn = false;
        const started =
          snapshot.startedAt != null && snapshot.startedAt >= promptTurnStartedAt
            ? snapshot.startedAt
            : Math.max(snapshot.updatedAt || 0, snapshotUpdatedAt);
        activeTurnStartedAt = started;
        runtimeTurnStartedAt = started;
      }
    } else if (
      responseState === "idle" &&
      (session.state === "responding" ||
        session.state === "needsInput" ||
        snapshot.wasInterrupted ||
        snapshot.hasCompletedTurn)
    ) {
      const turnCompletedAt =
        snapshot.completedAt ?? (snapshot.wasInterrupted || snapshot.hasCompletedTurn ? snapshot.updatedAt : undefined);
      const canResolveIdle =
        snapshot.wasInterrupted || snapshot.hasCompletedTurn
          ? turnCompletedAt != null && turnCompletedAt >= promptTurnStartedAt
          : session.state === "needsInput"
            ? true
            : session.runtimeTurnStartedAt != null &&
              session.runtimeTurnStartedAt >= promptTurnStartedAt &&
              snapshot.updatedAt >= session.runtimeTurnStartedAt;
      if (canResolveIdle) {
        state = "idle";
        activeTurnStartedAt = undefined;
        runtimeTurnStartedAt = undefined;
        wasInterrupted = Boolean(snapshot.wasInterrupted ?? false);
        hasCompletedTurn = Boolean(snapshot.hasCompletedTurn ?? !wasInterrupted);
      }
    }

    const next: AISessionSnapshot = {
      ...session,
      tool: canonicalToolName(snapshot.tool) || session.tool,
      aiSessionId: normalize(snapshot.externalSessionID) ?? session.aiSessionId,
      model: normalize(snapshot.model) ?? session.model,
      state,
      status: statusForState(state),
      isRunning: state === "responding",
      inputTokens: Math.max(session.inputTokens, numberOr(0, snapshot.inputTokens)),
      outputTokens: Math.max(session.outputTokens, numberOr(0, snapshot.outputTokens)),
      cachedInputTokens: Math.max(session.cachedInputTokens, numberOr(0, snapshot.cachedInputTokens)),
      totalTokens: Math.max(session.totalTokens, numberOr(0, snapshot.totalTokens)),
      updatedAt: snapshotUpdatedAt,
      activeTurnStartedAt,
      runtimeTurnStartedAt,
      wasInterrupted,
      hasCompletedTurn,
      latestAssistantPreview: normalize(snapshot.assistantPreview) ?? session.latestAssistantPreview,
    };
    const didUpdateActiveStartedAt =
      activeTurnStartedAt != null && this.noteLatestActiveStartedAt(session.projectId, activeTurnStartedAt);

    if (shallowEqualSession(session, next)) {
      if (didUpdateActiveStartedAt) this.emit();
      return didUpdateActiveStartedAt;
    }
    this.sessions.set(terminalId, next);
    this.emit();
    return true;
  }

  reconcileBridgeSnapshot(snapshot: AIRuntimeBridgeSnapshot) {
    let didChange = false;
    const now = Date.now() / 1000;
    const liveTerminalIds = new Set(snapshot.terminals.map((terminal) => terminal.terminalId));

    for (const terminal of snapshot.terminals) {
      const existing = this.sessions.get(terminal.terminalId);
      if (!existing || existing.state !== "responding") continue;
      if (terminal.terminalInstanceId && existing.terminalInstanceId !== terminal.terminalInstanceId) {
        this.sessions.delete(terminal.terminalId);
        didChange = true;
        continue;
      }
      if (now - existing.updatedAt > RUNNING_STALE_MS / 1000) {
        this.sessions.set(terminal.terminalId, markInterrupted(existing, now));
        didChange = true;
      }
    }

    for (const [terminalId, session] of this.sessions) {
      if (!liveTerminalIds.has(terminalId) && session.state !== "idle") {
        this.sessions.set(terminalId, markInterrupted(session, now));
        didChange = true;
      }
    }

    if (didChange) this.emit();
    return didChange;
  }

  private emit() {
    for (const listener of this.listeners) listener();
  }

  private isCompletionDismissed(projectId: string, completedAt: number) {
    const dismissedAt = this.dismissedCompletedAt.get(projectId);
    return dismissedAt != null && completedAt <= dismissedAt;
  }

  private noteLatestActiveStartedAt(projectId: string, startedAt: number) {
    const previous = this.latestActiveStartedAtByProject.get(projectId) ?? 0;
    if (startedAt <= previous) return false;
    this.latestActiveStartedAtByProject.set(projectId, startedAt);
    return true;
  }
}

function nextState(kind: AIHookEventPayload["kind"], metadata?: AIHookEventMetadata | null): AIState {
  if (kind === "promptSubmitted") return "responding";
  if (kind === "sessionStarted") return "idle";
  if (kind === "needsInput") return "needsInput";
  if (kind === "turnCompleted" || kind === "sessionEnded") return "idle";
  return metadata?.notificationType ? "needsInput" : "idle";
}

function markInterrupted(session: AISessionSnapshot, updatedAt: number): AISessionSnapshot {
  return {
    ...session,
    state: "idle",
    status: "idle",
    isRunning: false,
    wasInterrupted: true,
    hasCompletedTurn: false,
    activeTurnStartedAt: undefined,
    runtimeTurnStartedAt: undefined,
    updatedAt,
  };
}

function isToolActivityWithoutLoading(event: AIHookEventPayload, previous?: AISessionSnapshot) {
  if (event.kind !== "promptSubmitted" || normalize(event.metadata?.source) !== "tool-use") {
    return false;
  }
  if (!previous) return true;
  return previous.hasCompletedTurn || previous.wasInterrupted;
}

function shallowEqualSession(left: AISessionSnapshot | undefined, right: AISessionSnapshot) {
  if (!left) return false;
  return JSON.stringify(left) === JSON.stringify(right);
}
