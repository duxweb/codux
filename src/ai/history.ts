import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { WorkspaceProject } from "../types";

export type AIProjectUsageSummary = {
  projectId: string;
  projectName: string;
  currentSessionTokens: number;
  currentSessionCachedInputTokens: number;
  projectTotalTokens: number;
  projectCachedInputTokens: number;
  todayTotalTokens: number;
  todayCachedInputTokens: number;
  currentTool?: string | null;
  currentModel?: string | null;
  currentSessionUpdatedAt?: number | null;
};

export type AIHistorySessionSummary = {
  sessionId: string;
  externalSessionId?: string | null;
  projectId: string;
  projectName: string;
  sessionTitle: string;
  firstSeenAt: number;
  lastSeenAt: number;
  lastTool?: string | null;
  lastModel?: string | null;
  requestCount: number;
  totalInputTokens: number;
  totalOutputTokens: number;
  totalTokens: number;
  cachedInputTokens: number;
  activeDurationSeconds: number;
  todayTokens: number;
  todayCachedInputTokens: number;
};

export type AIHeatmapDay = {
  day: number;
  totalTokens: number;
  cachedInputTokens: number;
  requestCount: number;
};

export type AITimeBucket = {
  start: number;
  end: number;
  totalTokens: number;
  cachedInputTokens: number;
  requestCount: number;
};

export type AIUsageBreakdownItem = {
  key: string;
  totalTokens: number;
  cachedInputTokens: number;
  requestCount: number;
};

export type AIHistorySnapshot = {
  projectId: string;
  projectName: string;
  projectSummary: AIProjectUsageSummary;
  sessions: AIHistorySessionSummary[];
  heatmap: AIHeatmapDay[];
  todayTimeBuckets: AITimeBucket[];
  toolBreakdown: AIUsageBreakdownItem[];
  modelBreakdown: AIUsageBreakdownItem[];
  indexedAt: number;
};

export type AIHistoryProjectState = {
  projectId: string;
  projectName: string;
  projectPath: string;
  snapshot: AIHistorySnapshot | null;
  isLoading: boolean;
  queued: boolean;
  progress: number | null;
  detail: string;
  error: string | null;
  version: number;
};

export type AIGlobalHistorySnapshot = {
  totalTokens: number;
  cachedInputTokens: number;
  todayTotalTokens: number;
  todayCachedInputTokens: number;
  sessions: AIHistorySessionSummary[];
  projectCount: number;
  indexedAt: number;
};

type AIHistoryEvent =
  | { kind: "project"; snapshot: AIHistorySnapshot }
  | { kind: "projectState"; state: AIHistoryProjectState }
  | { kind: "global"; snapshot: AIGlobalHistorySnapshot }
  | {
      kind: "status";
      scope: "project" | "global";
      projectId?: string | null;
      isLoading: boolean;
      detail: string;
    };

type GlobalHistoryOptions = {
  enabled?: boolean;
};

type AIHistoryRefreshOptions = {
  mode?: "foreground" | "silent";
};

const projectHistoryRequests = new Map<string, Promise<AIHistoryProjectState>>();
const projectStateRequests = new Map<string, Promise<AIHistoryProjectState>>();
const globalHistoryRequests = new Map<string, Promise<AIGlobalHistorySnapshot>>();

