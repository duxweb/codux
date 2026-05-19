import {
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  Copy,
  FileCode2,
  FileText,
  Folder,
  GitPullRequest,
  Maximize2,
  RotateCcw,
  Search,
  TerminalSquare,
  Undo2,
  Redo2,
  X,
} from "../icons";
import { listen } from "@tauri-apps/api/event";
import type { CSSProperties, MutableRefObject, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "./Button";
import { PressableButton } from "./PressableButton";
import { TabStrip } from "./TabStrip";
import { TerminalView } from "./TerminalView";
import { Tooltip } from "./Tooltip";
import {
  loadGitReviewFileContent,
  useGitReviewSnapshot,
  type GitReviewContentSnapshot,
  type GitReviewFile,
} from "../git/review";
import type { CodeEditorLineHighlight } from "./CodeEditor";
import { isConfiguredShortcut, registerShortcutHandler } from "../shortcuts";
import {
  countTopSplits,
  equalRatios,
  normalizeRatios,
  resolvePrimaryTerminalId,
  restoreTerminalLayout,
  resolveActiveSlotId,
  snapshotTerminalLayout,
  terminalLayoutStore,
  resolveVisibleTerminalId,
  loadTerminalLayoutSnapshot,
  rememberTerminalLayoutSnapshot,
  type BottomTabLayout,
  type TerminalLayoutState,
  type TopPaneLayout,
} from "../terminalLayout";
import { terminalRuntime } from "../terminal/runtime";
import type { MainView, TerminalSession, WorkspaceProject } from "../types";
import { openDetachedTerminalWindow } from "../windowing";
import { broadcastWorkspaceCommand, listenWorkspaceCommand } from "../workspaceCommands";
import { systemConfirm } from "../systemDialog";
import { formatI18n, tm } from "../i18n";
import {
  languageForPath,
  readFile,
  revealFile,
  unwatchProjectFiles,
  watchProjectFiles,
  writeFile,
  type FileChangeEvent,
  type FileReadResult,
} from "../files/api";
import { CodeEditor, type CodeEditorHandle, type CodeEditorScrollInfo } from "./CodeEditor";

const MAX_TERMINAL_SPLITS = 6;
const MIN_TOP_PANE_RATIO = 0.12;
const MIN_BOTTOM_RATIO = 0.18;
const MAX_BOTTOM_RATIO = 0.72;
const FALLBACK_PROJECT_ID = "00000000-0000-5000-8000-000000000001";
const FILE_WATCH_DEBOUNCE_MS = 180;

type Props = {
  mainView: MainView;
  selectedProject?: WorkspaceProject;
  onSessionChange: (session: TerminalSession | null) => void;
  terminalFocusRequest?: number;
};

type BottomTab = BottomTabLayout;
type TopPane = TopPaneLayout;
type TerminalLaunchOptions = {
  title?: string;
  label?: string;
  command?: string;
  tool?: string;
};

export function Workspace({ mainView, selectedProject, onSessionChange, terminalFocusRequest = 0 }: Props) {
  const canUsePreviewFallback = !window.__TAURI_INTERNALS__;
  const projectId = selectedProject?.id ?? (canUsePreviewFallback ? FALLBACK_PROJECT_ID : "");
  const cwd = selectedProject?.path ?? (canUsePreviewFallback ? "/Volumes/Web/codux-tauri" : "");
  const fallbackProject = useMemo<WorkspaceProject | undefined>(
    () => {
      if (selectedProject) return selectedProject;
      if (!canUsePreviewFallback) return undefined;
      return {
        id: projectId,
        name: cwd.split("/").filter(Boolean).pop() ?? "Workspace",
        path: cwd,
        badge: "WS",
        status: "active",
        branch: "master",
        aiState: "idle",
        terminals: 0,
        changes: 0,
      };
    },
    [canUsePreviewFallback, cwd, projectId, selectedProject],
  );
  const [terminalProjects, setTerminalProjects] = useState<WorkspaceProject[]>(() =>
    fallbackProject ? [fallbackProject] : [],
  );

  useEffect(() => {
    if (!fallbackProject) return;
    setTerminalProjects((current) => {
      const index = current.findIndex((project) => project.id === fallbackProject.id);
      if (index < 0) {
        return [...current, fallbackProject];
      }
      const next = [...current];
      next[index] = fallbackProject;
      return next;
    });
  }, [fallbackProject]);

  return (
    <section className="h-full overflow-hidden">
      {terminalProjects.map((project) => {
        const visible = mainView === "terminal" && project.id === projectId;
        return (
          <div key={project.id} className={visible ? "h-full" : "hidden"}>
            <TerminalMode
              cwd={project.path}
              projectId={project.id}
              projectName={project.name}
              visible={visible}
              focusRequest={terminalFocusRequest}
              onSessionChange={onSessionChange}
            />
          </div>
        );
      })}
      <div className={mainView === "files" ? "h-full" : "hidden"}>
        <FilesMode project={selectedProject} />
      </div>
      <div className={mainView === "review" ? "h-full" : "hidden"}>
        <ReviewMode project={selectedProject} />
      </div>
    </section>
  );
}

function TerminalMode({
  cwd,
  projectId,
  projectName,
  visible,
  focusRequest: externalFocusRequest,
  onSessionChange,
}: {
  cwd: string;
  projectId: string;
  projectName?: string;
  visible: boolean;
  focusRequest: number;
  onSessionChange: (session: TerminalSession | null) => void;
}) {
  const ensureTerminal = useCallback(
    (slot: string, title: string, launch?: TerminalLaunchOptions) =>
      terminalRuntime.ensureTerminal({
        projectId,
        slotId: slot,
        title: launch?.title ?? title,
        cwd,
        projectName,
        command: launch?.command,
        tool: launch?.tool,
      }),
    [cwd, projectId, projectName],
  );
  const initialLayout = useMemo(() => {
    return restoreTerminalLayout(terminalLayoutStore.get(projectId), ensureTerminal);
  }, [ensureTerminal, projectId]);

  const [tabs, setTabs] = useState(initialLayout.tabs);
  const [activeTabId, setActiveTabId] = useState(initialLayout.activeTabId);
  const [topPanes, setTopPanes] = useState(initialLayout.topPanes);
  const [activeTerminalId, setActiveTerminalId] = useState(initialLayout.activeTerminalId);
  const [focusRequest, setFocusRequest] = useState(0);
  const [topRatios, setTopRatios] = useState(initialLayout.topRatios);
  const [bottomRatio, setBottomRatio] = useState(initialLayout.bottomRatio);
  const [isLayoutHydrated, setLayoutHydrated] = useState(
    () => !window.__TAURI_INTERNALS__ || terminalLayoutStore.has(projectId),
  );
  const workspaceRef = useRef<HTMLDivElement | null>(null);
  const topGridRef = useRef<HTMLDivElement | null>(null);
  const visibleTopPanes = useMemo(
    () => topPanes.filter((pane) => !pane.detached),
    [topPanes],
  );
  const layoutRef = useRef<TerminalLayoutState>({
    tabs,
    activeTabId,
    topPanes,
    topRatios,
    bottomRatio,
    activeTerminalId,
    activeSlotId: resolveActiveSlotId(topPanes, tabs, activeTerminalId),
  });

  const activeTerminalSessionId = useMemo(
    () =>
      activeTerminalId ||
      topPanes[0]?.terminalId ||
      tabs.find((tab) => tab.id === activeTabId)?.terminalId ||
      null,
    [activeTabId, activeTerminalId, tabs, topPanes],
  );

  const activateTerminal = useCallback((terminalId: string, options?: { focus?: boolean }) => {
    if (!terminalId) return;
    setActiveTerminalId(terminalId);
    if (options?.focus) {
      setFocusRequest((current) => current + 1);
    }
  }, []);

  const applyTerminalLayout = useCallback((layout: TerminalLayoutState) => {
    setTabs(layout.tabs);
    setActiveTabId(layout.activeTabId);
    setTopPanes(layout.topPanes);
    setTopRatios(layout.topRatios);
    setBottomRatio(layout.bottomRatio);
    setActiveTerminalId(layout.activeTerminalId);
  }, []);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || terminalLayoutStore.has(projectId)) {
      setLayoutHydrated(true);
      return;
    }

    let disposed = false;
    setLayoutHydrated(false);
    void loadTerminalLayoutSnapshot(projectId)
      .then((snapshot) => {
        if (disposed) return;
        applyTerminalLayout(restoreTerminalLayout(snapshot, ensureTerminal));
        setLayoutHydrated(true);
      })
      .catch((error) => {
        console.error("failed to load terminal layout", error);
        if (!disposed) {
          setLayoutHydrated(true);
        }
      });

    return () => {
      disposed = true;
    };
  }, [applyTerminalLayout, ensureTerminal, projectId]);

  useEffect(() => {
    if (!visible) return;
    if (!activeTerminalSessionId) {
      onSessionChange(null);
      return;
    }
    onSessionChange(terminalRuntime.getSession(activeTerminalSessionId) ?? null);
    return terminalRuntime.subscribe(activeTerminalSessionId, (event) => {
      if (event.type === "closed") {
        onSessionChange(null);
        return;
      }
      if (event.type === "state" || event.type === "reset" || event.type === "output") {
        onSessionChange(event.session);
      }
    });
  }, [activeTerminalSessionId, onSessionChange, visible]);

  useEffect(() => {
    layoutRef.current = {
      tabs,
      activeTabId,
      topPanes,
      topRatios,
      bottomRatio,
      activeTerminalId,
      activeSlotId: resolveActiveSlotId(topPanes, tabs, activeTerminalId),
    };
    const snapshot = snapshotTerminalLayout(layoutRef.current);
    if (isLayoutHydrated) {
      rememberTerminalLayoutSnapshot(projectId, snapshot);
    } else {
      terminalLayoutStore.set(projectId, snapshot);
    }
  }, [
    activeTabId,
    activeTerminalId,
    bottomRatio,
    isLayoutHydrated,
    projectId,
    tabs,
    topPanes,
    topRatios,
  ]);

  useEffect(() => {
    return () => {
      const snapshot = snapshotTerminalLayout(layoutRef.current);
      if (isLayoutHydrated) {
        rememberTerminalLayoutSnapshot(projectId, snapshot);
      } else {
        terminalLayoutStore.set(projectId, snapshot);
      }
    };
  }, [isLayoutHydrated, projectId]);

  const closeTopPane = useCallback((id: string) => {
    setTopPanes((current) => {
      const pane = current.find((item) => item.id === id);
      if (pane) {
        void terminalRuntime.close(pane.terminalId);
      }
      const next = current.filter((item) => item.id !== id);
      setTopRatios(equalRatios(next.length || 1));
      if (pane?.terminalId === activeTerminalId) {
        activateTerminal(next[0]?.terminalId ?? tabs.find((tab) => tab.id === activeTabId)?.terminalId ?? "");
      }
      return next;
    });
  }, [activateTerminal, activeTabId, activeTerminalId, tabs]);

  const detachTopPane = useCallback((paneId: string) => {
    const pane = topPanes.find((item) => item.id === paneId);
    if (!pane || pane.detached) return;
    setTopPanes((current) =>
      current.map((item) => (item.id === paneId ? { ...item, detached: true } : item)),
    );
    if (activeTerminalId === pane.terminalId) {
      const nextPane = topPanes.find((item) => item.id !== paneId && !item.detached);
      activateTerminal(nextPane?.terminalId ?? tabs.find((tab) => tab.id === activeTabId)?.terminalId ?? "");
    }
    void openTerminalWindow(pane.terminalId, pane.id);
  }, [activateTerminal, activeTabId, activeTerminalId, tabs, topPanes]);

  const addTopPane = useCallback((launch?: TerminalLaunchOptions) => {
    setTopPanes((current) => {
      if (countTopSplits({ topPanes: current }) >= MAX_TERMINAL_SPLITS) {
        return current;
      }
      const id = nextTerminalPaneId("top", current.map((pane) => pane.id));
      const title = launch?.title ?? formatI18n(tm("workspace.split_format", "Split %@"), current.length + 1);
      const terminalId = ensureTerminal(id, title, launch).id;
      setTopRatios(equalRatios(current.length + 1));
      activateTerminal(terminalId);
      return [
        ...current,
        {
          id,
          title,
          terminalId,
        },
      ];
    });
  }, [activateTerminal, ensureTerminal]);

  const addBottomTab = useCallback((launch?: TerminalLaunchOptions) => {
    setTabs((current) => {
      const id = nextTerminalPaneId("bottom", current.map((tab) => tab.id));
      const label = launch?.label ?? formatI18n(tm("workspace.tab_format", "Tab %@"), current.length + 1);
      setActiveTabId(id);
      const terminalId = ensureTerminal(id, label, launch).id;
      activateTerminal(terminalId);
      return [
        ...current,
        {
          id,
          label,
          terminalId,
        },
      ];
    });
  }, [activateTerminal, ensureTerminal]);

  useEffect(
    () =>
      listenWorkspaceCommand((command) => {
        if (command.type === "add-top-terminal-split") {
          addTopPane({
            title: command.title,
            command: command.command,
            tool: command.command ? "manual" : undefined,
          });
        }
        if (command.type === "add-bottom-terminal-tab") {
          addBottomTab({
            label: command.label,
            command: command.command,
            tool: command.command ? "manual" : undefined,
          });
        }
        if (command.type === "insert-terminal-text") {
          const terminalId = resolveVisibleTerminalId(
            {
              topPanes,
              tabs,
              activeTabId,
            },
            activeTerminalId,
          );
          if (terminalId) {
            terminalRuntime.write(terminalId, command.text);
            activateTerminal(terminalId, { focus: true });
          }
        }
        if (command.type === "reattach-terminal-pane") {
          setTopPanes((current) =>
            current.map((pane) =>
              pane.id === command.paneId && pane.terminalId === command.terminalId
                ? { ...pane, detached: false }
                : pane,
            ),
          );
        }
      }),
    [activateTerminal, activeTabId, activeTerminalId, addBottomTab, addTopPane, tabs, topPanes],
  );

  useEffect(() => {
    if (!visible) return;
    const terminalId = resolveVisibleTerminalId(
      {
        topPanes,
        tabs,
        activeTabId,
      },
      activeTerminalId,
    );
    if (terminalId) {
      activateTerminal(terminalId);
    }
  }, [activateTerminal, activeTabId, activeTerminalId, tabs, topPanes, visible]);

  useEffect(() => {
    if (!visible || externalFocusRequest <= 0) return;
    const terminalId = resolveVisibleTerminalId(
      {
        topPanes,
        tabs,
        activeTabId,
      },
      activeTerminalId,
    );
    if (!terminalId) return;
    activateTerminal(terminalId, { focus: true });
  }, [
    activateTerminal,
    activeTabId,
    activeTerminalId,
    externalFocusRequest,
    tabs,
    topPanes,
    visible,
  ]);

  const closeBottomTab = useCallback((id: string) => {
    setTabs((current) => {
      const tab = current.find((item) => item.id === id);
      if (tab) {
        void terminalRuntime.close(tab.terminalId);
      }
      const next = current.filter((tab) => tab.id !== id);
      if (activeTabId === id) {
        setActiveTabId(next[next.length - 1]?.id ?? "");
      }
      if (tab?.terminalId === activeTerminalId) {
        const nextTerminalId = next[next.length - 1]?.terminalId ?? visibleTopPanes[0]?.terminalId ?? "";
        if (nextTerminalId) {
          activateTerminal(nextTerminalId);
        } else {
          setActiveTerminalId("");
        }
      }
      return next;
    });
  }, [activateTerminal, activeTabId, activeTerminalId, visibleTopPanes]);

  const closeActiveTerminal = useCallback(() => {
    const activeTopPane = visibleTopPanes.find((pane) => pane.terminalId === activeTerminalId);
    const activeTab = tabs.find((tab) => tab.terminalId === activeTerminalId);
    if (!activeTopPane && !activeTab) {
      return;
    }
    if (activeTopPane && visibleTopPanes.length <= 1) {
      return;
    }

    const label = activeTopPane?.title ?? activeTab?.label ?? tm("terminal.current", "Current Terminal");
    void systemConfirm(formatI18n(tm("terminal.close.message_format", "Close %@?"), label), {
      title: tm("terminal.close.title", "Close Terminal"),
      kind: "warning",
      okLabel: tm("common.close", "Close"),
      cancelLabel: tm("common.cancel", "Cancel"),
    }).then((confirmed) => {
      if (!confirmed) return;
      if (activeTopPane) {
        closeTopPane(activeTopPane.id);
        return;
      }
      if (activeTab) {
        closeBottomTab(activeTab.id);
      }
    });
  }, [activeTerminalId, closeBottomTab, closeTopPane, tabs, visibleTopPanes]);

  useEffect(() => {
    return registerShortcutHandler("workspace", (event, context) => {
      if (context.mainView !== "terminal") {
        return false;
      }
      if (!visible) {
        return false;
      }
      if (!isConfiguredShortcut(event, "close.active")) {
        return false;
      }

      closeActiveTerminal();
      return true;
    });
  }, [closeActiveTerminal, visible]);

  useEffect(
    () =>
      listenWorkspaceCommand((command) => {
        if (command.type === "close-active") {
          if (!visible) return;
          closeActiveTerminal();
        }
      }),
    [closeActiveTerminal, visible],
  );

  const adjustTopRatio = useCallback((dividerIndex: number, clientX: number) => {
    const container = topGridRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    if (rect.width <= 0) return;

    setTopRatios((current) => {
      const ratios = normalizeRatios(current, topPanes.length);
      const previousSum = ratios
        .slice(0, dividerIndex)
        .reduce((sum, value) => sum + value, 0);
      const pairSum = ratios[dividerIndex] + ratios[dividerIndex + 1];
      const pointerRatio = clamp((clientX - rect.left) / rect.width, 0, 1);
      const nextLeft = clamp(
        pointerRatio - previousSum,
        Math.min(MIN_TOP_PANE_RATIO, pairSum / 2),
        Math.max(MIN_TOP_PANE_RATIO, pairSum - MIN_TOP_PANE_RATIO),
      );
      ratios[dividerIndex] = nextLeft;
      ratios[dividerIndex + 1] = pairSum - nextLeft;
      return normalizeRatios(ratios, topPanes.length);
    });
  }, [topPanes.length]);

  const adjustBottomRatio = useCallback((clientY: number) => {
    const container = workspaceRef.current;
    if (!container) return;
    const rect = container.getBoundingClientRect();
    if (rect.height <= 0) return;
    const next = (rect.bottom - clientY) / rect.height;
    setBottomRatio(clamp(next, MIN_BOTTOM_RATIO, MAX_BOTTOM_RATIO));
  }, []);

  return (
    <div
      ref={workspaceRef}
      className="terminal-workspace h-full grid"
      style={workspaceGridStyle(bottomRatio, tabs.length > 0)}
    >
      <div
        ref={topGridRef}
        className="terminal-split-grid"
        style={topGridStyle(visibleTopPanes.length, topRatios)}
      >
        {visibleTopPanes.map((pane, index) => (
          <TerminalPaneGroup key={pane.id}>
            {index > 0 && (
              <TerminalSplitDivider
                orientation="vertical"
                onDrag={(event) => adjustTopRatio(index - 1, event.clientX)}
              />
            )}
            <TerminalPane
              title={pane.title}
              terminalId={pane.terminalId}
              detached={Boolean(pane.detached)}
              active={visible && pane.terminalId === activeTerminalId}
              canClose={visibleTopPanes.length > 1}
              focusRequest={focusRequest}
              onActivate={() => activateTerminal(pane.terminalId, { focus: true })}
              onFocusActivate={() => activateTerminal(pane.terminalId)}
              onClose={() => closeTopPane(pane.id)}
              onDetach={() => detachTopPane(pane.id)}
            />
          </TerminalPaneGroup>
        ))}
      </div>
      <TerminalSplitDivider
        orientation="horizontal"
        onDrag={(event) => adjustBottomRatio(event.clientY)}
        disabled={tabs.length === 0}
      />
      <div
        className={`terminal-bottom-area grid min-w-0 ${tabs.length > 0 ? "grid-rows-[44px_minmax(0,1fr)]" : "grid-rows-[44px]"}`}
      >
        <TabStrip
          items={tabs.map((tab) => ({
            id: tab.id,
            label: tab.label,
            closable: true,
          }))}
          activeId={activeTabId}
          emptyLabel={tm("terminal.title", "Terminal")}
          onSelect={(id) => {
            const tab = tabs.find((item) => item.id === id);
            if (!tab) return;
            setActiveTabId(tab.id);
            activateTerminal(tab.terminalId, { focus: true });
          }}
          onClose={closeBottomTab}
          onAdd={addBottomTab}
        />
        {tabs.length > 0 &&
          tabs.map((tab) => (
            <div
              key={tab.id}
              className={
                tab.id === activeTabId
                  ? `terminal-tab-pane ${tab.terminalId === activeTerminalId ? "is-active" : "is-inactive"}`
                  : "hidden"
              }
              onPointerDown={() => activateTerminal(tab.terminalId, { focus: true })}
              onFocusCapture={() => activateTerminal(tab.terminalId)}
            >
              {tab.id === activeTabId && (
                <TerminalView
                  terminalId={tab.terminalId}
                  chrome={false}
                  active={visible && tab.terminalId === activeTerminalId}
                  focusRequest={focusRequest}
                />
              )}
              {tab.id === activeTabId && tab.terminalId !== activeTerminalId && (
                <div className="terminal-pane-dim" aria-hidden="true" />
              )}
            </div>
          ))}
      </div>
    </div>
  );
}

