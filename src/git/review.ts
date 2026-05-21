import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback } from "react";
import { useRuntimeStore } from "../runtimeStore";
import { sanitizeGitRepositorySnapshot } from "./status";

export interface GitReviewFile {
  path: string;
  status: "added" | "modified" | "deleted" | "renamed" | "copied" | "typeChanged" | "unknown";
  additions: number;
  deletions: number;
}

export interface GitReviewSnapshot {
  mode: "workingTreeAudit" | "taskBranch";
  title: string;
  baseBranch?: string | null;
  diffStat: string;
  files: GitReviewFile[];
  isRepository: boolean;
  error?: string | null;
}

export interface GitReviewDiffSnapshot {
  path: string;
  diff: string;
  isRepository: boolean;
  error?: string | null;
}

export interface GitReviewContentSnapshot {
  path: string;
  headContent: string;
  baseContent?: string | null;
  indexContent?: string | null;
  worktreeContent: string;
  addedLines: number[];
  deletedLines: number[];
  isRepository: boolean;
  error?: string | null;
}

export interface GitReviewEvent {
  projectId: string;
  projectName: string;
  projectPath: string;
  baseBranch?: string | null;
  snapshot: GitReviewSnapshot;
}

const emptyReviewSnapshot: GitReviewSnapshot = {
  mode: "workingTreeAudit",
  title: "Uncommitted Audit",
  baseBranch: null,
  diffStat: "",
  files: [],
  isRepository: false,
  error: null,
};

let gitReviewCacheListenerPromise: Promise<() => void> | null = null;

function reviewCacheKey(projectPath?: string, baseBranch?: string | null) {
  return projectPath ? `${projectPath}:${baseBranch ?? ""}` : "";
}

function cacheGitReviewEvent(event: GitReviewEvent) {
  const key = reviewCacheKey(event.projectPath, event.baseBranch);
  if (!key) return;
  const snapshot = sanitizeGitRepositorySnapshot(event.snapshot);
  useRuntimeStore.getState().setGitReview(key, {
    snapshot,
    error: snapshot.error ?? null,
    updatedAt: Date.now(),
  });
}

export function ensureGitReviewEventCacheSubscription() {
  if (!window.__TAURI_INTERNALS__ || gitReviewCacheListenerPromise) return;
  gitReviewCacheListenerPromise = listen<GitReviewEvent>("git:review", (event) => {
    cacheGitReviewEvent(event.payload);
  }).catch((error) => {
    gitReviewCacheListenerPromise = null;
    console.error("failed to cache git review events", error);
    return () => {};
  });
}

export function useGitReviewSnapshot(projectPath?: string, baseBranch?: string | null) {
  const cacheKey = reviewCacheKey(projectPath, baseBranch);
  const cached = useRuntimeStore((state) => (cacheKey ? state.gitReviewByKey[cacheKey] : undefined));
  const snapshot = cached?.snapshot ?? emptyReviewSnapshot;
  const error = cached?.error ?? null;

  const refresh = useCallback(async () => {
    if (!projectPath || !window.__TAURI_INTERNALS__) {
      return;
    }
    try {
      const next = await invoke<GitReviewSnapshot>("git_review", {
        projectPath,
        baseBranch,
      });
      const normalized = sanitizeGitRepositorySnapshot(next);
      useRuntimeStore.getState().setGitReview(reviewCacheKey(projectPath, baseBranch), {
        snapshot: normalized,
        error: normalized.error ?? null,
        updatedAt: Date.now(),
      });
    } catch (nextError) {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      useRuntimeStore.getState().setGitReview(reviewCacheKey(projectPath, baseBranch), {
        snapshot: {
          ...emptyReviewSnapshot,
          error: message,
        },
        error: message,
        updatedAt: Date.now(),
      });
    }
  }, [baseBranch, projectPath]);

  return {
    snapshot,
    isLoading: false,
    error,
    refresh,
  };
}

export async function loadGitReviewDiff(projectPath: string, path: string, baseBranch?: string | null) {
  if (!window.__TAURI_INTERNALS__) {
    return {
      path,
      diff: "",
      isRepository: true,
      error: null,
    } satisfies GitReviewDiffSnapshot;
  }
  return invoke<GitReviewDiffSnapshot>("git_review_diff_file", {
    request: {
      projectPath,
      path,
      baseBranch,
    },
  });
}

export async function loadGitReviewFileContent(projectPath: string, path: string, baseBranch?: string | null) {
  if (!window.__TAURI_INTERNALS__) {
    return {
      path,
      headContent: "",
      baseContent: null,
      indexContent: null,
      worktreeContent: "",
      addedLines: [],
      deletedLines: [],
      isRepository: true,
      error: null,
    } satisfies GitReviewContentSnapshot;
  }
  return invoke<GitReviewContentSnapshot>("git_review_file_content", {
    request: {
      projectPath,
      path,
      baseBranch,
    },
  });
}
