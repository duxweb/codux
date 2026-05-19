import { Folder, GitBranch, ListChecks, Plus, SquareTerminal } from "../icons";
import { useEffect, useMemo, useState } from "react";
import { formatI18n, tm } from "../i18n";
import {
  listProjectOpenApplications,
  openProjectInApplication,
  revealProjectInFileManager,
  type ProjectOpenApplication,
} from "../ide";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import { PressableButton } from "./PressableButton";
import type { WorkspaceProject } from "../types";
import type { ProjectWorktreeSnapshot, WorktreeTaskSnapshot, WorktreeTaskStatus } from "../worktree/snapshot";

type TaskRow = {
  id: string;
  title: string;
  branch: string;
  changes?: number;
  outgoing?: number;
  incoming?: number;
  watermark?: { letter: string; tone: "running" | "review" | "ready" | "done" | "todo" };
  status: WorktreeTaskStatus;
  worktree: ProjectWorktreeSnapshot;
};

const watermarkTone: Record<NonNullable<TaskRow["watermark"]>["tone"], string> = {
  running: "text-brand-amber/12",
  review: "text-brand-blue/14",
  ready: "text-brand-amber/12",
  done: "text-brand-green/12",
  todo: "text-ink-faint/12",
};

type Props = {
  selectedProject?: WorkspaceProject;
  worktrees?: ProjectWorktreeSnapshot[];
  tasks?: WorktreeTaskSnapshot[];
  selectedWorktreeId?: string;
  onSelectWorktree?: (id: string) => void;
  onCreateWorktree?: (input: { branchName: string; taskTitle: string }) => void;
  onRemoveWorktree?: (worktree: ProjectWorktreeSnapshot) => void;
  onOpenWorktreeTerminal?: (worktree: ProjectWorktreeSnapshot) => void;
  onReviewWorktree?: (worktree: ProjectWorktreeSnapshot) => void;
  isBusy?: boolean;
  createRequest?: number;
};

export function TaskSidebar({
  selectedProject,
  worktrees = [],
  tasks = [],
  selectedWorktreeId,
  onSelectWorktree,
  onCreateWorktree,
  onRemoveWorktree,
  onOpenWorktreeTerminal,
  onReviewWorktree,
  isBusy,
  createRequest = 0,
}: Props) {
  const [isCreating, setCreating] = useState(false);
  const [branchName, setBranchName] = useState("");
  const [taskTitle, setTaskTitle] = useState("");
  const [applications, setApplications] = useState<ProjectOpenApplication[]>([]);
  const taskByWorktree = new Map(tasks.map((task) => [task.worktreeId, task]));
  const defaultWorktree =
    worktrees.find((worktree) => worktree.isDefault) ??
    fallbackDefaultWorktree(selectedProject);
  const taskRows = [defaultWorktree, ...worktrees.filter((worktree) => worktree.id !== defaultWorktree.id)]
    .map((worktree) => toTaskRow(worktree, taskByWorktree.get(worktree.id)));
  const canCreate = branchName.trim().length > 0 && !isBusy;

  useEffect(() => {
    let cancelled = false;
    void listProjectOpenApplications()
      .then((items) => {
        if (!cancelled) setApplications(items.filter((item) => item.installed));
      })
      .catch((error) => console.error("failed to load installed applications", error));
    return () => {
      cancelled = true;
    };
  }, []);

  useEffect(() => {
    if (!isCreating || branchName) return;
    setBranchName(`task/${timestampSlug()}`);
  }, [branchName, isCreating]);

  useEffect(() => {
    if (createRequest <= 0) return;
    setCreating(true);
    setBranchName(`task/${timestampSlug()}`);
    setTaskTitle("");
  }, [createRequest]);

  const submitCreate = () => {
    const nextBranch = branchName.trim();
    if (!nextBranch) return;
    onCreateWorktree?.({
      branchName: nextBranch,
      taskTitle: taskTitle.trim() || branchTitle(nextBranch),
    });
    setCreating(false);
    setBranchName("");
    setTaskTitle("");
  };

  return (
    <aside className="h-full flex flex-col">
      <div className="h-[42px] px-3.5 flex items-center justify-between flex-shrink-0">
        <span className="text-sm font-semibold tracking-tight">{tm("worktree.sidebar.title", "Tasks")}</span>
        <PressableButton
          className="w-6 h-6 grid place-items-center rounded-md text-ink-mute hover:text-ink hover:bg-fill/8 transition-colors"
          onPressUp={() => {
            setCreating(true);
            setBranchName(`task/${timestampSlug()}`);
            setTaskTitle("");
          }}
        >
          <Plus size={12} strokeWidth={2.4} />
        </PressableButton>
      </div>
      <div className="h-px bg-line mx-3 opacity-60" />

      <div className="flex-1 overflow-y-auto scrollbar-overlay px-2 pt-3 pb-2.5">
        {taskRows.length > 0 ? (
          taskRows.map((task) => (
            <TaskCard
              key={task.id}
              task={task}
              isSelected={(selectedWorktreeId ?? taskRows[0]?.id) === task.id}
              onSelect={() => onSelectWorktree?.(task.id)}
              onRemove={task.worktree.isDefault ? undefined : () => onRemoveWorktree?.(task.worktree)}
              onOpenTerminal={() => {
                onOpenWorktreeTerminal?.(task.worktree);
              }}
              onOpenFolder={() => {
                if (task.worktree.path) void revealProjectInFileManager(task.worktree.path);
              }}
              onReview={() => {
                onSelectWorktree?.(task.id);
                onReviewWorktree?.(task.worktree);
              }}
              applications={applications}
            />
          ))
        ) : (
          <div className="px-2 py-2 text-xs leading-relaxed text-ink-faint">
            {tm("worktree.sidebar.empty", "No task worktrees")}
          </div>
        )}
      </div>
      {isCreating && (
        <div className="fixed inset-0 z-[9000] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
          <form
            className="w-[min(360px,calc(100vw-32px))] rounded-[12px] border border-line-strong bg-surface-chrome p-4 shadow-pop"
            onSubmit={(event) => {
              event.preventDefault();
              if (canCreate) submitCreate();
            }}
          >
            <div className="mb-3">
              <div className="text-sm font-semibold text-ink">{tm("worktree.task.create", "New Task")}</div>
              <div className="mt-1 text-xs text-ink-faint">{selectedProject?.name ?? selectedProject?.path ?? ""}</div>
            </div>
            <label className="grid gap-1.5">
              <span className="text-[11px] font-semibold text-ink-soft">{tm("worktree.task.branch", "Task Branch")}</span>
              <input
                className="h-8 rounded-md border border-line bg-fill/[0.035] px-2.5 text-xs text-ink outline-none focus:border-brand-blue/60"
                value={branchName}
                onChange={(event) => setBranchName(event.currentTarget.value)}
                autoFocus
              />
            </label>
            <label className="mt-3 grid gap-1.5">
              <span className="text-[11px] font-semibold text-ink-soft">{tm("worktree.task.title", "Task Title")}</span>
              <input
                className="h-8 rounded-md border border-line bg-fill/[0.035] px-2.5 text-xs text-ink outline-none focus:border-brand-blue/60"
                value={taskTitle}
                placeholder={branchTitle(branchName)}
                onChange={(event) => setTaskTitle(event.currentTarget.value)}
              />
            </label>
            <div className="mt-4 flex justify-end gap-2">
              <PressableButton
                className="h-8 rounded-md px-3 text-xs font-semibold text-ink-soft hover:bg-fill/8 hover:text-ink"
                onPressUp={() => {
                  setCreating(false);
                  setTaskTitle("");
                }}
              >
                {tm("common.cancel", "Cancel")}
              </PressableButton>
              <PressableButton
                className="h-8 rounded-md bg-brand-blue px-3 text-xs font-semibold text-on-brand disabled:opacity-50"
                disabled={!canCreate}
                type="submit"
              >
                {tm("common.create", "Create")}
              </PressableButton>
            </div>
          </form>
        </div>
      )}
    </aside>
  );
}