async function openTerminalWindow(terminalId: string, paneId: string) {
  const session = terminalRuntime.getSession(terminalId);
  if (!session?.backendId) return;
  await openDetachedTerminalWindow({
    terminalId,
    backendId: session.backendId,
    projectId: session.projectId,
    slotId: session.slotId,
    paneId,
    title: session.title,
    cwd: session.cwd,
    projectName: session.projectName,
  });
}

function TerminalPaneGroup({ children }: { children: ReactNode }) {
  return <>{children}</>;
}

function nextTerminalPaneId(prefix: string, existingIds: string[]) {
  const used = new Set(existingIds);
  const maxIndex = prefix === "top" ? MAX_TERMINAL_SPLITS : existingIds.length + 1;
  for (let index = 1; index <= maxIndex; index += 1) {
    const id = `${prefix}-${index}`;
    if (!used.has(id)) return id;
  }
  return `${prefix}-${Date.now()}`;
}

function workspaceGridStyle(bottomRatio: number, hasBottomTabs: boolean): CSSProperties {
  if (!hasBottomTabs) {
    return {
      gridTemplateRows: "minmax(180px, 1fr) 1px 44px",
    };
  }

  const bottom = `${(bottomRatio * 100).toFixed(3)}fr`;
  const top = `${((1 - bottomRatio) * 100).toFixed(3)}fr`;
  return {
    gridTemplateRows: `minmax(180px, ${top}) 1px minmax(160px, ${bottom})`,
  };
}

