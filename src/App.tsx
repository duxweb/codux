import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { aggregateProjectPhase, phaseToAIState, resolveDisplayedProjectPhase } from "./ai/projectPhase";
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
import { installTerminalFocusEventTrace, logTerminalFocusDebug } from "./debug/terminalFocusDebug";
import {
  readCachedProjectListSnapshot,
  writeCachedProjectListSnapshot,
} from "./projectSnapshotCache";
import {
  dispatchShortcut,
  isConfiguredShortcut,
  registerShortcutHandler,
  type ShortcutScope,
} from "./shortcuts";
import { openAppWindow, revealMainAppWindow } from "./windowing";
import { listenWorkspaceCommand } from "./workspaceCommands";
import { useWorktreeSnapshot } from "./worktree/snapshot";
import { subscribeAppSettings } from "./settings";
import { systemConfirm } from "./systemDialog";
import { tm } from "./i18n";
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

type HydratableProject = ProjectSummary & Partial<Pick<WorkspaceProject, "branch" | "aiState" | "terminals" | "changes">>;

let projectListSnapshotPromise: Promise<ProjectListSnapshot> | null = null;
let remoteStatusPromise: Promise<RemoteStatus> | null = null;

function loadProjectListSnapshot() {
  projectListSnapshotPromise ??= invoke<ProjectListSnapshot>("project_list").finally(() => {
    projectListSnapshotPromise = null;
  });
  return projectListSnapshotPromise;
}

