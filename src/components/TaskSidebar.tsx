import { GitBranch, MoreHorizontal, Plus } from "../icons";
import { useEffect, useState } from "react";
import { formatI18n, tm } from "../i18n";
import { DesktopMenu, DesktopMenuItem } from "./DesktopMenu";
import { PressableButton } from "./PressableButton";
import { Tooltip } from "./Tooltip";
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
  isBusy,
  createRequest = 0,
}: Props) {
  const [isCreating, setCreating] = useState(false);
  const [branchName, setBranchName] = useState("");
  const [taskTitle, setTaskTitle] = useState("");
  const taskByWorktree = new Map(tasks.map((task) => [task.worktreeId, task]));
  const defaultWorktree =
    worktrees.find((worktree) => worktree.isDefault) ??
    fallbackDefaultWorktree(selectedProject);
  const taskRows = [defaultWorktree, ...worktrees.filter((worktree) => worktree.id !== defaultWorktree.id)]
    .map((worktree) => toTaskRow(worktree, taskByWorktree.get(worktree.id)));
  const canCreate = branchName.trim().length > 0 && !isBusy;

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
          onPressUp={() => setCreating((value) => !value)}
        >
          <Plus size={12} strokeWidth={2.4} />
        </PressableButton>
      </div>
      <div className="h-px bg-line mx-3 opacity-60" />

      <div className="flex-1 overflow-y-auto scrollbar-overlay px-2 pt-3 pb-2.5">
        {isCreating && (
          <form
            className="mb-2 rounded-[8px] border border-line bg-fill/[0.04] p-2.5"
            onSubmit={(event) => {
              event.preventDefault();
              if (canCreate) submitCreate();
            }}
          >
            <label className="grid gap-1">
              <span className="text-[11px] font-semibold text-ink-soft">{tm("worktree.task.branch", "Task Branch")}</span>
              <input
                className="h-7 rounded-md border border-line bg-surface-chrome/55 px-2 text-xs text-ink outline-none focus:border-brand-blue/60"
                value={branchName}
                onChange={(event) => setBranchName(event.currentTarget.value)}
                autoFocus
              />
            </label>
            <label className="mt-2 grid gap-1">
              <span className="text-[11px] font-semibold text-ink-soft">{tm("worktree.task.title", "Task Title")}</span>
              <input
                className="h-7 rounded-md border border-line bg-surface-chrome/55 px-2 text-xs text-ink outline-none focus:border-brand-blue/60"
                value={taskTitle}
                placeholder={branchTitle(branchName)}
                onChange={(event) => setTaskTitle(event.currentTarget.value)}
              />
            </label>
            <div className="mt-2 flex justify-end gap-1.5">
              <PressableButton
                className="h-6 rounded-md px-2 text-xs font-semibold text-ink-soft hover:bg-fill/8 hover:text-ink"
                onPressUp={() => {
                  setCreating(false);
                  setTaskTitle("");
                }}
              >
                {tm("common.cancel", "Cancel")}
              </PressableButton>
              <PressableButton
                className="h-6 rounded-md bg-brand-blue px-2 text-xs font-semibold text-on-brand disabled:opacity-50"
                disabled={!canCreate}
                type="submit"
              >
                {tm("common.create", "Create")}
              </PressableButton>
            </div>
          </form>
        )}
        {taskRows.length > 0 ? (
          taskRows.map((task) => (
            <TaskCard
              key={task.id}
              task={task}
              isSelected={(selectedWorktreeId ?? taskRows[0]?.id) === task.id}
              onSelect={() => onSelectWorktree?.(task.id)}
              onRemove={task.worktree.isDefault ? undefined : () => onRemoveWorktree?.(task.worktree)}
            />
          ))
        ) : (
          <div className="px-2 py-2 text-xs leading-relaxed text-ink-faint">
            {tm("worktree.sidebar.empty", "No task worktrees")}
          </div>
        )}
      </div>
    </aside>
  );
}

function TaskCard({
  task,
  isSelected,
  onSelect,
  onRemove,
}: {
  task: TaskRow;
  isSelected?: boolean;
  onSelect?: () => void;
  onRemove?: () => void;
}) {
  const [menuOpen, setMenuOpen] = useState(false);
  const interactionBg = isSelected ? "bg-brand-blue/14" : "hover:bg-fill/4";
  const borderColor = isSelected ? "border-brand-blue/45" : "border-transparent";

  return (
    <div className="group relative mb-1.5">
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
      {onRemove && (
        <div className="absolute right-1 top-1/2 flex -translate-y-1/2 rounded bg-surface-chrome/95 opacity-0 pointer-events-none transition-opacity group-hover:opacity-100 group-hover:pointer-events-auto">
          <DesktopMenu
            ariaLabel={tm("files.panel.actions", "Actions")}
            isOpen={menuOpen}
            onOpenChange={setMenuOpen}
            trigger={
              <button
                type="button"
                className="grid h-6 w-6 place-items-center rounded text-ink-faint hover:bg-fill/8 hover:text-ink"
                aria-label={tm("files.panel.actions", "Actions")}
              >
                <MoreHorizontal size={13} />
              </button>
            }
          >
            <DesktopMenuItem label={tm("worktree.menu.remove", "Remove")} onSelect={onRemove}>{tm("worktree.menu.remove", "Remove")}</DesktopMenuItem>
          </DesktopMenu>
        </div>
      )}
    </div>
  );
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
