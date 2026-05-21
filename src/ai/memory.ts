import { invoke } from "@tauri-apps/api/core";
import { useEffect, useState } from "react";

export type MemoryExtractionStatus = "idle" | "queued" | "processing" | "failed";

export type MemoryExtractionStatusSnapshot = {
  status: MemoryExtractionStatus;
  pendingCount: number;
  runningCount: number;
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
  lastError: null,
  updatedAt: 0,
};

export async function readMemoryExtractionStatus() {
  if (!window.__TAURI_INTERNALS__) return idleSnapshot;
  return invoke<MemoryExtractionStatusSnapshot>("memory_extraction_status");
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

export async function updateMemorySummary(request: { summaryId: string; content: string; maxVersions?: number }) {
  return invoke<MemorySummary>("memory_update_summary", { request });
}

export async function indexMemoryNow() {
  await invoke("memory_index_now");
}

export function useMemoryExtractionStatus(refreshMs = 5000) {
  const [snapshot, setSnapshot] = useState<MemoryExtractionStatusSnapshot>(idleSnapshot);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) {
      setSnapshot(idleSnapshot);
      return;
    }
    let cancelled = false;
    let timer: number | undefined;

    const load = () => {
      void readMemoryExtractionStatus()
        .then((next) => {
          if (cancelled) return;
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

  return snapshot;
}

function memoryStatusEquals(left: MemoryExtractionStatusSnapshot, right: MemoryExtractionStatusSnapshot) {
  return (
    left.status === right.status &&
    left.pendingCount === right.pendingCount &&
    left.runningCount === right.runningCount &&
    left.lastError === right.lastError &&
    left.updatedAt === right.updatedAt
  );
}
