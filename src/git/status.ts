import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef } from "react";
import { useRuntimeStore } from "../runtimeStore";
import type { WorkspaceProject } from "../types";

export interface GitFileStatus {
  path: string;
  indexStatus: string;
  worktreeStatus: string;
}

export interface GitCommitSummary {
  hash: string;
  title: string;
  relativeTime: string;
  decorations?: string | null;
  graphPrefix: string;
  author: string;
}

export interface GitRemoteSummary {
  name: string;
  url: string;
}

export interface GitStatusSnapshot {
  branch: string;
  upstream?: string | null;
  ahead: number;
  behind: number;
  staged: GitFileStatus[];
  unstaged: GitFileStatus[];
  untracked: GitFileStatus[];
  commits: GitCommitSummary[];
  branches: GitBranchSummary[];
  remoteBranches: string[];
  remotes: GitRemoteSummary[];
  isRepository: boolean;
  error?: string | null;
}

export interface GitBranchSummary {
  name: string;
  upstream?: string | null;
  hash: string;
  isCurrent: boolean;
}

export interface GitBranchesSnapshot {
  current: string;
  local: GitBranchSummary[];
  remote: GitBranchSummary[];
  isRepository: boolean;
  error?: string | null;
}

export interface GitDiffSnapshot {
  path: string;
  diff: string;
  isRepository: boolean;
  error?: string | null;
}

export interface GitCommitMessageContextSnapshot {
  diff: string;
  truncated: boolean;
  isRepository: boolean;
  error?: string | null;
}

export interface GitRepositoryChangeEvent {
  projectPath: string;
  repositoryPath: string;
  changedPaths: string[];
}

export interface GitStatusEvent {
  projectId: string;
  projectName: string;
  projectPath: string;
  snapshot: GitStatusSnapshot;
}

export type GitCommitAction = "commit" | "commitAndPush" | "commitAndSync";

const emptySnapshot: GitStatusSnapshot = {
  branch: "uninitialized",
  upstream: null,
  ahead: 0,
  behind: 0,
  staged: [],
  unstaged: [],
  untracked: [],
  commits: [],
  branches: [],
  remoteBranches: [],
  remotes: [],
  isRepository: false,
  error: null,
};

let gitStatusCacheListenerPromise: Promise<() => void> | null = null;

function optimisticGitSnapshot(project?: WorkspaceProject): GitStatusSnapshot {
  if (!project?.path) return emptySnapshot;
  return {
    ...emptySnapshot,
    branch: project.branch || "loading",
    isRepository: true,
  };
}

function cacheGitStatusEvent(event: GitStatusEvent) {
  const key = normalizeGitEventPath(event.projectPath);
  if (!key) return;
  const snapshot = sanitizeGitRepositorySnapshot(event.snapshot);
  useRuntimeStore.getState().setGitStatus(key, {
    snapshot,
    error: snapshot.error ?? null,
    updatedAt: Date.now(),
  });
}

export function ensureGitStatusEventCacheSubscription() {
  if (!window.__TAURI_INTERNALS__ || gitStatusCacheListenerPromise) return;
  gitStatusCacheListenerPromise = listen<GitStatusEvent>("git:status", (event) => {
    cacheGitStatusEvent(event.payload);
  }).catch((error) => {
    gitStatusCacheListenerPromise = null;
    console.error("failed to cache git status events", error);
    return () => {};
  });
}

export function sanitizeGitRepositorySnapshot<T extends { isRepository: boolean; error?: string | null }>(
  snapshot: T,
): T {
  if (snapshot.isRepository) return snapshot;
  if (!snapshot.error || isGitNotRepositoryError(snapshot.error)) {
    return {
      ...snapshot,
      error: null,
    };
  }
  return snapshot;
}

export function normalizeGitEventPath(value: string) {
  return value.replace(/\\/g, "/").replace(/\/+$/, "");
}

export function isGitChangeForProject(event: GitRepositoryChangeEvent, projectPath: string) {
  const project = normalizeGitEventPath(projectPath);
  const eventProject = normalizeGitEventPath(event.projectPath);
  const repository = normalizeGitEventPath(event.repositoryPath);

  return (
    eventProject === project ||
    repository === project ||
    project.startsWith(`${repository}/`) ||
    repository.startsWith(`${project}/`)
  );
}

