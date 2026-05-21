import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useRef } from "react";
import { useRuntimeStore } from "../runtimeStore";
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

let aiHistoryCacheListenerPromise: Promise<UnlistenFn> | null = null;

function projectHistoryKey(project: WorkspaceProject) {
  return project.path;
}

function projectStateKey(state: Pick<AIHistoryProjectState, "projectPath">) {
  return state.projectPath;
}

function cacheAIHistoryEvent(event: AIHistoryEvent) {
  const store = useRuntimeStore.getState();
  if (event.kind === "status") {
    if (event.scope === "global") {
      store.setAIGlobalStatus({ isLoading: event.isLoading });
      return;
    }
    if (!event.projectId) return;
    store.updateAIProjectStateByProjectId(event.projectId, (previous) => ({
      ...previous,
      isLoading: event.isLoading,
      queued: event.isLoading,
      progress: event.isLoading ? previous.progress : null,
      detail: event.detail,
      error: null,
    }));
    return;
  }
  if (event.kind === "projectState") {
    store.setAIProjectState(projectStateKey(event.state), event.state);
    return;
  }
  if (event.kind === "project") {
    store.updateAIProjectStateByProjectId(event.snapshot.projectId, (previous) => ({
      ...previous,
      snapshot: event.snapshot,
      isLoading: false,
      queued: false,
      progress: 1,
      detail: "completed",
      error: null,
      version: previous.version + 1,
    }));
    return;
  }
  if (event.kind === "global") {
    store.setAIGlobalHistory(event.snapshot);
  }
}

export function ensureAIHistoryEventCacheSubscription() {
  if (!window.__TAURI_INTERNALS__ || aiHistoryCacheListenerPromise) return;
  aiHistoryCacheListenerPromise = listen<AIHistoryEvent>("ai-history:event", (event) => {
    cacheAIHistoryEvent(event.payload);
  }).catch((error) => {
    aiHistoryCacheListenerPromise = null;
    console.error("failed to cache ai history events", error);
    return () => {};
  });
}

export function useAIHistorySnapshot(project?: WorkspaceProject) {
  const projectCacheKey = project ? projectHistoryKey(project) : "";
  const cachedState = useRuntimeStore((state) =>
    projectCacheKey ? state.aiProjectStateByKey[projectCacheKey] : undefined,
  );
  const snapshot = cachedState?.snapshot ?? emptyHistorySnapshot(project);
  const isLoading = cachedState?.isLoading ?? false;
  const error = cachedState?.error ?? null;
  const detail = cachedState?.detail ?? "idle";
  const progress = cachedState?.progress ?? null;
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
      useRuntimeStore.getState().setAIProjectState(projectHistoryKey(project), next);
    },
    [project],
  );

  const refresh = useCallback(
    async (options: AIHistoryRefreshOptions = {}) => {
      if (!project || !window.__TAURI_INTERNALS__) {
        stateVersionRef.current = 0;
        foregroundProjectIdRef.current = null;
        return;
      }
      if (options.mode !== "silent") {
        foregroundProjectIdRef.current = project.id;
      }
      useRuntimeStore.getState().setAIProjectState(projectHistoryKey(project), {
        projectId: project.id,
        projectName: project.name,
        projectPath: project.path,
        snapshot,
        isLoading: true,
        queued: true,
        progress: 0,
        detail: "queued",
        error: null,
        version: stateVersionRef.current,
      });
      try {
        await invoke("ai_history_refresh_project", {
          project: {
            id: project.id,
            name: project.name,
            path: project.path,
          },
        });
      } catch (reason) {
        if (activeProjectIdRef.current !== project.id) return;
        console.error("failed to load ai history", reason);
        useRuntimeStore.getState().setAIProjectState(projectHistoryKey(project), {
          projectId: project.id,
          projectName: project.name,
          projectPath: project.path,
          snapshot: emptyHistorySnapshot(project),
          isLoading: false,
          queued: false,
          progress: null,
          detail: "failed",
          error: reason instanceof Error ? reason.message : String(reason),
          version: stateVersionRef.current + 1,
        });
        foregroundProjectIdRef.current = null;
      }
    },
    [project, snapshot],
  );

  const loadState = useCallback(async () => {
    if (!project || !window.__TAURI_INTERNALS__) {
      stateVersionRef.current = 0;
      foregroundProjectIdRef.current = null;
      return;
    }
    const cached = useRuntimeStore.getState().aiProjectStateByKey[projectHistoryKey(project)];
    if (cached) applyProjectState(cached);
  }, [applyProjectState, project]);

  useEffect(() => {
    if (!project || !window.__TAURI_INTERNALS__) {
      return;
    }
    void loadState();
  }, [loadState, project]);

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

export function useAIGlobalHistorySnapshot(projects: WorkspaceProject[], options: GlobalHistoryOptions = {}) {
  const cachedGlobalHistory = useRuntimeStore((state) => state.aiGlobalHistory);
  const globalStatus = useRuntimeStore((state) => state.aiGlobalStatus);
  const snapshot = cachedGlobalHistory ?? emptyGlobalHistorySnapshot;
  const enabled = options.enabled !== false;
  const refresh = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__ || !shouldLoadGlobalHistory(enabled, projects.length)) {
      useRuntimeStore.getState().setAIGlobalHistory(null);
      useRuntimeStore.getState().setAIGlobalStatus({ isLoading: false, error: null });
      return;
    }
    useRuntimeStore.getState().setAIGlobalStatus({ isLoading: true, error: null });
    try {
      await invoke("ai_history_refresh_global", {
        projects: projects.map((project) => ({
          id: project.id,
          name: project.name,
          path: project.path,
        })),
      });
    } catch (reason) {
      console.error("failed to load global ai history", reason);
      useRuntimeStore.getState().setAIGlobalStatus({
        isLoading: false,
        error: reason instanceof Error ? reason.message : String(reason),
      });
    }
  }, [enabled, projects]);

  return useMemo(
    () => ({
      snapshot,
      isLoading: globalStatus.isLoading,
      error: globalStatus.error,
      refresh,
    }),
    [globalStatus.error, globalStatus.isLoading, refresh, snapshot],
  );
}

export function shouldLoadGlobalHistory(enabled: boolean, projectCount: number) {
  return enabled && projectCount > 0;
}

export function shouldApplyAIHistoryProjectState(next: Pick<AIHistoryProjectState, "version">, currentVersion: number) {
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
