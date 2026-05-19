import type { ProjectListSnapshot, ProjectStatus, ProjectSummary } from "./types";

const CACHE_KEY = "codux.projectListSnapshot.v1";

export function readCachedProjectListSnapshot(): ProjectListSnapshot | null {
  if (typeof window === "undefined") return null;
  try {
    const raw = window.localStorage.getItem(CACHE_KEY);
    if (!raw) return null;
    return sanitizeProjectListSnapshot(JSON.parse(raw));
  } catch {
    return null;
  }
}

export function writeCachedProjectListSnapshot(snapshot: ProjectListSnapshot) {
  if (typeof window === "undefined") return;
  try {
    window.localStorage.setItem(CACHE_KEY, JSON.stringify(snapshot));
  } catch {
    // Cache write failure should not affect the workspace model.
  }
}

export function sanitizeProjectListSnapshot(value: unknown): ProjectListSnapshot | null {
  if (!value || typeof value !== "object") return null;
  const record = value as Partial<ProjectListSnapshot>;
  if (!Array.isArray(record.projects)) return null;
  const projects = record.projects.flatMap((project): ProjectSummary[] => {
    if (!project || typeof project !== "object") return [];
    const item = project as unknown as Record<string, unknown>;
    const id = stringValue(item.id);
    const name = stringValue(item.name);
    const path = stringValue(item.path);
    if (!id || !name || !path) return [];
    return [
      {
        id,
        name,
        path,
        badge: stringValue(item.badge) ?? name.slice(0, 2).toUpperCase(),
        status: statusValue(item.status),
        branch: stringValue(item.branch) ?? "master",
        terminals: numberValue(item.terminals),
        changes: numberValue(item.changes),
        badgeSymbol: stringValue(item.badgeSymbol),
        badgeColorHex: stringValue(item.badgeColorHex),
      },
    ];
  });
  if (projects.length === 0) return null;
  const selectedProjectId =
    typeof record.selectedProjectId === "string" &&
    projects.some((project) => project.id === record.selectedProjectId)
      ? record.selectedProjectId
      : projects[0]?.id;
  const selectedWorktreeIdByProject =
    record.selectedWorktreeIdByProject &&
    typeof record.selectedWorktreeIdByProject === "object" &&
    !Array.isArray(record.selectedWorktreeIdByProject)
      ? Object.fromEntries(
          Object.entries(record.selectedWorktreeIdByProject).filter(
            ([projectId, worktreeId]) =>
              typeof projectId === "string" &&
              projectId.trim() !== "" &&
              typeof worktreeId === "string" &&
              worktreeId.trim() !== "",
          ),
        )
      : {};

  return {
    projects,
    selectedProjectId,
    selectedWorktreeIdByProject,
  };
}

function stringValue(value: unknown) {
  return typeof value === "string" && value.trim() !== "" ? value : undefined;
}

function numberValue(value: unknown) {
  return typeof value === "number" && Number.isFinite(value) ? value : undefined;
}

function statusValue(value: unknown): ProjectStatus {
  return value === "reference" || value === "idle" ? value : "active";
}
