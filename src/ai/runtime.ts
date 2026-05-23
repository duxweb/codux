import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useRuntimeStore } from "../runtimeStore";
import { AIRuntimeIngressService } from "./ingressService";
import { AIRuntimePollingService } from "./pollingService";
import { AISessionStore, type SessionStoreListener } from "./sessionStore";
import { aiToolDriverFactory, type AIToolDriverFactory } from "./toolDrivers";
import type { AIProjectPhase, AIProjectTotals, AIRuntimeStateSnapshot, AIHookEventPayload } from "./types";

export type {
  AIHookEventMetadata,
  AIHookEventPayload,
  AIHookKind,
  AIProjectPhase,
  AIRuntimeBridgeSnapshot,
  AIRuntimeContextSnapshot,
  AIRuntimeEvent,
  AIRuntimeProbeRequest,
  AIRuntimeStateSnapshot,
  AIRuntimeTerminalState,
  AISessionSnapshot,
  AIState,
} from "./types";

export class AIRuntimeStore {
  private readonly sessionStore: AISessionStore;
  private readonly ingressService: AIRuntimeIngressService;
  private readonly pollingService: AIRuntimePollingService;
  private runtimeState?: AIRuntimeStateSnapshot;
  private runtimeStateSignature = "";
  private sortedRuntimeSessions?: AIRuntimeStateSnapshot["sessions"];
  private listeners = new Set<SessionStoreListener>();
  private unlistenState?: UnlistenFn;
  private startPromise?: Promise<void>;

  constructor(toolDriverFactory: AIToolDriverFactory = aiToolDriverFactory) {
    this.sessionStore = new AISessionStore(toolDriverFactory);
    this.pollingService = new AIRuntimePollingService(this.sessionStore, toolDriverFactory);
    this.ingressService = new AIRuntimeIngressService(this.sessionStore, toolDriverFactory, (terminalId, reason) => {
      this.pollingService.noteHookApplied(terminalId, reason);
      this.pollingService.sync(`hook:${reason}`);
    });
    this.sessionStore.subscribe(() => this.emit());
  }

  subscribe(listener: SessionStoreListener) {
    this.listeners.add(listener);
    void this.start();
    return () => {
      this.listeners.delete(listener);
    };
  }

  async start() {
    if (this.startPromise) return this.startPromise;
    if (window.__TAURI_INTERNALS__) {
      this.startPromise = this.startRustStateListener();
      return this.startPromise;
    }
    this.startPromise = this.ingressService.start().then(() => {
      this.pollingService.start();
    });
    return this.startPromise;
  }

  snapshots(projectId?: string) {
    if (this.runtimeState) {
      const sessions = this.sortedRuntimeSessions ?? this.runtimeState.sessions;
      return projectId ? sessions.filter((session) => session.projectId === projectId) : sessions;
    }
    return this.sessionStore.snapshots(projectId);
  }

  projectPhase(projectId: string) {
    if (this.runtimeState) return this.projectState(projectId)?.projectPhase ?? idlePhase();
    return this.sessionStore.projectPhase(projectId);
  }

  completedPhase(projectId: string) {
    if (this.runtimeState) return this.projectState(projectId)?.completedPhase ?? idlePhase();
    return this.sessionStore.completedPhase(projectId);
  }

  dismissCompletion(projectId: string) {
    if (this.runtimeState && window.__TAURI_INTERNALS__) {
      const project = this.projectState(projectId);
      if (project?.completedPhase.kind === "completed") {
        this.setRuntimeState({
          ...this.runtimeState,
          projects: this.runtimeState.projects.map((item) =>
            item.projectId === projectId ? { ...item, completedPhase: idlePhase() } : item,
          ),
        });
        this.emit();
      }
      void invoke("ai_runtime_dismiss_completion", { projectId }).catch((error) => {
        console.error("failed to dismiss ai completion", error);
      });
      return project?.completedPhase.kind === "completed";
    }
    return this.sessionStore.dismissCompletion(projectId);
  }

  projectTotals(projectId?: string) {
    if (this.runtimeState) {
      if (!projectId) return this.runtimeState.globalTotals;
      return this.projectState(projectId)?.totals ?? emptyTotals();
    }
    return this.sessionStore.projectTotals(projectId);
  }

  applyHookForTesting(event: AIHookEventPayload) {
    return this.ingressService.applyHookForTesting(event);
  }

  private async startRustStateListener() {
    if (this.unlistenState) return;
    const cachedSnapshot = useRuntimeStore.getState().aiRuntimeSnapshot;
    if (cachedSnapshot && this.setRuntimeState(cachedSnapshot)) {
      this.emit();
    }
    this.unlistenState = await listen<AIRuntimeStateSnapshot>("ai-runtime:state", (event) => {
      if (this.setRuntimeState(event.payload)) {
        this.emit();
      }
    });
  }

