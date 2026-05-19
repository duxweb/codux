import { invoke } from "@tauri-apps/api/core";
import { formatI18n, tm } from "./i18n";

export type BottomTabLayout = {
  id: string;
  label: string;
  terminalId: string;
};

export type TopPaneLayout = {
  id: string;
  title: string;
  terminalId: string;
  detached?: boolean;
};

export type TerminalLayoutState = {
  tabs: BottomTabLayout[];
  activeTabId: string;
  topPanes: TopPaneLayout[];
  topRatios: number[];
  bottomRatio: number;
  activeTerminalId: string;
  activeSlotId: string;
};

export type TerminalLayoutSnapshot = Omit<TerminalLayoutState, "activeTerminalId">;

export type EnsureTerminalForLayout = (slot: string, title: string) => { id: string };

export const terminalLayoutStore = new Map<string, TerminalLayoutSnapshot>();

type TerminalLayoutSaveQueue = {
  flushing: boolean;
  pending?: TerminalLayoutSnapshot;
};

const terminalLayoutSaveQueues = new Map<string, TerminalLayoutSaveQueue>();

export async function loadTerminalLayoutSnapshot(projectId: string) {
  const cached = terminalLayoutStore.get(projectId);
  if (!canUseTauri() || !projectId) {
    return cached;
  }

  const snapshot = await invoke<TerminalLayoutSnapshot | null>("terminal_layout_get", {
    projectId,
  });
  if (snapshot) {
    terminalLayoutStore.set(projectId, snapshot);
  }
  return snapshot ?? cached;
}

export function rememberTerminalLayoutSnapshot(
  projectId: string,
  snapshot: TerminalLayoutSnapshot,
) {
  terminalLayoutStore.set(projectId, snapshot);
  if (!canUseTauri() || !projectId) {
    return;
  }
  enqueueTerminalLayoutSave(projectId, persistedTerminalLayoutSnapshot(snapshot));
}

export function createDefaultTerminalLayout(ensureTerminal: EnsureTerminalForLayout) {
  const bottomTitle = formatI18n(tm("workspace.tab_format", "Tab %@"), 1);
  const topTitle = formatI18n(tm("workspace.split_format", "Split %@"), 1);
  const tabs: BottomTabLayout[] = [
    {
      id: "bottom-1",
      label: bottomTitle,
      terminalId: ensureTerminal("bottom-1", bottomTitle).id,
    },
  ];
  const topPanes: TopPaneLayout[] = [
    {
      id: "top-1",
      title: topTitle,
      terminalId: ensureTerminal("top-1", topTitle).id,
    },
  ];
  const activeTerminalId = topPanes[0]?.terminalId ?? tabs[0]?.terminalId ?? "";

  return {
    tabs,
    activeTabId: tabs[0]?.id ?? "",
    topPanes,
    topRatios: equalRatios(topPanes.length),
    bottomRatio: 0.32,
    activeTerminalId,
    activeSlotId: resolveActiveSlotId(topPanes, tabs, activeTerminalId),
  };
}

export function restoreTerminalLayout(
  cached: TerminalLayoutSnapshot | undefined,
  ensureTerminal: EnsureTerminalForLayout,
) {
  if (!cached) {
    return createDefaultTerminalLayout(ensureTerminal);
  }

  const tabs = sortBySlotId(cached.tabs).map((tab) => ({
    ...tab,
    terminalId: ensureTerminal(tab.id, tab.label).id,
  }));
  const topEntries = sortPaneRatioEntries(cached.topPanes, cached.topRatios);
  const topPanes = topEntries.map(({ pane }) => ({
    ...pane,
    terminalId: ensureTerminal(pane.id, pane.title).id,
  }));
  const topRatios = topEntries.map(({ ratio }) => ratio);
  const activeTerminalId =
    topPanes.find((pane) => pane.id === cached.activeSlotId && !pane.detached)?.terminalId ??
    tabs.find((tab) => tab.id === cached.activeSlotId)?.terminalId ??
    topPanes.find((pane) => !pane.detached)?.terminalId ??
    tabs[0]?.terminalId ??
    "";

  return {
    tabs,
    topPanes,
    topRatios: normalizeRatios(topRatios, topPanes.length),
    bottomRatio: cached.bottomRatio,
    activeTabId: tabs.some((tab) => tab.id === cached.activeTabId)
      ? cached.activeTabId
      : tabs[0]?.id ?? "",
    activeTerminalId,
    activeSlotId: resolveActiveSlotId(topPanes, tabs, activeTerminalId),
  };
}

export function resolvePrimaryTerminalId(layout: {
  topPanes: TopPaneLayout[];
  tabs: BottomTabLayout[];
  activeTabId?: string;
}) {
  return (
    layout.topPanes.find((pane) => !pane.detached)?.terminalId ??
    layout.tabs.find((tab) => tab.id === layout.activeTabId)?.terminalId ??
    layout.tabs[0]?.terminalId ??
    ""
  );
}