function loadRemoteStatus() {
  remoteStatusPromise ??= invoke<RemoteStatus>("remote_status").finally(() => {
    remoteStatusPromise = null;
  });
  return remoteStatusPromise;
}

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
      ? cachedProjectSnapshot?.selectedProjectId ?? initialProjects[0]?.id ?? ""
      : fallbackProjects[0]?.id ?? "",
  );
  const [mainView, setMainView] = useState<MainView>("terminal");
  const [isSidebarExpanded, setSidebarExpanded] = useState(false);
  const [isTaskSidebarExpanded, setTaskSidebarExpanded] = useState(true);
  const [rightPanel, setRightPanel] = useState<RightPanelKind | null>(null);
  const [_session, setSession] = useState<TerminalSession | null>(null);
  const [remoteStatus, setRemoteStatus] = useState<RemoteStatus | null>(null);
  const [selectedWorktreeByProject, setSelectedWorktreeByProject] = useState<Record<string, string>>({});
  const [terminalFocusRequest, setTerminalFocusRequest] = useState(0);
  const [taskCreateRequest, setTaskCreateRequest] = useState(0);
  const [aiVersion, setAiVersion] = useState(0);
  const focusScopeRef = useRef<ShortcutScope>("workspace");

  const applyProjectSnapshot = useCallback((snapshot: ProjectListSnapshot) => {
    writeCachedProjectListSnapshot(snapshot);
    const next = snapshot.projects.map(hydrate);
    setProjects(next);
    setSelectedWorktreeByProject(snapshot.selectedWorktreeIdByProject ?? {});
    setSelectedProjectId((current) => {
      if (snapshot.selectedProjectId && next.some((project) => project.id === snapshot.selectedProjectId)) {
        return snapshot.selectedProjectId;
      }
      if (current && next.some((project) => project.id === current)) {
        return current;
      }
      return next[0]?.id ?? "";
    });
  }, []);

  useEffect(() => {
    const unsubscribe = aiRuntime.subscribe(() => setAiVersion((current) => current + 1));
    if (!window.__TAURI_INTERNALS__) return unsubscribe;
    let isDisposed = false;
    let unlistenProjects: (() => void) | undefined;
    void loadProjectListSnapshot()
      .then(applyProjectSnapshot)
      .catch((error) => console.error("failed to load projects", error));

    void listen<ProjectListSnapshot>("project:updated", (event) => {
      applyProjectSnapshot(event.payload);
    }).then((unlisten) => {
      if (isDisposed) {
        unlisten();
        return;
      }
      unlistenProjects = unlisten;
    });

    void loadRemoteStatus()
      .then(setRemoteStatus)
      .catch((error) => console.error("failed to load remote status", error));
    return () => {
      isDisposed = true;
      unlistenProjects?.();
      unsubscribe();
    };
  }, [applyProjectSnapshot]);

  const projectsWithAIState = useMemo(
    () =>
      projects.map((project) => {
        const phase = aggregateProjectPhase(
          project.id,
          selectedWorktreeByProject[project.id],
          (id) => resolveDisplayedProjectPhase(aiRuntime.projectPhase(id), aiRuntime.completedPhase(id)),
        );
        return { ...project, aiState: phaseToAIState(phase) };
      }),
    [aiVersion, projects, selectedWorktreeByProject],
  );
  const pet = usePetLedger(projectsWithAIState);

  const selectedProjectWithAIState = useMemo(
    () =>
      projectsWithAIState.find((p) => p.id === selectedProjectId) ?? projectsWithAIState[0],
    [projectsWithAIState, selectedProjectId],
  );

  useEffect(() => subscribeAppSettings(() => void aiRuntime.start()), []);
  useEffect(
    () =>
      subscribeAppSettings(() => {
        if (!window.__TAURI_INTERNALS__) return;
        void loadRemoteStatus()
          .then(setRemoteStatus)
          .catch((error) => console.error("failed to load remote status", error));
      }),
    [],
  );
  const worktree = useWorktreeSnapshot(selectedProjectWithAIState);
  const worktreeSnapshot = worktree.snapshot;
  const selectedWorktreeId =
    selectedProjectWithAIState
      ? selectedWorktreeByProject[selectedProjectWithAIState.id] ||
        worktreeSnapshot.selectedWorktreeId ||
        selectedProjectWithAIState.id
      : "";
  const selectedWorktree =
    worktreeSnapshot.worktrees.find((item) => item.id === selectedWorktreeId) ??
    worktreeSnapshot.worktrees[0];
  const selectedWorktreeTask = selectedWorktree
    ? worktreeSnapshot.tasks.find((task) => task.worktreeId === selectedWorktree.id)
    : undefined;
  const selectedWorkspaceProject = useMemo<WorkspaceProject | undefined>(() => {
    if (!selectedProjectWithAIState) return undefined;
    if (!selectedWorktree) return selectedProjectWithAIState;
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
      baseBranch: selectedWorktreeTask?.baseBranch ?? null,
      isDefaultWorktree: selectedWorktree.isDefault,
      changes: selectedWorktree.gitSummary.changes,
      aiState: phaseToAIState(phase),
      badgeSymbol: selectedProjectWithAIState.badgeSymbol,
      badgeColorHex: selectedProjectWithAIState.badgeColorHex,
    };
  }, [aiVersion, selectedProjectWithAIState, selectedWorktree, selectedWorktreeTask]);

  useEffect(() => {
    if (!selectedProjectWithAIState || worktreeSnapshot.worktrees.length === 0) return;
    const current = selectedWorktreeByProject[selectedProjectWithAIState.id];
    if (current && worktreeSnapshot.worktrees.some((worktree) => worktree.id === current)) {
      return;
    }
    setSelectedWorktreeByProject((existing) => ({
      ...existing,
      [selectedProjectWithAIState.id]:
        worktreeSnapshot.selectedWorktreeId || worktreeSnapshot.worktrees[0].id,
    }));
  }, [selectedProjectWithAIState, selectedWorktreeByProject, worktreeSnapshot]);

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
          setSidebarExpanded((value) => {
            logTerminalFocusDebug("state:sidebar-toggle", { from: value, to: !value });
            return !value;
          });
        },
        toggleTasks: () => {
          setTaskSidebarExpanded((value) => {
            logTerminalFocusDebug("state:task-sidebar-toggle", { from: value, to: !value });
            return !value;
          });
        },
        toggleRightPanel,
        createTask: () => {
          setTaskSidebarExpanded(true);
          setShortcutFocusScope("task-sidebar");
          setTaskCreateRequest((value) => value + 1);
        },
        openProjectFolder: () => {
          void openProjectFolderFromMenu()
            .then((snapshot) => {
              if (snapshot) applyProjectSnapshot(snapshot);
            })
            .catch((error) => console.error("failed to open project folder", error));
        },
        closeCurrentProject: () => {
          void closeProjectFromMenu(selectedProjectWithAIState)
            .then((snapshot) => {
              if (snapshot) applyProjectSnapshot(snapshot);
            })
            .catch((error) => console.error("failed to close project", error));
        },
        closeAllProjects: () => {
          void closeAllProjectsFromMenu(projectsWithAIState)
            .then((snapshot) => {
              if (snapshot) applyProjectSnapshot(snapshot);
            })
            .catch((error) => console.error("failed to close all projects", error));
        },
      }),
    [applyProjectSnapshot, projectsWithAIState, requestTerminalFocus, selectedProjectWithAIState],
  );

  useEffect(() => {
    return installTerminalFocusEventTrace();
  }, []);

  const selectProject = (id: string) => {
    aiRuntime.dismissCompletion(id);
    const worktreeId = selectedWorktreeByProject[id];
    if (worktreeId && worktreeId !== id) {
      aiRuntime.dismissCompletion(worktreeId);
    }
    setSelectedProjectId(id);
    if (window.__TAURI_INTERNALS__) {
      void invoke<ProjectListSnapshot>("project_select", { projectId: id })
        .then(applyProjectSnapshot)
        .catch((error) => console.error("failed to select project", error));
    }
    if (mainView === "terminal") {
      requestTerminalFocus();
    }
  };

  const selectWorktree = (id: string) => {
    if (!selectedProjectWithAIState) return;
    setSelectedWorktreeByProject((existing) => ({
      ...existing,
      [selectedProjectWithAIState.id]: id,
    }));
    if (window.__TAURI_INTERNALS__) {
      void invoke<ProjectListSnapshot>("project_select_worktree", {
        request: {
          projectId: selectedProjectWithAIState.id,
          worktreeId: id,
        },
      }).catch((error) => console.error("failed to select worktree", error));
    }
    if (mainView === "terminal") {
      requestTerminalFocus();
    }
  };

  const createWorktreeForSelectedProject = async (input?: { branchName: string; taskTitle: string }) => {
    if (!selectedProjectWithAIState) return;
    const branchName = input?.branchName.trim();
    if (!branchName) return;
    const taskTitle = input?.taskTitle.trim() || branchName.split("/").filter(Boolean).pop() || "New Task";
    try {
      const next = await worktree.create({
        projectId: selectedProjectWithAIState.id,
        projectPath: selectedProjectWithAIState.path,
        baseBranch: selectedWorktree?.branch || selectedProjectWithAIState.branch,
        branchName,
        taskTitle,
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
    }
  };

  const removeWorktreeForSelectedProject = async (target: ProjectWorktreeSnapshot) => {
    if (!selectedProjectWithAIState || target.isDefault) return;
    if (
      !(await systemConfirm(tm("worktree.remove.message_format", "Remove %@ from Codux and the Git worktree list? The branch will not be deleted.").replace("%@", target.branch || target.name), {
        title: tm("worktree.remove.title", "Remove Worktree"),
        kind: "warning",
        okLabel: tm("worktree.menu.remove", "Remove"),
        cancelLabel: tm("common.cancel", "Cancel"),
      }))
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
  };

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
        const nextIndex =
          (currentIndex + delta + projectsWithAIState.length) % projectsWithAIState.length;
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
      }),
    [],
  );

  return (
    <main className="relative w-screen h-screen overflow-hidden text-ink">
      <Titlebar
        projects={projectsWithAIState}
        mainView={mainView}
        setMainView={setMainView}
        isSidebarExpanded={isSidebarExpanded}
        toggleSidebar={() => {
          setSidebarExpanded((value) => {
            logTerminalFocusDebug("state:sidebar-toggle", { from: value, to: !value });
            return !value;
          });
        }}
        isTaskSidebarExpanded={isTaskSidebarExpanded}
        toggleTaskSidebar={() => {
          setTaskSidebarExpanded((value) => {
            logTerminalFocusDebug("state:task-sidebar-toggle", { from: value, to: !value });
            return !value;
          });
        }}
        rightPanel={rightPanel}
        toggleRightPanel={toggleRightPanel}
        remoteStatus={remoteStatus}
        pet={pet}
      />

      <div
        className="absolute inset-x-0 bottom-0 flex"
        style={{ top: "var(--titlebar-height)" }}
      >
        <ProjectSidebar
          projects={projectsWithAIState}
          selectedProjectId={selectedProjectWithAIState?.id ?? ""}
          onSelect={selectProject}
          isExpanded={isSidebarExpanded}
          onFocusScope={() => setShortcutFocusScope("project-sidebar")}
          onCreateProject={() => void openAppWindow("project-create")}
          onOpenSettings={() => void openAppWindow("settings")}
        />

        <div className="flex-1 min-w-0 flex">
          <div className="flex-1 min-w-0 flex rounded-tl-workspace overflow-hidden border-t border-l border-line-strong bg-surface-terminal">
            {isTaskSidebarExpanded && (
              <aside
                className="w-[216px] flex-shrink-0 border-r border-line bg-fill/[0.025]"
                onPointerDown={() => setShortcutFocusScope("task-sidebar")}
                onFocusCapture={() => setShortcutFocusScope("task-sidebar")}
              >
                <TaskSidebar
                  selectedProject={selectedProjectWithAIState}
                  worktrees={worktreeSnapshot.worktrees}
                  tasks={worktreeSnapshot.tasks}
                  selectedWorktreeId={selectedWorktreeId}
                  onSelectWorktree={selectWorktree}
                  onCreateWorktree={createWorktreeForSelectedProject}
                  onRemoveWorktree={removeWorktreeForSelectedProject}
                  isBusy={worktree.isLoading}
                  createRequest={taskCreateRequest}
                />
              </aside>
            )}
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
              <Inspector panel={rightPanel} selectedProject={selectedWorkspaceProject} />
            </div>
          )}
        </div>
      </div>
    </main>
  );
}

export default App;
