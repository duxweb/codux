import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useRef, useState } from "react";
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

export function sanitizeGitRepositorySnapshot<T extends { isRepository: boolean; error?: string | null }>(snapshot: T): T {
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
  const [snapshot, setSnapshot] = useState<GitStatusSnapshot>(emptySnapshot);
  const [isLoading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const projectPathRef = useRef(project?.path);

  useEffect(() => {
    projectPathRef.current = project?.path;
  }, [project?.path]);

  useEffect(() => {
    setSnapshot(
      project?.path && !window.__TAURI_INTERNALS__
        ? {
            ...emptySnapshot,
            isRepository: true,
            branch: project.branch,
          }
        : emptySnapshot,
    );
    setError(null);
    setLoading(false);
  }, [project?.branch, project?.path]);

  const refresh = useCallback(async (options?: { silent?: boolean }) => {
    if (!project?.path) {
      setSnapshot(emptySnapshot);
      setError(null);
      return;
    }
    if (!window.__TAURI_INTERNALS__) {
      setSnapshot({
        ...emptySnapshot,
        isRepository: true,
        branch: project.branch,
      });
      return;
    }
    const requestPath = project.path;
    const silent = options?.silent ?? false;
    if (!silent) setLoading(true);
    try {
      const next = await invoke<GitStatusSnapshot>("git_status", {
        projectPath: requestPath,
      });
      if (projectPathRef.current !== requestPath) return;
      const normalized = sanitizeGitRepositorySnapshot(next);
      setSnapshot(normalized);
      setError(normalized.error ?? null);
    } catch (nextError) {
      if (projectPathRef.current !== requestPath) return;
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      setError(message);
      setSnapshot({
        ...emptySnapshot,
        error: message,
      });
    } finally {
      if (!silent && projectPathRef.current === requestPath) setLoading(false);
    }
  }, [project?.branch, project?.path]);

  const runSnapshotAction = useCallback(
    async (command: string, payload: Record<string, unknown>) => {
      if (!project?.path || !window.__TAURI_INTERNALS__) return snapshot;
      setLoading(true);
      try {
        const next = await invoke<GitStatusSnapshot>(command, payload);
        const normalized = sanitizeGitRepositorySnapshot(next);
        setSnapshot(normalized);
        setError(normalized.error ?? null);
        return normalized;
      } catch (nextError) {
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        setError(message);
        throw nextError;
      } finally {
        setLoading(false);
      }
    },
    [project?.path, snapshot],
  );

  const stage = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_stage", {
        request: {
          projectPath: project?.path ?? "",
          paths,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const unstage = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_unstage", {
        request: {
          projectPath: project?.path ?? "",
          paths,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const commit = useCallback(
    (message: string) =>
      runSnapshotAction("git_commit", {
        request: {
          projectPath: project?.path ?? "",
          message,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const commitAction = useCallback(
    (message: string, action: GitCommitAction) =>
      runSnapshotAction("git_commit_action", {
        request: {
          projectPath: project?.path ?? "",
          message,
          action,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const amendLastCommitMessage = useCallback(
    (message: string) =>
      runSnapshotAction("git_amend_last_commit_message", {
        request: {
          projectPath: project?.path ?? "",
          message,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const lastCommitMessage = useCallback(async () => {
    if (!project?.path || !window.__TAURI_INTERNALS__) return "";
    return invoke<string>("git_last_commit_message", {
      projectPath: project.path,
    });
  }, [project?.path]);

  const undoLastCommit = useCallback(
    () =>
      runSnapshotAction("git_undo_last_commit", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const headCommitPushed = useCallback(async () => {
    if (!project?.path || !window.__TAURI_INTERNALS__) return false;
    return invoke<boolean>("git_head_commit_pushed", {
      projectPath: project.path,
    });
  }, [project?.path]);

  const init = useCallback(
    () =>
      runSnapshotAction("git_init", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const cloneRepository = useCallback(
    (remoteUrl: string) =>
      runSnapshotAction("git_clone", {
        request: {
          projectPath: project?.path ?? "",
          remoteUrl,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const discard = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_discard", {
        request: {
          projectPath: project?.path ?? "",
          paths,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const checkoutBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_checkout_branch", {
        request: {
          projectPath: project?.path ?? "",
          branch,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const checkoutRemoteBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_checkout_remote_branch", {
        request: {
          projectPath: project?.path ?? "",
          branch,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const createBranch = useCallback(
    (branch: string, checkout = true, from?: string) =>
      runSnapshotAction("git_create_branch", {
        request: {
          projectPath: project?.path ?? "",
          branch,
          checkout,
          from,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const mergeBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_merge_branch", {
        request: {
          projectPath: project?.path ?? "",
          branch,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const squashMergeBranch = useCallback(
    (branch: string) =>
      runSnapshotAction("git_squash_merge_branch", {
        request: {
          projectPath: project?.path ?? "",
          branch,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const deleteBranch = useCallback(
    (branch: string, force = false) =>
      runSnapshotAction("git_delete_branch", {
        request: {
          projectPath: project?.path ?? "",
          branch,
          force,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const pull = useCallback(
    () =>
      runSnapshotAction("git_pull", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const fetch = useCallback(
    () =>
      runSnapshotAction("git_fetch", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const sync = useCallback(
    () =>
      runSnapshotAction("git_sync", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const push = useCallback(
    () =>
      runSnapshotAction("git_push", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const pushRemote = useCallback(
    (remote: string) =>
      runSnapshotAction("git_push_remote", {
        request: {
          projectPath: project?.path ?? "",
          remote,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const pushRemoteBranch = useCallback(
    (remoteBranch: string, localBranch?: string) =>
      runSnapshotAction("git_push_remote_branch", {
        request: {
          projectPath: project?.path ?? "",
          remoteBranch,
          localBranch,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const forcePush = useCallback(
    () =>
      runSnapshotAction("git_force_push", {
        projectPath: project?.path ?? "",
      }),
    [project?.path, runSnapshotAction],
  );

  const checkoutCommit = useCallback(
    (commit: string) =>
      runSnapshotAction("git_checkout_commit", {
        request: {
          projectPath: project?.path ?? "",
          commit,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const revertCommit = useCallback(
    (commit: string) =>
      runSnapshotAction("git_revert_commit", {
        request: {
          projectPath: project?.path ?? "",
          commit,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const restoreCommit = useCallback(
    (commit: string, forceRemote = false) =>
      runSnapshotAction("git_restore_commit", {
        request: {
          projectPath: project?.path ?? "",
          commit,
          forceRemote,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const addRemote = useCallback(
    (name: string, url: string) =>
      runSnapshotAction("git_add_remote", {
        request: {
          projectPath: project?.path ?? "",
          name,
          url,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const removeRemote = useCallback(
    (name: string) =>
      runSnapshotAction("git_remove_remote", {
        request: {
          projectPath: project?.path ?? "",
          name,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const appendGitignore = useCallback(
    (paths: string[]) =>
      runSnapshotAction("git_append_gitignore", {
        request: {
          projectPath: project?.path ?? "",
          paths,
        },
      }),
    [project?.path, runSnapshotAction],
  );

  const branches = useCallback(async () => {
    if (!project?.path || !window.__TAURI_INTERNALS__) {
      return {
        current: snapshot.branch,
        local: [],
        remote: [],
        isRepository: snapshot.isRepository,
        error: null,
      } satisfies GitBranchesSnapshot;
    }
    return invoke<GitBranchesSnapshot>("git_branches", {
      projectPath: project.path,
    });
  }, [project?.path, snapshot.branch, snapshot.isRepository]);

  const diffFile = useCallback(
    async (path: string, staged = false) => {
      if (!project?.path || !window.__TAURI_INTERNALS__) {
        return {
          path,
          diff: "",
          isRepository: snapshot.isRepository,
          error: null,
        } satisfies GitDiffSnapshot;
      }
      return invoke<GitDiffSnapshot>("git_diff_file", {
        request: {
          projectPath: project.path,
          path,
          staged,
        },
      });
    },
    [project?.path, snapshot.isRepository],
  );

  useEffect(() => {
    void refresh({ silent: true });
  }, [refresh]);

  useEffect(() => {
    if (!project?.path || !window.__TAURI_INTERNALS__) return;
    const projectPath = project.path;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void listen<GitStatusEvent>("git:status", (event) => {
      if (disposed || normalizeGitEventPath(event.payload.projectPath) !== normalizeGitEventPath(projectPath)) return;
      const normalized = sanitizeGitRepositorySnapshot(event.payload.snapshot);
      setSnapshot(normalized);
      setError(normalized.error ?? null);
      setLoading(false);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    }).catch((nextError) => {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      setError(message);
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [project?.path]);

  useEffect(() => {
    if (!project?.path || !window.__TAURI_INTERNALS__) return;
    const projectPath = project.path;
    let cancelled = false;
    let debounceTimer: number | undefined;
    let unlisten: (() => void) | undefined;
    let didUnlisten = false;
    const stopListening = (nextUnlisten: () => void) => {
      if (didUnlisten) return;
      didUnlisten = true;
      nextUnlisten();
    };

    const unlistenPromise = listen<GitRepositoryChangeEvent>("git:changed", (event) => {
      if (cancelled || !isGitChangeForProject(event.payload, projectPath)) return;
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(() => {
        void refresh({ silent: true });
      }, 280);
    });
    unlistenPromise
      .then((nextUnlisten) => {
        if (cancelled) {
          stopListening(nextUnlisten);
          return;
        }
        unlisten = () => stopListening(nextUnlisten);
      })
      .catch((nextError) => {
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        setError(message);
      });

    void invoke("git_watch", { projectPath }).catch((nextError) => {
      if (cancelled) return;
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      setError(message);
    });

    return () => {
      cancelled = true;
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      if (unlisten) {
        unlisten();
      } else {
        void unlistenPromise.then((nextUnlisten) => stopListening(nextUnlisten)).catch(() => undefined);
      }
      void invoke("git_unwatch", { projectPath }).catch(() => undefined);
    };
  }, [project?.path, refresh]);

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
