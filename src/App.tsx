import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useTransition } from "react";
import { aggregateProjectPhase, phaseToAIState, resolveDisplayedProjectPhase } from "./ai/projectPhase";
import { ensureAIHistoryEventCacheSubscription } from "./ai/history";
import { ensureMemoryEventSubscription } from "./ai/memory";
import { usePetLedger } from "./ai/petState";
import { aiRuntime } from "./ai/runtime";
import {
  closeAllProjectsFromMenu,
  closeProjectFromMenu,
  installAppMenuActions,
  installWorkspaceMenuActions,
  openProjectFolderFromMenu,
} from "./appActions";
import { AppIconMark } from "./components/AppIconMark";
import { Button } from "./components/Button";
import { Inspector } from "./components/Inspector";
import { ProjectSidebar } from "./components/ProjectSidebar";
import { TaskSidebar } from "./components/TaskSidebar";
import { Titlebar } from "./components/Titlebar";
import { Workspace } from "./components/Workspace";
import { ensureGitReviewEventCacheSubscription } from "./git/review";
import { ensureGitStatusEventCacheSubscription } from "./git/status";
import { readCachedProjectListSnapshot, writeCachedProjectListSnapshot } from "./projectSnapshotCache";
import { useRuntimeStore } from "./runtimeStore";
import {
  dispatchShortcut,
  isConfiguredShortcut,
  registerShortcutHandler,
  shortcutDisplayValue,
  shouldTerminalSkipShell,
  type ShortcutScope,
} from "./shortcuts";
import { openAppWindow, revealMainAppWindow } from "./windowing";
import { broadcastWorkspaceCommand, listenWorkspaceCommand } from "./workspaceCommands";
import { ensureWorktreeSnapshotEventCacheSubscription, useWorktreeSnapshot } from "./worktree/snapshot";
import { readAppSettings, subscribeAppSettings } from "./settings";
import { runAfterFirstPaint, runWhenIdle } from "./startupScheduler";
import { systemConfirm, systemMessage } from "./systemDialog";
import { isTerminalInputActive } from "./terminal/focus";
import { tm } from "./i18n";
import { ensureTerminalLayoutsSnapshotSubscription } from "./terminalLayout";
import { Columns2, FolderOpen, FolderPlus, GitBranch, Sparkles, Square2Stack } from "./icons";
import type {
  MainView,
  ProjectListSnapshot,
  ProjectSummary,
  RemoteStatus,
  RightPanelKind,
  TerminalSession,
  WorkspaceProject,
} from "./types";
import type { ProjectWorktreeSnapshot } from "./worktree/snapshot";

type HydratableProject = ProjectSummary &
  Partial<Pick<WorkspaceProject, "branch" | "aiState" | "terminals" | "changes">>;

const EMPTY_WORKTREES: ProjectWorktreeSnapshot[] = [];
const EMPTY_WORKTREE_AI_STATE: Record<string, WorkspaceProject["aiState"]> = {};
const MemoProjectSidebar = memo(ProjectSidebar);
const MemoTaskSidebar = memo(TaskSidebar);
const NON_GIT_WORKTREE_ERROR = "non_git_repository";

function hydrate(project: HydratableProject, index: number): WorkspaceProject {
  return {
    ...project,
    branch: project.branch ?? "master",
    aiState: project.aiState ?? "idle",
    terminals: project.terminals ?? (index === 0 ? 2 : 6),
    changes: project.changes ?? (index === 0 ? 6 : 4),
  };
}

function isTextEntryTarget(target: EventTarget | null) {
  const element = target instanceof Element ? target : null;
  if (!element) return false;
  if (element.closest("[contenteditable='true']")) return true;
  const field = element.closest("input, textarea, select");
  return Boolean(field);
}

function shouldSkipGlobalShortcutDispatch(event: KeyboardEvent) {
  if (!isTerminalInputActive(event.target)) return false;
  return !shouldTerminalSkipShell(event);
}

function worktreeIdsForProject(
  project: WorkspaceProject,
  snapshotsByKey: ReturnType<typeof useRuntimeStore.getState>["worktreeSnapshotByKey"],
) {
  const entry = snapshotsByKey[`${project.id}:${project.path}`];
  return entry?.snapshot.worktrees.map((worktree) => worktree.id) ?? [project.id];
}

