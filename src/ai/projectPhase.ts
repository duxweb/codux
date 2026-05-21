import type { AIProjectPhase } from "./types";
import type { WorkspaceProject } from "../types";

export function phaseToAIState(phase: AIProjectPhase): WorkspaceProject["aiState"] {
  if (phase.kind === "running") return "running";
  if (phase.kind === "needsInput") return "review";
  if (phase.kind === "completed") return "done";
  return "idle";
}

export function aggregateProjectPhase(
  projectId: string,
  worktreeIds: string[] | undefined,
  phaseFor: (projectId: string) => AIProjectPhase,
): AIProjectPhase {
  const ids = uniqueIds([projectId, ...(worktreeIds ?? [])]);
  const phases = ids.map(phaseFor);
  const needsInput = phases.find((phase) => phase.kind === "needsInput");
  if (needsInput) return needsInput;
  const running = phases.find((phase) => phase.kind === "running");
  if (running) return running;
  if (phases.length > 0 && phases.every((phase) => phase.kind === "completed")) {
    return phases.reduce(preferredPhase, { kind: "idle" } as AIProjectPhase);
  }
  return { kind: "idle" };
}

export function resolveDisplayedProjectPhase(
  runtimePhase: AIProjectPhase,
  completedPhase: AIProjectPhase,
): AIProjectPhase {
  return runtimePhase.kind === "idle" ? completedPhase : runtimePhase;
}

function preferredPhase(left: AIProjectPhase, right: AIProjectPhase): AIProjectPhase {
  const leftPriority = phasePriority(left);
  const rightPriority = phasePriority(right);
  if (rightPriority > leftPriority) return right;
  if (rightPriority < leftPriority) return left;
  if (left.kind === "completed" && right.kind === "completed") {
    return right.updatedAt > left.updatedAt ? right : left;
  }
  return left.kind === "idle" ? right : left;
}

function phasePriority(phase: AIProjectPhase) {
  if (phase.kind === "needsInput") return 3;
  if (phase.kind === "running") return 2;
  if (phase.kind === "completed") return 1;
  return 0;
}

function uniqueIds(ids: string[]) {
  return ids.filter((id, index) => id && ids.indexOf(id) === index);
}
