import type { AISessionSnapshot } from "./types";
import { tm } from "../i18n";

export type AIIndexingIndicator = "check" | "progress" | "spinner";
export type AIIndexingTone = "info" | "warning" | "danger";
export type AIIndexingStatusKey =
  | "failed"
  | "queued"
  | "fullIndexing"
  | "manualRefreshing"
  | "silentIndexing"
  | "completed";

export type AIIndexingPresentation = {
  statusKey: AIIndexingStatusKey;
  tone: AIIndexingTone;
  text: string;
  indicator: AIIndexingIndicator;
  progressValue: number;
  showRefreshAction: boolean;
};

export type AIIndexingPresentationInput = {
  error: string | null;
  isLoading: boolean;
  isForegroundIndexing: boolean;
  statusDetail: string;
  progress: number | null;
  indexedAt: number;
};

const AI_INDEXING_STATUS_TEXT: Record<AIIndexingStatusKey, { key: string; fallback: string }> = {
  failed: { key: "ai.indexing.status.failed", fallback: "Index failed" },
  queued: { key: "ai.indexing.status.queued", fallback: "Queued for indexing" },
  fullIndexing: { key: "ai.indexing.status.full_indexing", fallback: "Indexing all usage" },
  manualRefreshing: { key: "ai.indexing.status.manual_refreshing", fallback: "Refreshing stats" },
  silentIndexing: { key: "ai.indexing.status.silent_indexing", fallback: "Indexing in background" },
  completed: { key: "ai.indexing.status.completed", fallback: "Index complete" },
};

export function aiIndexingPresentation(input: AIIndexingPresentationInput): AIIndexingPresentation {
  const statusKey = classifyAIIndexingStatus(input);
  const progressValue = progressPercentage(input.progress);

  return {
    statusKey,
    tone: indexingTone(statusKey),
    text: indexingStatusText(statusKey),
    indicator: indexingIndicator(statusKey, input.isForegroundIndexing, input.progress),
    progressValue,
    showRefreshAction: canRefreshIndexing(statusKey),
  };
}

function indexingStatusText(statusKey: AIIndexingStatusKey) {
  const item = AI_INDEXING_STATUS_TEXT[statusKey];
  return tm(item.key, item.fallback);
}

export function classifyAIIndexingStatus({
  error,
  isLoading,
  isForegroundIndexing,
  statusDetail,
  indexedAt,
}: AIIndexingPresentationInput): AIIndexingStatusKey {
  const hasIndexedSnapshot = indexedAt > 0;

  switch (true) {
    case Boolean(error):
      return "failed";
    case isLoading && statusDetail === "queued":
      return "queued";
    case isLoading && !hasIndexedSnapshot:
      return "fullIndexing";
    case isLoading && isForegroundIndexing:
      return "manualRefreshing";
    case isLoading && hasIndexedSnapshot:
      return "silentIndexing";
    default:
      return "completed";
  }
}

function indexingTone(statusKey: AIIndexingStatusKey): AIIndexingTone {
  switch (statusKey) {
    case "failed":
      return "danger";
    case "fullIndexing":
      return "warning";
    case "queued":
    case "manualRefreshing":
    case "silentIndexing":
    case "completed":
      return "info";
  }
}

function indexingIndicator(
  statusKey: AIIndexingStatusKey,
  isForegroundIndexing: boolean,
  progress: number | null,
): AIIndexingIndicator {
  switch (statusKey) {
    case "queued":
      return isForegroundIndexing && progress != null ? "progress" : "spinner";
    case "fullIndexing":
    case "manualRefreshing":
      return progress != null ? "progress" : "spinner";
    case "silentIndexing":
      return "spinner";
    case "failed":
    case "completed":
      return "check";
  }
}

function canRefreshIndexing(statusKey: AIIndexingStatusKey): boolean {
  switch (statusKey) {
    case "failed":
    case "completed":
      return true;
    case "queued":
    case "fullIndexing":
    case "manualRefreshing":
    case "silentIndexing":
      return false;
  }
}

function progressPercentage(progress: number | null): number {
  return Math.max(0, Math.min(100, Math.round((progress ?? 0) * 100)));
}

export function liveSessionTotalTokens(session: AISessionSnapshot) {
  return Math.max(0, session.totalTokens);
}