function topGridStyle(count: number, ratios: number[]): CSSProperties {
  if (count <= 0) {
    return { gridTemplateColumns: "minmax(0, 1fr)" };
  }
  const normalized = normalizeRatios(ratios, count);
  return {
    gridTemplateColumns: Array.from({ length: count }, (_, index) =>
      index === 0
        ? `minmax(180px, ${normalized[index]}fr)`
        : `1px minmax(180px, ${normalized[index]}fr)`,
    ).join(" "),
  };
}

function clamp(value: number, min: number, max: number) {
  return Math.min(max, Math.max(min, value));
}

function TerminalSplitDivider({
  orientation,
  onDrag,
  disabled = false,
}: {
  orientation: "vertical" | "horizontal";
  onDrag: (event: globalThis.PointerEvent) => void;
  disabled?: boolean;
}) {
  const [isDragging, setIsDragging] = useState(false);

  const startDrag = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (disabled) {
      return;
    }
    event.preventDefault();
    setIsDragging(true);
    const pointerId = event.pointerId;
    event.currentTarget.setPointerCapture(pointerId);

    const handleMove = (moveEvent: globalThis.PointerEvent) => {
      onDrag(moveEvent);
    };
    const finishDrag = () => {
      setIsDragging(false);
      window.removeEventListener("pointermove", handleMove);
      window.removeEventListener("pointerup", finishDrag);
      window.removeEventListener("pointercancel", finishDrag);
    };

    window.addEventListener("pointermove", handleMove);
    window.addEventListener("pointerup", finishDrag);
    window.addEventListener("pointercancel", finishDrag);
  };

  return (
    <div
      className={`terminal-split-divider ${orientation} ${isDragging ? "dragging" : ""}`}
      onPointerDown={startDrag}
      role="separator"
      aria-disabled={disabled}
      aria-orientation={orientation === "vertical" ? "vertical" : "horizontal"}
    />
  );
}

function TerminalPane({
  title,
  terminalId,
  detached,
  active,
  canClose,
  focusRequest,
  onActivate,
  onFocusActivate,
  onClose,
  onDetach,
}: {
  title: string;
  terminalId: string;
  detached: boolean;
  active: boolean;
  canClose: boolean;
  focusRequest: number;
  onActivate: () => void;
  onFocusActivate: () => void;
  onClose: () => void;
  onDetach: () => void;
}) {
  const activatePane = (event: ReactPointerEvent<HTMLElement>) => {
    const target = event.target as Element | null;
    if (target?.closest("button, a, input, textarea, select, [data-terminal-control]")) {
      return;
    }
    onActivate();
  };

  return (
    <section
      className={`terminal-pane ${active ? "is-active" : "is-inactive"}`}
      onPointerDown={activatePane}
      onFocusCapture={onFocusActivate}
    >
      <div
        className="absolute z-20 top-2 right-2 flex items-center gap-1 text-xs text-ink-faint"
        data-terminal-control
      >
        <span className="text-ink-mute mr-1">{title}</span>
        <Tooltip label={tm("terminal.detach", "Open in Separate Window")} placement="bottom">
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            onPress={onDetach}
            aria-label={tm("terminal.detach", "Open in Separate Window")}
            className="h-[22px] w-[22px] min-w-[22px] focus:outline-none focus-visible:outline-none"
            disabled={detached}
          >
            <Maximize2 size={12} strokeWidth={2.2} />
          </Button>
        </Tooltip>
        {canClose && (
          <Tooltip label={tm("terminal.split.close", "Close Split")} placement="bottom">
            <Button
              isIconOnly
              size="sm"
              variant="ghost"
              onPress={onClose}
              aria-label={tm("terminal.split.close", "Close Split")}
              className="h-[22px] w-[22px] min-w-[22px] focus:outline-none focus-visible:outline-none"
            >
              <X size={12} strokeWidth={2.2} />
            </Button>
          </Tooltip>
        )}
      </div>
      {detached ? (
        <div className="h-full min-w-0 min-h-0 grid place-items-center rounded-[8px] border border-dashed border-line text-xs text-ink-mute">
          {tm("terminal.detached.message", "This terminal is open in a separate window.")}
        </div>
      ) : (
        <TerminalView
          terminalId={terminalId}
          chrome={false}
          active={active}
          focusRequest={focusRequest}
        />
      )}
      {!active && <div className="terminal-pane-dim" aria-hidden="true" />}
    </section>
  );
}

