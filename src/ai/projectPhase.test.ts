import { describe, expect, it } from "vitest";
import { aggregateProjectPhase, phaseToAIState, resolveDisplayedProjectPhase } from "./projectPhase";
import type { AIProjectPhase } from "./types";

function phaseMap(phases: Record<string, AIProjectPhase>) {
  return (projectId: string) => phases[projectId] ?? ({ kind: "idle" } as const);
}

describe("project ai phase", () => {
  it("uses any worktree phase for the parent project loading state", () => {
    const phase = aggregateProjectPhase(
      "project-a",
      ["worktree-a", "worktree-b"],
      phaseMap({
        "project-a": { kind: "idle" },
        "worktree-a": { kind: "running", tool: "codex" },
        "worktree-b": { kind: "idle" },
      }),
    );

    expect(phase).toEqual({ kind: "running", tool: "codex" });
    expect(phaseToAIState(phase)).toBe("running");
  });

  it("keeps needs-input above running and completed phases", () => {
    const phase = aggregateProjectPhase(
      "project-a",
      ["worktree-a"],
      phaseMap({
        "project-a": { kind: "running", tool: "codex" },
        "worktree-a": { kind: "needsInput", tool: "claude" },
      }),
    );

    expect(phase).toEqual({ kind: "needsInput", tool: "claude" });
    expect(phaseToAIState(phase)).toBe("review");
  });

  it("prefers the latest completed phase only when all ids completed", () => {
    const phase = aggregateProjectPhase(
      "project-a",
      ["worktree-a"],
      phaseMap({
        "project-a": {
          kind: "completed",
          tool: "codex",
          wasInterrupted: false,
          updatedAt: 10,
        },
        "worktree-a": {
          kind: "completed",
          tool: "gemini",
          wasInterrupted: true,
          updatedAt: 20,
        },
      }),
    );

    expect(phase).toEqual({
      kind: "completed",
      tool: "gemini",
      wasInterrupted: true,
      updatedAt: 20,
    });
    expect(phaseToAIState(phase)).toBe("done");
  });

  it("keeps the project idle when only some worktrees are completed", () => {
    const phase = aggregateProjectPhase(
      "project-a",
      ["worktree-a", "worktree-b"],
      phaseMap({
        "project-a": { kind: "completed", tool: "codex", wasInterrupted: false, updatedAt: 10 },
        "worktree-a": { kind: "completed", tool: "claude", wasInterrupted: false, updatedAt: 20 },
        "worktree-b": { kind: "idle" },
      }),
    );

    expect(phase).toEqual({ kind: "idle" });
    expect(phaseToAIState(phase)).toBe("idle");
  });

  it("keeps the project running until all worktrees are no longer active", () => {
    const phase = aggregateProjectPhase(
      "project-a",
      ["worktree-a", "worktree-b"],
      phaseMap({
        "project-a": { kind: "completed", tool: "codex", wasInterrupted: false, updatedAt: 10 },
        "worktree-a": { kind: "completed", tool: "claude", wasInterrupted: false, updatedAt: 20 },
        "worktree-b": { kind: "running", tool: "gemini" },
      }),
    );

    expect(phase).toEqual({ kind: "running", tool: "gemini" });
    expect(phaseToAIState(phase)).toBe("running");
  });

  it("shows runtime activity ahead of completion presentation", () => {
    expect(
      resolveDisplayedProjectPhase(
        { kind: "running", tool: "codex" },
        { kind: "completed", tool: "codex", wasInterrupted: false, updatedAt: 10 },
      ),
    ).toEqual({ kind: "running", tool: "codex" });
    expect(
      resolveDisplayedProjectPhase(
        { kind: "idle" },
        { kind: "completed", tool: "codex", wasInterrupted: false, updatedAt: 10 },
      ),
    ).toEqual({ kind: "completed", tool: "codex", wasInterrupted: false, updatedAt: 10 });
  });
});
