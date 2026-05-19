import { invoke } from "@tauri-apps/api/core";
import type { AIRuntimeBridgeSnapshot, AIRuntimeContextSnapshot, AISessionSnapshot } from "./types";
import { AISessionStore } from "./sessionStore";
import { aiToolDriverFactory, type AIToolDriverFactory } from "./toolDrivers";
import { canonicalToolName, normalize } from "./utils";

const POLL_INTERVAL_MS = 6000;
const RUNNING_STATE_RENEWAL_MS = 30_000;
const CODEX_INTERVAL_POLL_MINIMUM_MS = 60_000;

export class AIRuntimePollingService {
  private pollTimer?: number;
  private pollInFlight = false;
  private pendingReason?: string;
  private lastHookAppliedAtByTerminalId = new Map<string, number>();

  constructor(
    private readonly sessionStore: AISessionStore,
    private readonly toolDriverFactory: AIToolDriverFactory = aiToolDriverFactory,
    private readonly intervalMs = POLL_INTERVAL_MS,
  ) {}

  start() {
    if (!window.__TAURI_INTERNALS__) return;
    this.sync("start");
  }

  noteHookApplied(terminalId: string, reason: string) {
    void reason;
    this.lastHookAppliedAtByTerminalId.set(terminalId, Date.now());
    this.pruneHookMarkers();
  }

  sync(reason: string) {
    const trackedSessions = this.sessionStore.runtimeTrackedSessions();
    if (!trackedSessions.length) {
      this.stopPolling();
      return;
    }

    if (!this.pollTimer) {
      this.pollTimer = window.setInterval(() => {
        void this.schedulePoll("interval");
      }, this.intervalMs);
    }

    void this.schedulePoll(reason);
  }

  private stopPolling() {
    if (this.pollTimer) {
      window.clearInterval(this.pollTimer);
      this.pollTimer = undefined;
    }
    this.pendingReason = undefined;
    this.pollInFlight = false;
    this.lastHookAppliedAtByTerminalId.clear();
  }

  private async schedulePoll(reason: string) {
    if (!window.__TAURI_INTERNALS__) return;
    if (this.pollInFlight) {
      this.pendingReason = reason;
      return;
    }

    const now = Date.now();
    const trackedSessions = this.sessionStore
      .runtimeTrackedSessions()
      .filter((session) => this.shouldPoll(session, reason, now));
    if (!trackedSessions.length) {
      if (!this.sessionStore.runtimeTrackedSessions().length) this.stopPolling();
      return;
    }

    this.pollInFlight = true;
    const startedAt = Date.now();
    try {
      const bridgeSnapshot = await invoke<AIRuntimeBridgeSnapshot>("ai_runtime_snapshot");
      this.sessionStore.reconcileBridgeSnapshot(bridgeSnapshot);

      const updates: Array<[string, AIRuntimeContextSnapshot]> = [];
      for (const session of trackedSessions) {
        const driver = this.toolDriverFactory.driver(session.tool);
        const snapshot = await driver?.runtimeSnapshot(session);
        if (snapshot) updates.push([session.terminalId, snapshot]);
      }
      this.finishPoll(updates, startedAt);
    } catch (error) {
      console.error("failed to poll ai runtime", error);
    } finally {
      this.pollInFlight = false;
      const pendingReason = this.pendingReason;
      this.pendingReason = undefined;
      if (pendingReason) void this.schedulePoll(pendingReason);
    }
  }

  private finishPoll(updates: Array<[string, AIRuntimeContextSnapshot]>, pollStartedAt: number) {
    const now = Date.now();
    for (const [terminalId, snapshot] of updates) {
      if (this.shouldSkipSnapshot(terminalId, pollStartedAt)) continue;
      const observedSnapshot = this.shouldRenewRunningState(terminalId, snapshot, now)
        ? { ...snapshot, updatedAt: Math.max(snapshot.updatedAt, now / 1000) }
        : snapshot;
      this.sessionStore.applyRuntimeSnapshot(terminalId, observedSnapshot);
    }
    this.pruneHookMarkers();
    if (!this.sessionStore.runtimeTrackedSessions().length) this.stopPolling();
  }

  private shouldPoll(session: AISessionSnapshot, reason: string, now: number) {
    if (
      canonicalToolName(session.tool) === "codex" &&
      normalize(session.transcriptPath) &&
      reason === "interval" &&
      now - session.updatedAt * 1000 < CODEX_INTERVAL_POLL_MINIMUM_MS
    ) {
      return false;
    }
    if (session.state === "responding" || session.state === "needsInput") return true;
    return !session.hasCompletedTurn;
  }

  private shouldSkipSnapshot(terminalId: string, pollStartedAt: number) {
    const lastHookAppliedAt = this.lastHookAppliedAtByTerminalId.get(terminalId);
    return Boolean(lastHookAppliedAt && lastHookAppliedAt > pollStartedAt);
  }

  private shouldRenewRunningState(terminalId: string, snapshot: AIRuntimeContextSnapshot, now: number) {
    if (snapshot.responseState !== "responding") return false;
    const session = this.sessionStore.sessionForTerminal(terminalId);
    if (!session) return false;
    return now - session.updatedAt * 1000 >= RUNNING_STATE_RENEWAL_MS;
  }

  private pruneHookMarkers() {
    const now = Date.now();
    const ttl = Math.max(this.intervalMs * 2, 10_000);
    for (const [terminalId, seenAt] of this.lastHookAppliedAtByTerminalId) {
      if (now - seenAt > ttl) this.lastHookAppliedAtByTerminalId.delete(terminalId);
    }
  }
}
