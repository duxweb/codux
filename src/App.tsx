import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { memo, useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState, useTransition } from "react";
import { aggregateProjectPhase, phaseToAIState, resolveDisplayedProjectPhase } from "./ai/projectPhase";
import { ensureAIHistoryEventCacheSubscription } from "./ai/history";
import { usePetLedger } from "./ai/petState";
import { aiRuntime } from "./ai/runtime";
import {
  closeAllProjectsFromMenu,
  closeProjectFromMenu,
  installAppMenuActions,
  installWorkspaceMenuActions,
  openProjectFolderFromMenu,
} from "./appActions";
import { Inspector } from "./components/Inspector";
import { ProjectSidebar } from "./components/ProjectSidebar";
import { TaskSidebar } from "./components/TaskSidebar";
import { Titlebar } from "./components/Titlebar";
import { Workspace } from "./components/Workspace";
import { fallbackProjects } from "./data/mock";
import { ensureGitReviewEventCacheSubscription } from "./git/review";
import { ensureGitStatusEventCacheSubscription } from "./git/status";
import { readCachedProjectListSnapshot, writeCachedProjectListSnapshot } from "./projectSnapshotCache";
import { useRuntimeStore } from "./runtimeStore";
import { dispatchShortcut, isConfiguredShortcut, registerShortcutHandler, type ShortcutScope } from "./shortcuts";
import { openAppWindow, revealMainAppWindow } from "./windowing";
import { listenWorkspaceCommand } from "./workspaceCommands";
import { ensureWorktreeSnapshotEventCacheSubscription, useWorktreeSnapshot } from "./worktree/snapshot";
import { subscribeAppSettings } from "./settings";
import { systemConfirm } from "./systemDialog";
import { tm } from "./i18n";
import { ensureTerminalLayoutsSnapshotSubscription } from "./terminalLayout";
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