export function useAIHistorySnapshot(project?: WorkspaceProject) {
  const [snapshot, setSnapshot] = useState<AIHistorySnapshot>(() => emptyHistorySnapshot(project));
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [detail, setDetail] = useState("idle");
  const [progress, setProgress] = useState<number | null>(null);
  const stateVersionRef = useRef(0);
  const activeProjectIdRef = useRef<string | null>(null);
  const foregroundProjectIdRef = useRef<string | null>(null);
  const activeProjectId = project?.id ?? null;
  if (activeProjectIdRef.current !== activeProjectId) {
    activeProjectIdRef.current = activeProjectId;
    stateVersionRef.current = 0;
    foregroundProjectIdRef.current = null;
  }

  const applyProjectState = useCallback(
    (next: AIHistoryProjectState) => {
      if (!project || next.projectId !== activeProjectIdRef.current) return;
      if (!shouldApplyAIHistoryProjectState(next, stateVersionRef.current)) return;
      stateVersionRef.current = next.version;
      if (!next.isLoading) {
        foregroundProjectIdRef.current = null;
      }
      setSnapshot((current) =>
        next.snapshot ?? (current.projectId === project.id ? current : emptyHistorySnapshot(project)),
      );
      setIsLoading(next.isLoading);
      setError(next.error ?? null);
      setDetail(next.detail);
      setProgress(next.progress);
    },
    [project?.id, project?.name, project?.path],
  );

  const refresh = useCallback(async (options: AIHistoryRefreshOptions = {}) => {
    if (!project || !window.__TAURI_INTERNALS__) {
      setSnapshot(emptyHistorySnapshot(project));
      setIsLoading(false);
      setError(null);
      setDetail("idle");
      setProgress(null);
      stateVersionRef.current = 0;
      foregroundProjectIdRef.current = null;
      return;
    }
    if (options.mode !== "silent") {
      foregroundProjectIdRef.current = project.id;
    }
    setIsLoading(true);
    setError(null);
    setDetail("queued");
    setProgress(0);
    try {
      const requestKey = `${project.id}:${project.path}:${project.name}`;
      let request = projectHistoryRequests.get(requestKey);
      if (!request) {
        request = invoke<AIHistoryProjectState>("ai_history_project_summary", {
          project: {
            id: project.id,
            name: project.name,
            path: project.path,
          },
        }).finally(() => {
          projectHistoryRequests.delete(requestKey);
        });
        projectHistoryRequests.set(requestKey, request);
      }
      const next = await request;
      if (activeProjectIdRef.current !== project.id) return;
      applyProjectState(next);
    } catch (reason) {
      if (activeProjectIdRef.current !== project.id) return;
      console.error("failed to load ai history", reason);
      setError(reason instanceof Error ? reason.message : String(reason));
      setSnapshot(emptyHistorySnapshot(project));
      setIsLoading(false);
      setDetail("failed");
      setProgress(null);
      foregroundProjectIdRef.current = null;
    }
  }, [applyProjectState, project?.id, project?.name, project?.path]);

  const loadState = useCallback(async () => {
    if (!project || !window.__TAURI_INTERNALS__) {
      setSnapshot(emptyHistorySnapshot(project));
      setIsLoading(false);
      setError(null);
      setDetail("idle");
      setProgress(null);
      stateVersionRef.current = 0;
      foregroundProjectIdRef.current = null;
      return;
    }
    try {
      const requestKey = `${project.id}:${project.path}:${project.name}`;
      let request = projectStateRequests.get(requestKey);
      if (!request) {
        request = invoke<AIHistoryProjectState>("ai_history_project_state", {
          project: {
            id: project.id,
            name: project.name,
            path: project.path,
          },
        }).finally(() => {
          projectStateRequests.delete(requestKey);
        });
        projectStateRequests.set(requestKey, request);
      }
      const next = await request;
      if (activeProjectIdRef.current !== project.id) return;
      applyProjectState(next);
    } catch (reason) {
      if (activeProjectIdRef.current !== project.id) return;
      console.error("failed to load ai history state", reason);
      setError(reason instanceof Error ? reason.message : String(reason));
      setSnapshot(emptyHistorySnapshot(project));
      setIsLoading(false);
      setDetail("failed");
      setProgress(null);
      foregroundProjectIdRef.current = null;
    }
  }, [applyProjectState, project?.id, project?.name, project?.path]);

  useEffect(() => {
    if (!project || !window.__TAURI_INTERNALS__) {
      void refresh();
      return;
    }
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<AIHistoryEvent>("ai-history:event", (event) => {
      if (event.payload.kind === "projectState") {
        if (event.payload.state.projectId === activeProjectIdRef.current) {
          applyProjectState(event.payload.state);
        }
        return;
      }
      if (event.payload.kind === "status") {
        if (event.payload.scope === "project" && event.payload.projectId === project.id) {
          if (!event.payload.isLoading) {
            foregroundProjectIdRef.current = null;
          }
          setIsLoading(event.payload.isLoading);
          setDetail(event.payload.detail);
          if (!event.payload.isLoading) setProgress(null);
        }
        return;
      }
      if (event.payload.kind !== "project") return;
      if (event.payload.snapshot.projectId !== project.id) return;
      foregroundProjectIdRef.current = null;
      setSnapshot(event.payload.snapshot);
      setError(null);
      setIsLoading(false);
      setDetail("completed");
      setProgress(1);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
      void loadState();
    }).catch((reason) => {
      console.error("failed to listen ai history", reason);
      if (!disposed) void loadState();
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [applyProjectState, loadState, project?.id]);

  return useMemo(
    () => ({
      snapshot,
      isLoading,
      error,
      detail,
      progress,
      isForegroundLoading: isLoading && foregroundProjectIdRef.current === activeProjectId,
      refresh,
    }),
    [activeProjectId, detail, error, isLoading, progress, refresh, snapshot],
  );
}

export function useAIGlobalHistorySnapshot(
  projects: WorkspaceProject[],
  options: GlobalHistoryOptions = {},
) {
  const [snapshot, setSnapshot] = useState<AIGlobalHistorySnapshot>(emptyGlobalHistorySnapshot);
  const [isLoading, setIsLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const enabled = options.enabled !== false;
  const projectKey = projects
    .map((project) => `${project.id}:${project.path}:${project.name}`)
    .join("|");

  const refresh = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__ || !shouldLoadGlobalHistory(enabled, projects.length)) {
      setIsLoading(false);
      setSnapshot(emptyGlobalHistorySnapshot);
      setError(null);
      return;
    }
    setError(null);
    try {
      let request = globalHistoryRequests.get(projectKey);
      if (!request) {
        request = invoke<AIGlobalHistorySnapshot | null>("ai_history_global_state", {
          projects: projects.map((project) => ({
            id: project.id,
            name: project.name,
            path: project.path,
          })),
        }).then((next) => next ?? emptyGlobalHistorySnapshot).finally(() => {
          globalHistoryRequests.delete(projectKey);
        });
        globalHistoryRequests.set(projectKey, request);
      }
      const next = await request;
      setSnapshot(next);
    } catch (reason) {
      console.error("failed to load global ai history", reason);
      setError(reason instanceof Error ? reason.message : String(reason));
      setSnapshot(emptyGlobalHistorySnapshot);
    }
  }, [enabled, projectKey]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || !enabled) return;
    let disposed = false;
    let unlisten: UnlistenFn | undefined;
    void listen<AIHistoryEvent>("ai-history:event", (event) => {
      if (event.payload.kind === "status") {
        if (event.payload.scope === "global") {
          setIsLoading(event.payload.isLoading);
        }
        return;
      }
      if (event.payload.kind !== "global") return;
      setSnapshot(event.payload.snapshot);
      setError(null);
      setIsLoading(false);
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
        return;
      }
      unlisten = nextUnlisten;
    });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [enabled, projectKey]);

  return useMemo(
    () => ({
      snapshot,
      isLoading,
      error,
      refresh,
    }),
    [error, isLoading, refresh, snapshot],
  );
}