type OpenFileTab = FileReadResult & {
  rootPath: string;
  language: string;
  savedContent: string;
  dirty: boolean;
  version: number;
  scrollTop?: number;
  externalModifiedAt?: number;
  externalSize?: number;
};

type FilesModeState = {
  tabs: OpenFileTab[];
  activePath: string;
};

const filesModeStateByProject = new Map<string, FilesModeState>();

function normalizeFileEventPath(value: string) {
  return value.replace(/\\/g, "/").replace(/\/+$/, "");
}

function relativeParentDirectory(path: string, fileName: string) {
  const normalized = normalizeFileEventPath(path);
  const parts = normalized.split("/").filter(Boolean);
  if (parts.length <= 1) return "";
  const lastPart = parts[parts.length - 1];
  if (lastPart === fileName) {
    parts.pop();
  }
  return parts.join("/");
}

function fileChangeTouchesTab(event: FileChangeEvent, tab: OpenFileTab) {
  const tabPath = normalizeFileEventPath(tab.path);
  const rootPath = normalizeFileEventPath(tab.rootPath);
  const eventProject = normalizeFileEventPath(event.projectPath);
  if (eventProject !== rootPath && !tabPath.startsWith(`${eventProject}/`) && !eventProject.startsWith(`${rootPath}/`)) {
    return false;
  }
  return event.changedPaths.some((path) => normalizeFileEventPath(path) === tabPath);
}

function FilesMode({ project }: { project?: WorkspaceProject }) {
  const projectStateKey = project?.id ?? "";
  const [tabs, setTabs] = useState<OpenFileTab[]>(() => filesModeStateByProject.get(projectStateKey)?.tabs ?? []);
  const [activePath, setActivePath] = useState(() => filesModeStateByProject.get(projectStateKey)?.activePath ?? "");
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setBusy] = useState(false);
  const editorRef = useRef<CodeEditorHandle | null>(null);
  const tabsRef = useRef<OpenFileTab[]>([]);
  const active = tabs.find((tab) => tab.path === activePath) ?? tabs[0];

  useEffect(() => {
    tabsRef.current = tabs;
  }, [tabs]);

  useEffect(() => {
    const stored = filesModeStateByProject.get(projectStateKey);
    setTabs(stored?.tabs ?? []);
    setActivePath(stored?.activePath ?? "");
    setError(null);
  }, [projectStateKey]);

  useEffect(() => {
    if (!projectStateKey) return;
    filesModeStateByProject.set(projectStateKey, { tabs, activePath });
  }, [activePath, projectStateKey, tabs]);

  const replaceTabWithReadResult = useCallback((item: OpenFileTab, result: FileReadResult) => ({
    ...item,
    ...result,
    language: languageForPath(result.path),
    savedContent: result.content,
    dirty: false,
    version: item.version + 1,
    externalModifiedAt: undefined,
    externalSize: undefined,
  }), []);

  const openFileInEditor = useCallback(async (rootPath: string, path: string) => {
    setError(null);
    if (tabsRef.current.some((tab) => tab.path === path && tab.rootPath === rootPath)) {
      setActivePath(path);
      return;
    }
    setBusy(true);
    try {
      const result = await readFile(rootPath, path);
      const nextTab: OpenFileTab = {
        ...result,
        rootPath,
        language: languageForPath(result.path),
        savedContent: result.content,
        dirty: false,
        version: 0,
      };
      setTabs((current) => {
        const existing = current.findIndex((tab) => tab.path === result.path && tab.rootPath === rootPath);
        if (existing >= 0) {
          const next = [...current];
          next[existing] = nextTab;
          return next;
        }
        return [...current, nextTab];
      });
      setActivePath(result.path);
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setBusy(false);
    }
  }, []);

  const closeTab = useCallback(async (path: string) => {
    const tab = tabsRef.current.find((item) => item.path === path);
    if (
      tab?.dirty &&
      !(await systemConfirm(tm("files.preview.discard_changes.message", "This preview has edits that have not been saved to the original file."), {
        title: tm("files.preview.discard_changes.title", "Discard unsaved changes?"),
        kind: "warning",
        okLabel: tm("files.preview.discard_changes.discard", "Discard Changes"),
        cancelLabel: tm("common.cancel", "Cancel"),
      }))
    ) {
      return;
    }
    setTabs((current) => {
      const next = current.filter((item) => item.path !== path);
      if (activePath === path) {
        setActivePath(next[next.length - 1]?.path ?? "");
      }
      return next;
    });
  }, [activePath]);

  const updateActiveContent = useCallback((content: string) => {
    setTabs((current) =>
      current.map((tab) =>
        tab.path === activePath ? { ...tab, content, dirty: content !== tab.savedContent } : tab,
      ),
    );
  }, [activePath]);

  const saveActive = useCallback(async () => {
    const tab = tabsRef.current.find((item) => item.path === activePath);
    if (!tab || tab.readOnly || !tab.dirty) return;
    setBusy(true);
    setError(null);
    try {
      if (window.__TAURI_INTERNALS__) {
        const currentDisk = await readFile(tab.rootPath, tab.path);
        const diskChanged = currentDisk.modifiedAt !== tab.modifiedAt || currentDisk.size !== tab.size;
        if (diskChanged) {
          const overwrite = await systemConfirm(
            formatI18n(tm("files.preview.external_save.message_format", "\"%@\" was changed outside the app. Saving now will overwrite the newer disk version."), tab.name),
            {
              title: tm("files.preview.external_save.title", "Overwrite External Changes"),
              kind: "warning",
              okLabel: tm("files.preview.external_save.overwrite", "Overwrite and Save"),
              cancelLabel: tm("common.cancel", "Cancel"),
            },
          );
          if (!overwrite) {
            setTabs((current) =>
              current.map((item) =>
                item.path === tab.path
                  ? {
                      ...item,
                      externalModifiedAt: currentDisk.modifiedAt,
                      externalSize: currentDisk.size,
                    }
                  : item,
              ),
            );
            return;
          }
        }
      }
      const saved = await writeFile(tab.rootPath, tab.path, tab.content);
      setTabs((current) =>
        current.map((item) =>
          item.path === tab.path
            ? replaceTabWithReadResult(item, saved)
            : item,
        ),
      );
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setBusy(false);
    }
  }, [activePath, replaceTabWithReadResult]);

  const reloadActive = useCallback(async () => {
    const tab = tabsRef.current.find((item) => item.path === activePath);
    if (!tab) return;
    if (
      tab.dirty &&
      !(await systemConfirm(tm("files.preview.discard_changes.message", "This preview has edits that have not been saved to the original file."), {
        title: tm("files.preview.discard_changes.title", "Discard unsaved changes?"),
        kind: "warning",
        okLabel: tm("files.preview.reload", "Reload"),
        cancelLabel: tm("common.cancel", "Cancel"),
      }))
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await readFile(tab.rootPath, tab.path);
      setTabs((current) =>
        current.map((item) =>
          item.path === tab.path
            ? replaceTabWithReadResult(item, result)
            : item,
        ),
      );
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    } finally {
      setBusy(false);
    }
  }, [activePath, replaceTabWithReadResult]);

  const checkFileForExternalChanges = useCallback(async (path: string) => {
    const tab = tabsRef.current.find((item) => item.path === path);
    if (!tab || tab.readOnly || !window.__TAURI_INTERNALS__) return;
    try {
      const result = await readFile(tab.rootPath, tab.path);
      const changed = result.modifiedAt !== tab.modifiedAt || result.size !== tab.size;
      if (!changed) return;
      if (result.content === tab.content && result.isBinary === tab.isBinary) {
        setTabs((current) =>
          current.map((item) =>
            item.path === tab.path
              ? {
                  ...item,
                  ...result,
                  language: languageForPath(result.path),
                  savedContent: result.content,
                  dirty: false,
                  externalModifiedAt: undefined,
                  externalSize: undefined,
                }
              : item,
          ),
        );
        return;
      }
      if (!tab.dirty) {
        setTabs((current) =>
          current.map((item) =>
            item.path === tab.path ? replaceTabWithReadResult(item, result) : item,
          ),
        );
        return;
      }
      if (
        tab.dirty &&
        tab.externalModifiedAt === result.modifiedAt &&
        tab.externalSize === result.size
      ) {
        return;
      }

      if (tab.dirty) {
        setTabs((current) =>
          current.map((item) =>
            item.path === tab.path
              ? {
                  ...item,
                  externalModifiedAt: result.modifiedAt,
                  externalSize: result.size,
                }
              : item,
          ),
        );
        const reload = await systemConfirm(
          formatI18n(tm("files.preview.external_reload.message_format", "\"%@\" was changed outside the app. Reload the version on disk?"), tab.name),
          {
            title: tm("files.preview.external_reload.title", "File Changed on Disk"),
            kind: "warning",
            okLabel: tm("files.preview.reload", "Reload"),
            cancelLabel: tm("files.preview.external_reload.keep", "Keep Current Content"),
          },
        );
        if (!reload) return;
      }

      setTabs((current) =>
        current.map((item) =>
          item.path === tab.path ? replaceTabWithReadResult(item, result) : item,
        ),
      );
    } catch (nextError) {
      setError(nextError instanceof Error ? nextError.message : String(nextError));
    }
  }, [replaceTabWithReadResult]);

  useEffect(
    () =>
      listenWorkspaceCommand((command) => {
        if (command.type !== "open-file" && !project) {
          return;
        }
        if (command.type === "open-file") {
          void openFileInEditor(command.rootPath, command.path);
          return;
        }
        if (!project) {
          return;
        }
        if (command.type === "editor-save") {
          void saveActive();
          return;
        }
        if (command.type === "editor-search") {
          editorRef.current?.openSearch();
          return;
        }
        if (command.type === "close-active") {
          if (activePath) {
            void closeTab(activePath);
          }
        }
      }),
    [activePath, closeTab, openFileInEditor, project, saveActive],
  );

  useEffect(() => {
    return registerShortcutHandler("workspace", (event, context) => {
      if (context.mainView !== "files") return false;
      if (isConfiguredShortcut(event, "editor.save")) {
        event.preventDefault();
        void saveActive();
        return true;
      }
      if (isConfiguredShortcut(event, "editor.search")) {
        event.preventDefault();
        editorRef.current?.openSearch();
        return true;
      }
      if (isConfiguredShortcut(event, "close.active")) {
        if (activePath) {
          void closeTab(activePath);
        }
        return true;
      }
      return false;
    });
  }, [activePath, closeTab, saveActive]);

  useEffect(() => {
    if (!activePath) return;
    void checkFileForExternalChanges(activePath);
  }, [activePath, checkFileForExternalChanges]);

  useEffect(() => {
    if (!project?.path || !window.__TAURI_INTERNALS__) return;
    const projectPath = project.path;
    const pendingPaths = new Set<string>();
    let cancelled = false;
    let debounceTimer: number | undefined;
    let unlisten: (() => void) | undefined;
    let didUnlisten = false;
    const stopListening = (nextUnlisten: () => void) => {
      if (didUnlisten) return;
      didUnlisten = true;
      nextUnlisten();
    };

    const flush = () => {
      const paths = Array.from(pendingPaths);
      pendingPaths.clear();
      for (const path of paths) {
        void checkFileForExternalChanges(path);
      }
    };

    const unlistenPromise = listen<FileChangeEvent>("file:changed", (event) => {
      if (cancelled) return;
      for (const tab of tabsRef.current) {
        if (fileChangeTouchesTab(event.payload, tab)) {
          pendingPaths.add(tab.path);
        }
      }
      if (pendingPaths.size === 0) return;
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(flush, FILE_WATCH_DEBOUNCE_MS);
    });

    unlistenPromise
      .then((nextUnlisten) => {
        if (cancelled) {
          stopListening(nextUnlisten);
          return;
        }
        unlisten = () => stopListening(nextUnlisten);
      })
      .catch((nextError) => {
        const message = nextError instanceof Error ? nextError.message : String(nextError);
        setError(message);
      });

    void watchProjectFiles(projectPath).catch((nextError) => {
      if (cancelled) return;
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      setError(message);
    });

    return () => {
      cancelled = true;
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      if (unlisten) {
        unlisten();
      } else {
        void unlistenPromise.then((nextUnlisten) => stopListening(nextUnlisten)).catch(() => undefined);
      }
      void unwatchProjectFiles(projectPath).catch(() => undefined);
    };
  }, [checkFileForExternalChanges, project?.path]);

  useEffect(() => {
    if (!activePath) return;
    const checkActive = () => {
      const currentPath = activePath;
      if (document.visibilityState === "hidden") return;
      void checkFileForExternalChanges(currentPath);
    };
    window.addEventListener("focus", checkActive);
    document.addEventListener("visibilitychange", checkActive);
    return () => {
      window.removeEventListener("focus", checkActive);
      document.removeEventListener("visibilitychange", checkActive);
    };
  }, [activePath, checkFileForExternalChanges]);

  if (!project) {
    return (
      <EditorEmptyState
        title={tm("files.panel.no_project", "No Project Selected")}
        description={tm("files.panel.no_project.help", "Select or add a project to browse its files.")}
      />
    );
  }

  return (
    <div className="h-full grid grid-rows-[46px_minmax(0,1fr)] min-w-0">
      <TabStrip
        className="h-[46px] border-b border-line bg-transparent"
        items={tabs.map((tab) => ({
          id: tab.path,
          label: `${tab.name}${tab.dirty ? " •" : ""}`,
          icon: FileText,
          closable: true,
        }))}
        activeId={active?.path ?? ""}
        emptyLabel={isBusy ? tm("common.processing", "Processing") : tm("files.panel.title", "Files")}
        onSelect={(path) => {
          setActivePath(path);
          void checkFileForExternalChanges(path);
        }}
        onClose={(path) => void closeTab(path)}
      />
      {active ? (
        <FileEditor
          tab={active}
          editorRef={editorRef}
          isBusy={isBusy}
          error={error}
          onChange={updateActiveContent}
          onScrollChange={(scrollTop) => {
            setTabs((current) =>
              current.map((tab) =>
                tab.path === active.path && tab.rootPath === active.rootPath
                  ? { ...tab, scrollTop }
                  : tab,
              ),
            );
          }}
          onSave={() => void saveActive()}
          onReload={() => void reloadActive()}
          onReveal={() => void revealFile(active.rootPath, active.path)}
          onCopyPath={() => void navigator.clipboard?.writeText(active.path)}
        />
      ) : (
        <EditorEmptyState
          title={tm("files.panel.title", "Files")}
          description={tm("files.panel.no_project.help", "Select or add a project to browse its files.")}
        />
      )}
    </div>
  );
}

