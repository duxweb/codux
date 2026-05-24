import { create } from "zustand";
import type { AIGlobalHistorySnapshot, AIHistoryProjectState, AIHistorySessionSummary } from "./ai/history";
import type { PetSnapshot } from "./ai/petState";
import type { MemoryExtractionStatusSnapshot, MemoryManagerSnapshot } from "./ai/memory";
import type { AIRuntimeStateSnapshot } from "./ai/types";
import type { GitReviewSnapshot } from "./git/review";
import type { GitStatusSnapshot } from "./git/status";
import type { ProjectListSnapshot, RemoteStatus } from "./types";
import type { WorktreeSnapshot } from "./worktree/snapshot";
import { approximateJsonBytes, runtimeTrace } from "./runtimeTrace";

export type GitStatusCacheEntry = {
  snapshot: GitStatusSnapshot;
  error: string | null;
  updatedAt: number;
};

export type GitReviewCacheEntry = {
  snapshot: GitReviewSnapshot;
  error: string | null;
  updatedAt: number;
  signature?: string;
};

export type WorktreeCacheEntry = {
  snapshot: WorktreeSnapshot;
  error: string | null;
  updatedAt: number;
};

export type AIProjectSessionsCacheEntry = {
  sessions: AIHistorySessionSummary[];
  updatedAt: number;
};

type RuntimeState = {
  gitStatusByPath: Record<string, GitStatusCacheEntry>;
  gitReviewByKey: Record<string, GitReviewCacheEntry>;
  worktreeSnapshotByKey: Record<string, WorktreeCacheEntry>;
  aiProjectStateByKey: Record<string, AIHistoryProjectState>;
  aiProjectSessionsByKey: Record<string, AIProjectSessionsCacheEntry>;
  aiGlobalHistory: AIGlobalHistorySnapshot | null;
  aiRuntimeSnapshot: AIRuntimeStateSnapshot | null;
  petSnapshot: PetSnapshot | null;
  memoryExtractionStatus: MemoryExtractionStatusSnapshot | null;
  memoryManagerSnapshot: MemoryManagerSnapshot | null;
  projectListSnapshot: ProjectListSnapshot | null;
  remoteStatus: RemoteStatus | null;
  aiGlobalStatus: {
    isLoading: boolean;
    error: string | null;
  };
  gitLoadingByPath: Record<string, boolean>;
  gitErrorByPath: Record<string, string | null>;
  worktreeLoadingByKey: Record<string, boolean>;
  worktreeErrorByKey: Record<string, string | null>;
  setGitStatus: (pathKey: string, entry: GitStatusCacheEntry) => void;
  setGitReview: (key: string, entry: GitReviewCacheEntry) => void;
  setGitLoading: (pathKey: string, isLoading: boolean) => void;
  setGitError: (pathKey: string, error: string | null) => void;
  setWorktreeSnapshot: (key: string, entry: WorktreeCacheEntry) => void;
  setWorktreeLoading: (key: string, isLoading: boolean) => void;
  setWorktreeError: (key: string, error: string | null) => void;
  setAIProjectState: (key: string, state: AIHistoryProjectState) => void;
  setAIProjectSessions: (key: string, entry: AIProjectSessionsCacheEntry) => void;
  setAIRuntimeSnapshot: (snapshot: AIRuntimeStateSnapshot | null) => void;
  setPetSnapshot: (snapshot: PetSnapshot | null) => void;
  setMemoryExtractionStatus: (snapshot: MemoryExtractionStatusSnapshot | null) => void;
  setMemoryManagerSnapshot: (snapshot: MemoryManagerSnapshot | null) => void;
  setProjectListSnapshot: (snapshot: ProjectListSnapshot | null) => void;
  setRemoteStatus: (status: RemoteStatus | null) => void;
  updateAIProjectStateByProjectId: (
    projectId: string,
    updater: (state: AIHistoryProjectState, key: string) => AIHistoryProjectState,
  ) => void;
  setAIGlobalHistory: (snapshot: AIGlobalHistorySnapshot | null) => void;
  setAIGlobalStatus: (status: { isLoading?: boolean; error?: string | null }) => void;
};

