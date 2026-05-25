export function normalize(value?: string | null) {
  const normalized = value?.trim();
  return normalized ? normalized : undefined;
}

export function numberOr(previous: number | undefined, value?: number | null) {
  if (typeof value === "number" && Number.isFinite(value)) {
    return Math.max(0, Math.floor(value));
  }
  return previous ?? 0;
}

export function canonicalToolName(tool?: string | null) {
  const normalized = normalize(tool)?.toLowerCase();
  if (!normalized) return "";
  if (normalized === "claude-code") return "claude";
  if (normalized === "agy") return "gemini";
  return normalized;
}

export function statusForState(state: "idle" | "responding" | "needsInput") {
  if (state === "responding") return "running";
  if (state === "needsInput") return "needs-input";
  return "idle";
}

export function projectPathContains(projectPath?: string | null, cwd?: string | null) {
  const project = normalizePath(projectPath);
  const current = normalizePath(cwd);
  if (!project || !current) return true;
  return current === project || current.startsWith(`${project}/`);
}

export function normalizePath(path?: string | null) {
  const normalized = normalize(path);
  if (!normalized) return undefined;
  return normalized.replace(/\/+$/g, "");
}
