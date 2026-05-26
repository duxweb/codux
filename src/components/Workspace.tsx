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
  Star,
  Undo2,
  Redo2,
  X,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { CSSProperties, MutableRefObject, PointerEvent as ReactPointerEvent, ReactNode } from "react";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { Button } from "./Button";
import { Checkbox, TextInput } from "./Form";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import { Dropdown } from "./Dropdown";
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
import { useGitStatusSnapshot } from "../git/status";
import type { CodeEditorLineHighlight } from "./CodeEditor";
import { isConfiguredShortcut, registerShortcutHandler } from "../shortcuts";
import {
  countTopSplits,
  equalRatios,
  normalizeRatios,
  restoreTerminalLayout,
  resolveActiveSlotId,
  snapshotTerminalLayout,
  terminalLayoutStore,
  resolveVisibleTerminalId,
  loadTerminalLayoutSnapshot,
  rememberTerminalLayoutSnapshot,
  type TerminalLayoutState,
} from "../terminalLayout";
import { terminalRuntime } from "../terminal/runtime";
import type { MainView, TerminalSession, WorkspaceProject } from "../types";
import { openDetachedTerminalWindow } from "../windowing";
import {
  broadcastWorkspaceCommand,
  clearPendingOpenFileCommand,
  clearPendingWorkspaceCommand,
  consumePendingOpenFileCommand,
  consumePendingWorkspaceCommands,
  listenWorkspaceCommand,
  type WorkspaceCommand,
  workspacePathsMatch,
} from "../workspaceCommands";
import { systemConfirm, systemMessage } from "../systemDialog";
import { formatI18n, tm } from "../i18n";
import {
  languageForPath,
  deleteFile,
  readFile,
  revealFile,
  unwatchProjectFiles,
  watchProjectFiles,
  writeFile,
  type FileChangeEvent,
  type FileReadResult,
} from "../files/api";
import { CodeEditor, type CodeEditorHandle, type CodeEditorScrollInfo, type CodeEditorSearchQuery } from "./CodeEditor";

const MAX_TERMINAL_SPLITS = 6;
const MIN_TOP_PANE_RATIO = 0.12;
const MIN_BOTTOM_RATIO = 0.18;
const MAX_BOTTOM_RATIO = 0.72;
const FALLBACK_PROJECT_ID = "00000000-0000-5000-8000-000000000001";
const FALLBACK_PROJECT_PATH = "/preview/workspace";
const FILE_WATCH_DEBOUNCE_MS = 180;
const DEFAULT_EDITOR_SEARCH_QUERY: CodeEditorSearchQuery = {
  search: "",
  replace: "",
  caseSensitive: false,
  regexp: false,
  wholeWord: false,
};

const writtenDeferredTerminalCommands = new Set<string>();

function writeDeferredTerminalCommand(terminalId: string, command: string) {
  const commandKey = `${terminalId}\u0000${command}`;
  if (writtenDeferredTerminalCommands.has(commandKey)) return;
  writtenDeferredTerminalCommands.add(commandKey);
  const writeCommand = () => terminalRuntime.write(terminalId, `${command}\r`);
  const session = terminalRuntime.getSession(terminalId);
  if (session?.state === "running") {
    writeCommand();
    return;
  }
  const unsubscribe = terminalRuntime.subscribe(terminalId, (event) => {
    if (event.type === "closed" || event.type === "output" || event.session.state !== "running") return;
    unsubscribe();
    writeCommand();
  });
}

type Props = {
  mainView: MainView;
  selectedProject?: WorkspaceProject;
  onSessionChange: (session: TerminalSession | null) => void;
  terminalFocusRequest?: number;
};

type TerminalLaunchOptions = {
  title?: string;
  label?: string;
  command?: string;
  deferredCommand?: string;
  tool?: string;
};

export function Workspace({ mainView, selectedProject, onSessionChange, terminalFocusRequest = 0 }: Props) {
  const canUsePreviewFallback = !window.__TAURI_INTERNALS__;
  const projectId = selectedProject?.id ?? (canUsePreviewFallback ? FALLBACK_PROJECT_ID : "");
  const cwd = selectedProject?.path ?? (canUsePreviewFallback ? FALLBACK_PROJECT_PATH : "");
  const fallbackProject = useMemo<WorkspaceProject | undefined>(() => {
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
  }, [canUsePreviewFallback, cwd, projectId, selectedProject]);
  const activeTerminalProject = fallbackProject;

  return (
    <section className="h-full overflow-hidden">
      {mainView === "terminal" && activeTerminalProject && (
        <div className="h-full">
          <TerminalMode
            key={activeTerminalProject.id}
            cwd={activeTerminalProject.path}
            projectId={activeTerminalProject.id}
            projectName={activeTerminalProject.name}
            visible
            acceptsWorkspaceCommands
            focusRequest={terminalFocusRequest}
            onSessionChange={onSessionChange}
          />
        </div>
      )}
      {mainView === "files" && (
        <div className="h-full">
          <FilesMode project={selectedProject} />
        </div>
      )}
      {mainView === "review" && (
        <div className="h-full">
          <ReviewMode project={selectedProject} />
        </div>
      )}
    </section>
  );
}

