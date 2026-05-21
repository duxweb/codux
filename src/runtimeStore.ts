import { create } from "zustand";
import type { AIGlobalHistorySnapshot, AIHistoryProjectState, AIHistorySessionSummary } from "./ai/history";
import type { PetSnapshot } from "./ai/petState";
import type { AIRuntimeStateSnapshot } from "./ai/types";
import type { GitReviewSnapshot } from "./git/review";
import type { GitStatusSnapshot } from "./git/status";
import type { ProjectListSnapshot, RemoteStatus } from "./types";
import type { WorktreeSnapshot } from "./worktree/snapshot";

export type GitStatusCacheEntry = {
  snapshot: GitStatusSnapshot;
  error: string | null;
  updatedAt: number;
};

export type GitReviewCacheEntry = {
  snapshot: GitReviewSnapshot;
  error: string | null;
  updatedAt: number;
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
    set((state) => ({
      gitStatusByPath: {
        ...state.gitStatusByPath,
        [pathKey]: entry,
      },
      gitErrorByPath: {
        ...state.gitErrorByPath,
        [pathKey]: entry.error,
      },
    })),
  setGitLoading: (pathKey, isLoading) =>
    set((state) => ({
      gitLoadingByPath: {
        ...state.gitLoadingByPath,
        [pathKey]: isLoading,
      },
    })),
  setGitReview: (key, entry) =>
    set((state) => ({
      gitReviewByKey: {
        ...state.gitReviewByKey,
        [key]: entry,
      },
    })),
  setGitError: (pathKey, error) =>
    set((state) => ({
      gitErrorByPath: {
        ...state.gitErrorByPath,
        [pathKey]: error,
      },
    })),
  setWorktreeSnapshot: (key, entry) =>
    set((state) => ({
      worktreeSnapshotByKey: {
        ...state.worktreeSnapshotByKey,
        [key]: entry,
      },
      worktreeErrorByKey: {
        ...state.worktreeErrorByKey,
        [key]: entry.error,
      },
    })),
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
    set((state) => ({
      aiProjectStateByKey: {
        ...state.aiProjectStateByKey,
        [key]: stripAIProjectStateSessions(projectState),
      },
    })),
  setAIProjectSessions: (key, entry) =>
    set((state) => ({
      aiProjectSessionsByKey: {
        ...state.aiProjectSessionsByKey,
        [key]: entry,
      },
    })),
  setAIRuntimeSnapshot: (snapshot) => set({ aiRuntimeSnapshot: snapshot }),
  setPetSnapshot: (snapshot) => set({ petSnapshot: snapshot }),
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
  setAIGlobalHistory: (snapshot) => set({ aiGlobalHistory: stripAIGlobalHistorySessions(snapshot) }),
  setAIGlobalStatus: (status) =>
    set((state) => ({
      aiGlobalStatus: {
        ...state.aiGlobalStatus,
        ...status,
      },
    })),
}));

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