function App() {
  const cachedProjectSnapshot = window.__TAURI_INTERNALS__ ? readCachedProjectListSnapshot() : null;
  const initialProjects = cachedProjectSnapshot?.projects ?? (window.__TAURI_INTERNALS__ ? [] : fallbackProjects);
  const [projects, setProjects] = useState<WorkspaceProject[]>(() => initialProjects.map(hydrate));
  const [selectedProjectId, setSelectedProjectId] = useState(() =>
    window.__TAURI_INTERNALS__
      ? (cachedProjectSnapshot?.selectedProjectId ?? initialProjects[0]?.id ?? "")
      : (fallbackProjects[0]?.id ?? ""),
  );
  const [activeProjectId, setActiveProjectId] = useState(() =>
    window.__TAURI_INTERNALS__
      ? (cachedProjectSnapshot?.selectedProjectId ?? initialProjects[0]?.id ?? "")
      : (fallbackProjects[0]?.id ?? ""),
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
  const [aiVersion, setAiVersion] = useState(0);
  const [, startInspectorTransition] = useTransition();
  const focusScopeRef = useRef<ShortcutScope>("workspace");
  const activeWorkspaceKeyRef = useRef("");

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
    ensureAIHistoryEventCacheSubscription();
    ensureGitReviewEventCacheSubscription();
    ensureGitStatusEventCacheSubscription();
    ensureWorktreeSnapshotEventCacheSubscription();
    ensureTerminalLayoutsSnapshotSubscription();
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
        void invoke("app_runtime_ready").catch((error) =>
          console.error("failed to initialize runtime snapshots", error),
        );
      })
      .catch((error) => console.error("failed to initialize runtime event listeners", error));
    void aiRuntime.start().catch((error) => console.error("failed to initialize ai runtime", error));
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
        const phase = aggregateProjectPhase(project.id, selectedWorktreeByProject[project.id], (id) =>
          resolveDisplayedProjectPhase(aiRuntime.projectPhase(id), aiRuntime.completedPhase(id)),
        );
        return { ...project, aiState: phaseToAIState(phase) };
      }),
    [aiVersion, projects, selectedWorktreeByProject],
  );
  const pet = usePetLedger(projectsWithAIState);

  const selectedProjectWithAIState = useMemo(
    () => projectsWithAIState.find((p) => p.id === activeProjectId) ?? projectsWithAIState[0],
    [activeProjectId, projectsWithAIState],
  );
  useEffect(() => subscribeAppSettings(() => void aiRuntime.start()), []);
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
  const worktree = useWorktreeSnapshot(selectedProjectWithAIState);
  const worktreeSnapshot = worktree.snapshot;
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
          setTaskSidebarExpanded(true);
          setShortcutFocusScope("task-sidebar");
          setTaskCreateRequest((value) => value + 1);
        },
        openProjectFolder: () => {
          void openProjectFolderFromMenu().catch((error) => console.error("failed to open project folder", error));
        },
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
    [projectsWithAIState, requestTerminalFocus, selectedProjectWithAIState],
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
    [selectedProjectWithAIState, selectedWorktree?.branch, worktree],
  );

  const removeWorktreeForSelectedProject = useCallback(
    async (target: ProjectWorktreeSnapshot) => {
      if (!selectedProjectWithAIState || target.isDefault) return;
      if (
        !(await systemConfirm(
          tm(
            "worktree.remove.message_format",
            "Remove %@ from Codux and the Git worktree list? The branch will not be deleted.",
          ).replace("%@", target.branch || target.name),
          {
            title: tm("worktree.remove.title", "Remove Worktree"),
            kind: "warning",
            okLabel: tm("worktree.menu.remove", "Remove"),
            cancelLabel: tm("common.cancel", "Cancel"),
          },
        ))
      ) {
        return;
      }
      try {
        const next = await worktree.remove({
          projectId: selectedProjectWithAIState.id,
          projectPath: selectedProjectWithAIState.path,
          worktreePath: target.path,
        });
        const nextSelected = next.worktrees[0]?.id;
        if (nextSelected) {
          setSelectedWorktreeByProject((existing) => ({
            ...existing,
            [selectedProjectWithAIState.id]: nextSelected,
          }));
        }
      } catch (error) {
        console.error("failed to remove worktree", error);
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
      setTaskCreateRequest((value) => value + 1);
    },
    [selectProject],
  );

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      const handled = dispatchShortcut(event, {
        focusScope: focusScopeRef.current,
        mainView,
        rightPanel,
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
  }, [mainView, rightPanel]);

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
      return false;
    });
  }, [requestTerminalFocus]);

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
  }, [selectWorktree, selectedWorktreeId, worktreeSnapshot.worktrees]);

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
          setRightPanel(command.panel);
          setShortcutFocusScope("right-sidebar");
        }
      }),
    [requestTerminalFocus],
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
        rightPanel={rightPanel}
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
          <div className="flex-1 min-w-0 flex rounded-tl-workspace overflow-hidden border-t border-l border-line-strong bg-surface-terminal/95">
            <aside
              className={`flex-shrink-0 overflow-hidden border-r border-line bg-fill/[0.025] transition-[width,opacity] duration-150 ${
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
                  onSelectWorktree={selectWorktree}
                  onCreateWorktree={createWorktreeForSelectedProject}
                  onRemoveWorktree={removeWorktreeForSelectedProject}
                  onOpenWorktreeTerminal={openWorktreeTerminal}
                  onReviewWorktree={reviewWorktree}
                  onRefreshWorktrees={refreshWorktrees}
                  isBusy={worktree.isLoading || isCreatingWorktree}
                  createRequest={taskCreateRequest}
                />
              </div>
            </aside>
            <div
              className="flex-1 min-w-0"
              onPointerDown={() => setShortcutFocusScope("workspace")}
              onFocusCapture={() => setShortcutFocusScope("workspace")}
            >
              <Workspace
                mainView={mainView}
                selectedProject={selectedWorkspaceProject}
                terminalFocusRequest={terminalFocusRequest}
                onSessionChange={setSession}
              />
            </div>
          </div>

          {rightPanel && (
            <div
              className="w-[320px] flex-shrink-0 border-t border-l border-line-strong"
              onPointerDown={() => setShortcutFocusScope("right-sidebar")}
              onFocusCapture={() => setShortcutFocusScope("right-sidebar")}
            >
              <Inspector panel={rightPanel} selectedProject={inspectorProject} />
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

export default App;