export const useRuntimeStore = create<RuntimeState>((set) => ({
  gitStatusByPath: {},
  gitReviewByKey: {},
  worktreeSnapshotByKey: {},
  aiProjectStateByKey: {},
  aiProjectSessionsByKey: {},
  aiGlobalHistory: null,
  aiRuntimeSnapshot: null,
  petSnapshot: null,
  memoryExtractionStatus: null,
  memoryManagerSnapshot: null,
  projectListSnapshot: null,
  remoteStatus: null,
  aiGlobalStatus: {
    isLoading: false,
    error: null,
  },
  gitLoadingByPath: {},
  gitErrorByPath: {},
  worktreeLoadingByKey: {},
  worktreeErrorByKey: {},
  setGitStatus: (pathKey, entry) =>
    set((state) => {
      const nextGitStatusByPath = {
        ...state.gitStatusByPath,
        [pathKey]: entry,
      };
      traceRuntimeStoreWrite("gitStatus", pathKey, entry, Object.keys(nextGitStatusByPath).length);
      return {
        gitStatusByPath: nextGitStatusByPath,
        gitErrorByPath: {
          ...state.gitErrorByPath,
          [pathKey]: entry.error,
        },
      };
    }),
  setGitLoading: (pathKey, isLoading) =>
    set((state) => ({
      gitLoadingByPath: {
        ...state.gitLoadingByPath,
        [pathKey]: isLoading,
      },
    })),
  setGitReview: (key, entry) =>
    set((state) => {
      const nextGitReviewByKey = {
        ...state.gitReviewByKey,
        [key]: entry,
      };
      traceRuntimeStoreWrite("gitReview", key, entry, Object.keys(nextGitReviewByKey).length);
      return {
        gitReviewByKey: nextGitReviewByKey,
      };
    }),
  setGitError: (pathKey, error) =>
    set((state) => ({
      gitErrorByPath: {
        ...state.gitErrorByPath,
        [pathKey]: error,
      },
    })),
  setWorktreeSnapshot: (key, entry) =>
    set((state) => {
      const nextWorktreeSnapshotByKey = {
        ...state.worktreeSnapshotByKey,
        [key]: entry,
      };
      traceRuntimeStoreWrite("worktreeSnapshot", key, entry, Object.keys(nextWorktreeSnapshotByKey).length);
      return {
        worktreeSnapshotByKey: nextWorktreeSnapshotByKey,
        worktreeErrorByKey: {
          ...state.worktreeErrorByKey,
          [key]: entry.error,
        },
      };
    }),
  setWorktreeLoading: (key, isLoading) =>
    set((state) => ({
      worktreeLoadingByKey: {
        ...state.worktreeLoadingByKey,
        [key]: isLoading,
      },
    })),
  setWorktreeError: (key, error) =>
    set((state) => ({
      worktreeErrorByKey: {
        ...state.worktreeErrorByKey,
        [key]: error,
      },
    })),
  setAIProjectState: (key, projectState) =>
    set((state) => {
      const stripped = stripAIProjectStateSessions(projectState);
      const existing = state.aiProjectStateByKey[key];
      if (existing && isSameAIHistoryProjectState(existing, stripped)) {
        return state;
      }
      const nextAIProjectStateByKey = {
        ...state.aiProjectStateByKey,
        [key]: stripped,
      };
      traceRuntimeStoreWrite("aiProjectState", key, stripped, Object.keys(nextAIProjectStateByKey).length);
      return {
        aiProjectStateByKey: nextAIProjectStateByKey,
      };
    }),
  setAIProjectSessions: (key, entry) =>
    set((state) => {
      const existing = state.aiProjectSessionsByKey[key];
      if (existing && isSameAIProjectSessions(existing, entry)) {
        return state;
      }
      const nextAIProjectSessionsByKey = {
        ...state.aiProjectSessionsByKey,
        [key]: entry,
      };
      traceRuntimeStoreWrite("aiProjectSessions", key, entry, Object.keys(nextAIProjectSessionsByKey).length);
      return {
        aiProjectSessionsByKey: nextAIProjectSessionsByKey,
      };
    }),
  setAIRuntimeSnapshot: (snapshot) => set({ aiRuntimeSnapshot: snapshot }),
  setPetSnapshot: (snapshot) => set({ petSnapshot: snapshot }),
  setMemoryExtractionStatus: (snapshot) => set({ memoryExtractionStatus: snapshot }),
  setMemoryManagerSnapshot: (snapshot) => set({ memoryManagerSnapshot: snapshot }),
  setProjectListSnapshot: (snapshot) => set({ projectListSnapshot: snapshot }),
  setRemoteStatus: (status) => set({ remoteStatus: status }),
  updateAIProjectStateByProjectId: (projectId, updater) =>
    set((state) => {
      const entry = Object.entries(state.aiProjectStateByKey).find(([, value]) => value.projectId === projectId);
      if (!entry) return state;
      const [key, value] = entry;
      return {
        aiProjectStateByKey: {
          ...state.aiProjectStateByKey,
          [key]: updater(value, key),
        },
      };
    }),
  setAIGlobalHistory: (snapshot) => {
    const stripped = stripAIGlobalHistorySessions(snapshot);
    if (useRuntimeStore.getState().aiGlobalHistory && isSameAIGlobalHistory(useRuntimeStore.getState().aiGlobalHistory, stripped)) {
      return;
    }
    traceRuntimeStoreWrite("aiGlobalHistory", "global", stripped, stripped ? 1 : 0);
    set({ aiGlobalHistory: stripped });
  },
  setAIGlobalStatus: (status) =>
    set((state) => ({
      aiGlobalStatus: {
        ...state.aiGlobalStatus,
        ...status,
      },
    })),
}));

export function traceRuntimeStoreWrite(label: string, key: string, value: unknown, entryCount: number) {
  runtimeTrace(
    "frontend-store",
    `${label} key=${shortTraceKey(key)} bytes=${approximateJsonBytes(value)} entries=${entryCount}`,
  );
}

function stripAIProjectStateSessions(state: AIHistoryProjectState): AIHistoryProjectState {
  if (!state.snapshot?.sessions.length) return state;
  return {
    ...state,
    snapshot: {
      ...state.snapshot,
      sessions: [],
    },
  };
}

function stripAIGlobalHistorySessions(snapshot: AIGlobalHistorySnapshot | null): AIGlobalHistorySnapshot | null {
  if (!snapshot?.sessions.length) return snapshot;
  return {
    ...snapshot,
    sessions: [],
  };
}

function isSameAIHistoryProjectState(left: AIHistoryProjectState, right: AIHistoryProjectState) {
  return serializeStable(left) === serializeStable(right);
}

function isSameAIProjectSessions(left: AIProjectSessionsCacheEntry, right: AIProjectSessionsCacheEntry) {
  return serializeStable(left.sessions) === serializeStable(right.sessions);
}

function isSameAIGlobalHistory(
  left: AIGlobalHistorySnapshot | null,
  right: AIGlobalHistorySnapshot | null,
) {
  return serializeStable(left) === serializeStable(right);
}

function shortTraceKey(key: string) {
  if (key.length <= 96) return key;
  return `${key.slice(0, 44)}...${key.slice(-44)}`;
}

function serializeStable(value: unknown) {
  return JSON.stringify(value);
}