function TaskCard({
  task,
  isSelected,
  onSelect,
  onRemove,
  onOpenTerminal,
  onOpenFolder,
  onReview,
  applications,
}: {
  task: TaskRow;
  isSelected?: boolean;
  onSelect?: () => void;
  onRemove?: () => void;
  onOpenTerminal?: () => void;
  onOpenFolder?: () => void;
  onReview?: () => void;
  applications: ProjectOpenApplication[];
}) {
  const contextMenu = useContextMenu();
  const ideApplications = useMemo(
    () => applications.filter((item) => item.id !== "terminal"),
    [applications],
  );
  const interactionBg = isSelected ? "bg-brand-blue/14" : "hover:bg-fill/4";
  const borderColor = isSelected ? "border-brand-blue/45" : "border-transparent";
  const openWithApplication = (application: ProjectOpenApplication) => {
    if (!task.worktree.path) return;
    void openProjectInApplication(task.worktree.path, application.id).catch((error) =>
      console.error("failed to open worktree in application", error),
    );
  };
  const menuItems = (
    <>
      <ContextMenuItem label={tm("worktree.menu.open_terminal", "Open Terminal")} onSelect={onOpenTerminal}>
        <SquareTerminal size={13} />
        {tm("worktree.menu.open_terminal", "Open Terminal")}
      </ContextMenuItem>
      <ContextMenuItem label={tm("worktree.menu.open_folder", "Open Folder")} onSelect={onOpenFolder} disabled={!task.worktree.path}>
        <Folder size={13} />
        {tm("worktree.menu.open_folder", "Open Folder")}
      </ContextMenuItem>
      <ContextMenuItem label={tm("worktree.menu.review", "Review")} onSelect={onReview}>
        <ListChecks size={13} />
        {tm("worktree.menu.review", "Review")}
      </ContextMenuItem>
      {ideApplications.length > 0 && (
        <>
          <ContextMenuSeparator />
          {ideApplications.map((application) => (
            <ContextMenuItem
              key={application.id}
              label={formatOpenTitle(application.label)}
              onSelect={() => openWithApplication(application)}
              disabled={!task.worktree.path}
            >
              {formatOpenTitle(application.label)}
            </ContextMenuItem>
          ))}
        </>
      )}
      {onRemove && (
        <>
          <ContextMenuSeparator />
          <ContextMenuItem label={tm("worktree.menu.remove", "Remove")} onSelect={onRemove}>
            {tm("worktree.menu.remove", "Remove")}
          </ContextMenuItem>
        </>
      )}
    </>
  );

  return (
    <div className="group relative mb-1.5" onContextMenu={contextMenu.openMenu}>
      <PressableButton
        onPressUp={onSelect}
        className={`relative w-full min-h-[52px] rounded-[8px] border ${borderColor} overflow-hidden text-left transition-colors`}
      >
      <span
        className={`absolute inset-0 rounded-[8px] ${interactionBg} transition-colors`}
      />

      {task.watermark && (
        <span
          className={`absolute right-2 top-1/2 -translate-y-1/2 text-[32px] font-black leading-none select-none pointer-events-none ${
            watermarkTone[task.watermark.tone]
          }`}
        >
          {task.watermark.letter}
        </span>
      )}

      <div className="relative flex items-center gap-2.5 px-2.5 py-2 h-[52px]">
        <span className="w-4 h-5 grid place-items-center flex-shrink-0">
          <span className="w-2.5 h-2.5 rounded-full bg-brand-blue" />
        </span>
        <div className="min-w-0 flex-1">
          <div
            className={`text-sm font-semibold leading-tight truncate ${
              isSelected ? "text-ink" : "text-ink-soft"
            }`}
          >
            {task.title}
          </div>
          <div className="mt-1 flex items-center gap-1.5 text-xs font-medium text-ink-faint">
            <GitBranch size={9} strokeWidth={2.2} />
            <span className="truncate">{task.branch}</span>
            <span className="tabular-nums">
              {formatI18n(tm("worktree.sidebar.changed_format", "%@ changed"), task.changes ?? 0)}
            </span>
            {task.incoming ? <span className="tabular-nums">↓{task.incoming}</span> : null}
            {task.outgoing ? <span className="tabular-nums">↑{task.outgoing}</span> : null}
          </div>
        </div>
      </div>
      </PressableButton>
      <ContextMenu
        ariaLabel={formatI18n(tm("worktree.menu.actions_format", "%@ Actions"), task.title)}
        menu={contextMenu.menu}
        onClose={contextMenu.closeMenu}
      >
        {menuItems}
      </ContextMenu>
    </div>
  );
}

