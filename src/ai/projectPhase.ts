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
  selectedWorktreeId: string | undefined,
  phaseFor: (projectId: string) => AIProjectPhase,
): AIProjectPhase {
  const ids =
    selectedWorktreeId && selectedWorktreeId !== projectId
      ? [projectId, selectedWorktreeId]
      : [projectId];
  return ids.map(phaseFor).reduce(preferredPhase, { kind: "idle" } as AIProjectPhase);
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
