import { afterEach, describe, expect, it, vi } from "vitest";
import { AIRuntimeStore, type AIHookEventPayload } from "./runtime";
import { AIRuntimeIngressService } from "./ingressService";
import { AISessionStore } from "./sessionStore";
import { AIToolDriverFactory, CodexToolDriver, OpenCodeToolDriver } from "./toolDrivers";

function hook(patch: Partial<AIHookEventPayload>): AIHookEventPayload {
  return {
    kind: "promptSubmitted",
    terminalID: "term-1",
    terminalInstanceID: "instance-1",
    projectID: "project-1",
    projectName: "Project",
    projectPath: "/project",
    sessionTitle: "分屏 1",
    tool: "codex",
    aiSessionID: "ai-1",
    model: "gpt-5.5",
    totalTokens: 100,
    updatedAt: 1000,
    ...patch,
  };
}

describe("ai runtime store", () => {
  afterEach(() => {
    vi.useRealTimers();
  });

  it("tracks hook-driven running and completed project phases separately", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-17T05:00:00Z"));
    const store = new AIRuntimeStore();
    const now = Date.now() / 1000;

    store.applyHookForTesting(hook({ kind: "promptSubmitted", updatedAt: now }));

    expect(store.projectPhase("project-1")).toEqual({ kind: "running", tool: "codex" });
    expect(store.snapshots("project-1")[0]).toMatchObject({
      terminalId: "term-1",
      state: "responding",
      tool: "codex",
      model: "gpt-5.5",
    });

    store.applyHookForTesting(
      hook({
        kind: "turnCompleted",
        totalTokens: 150,
        updatedAt: now + 5,
        metadata: { hasCompletedTurn: true },
      }),
    );

    expect(store.projectPhase("project-1")).toEqual({ kind: "idle" });
    expect(store.completedPhase("project-1")).toEqual({
      kind: "completed",
      tool: "codex",
      wasInterrupted: false,
      updatedAt: now + 5,
    });
    expect(store.projectTotals("project-1").totalTokens).toBe(50);
  });

  it("keeps completion visible until explicit dismissal instead of a fixed timer", () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date("2026-05-17T05:00:00Z"));
    const store = new AIRuntimeStore();
    const now = Date.now() / 1000;

    store.applyHookForTesting(hook({ kind: "promptSubmitted", updatedAt: now }));
    store.applyHookForTesting(
      hook({
        kind: "turnCompleted",
        totalTokens: 150,
        updatedAt: now + 5,
        metadata: { hasCompletedTurn: true },
      }),
    );

    vi.advanceTimersByTime(10 * 60 * 1000);
    expect(store.completedPhase("project-1")).toEqual({
      kind: "completed",
      tool: "codex",
      wasInterrupted: false,
      updatedAt: now + 5,
    });

    expect(store.dismissCompletion("project-1")).toBe(true);
    expect(store.completedPhase("project-1")).toEqual({ kind: "idle" });
  });

  it("does not revive a stale completion after newer active work ends without completion", () => {
    const store = new AIRuntimeStore();

    store.applyHookForTesting(hook({ kind: "promptSubmitted", updatedAt: 1000 }));
    store.applyHookForTesting(
      hook({
        kind: "turnCompleted",
        totalTokens: 150,
        updatedAt: 1005,
        metadata: { hasCompletedTurn: true },
      }),
    );
    expect(store.completedPhase("project-1").kind).toBe("completed");

    store.applyHookForTesting(
      hook({
        kind: "promptSubmitted",
        terminalID: "term-2",
        terminalInstanceID: "instance-2",
        aiSessionID: "ai-2",
        updatedAt: 1100,
      }),
    );
    expect(store.projectPhase("project-1")).toEqual({ kind: "running", tool: "codex" });
    expect(store.completedPhase("project-1")).toEqual({ kind: "idle" });

    store.applyHookForTesting(
      hook({
        kind: "sessionEnded",
        terminalID: "term-2",
        terminalInstanceID: "instance-2",
        aiSessionID: "ai-2",
        updatedAt: 1110,
      }),
    );

    expect(store.projectPhase("project-1")).toEqual({ kind: "idle" });
    expect(store.completedPhase("project-1")).toEqual({ kind: "idle" });
  });

  it("does not expose a terminal-input fallback for AI state", () => {
    const store = new AIRuntimeStore();

    expect("noteTerminalInput" in store).toBe(false);
    expect(store.projectPhase("project-2")).toEqual({ kind: "idle" });
  });

  it("lets the tool driver resolve hook payloads from runtime probes", async () => {
    vi.useFakeTimers();
    vi.setSystemTime(new Date(1_011_000));
    const driver = new CodexToolDriver(async () => ({
      tool: "codex",
      externalSessionID: "ai-1",
      model: "gpt-5.5-probed",
      inputTokens: 120,
      outputTokens: 55,
      cachedInputTokens: 10,
      totalTokens: 175,
      updatedAt: 1010,
      completedAt: 1010,
      responseState: "idle",
      wasInterrupted: false,
      hasCompletedTurn: true,
      sessionOrigin: "fresh",
      source: "probe",
    }));
    const factory = new AIToolDriverFactory([driver]);
    const sessionStore = new AISessionStore(factory);
    const ingress = new AIRuntimeIngressService(sessionStore, factory);

    sessionStore.applyHook(hook({ kind: "promptSubmitted", totalTokens: 100 }));
    await ingress.processRuntimeEvent({
      kind: "hook",
      payload: hook({
        kind: "turnCompleted",
        model: null,
        totalTokens: null,
        metadata: { transcriptPath: "/tmp/codex-rollout.jsonl" },
      }),
    });

    expect(sessionStore.snapshots("project-1")[0]).toMatchObject({
      model: "gpt-5.5",
      totalTokens: 175,
      cachedInputTokens: 10,
      hasCompletedTurn: true,
      state: "idle",
    });
  });

  it("registers opencode as a realtime hook-driven tool", () => {
    const driver = new OpenCodeToolDriver();
    const factory = new AIToolDriverFactory([driver]);

    expect(factory.canonicalToolName("opencode")).toBe("opencode");
    expect(factory.isRealtimeTool("opencode")).toBe(true);
  });
});
