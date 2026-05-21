import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
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
  const signature = gitReviewSignature(snapshot);
  const current = useRuntimeStore.getState().gitReviewByKey[key];
  if (current?.signature === signature) return;
  useRuntimeStore.getState().setGitReview(key, {
    snapshot,
    error: snapshot.error ?? null,
    updatedAt: Date.now(),
    signature,
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
  const [isLoading, setLoading] = useState(false);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || !projectPath || cached) {
      setLoading(false);
      return;
    }
    let disposed = false;
    setLoading(true);
    invoke<GitReviewSnapshot>("git_review", {
      projectPath,
      baseBranch,
    })
      .catch((reason) => {
        if (disposed) return;
        console.error("failed to refresh git review snapshot", reason);
      })
      .finally(() => {
        if (!disposed) setLoading(false);
      });
    return () => {
      disposed = true;
    };
  }, [baseBranch, cacheKey, cached, projectPath]);

  return {
    snapshot,
    updatedAt: cached?.updatedAt ?? 0,
    isLoading,
    error,
  };
}

function gitReviewSignature(snapshot: GitReviewSnapshot) {
  return JSON.stringify({
    mode: snapshot.mode,
    baseBranch: snapshot.baseBranch ?? null,
    diffStat: snapshot.diffStat,
    isRepository: snapshot.isRepository,
    error: snapshot.error ?? null,
    files: snapshot.files.map((file) => [file.path, file.status, file.additions, file.deletions]),
  });
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
