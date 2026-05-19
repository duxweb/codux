import { invoke } from "@tauri-apps/api/core";
import type {
  AIHookEventPayload,
  AIRuntimeContextSnapshot,
  AIRuntimeProbeRequest,
  AISessionSnapshot,
} from "./types";
import { canonicalToolName, normalize, numberOr } from "./utils";

export type RuntimeProbe = (
  request: AIRuntimeProbeRequest,
) => Promise<AIRuntimeContextSnapshot | null | undefined>;

export interface AIToolDriver {
  id: string;
  aliases: Set<string>;
  isRealtimeTool: boolean;
  matches(tool?: string | null): boolean;
  resolveHookEvent(
    event: AIHookEventPayload,
    currentSession?: AISessionSnapshot,
  ): Promise<AIHookEventPayload>;
  runtimeSnapshot(session: AISessionSnapshot): Promise<AIRuntimeContextSnapshot | null | undefined>;
}

const defaultProbe: RuntimeProbe = async (request) => {
  if (!window.__TAURI_INTERNALS__) return undefined;
  return invoke<AIRuntimeContextSnapshot | null>("ai_runtime_probe", { request });
};

abstract class BaseToolDriver implements AIToolDriver {
  abstract id: string;
  abstract aliases: Set<string>;
  isRealtimeTool = true;

  constructor(protected readonly probe: RuntimeProbe = defaultProbe) {}

  matches(tool?: string | null) {
    const normalized = canonicalToolName(tool);
    return Boolean(normalized && this.aliases.has(normalized));
  }

  async resolveHookEvent(event: AIHookEventPayload, currentSession?: AISessionSnapshot) {
    void currentSession;
    return event;
  }

  async runtimeSnapshot(session: AISessionSnapshot) {
    return this.probe(this.probeRequest(session));
  }

  protected matchingFallbackSession(event: AIHookEventPayload, currentSession?: AISessionSnapshot) {
    if (!currentSession) return undefined;
    const incomingTool = canonicalToolName(event.tool);
    if (canonicalToolName(currentSession.tool) !== incomingTool) return undefined;

    const incomingSessionId = normalize(event.aiSessionID);
    const currentSessionId = normalize(currentSession.aiSessionId);
    if (incomingSessionId && currentSessionId !== incomingSessionId) return undefined;
    if (event.kind === "sessionStarted" && !incomingSessionId) return undefined;
    return currentSession;
  }

  protected probeRequest(session: AISessionSnapshot): AIRuntimeProbeRequest {
    return {
      terminalId: session.terminalId,
      terminalInstanceId: session.terminalInstanceId,
      projectId: session.projectId,
      projectPath: session.projectPath,
      tool: this.id,
      externalSessionId: session.aiSessionId,
      transcriptPath: session.transcriptPath,
      startedAt: session.startedAt,
      updatedAt: session.updatedAt,
    };
  }
}

export class CodexToolDriver extends BaseToolDriver {
  id = "codex";
  aliases = new Set(["codex"]);

  async resolveHookEvent(event: AIHookEventPayload, currentSession?: AISessionSnapshot) {
    if (!this.matches(event.tool)) return event;
    const fallbackSession = this.matchingFallbackSession(event, currentSession);
    const resolved = withFallback(event, fallbackSession);
    if (event.kind !== "turnCompleted" || !normalize(event.metadata?.transcriptPath)) {
      return resolved;
    }

    const snapshot = await this.probe({
      terminalId: event.terminalID,
      terminalInstanceId: event.terminalInstanceID,
      projectId: event.projectID,
      projectPath: event.projectPath,
      tool: this.id,
      externalSessionId: normalize(event.aiSessionID) ?? fallbackSession?.aiSessionId,
      transcriptPath: event.metadata?.transcriptPath,
      startedAt: fallbackSession?.startedAt ?? event.updatedAt,
      updatedAt: event.updatedAt,
    }).catch(() => undefined);
    if (!snapshot) return resolved;

    return mergeSnapshotIntoHook(resolved, snapshot, fallbackSession);
  }
}

export class ClaudeToolDriver extends BaseToolDriver {
  id = "claude";
  aliases = new Set(["claude", "claude-code"]);

  async resolveHookEvent(event: AIHookEventPayload, currentSession?: AISessionSnapshot) {
    if (!this.matches(event.tool)) return event;
    const fallbackSession = this.matchingFallbackSession(event, currentSession);
    const resolved = withFallback(event, fallbackSession);
    if (event.kind !== "turnCompleted") return resolved;

    const externalSessionId = normalize(event.aiSessionID) ?? fallbackSession?.aiSessionId;
    if (!normalize(event.projectPath) || !externalSessionId) return resolved;
    const snapshot = await this.probe({
      terminalId: event.terminalID,
      terminalInstanceId: event.terminalInstanceID,
      projectId: event.projectID,
      projectPath: event.projectPath,
      tool: this.id,
      externalSessionId,
      startedAt: fallbackSession?.startedAt ?? event.updatedAt,
      updatedAt: event.updatedAt,
    }).catch(() => undefined);
    if (!snapshot) return resolved;
    return mergeSnapshotIntoHook({ ...resolved, aiSessionID: normalize(resolved.aiSessionID) ?? externalSessionId }, snapshot, fallbackSession);
  }
}

