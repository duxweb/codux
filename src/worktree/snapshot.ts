import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef } from "react";
import { useRuntimeStore } from "../runtimeStore";
import type { WorkspaceProject } from "../types";

export type WorktreeTaskStatus =
  | "todo"
  | "planning"
  | "ready"
  | "running"
  | "waiting"
  | "review"
  | "blocked"
  | "done"
  | "merged"
  | "archived";

export interface ProjectWorktreeGitSummary {
  changes: number;
  incoming: number;
  outgoing: number;
  additions: number;
  deletions: number;
}

export interface ProjectWorktreeSnapshot {
  id: string;
  projectId: string;
  name: string;
  branch: string;
  path: string;
  status: WorktreeTaskStatus;
  isDefault: boolean;
  createdAt: number;
  updatedAt: number;
  gitSummary: ProjectWorktreeGitSummary;
}

export interface WorktreeTaskSnapshot {
  worktreeId: string;
  title: string;
  baseBranch: string;
  baseCommit?: string | null;
  status: WorktreeTaskStatus;
  createdAt: number;
  updatedAt: number;
  startedAt?: number | null;
  completedAt?: number | null;
}

export interface WorktreeSnapshot {
  projectId: string;
  selectedWorktreeId: string;
  worktrees: ProjectWorktreeSnapshot[];
  /** @deprecated Worktree is the task entity; kept only for old persisted snapshots. */
  tasks: WorktreeTaskSnapshot[];
  error?: string | null;
}

export interface WorktreeSnapshotEvent {
  projectId: string;
  projectPath: string;
  snapshot: WorktreeSnapshot;
}

export interface WorktreeCreateInput {
  projectId: string;
  projectPath: string;
  baseBranch?: string | null;
  branchName: string;
}

export interface WorktreeRemoveInput {
  projectId: string;
  projectPath: string;
  worktreePath: string;
}

let worktreeSnapshotListenerPromise: Promise<() => void> | null = null;

function worktreeSnapshotKey(projectId: string, projectPath: string) {
  return projectId && projectPath ? `${projectId}:${projectPath}` : "";
}

function cacheWorktreeSnapshot(projectId: string, projectPath: string, snapshot: WorktreeSnapshot) {
  const key = worktreeSnapshotKey(projectId, projectPath);
  if (!key) return;
  const store = useRuntimeStore.getState();
  store.setWorktreeSnapshot(key, {
    snapshot,
    error: snapshot.error ?? null,
    updatedAt: Date.now(),
  });
  store.setWorktreeLoading(key, false);
}

export function ensureWorktreeSnapshotEventCacheSubscription() {
  if (!window.__TAURI_INTERNALS__ || worktreeSnapshotListenerPromise) return;
  worktreeSnapshotListenerPromise = listen<WorktreeSnapshotEvent>("worktree:snapshot", (event) => {
    cacheWorktreeSnapshot(event.payload.projectId, event.payload.projectPath, event.payload.snapshot);
  }).catch((error) => {
    worktreeSnapshotListenerPromise = null;
    console.error("failed to cache worktree snapshot events", error);
    return () => {};
  });
}

export function emptyWorktreeSnapshot(project?: WorkspaceProject): WorktreeSnapshot {
  if (!project) {
    return {
      projectId: "",
      selectedWorktreeId: "",
      worktrees: [],
      tasks: [],
      error: null,
    };
  }
  return {
    projectId: project.id,
    selectedWorktreeId: project.id,
    worktrees: [
      {
        id: project.id,
        projectId: project.id,
        name: project.branch || project.name,
        branch: project.branch,
        path: project.path,
        status: "todo",
        isDefault: true,
        createdAt: 0,
        updatedAt: 0,
        gitSummary: {
          changes: project.changes,
          incoming: 0,
          outgoing: 0,
          additions: 0,
          deletions: 0,
        },
      },
    ],
    tasks: [],
    error: null,
  };
}

export function useWorktreeSnapshot(project?: WorkspaceProject) {
  const projectId = project?.id ?? "";
  const projectPath = project?.path ?? "";
  const cacheKey = worktreeSnapshotKey(projectId, projectPath);
  const cachedEntry = useRuntimeStore((state) => (cacheKey ? state.worktreeSnapshotByKey[cacheKey] : undefined));
  const snapshot = cachedEntry?.snapshot ?? emptyWorktreeSnapshot(project);
  const isLoading = useRuntimeStore((state) => (cacheKey ? (state.worktreeLoadingByKey[cacheKey] ?? false) : false));
  const error = useRuntimeStore((state) => (cacheKey ? (state.worktreeErrorByKey[cacheKey] ?? null) : null));
  const projectKeyRef = useRef(cacheKey);

  useEffect(() => {
    projectKeyRef.current = cacheKey;
  }, [cacheKey]);

  const applySnapshot = useCallback((key: string, next: WorktreeSnapshot) => {
    if (!key) return next;
    useRuntimeStore.getState().setWorktreeSnapshot(key, {
      snapshot: next,
      error: next.error ?? null,
      updatedAt: Date.now(),
    });
    return next;
  }, []);

  const refresh = useCallback(async () => {
    if (!project || !projectId || !projectPath) {
      const next = emptyWorktreeSnapshot(project);
      if (cacheKey) applySnapshot(cacheKey, next);
      return next;
    }
    if (!window.__TAURI_INTERNALS__) {
      const next = emptyWorktreeSnapshot(project);
      applySnapshot(cacheKey, next);
      return next;
    }
    const requestKey = `${projectId}:${projectPath}`;
    useRuntimeStore.getState().setWorktreeLoading(requestKey, true);
    try {
      await invoke("project_mark_active", {
        project: {
          id: project.id,
          name: project.name,
          path: project.path,
          badge: project.badge,
          status: project.status,
          branch: project.branch,
          changes: project.changes,
          badgeSymbol: project.badgeSymbol ?? null,
          badgeColorHex: project.badgeColorHex ?? null,
          gitDefaultPushRemoteName: project.gitDefaultPushRemoteName ?? null,
        },
      });
    } catch (nextError) {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      const next = { ...emptyWorktreeSnapshot(project), error: message };
      if (projectKeyRef.current === requestKey) {
        applySnapshot(requestKey, next);
      }
      return next;
    } finally {
      useRuntimeStore.getState().setWorktreeLoading(requestKey, false);
    }
    return useRuntimeStore.getState().worktreeSnapshotByKey[requestKey]?.snapshot ?? emptyWorktreeSnapshot(project);
  }, [applySnapshot, cacheKey, project, projectId, projectPath]);

  const create = useCallback(
    async (input: WorktreeCreateInput) => {
      if (!window.__TAURI_INTERNALS__) return snapshot;
      const requestKey = `${input.projectId}:${input.projectPath}`;
      useRuntimeStore.getState().setWorktreeLoading(requestKey, true);
      try {
        const next = await invoke<WorktreeSnapshot>("worktree_create", { request: input });
        applySnapshot(requestKey, next);
        return next;
      } finally {
        useRuntimeStore.getState().setWorktreeLoading(requestKey, false);
      }
    },
    [applySnapshot, snapshot],
  );

  const remove = useCallback(
    async (input: WorktreeRemoveInput) => {
      if (!window.__TAURI_INTERNALS__) return snapshot;
      const next = await invoke<WorktreeSnapshot>("worktree_remove", { request: input });
      applySnapshot(`${input.projectId}:${input.projectPath}`, next);
      return next;
    },
    [applySnapshot, snapshot],
  );

  return {
    snapshot,
    isLoading,
    error,
    refresh,
    create,
    remove,
  };
}