function FileEditor({
  tab,
  editorRef,
  isBusy,
  error,
  onChange,
  onScrollChange,
  onSave,
  onReload,
  onReveal,
  onCopyPath,
}: {
  tab: OpenFileTab;
  editorRef: MutableRefObject<CodeEditorHandle | null>;
  isBusy: boolean;
  error: string | null;
  onChange: (value: string) => void;
  onScrollChange: (scrollTop: number) => void;
  onSave: () => void;
  onReload: () => void;
  onReveal: () => void;
  onCopyPath: () => void;
}) {
  const blockedMessage = tab.message || (tab.isBinary ? tm("files.preview.binary", "Binary files cannot be previewed here.") : "");
  const hasExternalChange = tab.externalModifiedAt != null || tab.externalSize != null;
  const parentDirectory = relativeParentDirectory(tab.relativePath || tab.path, tab.name) || "/";
  const [scrollInfo, setScrollInfo] = useState<CodeEditorScrollInfo>({
    ratio: 0,
    scrollTop: 0,
    scrollHeight: 0,
    clientHeight: 0,
  });
  useEffect(() => {
    setScrollInfo({
      ratio: 0,
      scrollTop: 0,
      scrollHeight: 0,
      clientHeight: 0,
    });
  }, [tab.path, tab.version]);
  return (
    <div className="relative h-full min-h-0 overflow-hidden">
      <div className="h-[56px] flex items-center justify-between gap-4 px-[18px] border-b border-line">
        <div className="leading-tight min-w-0">
          <div className="text-sm font-semibold truncate">
            {tab.name}
            {tab.dirty && <span className="ml-1 text-brand-amber">•</span>}
          </div>
          <div className="text-xs text-ink-faint truncate">{parentDirectory}</div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0">
          <EditorBtn icon={CheckCircle2} tooltip={tm("files.preview.save", "Save")} onPress={onSave} disabled={!tab.dirty || tab.readOnly || isBusy} />
          <EditorBtn icon={Undo2} tooltip={tm("files.preview.undo", "Undo")} onPress={() => editorRef.current?.undo()} disabled={tab.readOnly} />
          <EditorBtn icon={Redo2} tooltip={tm("files.preview.redo", "Redo")} onPress={() => editorRef.current?.redo()} disabled={tab.readOnly} />
          <EditorBtn icon={Search} tooltip={tm("files.preview.find", "Find")} onPress={() => editorRef.current?.openSearch()} />
          <EditorBtn icon={Copy} tooltip={tm("files.preview.copy_path", "Copy Path")} onPress={onCopyPath} />
          <EditorBtn icon={RotateCcw} tooltip={tm("files.preview.reload", "Reload")} onPress={onReload} disabled={isBusy} />
          <EditorBtn icon={Folder} tooltip={tm("files.preview.reveal_finder", "Reveal in Finder")} onPress={onReveal} />
        </div>
      </div>

      {error && (
        <div className="absolute z-30 top-[64px] left-4 right-4 rounded-md border border-brand-red/30 bg-brand-red/12 px-3 py-2 text-xs text-brand-red">
          {error}
        </div>
      )}

      {hasExternalChange && !error && (
        <div className="absolute z-20 top-[64px] left-4 right-4 flex items-center justify-between gap-3 rounded-md border border-brand-amber/35 bg-brand-amber/12 px-3 py-2 text-xs text-ink-soft shadow-pop">
          <span className="min-w-0 truncate">
            {tm("files.preview.external_kept", "This file changed on disk. Current editor content is being kept until you reload or save.")}
          </span>
          <PressableButton
            className="flex-shrink-0 rounded px-2 py-1 font-semibold text-brand-amber hover:bg-brand-amber/12"
            onPressUp={onReload}
          >
            {tm("files.preview.reload", "Reload")}
          </PressableButton>
        </div>
      )}

      <div className="grid grid-cols-[minmax(0,1fr)_84px] h-[calc(100%-56px)] min-h-0">
        {tab.isBinary ? (
          <EditorEmptyState title={tm("files.preview.read_error", "Could not read this file.")} description={blockedMessage} compact />
        ) : (
          <CodeEditor
            ref={editorRef}
            documentKey={`${tab.path}:${tab.version}`}
            value={tab.content}
            language={tab.language}
            readOnly={tab.readOnly}
            onChange={onChange}
            initialScrollTop={tab.scrollTop ?? 0}
            onScrollInfoChange={(info) => {
              setScrollInfo(info);
              onScrollChange(info.scrollTop);
            }}
          />
        )}
        <EditorMinimap
          content={tab.content}
          disabled={tab.isBinary}
          scrollInfo={scrollInfo}
          onJump={(ratio) => editorRef.current?.scrollToRatio(ratio)}
        />
      </div>
      {blockedMessage && !tab.isBinary && (
        <div className="absolute bottom-3 left-4 rounded-md border border-line bg-surface-panel/95 px-2.5 py-1.5 text-xs text-ink-mute shadow-pop">
          {blockedMessage}
        </div>
      )}
    </div>
  );
}