function TerminalMode({
  cwd,
  projectId,
  projectName,
  visible,
  acceptsWorkspaceCommands,
  focusRequest: externalFocusRequest,
  onSessionChange,
}: {
  cwd: string;
  projectId: string;
  projectName?: string;
  visible: boolean;
  acceptsWorkspaceCommands: boolean;
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
  const visibleTopPanes = useMemo(() => topPanes.filter((pane) => !pane.detached), [topPanes]);
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
    () => activeTerminalId || topPanes[0]?.terminalId || tabs.find((tab) => tab.id === activeTabId)?.terminalId || null,
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
      if (event.type === "state" || event.type === "reset") {
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
  }, [activeTabId, activeTerminalId, bottomRatio, isLayoutHydrated, projectId, tabs, topPanes, topRatios]);

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

  const closeTopPane = useCallback(
    (id: string) => {
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
    },
    [activateTerminal, activeTabId, activeTerminalId, tabs],
  );

  const detachTopPane = useCallback(
    (paneId: string) => {
      const pane = topPanes.find((item) => item.id === paneId);
      if (!pane || pane.detached) return;
      setTopPanes((current) => current.map((item) => (item.id === paneId ? { ...item, detached: true } : item)));
      if (activeTerminalId === pane.terminalId) {
        const nextPane = topPanes.find((item) => item.id !== paneId && !item.detached);
        activateTerminal(nextPane?.terminalId ?? tabs.find((tab) => tab.id === activeTabId)?.terminalId ?? "");
      }
      void openTerminalWindow(pane.terminalId, pane.id);
    },
    [activateTerminal, activeTabId, activeTerminalId, tabs, topPanes],
  );

  const addTopPane = useCallback(
    (launch?: TerminalLaunchOptions) => {
      setTopPanes((current) => {
        if (countTopSplits({ topPanes: current }) >= MAX_TERMINAL_SPLITS) {
          return current;
        }
        const id = nextTerminalPaneId(
          "top",
          current.map((pane) => pane.id),
        );
        const title = launch?.title ?? formatI18n(tm("workspace.split_format", "Split %@"), current.length + 1);
        const terminalId = ensureTerminal(id, title, launch).id;
        setTopRatios(equalRatios(current.length + 1));
        activateTerminal(terminalId);
        if (launch?.deferredCommand) {
          writeDeferredTerminalCommand(terminalId, launch.deferredCommand);
        }
        window.requestAnimationFrame(() => window.dispatchEvent(new Event("resize")));
        return [
          ...current,
          {
            id,
            title,
            terminalId,
          },
        ];
      });
    },
    [activateTerminal, ensureTerminal],
  );

  const addBottomTab = useCallback(
    (launch?: TerminalLaunchOptions) => {
      setTabs((current) => {
        const id = nextTerminalPaneId(
          "bottom",
          current.map((tab) => tab.id),
        );
        const label = launch?.label ?? formatI18n(tm("workspace.tab_format", "Tab %@"), current.length + 1);
        setActiveTabId(id);
        const terminalId = ensureTerminal(id, label, launch).id;
        activateTerminal(terminalId);
        if (launch?.deferredCommand) {
          writeDeferredTerminalCommand(terminalId, launch.deferredCommand);
        }
        return [
          ...current,
          {
            id,
            label,
            terminalId,
          },
        ];
      });
    },
    [activateTerminal, ensureTerminal],
  );

  const handleTerminalWorkspaceCommand = useCallback(
    (command: WorkspaceCommand) => {
      if (command.type === "add-top-terminal-split") {
        if (!acceptsWorkspaceCommands && command.projectId !== projectId) return false;
        addTopPane({
          title: command.title,
          command: command.command,
          deferredCommand: command.deferredCommand,
          tool: command.command || command.deferredCommand ? "manual" : undefined,
        });
        return true;
      }
      if (command.type === "add-bottom-terminal-tab") {
        if (!acceptsWorkspaceCommands && command.projectId !== projectId) return false;
        addBottomTab({
          label: command.label,
          title: command.label,
          command: command.command,
          deferredCommand: command.deferredCommand,
          tool: command.command || command.deferredCommand ? "manual" : undefined,
        });
        return true;
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
        if (!terminalId) return false;
        terminalRuntime.write(terminalId, command.text);
        activateTerminal(terminalId, { focus: true });
        return true;
      }
      if (command.type === "reattach-terminal-pane") {
        setTopPanes((current) =>
          current.map((pane) =>
            pane.id === command.paneId && pane.terminalId === command.terminalId
              ? { ...pane, detached: false }
              : pane,
          ),
        );
        return true;
      }
      return false;
    },
    [
      acceptsWorkspaceCommands,
      activateTerminal,
      activeTabId,
      activeTerminalId,
      addBottomTab,
      addTopPane,
      projectId,
      tabs,
      topPanes,
    ],
  );

  useEffect(
    () =>
      listenWorkspaceCommand((command) => {
        if (handleTerminalWorkspaceCommand(command)) {
          clearPendingWorkspaceCommand(command);
        }
      }),
    [handleTerminalWorkspaceCommand],
  );

  useEffect(() => {
    for (const command of consumePendingWorkspaceCommands(handleTerminalWorkspaceCommand)) {
      void command;
    }
  }, [handleTerminalWorkspaceCommand]);

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
  }, [activateTerminal, activeTabId, activeTerminalId, externalFocusRequest, tabs, topPanes, visible]);

  const closeBottomTab = useCallback(
    (id: string) => {
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
    },
    [activateTerminal, activeTabId, activeTerminalId, visibleTopPanes],
  );

  const renameBottomTab = useCallback((id: string, label: string) => {
    setTabs((current) => current.map((tab) => (tab.id === id ? { ...tab, label } : tab)));
  }, []);

  const reorderBottomTab = useCallback((sourceId: string, targetId: string) => {
    setTabs((current) => reorderById(current, sourceId, targetId));
  }, []);

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

  const adjustTopRatio = useCallback(
    (dividerIndex: number, clientX: number) => {
      const container = topGridRef.current;
      if (!container) return;
      const rect = container.getBoundingClientRect();
      if (rect.width <= 0) return;

      setTopRatios((current) => {
        const ratios = normalizeRatios(current, topPanes.length);
        const previousSum = ratios.slice(0, dividerIndex).reduce((sum, value) => sum + value, 0);
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
    },
    [topPanes.length],
  );

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
      {!isLayoutHydrated ? (
        <div className="min-h-0" />
      ) : (
        <div ref={topGridRef} className="terminal-split-grid" style={topGridStyle(visibleTopPanes.length, topRatios)}>
          {visibleTopPanes.map((pane, index) => (
            <TerminalPaneGroup key={pane.id}>
              {index > 0 && (
                <TerminalSplitDivider
                  orientation="vertical"
                  onDrag={(event) => adjustTopRatio(index - 1, event.clientX)}
                />
              )}
              <TerminalPane
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
      )}
      {isLayoutHydrated ? (
        <>
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
              onRename={renameBottomTab}
              onReorder={reorderBottomTab}
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
                </div>
              ))}
          </div>
        </>
      ) : (
        <>
          <div />
          <div />
        </>
      )}
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

function reorderById<T extends { id: string }>(items: T[], sourceId: string, targetId: string) {
  const sourceIndex = items.findIndex((item) => item.id === sourceId);
  const targetIndex = items.findIndex((item) => item.id === targetId);
  if (sourceIndex < 0 || targetIndex < 0 || sourceIndex === targetIndex) return items;
  const next = [...items];
  const [moved] = next.splice(sourceIndex, 1);
  next.splice(targetIndex, 0, moved);
  return next;
}

function workspaceGridStyle(bottomRatio: number, hasBottomTabs: boolean): CSSProperties {
  if (!hasBottomTabs) {
    return {
      gridTemplateRows: "minmax(0, 1fr) 1px 44px",
    };
  }

  const bottom = `${(bottomRatio * 100).toFixed(3)}fr`;
  const top = `${((1 - bottomRatio) * 100).toFixed(3)}fr`;
  return {
    gridTemplateRows: `minmax(0, ${top}) 1px minmax(0, ${bottom})`,
  };
}

function topGridStyle(count: number, ratios: number[]): CSSProperties {
  if (count <= 0) {
    return { gridTemplateColumns: "minmax(0, 1fr)" };
  }
  const normalized = normalizeRatios(ratios, count);
  return {
    gridTemplateColumns: Array.from({ length: count }, (_, index) =>
      index === 0 ? `minmax(0, ${normalized[index]}fr)` : `1px minmax(0, ${normalized[index]}fr)`,
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
      <div className="absolute z-20 top-2 right-2 flex items-center gap-1 text-xs text-ink-faint" data-terminal-control>
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
        <div className="h-full min-w-0 min-h-0 grid place-items-center rounded-[8px] border border-dashed border-border-subtle text-xs text-ink-mute">
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

function fileTabId(rootPath: string, path: string) {
  return `${normalizeFileEventPath(rootPath)}\u0000${normalizeFileEventPath(path)}`;
}

function fileTabMatchesId(tab: OpenFileTab, id: string) {
  return fileTabId(tab.rootPath, tab.path) === id || tab.path === id;
}

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
  if (
    eventProject !== rootPath &&
    !tabPath.startsWith(`${eventProject}/`) &&
    !eventProject.startsWith(`${rootPath}/`)
  ) {
    return false;
  }
  return event.changedPaths.some((path) => normalizeFileEventPath(path) === tabPath);
}

function displayableFileError(error: unknown) {
  const message = error instanceof Error ? error.message : String(error);
  if (message === "Path is outside the current project.") {
    return null;
  }
  return message;
}

function FilesMode({ project }: { project?: WorkspaceProject }) {
  const projectStateKey = project?.id ?? "";
  const [tabs, setTabs] = useState<OpenFileTab[]>(() => filesModeStateByProject.get(projectStateKey)?.tabs ?? []);
  const [activePath, setActivePath] = useState(() => filesModeStateByProject.get(projectStateKey)?.activePath ?? "");
  const [error, setError] = useState<string | null>(null);
  const [isBusy, setBusy] = useState(false);
  const editorRef = useRef<CodeEditorHandle | null>(null);
  const tabsRef = useRef<OpenFileTab[]>([]);
  const active = tabs.find((tab) => fileTabMatchesId(tab, activePath)) ?? tabs[0];

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

  const replaceTabWithReadResult = useCallback(
    (item: OpenFileTab, result: FileReadResult) => ({
      ...item,
      ...result,
      language: languageForPath(result.path),
      savedContent: result.content,
      dirty: false,
      version: item.version + 1,
      externalModifiedAt: undefined,
      externalSize: undefined,
    }),
    [],
  );

  const openFileInEditor = useCallback(async (rootPath: string, path: string) => {
    setError(null);
    const existing = tabsRef.current.find((tab) => tab.path === path && workspacePathsMatch(tab.rootPath, rootPath));
    if (existing) {
      setActivePath(fileTabId(existing.rootPath, existing.path));
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
        const existing = current.findIndex(
          (tab) => tab.path === result.path && workspacePathsMatch(tab.rootPath, rootPath),
        );
        if (existing >= 0) {
          const next = [...current];
          next[existing] = nextTab;
          return next;
        }
        return [...current, nextTab];
      });
      setActivePath(fileTabId(rootPath, result.path));
    } catch (nextError) {
      setError(displayableFileError(nextError));
    } finally {
      setBusy(false);
    }
  }, []);

  const closeTab = useCallback(
    async (path: string) => {
      const tab = tabsRef.current.find((item) => fileTabMatchesId(item, path));
      if (
        tab?.dirty &&
        !(await systemConfirm(
          tm(
            "files.preview.discard_changes.message",
            "This preview has edits that have not been saved to the original file.",
          ),
          {
            title: tm("files.preview.discard_changes.title", "Discard unsaved changes?"),
            kind: "warning",
            okLabel: tm("files.preview.discard_changes.discard", "Discard Changes"),
            cancelLabel: tm("common.cancel", "Cancel"),
          },
        ))
      ) {
        return;
      }
      setTabs((current) => {
        const next = current.filter((item) => !fileTabMatchesId(item, path));
        const wasActive = tabsRef.current.some((item) => fileTabMatchesId(item, activePath) && fileTabMatchesId(item, path));
        if (wasActive) {
          const nextActive = next[next.length - 1];
          setActivePath(nextActive ? fileTabId(nextActive.rootPath, nextActive.path) : "");
        }
        return next;
      });
    },
    [activePath],
  );

  const updateActiveContent = useCallback(
    (content: string) => {
      setTabs((current) =>
        current.map((tab) =>
          fileTabMatchesId(tab, activePath)
            ? { ...tab, content, dirty: content !== tab.savedContent }
            : tab,
        ),
      );
    },
    [activePath],
  );

  const saveActive = useCallback(async () => {
    const tab = tabsRef.current.find((item) => fileTabMatchesId(item, activePath));
    if (!tab || tab.readOnly || !tab.dirty) return;
    setBusy(true);
    setError(null);
    try {
      if (window.__TAURI_INTERNALS__) {
        const currentDisk = await readFile(tab.rootPath, tab.path);
        const diskChanged = currentDisk.modifiedAt !== tab.modifiedAt || currentDisk.size !== tab.size;
        if (diskChanged) {
          const overwrite = await systemConfirm(
            formatI18n(
              tm(
                "files.preview.external_save.message_format",
                '"%@" was changed outside the app. Saving now will overwrite the newer disk version.',
              ),
              tab.name,
            ),
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
                fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
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
          fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
            ? replaceTabWithReadResult(item, saved)
            : item,
        ),
      );
    } catch (nextError) {
      setError(displayableFileError(nextError));
    } finally {
      setBusy(false);
    }
  }, [activePath, replaceTabWithReadResult]);

  const reloadActive = useCallback(async () => {
    const tab = tabsRef.current.find((item) => fileTabMatchesId(item, activePath));
    if (!tab) return;
    if (
      tab.dirty &&
      !(await systemConfirm(
        tm(
          "files.preview.discard_changes.message",
          "This preview has edits that have not been saved to the original file.",
        ),
        {
          title: tm("files.preview.discard_changes.title", "Discard unsaved changes?"),
          kind: "warning",
          okLabel: tm("files.preview.reload", "Reload"),
          cancelLabel: tm("common.cancel", "Cancel"),
        },
      ))
    ) {
      return;
    }
    setBusy(true);
    setError(null);
    try {
      const result = await readFile(tab.rootPath, tab.path);
      setTabs((current) =>
        current.map((item) =>
          fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
            ? replaceTabWithReadResult(item, result)
            : item,
        ),
      );
    } catch (nextError) {
      setError(displayableFileError(nextError));
    } finally {
      setBusy(false);
    }
  }, [activePath, replaceTabWithReadResult]);

  const checkFileForExternalChanges = useCallback(
    async (path: string) => {
      const tab = tabsRef.current.find((item) => fileTabMatchesId(item, path));
      if (!tab || tab.readOnly || !window.__TAURI_INTERNALS__) return;
      try {
        const result = await readFile(tab.rootPath, tab.path);
        const changed = result.modifiedAt !== tab.modifiedAt || result.size !== tab.size;
        if (!changed) return;
        if (result.content === tab.content && result.isBinary === tab.isBinary) {
          setTabs((current) =>
            current.map((item) =>
              fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
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
              fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
                ? replaceTabWithReadResult(item, result)
                : item,
            ),
          );
          return;
        }
        if (tab.dirty && tab.externalModifiedAt === result.modifiedAt && tab.externalSize === result.size) {
          return;
        }

        if (tab.dirty) {
          setTabs((current) =>
            current.map((item) =>
              fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
                ? {
                    ...item,
                    externalModifiedAt: result.modifiedAt,
                    externalSize: result.size,
                  }
                : item,
            ),
          );
          const reload = await systemConfirm(
            formatI18n(
              tm(
                "files.preview.external_reload.message_format",
                '"%@" was changed outside the app. Reload the version on disk?',
              ),
              tab.name,
            ),
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
            fileTabId(item.rootPath, item.path) === fileTabId(tab.rootPath, tab.path)
              ? replaceTabWithReadResult(item, result)
              : item,
          ),
        );
      } catch (nextError) {
        setError(displayableFileError(nextError));
      }
    },
    [replaceTabWithReadResult],
  );

  useEffect(
    () =>
      listenWorkspaceCommand((command) => {
        if (command.type !== "open-file" && !project) {
          return;
        }
        if (command.type === "open-file") {
          if (!project?.path || !workspacePathsMatch(command.rootPath, project.path)) {
            return;
          }
          clearPendingOpenFileCommand(command);
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
    if (!project?.path) return;
    const command = consumePendingOpenFileCommand(project.path);
    if (command) {
      void openFileInEditor(command.rootPath, command.path);
    }
  }, [openFileInEditor, project?.path]);

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
        className="h-[46px] border-b border-border-subtle bg-transparent"
        items={tabs.map((tab) => ({
          id: fileTabId(tab.rootPath, tab.path),
          label: `${tab.name}${tab.dirty ? " •" : ""}`,
          icon: FileText,
          closable: true,
        }))}
        activeId={active ? fileTabId(active.rootPath, active.path) : ""}
        emptyLabel={isBusy ? tm("common.processing", "Processing") : tm("files.panel.title", "Files")}
        onSelect={(id) => {
          setActivePath(id);
          void checkFileForExternalChanges(id);
        }}
        onClose={(id) => void closeTab(id)}
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
                tab.path === active.path && tab.rootPath === active.rootPath ? { ...tab, scrollTop } : tab,
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
  const blockedMessage =
    tab.message || (tab.isBinary ? tm("files.preview.binary", "Binary files cannot be previewed here.") : "");
  const hasExternalChange = tab.externalModifiedAt != null || tab.externalSize != null;
  const parentDirectory = relativeParentDirectory(tab.relativePath || tab.path, tab.name) || "/";
  const [scrollInfo, setScrollInfo] = useState<CodeEditorScrollInfo>({
    ratio: 0,
    scrollTop: 0,
    scrollHeight: 0,
    clientHeight: 0,
  });
  const [searchOpen, setSearchOpen] = useState(false);
  const [searchQuery, setSearchQuery] = useState<CodeEditorSearchQuery>(DEFAULT_EDITOR_SEARCH_QUERY);
  const searchInputRef = useRef<HTMLInputElement | null>(null);
  const updateSearchQuery = useCallback(
    (patch: Partial<CodeEditorSearchQuery>) => {
      setSearchQuery((current) => {
        const next = { ...current, ...patch };
        editorRef.current?.setSearchQuery(next);
        return next;
      });
    },
    [editorRef],
  );
  const openSearch = useCallback(() => {
    setSearchOpen(true);
    window.requestAnimationFrame(() => searchInputRef.current?.focus());
  }, []);
  const closeSearch = useCallback(() => {
    setSearchOpen(false);
    editorRef.current?.focus();
  }, [editorRef]);
  useEffect(() => {
    setScrollInfo({
      ratio: 0,
      scrollTop: 0,
      scrollHeight: 0,
      clientHeight: 0,
    });
  }, [tab.path, tab.version]);
  useEffect(() => {
    editorRef.current?.setSearchQuery(searchQuery);
  }, [editorRef, searchQuery, tab.path, tab.version]);
  const readOnlyMessage = !tab.isBinary && blockedMessage ? blockedMessage : "";
  return (
    <div className="relative flex h-full min-h-0 flex-col overflow-hidden">
      <div className="h-[56px] flex flex-shrink-0 items-center justify-between gap-4 px-[18px] border-b border-border-subtle">
        <div className="leading-tight min-w-0">
          <div className="text-sm font-semibold truncate">
            {tab.name}
            {tab.dirty && <span className="ml-1 text-brand-amber">•</span>}
          </div>
          <div className="text-xs text-ink-faint truncate">{parentDirectory}</div>
        </div>
        <div className="flex items-center gap-1.5 flex-shrink-0">
          <EditorBtn
            icon={CheckCircle2}
            tooltip={tm("files.preview.save", "Save")}
            onPress={onSave}
            tone={tab.dirty && !tab.readOnly && !isBusy ? "success" : "default"}
            disabled={!tab.dirty || tab.readOnly || isBusy}
          />
          <EditorBtn
            icon={Undo2}
            tooltip={tm("files.preview.undo", "Undo")}
            onPress={() => editorRef.current?.undo()}
            disabled={tab.readOnly}
          />
          <EditorBtn
            icon={Redo2}
            tooltip={tm("files.preview.redo", "Redo")}
            onPress={() => editorRef.current?.redo()}
            disabled={tab.readOnly}
          />
          <EditorBtn icon={Search} tooltip={tm("files.preview.find", "Find")} onPress={openSearch} />
          <EditorBtn icon={Copy} tooltip={tm("files.preview.copy_path", "Copy Path")} onPress={onCopyPath} />
          <EditorBtn
            icon={RotateCcw}
            tooltip={tm("files.preview.reload", "Reload")}
            onPress={onReload}
            disabled={isBusy}
          />
          <EditorBtn icon={Folder} tooltip={tm("files.preview.reveal_finder", "Reveal in Finder")} onPress={onReveal} />
        </div>
      </div>

      {searchOpen && (
        <EditorSearchBar
          query={searchQuery}
          inputRef={searchInputRef}
          readOnly={tab.readOnly}
          onChange={updateSearchQuery}
          onClose={closeSearch}
          onFindNext={() => editorRef.current?.findNext()}
          onFindPrevious={() => editorRef.current?.findPrevious()}
          onSelectAll={() => editorRef.current?.selectMatches()}
          onReplace={() => editorRef.current?.replaceNext()}
          onReplaceAll={() => editorRef.current?.replaceAll()}
        />
      )}

      {error && (
        <div className="absolute z-30 top-[calc(100%-8px)] left-4 right-4 rounded-md border border-brand-red/30 bg-brand-red/12 px-3 py-2 text-xs text-brand-red">
          {error}
        </div>
      )}

      {hasExternalChange && !error && (
        <div className="absolute z-20 top-[calc(100%-8px)] left-4 right-4 flex items-center justify-between gap-3 rounded-md border border-brand-amber/35 bg-brand-amber/12 px-3 py-2 text-xs text-ink-soft shadow-floating">
          <span className="min-w-0 truncate">
            {tm(
              "files.preview.external_kept",
              "This file changed on disk. Current editor content is being kept until you reload or save.",
            )}
          </span>
          <PressableButton
            className="flex-shrink-0 rounded px-2 py-1 font-semibold text-brand-amber hover:bg-brand-amber/12"
            onPressUp={onReload}
          >
            {tm("files.preview.reload", "Reload")}
          </PressableButton>
        </div>
      )}

      {readOnlyMessage && !error && !hasExternalChange && (
        <div className="absolute z-20 top-[calc(100%-8px)] left-4 right-4 rounded-md border border-border-subtle bg-surface-muted/95 px-3 py-2 text-xs text-ink-mute shadow-floating">
          {readOnlyMessage}
        </div>
      )}

      <div className="relative grid min-h-0 flex-1 grid-cols-[minmax(0,1fr)_84px]">
        {tab.isBinary ? (
          <EditorEmptyState
            title={tm("files.preview.read_error", "Could not read this file.")}
            description={blockedMessage}
            compact
          />
        ) : (
          <CodeEditor
            ref={editorRef}
            documentKey={`${tab.path}:${tab.version}`}
            value={tab.content}
            language={tab.language}
            readOnly={tab.readOnly}
            onChange={onChange}
            onSave={onSave}
            onSearchOpen={openSearch}
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
    </div>
  );
}

function EditorBtn({
  icon: Icon,
  tooltip,
  onPress,
  disabled,
  tone = "default",
}: {
  icon: typeof Search;
  tooltip: string;
  onPress?: () => void;
  disabled?: boolean;
  tone?: "default" | "success";
}) {
  return (
    <Tooltip label={tooltip} placement="bottom">
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={tooltip}
        className={`h-[28px] w-[28px] min-w-[28px] ${
          tone === "success" && !disabled ? "text-brand-green" : "text-ink-mute"
        }`}
        onPress={onPress}
        disabled={disabled}
      >
        <Icon size={14} strokeWidth={2} />
      </Button>
    </Tooltip>
  );
}

function EditorSearchBar({
  query,
  inputRef,
  readOnly,
  onChange,
  onClose,
  onFindNext,
  onFindPrevious,
  onSelectAll,
  onReplace,
  onReplaceAll,
}: {
  query: CodeEditorSearchQuery;
  inputRef: MutableRefObject<HTMLInputElement | null>;
  readOnly: boolean;
  onChange: (patch: Partial<CodeEditorSearchQuery>) => void;
  onClose: () => void;
  onFindNext: () => void;
  onFindPrevious: () => void;
  onSelectAll: () => void;
  onReplace: () => void;
  onReplaceAll: () => void;
}) {
  const canSearch = query.search.trim().length > 0;
  const canReplace = canSearch && !readOnly;

  return (
    <div className="flex-shrink-0 border-b border-border-subtle bg-surface-main/75 px-[18px] py-2">
      <form
        className="flex flex-col gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSearch) onFindNext();
        }}
      >
        <div className="flex min-h-[28px] items-center justify-between gap-2">
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-1.5">
            <TextInput
              ref={inputRef}
              className="h-[26px] min-h-0 w-full max-w-[320px] flex-none px-2 py-0 text-sm leading-none"
              value={query.search}
              placeholder={tm("files.preview.search.find", "Find")}
              onChange={(event) => onChange({ search: event.currentTarget.value })}
              onKeyDown={(event) => {
                if (event.key !== "Enter" || event.nativeEvent.isComposing) return;
                event.preventDefault();
                if (canSearch) onFindNext();
              }}
            />
            <div className="flex items-center gap-1">
              <Button size="sm" variant="secondary" disabled={!canSearch} onPress={onFindNext}>
                {tm("files.preview.search.find", "Find")}
              </Button>
              <EditorSearchIconButton
                icon={ChevronDown}
                label={tm("files.preview.search.next", "Next")}
                disabled={!canSearch}
                onPress={onFindNext}
              />
              <EditorSearchIconButton
                icon={ChevronRight}
                label={tm("files.preview.search.previous", "Previous")}
                disabled={!canSearch}
                onPress={onFindPrevious}
                className="-rotate-90"
              />
              <EditorSearchIconButton
                icon={Star}
                label={tm("files.preview.search.all", "All")}
                disabled={!canSearch}
                onPress={onSelectAll}
              />
            </div>
          </div>
          <EditorSearchIconButton icon={X} label={tm("files.preview.search.close", "Close")} onPress={onClose} />
        </div>
        <div className="flex min-h-[28px] flex-wrap items-center gap-1.5">
          <TextInput
            className="h-[26px] min-h-0 w-full max-w-[320px] flex-none px-2 py-0 text-sm leading-none"
            value={query.replace}
            placeholder={tm("files.preview.search.replace", "Replace")}
            onChange={(event) => onChange({ replace: event.currentTarget.value })}
            disabled={readOnly}
          />
          <div className="flex min-w-0 flex-1 flex-wrap items-center gap-x-2 gap-y-1 text-sm text-ink-soft">
            <Checkbox
              checked={query.caseSensitive}
              onChange={(caseSensitive) => onChange({ caseSensitive })}
              label={tm("files.preview.search.match_case", "Match case")}
            />
            <Checkbox
              checked={query.regexp}
              onChange={(regexp) => onChange({ regexp })}
              label={tm("files.preview.search.regexp", "Regex")}
            />
            <Checkbox
              checked={query.wholeWord}
              onChange={(wholeWord) => onChange({ wholeWord })}
              label={tm("files.preview.search.by_word", "Whole word")}
            />
          </div>
          <div className="ml-auto flex min-w-0 items-center justify-end gap-2">
            <Button size="sm" variant="secondary" disabled={!canReplace} onPress={onReplace}>
              {tm("files.preview.search.replace_action", "Replace")}
            </Button>
            <Button size="sm" variant="secondary" disabled={!canReplace} onPress={onReplaceAll}>
              {tm("files.preview.search.replace_all", "Replace all")}
            </Button>
          </div>
        </div>
      </form>
    </div>
  );
}

function EditorSearchIconButton({
  icon: Icon,
  label,
  disabled,
  className,
  onPress,
}: {
  icon: typeof Search;
  label: string;
  disabled?: boolean;
  className?: string;
  onPress: () => void;
}) {
  return (
    <Tooltip label={label} placement="bottom">
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        aria-label={label}
        className={`h-[28px] w-[28px] min-w-[28px] text-ink-soft ${className ?? ""}`}
        disabled={disabled}
        onPress={onPress}
      >
        <Icon size={14} strokeWidth={2} />
      </Button>
    </Tooltip>
  );
}

function EditorEmptyState({ title, description, compact }: { title: string; description: string; compact?: boolean }) {
  return (
    <div className={`${compact ? "h-full" : "h-full"} grid place-items-center bg-surface-secondary text-center px-6`}>
      <div>
        <div className="w-11 h-11 mx-auto rounded-[10px] border border-border-subtle bg-fill/[0.04] grid place-items-center text-ink-mute">
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
    scrollInfo.scrollHeight > 0 ? Math.min(1, scrollInfo.clientHeight / scrollInfo.scrollHeight) : 1;
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
      className={`relative h-full min-h-0 overflow-hidden border-l border-border-subtle bg-surface-secondary ${
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
  const git = useGitStatusSnapshot(project);
  const snapshot = review.snapshot;
  const reviewUpdatedAt = review.updatedAt;
  const [selectedPath, setSelectedPath] = useState("");
  const [expandedReviewPaths, setExpandedReviewPaths] = useState<Set<string>>(new Set());
  const [content, setContent] = useState<GitReviewContentSnapshot | null>(null);
  const [diffError, setDiffError] = useState<string | null>(null);
  const [reviewScrollTop, setReviewScrollTop] = useState(0);
  const [reviewAction, setReviewAction] = useState<"" | "commit" | "push">("");
  const reviewScrollSourceRef = useRef("");
  const reviewTree = useMemo(() => buildReviewTree(snapshot.files), [snapshot.files]);
  const reviewDirectoryPaths = useMemo(() => collectReviewDirectoryPaths(reviewTree), [reviewTree]);
  const reviewTreeInitializedRef = useRef(false);
  const previousReviewDirectoryPathsRef = useRef<Set<string>>(new Set());
  const selectedFile = useMemo(() => {
    if (snapshot.files.length === 0) return undefined;
    return snapshot.files.find((file) => file.path === selectedPath) ?? snapshot.files[0];
  }, [selectedPath, snapshot.files]);
  const title =
    snapshot.mode === "taskBranch"
      ? tm("worktree.review.title", "Worktree Review")
      : tm("worktree.review.audit_title", "Uncommitted Audit");
  const subtitle =
    snapshot.mode === "taskBranch" && snapshot.baseBranch
      ? `${project?.branch ?? "HEAD"} ← ${snapshot.baseBranch}`
      : (project?.path ?? tm("worktree.review.audit_working_tree", "Working Tree"));
  const totalAdditions = snapshot.files.reduce((sum, file) => sum + file.additions, 0);
  const totalDeletions = snapshot.files.reduce((sum, file) => sum + file.deletions, 0);
  const committablePaths = useMemo(() => {
    const paths = new Set<string>();
    for (const file of git.snapshot.staged) paths.add(file.path);
    for (const file of git.snapshot.unstaged) paths.add(file.path);
    for (const file of git.snapshot.untracked) paths.add(file.path);
    return [...paths];
  }, [git.snapshot.staged, git.snapshot.unstaged, git.snapshot.untracked]);
  const selectedFilePath = selectedFile?.path ?? "";
  const language = languageForPath(selectedFilePath);
  const addedReviewLineHighlights = useMemo(() => addedLineHighlights(content), [content]);
  const deletedReviewLineHighlights = useMemo(() => deletedLineHighlights(content), [content]);

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
    if (!project?.path || !selectedFilePath) {
      setContent(null);
      setDiffError(null);
      return;
    }
    let disposed = false;
    const projectPath = project.path;
    const filePath = selectedFilePath;
    const baseBranch = snapshot.baseBranch;
    setDiffError(null);
    void loadGitReviewFileContent(projectPath, filePath, baseBranch)
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
  }, [project?.path, selectedFilePath, snapshot.baseBranch, reviewUpdatedAt]);

  useEffect(() => {
    reviewScrollSourceRef.current = "";
    setReviewScrollTop(0);
  }, [selectedFilePath, snapshot.baseBranch]);

  const handleReviewColumnScroll = useCallback((source: string, info: CodeEditorScrollInfo) => {
    reviewScrollSourceRef.current = source;
    setReviewScrollTop(info.scrollTop);
  }, []);

  const refreshReview = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__ || !project?.path) return;
    try {
      await invoke("git_review", {
        projectPath: project.path,
        baseBranch,
      });
    } catch (error) {
      console.error("failed to refresh git review", error);
    }
  }, [baseBranch, project?.path]);

  const openReviewFile = useCallback(
    (file: GitReviewFile) => {
      if (!project?.path) return;
      broadcastWorkspaceCommand({
        type: "open-file",
        rootPath: project.path,
        path: file.path,
      });
    },
    [project?.path],
  );

  const deleteReviewFile = useCallback(
    async (file: GitReviewFile) => {
      if (!project?.path || file.status === "deleted") return;
      const confirmed = await systemConfirm(
        formatI18n(tm("files.panel.delete.confirm_message_format", 'Delete "%@"?'), file.path),
        {
          title: tm("files.panel.delete.confirm_title", "Delete"),
          kind: "warning",
          okLabel: tm("common.delete", "Delete"),
          cancelLabel: tm("common.cancel", "Cancel"),
        },
      );
      if (!confirmed) return;
      try {
        await deleteFile(project.path, file.path);
        await refreshReview();
      } catch (error) {
        await systemMessage(error instanceof Error ? error.message : String(error), {
          title: tm("files.panel.delete.failure_format", "Could not delete: %@").replace("%@", file.path),
          kind: "error",
          okLabel: tm("common.ok", "OK"),
        });
      }
    },
    [project?.path, refreshReview],
  );

  const commitReviewChanges = useCallback(
    async (pushAfterCommit = false) => {
      if (!project?.path || reviewAction) return;
      const message = window.prompt(tm("git.commit.message.placeholder", "Commit message"))?.trim();
      if (!message) return;
      setReviewAction("commit");
      try {
        if (committablePaths.length > 0) {
          await git.stage(committablePaths);
        }
        await git.commit(message);
        if (pushAfterCommit) {
          setReviewAction("push");
          await git.push();
        }
        await refreshReview();
      } catch (error) {
        await systemMessage(error instanceof Error ? error.message : String(error), {
          title: pushAfterCommit ? tm("git.commit.action_push", "Commit and Push") : tm("git.commit.action", "Commit"),
          kind: "error",
          okLabel: tm("common.ok", "OK"),
        });
      } finally {
        setReviewAction("");
      }
    },
    [committablePaths, git, project?.path, refreshReview, reviewAction],
  );

  return (
    <div className="h-full min-h-0 grid grid-rows-[auto_minmax(0,1fr)_auto] bg-surface-secondary">
      <section className="flex items-center justify-between gap-4 border-b border-border-subtle bg-fill/[0.025] px-5 py-3.5">
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
        <div className="hidden md:flex items-center text-xs text-ink-faint">
          <ReviewCommitSplitButton
            disabled={!snapshot.isRepository || committablePaths.length === 0 || Boolean(reviewAction)}
            busy={Boolean(reviewAction)}
            onCommit={() => commitReviewChanges(false)}
            onCommitAndPush={() => commitReviewChanges(true)}
          />
        </div>
      </section>
      <div className="min-h-0 grid grid-cols-[280px_minmax(0,1fr)]">
        <div className="min-h-0 overflow-y-auto scrollbar-overlay border-r border-border-subtle bg-fill/[0.018] p-2">
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
                onOpenFile={openReviewFile}
                onDeleteFile={(file) => void deleteReviewFile(file)}
              />
            ))
          ) : (
            <div className="rounded-md border border-border-subtle bg-fill/[0.025] px-3 py-3 text-sm text-ink-faint">
              {!snapshot.isRepository
                ? tm("worktree.review.no_repository", "No Git repository.")
                : snapshot.error ||
                  review.error ||
                  tm("worktree.review.no_changes", "No changes relative to the base branch.")}
            </div>
          )}
        </div>
        <div className="min-h-0 min-w-0 grid grid-rows-[38px_minmax(0,1fr)]">
          <div className="min-w-0 flex items-center justify-between gap-3 border-b border-border-subtle px-3 text-xs text-ink-mute">
            <span className="truncate">{selectedFile?.path ?? tm("worktree.review.check.diff", "Diff")}</span>
            {selectedFile && (
              <PressableButton
                className="h-6 rounded-md px-2 text-ink-soft hover:bg-fill/[0.06] hover:text-ink"
                onPressUp={() => openReviewFile(selectedFile)}
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
                subtitle={
                  snapshot.mode === "taskBranch"
                    ? (snapshot.baseBranch ?? tm("worktree.task.base_branch", "Base Branch"))
                    : "HEAD"
                }
                value={snapshot.mode === "taskBranch" ? (content?.baseContent ?? "") : (content?.headContent ?? "")}
                language={language}
                lineHighlights={deletedReviewLineHighlights}
                scrollTop={reviewScrollTop}
                scrollSource={reviewScrollSourceRef.current}
                onScroll={handleReviewColumnScroll}
              />
              <ReviewCodeColumn
                id="new"
                title={tm("worktree.review.column.new", "New File")}
                subtitle={
                  content?.indexContent != null
                    ? tm("git.files.staged", "Staged")
                    : tm("worktree.review.audit_working_tree", "Working Tree")
                }
                value={content?.indexContent ?? content?.worktreeContent ?? ""}
                language={language}
                lineHighlights={addedReviewLineHighlights}
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
                lineHighlights={addedReviewLineHighlights}
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
                  lineHighlights={addedReviewLineHighlights}
                  scrollTop={reviewScrollTop}
                  scrollSource={reviewScrollSourceRef.current}
                  onScroll={handleReviewColumnScroll}
                />
              )}
            </div>
          ) : (
            <EditorEmptyState
              title={
                review.isLoading
                  ? tm("git.diff.loading", "Loading diff...")
                  : tm("git.diff.empty", "No diff to display")
              }
              description={
                selectedFile
                  ? tm("git.diff.empty_description", "This file did not produce diff content.")
                  : tm("worktree.review.select_file", "Select a changed file to compare.")
              }
              compact
            />
          )}
        </div>
      </div>
      <div className="h-[34px] flex items-center justify-between border-t border-border-subtle bg-fill/[0.025] px-3 text-xs text-ink-faint">
        <span className="truncate">{subtitle}</span>
        <span className="tabular-nums">
          {snapshot.files.length} · <span className="text-brand-green">+{totalAdditions}</span> ·{" "}
          <span className="text-brand-red">-{totalDeletions}</span>
        </span>
      </div>
    </div>
  );
}

function ReviewCommitSplitButton({
  disabled,
  busy,
  onCommit,
  onCommitAndPush,
}: {
  disabled?: boolean;
  busy?: boolean;
  onCommit: () => void;
  onCommitAndPush: () => void;
}) {
  const label = busy ? tm("git.commit.submitting", "Committing") : tm("git.commit.action", "Commit");
  return (
    <div className="flex overflow-hidden rounded-[9px] border border-brand-blue/50 bg-brand-blue text-on-brand shadow-[0_1px_2px_rgb(0_0_0_/_0.08)]">
      <Button
        size="sm"
        variant="primary"
        disabled={disabled}
        className="codux-split-button-main h-8 min-w-[86px] bg-transparent px-4 text-on-brand hover:bg-white/8 active:bg-white/12"
        onPressUp={onCommit}
      >
        {label}
      </Button>
      <Dropdown>
        <Dropdown.Trigger
          className="codux-split-button-trigger h-8 bg-transparent hover:bg-white/8"
          isDisabled={disabled}
          aria-label={tm("git.commit.options", "Commit Options")}
        >
          <ChevronDown size={13} strokeWidth={2.4} />
        </Dropdown.Trigger>
        <Dropdown.Popover
          placement="bottom end"
          offset={6}
          className="min-w-[174px] rounded-[10px] border border-border-subtle bg-surface-popover p-1 shadow-floating"
        >
          <Dropdown.Menu
            aria-label={tm("git.commit.options", "Commit Options")}
            onAction={(key) => {
              if (key === "commit") onCommit();
              if (key === "commitAndPush") onCommitAndPush();
            }}
          >
            <Dropdown.Item id="commit">{tm("git.commit.action", "Commit")}</Dropdown.Item>
            <Dropdown.Item id="commitAndPush">{tm("git.commit.action_push", "Commit and Push")}</Dropdown.Item>
          </Dropdown.Menu>
        </Dropdown.Popover>
      </Dropdown>
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
    <div className="min-h-0 min-w-0 grid grid-rows-[40px_minmax(0,1fr)] border-r border-border-subtle last:border-r-0 bg-surface-secondary">
      <div className="min-w-0 border-b border-border-subtle bg-fill/[0.02] px-2.5 py-1.5">
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
  onOpenFile,
  onDeleteFile,
}: {
  node: ReviewTreeNode;
  depth: number;
  projectPath?: string;
  selectedPath?: string;
  expandedPaths: Set<string>;
  onToggleDirectory: (path: string) => void;
  onSelectFile: (path: string) => void;
  onOpenFile: (file: GitReviewFile) => void;
  onDeleteFile: (file: GitReviewFile) => void;
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
        onOpen={() => onOpenFile(node.file)}
        onDelete={() => onDeleteFile(node.file)}
      />
    );
  }
  const expanded = expandedPaths.has(node.path);
  return (
    <>
      <ReviewDirectoryRow node={node} depth={depth} expanded={expanded} onToggle={() => onToggleDirectory(node.path)} />
      {expanded &&
        node.children.map((child) => (
          <ReviewTreeRow
            key={`${child.kind}:${child.path}`}
            node={child}
            depth={depth + 1}
            projectPath={projectPath}
            selectedPath={selectedPath}
            expandedPaths={expandedPaths}
            onToggleDirectory={onToggleDirectory}
            onSelectFile={onSelectFile}
            onOpenFile={onOpenFile}
            onDeleteFile={onDeleteFile}
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
        className="w-full h-8 grid grid-cols-[18px_16px_minmax(0,1fr)_42px_42px_18px] items-center gap-1.5 pr-2.5 rounded-md text-ink-soft text-sm hover:bg-fill/[0.045] hover:text-ink"
        style={{ paddingLeft: `${8 + depth * 14}px` }}
        onPressUp={onToggle}
      >
        {expanded ? (
          <ChevronDown size={12} className="text-ink-faint" />
        ) : (
          <ChevronRight size={12} className="text-ink-faint" />
        )}
        <Folder size={13} className="text-brand-blue/85" />
        <span className="min-w-0 truncate text-left text-xs font-medium">{node.name}</span>
        <span className="text-right text-xs tabular-nums text-brand-green">
          {node.additions > 0 ? `+${node.additions}` : ""}
        </span>
        <span className="text-right text-xs tabular-nums text-brand-red">
          {node.deletions > 0 ? `-${node.deletions}` : ""}
        </span>
        <span />
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
  onOpen,
  onDelete,
}: {
  file: GitReviewFile;
  displayName?: string;
  depth?: number;
  projectPath?: string;
  selected?: boolean;
  onSelect?: () => void;
  onOpen?: () => void;
  onDelete?: () => void;
}) {
  const badge = reviewFileStatusBadge(file.status);
  const contextMenu = useContextMenu();
  const canDelete = file.status !== "deleted";
  return (
    <>
      <Tooltip label={file.path} placement="right" triggerClassName="block w-full">
        <PressableButton
          className={`w-full h-8 grid grid-cols-[18px_minmax(0,1fr)_42px_42px_18px] items-center gap-1.5 pr-2.5 rounded-md text-ink-soft text-sm hover:bg-fill/[0.045] ${
            selected ? "bg-brand-blue/12 text-ink" : ""
          }`}
          style={{ paddingLeft: `${8 + depth * 14}px` }}
          onPressUp={onSelect}
          onDoubleClick={() => {
            if (!projectPath) return;
            onOpen?.();
          }}
          onContextMenu={(event) => {
            onSelect?.();
            contextMenu.openMenu(event);
          }}
        >
          <FileCode2 size={13} className="text-ink-mute" />
          <span className="min-w-0 truncate text-left text-xs">{displayName ?? file.path}</span>
          <span className="text-right text-xs tabular-nums text-brand-green">
            {file.additions > 0 ? `+${file.additions}` : ""}
          </span>
          <span className="text-right text-xs tabular-nums text-brand-red">
            {file.deletions > 0 ? `-${file.deletions}` : ""}
          </span>
          <span className={`text-center text-xs font-bold ${badge.tone}`}>{badge.label}</span>
        </PressableButton>
      </Tooltip>
      <ContextMenu ariaLabel={file.path} menu={contextMenu.menu} onClose={contextMenu.closeMenu}>
        <ContextMenuItem label={tm("files.panel.open", "Open")} onSelect={onOpen}>
          {tm("files.panel.open", "Open")}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem label={tm("common.delete", "Delete")} disabled={!canDelete} onSelect={onDelete}>
          {tm("common.delete", "Delete")}
        </ContextMenuItem>
      </ContextMenu>
    </>
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