export class GeminiToolDriver extends BaseToolDriver {
  id = "gemini";
  aliases = new Set(["gemini"]);

  async resolveHookEvent(event: AIHookEventPayload, currentSession?: AISessionSnapshot) {
    if (!this.matches(event.tool)) return event;
    const fallbackSession = this.matchingFallbackSession(event, currentSession);
    const resolved = withFallback(event, fallbackSession);
    if (!normalize(event.projectPath)) return resolved;

    const snapshot = await this.probe({
      terminalId: event.terminalID,
      terminalInstanceId: event.terminalInstanceID,
      projectId: event.projectID,
      projectPath: event.projectPath,
      tool: this.id,
      externalSessionId: normalize(event.aiSessionID) ?? fallbackSession?.aiSessionId,
      startedAt: fallbackSession?.startedAt ?? event.updatedAt,
      updatedAt: event.updatedAt,
    }).catch(() => undefined);
    if (!snapshot) return resolved;
    return mergeSnapshotIntoHook(resolved, snapshot, fallbackSession);
  }
}

export class OpenCodeToolDriver extends BaseToolDriver {
  id = "opencode";
  aliases = new Set(["opencode"]);

  runtimeSnapshot() {
    return Promise.resolve(undefined);
  }
}

export class AIToolDriverFactory {
  private drivers: AIToolDriver[];

  constructor(drivers?: AIToolDriver[]) {
    this.drivers = drivers ?? [
      new ClaudeToolDriver(),
      new CodexToolDriver(),
      new OpenCodeToolDriver(),
      new GeminiToolDriver(),
    ];
  }

  driver(tool?: string | null) {
    return this.drivers.find((driver) => driver.matches(tool));
  }

  canonicalToolName(tool: string) {
    return this.driver(tool)?.id ?? canonicalToolName(tool);
  }

  isRealtimeTool(tool: string) {
    return this.driver(tool)?.isRealtimeTool ?? false;
  }

  resolveHookEvent(event: AIHookEventPayload, currentSession?: AISessionSnapshot) {
    return this.driver(event.tool)?.resolveHookEvent(event, currentSession) ?? Promise.resolve(event);
  }
}

export const aiToolDriverFactory = new AIToolDriverFactory();

function withFallback(event: AIHookEventPayload, fallbackSession?: AISessionSnapshot): AIHookEventPayload {
  if (!fallbackSession) return event;
  return {
    ...event,
    tool: canonicalToolName(event.tool),
    aiSessionID: normalize(event.aiSessionID) ?? fallbackSession.aiSessionId,
    model: normalize(event.model) ?? fallbackSession.model,
    totalTokens: event.totalTokens ?? fallbackSession.totalTokens,
  };
}

function mergeSnapshotIntoHook(
  event: AIHookEventPayload,
  snapshot: AIRuntimeContextSnapshot,
  fallbackSession?: AISessionSnapshot,
): AIHookEventPayload {
  const wasInterrupted = snapshot.wasInterrupted ?? event.metadata?.wasInterrupted ?? false;
  const hasCompletedTurn =
    snapshot.hasCompletedTurn ?? event.metadata?.hasCompletedTurn ?? !wasInterrupted;
  const nextKind =
    snapshot.responseState === "responding"
      ? "promptSubmitted"
      : event.kind;
  return {
    ...event,
    kind: nextKind,
    aiSessionID: normalize(event.aiSessionID) ?? normalize(snapshot.externalSessionID) ?? fallbackSession?.aiSessionId,
    model: normalize(event.model) ?? normalize(snapshot.model) ?? fallbackSession?.model,
    inputTokens: numberOr(event.inputTokens ?? fallbackSession?.inputTokens, snapshot.inputTokens),
    outputTokens: numberOr(event.outputTokens ?? fallbackSession?.outputTokens, snapshot.outputTokens),
    cachedInputTokens: numberOr(event.cachedInputTokens ?? fallbackSession?.cachedInputTokens, snapshot.cachedInputTokens),
    totalTokens: Math.max(
      event.totalTokens ?? 0,
      fallbackSession?.totalTokens ?? 0,
      snapshot.totalTokens ?? 0,
    ),
    updatedAt: Math.max(event.updatedAt, snapshot.completedAt ?? 0, snapshot.updatedAt ?? 0),
    metadata: {
      ...event.metadata,
      wasInterrupted,
      hasCompletedTurn,
    },
  };
}
