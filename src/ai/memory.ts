import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import { useEffect, useState } from "react";
import { useRuntimeStore } from "../runtimeStore";

export type MemoryExtractionStatus = "idle" | "queued" | "processing" | "failed";

export type MemoryExtractionStatusSnapshot = {
  status: MemoryExtractionStatus;
  pendingCount: number;
  runningCount: number;
  checkedCount: number;
  enqueuedCount: number;
  lastError?: string | null;
  updatedAt: number;
};

export type MemoryScope = "user" | "project";
export type MemoryManagerTab = "summary" | "active" | "history";
export type MemoryTier = "core" | "working" | "archive";
export type MemoryKind = "preference" | "convention" | "decision" | "fact" | "bug_lesson";
export type MemoryEntryStatus = "active" | "merged" | "archived";

export type MemoryEntry = {
  id: string;
  scope: "user" | "project";
  projectId?: string | null;
  toolId?: string | null;
  tier: MemoryTier;
  kind: MemoryKind;
  content: string;
  rationale?: string | null;
  sourceTool?: string | null;
  sourceSessionId?: string | null;
  sourceFingerprint?: string | null;
  normalizedHash: string;
  supersededBy?: string | null;
  status: MemoryEntryStatus;
  mergedSummaryId?: string | null;
  mergedAt?: number | null;
  archivedAt?: number | null;
  accessCount: number;
  lastAccessedAt?: number | null;
  createdAt: number;
  updatedAt: number;
};

export type MemorySummary = {
  id: string;
  scope: "user" | "project";
  projectId?: string | null;
  toolId?: string | null;
  content: string;
  version: number;
  sourceEntryIds: string[];
  tokenEstimate: number;
  createdAt: number;
  updatedAt: number;
};

export type MemoryScopeOverview = {
  activeEntryCount: number;
  archivedEntryCount: number;
  mergedEntryCount: number;
  summaryCount: number;
  updatedAt?: number | null;
};

export type MemoryManagerTargetRow = {
  id: string;
  scope: MemoryScope;
  projectId?: string | null;
  title: string;
  subtitle: string;
  count: number;
  updatedAt?: number | null;
  isOpenProject: boolean;
};

export type MemoryManagerSnapshot = {
  targetRows: MemoryManagerTargetRow[];
  selectedTargetTitle: string;
  currentOverview: MemoryScopeOverview;
  entries: MemoryEntry[];
  summaries: MemorySummary[];
  extraction: MemoryExtractionStatusSnapshot;
};

const idleSnapshot: MemoryExtractionStatusSnapshot = {
  status: "idle",
  pendingCount: 0,
  runningCount: 0,
  checkedCount: 0,
  enqueuedCount: 0,
  lastError: null,
  updatedAt: 0,
};

export async function readMemoryExtractionStatus() {
  if (!window.__TAURI_INTERNALS__) return idleSnapshot;
  return invoke<MemoryExtractionStatusSnapshot>("memory_extraction_status");
}

export async function cancelMemoryExtraction() {
  if (!window.__TAURI_INTERNALS__) return idleSnapshot;
  const snapshot = await invoke<MemoryExtractionStatusSnapshot>("memory_extraction_cancel");
  useRuntimeStore.getState().setMemoryExtractionStatus(snapshot);
  return snapshot;
}

export async function readMemoryManagerSnapshot(request: {
  scope: MemoryScope;
  projectId?: string | null;
  tab: MemoryManagerTab;
  limit?: number;
}) {
  return invoke<MemoryManagerSnapshot>("memory_manager_snapshot", { request });
}

export async function archiveMemoryEntry(entryId: string) {
  await invoke("memory_archive_entry", { entryId });
}

export async function deleteMemoryEntry(entryId: string) {
  await invoke("memory_delete_entry", { entryId });
}

export async function deleteMemorySummary(summaryId: string) {
  await invoke("memory_delete_summary", { summaryId });
}

export async function deleteProjectMemory(projectId: string) {
  await invoke("memory_delete_project", { projectId });
}

export async function migrateProjectMemory(request: {
  fromProjectId: string;
  toProjectId: string;
  overwrite?: boolean;
}) {
  await invoke("memory_migrate_project", {
    request: {
      fromProjectId: request.fromProjectId,
      toProjectId: request.toProjectId,
      overwrite: Boolean(request.overwrite),
    },
  });
}

export async function updateMemorySummary(request: { summaryId: string; content: string; maxVersions?: number }) {
  return invoke<MemorySummary>("memory_update_summary", { request });
}

export async function indexMemoryNow() {
  if (!window.__TAURI_INTERNALS__) return idleSnapshot;
  const snapshot = await invoke<MemoryExtractionStatusSnapshot>("memory_index_now");
  useRuntimeStore.getState().setMemoryExtractionStatus(snapshot);
  return snapshot;
}

export function useMemoryExtractionStatus(refreshMs = 5000) {
  const storeSnapshot = useRuntimeStore((state) => state.memoryExtractionStatus);
  const [snapshot, setSnapshot] = useState<MemoryExtractionStatusSnapshot>(storeSnapshot ?? idleSnapshot);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) {
      setSnapshot(idleSnapshot);
      useRuntimeStore.getState().setMemoryExtractionStatus(idleSnapshot);
      return;
    }
    let cancelled = false;
    let timer: number | undefined;

    const load = () => {
      void readMemoryExtractionStatus()
        .then((next) => {
          if (cancelled) return;
          useRuntimeStore.getState().setMemoryExtractionStatus(next);
          setSnapshot((current) => (memoryStatusEquals(current, next) ? current : next));
          const isActive = next.status === "queued" || next.status === "processing";
          const nextRefreshMs = isActive ? refreshMs : Math.max(refreshMs * 6, 30_000);
          timer = window.setTimeout(load, nextRefreshMs);
        })
        .catch((error) => {
          console.error("failed to load memory status", error);
          if (!cancelled) timer = window.setTimeout(load, Math.max(refreshMs * 6, 30_000));
        });
    };

    load();
    return () => {
      cancelled = true;
      if (timer) window.clearTimeout(timer);
    };
  }, [refreshMs]);

  return storeSnapshot ?? snapshot;
}

let memoryEventSubscriptionPromise: Promise<UnlistenFn[]> | null = null;

export function ensureMemoryEventSubscription() {
  if (!window.__TAURI_INTERNALS__ || memoryEventSubscriptionPromise) return;
  memoryEventSubscriptionPromise = Promise.all([
    listen<MemoryExtractionStatusSnapshot>("memory:status", (event) => {
      useRuntimeStore.getState().setMemoryExtractionStatus(event.payload);
    }),
    listen<MemoryManagerSnapshot>("memory:manager", (event) => {
      useRuntimeStore.getState().setMemoryManagerSnapshot(event.payload);
    }),
  ]).catch((error) => {
    memoryEventSubscriptionPromise = null;
    console.error("failed to subscribe memory events", error);
    return [];
  });
}

function memoryStatusEquals(left: MemoryExtractionStatusSnapshot, right: MemoryExtractionStatusSnapshot) {
  return (
    left.status === right.status &&
    left.pendingCount === right.pendingCount &&
    left.runningCount === right.runningCount &&
    left.checkedCount === right.checkedCount &&
    left.enqueuedCount === right.enqueuedCount &&
    left.lastError === right.lastError &&
    left.updatedAt === right.updatedAt
  );
}