function EditorBtn({
  icon: Icon,
  tooltip,
  onPress,
  disabled,
}: {
  icon: typeof Search;
  tooltip: string;
  onPress?: () => void;
  disabled?: boolean;
}) {
  return (
    <Tooltip label={tooltip} placement="bottom">
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={tooltip}
        className="h-[28px] w-[28px] min-w-[28px] text-ink-mute"
        onPress={onPress}
        disabled={disabled}
      >
        <Icon size={14} strokeWidth={2} />
      </Button>
    </Tooltip>
  );
}

function EditorEmptyState({
  title,
  description,
  compact,
}: {
  title: string;
  description: string;
  compact?: boolean;
}) {
  return (
    <div className={`${compact ? "h-full" : "h-full"} grid place-items-center bg-surface-editor text-center px-6`}>
      <div>
        <div className="w-11 h-11 mx-auto rounded-[10px] border border-line bg-fill/[0.04] grid place-items-center text-ink-mute">
          <FileText size={18} />
        </div>
        <div className="mt-3 text-sm font-semibold text-ink">{title}</div>
        <div className="mt-1 text-xs text-ink-mute max-w-[260px] leading-relaxed">{description}</div>
      </div>
    </div>
  );
}

function EditorMinimap({
  content,
  disabled,
  scrollInfo,
  onJump,
}: {
  content: string;
  disabled: boolean;
  scrollInfo: CodeEditorScrollInfo;
  onJump: (ratio: number) => void;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const lines = useMemo(() => minimapLines(content), [content]);
  const canScroll = !disabled && scrollInfo.scrollHeight > scrollInfo.clientHeight + 1;
  const viewportRatio =
    scrollInfo.scrollHeight > 0
      ? Math.min(1, scrollInfo.clientHeight / scrollInfo.scrollHeight)
      : 1;
  const thumbHeight = canScroll ? Math.max(12, Math.min(62, viewportRatio * 100)) : 0;
  const thumbTop = Math.max(0, Math.min(100 - thumbHeight, scrollInfo.ratio * (100 - thumbHeight)));
  const jump = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (disabled || !canScroll) return;
    const rect = hostRef.current?.getBoundingClientRect();
    if (!rect || rect.height <= 0) return;
    onJump((event.clientY - rect.top) / rect.height);
  };
  return (
    <div
      ref={hostRef}
      className={`relative h-full min-h-0 overflow-hidden border-l border-line bg-surface-editor ${
        disabled ? "opacity-40" : canScroll ? "cursor-pointer" : ""
      }`}
      onPointerDown={jump}
      onPointerMove={(event) => {
        if (event.buttons === 1) jump(event);
      }}
    >
      <div className="absolute inset-x-2.5 top-2 bottom-2 overflow-hidden opacity-80">
        <div className="grid gap-[2px]">
          {lines.map((line, index) => (
            <div
              key={`${index}:${line.width}`}
              className={`h-px rounded-full ${line.tone}`}
              style={{ width: `${line.width}%`, marginLeft: `${line.indent}%` }}
            />
          ))}
        </div>
      </div>
      {!disabled && canScroll && (
        <div
          className="absolute left-2 right-2 rounded-[3px] bg-brand-blue/20"
          style={{
            top: `${thumbTop}%`,
            height: `${thumbHeight}%`,
          }}
        />
      )}
    </div>
  );
}

function minimapLines(content: string) {
  const rawLines = content.split(/\r?\n/);
  const maxLines = 260;
  const step = Math.max(1, Math.ceil(rawLines.length / maxLines));
  const result: { width: number; indent: number; tone: string }[] = [];
  for (let index = 0; index < rawLines.length; index += step) {
    const line = rawLines[index] ?? "";
    const trimmed = line.trim();
    const indentChars = Math.min(18, Math.max(0, line.length - line.trimStart().length));
    const contentChars = Math.max(1, Math.min(80, trimmed.length));
    result.push({
      indent: Math.min(26, indentChars * 1.8),
      width: trimmed.length === 0 ? 8 : Math.max(12, Math.min(92, contentChars * 1.15)),
      tone: minimapTone(trimmed),
    });
  }
  return result.length > 0 ? result : [{ indent: 0, width: 10, tone: "bg-fill/[0.12]" }];
}

