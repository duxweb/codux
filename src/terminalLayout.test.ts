import { describe, expect, it } from "vitest";
import {
  countTerminalSlots,
  countTopSplits,
  normalizeRatios,
  persistedTerminalLayoutSnapshot,
  resolvePrimaryTerminalId,
  resolveVisibleTerminalId,
  restoreTerminalLayout,
  snapshotTerminalLayout,
  type EnsureTerminalForLayout,
  type TerminalLayoutState,
} from "./terminalLayout";

function createEnsureTerminal(project = "project-a"): EnsureTerminalForLayout {
  return (slot) => ({ id: `${project}:${slot}` });
}

describe("terminal layout", () => {
  it("creates a default layout with a top split as the active terminal", () => {
    const layout = restoreTerminalLayout(undefined, createEnsureTerminal());

    expect(layout.topPanes).toHaveLength(1);
    expect(layout.tabs).toHaveLength(0);
    expect(layout.activeTabId).toBe("");
    expect(layout.activeSlotId).toBe("top-1");
    expect(layout.activeTerminalId).toBe("project-a:top-1");
    expect(layout.topRatios).toEqual([1]);
  });

  it("restores split and tab layout by slot id for the current project", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [
          { id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" },
          { id: "bottom-2", label: "标签页 2", terminalId: "old:bottom-2" },
        ],
        activeTabId: "bottom-2",
        topPanes: [
          { id: "top-1", title: "分屏 1", terminalId: "old:top-1" },
          { id: "top-2", title: "分屏 2", terminalId: "old:top-2" },
        ],
        topRatios: [0.7, 0.3],
        bottomRatio: 0.4,
        activeSlotId: "top-2",
      },
      createEnsureTerminal("project-b"),
    );

    expect(layout.topPanes.map((pane) => pane.terminalId)).toEqual(["project-b:top-1", "project-b:top-2"]);
    expect(layout.tabs.map((tab) => tab.terminalId)).toEqual(["project-b:bottom-1", "project-b:bottom-2"]);
    expect(layout.activeTabId).toBe("bottom-2");
    expect(layout.activeSlotId).toBe("top-2");
    expect(layout.activeTerminalId).toBe("project-b:top-2");
    expect(layout.bottomRatio).toBe(0.4);
    expect(layout.topRatios).toEqual([0.7, 0.3]);
  });

  it("restores splits in stable slot order", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" }],
        activeTabId: "bottom-1",
        topPanes: [
          { id: "top-3", title: "分屏 3", terminalId: "old:top-3" },
          { id: "top-2", title: "分屏 2", terminalId: "old:top-2" },
          { id: "top-1", title: "分屏 1", terminalId: "old:top-1" },
        ],
        topRatios: [1 / 3, 1 / 3, 1 / 3],
        bottomRatio: 0.32,
        activeSlotId: "top-3",
      },
      createEnsureTerminal("project-order"),
    );

    expect(layout.topPanes.map((pane) => pane.id)).toEqual(["top-1", "top-2", "top-3"]);
  });

  it("falls back to a valid active slot when cached active slot is missing", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" }],
        activeTabId: "missing-tab",
        topPanes: [{ id: "top-1", title: "分屏 1", terminalId: "old:top-1" }],
        topRatios: [2],
        bottomRatio: 0.32,
        activeSlotId: "missing",
      },
      createEnsureTerminal("project-c"),
    );

    expect(layout.activeTabId).toBe("bottom-1");
    expect(layout.activeSlotId).toBe("top-1");
    expect(layout.activeTerminalId).toBe("project-c:top-1");
    expect(layout.topRatios).toEqual([1]);
  });

  it("does not restore a detached pane as the active main-window terminal", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" }],
        activeTabId: "bottom-1",
        topPanes: [
          { id: "top-1", title: "分屏 1", terminalId: "old:top-1" },
          { id: "top-2", title: "分屏 2", terminalId: "old:top-2", detached: true },
        ],
        topRatios: [0.5, 0.5],
        bottomRatio: 0.32,
        activeSlotId: "top-2",
      },
      createEnsureTerminal("project-d"),
    );

    expect(layout.activeSlotId).toBe("top-1");
    expect(layout.activeTerminalId).toBe("project-d:top-1");
  });

  it("preserves the cached active split when entering a project", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" }],
        activeTabId: "bottom-1",
        topPanes: [
          { id: "top-1", title: "分屏 1", terminalId: "old:top-1" },
          { id: "top-2", title: "分屏 2", terminalId: "old:top-2" },
        ],
        topRatios: [0.5, 0.5],
        bottomRatio: 0.32,
        activeSlotId: "top-2",
      },
      createEnsureTerminal("project-e"),
    );

    expect(layout.activeTerminalId).toBe("project-e:top-2");
    expect(resolveVisibleTerminalId(layout, layout.activeTerminalId)).toBe("project-e:top-2");
    expect(resolvePrimaryTerminalId(layout)).toBe("project-e:top-1");
  });

  it("falls back to the first visible terminal when the preferred split is detached", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" }],
        activeTabId: "bottom-1",
        topPanes: [
          { id: "top-1", title: "分屏 1", terminalId: "old:top-1" },
          { id: "top-2", title: "分屏 2", terminalId: "old:top-2", detached: true },
        ],
        topRatios: [0.5, 0.5],
        bottomRatio: 0.32,
        activeSlotId: "top-1",
      },
      createEnsureTerminal("project-detached"),
    );

    expect(resolveVisibleTerminalId(layout, "project-detached:top-2")).toBe("project-detached:top-1");
  });

  it("falls back to the active bottom tab when all top splits are detached", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [
          { id: "bottom-1", label: "标签页 1", terminalId: "old:bottom-1" },
          { id: "bottom-2", label: "标签页 2", terminalId: "old:bottom-2" },
        ],
        activeTabId: "bottom-2",
        topPanes: [{ id: "top-1", title: "分屏 1", terminalId: "old:top-1", detached: true }],
        topRatios: [1],
        bottomRatio: 0.32,
        activeSlotId: "top-1",
      },
      createEnsureTerminal("project-f"),
    );

    expect(resolvePrimaryTerminalId(layout)).toBe("project-f:bottom-2");
  });

  it("restores a layout after all bottom tabs were closed", () => {
    const layout = restoreTerminalLayout(
      {
        tabs: [],
        activeTabId: "bottom-1",
        topPanes: [{ id: "top-1", title: "分屏 1", terminalId: "old:top-1" }],
        topRatios: [1],
        bottomRatio: 0.32,
        activeSlotId: "bottom-1",
      },
      createEnsureTerminal("project-empty-tabs"),
    );

    expect(layout.tabs).toEqual([]);
    expect(layout.activeTabId).toBe("");
    expect(layout.activeSlotId).toBe("top-1");
    expect(layout.activeTerminalId).toBe("project-empty-tabs:top-1");
  });

  it("snapshots the current active terminal as a stable slot id", () => {
    const state: TerminalLayoutState = {
      tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "project:bottom-1" }],
      activeTabId: "bottom-1",
      topPanes: [
        { id: "top-1", title: "分屏 1", terminalId: "project:top-1" },
        { id: "top-2", title: "分屏 2", terminalId: "project:top-2" },
      ],
      topRatios: [0.5, 0.5],
      bottomRatio: 0.25,
      activeTerminalId: "project:top-2",
      activeSlotId: "top-1",
    };

    expect(snapshotTerminalLayout(state).activeSlotId).toBe("top-2");
  });

  it("snapshots splits in stable slot order", () => {
    const state: TerminalLayoutState = {
      tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "project:bottom-1" }],
      activeTabId: "bottom-1",
      topPanes: [
        { id: "top-3", title: "分屏 3", terminalId: "project:top-3" },
        { id: "top-1", title: "分屏 1", terminalId: "project:top-1" },
        { id: "top-2", title: "分屏 2", terminalId: "project:top-2" },
      ],
      topRatios: [1 / 3, 1 / 3, 1 / 3],
      bottomRatio: 0.25,
      activeTerminalId: "project:top-3",
      activeSlotId: "top-1",
    };

    expect(snapshotTerminalLayout(state).topPanes.map((pane) => pane.id)).toEqual(["top-1", "top-2", "top-3"]);
  });

  it("snapshots empty bottom tabs without creating a fallback tab", () => {
    const state: TerminalLayoutState = {
      tabs: [],
      activeTabId: "",
      topPanes: [{ id: "top-1", title: "分屏 1", terminalId: "project:top-1" }],
      topRatios: [1],
      bottomRatio: 0.25,
      activeTerminalId: "project:top-1",
      activeSlotId: "top-1",
    };

    const snapshot = snapshotTerminalLayout(state);

    expect(snapshot.tabs).toEqual([]);
    expect(snapshot.activeTabId).toBe("");
    expect(snapshot.activeSlotId).toBe("top-1");
  });

  it("does not persist detached window state across app launches", () => {
    const snapshot = persistedTerminalLayoutSnapshot({
      tabs: [{ id: "bottom-1", label: "标签页 1", terminalId: "project:bottom-1" }],
      activeTabId: "bottom-1",
      topPanes: [
        { id: "top-1", title: "分屏 1", terminalId: "project:top-1", detached: true },
        { id: "top-2", title: "分屏 2", terminalId: "project:top-2" },
      ],
      topRatios: [0.5, 0.5],
      bottomRatio: 0.25,
      activeSlotId: "top-2",
    });

    expect(snapshot.topPanes).toEqual([
      { id: "top-1", title: "分屏 1", terminalId: "project:top-1" },
      { id: "top-2", title: "分屏 2", terminalId: "project:top-2" },
    ]);
  });

  it("normalizes ratios and fills missing panes", () => {
    expect(normalizeRatios([2, 1], 2)).toEqual([2 / 3, 1 / 3]);
    expect(normalizeRatios([], 3)).toEqual([1 / 3, 1 / 3, 1 / 3]);
  });

  it("counts terminal slots while keeping top split count available for split limits", () => {
    const layout = {
      topPanes: [
        { id: "top-1", title: "分屏 1", terminalId: "project:top-1" },
        { id: "top-2", title: "分屏 2", terminalId: "project:top-2" },
      ],
      tabs: [
        { id: "bottom-1", label: "标签页 1", terminalId: "project:bottom-1" },
        { id: "bottom-2", label: "标签页 2", terminalId: "project:bottom-2" },
        { id: "bottom-3", label: "标签页 3", terminalId: "project:bottom-3" },
      ],
    };

    expect(countTerminalSlots(layout)).toBe(5);
    expect(countTopSplits(layout)).toBe(2);
  });
});