export function useGitStatusSnapshot(project?: WorkspaceProject) {
  const projectPath = project?.path ?? "";
  const projectBranch = project?.branch ?? "";
  const defaultPushRemoteName = project?.gitDefaultPushRemoteName ?? "";
  const projectCacheKey = projectPath ? normalizeGitEventPath(projectPath) : "";
  const snapshot =
    useRuntimeStore((state) => (projectCacheKey ? state.gitStatusByPath[projectCacheKey]?.snapshot : undefined)) ??
    optimisticGitSnapshot(project);
  const hasCachedSnapshot = useRuntimeStore((state) =>
    projectCacheKey ? Boolean(state.gitStatusByPath[projectCacheKey]) : false,
  );
  const isLoading = useRuntimeStore((state) =>
    projectCacheKey ? (state.gitLoadingByPath[projectCacheKey] ?? false) : false,
  );
  const error = useRuntimeStore((state) => (projectCacheKey ? (state.gitErrorByPath[projectCacheKey] ?? null) : null));
  const projectPathRef = useRef(projectPath);

  useEffect(() => {
    projectPathRef.current = projectPath;
  }, [projectPath]);

  const applySnapshot = useCallback(
    (next: GitStatusSnapshot) => {
      const normalized = sanitizeGitRepositorySnapshot(next);
      if (projectCacheKey) {
        useRuntimeStore.getState().setGitStatus(projectCacheKey, {
          snapshot: normalized,
          error: normalized.error ?? null,
          updatedAt: Date.now(),
        });
      }
      return normalized;
    },
    [projectCacheKey],
  );

  const refresh = useCallback(
    async (options?: { silent?: boolean }) => {
      if (!projectPath) {
        return;
      }
      if (!window.__TAURI_INTERNALS__) {
        if (projectCacheKey) {
          useRuntimeStore.getState().setGitStatus(projectCacheKey, {
            snapshot: {
              ...emptySnapshot,
              isRepository: true,
              branch: projectBranch,
            },
            error: null,
            updatedAt: Date.now(),
          });
        }
        return;
      }
      const requestPath = projectPath;
      const silent = options?.silent ?? false;
      if (!silent && projectCacheKey) {
        useRuntimeStore.getState().setGitLoading(projectCacheKey, true);
      }
      try {
        await invoke("git_refresh_project", {
          projectPath: requestPath,
        });
        if (projectPathRef.current !== requestPath) return;
      } catch (nextError) {
        if (projectPathRef.current !== requestPath) return;
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        if (projectCacheKey) {
          useRuntimeStore.getState().setGitStatus(projectCacheKey, {
            snapshot: {
              ...emptySnapshot,
              error: message,
            },
            error: message,
            updatedAt: Date.now(),
          });
        }
      } finally {
        if (!silent && projectPathRef.current === requestPath && projectCacheKey) {
          useRuntimeStore.getState().setGitLoading(projectCacheKey, false);
        }
      }
    },
    [projectBranch, projectPath, projectCacheKey],
  );

  useEffect(() => {
    if (!projectPath || !projectCacheKey || hasCachedSnapshot || isLoading) {
      return;
    }
    void refresh({ silent: true });
  }, [hasCachedSnapshot, isLoading, projectCacheKey, projectPath, refresh]);

  const runSnapshotAction = useCallback(
    async (command: string, payload: Record<string, unknown>) => {
      if (!projectPath || !window.__TAURI_INTERNALS__) return snapshot;
      if (projectCacheKey) {
        useRuntimeStore.getState().setGitLoading(projectCacheKey, true);
      }
      try {
        const next = await invoke<GitStatusSnapshot>(command, payload);
        return applySnapshot(next);
      } catch (nextError) {
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        if (projectCacheKey) {
          useRuntimeStore.getState().setGitError(projectCacheKey, message);
        }
        throw nextError;
      } finally {
        if (projectCacheKey) {
          useRuntimeStore.getState().setGitLoading(projectCacheKey, false);
        }
      }
    },
    [applySnapshot, projectCacheKey, projectPath, snapshot],
  );

  const stage = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_stage", {
        request: {
          projectPath: projectPath,
          paths,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const unstage = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_unstage", {
        request: {
          projectPath: projectPath,
          paths,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const commit = useCallback(
    (message: string) =>
      runSnapshotAction("git_commit", {
        request: {
          projectPath: projectPath,
          message,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const commitAction = useCallback(
    (message: string, action: GitCommitAction) =>
      runSnapshotAction("git_commit_action", {
        request: {
          projectPath: projectPath,
          message,
          action,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const amendLastCommitMessage = useCallback(
    (message: string) =>
      runSnapshotAction("git_amend_last_commit_message", {
        request: {
          projectPath: projectPath,
          message,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const lastCommitMessage = useCallback(async () => {
    if (!projectPath || !window.__TAURI_INTERNALS__) return "";
    return invoke<string>("git_last_commit_message", {
      projectPath,
    });
  }, [projectPath]);

  const undoLastCommit = useCallback(
    () =>
      runSnapshotAction("git_undo_last_commit", {
        projectPath: projectPath,
      }),
    [projectPath, runSnapshotAction],
  );

  const headCommitPushed = useCallback(async () => {
    if (!projectPath || !window.__TAURI_INTERNALS__) return false;
    return invoke<boolean>("git_head_commit_pushed", {
      projectPath,
    });
  }, [projectPath]);

  const init = useCallback(
    () =>
      runSnapshotAction("git_init", {
        projectPath: projectPath,
      }),
    [projectPath, runSnapshotAction],
  );

  const cloneRepository = useCallback(
    (remoteUrl: string) =>
      runSnapshotAction("git_clone", {
        request: {
          projectPath: projectPath,
          remoteUrl,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const discard = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_discard", {
        request: {
          projectPath: projectPath,
          paths,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const checkoutBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_checkout_branch", {
        request: {
          projectPath: projectPath,
          branch,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const checkoutRemoteBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_checkout_remote_branch", {
        request: {
          projectPath: projectPath,
          branch,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const createBranch = useCallback(
    (branch: string, checkout = true, from?: string) =>
      runSnapshotAction("git_create_branch", {
        request: {
          projectPath: projectPath,
          branch,
          checkout,
          from,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const mergeBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_merge_branch", {
        request: {
          projectPath: projectPath,
          branch,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const squashMergeBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_squash_merge_branch", {
        request: {
          projectPath: projectPath,
          branch,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const deleteBranch = useCallback(
    (branch: string, force = false) =>
      runSnapshotAction("git_delete_branch", {
        request: {
          projectPath: projectPath,
          branch,
          force,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const pull = useCallback(
    () =>
      runSnapshotAction("git_pull", {
        projectPath: projectPath,
      }),
    [projectPath, runSnapshotAction],
  );

  const fetch = useCallback(
    () =>
      runSnapshotAction("git_fetch", {
        projectPath: projectPath,
      }),
    [projectPath, runSnapshotAction],
  );

  const sync = useCallback(
    () =>
      runSnapshotAction("git_sync", {
        projectPath: projectPath,
      }),
    [projectPath, runSnapshotAction],
  );

  const push = useCallback(() => {
    const remote = defaultPushRemoteName.trim();
    if (remote) {
      return runSnapshotAction("git_push_remote", {
        request: {
          projectPath: projectPath,
          remote,
        },
      });
    }
    return runSnapshotAction("git_push", {
      projectPath: projectPath,
    });
  }, [defaultPushRemoteName, projectPath, runSnapshotAction]);

  const pushRemote = useCallback(
    (remote: string) =>
      runSnapshotAction("git_push_remote", {
        request: {
          projectPath: projectPath,
          remote,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const pushRemoteBranch = useCallback(
    (remoteBranch: string, localBranch?: string) =>
      runSnapshotAction("git_push_remote_branch", {
        request: {
          projectPath: projectPath,
          remoteBranch,
          localBranch,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const forcePush = useCallback(
    () =>
      runSnapshotAction("git_force_push", {
        projectPath: projectPath,
      }),
    [projectPath, runSnapshotAction],
  );

  const checkoutCommit = useCallback(
    (commit: string) =>
      runSnapshotAction("git_checkout_commit", {
        request: {
          projectPath: projectPath,
          commit,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const revertCommit = useCallback(
    (commit: string) =>
      runSnapshotAction("git_revert_commit", {
        request: {
          projectPath: projectPath,
          commit,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const restoreCommit = useCallback(
    (commit: string, forceRemote = false) =>
      runSnapshotAction("git_restore_commit", {
        request: {
          projectPath: projectPath,
          commit,
          forceRemote,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const addRemote = useCallback(
    (name: string, url: string) =>
      runSnapshotAction("git_add_remote", {
        request: {
          projectPath: projectPath,
          name,
          url,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const removeRemote = useCallback(
    (name: string) =>
      runSnapshotAction("git_remove_remote", {
        request: {
          projectPath: projectPath,
          name,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const appendGitignore = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_append_gitignore", {
        request: {
          projectPath: projectPath,
          paths,
        },
      }),
    [projectPath, runSnapshotAction],
  );

  const branches = useCallback(async () => {
    if (!projectPath || !window.__TAURI_INTERNALS__) {
      return {
        current: snapshot.branch,
        local: [],
        remote: [],
        isRepository: snapshot.isRepository,
        error: null,
      } satisfies GitBranchesSnapshot;
    }
    return invoke<GitBranchesSnapshot>("git_branches", {
      projectPath,
    });
  }, [projectPath, snapshot.branch, snapshot.isRepository]);

  const diffFile = useCallback(
    async (path: string, staged = false) => {
      if (!projectPath || !window.__TAURI_INTERNALS__) {
        return {
          path,
          diff: "",
          isRepository: snapshot.isRepository,
          error: null,
        } satisfies GitDiffSnapshot;
      }
      return invoke<GitDiffSnapshot>("git_diff_file", {
        request: {
          projectPath,
          path,
          staged,
        },
      });
    },
    [projectPath, snapshot.isRepository],
  );

  return {
    snapshot,
    isLoading,
    error,
    refresh,
    stage,
    unstage,
    commit,
    commitAction,
    amendLastCommitMessage,
    lastCommitMessage,
    undoLastCommit,
    headCommitPushed,
    init,
    cloneRepository,
    discard,
    checkoutBranch,
    checkoutRemoteBranch,
    createBranch,
    mergeBranch,
    squashMergeBranch,
    deleteBranch,
    pull,
    fetch,
    sync,
    push,
    pushRemote,
    pushRemoteBranch,
    forcePush,
    checkoutCommit,
    revertCommit,
    restoreCommit,
    addRemote,
    removeRemote,
    appendGitignore,
    branches,
    diffFile,
  };
}

function isGitNotRepositoryError(message: string) {
  return /not a git repository|GIT_DISCOVERY_ACROSS_FILESYSTEM|must be run in a work tree/i.test(message);
}