  private setRuntimeState(snapshot: AIRuntimeStateSnapshot | undefined) {
    const signature = runtimeStateSignature(snapshot);
    if (signature === this.runtimeStateSignature) {
      return false;
    }
    this.runtimeStateSignature = signature;
    this.runtimeState = snapshot;
    this.sortedRuntimeSessions = snapshot
      ? [...snapshot.sessions].sort((left, right) => right.updatedAt - left.updatedAt)
      : undefined;
    useRuntimeStore.getState().setAIRuntimeSnapshot(snapshot ?? null);
    return true;
  }

  private projectState(projectId: string) {
    return this.runtimeState?.projects.find((project) => project.projectId === projectId);
  }

  private emit() {
    for (const listener of this.listeners) listener();
  }
}

export const aiRuntime = new AIRuntimeStore();

export function useAIRuntimeSnapshot(projectId?: string) {
  const [version, setVersion] = useState(0);
  useEffect(() => aiRuntime.subscribe(() => setVersion((current) => current + 1)), []);
  return {
    version,
    sessions: aiRuntime.snapshots(projectId),
    projectTotals: aiRuntime.projectTotals(projectId),
    globalTotals: aiRuntime.projectTotals(),
    projectPhase: projectId ? aiRuntime.projectPhase(projectId) : ({ kind: "idle" } as const),
    completedPhase: projectId ? aiRuntime.completedPhase(projectId) : ({ kind: "idle" } as const),
  };
}

function idlePhase(): AIProjectPhase {
  return { kind: "idle" };
}

function emptyTotals(): AIProjectTotals {
  return { totalTokens: 0, cachedInputTokens: 0, running: 0, needsInput: 0, completed: 0 };
}

function runtimeStateSignature(snapshot: AIRuntimeStateSnapshot | undefined) {
  if (!snapshot) return "null";
  return [
    snapshot.runningCount,
    snapshot.needsInputCount,
    snapshot.completionCount,
    totalsSignature(snapshot.globalTotals),
    snapshot.latestCompletion
      ? [
          snapshot.latestCompletion.id,
          snapshot.latestCompletion.projectId,
          snapshot.latestCompletion.tool,
          snapshot.latestCompletion.wasInterrupted ? 1 : 0,
          timestampSignature(snapshot.latestCompletion.updatedAt),
        ].join(":")
      : "",
    [...snapshot.projects]
      .sort((left, right) => left.projectId.localeCompare(right.projectId))
      .map(projectSignature)
      .join("|"),
    [...snapshot.sessions]
      .sort((left, right) => left.terminalId.localeCompare(right.terminalId))
      .map(sessionSignature)
      .join("|"),
  ].join("\n");
}

function projectSignature(project: AIRuntimeStateSnapshot["projects"][number]) {
  return [
    project.projectId,
    phaseSignature(project.projectPhase),
    phaseSignature(project.completedPhase),
    totalsSignature(project.totals),
  ].join(":");
}

function sessionSignature(session: AIRuntimeStateSnapshot["sessions"][number]) {
  return [
    session.terminalId,
    session.terminalInstanceId ?? "",
    session.projectId,
    session.projectName,
    session.projectPath ?? "",
    session.sessionTitle,
    session.tool,
    session.aiSessionId ?? "",
    session.model ?? "",
    session.state,
    session.status,
    session.isRunning ? 1 : 0,
    session.inputTokens,
    session.outputTokens,
    session.cachedInputTokens,
    session.totalTokens,
    session.baselineTotalTokens,
    session.baselineCachedInputTokens,
    timestampSignature(session.startedAt),
    timestampSignature(session.activeTurnStartedAt),
    timestampSignature(session.runtimeTurnStartedAt),
    session.state === "responding" ? "" : timestampSignature(session.updatedAt),
    session.hasCompletedTurn ? 1 : 0,
    session.wasInterrupted ? 1 : 0,
    session.transcriptPath ?? "",
    session.notificationType ?? "",
    session.targetToolName ?? "",
    session.message ?? "",
    session.latestAssistantPreview ?? "",
  ].join("\u001f");
}

function phaseSignature(phase: AIProjectPhase) {
  if (phase.kind === "idle") return "idle";
  if (phase.kind === "completed") {
    return ["completed", phase.tool, phase.wasInterrupted ? 1 : 0, timestampSignature(phase.updatedAt)].join(":");
  }
  return [phase.kind, phase.tool].join(":");
}

function totalsSignature(totals: AIProjectTotals) {
  return [totals.totalTokens, totals.cachedInputTokens, totals.running, totals.needsInput, totals.completed].join(":");
}

function timestampSignature(value: number | undefined | null) {
  if (value == null || !Number.isFinite(value)) return "";
  return Math.floor(value * 1000).toString();
}