function minimapTone(line: string) {
  if (!line) return "bg-fill/[0.10]";
  if (/^(\/\/|#|\/\*|\*)/.test(line)) return "bg-ink-faint/55";
  if (/\b(import|export|package|use|from|require)\b/.test(line)) return "bg-brand-pink/70";
  if (/\b(function|func|class|struct|enum|interface|const|let|var)\b/.test(line)) return "bg-brand-blue/70";
  if (/[`'"]/.test(line)) return "bg-brand-green/65";
  if (/\b(return|if|else|for|while|switch|match|case)\b/.test(line)) return "bg-brand-amber/68";
  return "bg-fill/[0.22]";
}

function ReviewMode({ project }: { project?: WorkspaceProject }) {
  const baseBranch = project?.isDefaultWorktree ? null : project?.baseBranch;
  const review = useGitReviewSnapshot(project?.path, baseBranch);
  const snapshot = review.snapshot;
  const [selectedPath, setSelectedPath] = useState("");
  const [expandedReviewPaths, setExpandedReviewPaths] = useState<Set<string>>(new Set());
  const [content, setContent] = useState<GitReviewContentSnapshot | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [reviewScrollTop, setReviewScrollTop] = useState(0);
  const reviewScrollSourceRef = useRef("");
  const reviewTree = useMemo(() => buildReviewTree(snapshot.files), [snapshot.files]);
  const reviewDirectoryPaths = useMemo(() => collectReviewDirectoryPaths(reviewTree), [reviewTree]);
  const reviewTreeInitializedRef = useRef(false);
  const previousReviewDirectoryPathsRef = useRef<Set<string>>(new Set());
  const selectedFile = useMemo(() => {
    if (snapshot.files.length === 0) return undefined;
    return snapshot.files.find((file) => file.path === selectedPath) ?? snapshot.files[0];
  }, [selectedPath, snapshot.files]);
  const title = snapshot.mode === "taskBranch"
    ? tm("worktree.review.title", "Worktree Review")
    : tm("worktree.review.audit_title", "Uncommitted Audit");
  const subtitle =
    snapshot.mode === "taskBranch" && snapshot.baseBranch
      ? `${project?.branch ?? "HEAD"} ← ${snapshot.baseBranch}`
      : project?.path ?? tm("worktree.review.audit_working_tree", "Working Tree");
  const totalAdditions = snapshot.files.reduce((sum, file) => sum + file.additions, 0);
  const totalDeletions = snapshot.files.reduce((sum, file) => sum + file.deletions, 0);
  const language = languageForPath(selectedFile?.path ?? "");

  useEffect(() => {
    reviewTreeInitializedRef.current = false;
    previousReviewDirectoryPathsRef.current = new Set();
    setExpandedReviewPaths(new Set());
  }, [project?.path, snapshot.baseBranch, snapshot.mode]);

  useEffect(() => {
    const validPaths = new Set(reviewDirectoryPaths);
    const previousPaths = previousReviewDirectoryPathsRef.current;
    setExpandedReviewPaths((current) => {
      if (!reviewTreeInitializedRef.current) {
        reviewTreeInitializedRef.current = true;
        return validPaths;
      }
      const next = new Set<string>();
      for (const path of current) {
        if (validPaths.has(path)) next.add(path);
      }
      for (const path of validPaths) {
        if (!previousPaths.has(path)) {
          next.add(path);
        }
      }
      return next;
    });
    previousReviewDirectoryPathsRef.current = validPaths;
  }, [reviewDirectoryPaths]);

  useEffect(() => {
    setSelectedPath((current) => {
      if (current && snapshot.files.some((file) => file.path === current)) return current;
      return snapshot.files[0]?.path ?? "";
    });
  }, [snapshot.files]);

  useEffect(() => {
    if (!project?.path || !selectedFile) {
      setContent(null);
      setDiffError(null);
      return;
    }
    let disposed = false;
    setDiffError(null);
    void loadGitReviewFileContent(project.path, selectedFile.path, snapshot.baseBranch)
      .then((nextContent) => {
        if (disposed) return;
        setContent(nextContent);
        setDiffError(nextContent.error ?? null);
      })
      .catch((reason) => {
        if (disposed) return;
        setContent(null);
        setDiffError(reason instanceof Error ? reason.message : String(reason));
      });
    return () => {
      disposed = true;
    };
  }, [project?.path, selectedFile, snapshot.baseBranch]);

  useEffect(() => {
    reviewScrollSourceRef.current = "";
    setReviewScrollTop(0);
  }, [selectedFile?.path, snapshot.baseBranch]);

  const handleReviewColumnScroll = useCallback((source: string, info: CodeEditorScrollInfo) => {
    reviewScrollSourceRef.current = source;
    setReviewScrollTop(info.scrollTop);
  }, []);

  return (
    <div className="h-full min-h-0 grid grid-rows-[auto_minmax(0,1fr)_auto] bg-surface-editor">
      <section className="flex items-center justify-between gap-4 border-b border-line bg-fill/[0.025] px-5 py-3.5">
        <div className="min-w-0 flex items-center gap-3">
          <span className="grid place-items-center w-8 h-8 rounded-[8px] bg-brand-blue/15 text-brand-blue">
            <GitPullRequest size={15} />
          </span>
          <div className="min-w-0">
            <div className="text-sm font-semibold">{title}</div>
            <div className="text-xs text-ink-mute mt-0.5 truncate">
              {review.isLoading ? tm("worktree.review.loading", "Loading review.") : snapshot.diffStat || subtitle}
            </div>
          </div>
        </div>
        <div className="hidden md:flex items-center gap-2 text-xs text-ink-faint">
          <ReviewMetric label={tm("worktree.review.changed_files", "Changed Files")} value={snapshot.files.length} />
          <ReviewMetric label="+" value={totalAdditions} tone="green" />
          <ReviewMetric label="-" value={totalDeletions} tone="red" />
        </div>
      </section>
      <div className="min-h-0 grid grid-cols-[280px_minmax(0,1fr)]">
        <div className="min-h-0 overflow-y-auto scrollbar-overlay border-r border-line bg-fill/[0.018] p-2">
          {snapshot.files.length > 0 ? (
            reviewTree.map((node) => (
              <ReviewTreeRow
                key={`${node.kind}:${node.path}`}
                node={node}
                depth={0}
                projectPath={project?.path}
                selectedPath={selectedFile?.path}
                expandedPaths={expandedReviewPaths}
                onToggleDirectory={(path) => {
                  setExpandedReviewPaths((current) => {
                    const next = new Set(current);
                    if (next.has(path)) {
                      next.delete(path);
                    } else {
                      next.add(path);
                    }
                    return next;
                  });
                }}
                onSelectFile={setSelectedPath}
              />
            ))
          ) : (
            <div className="rounded-md border border-line bg-fill/[0.025] px-3 py-3 text-sm text-ink-faint">
              {!snapshot.isRepository
                ? tm("worktree.review.no_repository", "No Git repository.")
                : snapshot.error || review.error || tm("worktree.review.no_changes", "No changes relative to the base branch.")}
            </div>
          )}
        </div>
        <div className="min-h-0 min-w-0 grid grid-rows-[38px_minmax(0,1fr)]">
          <div className="min-w-0 flex items-center justify-between gap-3 border-b border-line px-3 text-xs text-ink-mute">
            <span className="truncate">{selectedFile?.path ?? tm("worktree.review.check.diff", "Diff")}</span>
            {selectedFile && (
              <PressableButton
                className="h-6 rounded-md px-2 text-ink-soft hover:bg-fill/[0.06] hover:text-ink"
                onPressUp={() => {
                  if (!project?.path || !selectedFile) return;
                  broadcastWorkspaceCommand({
                    type: "open-file",
                    rootPath: project.path,
                    path: selectedFile.path,
                  });
                }}
              >
                {tm("files.panel.open", "Open")}
              </PressableButton>
            )}
          </div>
          {diffError ? (
            <EditorEmptyState title={tm("git.diff.empty", "No diff to display")} description={diffError} compact />
          ) : selectedFile ? (
            <div className={`min-h-0 min-w-0 grid ${snapshot.mode === "taskBranch" ? "grid-cols-4" : "grid-cols-3"}`}>
              <ReviewCodeColumn
                id="original"
                title={tm("worktree.review.column.original", "Original")}
                subtitle={snapshot.mode === "taskBranch" ? snapshot.baseBranch ?? tm("worktree.task.base_branch", "Base Branch") : "HEAD"}
                value={snapshot.mode === "taskBranch" ? content?.baseContent ?? "" : content?.headContent ?? ""}
                language={language}
                lineHighlights={deletedLineHighlights(content)}
                scrollTop={reviewScrollTop}
                scrollSource={reviewScrollSourceRef.current}
                onScroll={handleReviewColumnScroll}
              />
              <ReviewCodeColumn
                id="new"
                title={tm("worktree.review.column.new", "New File")}
                subtitle={content?.indexContent != null ? tm("git.files.staged", "Staged") : tm("worktree.review.audit_working_tree", "Working Tree")}
                value={content?.indexContent ?? content?.worktreeContent ?? ""}
                language={language}
                lineHighlights={addedLineHighlights(content)}
                scrollTop={reviewScrollTop}
                scrollSource={reviewScrollSourceRef.current}
                onScroll={handleReviewColumnScroll}
              />
              <ReviewCodeColumn
                id="final"
                title={tm("worktree.review.column.final", "Final File")}
                subtitle={tm("worktree.review.audit_working_tree", "Working Tree")}
                value={content?.worktreeContent ?? ""}
                language={language}
                lineHighlights={addedLineHighlights(content)}
                scrollTop={reviewScrollTop}
                scrollSource={reviewScrollSourceRef.current}
                onScroll={handleReviewColumnScroll}
              />
              {snapshot.mode === "taskBranch" && (
                <ReviewCodeColumn
                  id="branch"
                  title={tm("worktree.review.column.branch", "Branch")}
                  subtitle={project?.branch ?? "HEAD"}
                  value={content?.headContent ?? ""}
                  language={language}
                  lineHighlights={addedLineHighlights(content)}
                  scrollTop={reviewScrollTop}
                  scrollSource={reviewScrollSourceRef.current}
                  onScroll={handleReviewColumnScroll}
                />
              )}
            </div>
          ) : (
            <EditorEmptyState
              title={review.isLoading ? tm("git.diff.loading", "Loading diff...") : tm("git.diff.empty", "No diff to display")}
              description={selectedFile ? tm("git.diff.empty_description", "This file did not produce diff content.") : tm("worktree.review.select_file", "Select a changed file to compare.")}
              compact
            />
          )}
        </div>
      </div>
      <div className="h-[34px] flex items-center justify-between border-t border-line bg-fill/[0.025] px-3 text-xs text-ink-faint">
        <span className="truncate">{subtitle}</span>
        <span className="tabular-nums">
          {snapshot.files.length} · <span className="text-brand-green">+{totalAdditions}</span> · <span className="text-brand-red">-{totalDeletions}</span>
        </span>
      </div>
    </div>
  );
}

function ReviewMetric({
  label,
  value,
  tone = "neutral",
}: {
  label: string;
  value: number;
  tone?: "neutral" | "green" | "red";
}) {
  const toneClass =
    tone === "green" ? "text-brand-green" : tone === "red" ? "text-brand-red" : "text-ink-soft";
  return (
    <div className="h-7 min-w-[58px] rounded-[7px] border border-line bg-fill/[0.025] px-2 grid content-center">
      <span className={`text-sm font-semibold leading-none tabular-nums ${toneClass}`}>{value}</span>
      <span className="mt-0.5 text-[10px] leading-none text-ink-faint">{label}</span>
    </div>
  );
}

function ReviewCodeColumn({
  id,
  title,
  subtitle,
  value,
  language,
  lineHighlights = [],
  scrollTop,
  scrollSource,
  onScroll,
}: {
  id: string;
  title: string;
  subtitle: string;
  value: string;
  language: string;
  lineHighlights?: CodeEditorLineHighlight[];
  scrollTop: number;
  scrollSource: string;
  onScroll: (source: string, info: CodeEditorScrollInfo) => void;
}) {
  return (
    <div className="min-h-0 min-w-0 grid grid-rows-[40px_minmax(0,1fr)] border-r border-line last:border-r-0 bg-surface-editor">
      <div className="min-w-0 border-b border-line bg-fill/[0.02] px-2.5 py-1.5">
        <div className="truncate text-xs font-semibold text-ink-soft">{title}</div>
        <div className="truncate text-[10.5px] text-ink-faint">{subtitle}</div>
      </div>
      {value ? (
        <CodeEditor
          documentKey={`${id}:${language}:${value.length}:${value.slice(0, 80)}`}
          value={value}
          language={language}
          readOnly
          lineHighlights={lineHighlights}
          onChange={() => undefined}
          onScrollInfoChange={(info) => onScroll(id, info)}
          silentScrollTop={scrollSource && scrollSource !== id ? scrollTop : undefined}
        />
      ) : (
        <EditorEmptyState
          title={tm("git.diff.empty", "No diff to display")}
          description={tm("worktree.review.column.empty", "This version has no readable content.")}
          compact
        />
      )}
    </div>
  );
}

function addedLineHighlights(content: GitReviewContentSnapshot | null): CodeEditorLineHighlight[] {
  return (content?.addedLines ?? []).map((line) => ({ line, tone: "add" }));
}

function deletedLineHighlights(content: GitReviewContentSnapshot | null): CodeEditorLineHighlight[] {
  return (content?.deletedLines ?? []).map((line) => ({ line, tone: "delete" }));
}

type ReviewTreeNode = ReviewTreeDirectory | ReviewTreeFile;

type ReviewTreeDirectory = {
  kind: "directory";
  path: string;
  name: string;
  additions: number;
  deletions: number;
  children: ReviewTreeNode[];
};

type ReviewTreeFile = {
  kind: "file";
  path: string;
  name: string;
  file: GitReviewFile;
};

function ReviewTreeRow({
  node,
  depth,
  projectPath,
  selectedPath,
  expandedPaths,
  onToggleDirectory,
  onSelectFile,
}: {
  node: ReviewTreeNode;
  depth: number;
  projectPath?: string;
  selectedPath?: string;
  expandedPaths: Set<string>;
  onToggleDirectory: (path: string) => void;
  onSelectFile: (path: string) => void;
}) {
  if (node.kind === "file") {
    return (
      <ReviewFileRow
        file={node.file}
        displayName={node.name}
        depth={depth}
        projectPath={projectPath}
        selected={selectedPath === node.path}
        onSelect={() => onSelectFile(node.path)}
      />
    );
  }
  const expanded = expandedPaths.has(node.path);
  return (
    <>
      <ReviewDirectoryRow node={node} depth={depth} expanded={expanded} onToggle={() => onToggleDirectory(node.path)} />
      {expanded && node.children.map((child) => (
        <ReviewTreeRow
          key={`${child.kind}:${child.path}`}
          node={child}
          depth={depth + 1}
          projectPath={projectPath}
          selectedPath={selectedPath}
          expandedPaths={expandedPaths}
          onToggleDirectory={onToggleDirectory}
          onSelectFile={onSelectFile}
        />
      ))}
    </>
  );
}

function ReviewDirectoryRow({
  node,
  depth,
  expanded,
  onToggle,
}: {
  node: ReviewTreeDirectory;
  depth: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <Tooltip label={node.path} placement="right" triggerClassName="block w-full">
      <PressableButton
        className="w-full h-8 grid grid-cols-[18px_16px_minmax(0,1fr)_auto] items-center gap-1.5 pr-2.5 rounded-md text-ink-soft text-sm hover:bg-fill/[0.045] hover:text-ink"
        style={{ paddingLeft: `${8 + depth * 14}px` }}
        onPressUp={onToggle}
      >
        {expanded ? <ChevronDown size={12} className="text-ink-faint" /> : <ChevronRight size={12} className="text-ink-faint" />}
        <Folder size={13} className="text-brand-blue/85" />
        <span className="min-w-0 truncate text-left text-xs font-medium">{node.name}</span>
        {(node.additions > 0 || node.deletions > 0) && (
          <span className="text-xs tabular-nums">
            <span className="text-brand-green">+{node.additions}</span>
            <span className="mx-1 text-ink-faint">/</span>
            <span className="text-brand-red">-{node.deletions}</span>
          </span>
        )}
      </PressableButton>
    </Tooltip>
  );
}

function ReviewFileRow({
  file,
  displayName,
  depth = 0,
  projectPath,
  selected,
  onSelect,
}: {
  file: GitReviewFile;
  displayName?: string;
  depth?: number;
  projectPath?: string;
  selected?: boolean;
  onSelect?: () => void;
}) {
  const badge = reviewFileStatusBadge(file.status);
  return (
    <Tooltip label={file.path} placement="right" triggerClassName="block w-full">
      <PressableButton
        className={`w-full h-8 grid grid-cols-[18px_minmax(0,1fr)_auto_auto] items-center gap-2.5 pr-2.5 rounded-md text-ink-soft text-sm hover:bg-fill/[0.045] ${
          selected ? "bg-brand-blue/12 text-ink" : ""
        }`}
        style={{ paddingLeft: `${8 + depth * 14}px` }}
        onPressUp={onSelect}
        onDoubleClick={() => {
          if (!projectPath) return;
          broadcastWorkspaceCommand({
            type: "open-file",
            rootPath: projectPath,
            path: file.path,
          });
        }}
      >
        <FileCode2 size={13} className="text-ink-mute" />
        <span className="min-w-0 truncate text-right text-xs" dir="rtl">{displayName ?? file.path}</span>
        {(file.additions > 0 || file.deletions > 0) && (
          <span className="text-xs tabular-nums">
            <span className="text-brand-green">+{file.additions}</span>
            <span className="mx-1 text-ink-faint">/</span>
            <span className="text-brand-red">-{file.deletions}</span>
          </span>
        )}
        <span className={`text-xs font-bold ${badge.tone}`}>{badge.label}</span>
      </PressableButton>
    </Tooltip>
  );
}

function reviewFileStatusBadge(status: GitReviewFile["status"]): { label: string; tone: string } {
  switch (status) {
    case "added":
      return { label: "A", tone: "text-brand-green" };
    case "deleted":
      return { label: "D", tone: "text-brand-blue" };
    case "renamed":
      return { label: "R", tone: "text-brand-amber" };
    case "copied":
      return { label: "C", tone: "text-brand-blue" };
    case "typeChanged":
      return { label: "T", tone: "text-brand-amber" };
    case "modified":
      return { label: "M", tone: "text-brand-amber" };
    default:
      return { label: "?", tone: "text-ink-faint" };
  }
}

function buildReviewTree(files: GitReviewFile[]): ReviewTreeNode[] {
  type MutableDirectory = {
    kind: "directory";
    path: string;
    name: string;
    additions: number;
    deletions: number;
    children: Map<string, MutableDirectory | ReviewTreeFile>;
  };
  const root: MutableDirectory = {
    kind: "directory",
    path: "",
    name: "",
    additions: 0,
    deletions: 0,
    children: new Map(),
  };

  for (const file of files) {
    const parts = file.path.split("/").filter(Boolean);
    if (parts.length === 0) continue;
    let directory = root;
    directory.additions += file.additions;
    directory.deletions += file.deletions;
    for (let index = 0; index < parts.length - 1; index += 1) {
      const name = parts[index];
      const path = parts.slice(0, index + 1).join("/");
      const existing = directory.children.get(path);
      let nextDirectory: MutableDirectory;
      if (existing?.kind === "directory") {
        nextDirectory = existing;
      } else {
        nextDirectory = {
          kind: "directory",
          path,
          name,
          additions: 0,
          deletions: 0,
          children: new Map(),
        };
        directory.children.set(path, nextDirectory);
      }
      nextDirectory.additions += file.additions;
      nextDirectory.deletions += file.deletions;
      directory = nextDirectory;
    }
    directory.children.set(file.path, {
      kind: "file",
      path: file.path,
      name: parts[parts.length - 1],
      file,
    });
  }

  const materialize = (directory: MutableDirectory): ReviewTreeNode[] =>
    Array.from(directory.children.values())
      .sort((left, right) => {
        if (left.kind !== right.kind) return left.kind === "directory" ? -1 : 1;
        return left.name.localeCompare(right.name);
      })
      .map((node) => {
        if (node.kind === "file") return node;
        return {
          kind: "directory",
          path: node.path,
          name: node.name,
          additions: node.additions,
          deletions: node.deletions,
          children: materialize(node),
        };
      });

  return materialize(root);
}

function collectReviewDirectoryPaths(nodes: ReviewTreeNode[]) {
  const paths: string[] = [];
  const visit = (node: ReviewTreeNode) => {
    if (node.kind !== "directory") return;
    paths.push(node.path);
    node.children.forEach(visit);
  };
  nodes.forEach(visit);
  return paths;
}