export function resolveVisibleTerminalId(
  layout: {
    topPanes: TopPaneLayout[];
    tabs: BottomTabLayout[];
    activeTabId?: string;
  },
  preferredTerminalId?: string,
) {
  if (preferredTerminalId && isVisibleTerminalId(layout, preferredTerminalId)) {
    return preferredTerminalId;
  }
  return resolvePrimaryTerminalId(layout);
}

export function countTerminalSlots(layout: {
  topPanes: TopPaneLayout[];
  tabs: BottomTabLayout[];
}) {
  return layout.topPanes.length + layout.tabs.length;
}

export function countTopSplits(layout: { topPanes: TopPaneLayout[] }) {
  return layout.topPanes.length;
}

export function snapshotTerminalLayout(layout: TerminalLayoutState): TerminalLayoutSnapshot {
  const topEntries = sortPaneRatioEntries(layout.topPanes, layout.topRatios);
  const topPanes = topEntries.map(({ pane }) => pane);
  const tabs = sortBySlotId(layout.tabs);
  return {
    tabs,
    activeTabId: layout.activeTabId,
    topPanes,
    topRatios: topEntries.map(({ ratio }) => ratio),
    bottomRatio: layout.bottomRatio,
    activeSlotId: resolveActiveSlotId(topPanes, tabs, layout.activeTerminalId),
  };
}

export function persistedTerminalLayoutSnapshot(
  snapshot: TerminalLayoutSnapshot,
): TerminalLayoutSnapshot {
  return {
    ...snapshot,
    topPanes: snapshot.topPanes.map(({ detached: _detached, ...pane }) => pane),
  };
}

export function resolveActiveSlotId(
  topPanes: TopPaneLayout[],
  tabs: BottomTabLayout[],
  terminalId: string,
) {
  return (
    topPanes.find((pane) => pane.terminalId === terminalId)?.id ??
    tabs.find((tab) => tab.terminalId === terminalId)?.id ??
    topPanes[0]?.id ??
    tabs[0]?.id ??
    ""
  );
}

export function equalRatios(count: number) {
  const safeCount = Math.max(1, count);
  return Array.from({ length: safeCount }, () => 1 / safeCount);
}

export function normalizeRatios(ratios: number[], count: number) {
  const safeCount = Math.max(1, count);
  const next = ratios.slice(0, safeCount);
  while (next.length < safeCount) {
    next.push(1 / safeCount);
  }
  const total = next.reduce((sum, value) => sum + Math.max(0, value), 0);
  if (total <= 0) {
    return equalRatios(safeCount);
  }
  return next.map((value) => Math.max(0, value) / total);
}

function isVisibleTerminalId(
  layout: {
    topPanes: TopPaneLayout[];
    tabs: BottomTabLayout[];
    activeTabId?: string;
  },
  terminalId: string,
) {
  return (
    layout.topPanes.some((pane) => !pane.detached && pane.terminalId === terminalId) ||
    layout.tabs.some((tab) => tab.id === layout.activeTabId && tab.terminalId === terminalId)
  );
}

function sortBySlotId<T extends { id: string }>(items: T[]) {
  return [...items].sort((left, right) => compareSlotId(left.id, right.id));
}

function sortPaneRatioEntries(panes: TopPaneLayout[], ratios: number[]) {
  return panes
    .map((pane, index) => ({ pane, ratio: ratios[index] ?? 0 }))
    .sort((left, right) => compareSlotId(left.pane.id, right.pane.id));
}

function compareSlotId(left: string, right: string) {
  const leftParts = parseSlotId(left);
  const rightParts = parseSlotId(right);
  if (leftParts.prefix !== rightParts.prefix) {
    return leftParts.prefix.localeCompare(rightParts.prefix);
  }
  return leftParts.index - rightParts.index;
}

function parseSlotId(id: string) {
  const match = /^([a-z-]+)-(\d+)$/.exec(id);
  if (!match) {
    return { prefix: id, index: Number.MAX_SAFE_INTEGER };
  }
  return {
    prefix: match[1],
    index: Number(match[2]),
  };
}

function enqueueTerminalLayoutSave(projectId: string, snapshot: TerminalLayoutSnapshot) {
  const queue = terminalLayoutSaveQueues.get(projectId) ?? { flushing: false };
  queue.pending = snapshot;
  terminalLayoutSaveQueues.set(projectId, queue);
  if (queue.flushing) {
    return;
  }
  queue.flushing = true;
  queueMicrotask(() => {
    void flushTerminalLayoutSave(projectId, queue);
  });
}

async function flushTerminalLayoutSave(projectId: string, queue: TerminalLayoutSaveQueue) {
  while (queue.pending) {
    const snapshot = queue.pending;
    queue.pending = undefined;
    try {
      const saved = await invoke<TerminalLayoutSnapshot>("terminal_layout_save", {
        projectId,
        snapshot,
      });
      terminalLayoutStore.set(projectId, saved);
    } catch (error) {
      console.error("failed to save terminal layout", error);
    }
  }

  queue.flushing = false;
  if (queue.pending) {
    enqueueTerminalLayoutSave(projectId, queue.pending);
  } else {
    terminalLayoutSaveQueues.delete(projectId);
  }
}

function canUseTauri() {
  return typeof window !== "undefined" && Boolean(window.__TAURI_INTERNALS__);
}
