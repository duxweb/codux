import { invoke } from "@tauri-apps/api/core";
import { useCallback, useEffect, useState } from "react";
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

const emptyReviewSnapshot: GitReviewSnapshot = {
  mode: "workingTreeAudit",
  title: "Uncommitted Audit",
  baseBranch: null,
  diffStat: "",
  files: [],
  isRepository: false,
  error: null,
};

export function useGitReviewSnapshot(projectPath?: string, baseBranch?: string | null) {
  const [snapshot, setSnapshot] = useState<GitReviewSnapshot>(emptyReviewSnapshot);
  const [isLoading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);

  const refresh = useCallback(async () => {
    if (!projectPath || !window.__TAURI_INTERNALS__) {
      setSnapshot(emptyReviewSnapshot);
      setError(null);
      return;
    }
    setLoading(true);
    try {
      const next = await invoke<GitReviewSnapshot>("git_review", {
        projectPath,
        baseBranch,
      });
      const normalized = sanitizeGitRepositorySnapshot(next);
      setSnapshot(normalized);
      setError(normalized.error ?? null);
    } catch (nextError) {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      setError(message);
      setSnapshot({
        ...emptyReviewSnapshot,
        error: message,
      });
    } finally {
      setLoading(false);
    }
  }, [baseBranch, projectPath]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  return {
    snapshot,
    isLoading,
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
