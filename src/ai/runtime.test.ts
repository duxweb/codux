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

  it("subtracts cached input baselines from live project totals", () => {
    const sessionStore = new AISessionStore();

    sessionStore.applyHook(
      hook({
        kind: "promptSubmitted",
        totalTokens: 1_000,
        cachedInputTokens: 10_000,
        updatedAt: 1000,
      }),
    );
    sessionStore.applyHook(
      hook({
        kind: "turnCompleted",
        totalTokens: 1_200,
        cachedInputTokens: 12_500,
        updatedAt: 1010,
        metadata: { hasCompletedTurn: true },
      }),
    );

    expect(sessionStore.projectTotals("project-1")).toMatchObject({
      totalTokens: 200,
      cachedInputTokens: 2_500,
    });
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

  it("lets runtime polling start a new turn after an undismissed completion", () => {
    const sessionStore = new AISessionStore();

    sessionStore.applyHook(hook({ kind: "promptSubmitted", updatedAt: 1000 }));
    sessionStore.applyHook(
      hook({
        kind: "turnCompleted",
        totalTokens: 150,
        updatedAt: 1010,
        metadata: { hasCompletedTurn: true },
      }),
    );

    expect(sessionStore.completedPhase("project-1").kind).toBe("completed");
    expect(
      sessionStore.applyRuntimeSnapshot("term-1", {
        tool: "codex",
        externalSessionID: "ai-1",
        model: "gpt-5.5",
        inputTokens: 120,
        outputTokens: 80,
        cachedInputTokens: 0,
        totalTokens: 200,
        updatedAt: 1020,
        startedAt: 1020,
        responseState: "responding",
        wasInterrupted: false,
        hasCompletedTurn: false,
        sessionOrigin: "fresh",
        source: "probe",
      }),
    ).toBe(true);

    expect(sessionStore.projectPhase("project-1")).toEqual({ kind: "running", tool: "codex" });
    expect(sessionStore.completedPhase("project-1")).toEqual({ kind: "idle" });
    expect(sessionStore.snapshots("project-1")[0]).toMatchObject({
      state: "responding",
      hasCompletedTurn: false,
    });
  });

  it("lets runtime polling resume running after a permission prompt", () => {
    const sessionStore = new AISessionStore();

    sessionStore.applyHook(hook({ kind: "promptSubmitted", updatedAt: 1000 }));
    sessionStore.applyHook(
      hook({
        kind: "needsInput",
        updatedAt: 1005,
        metadata: { notificationType: "permission" },
      }),
    );

    expect(sessionStore.projectPhase("project-1")).toEqual({ kind: "needsInput", tool: "codex" });
    expect(
      sessionStore.applyRuntimeSnapshot("term-1", {
        tool: "codex",
        externalSessionID: "ai-1",
        model: "gpt-5.5",
        inputTokens: 120,
        outputTokens: 55,
        cachedInputTokens: 0,
        totalTokens: 175,
        updatedAt: 1015,
        responseState: "responding",
        wasInterrupted: false,
        hasCompletedTurn: false,
        sessionOrigin: "fresh",
        source: "probe",
      }),
    ).toBe(true);

    expect(sessionStore.projectPhase("project-1")).toEqual({ kind: "running", tool: "codex" });
    expect(sessionStore.snapshots("project-1")[0]).toMatchObject({
      state: "responding",
      hasCompletedTurn: false,
    });
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

  it("lets opencode runtime snapshots use the Rust probe fallback", async () => {
    const driver = new OpenCodeToolDriver(async (request) => ({
      tool: "opencode",
      externalSessionID: request.externalSessionId,
      model: "minimax-m2.5-free",
      inputTokens: 12,
      outputTokens: 8,
      cachedInputTokens: 3,
      totalTokens: 20,
      updatedAt: 1020,
      startedAt: 1000,
      completedAt: 1020,
      responseState: "idle",
      wasInterrupted: false,
      hasCompletedTurn: true,
      sessionOrigin: "restored",
      source: "probe",
    }));

    await expect(
      driver.runtimeSnapshot({
        terminalId: "term-1",
        terminalInstanceId: "instance-1",
        projectId: "project-1",
        projectName: "Project",
        projectPath: "/project",
        sessionTitle: "OpenCode",
        tool: "opencode",
        aiSessionId: "ses_opencode",
        model: "minimax-m2.5-free",
        state: "responding",
        status: "running",
        isRunning: true,
        inputTokens: 0,
        outputTokens: 0,
        cachedInputTokens: 0,
        totalTokens: 0,
        baselineTotalTokens: 0,
        baselineCachedInputTokens: 0,
        updatedAt: 1000,
        hasCompletedTurn: false,
        wasInterrupted: false,
      }),
    ).resolves.toMatchObject({
      tool: "opencode",
      responseState: "idle",
      hasCompletedTurn: true,
      totalTokens: 20,
    });
  });
});