export function shouldLoadGlobalHistory(enabled: boolean, projectCount: number) {
  return enabled && projectCount > 0;
}

export function shouldApplyAIHistoryProjectState(
  next: Pick<AIHistoryProjectState, "version">,
  currentVersion: number,
) {
  return next.version >= currentVersion;
}

function emptyHistorySnapshot(project?: WorkspaceProject): AIHistorySnapshot {
  const projectId = project?.id ?? "";
  const projectName = project?.name ?? "Workspace";
  return {
    projectId,
    projectName,
    projectSummary: {
      projectId,
      projectName,
      currentSessionTokens: 0,
      currentSessionCachedInputTokens: 0,
      projectTotalTokens: 0,
      projectCachedInputTokens: 0,
      todayTotalTokens: 0,
      todayCachedInputTokens: 0,
      currentTool: null,
      currentModel: null,
      currentSessionUpdatedAt: null,
    },
    sessions: [],
    heatmap: [],
    todayTimeBuckets: [],
    toolBreakdown: [],
    modelBreakdown: [],
    indexedAt: 0,
  };
}

const emptyGlobalHistorySnapshot: AIGlobalHistorySnapshot = {
  totalTokens: 0,
  cachedInputTokens: 0,
  todayTotalTokens: 0,
  todayCachedInputTokens: 0,
  sessions: [],
  projectCount: 0,
  indexedAt: 0,
};