function App() {
  const cachedProjectSnapshot = window.__TAURI_INTERNALS__ ? readCachedProjectListSnapshot() : null;
  const initialProjects = cachedProjectSnapshot?.projects ?? [];
  const [projects, setProjects] = useState<WorkspaceProject[]>(() => initialProjects.map(hydrate));
  const [selectedProjectId, setSelectedProjectId] = useState(() =>
    cachedProjectSnapshot?.selectedProjectId ?? initialProjects[0]?.id ?? "",
  );
  const [activeProjectId, setActiveProjectId] = useState(() =>
    cachedProjectSnapshot?.selectedProjectId ?? initialProjects[0]?.id ?? "",
  );
  const [inspectorProject, setInspectorProject] = useState<WorkspaceProject | undefined>(() => undefined);
  const [mainView, setMainView] = useState<MainView>("terminal");
  const [isSidebarExpanded, setSidebarExpanded] = useState(false);
  const [isTaskSidebarExpanded, setTaskSidebarExpanded] = useState(true);
  const [rightPanel, setRightPanel] = useState<RightPanelKind | null>(null);
  const [_session, setSession] = useState<TerminalSession | null>(null);
  const remoteStatus = useRuntimeStore((state) => state.remoteStatus);
  const projectListSnapshot = useRuntimeStore((state) => state.projectListSnapshot);
  const [selectedWorktreeByProject, setSelectedWorktreeByProject] = useState<Record<string, string>>({});
  const [terminalFocusRequest, setTerminalFocusRequest] = useState(0);
  const [taskCreateRequest, setTaskCreateRequest] = useState(0);
  const [isCreatingWorktree, setCreatingWorktree] = useState(false);
  const [isSecondaryStartupReady, setSecondaryStartupReady] = useState(!window.__TAURI_INTERNALS__);
  const [aiVersion, setAiVersion] = useState(0);
  const [iconStyle, setIconStyle] = useState(() => readAppSettings().iconStyle);
  const [, startInspectorTransition] = useTransition();
  const focusScopeRef = useRef<ShortcutScope>("workspace");
  const activeWorkspaceKeyRef = useRef("");
  const worktreeSnapshotByKey = useRuntimeStore((state) => state.worktreeSnapshotByKey);

  const applyProjectSnapshot = useCallback((snapshot: ProjectListSnapshot) => {
    writeCachedProjectListSnapshot(snapshot);
    const next = snapshot.projects.map(hydrate);
    const nextProjectIds = new Set(next.map((project) => project.id));
    setProjects(next);
    setSelectedWorktreeByProject((current) => {
      const incoming = snapshot.selectedWorktreeIdByProject ?? {};
      const merged: Record<string, string> = {};
      for (const project of next) {
        merged[project.id] = current[project.id] || incoming[project.id] || project.id;
      }
      return merged;
    });
    setSelectedProjectId((current) => {
      if (current && nextProjectIds.has(current)) {
        return current;
      }
      if (snapshot.selectedProjectId && nextProjectIds.has(snapshot.selectedProjectId)) {
        return snapshot.selectedProjectId;
      }
      return next[0]?.id ?? "";
    });
    setActiveProjectId((current) => {
      if (current && nextProjectIds.has(current)) {
        return current;
      }
      if (snapshot.selectedProjectId && nextProjectIds.has(snapshot.selectedProjectId)) {
        return snapshot.selectedProjectId;
      }
      return next[0]?.id ?? "";
    });
  }, []);

  useEffect(() => {
    const unsubscribe = aiRuntime.subscribe(() => setAiVersion((current) => current + 1));
    if (!window.__TAURI_INTERNALS__) return unsubscribe;
    let isDisposed = false;
    let unlistenProjects: (() => void) | undefined;
    let unlistenRemoteStatus: (() => void) | undefined;
    void Promise.all([
      listen<ProjectListSnapshot>("project:updated", (event) => {
        useRuntimeStore.getState().setProjectListSnapshot(event.payload);
      }),
      listen<RemoteStatus>("remote:status", (event) => {
        useRuntimeStore.getState().setRemoteStatus(event.payload);
      }),
    ])
      .then(([nextUnlistenProjects, nextUnlistenRemoteStatus]) => {
        if (isDisposed) {
          nextUnlistenProjects();
          nextUnlistenRemoteStatus();
          return;
        }
        unlistenProjects = nextUnlistenProjects;
        unlistenRemoteStatus = nextUnlistenRemoteStatus;
        runAfterFirstPaint(() => {
          if (isDisposed) return;
          void invoke("app_runtime_ready").catch((error) =>
            console.error("failed to initialize runtime snapshots", error),
          );
          runWhenIdle(() => {
            if (isDisposed) return;
            ensureTerminalLayoutsSnapshotSubscription();
            ensureWorktreeSnapshotEventCacheSubscription();
            ensureGitStatusEventCacheSubscription();
            ensureGitReviewEventCacheSubscription();
            ensureAIHistoryEventCacheSubscription();
            ensureMemoryEventSubscription();
            setSecondaryStartupReady(true);
          });
        });
      })
      .catch((error) => console.error("failed to initialize runtime event listeners", error));
    runWhenIdle(() => {
      if (!isDisposed) {
        void aiRuntime.start().catch((error) => console.error("failed to initialize ai runtime", error));
      }
    });
    return () => {
      isDisposed = true;
      unlistenProjects?.();
      unlistenRemoteStatus?.();
      unsubscribe();
    };
  }, [applyProjectSnapshot]);

  useEffect(() => {
    if (projectListSnapshot) applyProjectSnapshot(projectListSnapshot);
  }, [applyProjectSnapshot, projectListSnapshot]);

  const projectsWithAIState = useMemo(
    () =>
      projects.map((project) => {
        void aiVersion;
        const phase = aggregateProjectPhase(project.id, worktreeIdsForProject(project, worktreeSnapshotByKey), (id) =>
          resolveDisplayedProjectPhase(aiRuntime.projectPhase(id), aiRuntime.completedPhase(id)),
        );
        return { ...project, aiState: phaseToAIState(phase) };
      }),
    [aiVersion, projects, worktreeSnapshotByKey],
  );
  const pet = usePetLedger(projectsWithAIState, { enabled: isSecondaryStartupReady });

  const selectedProjectWithAIState = useMemo(
    () => projectsWithAIState.find((p) => p.id === activeProjectId) ?? projectsWithAIState[0],
    [activeProjectId, projectsWithAIState],
  );
  useEffect(() => subscribeAppSettings(() => void aiRuntime.start()), []);
  useEffect(() => subscribeAppSettings((settings) => setIconStyle(settings.iconStyle)), []);
  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    const reportWindowState = () => {
      void invoke("app_window_state", {
        visible: document.visibilityState !== "hidden",
        focused: document.hasFocus(),
      }).catch((error) => console.error("failed to report app window state", error));
    };
    reportWindowState();
    window.addEventListener("focus", reportWindowState);
    window.addEventListener("blur", reportWindowState);
    document.addEventListener("visibilitychange", reportWindowState);
    return () => {
      window.removeEventListener("focus", reportWindowState);
      window.removeEventListener("blur", reportWindowState);
      document.removeEventListener("visibilitychange", reportWindowState);
    };
  }, []);
  const worktree = useWorktreeSnapshot(isSecondaryStartupReady ? selectedProjectWithAIState : undefined);
  const worktreeSnapshot = worktree.snapshot;
  const isNonGitWorktree = worktreeSnapshot.error === NON_GIT_WORKTREE_ERROR;
  const canCreateWorktree = !worktreeSnapshot.error;
  const worktreeAIStateById = useMemo(() => {
    void aiVersion;
    return Object.fromEntries(
      worktreeSnapshot.worktrees.map((item) => [
        item.id,
        phaseToAIState(
          resolveDisplayedProjectPhase(aiRuntime.projectPhase(item.id), aiRuntime.completedPhase(item.id)),
        ),
      ]),
    );
  }, [aiVersion, worktreeSnapshot.worktrees]);
  const selectedWorktreeId = selectedProjectWithAIState
    ? selectedWorktreeByProject[selectedProjectWithAIState.id] ||
      worktreeSnapshot.selectedWorktreeId ||
      selectedProjectWithAIState.id
    : "";
  const selectedWorktree =
    worktreeSnapshot.worktrees.find((item) => item.id === selectedWorktreeId) ?? worktreeSnapshot.worktrees[0];
  const taskSidebarProject = isTaskSidebarExpanded ? selectedProjectWithAIState : undefined;
  const taskSidebarWorktrees = isTaskSidebarExpanded ? worktreeSnapshot.worktrees : EMPTY_WORKTREES;
  const taskSidebarAIStateById = isTaskSidebarExpanded ? worktreeAIStateById : EMPTY_WORKTREE_AI_STATE;
  const selectedWorkspaceProject = useMemo<WorkspaceProject | undefined>(() => {
    if (!selectedProjectWithAIState) return undefined;
    if (!selectedWorktree) return selectedProjectWithAIState;
    void aiVersion;
    const phase = aiRuntime.projectPhase(selectedWorktree.id);
    return {
      ...selectedProjectWithAIState,
      id: selectedWorktree.id,
      rootProjectId: selectedProjectWithAIState.id,
      worktreeId: selectedWorktree.id,
      name: selectedWorktree.isDefault
        ? selectedProjectWithAIState.name
        : `${selectedProjectWithAIState.name} · ${selectedWorktree.name}`,
      path: selectedWorktree.path,
      branch: selectedWorktree.branch || selectedProjectWithAIState.branch,
      baseBranch: selectedWorktree.isDefault ? null : selectedProjectWithAIState.branch,
      isDefaultWorktree: selectedWorktree.isDefault,
      changes: selectedWorktree.gitSummary.changes,
      aiState: phaseToAIState(phase),
      badgeSymbol: selectedProjectWithAIState.badgeSymbol,
      badgeColorHex: selectedProjectWithAIState.badgeColorHex,
      gitDefaultPushRemoteName: selectedProjectWithAIState.gitDefaultPushRemoteName,
    };
  }, [aiVersion, selectedProjectWithAIState, selectedWorktree]);
  const visibleRightPanel = selectedWorkspaceProject ? rightPanel : null;

  useEffect(() => {
    if (!selectedProjectWithAIState || worktreeSnapshot.worktrees.length === 0) return;
    const current = selectedWorktreeByProject[selectedProjectWithAIState.id];
    if (current && worktreeSnapshot.worktrees.some((worktree) => worktree.id === current)) {
      return;
    }
    setSelectedWorktreeByProject((existing) => ({
      ...existing,
      [selectedProjectWithAIState.id]: worktreeSnapshot.selectedWorktreeId || worktreeSnapshot.worktrees[0].id,
    }));
  }, [selectedProjectWithAIState, selectedWorktreeByProject, worktreeSnapshot]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || !selectedWorkspaceProject) return;
    const workspaceKey = [
      selectedWorkspaceProject.id,
      selectedWorkspaceProject.path,
      selectedWorkspaceProject.branch,
    ].join(":");
    if (activeWorkspaceKeyRef.current === workspaceKey) return;
    activeWorkspaceKeyRef.current = workspaceKey;
    void invoke("project_mark_active", {
      project: {
        id: selectedWorkspaceProject.id,
        name: selectedWorkspaceProject.name,
        path: selectedWorkspaceProject.path,
        badge: selectedWorkspaceProject.badge,
        status: selectedWorkspaceProject.status,
        branch: selectedWorkspaceProject.branch,
        changes: 0,
        badgeSymbol: selectedWorkspaceProject.badgeSymbol ?? null,
        badgeColorHex: selectedWorkspaceProject.badgeColorHex ?? null,
        gitDefaultPushRemoteName: selectedWorkspaceProject.gitDefaultPushRemoteName ?? null,
      },
    }).catch((error) => console.error("failed to mark active project", error));
  }, [selectedWorkspaceProject]);

  const toggleRightPanel = (next: RightPanelKind) => {
    setRightPanel((current) => (current === next ? null : next));
  };

  const setShortcutFocusScope = (scope: ShortcutScope) => {
    focusScopeRef.current = scope;
  };

  const requestTerminalFocus = useCallback(() => {
    setShortcutFocusScope("workspace");
    setTerminalFocusRequest((current) => current + 1);
  }, []);

  const openProjectFolder = useCallback(() => {
    void openProjectFolderFromMenu().catch((error) => console.error("failed to open project folder", error));
  }, []);

  useLayoutEffect(() => {
    void revealMainAppWindow().catch((error) => console.error("failed to reveal main window", error));
  }, []);

  useEffect(() => installAppMenuActions(), []);

  useEffect(
    () =>
      installWorkspaceMenuActions({
        setMainView: (view) => {
          setMainView(view);
          if (view === "terminal") {
            requestTerminalFocus();
            return;
          }
          setShortcutFocusScope("workspace");
        },
        toggleProjects: () => {
          setSidebarExpanded((value) => !value);
        },
        toggleTasks: () => {
          setTaskSidebarExpanded((value) => !value);
        },
        toggleRightPanel,
        createTask: () => {
          if (!canCreateWorktree) return;
          setTaskSidebarExpanded(true);
          setShortcutFocusScope("task-sidebar");
          setTaskCreateRequest((value) => value + 1);
        },
        openProjectFolder,
        closeCurrentProject: () => {
          void closeProjectFromMenu(selectedProjectWithAIState).catch((error) =>
            console.error("failed to close project", error),
          );
        },
        closeAllProjects: () => {
          void closeAllProjectsFromMenu(projectsWithAIState).catch((error) =>
            console.error("failed to close all projects", error),
          );
        },
      }),
    [canCreateWorktree, openProjectFolder, projectsWithAIState, requestTerminalFocus, selectedProjectWithAIState],
  );

  const selectProject = useCallback(
    (id: string) => {
      aiRuntime.dismissCompletion(id);
      const worktreeId = selectedWorktreeByProject[id];
      if (worktreeId && worktreeId !== id) {
        aiRuntime.dismissCompletion(worktreeId);
      }
      setSelectedProjectId(id);
      setActiveProjectId(id);
      if (window.__TAURI_INTERNALS__) {
        void invoke("project_select", { projectId: id }).catch((error) =>
          console.error("failed to select project", error),
        );
      }
    },
    [selectedWorktreeByProject],
  );

  useEffect(() => {
    startInspectorTransition(() => {
      setInspectorProject(selectedWorkspaceProject);
    });
  }, [selectedWorkspaceProject, startInspectorTransition]);

  const selectWorktree = useCallback(
    (id: string) => {
      if (!selectedProjectWithAIState) return;
      aiRuntime.dismissCompletion(id);
      setSelectedWorktreeByProject((existing) => {
        if (existing[selectedProjectWithAIState.id] === id) return existing;
        return {
          ...existing,
          [selectedProjectWithAIState.id]: id,
        };
      });
      if (window.__TAURI_INTERNALS__) {
        void invoke("project_select_worktree", {
          request: {
            projectId: selectedProjectWithAIState.id,
            worktreeId: id,
          },
        }).catch((error) => console.error("failed to select worktree", error));
      }
    },
    [selectedProjectWithAIState],
  );

  const createWorktreeForSelectedProject = useCallback(
    async (input?: { branchName: string; baseBranch?: string | null }) => {
      if (!selectedProjectWithAIState) return;
      if (!canCreateWorktree) return;
      const branchName = input?.branchName.trim();
      if (!branchName) return;
      const baseBranch = input?.baseBranch?.trim() || selectedWorktree?.branch || selectedProjectWithAIState.branch;
      setCreatingWorktree(true);
      try {
        const next = await worktree.create({
          projectId: selectedProjectWithAIState.id,
          projectPath: selectedProjectWithAIState.path,
          baseBranch,
          branchName,
        });
        const created = next.worktrees.find((item) => item.branch === branchName);
        if (created) {
          setSelectedWorktreeByProject((existing) => ({
            ...existing,
            [selectedProjectWithAIState.id]: created.id,
          }));
        }
      } catch (error) {
        console.error("failed to create worktree", error);
        throw error;
      } finally {
        setCreatingWorktree(false);
      }
    },
    [canCreateWorktree, selectedProjectWithAIState, selectedWorktree?.branch, worktree],
  );

  const removeWorktreeForSelectedProject = useCallback(
    async (target: ProjectWorktreeSnapshot, options?: { removeBranch?: boolean }) => {
      if (!selectedProjectWithAIState || target.isDefault) return;
      const removeBranch = options?.removeBranch ?? false;
      try {
        const next = await worktree.remove({
          projectId: selectedProjectWithAIState.id,
          projectPath: selectedProjectWithAIState.path,
          worktreePath: target.path,
          removeBranch,
        });
        const nextSelected = next.selectedWorktreeId || next.worktrees.find((item) => item.isDefault)?.id || next.worktrees[0]?.id;
        if (nextSelected) {
          setSelectedWorktreeByProject((existing) => ({
            ...existing,
            [selectedProjectWithAIState.id]: nextSelected,
          }));
        }
      } catch (error) {
        console.error("failed to remove worktree", error);
        void systemMessage(error instanceof Error ? error.message : String(error), {
          title: tm("worktree.remove.title", "Remove Worktree"),
          kind: "error",
          okLabel: tm("common.ok", "OK"),
        });
      }
    },
    [selectedProjectWithAIState, worktree],
  );

  const mergeWorktreeForSelectedProject = useCallback(
    async (target: ProjectWorktreeSnapshot, options?: { removeBranch?: boolean }) => {
      if (!selectedProjectWithAIState || target.isDefault) return;
      const removeBranch = options?.removeBranch ?? false;
      if (
        !(await systemConfirm(
          tm("worktree.merge_to_mainline.message_format", "Merge %@ into %@.").replace(
            "%@",
            target.branch || target.name,
          ).replace("%@", selectedProjectWithAIState.branch || "main"),
          {
            title: tm("worktree.merge.title", "Merge Worktree"),
            kind: "warning",
            okLabel: tm("worktree.menu.merge", "Merge"),
            cancelLabel: tm("common.cancel", "Cancel"),
          },
        ))
      ) {
        return;
      }
      try {
        const next = await worktree.merge({
          projectId: selectedProjectWithAIState.id,
          projectPath: selectedProjectWithAIState.path,
          worktreePath: target.path,
          baseBranch: selectedProjectWithAIState.branch,
          removeBranch,
        });
        const nextSelected = next.selectedWorktreeId || next.worktrees.find((item) => item.isDefault)?.id || next.worktrees[0]?.id;
        if (nextSelected) {
          setSelectedWorktreeByProject((existing) => ({
            ...existing,
            [selectedProjectWithAIState.id]: nextSelected,
          }));
        }
      } catch (error) {
        console.error("failed to merge worktree", error);
        void systemMessage(error instanceof Error ? error.message : String(error), {
          title: tm("worktree.merge.title", "Merge Worktree"),
          kind: "error",
          okLabel: tm("common.ok", "OK"),
        });
      }
    },
    [selectedProjectWithAIState, worktree],
  );

  const reviewWorktree = useCallback(
    (target: ProjectWorktreeSnapshot) => {
      selectWorktree(target.id);
      setMainView("review");
      setShortcutFocusScope("workspace");
    },
    [selectWorktree],
  );

  const openWorktreeTerminal = useCallback(
    (target: ProjectWorktreeSnapshot) => {
      selectWorktree(target.id);
      setMainView("terminal");
      requestTerminalFocus();
    },
    [requestTerminalFocus, selectWorktree],
  );

  const refreshWorktrees = useCallback(() => {
    void worktree.refresh();
  }, [worktree]);

  const openProjectCreateWindow = useCallback(() => {
    void openAppWindow("project-create");
  }, []);

  const openSettingsWindow = useCallback(() => {
    void openAppWindow("settings");
  }, []);

  const createWorktreeFromProject = useCallback(
    (project: WorkspaceProject) => {
      selectProject(project.id);
      setTaskSidebarExpanded(true);
      const key = `${project.id}:${project.path}`;
      const snapshot = useRuntimeStore.getState().worktreeSnapshotByKey[key]?.snapshot;
      if (!snapshot?.error) {
        setTaskCreateRequest((value) => value + 1);
      }
    },
    [selectProject],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (shouldSkipGlobalShortcutDispatch(event)) {
        return;
      }
      const handled = dispatchShortcut(event, {
        focusScope: focusScopeRef.current,
        mainView,
        rightPanel: visibleRightPanel,
      });
      if (!handled && isConfiguredShortcut(event, "close.active")) {
        event.preventDefault();
        event.stopPropagation();
        event.stopImmediatePropagation();
      }
    };

    window.addEventListener("keydown", handleKeyDown, true);
    return () => {
      window.removeEventListener("keydown", handleKeyDown, true);
    };
  }, [mainView, visibleRightPanel]);

  useEffect(() => {
    return registerShortcutHandler("global", (event) => {
      if (isConfiguredShortcut(event, "view.terminal")) {
        setMainView("terminal");
        requestTerminalFocus();
        return true;
      }
      if (isConfiguredShortcut(event, "view.files")) {
        setMainView("files");
        setShortcutFocusScope("workspace");
        return true;
      }
      if (isConfiguredShortcut(event, "view.review")) {
        setMainView("review");
        setShortcutFocusScope("workspace");
        return true;
      }
      if (isConfiguredShortcut(event, "terminal.split")) {
        setMainView("terminal");
        requestTerminalFocus();
        broadcastWorkspaceCommand({ type: "add-top-terminal-split" });
        return true;
      }
      if (isConfiguredShortcut(event, "terminal.tab")) {
        setMainView("terminal");
        requestTerminalFocus();
        broadcastWorkspaceCommand({ type: "add-bottom-terminal-tab" });
        return true;
      }
      if (isConfiguredShortcut(event, "panel.git")) {
        if (!selectedWorkspaceProject) return true;
        setRightPanel((panel) => (panel === "git" ? null : "git"));
        return true;
      }
      if (isConfiguredShortcut(event, "panel.ai")) {
        if (!selectedWorkspaceProject) return true;
        setRightPanel((panel) => (panel === "ai" ? null : "ai"));
        return true;
      }
      return false;
    });
  }, [requestTerminalFocus, selectedWorkspaceProject]);

  useEffect(() => {
    if (mainView === "terminal") {
      requestTerminalFocus();
    }
  }, [mainView, requestTerminalFocus, selectedWorkspaceProject?.id]);

  useEffect(() => {
    return registerShortcutHandler("project-sidebar", (event) => {
      if (isConfiguredShortcut(event, "project.create")) {
        void openAppWindow("project-create");
        return true;
      }
      if (isConfiguredShortcut(event, "close.active")) {
        setSidebarExpanded(false);
        return true;
      }
      if (isConfiguredShortcut(event, "settings.open")) {
        void openAppWindow("settings");
        return true;
      }
      if (isTextEntryTarget(event.target)) return false;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        if (projectsWithAIState.length === 0) return true;
        const currentIndex = Math.max(
          0,
          projectsWithAIState.findIndex((project) => project.id === selectedProjectId),
        );
        const delta = event.key === "ArrowDown" ? 1 : -1;
        const nextIndex = (currentIndex + delta + projectsWithAIState.length) % projectsWithAIState.length;
        selectProject(projectsWithAIState[nextIndex].id);
        return true;
      }
      return false;
    });
  }, [projectsWithAIState, selectProject, selectedProjectId]);

  useEffect(() => {
    return registerShortcutHandler("task-sidebar", (event) => {
      if (isConfiguredShortcut(event, "task.create")) {
        if (!canCreateWorktree) return true;
        setTaskSidebarExpanded(true);
        setTaskCreateRequest((value) => value + 1);
        return true;
      }
      if (isConfiguredShortcut(event, "close.active")) {
        setTaskSidebarExpanded(false);
        return true;
      }
      if (isTextEntryTarget(event.target)) return false;
      if (event.key === "ArrowDown" || event.key === "ArrowUp") {
        const worktrees = worktreeSnapshot.worktrees;
        if (worktrees.length === 0) return true;
        const currentIndex = Math.max(
          0,
          worktrees.findIndex((worktree) => worktree.id === selectedWorktreeId),
        );
        const delta = event.key === "ArrowDown" ? 1 : -1;
        const nextIndex = (currentIndex + delta + worktrees.length) % worktrees.length;
        selectWorktree(worktrees[nextIndex].id);
        return true;
      }
      return false;
    });
  }, [canCreateWorktree, selectWorktree, selectedWorktreeId, worktreeSnapshot.worktrees]);

  useEffect(() => {
    return registerShortcutHandler("right-sidebar", (event) => {
      if (isConfiguredShortcut(event, "close.active")) {
        setRightPanel(null);
        return true;
      }
      if (!isTextEntryTarget(event.target) && event.key === "Escape") {
        setRightPanel(null);
        return true;
      }
      return false;
    });
  }, []);

  useEffect(
    () =>
      listenWorkspaceCommand((command) => {
        if (command.type === "open-file") {
          setMainView("files");
          setShortcutFocusScope("workspace");
        }
        if (command.type === "add-top-terminal-split" || command.type === "add-bottom-terminal-tab") {
          setMainView("terminal");
          setShortcutFocusScope("workspace");
        }
        if (command.type === "insert-terminal-text") {
          setMainView("terminal");
          setShortcutFocusScope("workspace");
          requestTerminalFocus();
        }
        if (command.type === "open-right-panel") {
          if (!selectedWorkspaceProject) return;
          setRightPanel(command.panel);
          setShortcutFocusScope("right-sidebar");
        }
      }),
    [requestTerminalFocus, selectedWorkspaceProject],
  );

  return (
    <main className="app-shell relative w-screen h-screen overflow-hidden text-ink">
      <Titlebar
        projects={projectsWithAIState}
        selectedProject={selectedWorkspaceProject}
        mainView={mainView}
        setMainView={setMainView}
        isSidebarExpanded={isSidebarExpanded}
        toggleSidebar={() => {
          setSidebarExpanded((value) => !value);
        }}
        isTaskSidebarExpanded={isTaskSidebarExpanded}
        toggleTaskSidebar={() => {
          setTaskSidebarExpanded((value) => !value);
        }}
        rightPanel={visibleRightPanel}
        toggleRightPanel={toggleRightPanel}
        remoteStatus={remoteStatus}
        pet={pet}
      />

      <div className="absolute inset-x-0 bottom-0 flex" style={{ top: "var(--titlebar-height)" }}>
        <MemoProjectSidebar
          projects={projectsWithAIState}
          selectedProjectId={selectedProjectId}
          onSelect={selectProject}
          isExpanded={isSidebarExpanded}
          onFocusScope={() => setShortcutFocusScope("project-sidebar")}
          onCreateProject={openProjectCreateWindow}
          onOpenSettings={openSettingsWindow}
          onCreateWorktree={createWorktreeFromProject}
        />

        <div className="flex-1 min-w-0 flex">
          <div className="flex-1 min-w-0 flex rounded-tl-workspace overflow-hidden border-t border-l border-border bg-surface-secondary/95">
            {selectedProjectWithAIState && (
              <aside
                className={`flex-shrink-0 overflow-hidden border-r border-border bg-fill/[0.025] transition-[width,opacity] duration-150 ${
                  isTaskSidebarExpanded ? "w-[216px] opacity-100" : "w-0 opacity-0 pointer-events-none"
                }`}
                aria-hidden={!isTaskSidebarExpanded}
                onPointerDown={() => setShortcutFocusScope("task-sidebar")}
                onFocusCapture={() => setShortcutFocusScope("task-sidebar")}
              >
                <div className="h-full w-[216px]">
                  <MemoTaskSidebar
                    selectedProject={taskSidebarProject}
                    worktrees={taskSidebarWorktrees}
                    selectedWorktreeId={selectedWorktreeId}
                    aiStateByWorktreeId={taskSidebarAIStateById}
                    canCreateWorktree={canCreateWorktree}
                    repositoryMessage={isNonGitWorktree ? tm("worktree.repository.non_git", "Non-Git repository") : ""}
                    onSelectWorktree={selectWorktree}
                    onCreateWorktree={createWorktreeForSelectedProject}
                    onRemoveWorktree={removeWorktreeForSelectedProject}
                    onMergeWorktree={mergeWorktreeForSelectedProject}
                    onOpenWorktreeTerminal={openWorktreeTerminal}
                    onReviewWorktree={reviewWorktree}
                    onRefreshWorktrees={refreshWorktrees}
                    isBusy={worktree.isLoading || isCreatingWorktree}
                    createRequest={taskCreateRequest}
                  />
                </div>
              </aside>
            )}
            <div
              className="flex-1 min-w-0"
              onPointerDown={() => setShortcutFocusScope("workspace")}
              onFocusCapture={() => setShortcutFocusScope("workspace")}
            >
              {selectedWorkspaceProject ? (
                <Workspace
                  mainView={mainView}
                  selectedProject={selectedWorkspaceProject}
                  terminalFocusRequest={terminalFocusRequest}
                  onSessionChange={setSession}
                />
              ) : (
                <WelcomeWorkspace
                  iconStyle={iconStyle}
                  onCreateProject={openProjectCreateWindow}
                  onOpenFolder={openProjectFolder}
                />
              )}
            </div>
          </div>

          {visibleRightPanel && (
            <div
              className="w-[320px] flex-shrink-0 border-t border-l border-border"
              onPointerDown={() => setShortcutFocusScope("right-sidebar")}
              onFocusCapture={() => setShortcutFocusScope("right-sidebar")}
            >
              <Inspector panel={visibleRightPanel} selectedProject={inspectorProject} />
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

function WelcomeWorkspace({
  iconStyle,
  onCreateProject,
  onOpenFolder,
}: {
  iconStyle: string;
  onCreateProject: () => void;
  onOpenFolder: () => void;
}) {
  const title = tm("welcome.title_format", "Welcome to %@").replace("%@", "Codux");
  return (
    <section className="flex h-full min-h-0 flex-col px-10 py-5">
      <div className="flex flex-1 items-center justify-center">
        <div className="flex w-full max-w-[360px] flex-col items-center text-center">
          <AppIconMark styleName={iconStyle} size={72} className="mb-5 drop-shadow-[0_3px_6px_rgb(0_0_0_/_0.08)]" />
          <h1 className="text-[22px] font-bold leading-tight tracking-normal text-ink/90">{title}</h1>
          <p className="mt-1.5 max-w-[320px] text-[13px] leading-5 text-ink-soft/80">
            {tm("welcome.subtitle", "Create a project in the sidebar to get started")}
          </p>
          <div className="mt-5 grid w-full gap-2.5">
            <WelcomeActionButton
              icon={FolderPlus}
              label={tm("menu.file.new_project", "New Project")}
              variant="primary"
              onPress={onCreateProject}
            />
            <WelcomeActionButton
              icon={FolderOpen}
              label={tm("welcome.open_project", "Open Project")}
              variant="secondary"
              onPress={onOpenFolder}
            />
          </div>
        </div>
      </div>

      <WelcomeShortcutHints />
    </section>
  );
}

function WelcomeActionButton({
  icon: Icon,
  label,
  variant,
  onPress,
}: {
  icon: typeof FolderPlus;
  label: string;
  variant: "primary" | "secondary";
  onPress: () => void;
}) {
  const isPrimary = variant === "primary";
  return (
    <Button
      size="md"
      variant={isPrimary ? "primary" : "secondary"}
      onPress={onPress}
      className={`mx-auto !h-auto min-w-[136px] rounded-[9px] px-4 py-2.5 text-[15px] font-medium shadow-[0_2px_5px_rgb(0_0_0_/_0.08)] active:scale-[0.985] ${
        isPrimary
          ? "border border-white/15 text-on-brand"
          : "border border-border-subtle bg-fill/[0.09] text-ink hover:bg-fill/[0.13]"
      }`}
    >
      <span className="inline-flex items-center justify-center gap-[9px]">
        <Icon size={14} strokeWidth={2.2} />
        <span>{label}</span>
      </span>
    </Button>
  );
}

function WelcomeShortcutHints() {
  const hints = [
    {
      id: "split",
      icon: Columns2,
      label: tm("titlebar.split", "Split"),
      keys: shortcutDisplayValue("terminal.split"),
    },
    {
      id: "tab",
      icon: Square2Stack,
      label: tm("titlebar.tab", "Tab"),
      keys: shortcutDisplayValue("terminal.tab"),
    },
    {
      id: "git",
      icon: GitBranch,
      label: tm("titlebar.git", "Git"),
      keys: shortcutDisplayValue("panel.git"),
    },
    {
      id: "ai",
      icon: Sparkles,
      label: tm("titlebar.ai", "AI"),
      keys: shortcutDisplayValue("panel.ai"),
    },
  ];

  return (
    <div className="flex flex-wrap items-start justify-center gap-x-6 gap-y-4 pb-0 text-ink-soft/80">
      {hints.map((hint) => (
        <WelcomeShortcutHint key={hint.id} icon={hint.icon} label={hint.label} keys={hint.keys} />
      ))}
    </div>
  );
}

function WelcomeShortcutHint({
  icon: Icon,
  label,
  keys,
}: {
  icon: typeof Columns2;
  label: string;
  keys: string;
}) {
  return (
    <div className="grid min-w-[46px] justify-items-center gap-1">
      <Icon size={13} strokeWidth={1.9} className="text-ink-faint/75" />
      <div className="text-[11px] leading-none text-ink-soft/80">{label}</div>
      <div className="text-[10px] font-medium leading-none text-ink-faint/75">{keys}</div>
    </div>
  );
}

export default App;
