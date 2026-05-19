import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
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
  tasks: WorktreeTaskSnapshot[];
  error?: string | null;
}

export interface WorktreeCreateInput {
  projectId: string;
  projectPath: string;
  baseBranch?: string | null;
  branchName: string;
  taskTitle?: string | null;
}

export interface WorktreeRemoveInput {
  projectId: string;
  projectPath: string;
  worktreePath: string;
}

const worktreeSnapshotRequests = new Map<string, Promise<WorktreeSnapshot>>();

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
        },
      },
    ],
    tasks: [],
    error: null,
  };
}

export function useWorktreeSnapshot(project?: WorkspaceProject) {
  const [snapshot, setSnapshot] = useState<WorktreeSnapshot>(() =>
    emptyWorktreeSnapshot(project),
  );
  const [isLoading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!project?.id || !project.path) {
      const next = emptyWorktreeSnapshot(project);
      setSnapshot(next);
      setError(null);
      return next;
    }
    if (!window.__TAURI_INTERNALS__) {
      const next = emptyWorktreeSnapshot(project);
      setSnapshot(next);
      setError(null);
      return next;
    }
    setLoading(true);
    try {
      const requestKey = `${project.id}:${project.path}`;
      let request = worktreeSnapshotRequests.get(requestKey);
      if (!request) {
        request = invoke<WorktreeSnapshot>("worktree_snapshot", {
          projectId: project.id,
          projectPath: project.path,
        }).finally(() => {
          worktreeSnapshotRequests.delete(requestKey);
        });
        worktreeSnapshotRequests.set(requestKey, request);
      }
      const next = await request;
      setSnapshot(next);
      setError(next.error ?? null);
      return next;
    } catch (nextError) {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      const next = { ...emptyWorktreeSnapshot(project), error: message };
      setSnapshot(next);
      setError(message);
      return next;
    } finally {
      setLoading(false);
    }
  }, [project?.id, project?.path, project?.branch, project?.changes]);

  const create = useCallback(
    async (input: WorktreeCreateInput) => {
      if (!window.__TAURI_INTERNALS__) return snapshot;
      const next = await invoke<WorktreeSnapshot>("worktree_create", { request: input });
      setSnapshot(next);
      setError(next.error ?? null);
      return next;
    },
    [snapshot],
  );

  const remove = useCallback(
    async (input: WorktreeRemoveInput) => {
      if (!window.__TAURI_INTERNALS__) return snapshot;
      const next = await invoke<WorktreeSnapshot>("worktree_remove", { request: input });
      setSnapshot(next);
      setError(next.error ?? null);
      return next;
    },
    [snapshot],
  );

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!project?.path || !window.__TAURI_INTERNALS__) return;
    const timer = window.setInterval(() => void refresh(), 7000);
    return () => window.clearInterval(timer);
  }, [project?.path, refresh]);

  return {
    snapshot,
    isLoading,
    error,
    refresh,
    create,
    remove,
  };
}