function formatOpenTitle(label: string) {
  return tm("open.application.format", "Open in %@").replace("%@", label);
}

function fallbackDefaultWorktree(project?: WorkspaceProject): ProjectWorktreeSnapshot {
  return {
    id: project?.id ?? "main",
    projectId: project?.id ?? "",
    name: project?.branch ?? tm("worktree.branch.current", "current branch"),
    branch: project?.branch ?? tm("worktree.branch.current", "current branch"),
    path: project?.path ?? "",
    status: "todo",
    isDefault: true,
    createdAt: 0,
    updatedAt: 0,
    gitSummary: {
      changes: project?.changes ?? 0,
      incoming: 0,
      outgoing: 0,
    },
  };
}

function toTaskRow(
  worktree: ProjectWorktreeSnapshot,
  task: WorktreeTaskSnapshot | undefined,
): TaskRow {
  const status = task?.status ?? worktree.status;
  return {
    id: worktree.id,
    title: worktree.branch || worktree.name || task?.title || tm("worktree.task.default_title", "New Task"),
    branch: worktree.branch || worktree.path || tm("worktree.branch.current", "current branch"),
    changes: worktree.gitSummary.changes,
    incoming: worktree.gitSummary.incoming,
    outgoing: worktree.gitSummary.outgoing,
    watermark: watermarkForStatus(status),
    status,
    worktree,
  };
}

function watermarkForStatus(
  status: WorktreeTaskStatus,
): NonNullable<TaskRow["watermark"]> {
  if (status === "running" || status === "planning" || status === "waiting") {
    return { letter: "R", tone: "running" };
  }
  if (status === "review" || status === "blocked") {
    return { letter: "V", tone: "review" };
  }
  if (status === "ready") {
    return { letter: "A", tone: "ready" };
  }
  if (status === "done" || status === "merged") {
    return { letter: "D", tone: "done" };
  }
  return { letter: "T", tone: "todo" };
}

function timestampSlug() {
  const now = new Date();
  const pad = (value: number) => String(value).padStart(2, "0");
  return `${now.getFullYear()}${pad(now.getMonth() + 1)}${pad(now.getDate())}-${pad(now.getHours())}${pad(now.getMinutes())}${pad(now.getSeconds())}`;
}

function branchTitle(branch: string) {
  return branch.split("/").filter(Boolean).pop() || tm("worktree.task.default_title", "New Task");
}
