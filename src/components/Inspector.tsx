import {
  ArrowDownToLine,
  ArrowUpFromLine,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderPlus,
  GitBranch,
  KeyRound,
  Minus,
  Plus,
  RefreshCw,
  Server,
  Sparkles,
  Undo2,
  X,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { Button as HeroButton, Dropdown, Modal, ProgressBar, Spinner } from "@heroui/react";
import {
  memo,
  useCallback,
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type Key,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
} from "react";
import {
  useAIHistorySnapshot,
  type AIHeatmapDay,
  type AIHistorySessionSummary,
  type AIHistorySnapshot,
  type AITimeBucket,
  type AIUsageBreakdownItem,
} from "../ai/history";
import { aiIndexingPresentation } from "../ai/panelPresentation";
import { useAIRuntimeSnapshot, type AISessionSnapshot } from "../ai/runtime";
import {
  copyFile,
  createDirectory,
  createFile,
  deleteFile,
  importExternalFiles,
  listFileChildren,
  revealFile,
  renameFile,
  unwatchProjectFiles,
  watchProjectFiles,
  type FileChangeEvent,
  type FileEntry,
} from "../files/api";
import {
  useGitStatusSnapshot,
  type GitCommitAction,
  type GitCommitSummary,
  type GitFileStatus,
  type GitStatusSnapshot,
} from "../git/status";
import { Button } from "./Button";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import { DesktopMenu, DesktopMenuItem, DesktopMenuSeparator, DesktopSubmenu } from "./DesktopMenu";
import { Select, Textarea, TextInput } from "./Form";
import { PressableButton } from "./PressableButton";
import {
  PanelButton,
  PanelCard,
  PanelEmptyState,
  PanelHeader,
  PanelIconButton,
  PanelSection,
  PanelStatusBar,
} from "./PanelKit";
import { Tooltip } from "./Tooltip";
import type { RightPanelKind, WorkspaceProject } from "../types";
import { broadcastWorkspaceCommand } from "../workspaceCommands";
import { openGitDiffWindow } from "../windowing";
import { systemConfirm } from "../systemDialog";
import { formatI18n, tm } from "../i18n";
import { openLocalizedDialog } from "../localizedDialog";
import { readAppSettings, subscribeAppSettings, type AISettings, type AIStatisticsMode } from "../settings";
import { revealProjectInFileManager } from "../ide";

type Props = {
  panel: RightPanelKind;
  selectedProject?: WorkspaceProject;
};

export function Inspector({ panel, selectedProject }: Props) {
  return (
    <aside className="h-full min-w-0 flex flex-col bg-surface-chrome/35 backdrop-blur-md">
      {panel === "git" && <GitPanel project={selectedProject} />}
      {panel === "files" && <FilesPanel project={selectedProject} />}
      {panel === "ai" && <MemoAIPanel project={selectedProject} />}
      {panel === "ssh" && <SSHPanel project={selectedProject} />}
    </aside>
  );
}

function SectionHeader({
  open,
  setOpen,
  title,
  count,
  actions,
}: {
  open: boolean;
  setOpen: (v: boolean) => void;
  title: string;
  count?: number;
  actions?: ReactNode;
}) {
  return (
    <div className="sticky top-0 z-20 h-[34px] flex items-center justify-between border-b border-line/80 bg-surface-chrome px-3.5 text-xs text-ink-soft shadow-[0_1px_0_rgb(0_0_0_/_0.12)]">
      <button
        onClick={() => setOpen(!open)}
        className="min-w-0 h-full flex flex-1 items-center gap-2 text-left transition-colors hover:text-ink"
      >
        {open ? (
          <ChevronDown size={12} className="flex-shrink-0 text-ink-mute" />
        ) : (
          <ChevronRight size={12} className="flex-shrink-0 text-ink-mute" />
        )}
        <span className="truncate font-semibold">{title}</span>
      </button>
      <div className="flex items-center gap-1">
        {actions}
        {count != null && <span className="min-w-4 text-right text-xs text-ink-faint tabular-nums">{count}</span>}
      </div>
    </div>
  );
}

function HeaderActionButton({
  icon: Icon,
  label,
  disabled,
  onPress,
}: {
  icon: (props: { size?: number; strokeWidth?: number; className?: string }) => ReactNode;
  label: string;
  disabled?: boolean;
  onPress: () => void;
}) {
  return (
    <Tooltip label={label} placement="bottom">
      <HeroButton
        size="sm"
        variant="ghost"
        isIconOnly
        isDisabled={disabled}
        className="h-5 w-5 min-w-5 rounded px-0 text-ink-faint hover:text-ink"
        onPress={onPress}
      >
        <Icon size={11} strokeWidth={2.2} />
      </HeroButton>
    </Tooltip>
  );
}

const MIN_REFRESH_FEEDBACK_MS = 650;
const AI_REFRESH_FEEDBACK_MS = 1000;
const FILE_TREE_WATCH_DEBOUNCE_MS = 220;

function useRefreshFeedback(refresh: () => Promise<unknown>, minVisibleMs = MIN_REFRESH_FEEDBACK_MS) {
  const [isRefreshing, setRefreshing] = useState(false);
  const mountedRef = useRef(true);

  useEffect(() => {
    mountedRef.current = true;
    return () => {
      mountedRef.current = false;
    };
  }, []);

  const run = useCallback(async () => {
    const startedAt = Date.now();
    setRefreshing(true);
    try {
      await refresh();
    } finally {
      const elapsed = Date.now() - startedAt;
      const remaining = minVisibleMs - elapsed;
      if (remaining > 0) {
        await new Promise((resolve) => window.setTimeout(resolve, remaining));
      }
      if (mountedRef.current) {
        setRefreshing(false);
      }
    }
  }, [minVisibleMs, refresh]);

  return [isRefreshing, run] as const;
}

type GitInputState = {
  title: string;
  message?: string;
  label: string;
  value: string;
  secondaryLabel?: string;
  secondaryValue?: string;
  multiline?: boolean;
  onSubmit: (value: string, secondaryValue: string) => Promise<void>;
};

function GitInputPanel({
  input,
  onChange,
  onCancel,
  onSubmit,
}: {
  input: GitInputState;
  onChange: (input: GitInputState) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const canSubmit =
    input.value.trim().length > 0 && (input.secondaryLabel ? (input.secondaryValue ?? "").trim().length > 0 : true);
  const controlClass =
    "w-full rounded-md border border-line bg-surface-chrome/65 px-2 text-xs text-ink outline-none focus:border-brand-blue/60";

  return (
    <div className="mx-3 mt-3 rounded-[10px] border border-line bg-fill/[0.04] p-3">
      <div className="text-xs font-semibold text-ink">{input.title}</div>
      {input.message ? <div className="mt-1 text-[11px] leading-relaxed text-ink-faint">{input.message}</div> : null}
      <form
        className="mt-2 grid gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit) onSubmit();
        }}
      >
        <label className="grid gap-1">
          <span className="text-[11px] font-semibold text-ink-soft">{input.label}</span>
          {input.multiline ? (
            <textarea
              className={`${controlClass} min-h-[72px] py-1.5 resize-none`}
              value={input.value}
              autoFocus
              onChange={(event) => onChange({ ...input, value: event.currentTarget.value })}
            />
          ) : (
            <input
              className={`${controlClass} h-7`}
              value={input.value}
              autoFocus
              onFocus={(event) => event.currentTarget.select()}
              onChange={(event) => onChange({ ...input, value: event.currentTarget.value })}
            />
          )}
        </label>
        {input.secondaryLabel && (
          <label className="grid gap-1">
            <span className="text-[11px] font-semibold text-ink-soft">{input.secondaryLabel}</span>
            <input
              className={`${controlClass} h-7`}
              value={input.secondaryValue ?? ""}
              onChange={(event) => onChange({ ...input, secondaryValue: event.currentTarget.value })}
            />
          </label>
        )}
        <div className="mt-1 flex justify-end gap-1.5">
          <PressableButton
            className="h-6 rounded-md px-2 text-xs font-semibold text-ink-soft hover:bg-fill/8 hover:text-ink"
            onPressUp={onCancel}
          >
            {tm("common.cancel", "Cancel")}
          </PressableButton>
          <PressableButton
            className="h-6 rounded-md bg-brand-blue px-2 text-xs font-semibold text-on-brand disabled:opacity-50"
            disabled={!canSubmit}
            type="submit"
          >
            {tm("common.continue", "Continue")}
          </PressableButton>
        </div>
      </form>
    </div>
  );
}

function GitPanel({ project }: { project?: WorkspaceProject }) {
  const [stagedOpen, setStagedOpen] = useState(false);
  const [changesOpen, setChangesOpen] = useState(true);
  const [untrackedOpen, setUntrackedOpen] = useState(true);
  const [expandedGitFilePaths, setExpandedGitFilePaths] = useState<Record<string, Set<string>>>({
    staged: new Set(),
    unstaged: new Set(),
    untracked: new Set(),
  });
  const previousGitDirectoryPathsRef = useRef<Record<GitFileSectionKind, Set<string>>>({
    staged: new Set(),
    unstaged: new Set(),
    untracked: new Set(),
  });
  const [commitMessage, setCommitMessage] = useState("");
  const [commitAction, setCommitAction] = useState<GitCommitAction>("commit");
  const [selectedFileIds, setSelectedFileIds] = useState<Set<string>>(new Set());
  const [selectionAnchorFileId, setSelectionAnchorFileId] = useState("");
  const [selectedCommitHash, setSelectedCommitHash] = useState("");
  const [gitInput, setGitInput] = useState<GitInputState | null>(null);
  const [branchMenuOpen, setBranchMenuOpen] = useState(false);
  const [commitMenuOpen, setCommitMenuOpen] = useState(false);
  const [isSubmittingCommit, setSubmittingCommit] = useState(false);
  const [isGeneratingCommitMessage, setGeneratingCommitMessage] = useState(false);
  const [gitHistoryHeight, setGitHistoryHeight] = useState(190);
  const gitContentRef = useRef<HTMLDivElement | null>(null);
  const git = useGitStatusSnapshot(project);
  const [isManualRefreshing, refreshGit] = useRefreshFeedback(git.refresh);
  const isRefreshingGit = git.isLoading || isManualRefreshing;
  const snapshot = git.snapshot;
  const hasUpstream = Boolean(snapshot.upstream);
  const hasRemotes = snapshot.remotes.length > 0;
  const canUseCurrentBranchRemote = hasUpstream && snapshot.branch !== "HEAD" && snapshot.branch !== "uninitialized";
  const canCommit = snapshot.staged.length > 0 && commitMessage.trim().length > 0 && !isSubmittingCommit;
  const statusLabel = !snapshot.isRepository
    ? tm("git.repository.not_repository", "Current project is not a Git repository.")
    : hasUpstream
      ? snapshot.behind === 0 && snapshot.ahead === 0
        ? tm("git.remote.status.synced", "Remote Is Synced")
        : tm("git.remote.status.has_updates", "Remote Has Updates")
      : tm("git.remote.status.no_remote_branch", "No Remote Branch");
  const statusTone = snapshot.isRepository && hasUpstream ? "info" : "neutral";
  const statusButtonTone = statusTone === "info" ? "ghost" : "neutral";
  const StatusIcon =
    snapshot.isRepository && hasUpstream && snapshot.behind === 0 && snapshot.ahead === 0 ? CheckCircle2 : ChevronRight;
  const commitActionLabel = gitCommitActionLabel(commitAction);
  const remoteBranchesByRemote = useMemo(() => {
    return groupRemoteBranches(snapshot.remoteBranches);
  }, [snapshot.remoteBranches]);
  const localMergeCandidates = useMemo(
    () => snapshot.branches.filter((branch) => branch.name !== snapshot.branch),
    [snapshot.branch, snapshot.branches],
  );
  const visibleGitFiles = useMemo(
    () => [
      ...(stagedOpen ? flattenGitFileTree(buildGitFileTree(snapshot.staged), "staged") : []),
      ...(changesOpen ? flattenGitFileTree(buildGitFileTree(snapshot.unstaged), "unstaged") : []),
      ...(untrackedOpen ? flattenGitFileTree(buildGitFileTree(snapshot.untracked), "untracked") : []),
    ],
    [changesOpen, snapshot.staged, snapshot.unstaged, snapshot.untracked, stagedOpen, untrackedOpen],
  );
  const selectedGitFiles = useMemo(
    () => visibleGitFiles.filter((item) => selectedFileIds.has(item.id)),
    [selectedFileIds, visibleGitFiles],
  );
  const selectedByKind = useMemo(() => {
    const grouped: Record<GitFileSectionKind, GitFileStatus[]> = {
      staged: [],
      unstaged: [],
      untracked: [],
    };
    for (const item of selectedGitFiles) {
      grouped[item.kind].push(item.file);
    }
    return grouped;
  }, [selectedGitFiles]);
  useEffect(() => {
    previousGitDirectoryPathsRef.current = {
      staged: new Set(),
      unstaged: new Set(),
      untracked: new Set(),
    };
    setSelectedCommitHash("");
    setSelectedFileIds(new Set());
    setSelectionAnchorFileId("");
    setExpandedGitFilePaths({
      staged: new Set(),
      unstaged: new Set(),
      untracked: new Set(),
    });
  }, [project?.path]);
  useEffect(() => {
    if (canUseCurrentBranchRemote || commitAction === "commit") return;
    setCommitAction("commit");
  }, [canUseCurrentBranchRemote, commitAction]);
  useEffect(() => {
    const nextAvailable = {
      staged: collectGitDirectoryPaths(snapshot.staged),
      unstaged: collectGitDirectoryPaths(snapshot.unstaged),
      untracked: collectGitDirectoryPaths(snapshot.untracked),
    };
    const previousAvailable = previousGitDirectoryPathsRef.current;
    setExpandedGitFilePaths((current) => ({
      staged: mergeGitDirectoryPaths(current.staged, nextAvailable.staged, previousAvailable.staged),
      unstaged: mergeGitDirectoryPaths(current.unstaged, nextAvailable.unstaged, previousAvailable.unstaged),
      untracked: mergeGitDirectoryPaths(current.untracked, nextAvailable.untracked, previousAvailable.untracked),
    }));
    previousGitDirectoryPathsRef.current = nextAvailable;
  }, [snapshot.staged, snapshot.unstaged, snapshot.untracked]);
  const toggleGitDirectory = (kind: GitFileSectionKind, path: string) => {
    setExpandedGitFilePaths((current) => {
      const nextPaths = new Set(current[kind]);
      if (nextPaths.has(path)) {
        nextPaths.delete(path);
      } else {
        nextPaths.add(path);
      }
      return { ...current, [kind]: nextPaths };
    });
  };
  const openGitInput = (input: GitInputState) => setGitInput(input);
  const closeGitInput = () => setGitInput(null);
  const submitGitInput = async () => {
    if (!gitInput) return;
    const primary = gitInput.value.trim();
    const secondary = gitInput.secondaryValue?.trim() ?? "";
    if (!primary) return;
    setGitInput(null);
    await gitInput.onSubmit(primary, secondary);
  };
  const createBranch = () => {
    const seed = `worktree/${new Date().toISOString().slice(0, 10)}`;
    openGitInput({
      title: tm("git.branch.new", "New Branch"),
      message: tm("git.branch.new.message", "Enter a new branch name."),
      label: tm("git.branch.new", "New Branch"),
      value: seed,
      onSubmit: async (branch) => {
        await git.createBranch(branch, true);
      },
    });
  };
  const createBranchFromCommit = (commit: GitCommitSummary) => {
    openGitInput({
      title: tm("git.branch.create_from_commit.title", "Create Branch from Commit"),
      label: tm("git.branch.new", "New Branch"),
      value: `restore/${commit.hash.slice(0, 7)}`,
      onSubmit: async (branch) => {
        await git.createBranch(branch, true, commit.hash);
      },
    });
  };
  const addRemote = () => {
    openGitInput({
      title: tm("git.remote.add", "Add Remote"),
      label: tm("git.remote.name", "Remote Name"),
      value: "origin",
      secondaryLabel: tm("git.remote.add.url_message", "Remote URL"),
      secondaryValue: "",
      onSubmit: async (name, url) => {
        if (url) await git.addRemote(name, url);
      },
    });
  };
  const removeRemote = () => {
    openGitInput({
      title: tm("git.remote.remove", "Remove Remote"),
      message: snapshot.remotes.map((remote) => remote.name).join(", "),
      label: tm("git.remote.name", "Remote Name"),
      value: snapshot.remotes[0]?.name ?? "",
      onSubmit: async (name) => {
        if (
          await systemConfirm(formatI18n(tm("git.remote.remove.confirm_format", "Remove remote %@?"), name), {
            title: tm("git.remote.remove", "Remove Remote"),
            kind: "warning",
            okLabel: tm("common.delete", "Delete"),
            cancelLabel: tm("common.cancel", "Cancel"),
          })
        )
          await git.removeRemote(name);
      },
    });
  };
  const pushRemote = () => {
    openGitInput({
      title: tm("git.remote.push_to", "Push To..."),
      message: snapshot.remotes.map((remote) => remote.name).join(", "),
      label: tm("git.remote.name", "Remote Name"),
      value: snapshot.remotes[0]?.name ?? "origin",
      onSubmit: async (remote) => {
        await git.pushRemote(remote);
      },
    });
  };
  const runBranchAction = async (key: Key) => {
    const rawKey = String(key);
    if (rawKey.startsWith("checkoutLocal:")) {
      const branch = rawKey.slice("checkoutLocal:".length);
      if (branch && branch !== snapshot.branch) await git.checkoutBranch(branch);
      return;
    }
    if (rawKey.startsWith("checkoutRemote:")) {
      const branch = rawKey.slice("checkoutRemote:".length);
      if (branch) await git.checkoutRemoteBranch(branch);
      return;
    }
    if (rawKey.startsWith("pushRemote:")) {
      const remote = rawKey.slice("pushRemote:".length);
      if (remote) await git.pushRemote(remote);
      return;
    }
    if (rawKey.startsWith("setDefaultRemote:")) {
      const remote = rawKey.slice("setDefaultRemote:".length);
      if (project?.rootProjectId || project?.id) {
        await invoke("project_set_default_push_remote", {
          request: {
            projectId: project.rootProjectId ?? project.id,
            remoteName: project.gitDefaultPushRemoteName === remote ? null : remote,
          },
        });
      }
      return;
    }
    if (rawKey.startsWith("pushRemoteBranch:")) {
      const remoteBranch = rawKey.slice("pushRemoteBranch:".length);
      if (remoteBranch) await git.pushRemoteBranch(remoteBranch, snapshot.branch);
      return;
    }
    if (rawKey.startsWith("mergeLocal:")) {
      const branch = rawKey.slice("mergeLocal:".length);
      if (branch) await git.mergeBranch(branch);
      return;
    }
    if (rawKey.startsWith("squashLocal:")) {
      const branch = rawKey.slice("squashLocal:".length);
      if (branch) await git.squashMergeBranch(branch);
      return;
    }
    if (rawKey.startsWith("deleteLocal:")) {
      const branch = rawKey.slice("deleteLocal:".length);
      if (!branch || branch === snapshot.branch) return;
      const force = await systemConfirm(
        formatI18n(tm("git.branch.delete.confirm_format", "Delete local branch %@?"), branch),
        {
          title: tm("git.branch.delete_local", "Delete Local Branch"),
          kind: "warning",
          okLabel: tm("common.delete", "Delete"),
          cancelLabel: tm("common.cancel", "Cancel"),
        },
      );
      if (force) await git.deleteBranch(branch, false);
      return;
    }
    switch (String(key)) {
      case "create":
        createBranch();
        return;
      case "fetch":
        await git.fetch();
        return;
      case "pull":
        await git.pull();
        return;
      case "push":
        await git.push();
        return;
      case "forcePush":
        if (
          await systemConfirm(tm("git.remote.force_push.message", "Overwrite the current remote branch?"), {
            title: tm("git.remote.force_push", "Force Push"),
            kind: "warning",
            okLabel: tm("git.remote.force_push", "Force Push"),
            cancelLabel: tm("common.cancel", "Cancel"),
          })
        )
          await git.forcePush();
        return;
      case "undoLastCommit":
        if (snapshot.commits[0]) await runCommitAction(snapshot.commits[0], "undo");
        return;
      case "editLastCommitMessage":
        if (snapshot.commits[0]) await runCommitAction(snapshot.commits[0], "amend");
        return;
      case "showRepository":
        if (project?.path) void revealProjectInFileManager(project.path);
        return;
      case "addRemote":
        addRemote();
        return;
      case "removeRemote":
        removeRemote();
        return;
      case "pushRemote":
        pushRemote();
        return;
    }
  };
  const gitFileSelectionId = (file: GitFileStatus, staged: boolean) =>
    `${staged ? "staged" : file.indexStatus === "?" ? "untracked" : "unstaged"}:${file.path}`;

  const selectGitFile = (file: GitFileStatus, staged: boolean, modifiers?: { extend?: boolean; toggle?: boolean }) => {
    const id = gitFileSelectionId(file, staged);
    if (modifiers?.extend && selectionAnchorFileId) {
      const anchorIndex = visibleGitFiles.findIndex((item) => item.id === selectionAnchorFileId);
      const targetIndex = visibleGitFiles.findIndex((item) => item.id === id);
      if (anchorIndex >= 0 && targetIndex >= 0) {
        const [start, end] = anchorIndex < targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
        setSelectedFileIds(new Set(visibleGitFiles.slice(start, end + 1).map((item) => item.id)));
        return;
      }
    }
    if (modifiers?.toggle) {
      setSelectedFileIds((current) => {
        const next = new Set(current);
        if (next.has(id)) {
          next.delete(id);
        } else {
          next.add(id);
        }
        return next.size > 0 ? next : new Set([id]);
      });
      setSelectionAnchorFileId(id);
      return;
    }
    setSelectedFileIds(new Set([id]));
    setSelectionAnchorFileId(id);
  };

  const previewDiff = async (file: GitFileStatus, staged: boolean) => {
    selectGitFile(file, staged);
    if (!project?.path) return;
    await openGitDiffWindow({
      projectPath: project.path,
      path: file.path,
      staged,
    });
  };
  const submitCommit = async () => {
    const message = commitMessage.trim();
    if (!message) return;
    setSubmittingCommit(true);
    try {
      await git.commitAction(message, commitAction);
      setCommitMessage("");
    } finally {
      setSubmittingCommit(false);
    }
  };
  const runCommitAction = async (commit: GitCommitSummary, key: Key) => {
    switch (String(key)) {
      case "copy":
        await navigator.clipboard?.writeText(commit.hash);
        return;
      case "checkout":
        if (
          await systemConfirm(
            formatI18n(tm("git.history.checkout.message_format", "Check out commit %@?"), commit.hash.slice(0, 7)),
            {
              title: tm("git.history.checkout_commit", "Check Out This Commit"),
              kind: "warning",
              okLabel: tm("git.history.checkout_commit", "Check Out"),
              cancelLabel: tm("common.cancel", "Cancel"),
            },
          )
        )
          await git.checkoutCommit(commit.hash);
        return;
      case "branch":
        createBranchFromCommit(commit);
        return;
      case "undo": {
        const pushed = await git.headCommitPushed();
        if (
          pushed &&
          !(await systemConfirm(
            tm("git.history.undo_last_commit.remote_notice", "The last commit may already be pushed. Continue?"),
            {
              title: tm("git.history.undo_last_commit", "Undo Last Commit"),
              kind: "warning",
              okLabel: tm("common.continue", "Continue"),
              cancelLabel: tm("common.cancel", "Cancel"),
            },
          ))
        )
          return;
        await git.undoLastCommit();
        return;
      }
      case "amend": {
        const current = await git.lastCommitMessage();
        openGitInput({
          title: tm("git.history.edit_last_commit_message", "Edit Last Commit Message"),
          label: tm("git.commit.message.placeholder", "Enter Commit Message"),
          value: current,
          multiline: true,
          onSubmit: async (message) => {
            const pushed = await git.headCommitPushed();
            if (
              pushed &&
              !(await systemConfirm(
                tm("git.commit.edit_last_message.remote_notice", "The last commit may already be pushed. Continue?"),
                {
                  title: tm("git.history.edit_last_commit_message", "Edit Last Commit Message"),
                  kind: "warning",
                  okLabel: tm("common.continue", "Continue"),
                  cancelLabel: tm("common.cancel", "Cancel"),
                },
              ))
            )
              return;
            await git.amendLastCommitMessage(message);
          },
        });
        return;
      }
      case "revert":
        if (
          await systemConfirm(
            formatI18n(tm("git.history.revert.message_format", "Revert commit %@?"), commit.hash.slice(0, 7)),
            {
              title: tm("git.history.revert_commit", "Revert This Commit"),
              kind: "warning",
              okLabel: tm("git.history.revert_commit", "Revert"),
              cancelLabel: tm("common.cancel", "Cancel"),
            },
          )
        )
          await git.revertCommit(commit.hash);
        return;
      case "restoreLocal":
        if (
          await systemConfirm(
            formatI18n(
              tm("git.history.restore_local.message_format", "Reset the current branch locally to %@?"),
              commit.hash.slice(0, 7),
            ),
            {
              title: tm("git.history.restore_local", "Restore Locally"),
              kind: "warning",
              okLabel: tm("git.history.restore_local.action", "Restore Locally"),
              cancelLabel: tm("common.cancel", "Cancel"),
            },
          )
        )
          await git.restoreCommit(commit.hash, false);
        return;
      case "restoreRemote":
        if (
          await systemConfirm(
            formatI18n(
              tm("git.history.restore_remote.message_format", "Reset the current branch and remote to %@?"),
              commit.hash.slice(0, 7),
            ),
            {
              title: tm("git.history.restore_remote", "Restore Remote"),
              kind: "warning",
              okLabel: tm("git.history.restore_remote.action", "Restore Remote"),
              cancelLabel: tm("common.cancel", "Cancel"),
            },
          )
        )
          await git.restoreCommit(commit.hash, true);
        return;
    }
  };
  const beginGitHistoryResize = (event: ReactPointerEvent<HTMLDivElement>) => {
    event.preventDefault();
    const startY = event.clientY;
    const startHeight = gitHistoryHeight;
    const contentHeight = gitContentRef.current?.clientHeight ?? 640;
    const maxHeight = Math.max(180, Math.round(contentHeight * 0.6));
    const handlePointerMove = (moveEvent: PointerEvent) => {
      const nextHeight = startHeight - (moveEvent.clientY - startY);
      setGitHistoryHeight(Math.max(132, Math.min(maxHeight, Math.round(nextHeight))));
    };
    const handlePointerUp = () => {
      window.removeEventListener("pointermove", handlePointerMove);
      window.removeEventListener("pointerup", handlePointerUp);
      window.removeEventListener("pointercancel", handlePointerUp);
    };
    window.addEventListener("pointermove", handlePointerMove);
    window.addEventListener("pointerup", handlePointerUp);
    window.addEventListener("pointercancel", handlePointerUp);
  };

  return (
    <>
      <PanelHeader
        title={
          <DesktopMenu
            ariaLabel={tm("git.branch.actions", "Git Branch Actions")}
            isOpen={branchMenuOpen}
            onOpenChange={setBranchMenuOpen}
            placement="bottom-start"
            trigger={
              <button
                type="button"
                className="inline-flex items-center gap-1.5 text-sm font-semibold hover:text-ink/90"
              >
                <span className="truncate">{snapshot.branch || project?.branch || "master"}</span>
                <ChevronDown size={12} className="flex-shrink-0 text-ink-mute" />
              </button>
            }
          >
            <DesktopMenuItem
              label={tm("git.branch.create_and_switch", "New Branch")}
              onSelect={() => void runBranchAction("create")}
            >
              {tm("git.branch.create_and_switch", "New Branch")}
            </DesktopMenuItem>
            <DesktopMenuSeparator />
            <DesktopSubmenu label={tm("git.branch.local", "Local Branches")}>
              {snapshot.branches.length === 0 ? (
                <DesktopMenuItem label={tm("git.branch.local.empty", "No Local Branches")} disabled>
                  {tm("git.branch.local.empty", "No Local Branches")}
                </DesktopMenuItem>
              ) : (
                snapshot.branches.map((branch) => (
                  <DesktopSubmenu key={`localBranch:${branch.name}`} label={branch.name}>
                    <DesktopMenuItem
                      label={tm("git.branch.switch", "Switch Branch")}
                      disabled={branch.isCurrent}
                      onSelect={() => void runBranchAction(`checkoutLocal:${branch.name}`)}
                    >
                      {branch.isCurrent
                        ? tm("git.branch.current_label", "Current Branch")
                        : tm("git.branch.switch", "Switch Branch")}
                    </DesktopMenuItem>
                    {branch.upstream ? (
                      <DesktopMenuItem label={branch.upstream} disabled>
                        {branch.upstream}
                      </DesktopMenuItem>
                    ) : null}
                    <DesktopMenuSeparator />
                    <DesktopMenuItem
                      label={tm("git.branch.merge_current", "Merge into Current Branch")}
                      disabled={branch.isCurrent}
                      onSelect={() => void runBranchAction(`mergeLocal:${branch.name}`)}
                    >
                      {tm("git.branch.merge_current", "Merge into Current Branch")}
                    </DesktopMenuItem>
                    <DesktopMenuItem
                      label={tm("git.branch.squash_merge", "Squash Merge Branch")}
                      disabled={branch.isCurrent}
                      onSelect={() => void runBranchAction(`squashLocal:${branch.name}`)}
                    >
                      {tm("git.branch.squash_merge", "Squash Merge Branch")}
                    </DesktopMenuItem>
                    <DesktopMenuSeparator />
                    <DesktopMenuItem
                      label={tm("git.branch.delete_local", "Delete Local Branch")}
                      disabled={branch.isCurrent}
                      onSelect={() => void runBranchAction(`deleteLocal:${branch.name}`)}
                    >
                      {tm("git.branch.delete_local", "Delete Local Branch")}
                    </DesktopMenuItem>
                  </DesktopSubmenu>
                ))
              )}
            </DesktopSubmenu>
            <DesktopSubmenu label={tm("git.branch.merge_current", "Merge into Current Branch")}>
              {localMergeCandidates.length === 0 ? (
                <DesktopMenuItem label={tm("git.branch.merge.empty", "No Branches Available to Merge")} disabled>
                  {tm("git.branch.merge.empty", "No Branches Available to Merge")}
                </DesktopMenuItem>
              ) : (
                localMergeCandidates.map((branch) => (
                  <DesktopMenuItem
                    key={`mergeLocal:${branch.name}`}
                    label={branch.name}
                    onSelect={() => void runBranchAction(`mergeLocal:${branch.name}`)}
                  >
                    {branch.name}
                  </DesktopMenuItem>
                ))
              )}
            </DesktopSubmenu>
            <DesktopSubmenu label={tm("git.remote.remotes", "Remotes")}>
              <DesktopMenuItem
                label={tm("git.remote.add", "Add Remote")}
                onSelect={() => void runBranchAction("addRemote")}
              >
                {tm("git.remote.add", "Add Remote")}
              </DesktopMenuItem>
              <DesktopMenuSeparator />
              {snapshot.remotes.length === 0 ? (
                <DesktopMenuItem label={tm("git.remote.empty", "No Remotes")} disabled>
                  {tm("git.remote.empty", "No Remotes")}
                </DesktopMenuItem>
              ) : (
                snapshot.remotes.map((remote) => (
                  <DesktopSubmenu key={`remote:${remote.name}`} label={remote.name}>
                    <DesktopMenuItem
                      label={tm("git.remote.set_default", "Set as Default")}
                      onSelect={() => void runBranchAction(`setDefaultRemote:${remote.name}`)}
                    >
                      <span className="inline-flex min-w-0 items-center gap-2">
                        <span className="w-3 text-center">
                          {project?.gitDefaultPushRemoteName === remote.name ? "✓" : ""}
                        </span>
                        <span className="truncate">{tm("git.remote.set_default", "Set as Default")}</span>
                      </span>
                    </DesktopMenuItem>
                    <DesktopMenuSeparator />
                    <DesktopMenuItem
                      label={tm("git.remote.copy_url", "Copy URL")}
                      onSelect={() => void navigator.clipboard?.writeText(remote.url)}
                    >
                      {tm("git.remote.copy_url", "Copy URL")}
                    </DesktopMenuItem>
                    <DesktopMenuItem
                      label={tm("git.remote.remove", "Remove Remote")}
                      onSelect={() => void git.removeRemote(remote.name)}
                    >
                      {tm("git.remote.remove", "Remove Remote")}
                    </DesktopMenuItem>
                  </DesktopSubmenu>
                ))
              )}
            </DesktopSubmenu>
            <DesktopSubmenu label={tm("git.remote.branches", "Remote Branches")}>
              <DesktopMenuItem
                label={tm("git.remote.branches.refresh", "Refresh Remote Branches")}
                disabled={!hasRemotes}
                onSelect={() => void runBranchAction("fetch")}
              >
                {tm("git.remote.branches.refresh", "Refresh Remote Branches")}
              </DesktopMenuItem>
              <DesktopMenuSeparator />
              {remoteBranchesByRemote.length === 0 ? (
                <DesktopMenuItem label={tm("git.remote.branches.empty", "No Remote Branches")} disabled>
                  {tm("git.remote.branches.empty", "No Remote Branches")}
                </DesktopMenuItem>
              ) : (
                remoteBranchesByRemote.map(({ remote, branches }) => (
                  <DesktopSubmenu key={`remoteBranches:${remote}`} label={remote}>
                    {branches.map((branch) => (
                      <DesktopSubmenu key={`remoteBranch:${remote}/${branch.name}`} label={branch.name}>
                        <DesktopMenuItem
                          label={tm("git.remote.branch.checkout_local", "Checkout as Local Branch")}
                          onSelect={() => void runBranchAction(`checkoutRemote:${remote}/${branch.name}`)}
                        >
                          {tm("git.remote.branch.checkout_local", "Checkout as Local Branch")}
                        </DesktopMenuItem>
                        <DesktopMenuItem
                          label={tm("git.remote.branch.push_here", "Push to This Branch")}
                          onSelect={() => void runBranchAction(`pushRemoteBranch:${remote}/${branch.name}`)}
                        >
                          {tm("git.remote.branch.push_here", "Push to This Branch")}
                        </DesktopMenuItem>
                      </DesktopSubmenu>
                    ))}
                  </DesktopSubmenu>
                ))
              )}
            </DesktopSubmenu>
            <DesktopMenuItem
              label={tm("git.remote.fetch", "Fetch")}
              disabled={!hasRemotes}
              onSelect={() => void runBranchAction("fetch")}
            >
              {tm("git.remote.fetch", "Fetch")}
            </DesktopMenuItem>
            <DesktopMenuItem
              label={tm("git.remote.pull", "Pull")}
              disabled={!canUseCurrentBranchRemote}
              onSelect={() => void runBranchAction("pull")}
            >
              {tm("git.remote.pull", "Pull")}
            </DesktopMenuItem>
            <DesktopMenuItem
              label={tm("git.remote.push", "Push")}
              disabled={!canUseCurrentBranchRemote}
              onSelect={() => void runBranchAction("push")}
            >
              {tm("git.remote.push", "Push")}
            </DesktopMenuItem>
            <DesktopSubmenu label={tm("git.remote.push_to", "Push To...")}>
              {snapshot.remotes.length === 0 ? (
                <DesktopMenuItem label={tm("git.remote.empty", "No Remotes")} disabled>
                  {tm("git.remote.empty", "No Remotes")}
                </DesktopMenuItem>
              ) : (
                snapshot.remotes.map((remote) => (
                  <DesktopMenuItem
                    key={`pushToRemote:${remote.name}`}
                    label={remote.name}
                    onSelect={() => void runBranchAction(`pushRemote:${remote.name}`)}
                  >
                    <span className="grid min-w-0 grid-cols-[12px_minmax(0,1fr)] gap-x-2">
                      <span className="row-span-2 text-center">
                        {project?.gitDefaultPushRemoteName === remote.name ? "✓" : ""}
                      </span>
                      <span className="truncate">{remote.name}</span>
                      <span className="col-start-2 truncate text-[11px] font-normal text-ink-faint">{remote.url}</span>
                    </span>
                  </DesktopMenuItem>
                ))
              )}
            </DesktopSubmenu>
            <DesktopMenuItem
              label={tm("git.remote.force_push", "Force Push")}
              disabled={!canUseCurrentBranchRemote}
              onSelect={() => void runBranchAction("forcePush")}
            >
              {tm("git.remote.force_push", "Force Push")}
            </DesktopMenuItem>
            <DesktopMenuSeparator />
            <DesktopMenuItem
              label={tm("git.history.undo_last_commit", "Undo Last Commit")}
              disabled={!snapshot.commits[0]}
              onSelect={() => void runBranchAction("undoLastCommit")}
            >
              {tm("git.history.undo_last_commit", "Undo Last Commit")}
            </DesktopMenuItem>
            <DesktopMenuItem
              label={tm("git.history.edit_last_commit_message", "Edit Last Commit Message")}
              disabled={!snapshot.commits[0]}
              onSelect={() => void runBranchAction("editLastCommitMessage")}
            >
              {tm("git.history.edit_last_commit_message", "Edit Last Commit Message")}
            </DesktopMenuItem>
            <DesktopMenuSeparator />
            <DesktopMenuItem
              label={tm("git.repository.show_in_finder", "Show Repository in Finder")}
              disabled={!project?.path}
              onSelect={() => void runBranchAction("showRepository")}
            >
              {tm("git.repository.show_in_finder", "Show Repository in Finder")}
            </DesktopMenuItem>
          </DesktopMenu>
        }
        trailing={
          <>
            <PanelIconButton
              icon={Sparkles}
              tooltip={tm("git.commit.generate_message", "Generate Commit Message")}
              busy={isGeneratingCommitMessage}
              disabled={isGeneratingCommitMessage}
              onClick={() => {
                setGeneratingCommitMessage(true);
                void generateCommitMessage(snapshot)
                  .then((message) => {
                    if (message) setCommitMessage(message);
                  })
                  .catch((error) => {
                    console.error("failed to generate commit message", error);
                    const fallback = fallbackCommitMessage(snapshot);
                    if (fallback) setCommitMessage(fallback);
                  })
                  .finally(() => setGeneratingCommitMessage(false));
              }}
            />
            <PanelIconButton
              icon={RefreshCw}
              tooltip={
                isRefreshingGit
                  ? tm("git.empty.reading_status", "Reading Git Status")
                  : tm("git.status.refresh", "Refresh Git Status")
              }
              busy={isRefreshingGit}
              disabled={isRefreshingGit}
              onClick={() => void refreshGit()}
            />
          </>
        }
      />
      {gitInput && (
        <GitInputPanel
          input={gitInput}
          onChange={setGitInput}
          onCancel={closeGitInput}
          onSubmit={() => void submitGitInput()}
        />
      )}

      {!snapshot.isRepository ? (
        <PanelEmptyState
          icon={Folder}
          title={tm("git.empty.no_repository", "No Repository")}
          description={tm(
            "git.empty.description",
            "Initialize a repository or clone a remote repository to view commits, diffs, and branches here.",
          )}
          tone="warning"
          action={
            <div className="flex items-center gap-2">
              <HeroButton size="sm" variant="primary" onPress={() => void git.init()}>
                {tm("git.empty.initialize_repository", "Initialize Repository")}
              </HeroButton>
              <HeroButton
                size="sm"
                variant="secondary"
                onPress={() =>
                  openGitInput({
                    title: tm("git.empty.clone_remote_repository", "Clone Remote Repository"),
                    label: tm("git.remote.add.url_message", "Remote URL"),
                    value: "",
                    onSubmit: async (remoteUrl) => {
                      await git.cloneRepository(remoteUrl);
                    },
                  })
                }
              >
                {tm("git.empty.clone_remote_repository", "Clone Remote Repository")}
              </HeroButton>
            </div>
          }
        />
      ) : (
        <div ref={gitContentRef} className="min-h-0 flex-1 flex flex-col">
          <div className="flex-shrink-0 border-b border-line/80 p-3">
            <Textarea
              placeholder={tm("git.commit.message.placeholder", "Commit message")}
              value={commitMessage}
              onChange={(event) => setCommitMessage(event.target.value)}
              fullWidth
              variant="secondary"
              className="h-[78px] resize-none text-sm"
            />
            <div className="mt-2.5 flex rounded-lg shadow-sm">
              <Button
                variant="primary"
                size="sm"
                block
                disabled={!canCommit}
                className="h-[34px] rounded-l-lg rounded-r-none border-r border-white/15 text-sm font-semibold"
                onPress={() => void submitCommit()}
              >
                {isSubmittingCommit ? tm("git.commit.submitting", "Committing") : commitActionLabel}
              </Button>
              <Dropdown isOpen={commitMenuOpen} onOpenChange={setCommitMenuOpen}>
                <Dropdown.Trigger
                  isDisabled={!canCommit}
                  className="grid h-[34px] w-8 min-w-8 place-items-center rounded-l-none rounded-r-lg bg-brand-blue px-0 text-on-brand transition-colors hover:bg-brand-blue/90 disabled:cursor-default disabled:opacity-50"
                  aria-label={tm("git.commit.options", "Commit Options")}
                >
                  <ChevronDown size={13} strokeWidth={2.4} />
                </Dropdown.Trigger>
                <Dropdown.Popover
                  placement="bottom end"
                  className="min-w-[184px] rounded-[10px] border border-line-strong bg-surface-popover p-1 shadow-pop"
                >
                  <Dropdown.Menu
                    aria-label={tm("git.commit.options", "Commit Options")}
                    onAction={(key) => setCommitAction(String(key) as GitCommitAction)}
                    className="grid gap-0.5"
                  >
                    <Dropdown.Item id="commit" className="menu-item">
                      {tm("git.commit.action", "Commit")}
                    </Dropdown.Item>
                    <Dropdown.Item id="commitAndPush" className="menu-item" isDisabled={!canUseCurrentBranchRemote}>
                      {tm("git.commit.action_push", "Commit and Push")}
                    </Dropdown.Item>
                    <Dropdown.Item id="commitAndSync" className="menu-item" isDisabled={!canUseCurrentBranchRemote}>
                      {tm("git.commit.action_sync", "Commit and Sync")}
                    </Dropdown.Item>
                  </Dropdown.Menu>
                </Dropdown.Popover>
              </Dropdown>
            </div>
          </div>

          <div className="min-h-0 flex-1 overflow-y-auto scrollbar-overlay">
            <SectionHeader
              open={stagedOpen}
              setOpen={setStagedOpen}
              title={tm("git.files.staged", "Staged")}
              count={snapshot.staged.length}
              actions={
                <HeaderActionButton
                  icon={Minus}
                  label={
                    selectedByKind.staged.length > 0
                      ? tm("git.files.unstage_selected", "Unstage Selected")
                      : tm("git.files.unstage_all", "Unstage All")
                  }
                  disabled={snapshot.staged.length === 0}
                  onPress={() => {
                    const targets = selectedByKind.staged.length > 0 ? selectedByKind.staged : snapshot.staged;
                    void git.unstage(targets.map((file) => file.path));
                  }}
                />
              }
            />
            {stagedOpen && (
              <GitFileSection
                files={snapshot.staged}
                emptyLabel={tm("git.files.staged.empty", "No staged changes")}
                kind="staged"
                expandedPaths={expandedGitFilePaths.staged}
                rootPath={project?.path}
                selectedIds={selectedFileIds}
                primaryLabel={tm("git.files.unstage", "Unstage")}
                onPrimary={(files) => void git.unstage(files.map((file) => file.path))}
                onSelect={(file, modifiers) => selectGitFile(file, true, modifiers)}
                onOpenDiff={(file) => void previewDiff(file, true)}
                onToggleDirectory={(path) => toggleGitDirectory("staged", path)}
                onDiscard={(files) => {
                  void systemConfirm(
                    files.length > 1
                      ? formatI18n(
                          tm("git.files.discard_selected.confirm_format", "Discard %@ selected changes?"),
                          String(files.length),
                        )
                      : formatI18n(tm("git.files.discard.confirm_format", "Discard changes in %@?"), files[0].path),
                    {
                      title: tm("git.files.discard_changes", "Discard Changes"),
                      kind: "warning",
                      okLabel: tm("git.files.discard_changes", "Discard"),
                      cancelLabel: tm("common.cancel", "Cancel"),
                    },
                  ).then((confirmed) => {
                    if (confirmed) void git.discard(files.map((file) => file.path));
                  });
                }}
              />
            )}

            <SectionHeader
              open={changesOpen}
              setOpen={setChangesOpen}
              title={tm("git.files.changes", "Changes")}
              count={snapshot.unstaged.length}
              actions={
                <>
                  <HeaderActionButton
                    icon={Plus}
                    label={
                      selectedByKind.unstaged.length > 0
                        ? tm("git.files.stage_selected", "Stage Selected")
                        : tm("git.files.stage_all", "Stage All")
                    }
                    disabled={snapshot.unstaged.length === 0}
                    onPress={() => {
                      const targets = selectedByKind.unstaged.length > 0 ? selectedByKind.unstaged : snapshot.unstaged;
                      void git.stage(targets.map((file) => file.path));
                    }}
                  />
                  <HeaderActionButton
                    icon={Undo2}
                    label={
                      selectedByKind.unstaged.length > 0
                        ? tm("git.files.discard_selected", "Discard Selected")
                        : tm("git.files.discard_all", "Discard All")
                    }
                    disabled={snapshot.unstaged.length === 0}
                    onPress={() => {
                      const targets = selectedByKind.unstaged.length > 0 ? selectedByKind.unstaged : snapshot.unstaged;
                      void systemConfirm(tm("git.files.discard_all.confirm", "Discard all worktree changes?"), {
                        title: tm("git.files.discard_all", "Discard All"),
                        kind: "warning",
                        okLabel: tm("git.files.discard_all", "Discard All"),
                        cancelLabel: tm("common.cancel", "Cancel"),
                      }).then((confirmed) => {
                        if (confirmed) void git.discard(targets.map((file) => file.path));
                      });
                    }}
                  />
                </>
              }
            />
            {changesOpen && (
              <GitFileSection
                files={snapshot.unstaged}
                emptyLabel={tm("git.files.changes.empty", "No worktree changes")}
                kind="unstaged"
                expandedPaths={expandedGitFilePaths.unstaged}
                rootPath={project?.path}
                selectedIds={selectedFileIds}
                primaryLabel={tm("git.files.stage", "Stage")}
                onPrimary={(files) => void git.stage(files.map((file) => file.path))}
                onSelect={(file, modifiers) => selectGitFile(file, false, modifiers)}
                onOpenDiff={(file) => void previewDiff(file, false)}
                onToggleDirectory={(path) => toggleGitDirectory("unstaged", path)}
                onDiscard={(files) => {
                  void systemConfirm(
                    files.length > 1
                      ? formatI18n(
                          tm("git.files.discard_selected.confirm_format", "Discard %@ selected changes?"),
                          String(files.length),
                        )
                      : formatI18n(tm("git.files.discard.confirm_format", "Discard changes in %@?"), files[0].path),
                    {
                      title: tm("git.files.discard_changes", "Discard Changes"),
                      kind: "warning",
                      okLabel: tm("git.files.discard_changes", "Discard"),
                      cancelLabel: tm("common.cancel", "Cancel"),
                    },
                  ).then((confirmed) => {
                    if (confirmed) void git.discard(files.map((file) => file.path));
                  });
                }}
              />
            )}

            <SectionHeader
              open={untrackedOpen}
              setOpen={setUntrackedOpen}
              title={tm("git.files.untracked", "Untracked")}
              count={snapshot.untracked.length}
              actions={
                <>
                  <HeaderActionButton
                    icon={Plus}
                    label={
                      selectedByKind.untracked.length > 0
                        ? tm("git.files.stage_selected", "Stage Selected")
                        : tm("git.files.stage_all", "Stage All")
                    }
                    disabled={snapshot.untracked.length === 0}
                    onPress={() => {
                      const targets =
                        selectedByKind.untracked.length > 0 ? selectedByKind.untracked : snapshot.untracked;
                      void git.stage(targets.map((file) => file.path));
                    }}
                  />
                  <HeaderActionButton
                    icon={X}
                    label={tm("git.ignore.add_all", "Add All to .gitignore")}
                    disabled={snapshot.untracked.length === 0}
                    onPress={() => void git.appendGitignore(snapshot.untracked.map((file) => file.path))}
                  />
                </>
              }
            />
            {untrackedOpen && (
              <GitFileSection
                files={snapshot.untracked}
                emptyLabel={tm("git.files.untracked.empty", "No untracked files")}
                kind="untracked"
                expandedPaths={expandedGitFilePaths.untracked}
                rootPath={project?.path}
                selectedIds={selectedFileIds}
                primaryLabel={tm("git.files.stage", "Stage")}
                onPrimary={(files) => void git.stage(files.map((file) => file.path))}
                onSelect={(file, modifiers) => selectGitFile(file, false, modifiers)}
                onOpenDiff={(file) => void previewDiff(file, false)}
                onToggleDirectory={(path) => toggleGitDirectory("untracked", path)}
                onIgnore={(files) => void git.appendGitignore(files.map((file) => file.path))}
                onDiscard={(files) => {
                  void systemConfirm(
                    files.length > 1
                      ? formatI18n(
                          tm(
                            "git.files.delete_untracked_selected.confirm_format",
                            "Delete %@ selected untracked files?",
                          ),
                          String(files.length),
                        )
                      : formatI18n(
                          tm("git.files.delete_untracked.confirm_format", "Delete untracked file %@?"),
                          files[0].path,
                        ),
                    {
                      title: tm("git.files.delete_file", "Delete File"),
                      kind: "warning",
                      okLabel: tm("common.delete", "Delete"),
                      cancelLabel: tm("common.cancel", "Cancel"),
                    },
                  ).then((confirmed) => {
                    if (confirmed) void git.discard(files.map((file) => file.path));
                  });
                }}
              />
            )}

            {git.error && (
              <div className="mx-3 mt-2 rounded-md border border-brand-red/30 bg-brand-red/10 px-2.5 py-2 text-xs text-brand-red">
                {git.error}
              </div>
            )}
          </div>

          <div className="relative flex-shrink-0 bg-surface-chrome/25" style={{ height: gitHistoryHeight }}>
            <div
              className="peer/git-history-resize absolute inset-x-0 top-[-5px] z-20 h-3 cursor-row-resize"
              onPointerDown={beginGitHistoryResize}
              aria-label={tm("common.resize", "Resize")}
            />
            <div className="pointer-events-none absolute inset-x-0 top-0 z-10 h-px bg-line-strong/85 transition-colors peer-hover/git-history-resize:bg-brand-blue/70" />
            <PanelSection title={tm("git.history.title", "Git History")} className="h-full flex flex-col">
              <div className="min-h-0 flex-1 overflow-y-auto scrollbar-overlay pb-3">
                {snapshot.commits.length > 0 ? (
                  snapshot.commits.map((commit) => (
                    <CommitRow
                      key={commit.hash}
                      commit={commit}
                      isHead={snapshot.commits[0]?.hash === commit.hash}
                      selected={selectedCommitHash === commit.hash}
                      onSelect={() => setSelectedCommitHash(commit.hash)}
                      onAction={(key) => void runCommitAction(commit, key)}
                    />
                  ))
                ) : (
                  <div className="px-3.5 py-3 text-xs text-ink-faint">
                    {git.isLoading
                      ? tm("git.empty.reading_status", "Reading Git Status")
                      : tm("git.history.empty", "No Commit History")}
                  </div>
                )}
              </div>
            </PanelSection>
          </div>
        </div>
      )}

      <PanelStatusBar
        tone={statusTone}
        leading={
          <span className="flex min-w-0 items-center gap-1.5 truncate">
            {isRefreshingGit ? (
              <Spinner size="sm" color="current" className="text-current/90" />
            ) : snapshot.isRepository && hasUpstream ? (
              <StatusIcon size={12} className="opacity-90" />
            ) : (
              <GitBranch size={12} className="opacity-75" />
            )}
            <span>{isRefreshingGit ? tm("git.empty.reading_status", "Reading Git Status") : statusLabel}</span>
            {isRefreshingGit && (
              <ProgressBar
                aria-label={tm("git.status.refresh.progress", "Git refresh progress")}
                isIndeterminate
                size="sm"
                className="w-14"
              >
                <ProgressBar.Track className="h-1 bg-current/20">
                  <ProgressBar.Fill className="h-full bg-current/75" />
                </ProgressBar.Track>
              </ProgressBar>
            )}
          </span>
        }
        trailing={
          <>
            <PanelButton
              tone={statusButtonTone}
              leading={ArrowDownToLine}
              onClick={hasUpstream ? () => void git.pull() : undefined}
            >
              {tm("git.remote.pull", "Pull")}
              {snapshot.behind > 0 ? ` ${snapshot.behind}` : ""}
            </PanelButton>
            <PanelButton
              tone={statusButtonTone}
              leading={ArrowUpFromLine}
              onClick={hasUpstream ? () => void git.push() : () => void pushRemote()}
            >
              {tm("git.remote.push", "Push")}
              {snapshot.ahead > 0 ? ` ${snapshot.ahead}` : ""}
            </PanelButton>
          </>
        }
      />
    </>
  );
}

function FileRow({
  path,
  displayName,
  tag,
  tone,
  selected,
  selectionCount = 1,
  depth = 0,
  onSelect,
  onContextSelect,
  onOpenDiff,
  onPrimary,
  primaryLabel,
  rootPath,
  onDiscard,
  onIgnore,
}: {
  path: string;
  displayName?: string;
  tag: string;
  tone: "amber" | "green" | "blue";
  depth?: number;
  rootPath?: string;
  selected?: boolean;
  selectionCount?: number;
  onSelect?: (modifiers?: { extend?: boolean; toggle?: boolean }) => void;
  onContextSelect?: () => void;
  onOpenDiff?: () => void;
  onPrimary?: () => void;
  primaryLabel?: string;
  onDiscard?: () => void;
  onIgnore?: () => void;
}) {
  const contextMenu = useContextMenu();
  const toneClass = tone === "amber" ? "text-brand-amber" : tone === "green" ? "text-brand-green" : "text-brand-blue";
  return (
    <Tooltip label={path} placement="left" triggerClassName="block w-full">
      <div
        onContextMenu={(event) => {
          onContextSelect?.();
          contextMenu.openMenu(event);
        }}
        className={`group relative w-full h-[28px] pr-3 flex items-center gap-1.5 transition-colors text-xs text-ink-soft ${
          selected ? "bg-brand-blue/12 text-ink" : "hover:bg-fill/[0.04]"
        }`}
        style={{ paddingLeft: `${12 + depth * 14}px` }}
      >
        <PressableButton
          className="min-w-0 flex-1 h-full flex items-center gap-2 text-left"
          onPressUp={(event) =>
            onSelect?.({
              extend: event.shiftKey,
              toggle: event.metaKey || event.ctrlKey,
            })
          }
          onDoubleClick={onOpenDiff}
        >
          <span className="w-[13px] flex-shrink-0" />
          <FileText size={13} className="flex-shrink-0 text-ink-mute" />
          <span className="truncate flex-1 text-left">{displayName ?? path}</span>
          <span className={`flex-shrink-0 text-xs font-bold ${toneClass}`}>{tag}</span>
        </PressableButton>
        <ContextMenu
          ariaLabel={formatI18n(tm("git.files.actions_format", "%@ Actions"), path)}
          menu={contextMenu.menu}
          onClose={contextMenu.closeMenu}
        >
          <ContextMenuItem
            label={tm("git.files.copy_path", "Copy Path")}
            onSelect={() => void navigator.clipboard?.writeText(path)}
          >
            {tm("git.files.copy_path", "Copy Path")}
          </ContextMenuItem>
          {rootPath && (
            <ContextMenuItem
              label={tm("git.files.show_in_finder", "Show in Finder")}
              onSelect={() => void revealFile(rootPath, path)}
            >
              {tm("git.files.show_in_finder", "Show in Finder")}
            </ContextMenuItem>
          )}
          <ContextMenuSeparator />
          {selectionCount > 1 && (
            <div className="px-2 py-1 text-[11px] font-semibold text-ink-faint">
              {formatI18n(tm("git.files.selected_count_format", "%@ selected"), String(selectionCount))}
            </div>
          )}
          {onPrimary && primaryLabel && (
            <ContextMenuItem label={primaryLabel} onSelect={onPrimary}>
              {primaryLabel}
            </ContextMenuItem>
          )}
          <ContextMenuItem label={tm("git.diff.open", "Open Diff")} onSelect={onOpenDiff}>
            {tm("git.diff.open", "Open Diff")}
          </ContextMenuItem>
          {onIgnore && (
            <ContextMenuItem label={tm("git.ignore.add", "Add to .gitignore")} onSelect={onIgnore}>
              {tm("git.ignore.add", "Add to .gitignore")}
            </ContextMenuItem>
          )}
          {onDiscard && (
            <ContextMenuItem label={tm("git.files.discard_or_delete", "Discard / Delete")} onSelect={onDiscard}>
              {tm("git.files.discard_or_delete", "Discard / Delete")}
            </ContextMenuItem>
          )}
        </ContextMenu>
      </div>
    </Tooltip>
  );
}

function GitFileSection({
  files,
  emptyLabel,
  kind,
  expandedPaths,
  rootPath,
  selectedIds,
  primaryLabel,
  onSelect,
  onOpenDiff,
  onToggleDirectory,
  onPrimary,
  onDiscard,
  onIgnore,
}: {
  files: GitFileStatus[];
  emptyLabel: string;
  kind: GitFileSectionKind;
  expandedPaths: Set<string>;
  rootPath?: string;
  selectedIds: Set<string>;
  primaryLabel?: string;
  onSelect?: (file: GitFileStatus, modifiers?: { extend?: boolean; toggle?: boolean }) => void;
  onOpenDiff?: (file: GitFileStatus) => void;
  onToggleDirectory: (path: string) => void;
  onPrimary?: (files: GitFileStatus[]) => void;
  onDiscard?: (files: GitFileStatus[]) => void;
  onIgnore?: (files: GitFileStatus[]) => void;
}) {
  const tree = useMemo(() => buildGitFileTree(files), [files]);
  const selectedFiles = useMemo(
    () => files.filter((file) => selectedIds.has(gitFileNodeId(kind, file.path))),
    [files, kind, selectedIds],
  );
  if (files.length === 0) {
    return <div className="px-3.5 py-2.5 text-xs text-ink-faint">{emptyLabel}</div>;
  }
  return (
    <div className="pb-1">
      {tree.map((node) => (
        <GitFileTreeRow
          key={`${node.kind}:${node.path}`}
          node={node}
          depth={0}
          sectionKind={kind}
          expandedPaths={expandedPaths}
          rootPath={rootPath}
          selectedIds={selectedIds}
          selectedFiles={selectedFiles}
          primaryLabel={primaryLabel}
          onToggleDirectory={onToggleDirectory}
          onSelect={onSelect}
          onOpenDiff={onOpenDiff}
          onPrimary={onPrimary}
          onDiscard={onDiscard}
          onIgnore={onIgnore}
        />
      ))}
    </div>
  );
}

type GitFileSectionKind = "staged" | "unstaged" | "untracked";

type GitFileTreeNode = GitFileTreeDirectory | GitFileTreeFile;

type GitFileTreeDirectory = {
  kind: "directory";
  path: string;
  name: string;
  count: number;
  children: GitFileTreeNode[];
};

type GitFileTreeFile = {
  kind: "file";
  path: string;
  name: string;
  file: GitFileStatus;
};

type VisibleGitFile = {
  id: string;
  kind: GitFileSectionKind;
  file: GitFileStatus;
};

function gitFileNodeId(kind: GitFileSectionKind, path: string) {
  return `${kind}:${path}`;
}

function flattenGitFileTree(nodes: GitFileTreeNode[], kind: GitFileSectionKind): VisibleGitFile[] {
  const rows: VisibleGitFile[] = [];
  const visit = (node: GitFileTreeNode) => {
    if (node.kind === "file") {
      rows.push({ id: gitFileNodeId(kind, node.file.path), kind, file: node.file });
      return;
    }
    for (const child of node.children) visit(child);
  };
  for (const node of nodes) visit(node);
  return rows;
}

function GitFileTreeRow({
  node,
  depth,
  sectionKind,
  expandedPaths,
  rootPath,
  selectedIds,
  selectedFiles,
  primaryLabel,
  onToggleDirectory,
  onSelect,
  onOpenDiff,
  onPrimary,
  onDiscard,
  onIgnore,
}: {
  node: GitFileTreeNode;
  depth: number;
  sectionKind: GitFileSectionKind;
  expandedPaths: Set<string>;
  rootPath?: string;
  selectedIds: Set<string>;
  selectedFiles: GitFileStatus[];
  primaryLabel?: string;
  onToggleDirectory: (path: string) => void;
  onSelect?: (file: GitFileStatus, modifiers?: { extend?: boolean; toggle?: boolean }) => void;
  onOpenDiff?: (file: GitFileStatus) => void;
  onPrimary?: (files: GitFileStatus[]) => void;
  onDiscard?: (files: GitFileStatus[]) => void;
  onIgnore?: (files: GitFileStatus[]) => void;
}) {
  if (node.kind === "directory") {
    const expanded = expandedPaths.has(node.path);
    return (
      <>
        <GitDirectoryRow node={node} depth={depth} expanded={expanded} onToggle={() => onToggleDirectory(node.path)} />
        {expanded &&
          node.children.map((child) => (
            <GitFileTreeRow
              key={`${child.kind}:${child.path}`}
              node={child}
              depth={depth + 1}
              sectionKind={sectionKind}
              expandedPaths={expandedPaths}
              rootPath={rootPath}
              selectedIds={selectedIds}
              selectedFiles={selectedFiles}
              primaryLabel={primaryLabel}
              onToggleDirectory={onToggleDirectory}
              onSelect={onSelect}
              onOpenDiff={onOpenDiff}
              onPrimary={onPrimary}
              onDiscard={onDiscard}
              onIgnore={onIgnore}
            />
          ))}
      </>
    );
  }
  const meta = gitFileBadge(node.file, sectionKind);
  const id = gitFileNodeId(sectionKind, node.file.path);
  const rowSelected = selectedIds.has(id);
  const contextFiles = rowSelected && selectedFiles.length > 1 ? selectedFiles : [node.file];
  return (
    <FileRow
      path={node.file.path}
      displayName={node.name}
      tag={meta.tag}
      tone={meta.tone}
      depth={depth}
      rootPath={rootPath}
      selected={rowSelected}
      selectionCount={contextFiles.length}
      primaryLabel={primaryLabel}
      onSelect={(modifiers) => onSelect?.(node.file, modifiers)}
      onContextSelect={() => {
        if (!rowSelected) onSelect?.(node.file);
      }}
      onOpenDiff={() => onOpenDiff?.(node.file)}
      onPrimary={onPrimary ? () => onPrimary(contextFiles) : undefined}
      onDiscard={onDiscard ? () => onDiscard(contextFiles) : undefined}
      onIgnore={onIgnore ? () => onIgnore(contextFiles) : undefined}
    />
  );
}

function GitDirectoryRow({
  node,
  depth,
  expanded,
  onToggle,
}: {
  node: GitFileTreeDirectory;
  depth: number;
  expanded: boolean;
  onToggle: () => void;
}) {
  return (
    <Tooltip label={node.path} placement="left" triggerClassName="block w-full">
      <PressableButton
        className="w-full h-[28px] flex items-center gap-1.5 pr-3 text-left text-xs text-ink-soft transition-colors hover:bg-fill/[0.04] hover:text-ink"
        style={{ paddingLeft: `${12 + depth * 14}px` }}
        onPressUp={onToggle}
      >
        {expanded ? (
          <ChevronDown size={12} className="text-ink-faint" />
        ) : (
          <ChevronRight size={12} className="text-ink-faint" />
        )}
        <Folder size={13} className="text-brand-blue/85" />
        <span className="min-w-0 flex-1 truncate font-medium">{node.name}</span>
        <span className="text-[11px] text-ink-faint tabular-nums">{node.count}</span>
      </PressableButton>
    </Tooltip>
  );
}

function gitFileBadge(
  file: GitFileStatus,
  kind: GitFileSectionKind,
): { tag: string; tone: "amber" | "green" | "blue" } {
  if (kind === "untracked") return { tag: "U", tone: "green" };
  const raw = kind === "staged" ? file.indexStatus : file.worktreeStatus;
  const status = raw.trim();
  if (status === "A") return { tag: "A", tone: "green" };
  if (status === "D") return { tag: "D", tone: "blue" };
  if (status === "R") return { tag: "R", tone: "blue" };
  if (status === "C") return { tag: "C", tone: "blue" };
  return { tag: status || "M", tone: "amber" };
}

function buildGitFileTree(files: GitFileStatus[]): GitFileTreeNode[] {
  type MutableDirectory = {
    kind: "directory";
    path: string;
    name: string;
    count: number;
    children: Map<string, MutableDirectory | GitFileTreeFile>;
  };
  const root: MutableDirectory = {
    kind: "directory",
    path: "",
    name: "",
    count: 0,
    children: new Map(),
  };

  for (const file of files) {
    const parts = file.path.split("/").filter(Boolean);
    if (parts.length === 0) continue;
    let directory = root;
    directory.count += 1;
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
          count: 0,
          children: new Map(),
        };
        directory.children.set(path, nextDirectory);
      }
      nextDirectory.count += 1;
      directory = nextDirectory;
    }
    directory.children.set(file.path, {
      kind: "file",
      path: file.path,
      name: parts[parts.length - 1],
      file,
    });
  }

  const materialize = (directory: MutableDirectory): GitFileTreeNode[] =>
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
          count: node.count,
          children: materialize(node),
        };
      });

  return materialize(root);
}

function collectGitDirectoryPaths(files: GitFileStatus[]) {
  const paths = new Set<string>();
  for (const file of files) {
    const parts = file.path.split("/").filter(Boolean);
    for (let index = 0; index < parts.length - 1; index += 1) {
      paths.add(parts.slice(0, index + 1).join("/"));
    }
  }
  return paths;
}

function mergeGitDirectoryPaths(current: Set<string>, available: Set<string>, previousAvailable: Set<string>) {
  const next = new Set<string>();
  for (const path of current) {
    if (available.has(path)) next.add(path);
  }
  for (const path of available) {
    if (!previousAvailable.has(path)) next.add(path);
  }
  return next;
}

function formatDecorations(value?: string | null) {
  if (!value) return [];
  return value
    .replace(/\btag: /g, "")
    .split(",")
    .map((item) => item.trim())
    .filter(Boolean);
}

function compactGitDecorations(decorations: string[]) {
  return decorations
    .slice(0, 1)
    .map((decoration) => decoration.replace(/^HEAD -> /, "HEAD→").replace(/^origin\//, "o/"));
}

async function generateCommitMessage(snapshot: GitStatusSnapshot) {
  const fallback = fallbackCommitMessage(snapshot);
  if (!fallback || !window.__TAURI_INTERNALS__) return fallback;
  const ai = readAppSettings().ai;
  const prompt = buildCommitMessagePrompt(snapshot, ai);
  const response = await invoke<{ text: string }>("llm_complete", {
    request: {
      providerId: null,
      purpose: "gitCommitMessage",
      systemPrompt: [
        "You generate Git commit messages.",
        "Return only the final commit message text.",
        "Do not wrap it in quotes or markdown.",
        "Use one concise subject line unless the style rules explicitly ask for a body.",
      ].join("\n"),
      prompt,
    },
  });
  return sanitizeGeneratedCommitMessage(response.text) || fallback;
}

function fallbackCommitMessage(snapshot: GitStatusSnapshot) {
  const files = [...snapshot.staged, ...snapshot.unstaged, ...snapshot.untracked]
    .map((file) => file.path)
    .filter(Boolean);
  if (files.length === 0) return "";
  const summary = Array.from(new Set(files))
    .slice(0, 3)
    .map((path) => path.split("/").pop() || path)
    .join(", ");
  const suffix =
    files.length > 3
      ? formatI18n(tm("git.commit.generate.more_files_format", " and %@ more files"), String(files.length - 3))
      : "";
  return formatI18n(tm("git.commit.generate.simple_summary_format", "Update %@%@"), summary, suffix);
}

function buildCommitMessagePrompt(snapshot: GitStatusSnapshot, ai: AISettings) {
  const files = [...snapshot.staged, ...snapshot.unstaged, ...snapshot.untracked].slice(0, 80);
  const fileLines = files
    .map((file) => `- ${file.path} [index:${file.indexStatus.trim() || "-"} worktree:${file.worktreeStatus.trim() || "-"}]`)
    .join("\n");
  return [
    `Tone: ${ai.gitCommitMessageTone || "concise"}`,
    ai.gitCommitMessageStyleRules ? `Style rules:\n${ai.gitCommitMessageStyleRules}` : "",
    `Current branch: ${snapshot.branch}`,
    `Changed files:\n${fileLines}`,
  ]
    .filter(Boolean)
    .join("\n\n");
}

function sanitizeGeneratedCommitMessage(value: string) {
  return value
    .replace(/```[\s\S]*?```/g, (block) => block.replace(/```[a-zA-Z]*\n?|\n?```/g, ""))
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .join("\n")
    .slice(0, 1200)
    .trim();
}

function gitCommitActionLabel(action: GitCommitAction) {
  switch (action) {
    case "commitAndPush":
      return tm("git.commit.action_push", "Commit and Push");
    case "commitAndSync":
      return tm("git.commit.action_sync", "Commit and Sync");
    case "commit":
    default:
      return tm("git.commit.action", "Commit");
  }
}

function groupRemoteBranches(values: string[], upstream?: string | null) {
  const groups = new Map<string, Array<{ name: string; isUpstream: boolean }>>();
  for (const value of values) {
    const [remote, ...rest] = value.split("/");
    const branchName = rest.join("/");
    if (!remote || !branchName || branchName === "HEAD") continue;
    const branches = groups.get(remote) ?? [];
    if (!branches.some((branch) => branch.name === branchName)) {
      branches.push({ name: branchName, isUpstream: value === upstream });
    }
    groups.set(remote, branches);
  }
  return [...groups.entries()]
    .map(([remote, branches]) => ({
      remote,
      branches: branches.sort((left, right) => {
        if (left.isUpstream) return -1;
        if (right.isUpstream) return 1;
        return left.name.localeCompare(right.name);
      }),
    }))
    .sort((left, right) => left.remote.localeCompare(right.remote));
}

function CommitRow({
  commit,
  isHead,
  selected,
  onSelect,
  onAction,
}: {
  commit: GitCommitSummary;
  isHead?: boolean;
  selected?: boolean;
  onSelect?: () => void;
  onAction: (key: Key) => void;
}) {
  const allDecorations = formatDecorations(commit.decorations);
  const decorations = compactGitDecorations(allDecorations);
  const overflowDecorationCount = Math.max(0, allDecorations.length - decorations.length);
  const contextMenu = useContextMenu();
  return (
    <div
      className={`group relative min-h-[46px] py-1.5 pl-px pr-3 text-xs transition-colors ${
        selected ? "bg-brand-blue/12 text-ink" : "hover:bg-fill/[0.03]"
      }`}
      onPointerDown={(event) => {
        if (event.button === 0) onSelect?.();
      }}
      onContextMenu={(event) => {
        onSelect?.();
        contextMenu.openMenu(event);
      }}
    >
      {selected && <span className="absolute left-0 top-1.5 bottom-1.5 w-0.5 rounded-full bg-brand-blue" />}
      <div className="grid min-h-[34px] grid-cols-[14px_minmax(0,1fr)] items-center gap-1.5">
        <GitGraphPrefix prefix={commit.graphPrefix || (isHead ? "*" : "|")} />
        <Tooltip
          placement="top"
          triggerClassName="block min-w-0"
          contentClassName="max-w-[360px] px-2.5 py-2 text-left"
          label={
            <div className="grid gap-1">
              <div className="font-semibold leading-snug text-ink">{commit.title}</div>
              <div className="font-mono text-[10.5px] text-ink-faint">{commit.hash}</div>
              <div className="text-ink-mute">
                {commit.author} · {commit.relativeTime}
              </div>
              {allDecorations.length > 0 && <div className="text-brand-blue">{allDecorations.join(" · ")}</div>}
            </div>
          }
        >
          <div className="min-w-0">
            <div className="flex min-w-0 items-center gap-2 overflow-hidden">
              <span className="min-w-[9ch] flex-1 truncate text-[12.5px] font-medium leading-4 text-ink-soft">
                {commit.title}
              </span>
              <div className="min-w-0 flex-none max-w-[48%] overflow-hidden">
                <div className="flex min-w-0 items-center justify-end gap-1 overflow-hidden">
                  {decorations.map((decoration) => (
                    <span
                      key={decoration}
                      className="min-w-0 overflow-hidden text-ellipsis whitespace-nowrap px-1.5 h-[18px] inline-flex flex-shrink items-center text-xs font-semibold rounded-sm bg-brand-blue/18 text-brand-blue"
                    >
                      {decoration}
                    </span>
                  ))}
                  {overflowDecorationCount > 0 && (
                    <span className="h-[18px] inline-flex flex-shrink-0 items-center rounded-sm bg-fill/[0.07] px-1.5 text-xs font-semibold text-ink-faint">
                      +{overflowDecorationCount}
                    </span>
                  )}
                </div>
              </div>
            </div>
            <div className="mt-0.5 flex min-w-0 items-center gap-1.5 text-[11.5px] leading-4 text-ink-faint">
              <span className="min-w-0 truncate">{commit.author}</span>
              <span className="text-ink-faint/70">·</span>
              <span className="flex-shrink-0 whitespace-nowrap">{commit.relativeTime}</span>
            </div>
          </div>
        </Tooltip>
      </div>
      <ContextMenu
        ariaLabel={formatI18n(tm("git.history.commit_actions_format", "%@ Actions"), commit.hash.slice(0, 7))}
        menu={contextMenu.menu}
        onClose={contextMenu.closeMenu}
      >
        <ContextMenuItem
          label={tm("git.history.copy_commit_hash", "Copy Commit Hash")}
          onSelect={() => onAction("copy")}
        >
          {tm("git.history.copy_commit_hash", "Copy Commit Hash")}
        </ContextMenuItem>
        <ContextMenuItem
          label={tm("git.history.checkout_commit", "Check Out This Commit")}
          onSelect={() => onAction("checkout")}
        >
          {tm("git.history.checkout_commit", "Check Out This Commit")}
        </ContextMenuItem>
        <ContextMenuItem
          label={tm("git.history.create_branch_from_commit", "Create Branch from This Commit")}
          onSelect={() => onAction("branch")}
        >
          {tm("git.history.create_branch_from_commit", "Create Branch from This Commit")}
        </ContextMenuItem>
        {isHead && (
          <ContextMenuItem
            label={tm("git.history.undo_last_commit", "Undo Last Commit")}
            onSelect={() => onAction("undo")}
          >
            {tm("git.history.undo_last_commit", "Undo Last Commit")}
          </ContextMenuItem>
        )}
        {isHead && (
          <ContextMenuItem
            label={tm("git.history.edit_last_commit_message", "Edit Last Commit Message")}
            onSelect={() => onAction("amend")}
          >
            {tm("git.history.edit_last_commit_message", "Edit Last Commit Message")}
          </ContextMenuItem>
        )}
        <ContextMenuSeparator />
        <ContextMenuItem
          label={tm("git.history.revert_commit", "Revert This Commit")}
          onSelect={() => onAction("revert")}
        >
          {tm("git.history.revert_commit", "Revert This Commit")}
        </ContextMenuItem>
        <ContextMenuItem
          label={tm("git.history.restore_local", "Restore Locally")}
          onSelect={() => onAction("restoreLocal")}
        >
          {tm("git.history.restore_local", "Restore Locally")}
        </ContextMenuItem>
        <ContextMenuItem
          label={tm("git.history.restore_remote", "Restore Remote")}
          onSelect={() => onAction("restoreRemote")}
        >
          {tm("git.history.restore_remote", "Restore Remote")}
        </ContextMenuItem>
      </ContextMenu>
    </div>
  );
}

function GitGraphPrefix({ prefix }: { prefix: string }) {
  const chars = Array.from(prefix || "*");
  const columnWidth = 8;
  const width = 14;
  const startX = Math.max(0, width - chars.length * columnWidth);
  return (
    <div className="relative h-full min-h-[34px] w-[14px]" aria-hidden="true">
      {chars.map((char, index) => (
        <GitGraphToken
          key={`${char}:${index}`}
          char={char}
          index={index}
          centerX={startX + index * columnWidth + columnWidth / 2}
        />
      ))}
    </div>
  );
}

function GitGraphToken({ char, index, centerX }: { char: string; index: number; centerX: number }) {
  const tone = graphTone(index);
  const centerStyle: CSSProperties = { left: centerX };
  if (char === "|" || char === "*" || char === "o") {
    return (
      <>
        <span
          className={`absolute top-[-8px] bottom-[-8px] w-px ${char === "|" ? tone.line : tone.lineSoft}`}
          style={centerStyle}
        />
        {(char === "*" || char === "o") && (
          <span
            className={`absolute top-1/2 h-[7px] w-[7px] -translate-x-1/2 -translate-y-1/2 rounded-full ${tone.node}`}
            style={centerStyle}
          />
        )}
      </>
    );
  }
  if (char === "/" || char === "\\") {
    return (
      <span
        className={`absolute top-[-8px] h-[calc(100%+16px)] w-px origin-center ${char === "/" ? "rotate-[14deg]" : "-rotate-[14deg]"} ${tone.line}`}
        style={centerStyle}
      />
    );
  }
  return null;
}

function graphTone(index: number) {
  const tones = [
    { line: "bg-brand-blue/70", lineSoft: "bg-brand-blue/35", node: "bg-brand-blue" },
    { line: "bg-brand-green/70", lineSoft: "bg-brand-green/35", node: "bg-brand-green" },
    { line: "bg-brand-amber/70", lineSoft: "bg-brand-amber/35", node: "bg-brand-amber" },
    { line: "bg-brand-pink/70", lineSoft: "bg-brand-pink/35", node: "bg-brand-pink" },
    { line: "bg-brand-red/70", lineSoft: "bg-brand-red/35", node: "bg-brand-red" },
  ];
  return tones[index % tones.length];
}

function FilesPanel({ project }: { project?: WorkspaceProject }) {
  const rootPath = project?.path ?? "";
  const [childrenByPath, setChildrenByPath] = useState<Record<string, FileEntry[]>>({});
  const [expandedPaths, setExpandedPaths] = useState<Set<string>>(new Set());
  const [selectedPath, setSelectedPath] = useState("");
  const [selectedPaths, setSelectedPaths] = useState<Set<string>>(new Set());
  const [selectionAnchorPath, setSelectionAnchorPath] = useState("");
  const [pendingDeletePaths, setPendingDeletePaths] = useState<string[]>([]);
  const [copiedPath, setCopiedPath] = useState("");
  const [isDraggingExternalFiles, setDraggingExternalFiles] = useState(false);
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [inlineEdit, setInlineEdit] = useState<FileInlineEdit | null>(null);
  const expandedPathsRef = useRef(expandedPaths);
  const fileTreeRef = useRef<HTMLDivElement | null>(null);
  const filePanelActiveRef = useRef(false);
  const rowsRef = useRef<FileRowModel[]>([]);
  const selectedEntryRef = useRef<FileEntry | undefined>(undefined);
  const selectedEntriesRef = useRef<FileEntry[]>([]);
  const selectedPathRef = useRef("");
  const inlineEditRef = useRef<FileInlineEdit | null>(null);
  const fileTreeStateRef = useRef(new Map<string, { expandedPaths: Set<string>; selectedPath: string }>());

  const updateStatus = useCallback(
    (message: string, tone: "neutral" | "success" | "warning" | "danger" = "neutral") => {
      void message;
      void tone;
    },
    [],
  );

  const handleFileError = useCallback(
    (nextError: unknown) => {
      const message = nextError instanceof Error ? nextError.message : String(nextError);
      updateStatus(message, "danger");
    },
    [updateStatus],
  );

  useEffect(() => {
    if (!rootPath) return;
    const stored = fileTreeStateRef.current.get(rootPath);
    if (stored) {
      setExpandedPaths(new Set(stored.expandedPaths));
      setSelectedPath(stored.selectedPath);
      setSelectedPaths(stored.selectedPath ? new Set([stored.selectedPath]) : new Set());
      setSelectionAnchorPath(stored.selectedPath);
    }
  }, [rootPath]);

  useEffect(() => {
    if (!rootPath) return;
    fileTreeStateRef.current.set(rootPath, {
      expandedPaths: new Set(expandedPaths),
      selectedPath,
    });
  }, [expandedPaths, rootPath, selectedPath]);

  useEffect(() => {
    expandedPathsRef.current = expandedPaths;
  }, [expandedPaths]);

  const loadChildren = useCallback(
    async (directoryPath?: string) => {
      if (!rootPath) return false;
      const key = directoryPath || rootPath;
      setLoadingPaths((current) => new Set(current).add(key));
      try {
        const children = await listFileChildren(rootPath, directoryPath);
        setChildrenByPath((current) => ({
          ...current,
          [key]: children,
        }));
        return true;
      } catch (nextError) {
        handleFileError(nextError);
        return false;
      } finally {
        setLoadingPaths((current) => {
          const next = new Set(current);
          next.delete(key);
          return next;
        });
      }
    },
    [handleFileError, rootPath],
  );

  useEffect(() => {
    let disposed = false;
    let idleHandle: number | undefined;
    let timerHandle: ReturnType<typeof globalThis.setTimeout> | undefined;
    const stored = fileTreeStateRef.current.get(rootPath);
    setChildrenByPath({});
    setSelectedPath(stored?.selectedPath ?? "");
    setSelectedPaths(stored?.selectedPath ? new Set([stored.selectedPath]) : new Set());
    setSelectionAnchorPath(stored?.selectedPath ?? "");
    setPendingDeletePaths([]);
    setCopiedPath("");
    if (!rootPath) {
      setExpandedPaths(new Set());
      updateStatus(tm("files.panel.no_project", "No Project Selected"));
      return;
    }
    const nextExpanded = stored?.expandedPaths.size ? new Set(stored.expandedPaths) : new Set([rootPath]);
    setExpandedPaths(nextExpanded);
    updateStatus(tm("files.panel.status.ready", "Ready"));
    void loadChildren();
    const expandedDirectories = Array.from(nextExpanded).filter((path) => path !== rootPath);
    if (expandedDirectories.length > 0) {
      const restoreExpandedDirectories = () => {
        if (disposed) return;
        void Promise.all(expandedDirectories.map((path) => loadChildren(path)));
      };
      if (typeof window.requestIdleCallback === "function") {
        idleHandle = window.requestIdleCallback(restoreExpandedDirectories, { timeout: 700 });
      } else {
        timerHandle = globalThis.setTimeout(restoreExpandedDirectories, 120);
      }
    }
    return () => {
      disposed = true;
      if (idleHandle !== undefined && typeof window.cancelIdleCallback === "function") {
        window.cancelIdleCallback(idleHandle);
      }
      if (timerHandle !== undefined) {
        globalThis.clearTimeout(timerHandle);
      }
    };
  }, [loadChildren, rootPath, updateStatus]);

  const rows = useMemo(
    () => flattenFileRows(rootPath, childrenByPath, expandedPaths),
    [childrenByPath, expandedPaths, rootPath],
  );
  const fileTreeLabels = useMemo<FileTreeLabels>(
    () => ({
      open: tm("files.panel.open", "Open"),
      edit: tm("files.panel.edit", "Edit"),
      insertPathTerminal: tm("files.panel.insert_path_terminal", "Insert Path into Terminal"),
      copyPath: tm("files.panel.copy_path", "Copy Path"),
      copy: tm("files.panel.copy", "Copy"),
      cut: tm("files.panel.cut", "Cut"),
      paste: tm("files.panel.paste", "Paste"),
      reveal: tm("files.panel.reveal_finder", "Reveal in Finder"),
      rename: tm("common.rename", "Rename"),
      delete: tm("files.panel.delete", "Delete"),
      actions: tm("files.panel.actions", "Actions"),
    }),
    [],
  );
  const selectedEntry = useMemo(() => rows.find((row) => row.entry.path === selectedPath)?.entry, [rows, selectedPath]);
  const selectedEntries = useMemo(
    () => rows.filter((row) => selectedPaths.has(row.entry.path)).map((row) => row.entry),
    [rows, selectedPaths],
  );
  const pendingDeleteEntries = useMemo(
    () => rows.filter((row) => pendingDeletePaths.includes(row.entry.path)).map((row) => row.entry),
    [pendingDeletePaths, rows],
  );
  const pendingDeleteMessage = useMemo(
    () =>
      formatI18n(
        tm("files.panel.delete.pending_count_format", "%d item(s) marked for delete"),
        pendingDeletePaths.length,
      ),
    [pendingDeletePaths.length],
  );
  useEffect(() => {
    selectedEntryRef.current = selectedEntry;
  }, [selectedEntry]);
  useEffect(() => {
    rowsRef.current = rows;
  }, [rows]);
  useEffect(() => {
    selectedEntriesRef.current = selectedEntries;
  }, [selectedEntries]);
  useEffect(() => {
    selectedPathRef.current = selectedPath;
  }, [selectedPath]);
  useEffect(() => {
    inlineEditRef.current = inlineEdit;
  }, [inlineEdit]);

  const refresh = useCallback(() => {
    if (!rootPath) return;
    const remembered = new Set(expandedPaths);
    setChildrenByPath({});
    setExpandedPaths(remembered.size ? remembered : new Set([rootPath]));
    updateStatus(tm("files.panel.status.refreshing", "Refreshing files"));
    const loads = [loadChildren()];
    for (const path of remembered) {
      if (path !== rootPath) loads.push(loadChildren(path));
    }
    void Promise.all(loads).then((results) => {
      if (results.every(Boolean)) {
        updateStatus(tm("files.panel.status.refreshed", "Files refreshed"), "success");
      }
    });
  }, [expandedPaths, loadChildren, rootPath, updateStatus]);

  const targetDirectory = selectedEntry?.isDirectory
    ? selectedEntry.path
    : selectedPath
      ? parentPath(selectedPath, rootPath)
      : rootPath;

  const createItem = async (kind: "file" | "directory") => {
    if (!rootPath) return;
    setInlineEdit({
      mode: "create",
      kind,
      parentPath: targetDirectory,
      value: kind === "file" ? "untitled" : "New Folder",
    });
  };

  const submitInlineEdit = async () => {
    if (!rootPath || !inlineEdit) return;
    const name = inlineEdit.value.trim();
    if (!name) {
      setInlineEdit(null);
      return;
    }
    try {
      if (inlineEdit.mode === "rename") {
        if (name === inlineEdit.entry.name) {
          setInlineEdit(null);
          return;
        }
        const next = await renameFile(rootPath, inlineEdit.entry.path, name);
        setSelectedPath(next.path);
        setSelectedPaths(new Set([next.path]));
        setSelectionAnchorPath(next.path);
        setInlineEdit(null);
        updateStatus(formatI18n(tm("files.panel.status.renamed_format", "Renamed to %@"), next.name), "success");
        await loadChildren(parentPath(next.path, rootPath));
        return;
      }

      let entry: FileEntry;
      if (inlineEdit.kind === "file") {
        entry = await createFile(rootPath, inlineEdit.parentPath, name);
      } else {
        entry = await createDirectory(rootPath, inlineEdit.parentPath, name);
      }
      setExpandedPaths((current) => new Set(current).add(inlineEdit.parentPath));
      setSelectedPath(entry.path);
      setSelectedPaths(new Set([entry.path]));
      setSelectionAnchorPath(entry.path);
      setInlineEdit(null);
      updateStatus(formatI18n(tm("files.panel.status.created_format", "Created %@"), entry.name), "success");
      await loadChildren(inlineEdit.parentPath);
    } catch (nextError) {
      handleFileError(nextError);
    }
  };

  const renameEntry = useCallback(
    (entry?: FileEntry) => {
      const target = entry ?? selectedEntry;
      if (!rootPath || !target) return;
      setInlineEdit({
        mode: "rename",
        entry: target,
        parentPath: parentPath(target.path, rootPath),
        value: target.name,
      });
    },
    [rootPath, selectedEntry],
  );

  const stageDeleteEntries = useCallback(
    (entries: FileEntry[]) => {
      if (!rootPath || entries.length === 0) return;
      setPendingDeletePaths(entries.map((entry) => entry.path));
    },
    [rootPath],
  );

  const activateFilePanel = () => {
    filePanelActiveRef.current = true;
    fileTreeRef.current?.focus({ preventScroll: true });
  };

  const scrollFileRowIntoView = useCallback((path: string) => {
    window.requestAnimationFrame(() => {
      const row = fileTreeRef.current?.querySelector<HTMLElement>(`[data-file-path="${cssEscape(path)}"]`);
      row?.scrollIntoView({ block: "nearest" });
    });
  }, []);

  const selectEntryRange = useCallback(
    (entry: FileEntry, extend: boolean) => {
      setPendingDeletePaths([]);
      const visibleRows = rowsRef.current;
      if (extend) {
        const anchorPath = selectionAnchorPath || selectedPathRef.current || entry.path;
        const anchorIndex = visibleRows.findIndex((row) => row.entry.path === anchorPath);
        const targetIndex = visibleRows.findIndex((row) => row.entry.path === entry.path);
        if (anchorIndex >= 0 && targetIndex >= 0) {
          const [start, end] = anchorIndex < targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
          setSelectedPaths(new Set(visibleRows.slice(start, end + 1).map((row) => row.entry.path)));
          setSelectedPath(entry.path);
          scrollFileRowIntoView(entry.path);
          return;
        }
      }
      setSelectedPath(entry.path);
      setSelectedPaths(new Set([entry.path]));
      setSelectionAnchorPath(entry.path);
      scrollFileRowIntoView(entry.path);
    },
    [scrollFileRowIntoView, selectionAnchorPath],
  );

  const entriesForContextAction = (entry: FileEntry) =>
    selectedPaths.has(entry.path) && selectedEntries.length > 1 ? selectedEntries : [entry];

  const focusContextEntry = (entry: FileEntry) => {
    setPendingDeletePaths([]);
    if (selectedPaths.has(entry.path)) {
      setSelectedPath(entry.path);
      setSelectionAnchorPath(entry.path);
      return;
    }
    setSelectedPath(entry.path);
    setSelectedPaths(new Set([entry.path]));
    setSelectionAnchorPath(entry.path);
  };

  const copyEntryPaths = useCallback(
    (entries: FileEntry[]) => {
      if (entries.length === 0) return;
      void navigator.clipboard?.writeText(entries.map((entry) => entry.path).join("\n"));
      updateStatus(
        entries.length === 1
          ? formatI18n(tm("files.panel.status.copied_format", "Copied %@"), entries[0].name)
          : formatI18n(tm("files.panel.status.copied_paths_count_format", "Copied %d paths"), entries.length),
        "success",
      );
    },
    [updateStatus],
  );

  const confirmDeleteEntries = async () => {
    if (!rootPath || pendingDeleteEntries.length === 0) return;
    const targets = pendingDeleteEntries;
    try {
      const parentPaths = new Set(targets.map((target) => parentPath(target.path, rootPath)));
      for (const target of targets) {
        await deleteFile(rootPath, target.path);
      }
      if (targets.some((target) => selectedPaths.has(target.path))) {
        setSelectedPath("");
        setSelectedPaths(new Set());
        setSelectionAnchorPath("");
      }
      setPendingDeletePaths([]);
      updateStatus(
        targets.length === 1
          ? formatI18n(tm("files.panel.status.trashed_format", "Deleted %@"), targets[0].name)
          : formatI18n(tm("files.panel.status.trashed_count_format", "Deleted %d item(s)"), targets.length),
        "warning",
      );
      await Promise.all(Array.from(parentPaths).map((parent) => loadChildren(parent)));
    } catch (nextError) {
      handleFileError(nextError);
    }
  };

  const pasteCopiedPath = useCallback(async () => {
    if (!rootPath || !copiedPath) return;
    try {
      const entry = await copyFile(rootPath, copiedPath, targetDirectory);
      setExpandedPaths((current) => new Set(current).add(targetDirectory));
      setSelectedPath(entry.path);
      setSelectedPaths(new Set([entry.path]));
      setSelectionAnchorPath(entry.path);
      updateStatus(formatI18n(tm("files.panel.status.pasted_format", "Pasted %@"), entry.name), "success");
      await loadChildren(targetDirectory);
    } catch (nextError) {
      handleFileError(nextError);
    }
  }, [copiedPath, handleFileError, loadChildren, rootPath, targetDirectory, updateStatus]);

  const importFilesIntoTarget = useCallback(
    async (paths: string[], targetDirectoryPath = targetDirectory) => {
      if (!rootPath || paths.length === 0) return;
      try {
        const imported = await importExternalFiles(rootPath, paths, targetDirectoryPath);
        setExpandedPaths((current) => new Set(current).add(targetDirectoryPath));
        setSelectedPath(imported[0]?.path ?? "");
        setSelectedPaths(imported[0]?.path ? new Set([imported[0].path]) : new Set());
        setSelectionAnchorPath(imported[0]?.path ?? "");
        updateStatus(
          formatI18n(tm("files.panel.status.imported_count_format", "Imported %d item(s)"), imported.length),
          "success",
        );
        await loadChildren(targetDirectoryPath);
      } catch (nextError) {
        handleFileError(nextError);
      }
    },
    [handleFileError, loadChildren, rootPath, targetDirectory, updateStatus],
  );

  useEffect(() => {
    if (!rootPath || !window.__TAURI_INTERNALS__) return;
    let disposed = false;
    let unlisten: (() => void) | undefined;
    void getCurrentWindow()
      .onDragDropEvent((event) => {
        if (disposed) return;
        if (event.payload.type === "enter" || event.payload.type === "over") {
          setDraggingExternalFiles(true);
          updateStatus(tm("files.panel.status.drop_ready", "Release to copy into the current project"));
          return;
        }
        if (event.payload.type === "leave") {
          setDraggingExternalFiles(false);
          return;
        }
        if (event.payload.type === "drop") {
          setDraggingExternalFiles(false);
          void importFilesIntoTarget(event.payload.paths);
        }
      })
      .then((nextUnlisten) => {
        if (disposed) {
          nextUnlisten();
        } else {
          unlisten = nextUnlisten;
        }
      })
      .catch((nextError) => {
        handleFileError(nextError);
      });
    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [handleFileError, importFilesIntoTarget, rootPath, updateStatus]);

  useEffect(() => {
    if (!rootPath || !window.__TAURI_INTERNALS__) return;
    const projectPath = rootPath;
    let cancelled = false;
    let debounceTimer: number | undefined;
    let unlisten: (() => void) | undefined;
    let didUnlisten = false;
    const stopListening = (nextUnlisten: () => void) => {
      if (didUnlisten) return;
      didUnlisten = true;
      nextUnlisten();
    };

    const unlistenPromise = listen<FileChangeEvent>("file:changed", (event) => {
      if (cancelled || !fileEventTouchesRoot(event.payload, projectPath)) return;
      if (debounceTimer !== undefined) window.clearTimeout(debounceTimer);
      debounceTimer = window.setTimeout(() => {
        for (const path of expandedPathsRef.current) {
          void loadChildren(path === projectPath ? undefined : path);
        }
      }, FILE_TREE_WATCH_DEBOUNCE_MS);
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
        handleFileError(nextError);
      });

    void watchProjectFiles(projectPath).catch((nextError) => {
      if (cancelled) return;
      handleFileError(nextError);
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
  }, [handleFileError, loadChildren, rootPath]);

  const selectEntry = useCallback(
    (entry: FileEntry, options?: { extend?: boolean; toggle?: boolean }) => {
      setPendingDeletePaths([]);
      if (options?.extend && selectionAnchorPath) {
        const anchorIndex = rows.findIndex((row) => row.entry.path === selectionAnchorPath);
        const targetIndex = rows.findIndex((row) => row.entry.path === entry.path);
        if (anchorIndex >= 0 && targetIndex >= 0) {
          const [start, end] = anchorIndex < targetIndex ? [anchorIndex, targetIndex] : [targetIndex, anchorIndex];
          setSelectedPaths(new Set(rows.slice(start, end + 1).map((row) => row.entry.path)));
          setSelectedPath(entry.path);
          return;
        }
      }
      if (options?.toggle) {
        setSelectedPaths((current) => {
          const next = new Set(current);
          if (next.has(entry.path)) {
            next.delete(entry.path);
          } else {
            next.add(entry.path);
          }
          if (next.size === 0) {
            setSelectedPath(entry.path);
            return new Set([entry.path]);
          }
          const nextPaths = Array.from(next);
          setSelectedPath(next.has(entry.path) ? entry.path : (nextPaths[nextPaths.length - 1] ?? entry.path));
          return next;
        });
        setSelectionAnchorPath(entry.path);
        return;
      }
      setSelectedPath(entry.path);
      setSelectedPaths(new Set([entry.path]));
      setSelectionAnchorPath(entry.path);
      if (!entry.isDirectory) return;
      setExpandedPaths((current) => {
        const next = new Set(current);
        if (next.has(entry.path)) {
          next.delete(entry.path);
        } else {
          next.add(entry.path);
          if (!childrenByPath[entry.path]) void loadChildren(entry.path);
        }
        return next;
      });
    },
    [childrenByPath, loadChildren, rows, selectionAnchorPath],
  );

  const openEntry = useCallback(
    (entry: FileEntry) => {
      setSelectedPath(entry.path);
      setSelectedPaths(new Set([entry.path]));
      setSelectionAnchorPath(entry.path);
      if (entry.isDirectory) {
        selectEntry(entry);
        return;
      }
      broadcastWorkspaceCommand({
        type: "open-file",
        rootPath,
        path: entry.path,
      });
    },
    [rootPath, selectEntry],
  );

  const handleFileShortcutKeyDown = useCallback(
    (event: KeyboardEvent) => {
      if (!filePanelActiveRef.current || inlineEditRef.current) return;
      const target = event.target;
      if (target instanceof HTMLElement) {
        const tagName = target.tagName.toLowerCase();
        if (target.isContentEditable || tagName === "input" || tagName === "textarea" || tagName === "select") return;
      }
      const selected = selectedEntryRef.current;
      const visibleRows = rowsRef.current;
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "a") {
        event.preventDefault();
        if (visibleRows.length === 0) return;
        setPendingDeletePaths([]);
        setSelectedPaths(new Set(visibleRows.map((row) => row.entry.path)));
        const nextEntry = selected ?? visibleRows[0].entry;
        setSelectedPath(nextEntry.path);
        if (!selectionAnchorPath) setSelectionAnchorPath(nextEntry.path);
        scrollFileRowIntoView(nextEntry.path);
        return;
      }
      if (
        visibleRows.length > 0 &&
        (event.key === "ArrowUp" || event.key === "ArrowDown" || event.key === "Home" || event.key === "End")
      ) {
        event.preventDefault();
        const selectedIndex = visibleRows.findIndex((row) => row.entry.path === selectedPathRef.current);
        const currentIndex = selectedIndex >= 0 ? selectedIndex : event.key === "ArrowUp" ? visibleRows.length : -1;
        const nextIndex =
          event.key === "Home"
            ? 0
            : event.key === "End"
              ? visibleRows.length - 1
              : Math.min(visibleRows.length - 1, Math.max(0, currentIndex + (event.key === "ArrowDown" ? 1 : -1)));
        selectEntryRange(visibleRows[nextIndex].entry, event.shiftKey);
        return;
      }
      if (!selected) return;
      if (event.key === "ArrowRight") {
        event.preventDefault();
        if (selected.isDirectory) {
          if (!expandedPathsRef.current.has(selected.path)) {
            setExpandedPaths((current) => new Set(current).add(selected.path));
            if (!childrenByPath[selected.path]) void loadChildren(selected.path);
            return;
          }
          const currentIndex = visibleRows.findIndex((row) => row.entry.path === selected.path);
          const firstChild = currentIndex >= 0 ? visibleRows[currentIndex + 1] : undefined;
          if (firstChild && firstChild.depth > (visibleRows[currentIndex]?.depth ?? -1)) {
            selectEntryRange(firstChild.entry, event.shiftKey);
          }
        }
        return;
      }
      if (event.key === "ArrowLeft") {
        event.preventDefault();
        if (selected.isDirectory && expandedPathsRef.current.has(selected.path)) {
          setExpandedPaths((current) => {
            const next = new Set(current);
            next.delete(selected.path);
            return next;
          });
          return;
        }
        const parent = parentPath(selected.path, rootPath);
        const parentEntry = visibleRows.find((row) => row.entry.path === parent)?.entry;
        if (parentEntry) selectEntryRange(parentEntry, event.shiftKey);
        return;
      }
      if (event.key === "Enter" || event.key === "F2") {
        event.preventDefault();
        void renameEntry(selected);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "o") {
        event.preventDefault();
        openEntry(selected);
        return;
      }
      if (event.key === "Delete" || event.key === "Backspace") {
        event.preventDefault();
        const entries = selectedEntriesRef.current.length ? selectedEntriesRef.current : [selected];
        stageDeleteEntries(entries);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "c") {
        event.preventDefault();
        const entries = selectedEntriesRef.current.length ? selectedEntriesRef.current : [selected];
        setCopiedPath(entries.length === 1 ? entries[0].path : "");
        copyEntryPaths(entries);
        return;
      }
      if ((event.metaKey || event.ctrlKey) && event.key.toLowerCase() === "v") {
        event.preventDefault();
        void pasteCopiedPath();
      }
    },
    [
      childrenByPath,
      copyEntryPaths,
      loadChildren,
      openEntry,
      pasteCopiedPath,
      renameEntry,
      rootPath,
      scrollFileRowIntoView,
      selectionAnchorPath,
      selectEntryRange,
      stageDeleteEntries,
    ],
  );

  useEffect(() => {
    window.addEventListener("keydown", handleFileShortcutKeyDown);
    return () => window.removeEventListener("keydown", handleFileShortcutKeyDown);
  }, [handleFileShortcutKeyDown]);

  useEffect(() => {
    const handlePointerDown = (event: PointerEvent) => {
      if (!fileTreeRef.current?.contains(event.target as Node | null)) {
        filePanelActiveRef.current = false;
      }
    };
    window.addEventListener("pointerdown", handlePointerDown, true);
    return () => window.removeEventListener("pointerdown", handlePointerDown, true);
  }, []);

  return (
    <>
      <PanelHeader
        title={
          <div className="flex items-center gap-2">
            <Folder size={13} className="text-ink-mute" />
            <span>{tm("files.panel.title", "Files")}</span>
          </div>
        }
        trailing={
          <>
            <PanelIconButton
              icon={FileText}
              tooltip={tm("files.panel.new_file", "New File")}
              onClick={() => void createItem("file")}
            />
            <PanelIconButton
              icon={FolderPlus}
              tooltip={tm("files.panel.new_folder", "New Folder")}
              onClick={() => void createItem("directory")}
            />
            <PanelIconButton icon={RefreshCw} tooltip={tm("files.panel.refresh", "Refresh Files")} onClick={refresh} />
          </>
        }
      />
      <div className="px-3 pt-2 pb-1 text-xs text-ink-mute font-medium truncate">
        {project?.path?.split("/").pop() ?? tm("titlebar.projects", "Projects")}
      </div>
      <div
        ref={fileTreeRef}
        className={`relative flex-1 overflow-y-auto scrollbar-overlay px-1.5 pb-3 text-sm outline-none focus:outline-none focus-visible:outline-none ${isDraggingExternalFiles ? "bg-brand-blue/8" : ""}`}
        tabIndex={-1}
        onPointerDown={activateFilePanel}
        data-drop-zone
      >
        {!rootPath ? (
          <div className="px-2 py-3 text-xs text-ink-faint">{tm("files.panel.no_project", "No Project Selected")}</div>
        ) : rows.length > 0 || inlineEdit ? (
          <>
            {inlineEdit?.mode === "create" && inlineEdit.parentPath === rootPath && (
              <FileInlineEditRow
                edit={inlineEdit}
                depth={0}
                onChange={setInlineEdit}
                onCancel={() => setInlineEdit(null)}
                onSubmit={() => void submitInlineEdit()}
              />
            )}
            {rows.map((row) => (
              <FileTreeFragment
                key={row.entry.path}
                row={row}
                inlineEdit={inlineEdit}
                selected={selectedPaths.has(row.entry.path)}
                contextSelectionCount={selectedPaths.has(row.entry.path) ? selectedEntries.length : 1}
                expanded={expandedPaths.has(row.entry.path)}
                loading={loadingPaths.has(row.entry.path)}
                labels={fileTreeLabels}
                onInlineChange={setInlineEdit}
                onInlineCancel={() => setInlineEdit(null)}
                onInlineSubmit={() => void submitInlineEdit()}
                onSelect={(modifiers) => {
                  activateFilePanel();
                  selectEntry(row.entry, modifiers);
                }}
                onContextMenuOpen={() => {
                  activateFilePanel();
                  focusContextEntry(row.entry);
                }}
                onKeyAction={(event) => {
                  if (
                    event.key !== "Enter" &&
                    event.key !== "F2" &&
                    event.key !== "Delete" &&
                    event.key !== "Backspace"
                  )
                    return;
                  event.preventDefault();
                  event.stopPropagation();
                  const targets =
                    selectedPaths.has(row.entry.path) && selectedEntries.length > 1 ? selectedEntries : [row.entry];
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set(targets.map((entry) => entry.path)));
                  setSelectionAnchorPath(row.entry.path);
                  if (event.key === "Delete" || event.key === "Backspace") {
                    stageDeleteEntries(targets);
                  } else {
                    void renameEntry(row.entry);
                  }
                }}
                onOpen={() => openEntry(row.entry)}
                onEdit={() => {
                  setSelectedPath(row.entry.path);
                  openEntry(row.entry);
                }}
                onInsertPathIntoTerminal={() => {
                  const targets = entriesForContextAction(row.entry);
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set(targets.map((entry) => entry.path)));
                  setSelectionAnchorPath(row.entry.path);
                  broadcastWorkspaceCommand({
                    type: "insert-terminal-text",
                    text: targets.map((entry) => shellQuote(entry.path)).join(" "),
                  });
                }}
                onCopyPath={() => {
                  const targets = entriesForContextAction(row.entry);
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set(targets.map((entry) => entry.path)));
                  setSelectionAnchorPath(row.entry.path);
                  copyEntryPaths(targets);
                }}
                onRename={() => {
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set([row.entry.path]));
                  setSelectionAnchorPath(row.entry.path);
                  void renameEntry(row.entry);
                }}
                onDelete={() => {
                  const targets = entriesForContextAction(row.entry);
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set(targets.map((entry) => entry.path)));
                  setSelectionAnchorPath(row.entry.path);
                  stageDeleteEntries(targets);
                }}
                onCopy={() => {
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set([row.entry.path]));
                  setSelectionAnchorPath(row.entry.path);
                  setCopiedPath(row.entry.path);
                  void navigator.clipboard?.writeText(row.entry.path);
                  updateStatus(
                    formatI18n(tm("files.panel.status.copied_format", "Copied %@"), row.entry.name),
                    "success",
                  );
                }}
                onReveal={() => {
                  setSelectedPath(row.entry.path);
                  const targets = entriesForContextAction(row.entry);
                  setSelectedPaths(new Set(targets.map((entry) => entry.path)));
                  setSelectionAnchorPath(row.entry.path);
                  void Promise.all(targets.map((entry) => revealFile(rootPath, entry.path))).catch(handleFileError);
                }}
                onPaste={
                  copiedPath
                    ? () => {
                        setSelectedPath(row.entry.path);
                        setSelectedPaths(new Set([row.entry.path]));
                        setSelectionAnchorPath(row.entry.path);
                        void copyFile(
                          rootPath,
                          copiedPath,
                          row.entry.isDirectory ? row.entry.path : parentPath(row.entry.path, rootPath),
                        )
                          .then((entry) => {
                            const parent = parentPath(entry.path, rootPath);
                            setSelectedPath(entry.path);
                            setSelectedPaths(new Set([entry.path]));
                            setSelectionAnchorPath(entry.path);
                            setExpandedPaths((current) => new Set(current).add(parent));
                            updateStatus(
                              formatI18n(tm("files.panel.status.pasted_format", "Pasted %@"), entry.name),
                              "success",
                            );
                            return loadChildren(parent);
                          })
                          .catch((nextError) => {
                            handleFileError(nextError);
                          });
                      }
                    : undefined
                }
              />
            ))}
          </>
        ) : (
          <div className="px-2 py-3 text-xs text-ink-faint">
            {loadingPaths.has(rootPath)
              ? tm("files.panel.loading", "Reading files")
              : tm("files.panel.empty", "No Files")}
          </div>
        )}
        {isDraggingExternalFiles && (
          <div className="pointer-events-none absolute inset-2 grid place-items-center rounded-md border border-dashed border-brand-blue/55 bg-brand-blue/12 text-xs font-semibold text-brand-blue">
            {tm("files.panel.drop_to_copy", "Release to copy into the current project")}
          </div>
        )}
      </div>
      {pendingDeletePaths.length > 0 && (
        <PanelStatusBar
          tone="warning"
          leading={
            <>
              <FileText size={12} />
              <span className="truncate">{pendingDeleteMessage}</span>
            </>
          }
          trailing={
            <div className="flex items-center gap-1">
              <PressableButton
                className="h-6 rounded-md px-2 text-current/80 hover:bg-fill/10 hover:text-current"
                onPressUp={() => setPendingDeletePaths([])}
              >
                {tm("files.panel.delete.cancel", "Cancel Delete")}
              </PressableButton>
              <PressableButton
                className="h-6 rounded-md bg-brand-red px-2 font-semibold text-on-brand hover:bg-brand-red/90"
                onPressUp={() => void confirmDeleteEntries()}
              >
                {tm("files.panel.delete.confirm", "Confirm Delete")}
              </PressableButton>
            </div>
          }
        />
      )}
    </>
  );
}

type FileRowModel = {
  entry: FileEntry;
  depth: number;
};

type FileInlineEdit =
  | {
      mode: "create";
      kind: "file" | "directory";
      parentPath: string;
      value: string;
    }
  | {
      mode: "rename";
      entry: FileEntry;
      parentPath: string;
      value: string;
    };

type FileTreeLabels = {
  open: string;
  edit: string;
  insertPathTerminal: string;
  copyPath: string;
  copy: string;
  cut: string;
  paste: string;
  reveal: string;
  rename: string;
  delete: string;
  actions: string;
};

type FileSelectionModifiers = {
  extend: boolean;
  toggle: boolean;
};

function FileTreeFragment({
  row,
  inlineEdit,
  selected,
  contextSelectionCount,
  expanded,
  loading,
  labels,
  onInlineChange,
  onInlineCancel,
  onInlineSubmit,
  onSelect,
  onContextMenuOpen,
  onKeyAction,
  onOpen,
  onEdit,
  onInsertPathIntoTerminal,
  onCopyPath,
  onRename,
  onDelete,
  onCopy,
  onReveal,
  onPaste,
}: {
  row: FileRowModel;
  inlineEdit: FileInlineEdit | null;
  selected: boolean;
  contextSelectionCount: number;
  expanded: boolean;
  loading: boolean;
  labels: FileTreeLabels;
  onInlineChange: (edit: FileInlineEdit) => void;
  onInlineCancel: () => void;
  onInlineSubmit: () => void;
  onSelect: (modifiers: FileSelectionModifiers) => void;
  onContextMenuOpen?: () => void;
  onKeyAction?: (event: React.KeyboardEvent<HTMLButtonElement>) => void;
  onOpen: () => void;
  onEdit?: () => void;
  onInsertPathIntoTerminal?: () => void;
  onCopyPath?: () => void;
  onRename?: () => void;
  onDelete?: () => void;
  onCopy?: () => void;
  onReveal?: () => void;
  onPaste?: () => void;
}) {
  const editAfter = inlineEdit?.mode === "create" && inlineEdit.parentPath === row.entry.path && row.entry.isDirectory;
  const isRenaming = inlineEdit?.mode === "rename" && inlineEdit.entry.path === row.entry.path;

  return (
    <>
      {isRenaming && inlineEdit ? (
        <FileInlineEditRow
          edit={inlineEdit}
          depth={row.depth}
          onChange={onInlineChange}
          onCancel={onInlineCancel}
          onSubmit={onInlineSubmit}
        />
      ) : (
        <FileTreeRow
          row={row}
          selected={selected}
          contextSelectionCount={contextSelectionCount}
          expanded={expanded}
          loading={loading}
          labels={labels}
          onSelect={onSelect}
          onContextMenuOpen={onContextMenuOpen}
          onKeyAction={onKeyAction}
          onOpen={onOpen}
          onEdit={onEdit}
          onInsertPathIntoTerminal={onInsertPathIntoTerminal}
          onCopyPath={onCopyPath}
          onRename={onRename}
          onDelete={onDelete}
          onCopy={onCopy}
          onReveal={onReveal}
          onPaste={onPaste}
        />
      )}
      {editAfter && inlineEdit && (
        <FileInlineEditRow
          edit={inlineEdit}
          depth={row.depth + 1}
          onChange={onInlineChange}
          onCancel={onInlineCancel}
          onSubmit={onInlineSubmit}
        />
      )}
    </>
  );
}

function FileInlineEditRow({
  edit,
  depth,
  onChange,
  onCancel,
  onSubmit,
}: {
  edit: FileInlineEdit;
  depth: number;
  onChange: (edit: FileInlineEdit) => void;
  onCancel: () => void;
  onSubmit: () => void;
}) {
  const isDirectory = edit.mode === "create" ? edit.kind === "directory" : edit.entry.isDirectory;
  return (
    <form
      className="h-[26px] flex items-center rounded-md bg-fill/[0.065] text-ink"
      style={{ paddingLeft: `${8 + depth * 14}px` }}
      onSubmit={(event) => {
        event.preventDefault();
        onSubmit();
      }}
    >
      <span className="w-[11px]" />
      {isDirectory ? (
        <Folder size={12} className="mr-1.5 text-brand-blue/85" />
      ) : (
        <FileText size={12} className="mr-1.5 text-ink-mute" />
      )}
      <input
        className="h-5 min-w-0 flex-1 rounded border border-brand-blue/55 bg-surface-chrome px-1.5 text-xs outline-none"
        value={edit.value}
        autoFocus
        onFocus={(event) => event.currentTarget.select()}
        onChange={(event) => onChange({ ...edit, value: event.currentTarget.value })}
        onBlur={onSubmit}
        onKeyDown={(event) => {
          if (event.key === "Escape") {
            event.preventDefault();
            event.stopPropagation();
            onCancel();
          }
        }}
      />
    </form>
  );
}

function FileTreeRow({
  row,
  selected,
  contextSelectionCount,
  expanded,
  loading,
  onSelect,
  onContextMenuOpen,
  onKeyAction,
  onOpen,
  onEdit,
  onInsertPathIntoTerminal,
  onCopyPath,
  onRename,
  onDelete,
  onCopy,
  onReveal,
  onPaste,
  labels,
}: {
  row: FileRowModel;
  selected: boolean;
  contextSelectionCount: number;
  expanded: boolean;
  loading: boolean;
  labels: FileTreeLabels;
  onSelect: (modifiers: FileSelectionModifiers) => void;
  onContextMenuOpen?: () => void;
  onKeyAction?: (event: React.KeyboardEvent<HTMLButtonElement>) => void;
  onOpen: () => void;
  onEdit?: () => void;
  onInsertPathIntoTerminal?: () => void;
  onCopyPath?: () => void;
  onRename?: () => void;
  onDelete?: () => void;
  onCopy?: () => void;
  onReveal?: () => void;
  onPaste?: () => void;
}) {
  const entry = row.entry;
  const contextMenu = useContextMenu();
  const selectionModifiersRef = useRef<FileSelectionModifiers>({ extend: false, toggle: false });
  const isMultiContext = contextSelectionCount > 1;
  return (
    <Tooltip label={entry.relativePath || entry.name} placement="left" triggerClassName="block w-full">
      <div
        data-file-path={entry.path}
        onContextMenu={(event) => {
          onContextMenuOpen?.();
          contextMenu.openMenu(event);
        }}
        className={`group relative w-full h-[26px] flex items-center rounded-md transition-colors ${
          selected ? "bg-fill/[0.075] text-ink" : "text-ink-soft hover:bg-fill/[0.045] hover:text-ink"
        }`}
      >
        <PressableButton
          className="min-w-0 h-full flex-1 inline-flex items-center gap-1.5 pr-2 text-left"
          style={{ paddingLeft: `${8 + row.depth * 14}px` }}
          onPointerDown={(event) => {
            selectionModifiersRef.current = {
              extend: event.shiftKey,
              toggle: event.metaKey || event.ctrlKey,
            };
          }}
          onPressUp={() => onSelect(selectionModifiersRef.current)}
          onKeyDownCapture={onKeyAction}
          onDoubleClick={entry.isDirectory ? undefined : onOpen}
          excludeFromTabOrder={false}
        >
          {entry.isDirectory ? (
            <>
              {expanded ? (
                <ChevronDown size={11} className="text-ink-faint" />
              ) : (
                <ChevronRight size={11} className="text-ink-faint" />
              )}
              <Folder size={12} className="text-brand-blue/85" />
            </>
          ) : (
            <>
              <span className="w-[11px]" />
              <FileText size={12} className="text-ink-mute" />
            </>
          )}
          <span className="truncate text-xs">{entry.name}</span>
          {loading && <Spinner size="sm" color="current" className="ml-1 text-ink-faint" />}
        </PressableButton>
        <ContextMenu
          ariaLabel={`${entry.name} ${labels.actions}`}
          menu={contextMenu.menu}
          onClose={contextMenu.closeMenu}
        >
          <ContextMenuItem label={labels.open} disabled={entry.isDirectory || isMultiContext} onSelect={onOpen}>
            {labels.open}
          </ContextMenuItem>
          <ContextMenuItem label={labels.edit} disabled={entry.isDirectory || isMultiContext} onSelect={onEdit}>
            {labels.edit}
          </ContextMenuItem>
          <ContextMenuItem label={labels.insertPathTerminal} onSelect={onInsertPathIntoTerminal}>
            {labels.insertPathTerminal}
          </ContextMenuItem>
          <ContextMenuItem label={labels.copyPath} onSelect={onCopyPath}>
            {labels.copyPath}
          </ContextMenuItem>
          <ContextMenuItem label={labels.copy} disabled={isMultiContext} onSelect={onCopy}>
            {labels.copy}
          </ContextMenuItem>
          <ContextMenuItem label={labels.cut} disabled>
            {labels.cut}
          </ContextMenuItem>
          <ContextMenuItem label={labels.rename} disabled={isMultiContext} onSelect={onRename}>
            {labels.rename}
          </ContextMenuItem>
          <ContextMenuItem label={labels.paste} disabled={!onPaste} onSelect={onPaste}>
            {labels.paste}
          </ContextMenuItem>
          <ContextMenuItem label={labels.reveal} onSelect={onReveal}>
            {labels.reveal}
          </ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem label={labels.delete} onSelect={onDelete}>
            {labels.delete}
          </ContextMenuItem>
        </ContextMenu>
      </div>
    </Tooltip>
  );
}

function flattenFileRows(rootPath: string, childrenByPath: Record<string, FileEntry[]>, expandedPaths: Set<string>) {
  if (!rootPath) return [];
  const rows: FileRowModel[] = [];
  const visit = (directoryPath: string, depth: number) => {
    const children = childrenByPath[directoryPath] ?? [];
    for (const entry of children) {
      rows.push({ entry, depth });
      if (entry.isDirectory && expandedPaths.has(entry.path)) {
        visit(entry.path, depth + 1);
      }
    }
  };
  visit(rootPath, 0);
  return rows;
}

function parentPath(path: string, rootPath: string) {
  if (!path || path === rootPath) return rootPath;
  const index = path.lastIndexOf("/");
  if (index <= 0) return rootPath;
  const parent = path.slice(0, index);
  return parent.startsWith(rootPath) ? parent : rootPath;
}

function normalizeInspectorPath(value: string) {
  return value.replace(/\\/g, "/").replace(/\/+$/, "");
}

function cssEscape(value: string) {
  return typeof CSS !== "undefined" && typeof CSS.escape === "function"
    ? CSS.escape(value)
    : value.replace(/["\\]/g, "\\$&");
}

function fileEventTouchesRoot(event: FileChangeEvent, rootPath: string) {
  const root = normalizeInspectorPath(rootPath);
  const project = normalizeInspectorPath(event.projectPath);
  if (project !== root && !project.startsWith(`${root}/`) && !root.startsWith(`${project}/`)) {
    return false;
  }
  return event.changedPaths.some((path) => {
    const normalized = normalizeInspectorPath(path);
    return normalized === root || normalized.startsWith(`${root}/`);
  });
}

function shellQuote(value: string) {
  return `'${value.replace(/'/g, `'\\''`)}'`;
}

const MemoAIPanel = memo(AIPanel);

function AIPanel({ project }: { project?: WorkspaceProject }) {
  const { sessions } = useAIRuntimeSnapshot(project?.id);
  const history = useAIHistorySnapshot(project, { includeSessions: true });
  const [statisticsMode, setStatisticsMode] = useState<AIStatisticsMode>(
    () => readAppSettings().statisticsMode as AIStatisticsMode,
  );
  const [isManualRefreshFeedbackVisible, setManualRefreshFeedbackVisible] = useState(false);
  const manualRefreshStartedAtRef = useRef(0);
  const isRefreshingAIHistory = history.isLoading || isManualRefreshFeedbackVisible;
  const isForegroundAIIndexing = history.isForegroundLoading || isManualRefreshFeedbackVisible;
  const displayedProgress = isManualRefreshFeedbackVisible && !history.isLoading ? 1 : history.progress;
  const historySnapshot = history.snapshot;
  const indexedBaselines = useMemo(() => indexedSessionBaselines(historySnapshot.sessions), [historySnapshot.sessions]);
  const { projectTotalTokens, todayTotalTokens, toolRankingRows, modelRankingRows } = useMemo(() => {
    const liveProjectTokens = displayedLiveProjectTotals(sessions, indexedBaselines, statisticsMode);
    const liveTodayTokens = displayedTodayProjectTotals(sessions, indexedBaselines, statisticsMode);
    return {
      projectTotalTokens:
        displayedProjectSummaryTotal(historySnapshot.projectSummary, statisticsMode) + liveProjectTokens,
      todayTotalTokens: displayedProjectSummaryToday(historySnapshot, statisticsMode) + liveTodayTokens,
      toolRankingRows: toolRows(sessions, historySnapshot.toolBreakdown, indexedBaselines, statisticsMode),
      modelRankingRows: modelRows(sessions, historySnapshot.modelBreakdown, indexedBaselines, statisticsMode),
    };
  }, [historySnapshot, indexedBaselines, sessions, statisticsMode]);
  const refreshAIHistory = useCallback(async () => {
    manualRefreshStartedAtRef.current = Date.now();
    setManualRefreshFeedbackVisible(true);
    await history.refresh();
  }, [history]);

  useEffect(() => {
    if (!isManualRefreshFeedbackVisible || history.isLoading) return;
    const elapsed = Date.now() - manualRefreshStartedAtRef.current;
    const timer = window.setTimeout(
      () => setManualRefreshFeedbackVisible(false),
      Math.max(0, AI_REFRESH_FEEDBACK_MS - elapsed),
    );
    return () => window.clearTimeout(timer);
  }, [history.isLoading, isManualRefreshFeedbackVisible]);

  useEffect(() => {
    manualRefreshStartedAtRef.current = 0;
    setManualRefreshFeedbackVisible(false);
  }, [project?.id]);

  useEffect(
    () =>
      subscribeAppSettings((settings) => {
        setStatisticsMode(settings.statisticsMode as AIStatisticsMode);
      }),
    [],
  );

  return (
    <>
      <PanelHeader
        title={<span className="truncate">{tm("ai.panel.statistics_title", "AI Stats")}</span>}
        trailing={
          <PanelIconButton
            icon={RefreshCw}
            tooltip={
              isRefreshingAIHistory
                ? tm("ai.action.stop_refresh", "Stop the current AI stats refresh.")
                : tm("ai.action.refresh_current_project", "Refresh AI stats for the current project.")
            }
            busy={isRefreshingAIHistory}
            disabled={isRefreshingAIHistory}
            onClick={() => void refreshAIHistory()}
          />
        }
      />
      <div className="min-h-0 flex-1 overflow-y-auto scrollbar-overlay p-3 pb-5 flex flex-col gap-3">
        <PanelCard title={tm("ai.live_sessions", "Current Session Totals")} divider={false}>
          {sessions.length > 0 ? (
            <div className="flex flex-col gap-2">
              {sessions.map((session) => (
                <LiveSessionRow key={session.terminalId} session={session} mode={statisticsMode} />
              ))}
            </div>
          ) : (
            <div className="min-h-12 grid place-items-center text-xs font-medium text-ink-faint">
              {tm("ai.live_sessions.empty", "There are no current AI sessions right now")}
            </div>
          )}
        </PanelCard>

        <div className="grid grid-cols-2 gap-3">
          <Tooltip
            label={tm("ai.summary.current_project", "Current Project")}
            placement="bottom"
            triggerClassName="block w-full"
          >
            <PanelCard>
              <div className="text-xs text-ink-mute">{tm("ai.summary.current_project", "Current Project")}</div>
              <div className="text-lg font-semibold mt-1 tabular-nums">{formatTokens(projectTotalTokens)}</div>
            </PanelCard>
          </Tooltip>
          <Tooltip
            label={tm("ai.summary.today_total", "Today's Total")}
            placement="bottom"
            triggerClassName="block w-full"
          >
            <PanelCard>
              <div className="text-xs text-ink-mute">{tm("ai.summary.today_total", "Today's Total")}</div>
              <div className="text-lg font-semibold mt-1 tabular-nums">{formatTokens(todayTotalTokens)}</div>
            </PanelCard>
          </Tooltip>
        </div>

        <PanelCard title={tm("ai.today_usage", "Today's Usage")}>
          <BarsRow
            sessions={sessions}
            buckets={historySnapshot.todayTimeBuckets}
            indexedBaselines={indexedBaselines}
            mode={statisticsMode}
          />
          <div className="flex justify-between mt-1 text-xs text-ink-faint">
            <span>00:00</span>
            <span>06:00</span>
            <span>12:00</span>
            <span>18:00</span>
            <span>23:59</span>
          </div>
        </PanelCard>

        <PanelCard title={tm("ai.recent_usage", "Recent Usage")}>
          <HeatmapGrid
            sessions={sessions}
            days={historySnapshot.heatmap}
            indexedBaselines={indexedBaselines}
            mode={statisticsMode}
          />
        </PanelCard>

        <PanelCard title={tm("ai.breakdown.tool_ranking", "Tool Ranking")}>
          {toolRankingRows.map((row) => (
            <RankRow
              key={row.name}
              name={row.name}
              value={formatTokens(row.total)}
              pct={row.pct}
              tooltip={formatI18n(tm("ai.metric.usage_format", "%@ used %@ tokens"), row.name, formatTokens(row.total))}
            />
          ))}
          {toolRankingRows.length === 0 && <EmptyMetricRow label={tm("ai.empty.no_stats", "No AI Stats Yet")} />}
        </PanelCard>

        <PanelCard title={tm("ai.breakdown.model_ranking", "Model Ranking")}>
          {modelRankingRows.map((row) => (
            <RankRow
              key={row.name}
              name={row.name}
              value={formatTokens(row.total)}
              pct={row.pct}
              tooltip={formatI18n(tm("ai.metric.usage_format", "%@ used %@ tokens"), row.name, formatTokens(row.total))}
            />
          ))}
          {modelRankingRows.length === 0 && <EmptyMetricRow label={tm("ai.empty.no_stats", "No AI Stats Yet")} />}
        </PanelCard>
      </div>

      <AIIndexingStatusBar
        error={history.error}
        isLoading={isRefreshingAIHistory}
        isForegroundIndexing={isForegroundAIIndexing}
        statusDetail={history.detail}
        progress={displayedProgress}
        indexedAt={historySnapshot.indexedAt}
        onRefresh={() => void refreshAIHistory()}
      />
    </>
  );
}

function AIIndexingStatusBar({
  error,
  isLoading,
  isForegroundIndexing,
  statusDetail,
  progress,
  indexedAt,
  onRefresh,
}: {
  error: string | null;
  isLoading: boolean;
  isForegroundIndexing: boolean;
  statusDetail: string;
  progress: number | null;
  indexedAt: number;
  onRefresh: () => void;
}) {
  const status = aiIndexingPresentation({
    error,
    isLoading,
    isForegroundIndexing,
    statusDetail,
    progress,
    indexedAt,
  });
  const isFailed = Boolean(error);
  const actionLabel = isFailed ? tm("common.retry", "Retry") : tm("common.refresh", "Refresh");
  const actionTooltip = isFailed
    ? tm("ai.action.reload_current_project", "Reload AI stats for the current project.")
    : tm("ai.action.refresh_current_project", "Refresh AI stats for the current project.");

  return (
    <PanelStatusBar
      tone={status.tone}
      leading={
        <div className="min-w-0 flex items-center gap-2 font-semibold">
          {status.indicator === "spinner" ? (
            <Spinner size="sm" color="current" className="text-current/95" />
          ) : status.indicator === "progress" ? (
            <div className="w-[42px]">
              <ProgressBar
                aria-label={status.text}
                value={status.progressValue}
                maxValue={100}
                size="sm"
                color="warning"
                className="w-full"
              >
                <ProgressBar.Track className="h-1 bg-white/25">
                  <ProgressBar.Fill className="h-full bg-white/90" />
                </ProgressBar.Track>
              </ProgressBar>
            </div>
          ) : (
            <CheckCircle2 size={14} />
          )}
          <span className="truncate">{status.text}</span>
        </div>
      }
      trailing={
        status.showRefreshAction ? (
          <Tooltip label={actionTooltip} placement="top">
            <HeroButton
              size="sm"
              variant="ghost"
              className="h-7 min-w-0 px-2 text-xs text-current/90 hover:text-current hover:bg-white/14"
              onPress={onRefresh}
            >
              <RefreshCw size={12} strokeWidth={2} />
              <span className="text-xs font-semibold">{actionLabel}</span>
            </HeroButton>
          </Tooltip>
        ) : null
      }
    />
  );
}

function EmptyMetricRow({ label }: { label: string }) {
  return <div className="text-xs text-ink-faint">{label}</div>;
}

const LiveSessionRow = memo(function LiveSessionRow({
  session,
  mode,
}: {
  session: AISessionSnapshot;
  mode: AIStatisticsMode;
}) {
  const model = session.model || "-";
  return (
    <Tooltip label={session.sessionTitle} placement="left" triggerClassName="block w-full">
      <div className="flex items-start justify-between gap-3 rounded-lg bg-fill/[0.06] px-2.5 py-2">
        <div className="min-w-0">
          <div className="text-sm font-semibold text-ink truncate">{session.tool || "-"}</div>
          <div className="mt-0.5 text-xs font-medium text-ink-soft truncate">{model}</div>
        </div>
        <div className="flex-shrink-0 text-right">
          <div className="text-base font-semibold tabular-nums text-ink leading-none">
            {formatTokens(displayedLiveSessionTotal(session, mode))}
          </div>
          <div className="mt-1 text-xs text-ink-faint">{tm("ai.metric.session_total", "Session Total")}</div>
        </div>
      </div>
    </Tooltip>
  );
});

type IndexedSessionBaselines = Map<string, { totalTokens: number; cachedInputTokens: number }>;

function toolRows(
  sessions: AISessionSnapshot[],
  historyRows: AIUsageBreakdownItem[],
  indexedBaselines: IndexedSessionBaselines,
  mode: AIStatisticsMode,
) {
  return rankRows(sessions, historyRows, indexedBaselines, (session) => session.tool, mode);
}

function modelRows(
  sessions: AISessionSnapshot[],
  historyRows: AIUsageBreakdownItem[],
  indexedBaselines: IndexedSessionBaselines,
  mode: AIStatisticsMode,
) {
  return rankRows(sessions, historyRows, indexedBaselines, (session) => normalizeRankModelName(session.model), mode);
}

function rankRows(
  sessions: AISessionSnapshot[],
  historyRows: AIUsageBreakdownItem[],
  indexedBaselines: IndexedSessionBaselines,
  keyOf: (session: AISessionSnapshot) => string | null,
  mode: AIStatisticsMode,
) {
  const totals = new Map<string, number>();
  for (const row of historyRows) {
    if (!isDisplayableModelOrToolKey(row.key)) continue;
    totals.set(row.key, (totals.get(row.key) ?? 0) + displayedBreakdownTokens(row, mode));
  }
  for (const session of sessions) {
    const key = keyOf(session);
    if (!key || !isDisplayableModelOrToolKey(key)) continue;
    const value = displayedSessionDeltaTokens(session, mode, indexedBaselines);
    totals.set(key, (totals.get(key) ?? 0) + value);
  }
  const max = Math.max(...totals.values(), 1);
  return [...totals.entries()]
    .sort((left, right) => right[1] - left[1])
    .slice(0, 4)
    .map(([name, total]) => ({
      name,
      total,
      pct: Math.round((total / max) * 100),
    }));
}

function normalizeRankModelName(value?: string | null) {
  const trimmed = value?.trim();
  if (!trimmed || trimmed.toLowerCase() === "unknown") return null;
  return trimmed;
}

function isDisplayableModelOrToolKey(value: string) {
  return value.trim().length > 0 && value.trim().toLowerCase() !== "unknown";
}

function formatTokens(value: number) {
  if (value >= 1_000_000) return `${(value / 1_000_000).toFixed(2)}M`;
  if (value >= 1_000) return `${(value / 1_000).toFixed(1)}K`;
  return String(Math.max(0, Math.floor(value)));
}

function displayedSessionDeltaTokens(
  session: AISessionSnapshot,
  mode: AIStatisticsMode,
  indexedBaselines: IndexedSessionBaselines = new Map(),
  today?: number,
) {
  const indexedBaseline = indexedBaselineForSession(session, indexedBaselines);
  const updatedDay = startOfLocalDay(new Date(session.updatedAt * 1000)).getTime();
  if (today != null && updatedDay !== today) return 0;
  const startedDay = startOfLocalDay(new Date((session.startedAt ?? session.updatedAt) * 1000)).getTime();
  const todayTotalBaseline = today != null && startedDay !== today ? session.totalTokens : 0;
  const todayCachedBaseline = today != null && startedDay !== today ? session.cachedInputTokens : 0;
  const totalBaseline = Math.max(session.baselineTotalTokens, indexedBaseline.totalTokens, todayTotalBaseline);
  const cachedBaseline = Math.max(
    session.baselineCachedInputTokens ?? 0,
    indexedBaseline.cachedInputTokens,
    todayCachedBaseline,
  );
  const totalDelta = Math.max(0, session.totalTokens - totalBaseline);
  const cachedDelta = Math.max(0, session.cachedInputTokens - cachedBaseline);
  return totalDelta + (mode === "includingCache" ? cachedDelta : 0);
}

function displayedLiveProjectTotals(
  sessions: AISessionSnapshot[],
  indexedBaselines: IndexedSessionBaselines,
  mode: AIStatisticsMode,
) {
  return sessions.reduce((total, session) => total + displayedSessionDeltaTokens(session, mode, indexedBaselines), 0);
}

function indexedSessionBaselines(sessions: AIHistorySessionSummary[]): IndexedSessionBaselines {
  const baselines: IndexedSessionBaselines = new Map();
  for (const session of sessions) {
    const key = indexedSessionKey(session.lastTool, session.externalSessionId);
    if (!key) continue;
    const previous = baselines.get(key) ?? { totalTokens: 0, cachedInputTokens: 0 };
    baselines.set(key, {
      totalTokens: Math.max(previous.totalTokens, session.totalTokens),
      cachedInputTokens: Math.max(previous.cachedInputTokens, session.cachedInputTokens),
    });
  }
  return baselines;
}

function indexedBaselineForSession(session: AISessionSnapshot, baselines: IndexedSessionBaselines) {
  const key = indexedSessionKey(session.tool, session.aiSessionId);
  if (!key) return { totalTokens: 0, cachedInputTokens: 0 };
  return baselines.get(key) ?? { totalTokens: 0, cachedInputTokens: 0 };
}

function indexedSessionKey(tool?: string | null, externalSessionId?: string | null) {
  const normalizedTool = tool?.trim().toLowerCase();
  const normalizedSessionId = externalSessionId?.trim();
  if (!normalizedTool || !normalizedSessionId) return null;
  return `${normalizedTool}|${normalizedSessionId}`;
}

function displayedLiveSessionTotal(session: AISessionSnapshot, mode: AIStatisticsMode) {
  return Math.max(0, session.totalTokens) + (mode === "includingCache" ? Math.max(0, session.cachedInputTokens) : 0);
}

function displayedTodayProjectTotals(
  sessions: AISessionSnapshot[],
  indexedBaselines: IndexedSessionBaselines,
  mode: AIStatisticsMode,
) {
  const today = startOfLocalDay(new Date()).getTime();
  return sessions.reduce((total, session) => {
    return total + displayedSessionDeltaTokens(session, mode, indexedBaselines, today);
  }, 0);
}

function displayedProjectSummaryTotal(
  summary: { projectTotalTokens: number; projectCachedInputTokens: number },
  mode: AIStatisticsMode,
) {
  return summary.projectTotalTokens + (mode === "includingCache" ? summary.projectCachedInputTokens : 0);
}

function displayedProjectSummaryToday(snapshot: AIHistorySnapshot, mode: AIStatisticsMode) {
  const bucketTotal = snapshot.todayTimeBuckets.reduce(
    (total, bucket) => total + displayedBucketTokens(bucket, mode),
    0,
  );
  const today = startOfLocalDay(new Date()).getTime();
  const heatmapTotal = snapshot.heatmap.reduce((total, day) => {
    const dayStart = startOfLocalDay(new Date(day.day * 1000)).getTime();
    return dayStart === today ? total + displayedHeatmapTokens(day, mode) : total;
  }, 0);
  const summaryTotal = hasFreshTodayEvidence(snapshot, today)
    ? Math.max(0, snapshot.projectSummary.todayTotalTokens) +
      (mode === "includingCache" ? Math.max(0, snapshot.projectSummary.todayCachedInputTokens) : 0)
    : 0;
  return Math.max(summaryTotal, bucketTotal, heatmapTotal);
}

function hasFreshTodayEvidence(snapshot: AIHistorySnapshot, today: number) {
  if (snapshot.todayTimeBuckets.some((bucket) => startOfLocalDay(new Date(bucket.start * 1000)).getTime() === today)) {
    return true;
  }
  if (snapshot.heatmap.some((day) => startOfLocalDay(new Date(day.day * 1000)).getTime() === today)) {
    return true;
  }
  const updatedAt = snapshot.projectSummary.currentSessionUpdatedAt;
  if (updatedAt && startOfLocalDay(new Date(updatedAt * 1000)).getTime() === today) {
    return true;
  }
  return Boolean(snapshot.indexedAt && startOfLocalDay(new Date(snapshot.indexedAt * 1000)).getTime() === today);
}

function displayedBucketTokens(bucket: AITimeBucket, mode: AIStatisticsMode) {
  return bucket.totalTokens + (mode === "includingCache" ? bucket.cachedInputTokens : 0);
}

function displayedHeatmapTokens(day: AIHeatmapDay, mode: AIStatisticsMode) {
  return day.totalTokens + (mode === "includingCache" ? day.cachedInputTokens : 0);
}

function displayedBreakdownTokens(row: AIUsageBreakdownItem, mode: AIStatisticsMode) {
  return row.totalTokens + (mode === "includingCache" ? row.cachedInputTokens : 0);
}

const BarsRow = memo(function BarsRow({
  sessions,
  buckets,
  indexedBaselines,
  mode,
}: {
  sessions: AISessionSnapshot[];
  buckets: AITimeBucket[];
  indexedBaselines: IndexedSessionBaselines;
  mode: AIStatisticsMode;
}) {
  const data = useMemo(() => {
    const today = startOfLocalDay(new Date());
    const todayEnd = endOfLocalDay(today);
    const values = Array.from({ length: 48 }, (_, index) => {
      const start = new Date(today);
      start.setMinutes(index * 30, 0, 0);
      const end = index === 47 ? todayEnd : new Date(today);
      if (index !== 47) {
        end.setMinutes((index + 1) * 30, 0, 0);
      }
      return { index, start, end, value: 0, requestCount: 0 };
    });
    for (const bucket of buckets) {
      const date = new Date(bucket.start * 1000);
      if (startOfLocalDay(date).getTime() !== today.getTime()) continue;
      const index = todayBucketIndex(date);
      values[index].value += displayedBucketTokens(bucket, mode);
      values[index].requestCount += bucket.requestCount;
    }
    for (const session of sessions) {
      const date = new Date(session.updatedAt * 1000);
      if (startOfLocalDay(date).getTime() !== today.getTime()) continue;
      values[todayBucketIndex(date)].value += displayedSessionDeltaTokens(
        session,
        mode,
        indexedBaselines,
        today.getTime(),
      );
    }
    return values;
  }, [buckets, indexedBaselines, mode, sessions]);
  const max = Math.max(...data.map((d) => d.value), 1);
  return (
    <div className="flex items-end gap-px h-[64px]">
      {data.map((d) => {
        const hasValue = d.value > 0;
        const h = hasValue ? Math.max(2, Math.round((d.value / max) * 56)) : 2;
        return (
          <Tooltip
            key={d.index}
            label={
              <span>
                {formatTime(d.start)} - {formatBucketEndTime(d.end, d.index)} · {formatTokens(d.value)}
                {d.requestCount > 0
                  ? ` · ${formatI18n(tm("common.requests_format", "Requests %@"), d.requestCount)}`
                  : ""}
              </span>
            }
            placement="top"
            triggerClassName="flex flex-1 h-full min-w-0"
          >
            <div className="flex items-end h-full w-full">
              <div
                className={`w-full rounded-[3px] transition-colors ${
                  hasValue ? "bg-brand-blue/70 hover:bg-brand-blue" : "bg-brand-blue/18 hover:bg-brand-blue/35"
                }`}
                style={{ height: `${h}px` }}
              />
            </div>
          </Tooltip>
        );
      })}
    </div>
  );
});

const HEATMAP_GAP = 3;
const HEATMAP_BASE_CELL = 9;
const HEATMAP_DEFAULT_LAYOUT = { columns: 15, cellSize: 9 };

const HeatmapGrid = memo(function HeatmapGrid({
  sessions,
  days,
  indexedBaselines,
  mode,
}: {
  sessions: AISessionSnapshot[];
  days: AIHeatmapDay[];
  indexedBaselines: IndexedSessionBaselines;
  mode: AIStatisticsMode;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const [layout, setLayout] = useState(HEATMAP_DEFAULT_LAYOUT);
  const { columns, cellSize } = layout;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const updateLayout = () => {
      const width = host.clientWidth;
      if (width <= 0) return;

      const nextColumns = Math.max(2, Math.floor((width + HEATMAP_GAP) / (HEATMAP_BASE_CELL + HEATMAP_GAP)));
      const nextCellSize = Math.max(
        8,
        Math.min(10, Math.floor((width - HEATMAP_GAP * Math.max(nextColumns - 1, 0)) / nextColumns)),
      );

      setLayout((current) =>
        current.columns === nextColumns && current.cellSize === nextCellSize
          ? current
          : { columns: nextColumns, cellSize: nextCellSize },
      );
    };

    updateLayout();
    if (typeof ResizeObserver === "undefined") return;
    const resizeObserver = new ResizeObserver(updateLayout);
    resizeObserver.observe(host);
    return () => resizeObserver.disconnect();
  }, []);

  const data = useMemo(() => {
    const today = startOfLocalDay(new Date());
    const firstDay = new Date(today);
    firstDay.setDate(today.getDate() - (columns * 7 - 1));
    const values = new Map<number, { value: number; requestCount: number }>();
    for (const day of days) {
      values.set(startOfLocalDay(new Date(day.day * 1000)).getTime(), {
        value: displayedHeatmapTokens(day, mode),
        requestCount: day.requestCount,
      });
    }
    for (const session of sessions) {
      const day = startOfLocalDay(new Date(session.updatedAt * 1000));
      const existing = values.get(day.getTime()) ?? { value: 0, requestCount: 0 };
      values.set(day.getTime(), {
        value: existing.value + displayedSessionDeltaTokens(session, mode, indexedBaselines),
        requestCount: existing.requestCount,
      });
    }
    const cells = Array.from({ length: columns }, (_, col) =>
      Array.from({ length: 7 }, (_, row) => {
        const day = new Date(firstDay);
        day.setDate(firstDay.getDate() + col * 7 + row);
        const item = values.get(day.getTime());
        return {
          day,
          value: item?.value ?? 0,
          requestCount: item?.requestCount ?? 0,
          isKnown: item !== undefined,
        };
      }),
    );
    const nonZero = cells
      .flat()
      .map((item) => item.value)
      .filter((value) => value > 0)
      .sort((a, b) => a - b);
    return { cells, nonZero };
  }, [columns, days, indexedBaselines, mode, sessions]);

  const intensity = (v: number) => {
    if (v <= 0) return 0.14;
    if (data.nonZero.length <= 1) return 1;
    const upper = data.nonZero.findIndex((value) => value > v);
    const rank = Math.max(0, (upper === -1 ? data.nonZero.length : upper) - 1);
    const ratio = rank / Math.max(data.nonZero.length - 1, 1);
    if (ratio < 0.1) return 0.14;
    if (ratio < 0.2) return 0.22;
    if (ratio < 0.32) return 0.3;
    if (ratio < 0.44) return 0.4;
    if (ratio < 0.56) return 0.52;
    if (ratio < 0.68) return 0.64;
    if (ratio < 0.8) return 0.76;
    if (ratio < 0.92) return 0.88;
    return 1;
  };
  const gridWidth = columns * cellSize + Math.max(columns - 1, 0) * HEATMAP_GAP;
  const gridHeight = 7 * cellSize + 6 * HEATMAP_GAP;

  return (
    <div ref={hostRef} className="w-full overflow-hidden">
      <div
        className="grid grid-flow-col"
        style={{
          gap: `${HEATMAP_GAP}px`,
          gridTemplateRows: `repeat(7, ${cellSize}px)`,
          gridAutoColumns: `${cellSize}px`,
          width: `${gridWidth}px`,
          height: `${gridHeight}px`,
        }}
      >
        {data.cells.flatMap((column, colIdx) =>
          column.map((item, rowIdx) => {
            const alpha = intensity(item.value);
            return (
              <Tooltip
                key={`${colIdx}-${rowIdx}`}
                label={
                  <span>
                    {formatHeatmapDate(item.day)} · {formatTokens(item.value)}
                    {item.requestCount > 0
                      ? ` · ${formatI18n(tm("common.requests_format", "Requests %@"), item.requestCount)}`
                      : ""}
                  </span>
                }
                placement="top"
                triggerClassName="block"
              >
                <div
                  className="rounded-[3px] transition-colors"
                  style={{
                    width: `${cellSize}px`,
                    height: `${cellSize}px`,
                    background: item.isKnown
                      ? `color-mix(in oklab, var(--color-brand-blue) ${Math.round(alpha * 100)}%, transparent)`
                      : "color-mix(in oklab, var(--color-fill) 12%, transparent)",
                  }}
                />
              </Tooltip>
            );
          }),
        )}
      </div>
    </div>
  );
});

function startOfLocalDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate());
}

function endOfLocalDay(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), date.getDate(), 23, 59, 59, 999);
}

function todayBucketIndex(date: Date) {
  return Math.min(47, Math.max(0, date.getHours() * 2 + (date.getMinutes() >= 30 ? 1 : 0)));
}

function formatTime(date: Date) {
  return `${String(date.getHours()).padStart(2, "0")}:${String(date.getMinutes()).padStart(2, "0")}`;
}

function formatBucketEndTime(date: Date, bucketIndex: number) {
  if (bucketIndex !== 47) return formatTime(date);
  return `${formatTime(date)}:${String(date.getSeconds()).padStart(2, "0")}`;
}

function formatHeatmapDate(date: Date) {
  return new Intl.DateTimeFormat(undefined, {
    month: "numeric",
    day: "numeric",
    weekday: "short",
  }).format(date);
}

const RankRow = memo(function RankRow({
  name,
  value,
  pct,
  tooltip,
}: {
  name: string;
  value: string;
  pct: number;
  tooltip?: string;
}) {
  const body = (
    <div className="py-1.5 cursor-default">
      <div className="flex items-center justify-between gap-3 text-[13px] leading-5">
        <span className="text-ink font-medium truncate">{name}</span>
        <span className="flex items-center gap-2">
          <span className="tabular-nums text-ink-soft font-semibold">{value}</span>
          <span className="text-ink-faint w-9 text-right text-[11px] tabular-nums">{pct}%</span>
        </span>
      </div>
      <div className="mt-1.5 h-1 rounded-full bg-fill/[0.08] overflow-hidden">
        <div className="h-full rounded-full bg-brand-blue/65" style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
  if (tooltip) {
    return (
      <Tooltip label={tooltip} placement="left" triggerClassName="block w-full">
        {body}
      </Tooltip>
    );
  }
  return body;
});

type SSHCredentialKind = "none" | "password" | "privateKey";

type SSHConnectionProfile = {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  credentialKind: SSHCredentialKind;
  privateKeyPath: string;
  updatedAt: number;
  password?: string | null;
  keyPassphrase?: string | null;
};

type SSHProfilesSnapshot = {
  profiles: SSHConnectionProfile[];
};

type SSHLaunchCommand = {
  command: string;
  logCommand: string;
};

function SSHPanel({ project }: { project?: WorkspaceProject }) {
  const [profiles, setProfiles] = useState<SSHConnectionProfile[]>([]);
  const [isLoading, setLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [draft, setDraft] = useState<SSHProfileDraft | null>(null);
  const [isSaving, setSaving] = useState(false);
  const sshRowLabels = useMemo<SSHRowLabels>(
    () => ({
      connect: tm("ssh.profile.connect", "Connect"),
      copy: tm("ssh.profile.copy_command", "Copy SSH Command"),
      edit: tm("ssh.profile.edit", "Edit SSH Connection"),
      remove: tm("common.remove", "Remove"),
      actions: tm("files.panel.actions", "Actions"),
    }),
    [],
  );

  const handleSshError = useCallback((nextError: unknown) => {
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    setError(message);
  }, []);

  const refresh = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__) {
      setProfiles([]);
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const snapshot = await invoke<SSHProfilesSnapshot>("ssh_profiles");
      setProfiles(snapshot.profiles);
    } catch (nextError) {
      handleSshError(nextError);
    } finally {
      setLoading(false);
    }
  }, [handleSshError]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const startProfileEdit = (profile?: SSHConnectionProfile) => {
    setError(null);
    setDraft(profileToDraft(profile));
  };

  const pickPrivateKey = async () => {
    if (!window.__TAURI_INTERNALS__) return;
    const selected = await openLocalizedDialog({
      title: tm("ssh.profile.choose_key.title", "Choose Private Key"),
      message: tm("ssh.profile.choose_key.message", "Choose the private key used for this SSH connection."),
      prompt: tm("common.choose", "Choose"),
      multiple: false,
      directory: false,
    });
    if (!selected || Array.isArray(selected)) return;
    setDraft((current) => (current ? { ...current, privateKeyPath: selected } : current));
  };

  const upsertProfile = async (nextDraft: SSHProfileDraft) => {
    const validationError = sshDraftValidationError(nextDraft);
    if (validationError) {
      setError(validationError);
      return;
    }
    try {
      setSaving(true);
      const snapshot = await invoke<SSHProfilesSnapshot>("ssh_profile_upsert", {
        request: draftToSSHProfileRequest(nextDraft),
      });
      setProfiles(snapshot.profiles);
      setDraft(null);
      setError(null);
    } catch (nextError) {
      handleSshError(nextError);
    } finally {
      setSaving(false);
    }
  };

  const deleteProfile = async (profile: SSHConnectionProfile) => {
    if (
      !(await systemConfirm(
        formatI18n(
          tm("ssh.profile.delete.message_format", "Delete %@? The saved local credential will also be removed."),
          sshDisplayName(profile),
        ),
        {
          title: tm("ssh.profile.delete", "Delete SSH Connection"),
          kind: "warning",
          okLabel: tm("common.delete", "Delete"),
          cancelLabel: tm("common.cancel", "Cancel"),
        },
      ))
    )
      return;
    try {
      const snapshot = await invoke<SSHProfilesSnapshot>("ssh_profile_delete", {
        profileId: profile.id,
      });
      setProfiles(snapshot.profiles);
      setError(null);
    } catch (nextError) {
      handleSshError(nextError);
    }
  };

  const connectProfile = async (profile: SSHConnectionProfile) => {
    if (!project) {
      setError(tm("ssh.panel.status.no_project", "Select a project before connecting."));
      return;
    }
    try {
      const launch = await invoke<SSHLaunchCommand>("ssh_launch_command", {
        profileId: profile.id,
      });
      broadcastWorkspaceCommand({
        type: "add-bottom-terminal-tab",
        label: sshDisplayName(profile),
        command: launch.command,
      });
      setError(null);
    } catch (nextError) {
      handleSshError(nextError);
    }
  };

  return (
    <>
      <PanelHeader
        title={
          <div className="flex items-center gap-2">
            <Server size={13} className="text-ink-mute" />
            <span>{tm("ssh.panel.title", "SSH")}</span>
          </div>
        }
        trailing={
          <PanelIconButton
            icon={Plus}
            tooltip={tm("ssh.profile.add", "Add SSH Connection")}
            onClick={() => startProfileEdit()}
          />
        }
      />
      {profiles.length === 0 && !draft ? (
        <PanelEmptyState
          icon={Server}
          title={
            isLoading
              ? tm("ssh.panel.loading", "Reading SSH connections")
              : tm("ssh.panel.empty.title", "No SSH Connections")
          }
          description={tm(
            "ssh.panel.empty.help",
            "Add a global SSH profile and double-click it to connect in a terminal.",
          )}
          action={
            <HeroButton size="sm" variant="primary" onPress={() => startProfileEdit()}>
              {tm("ssh.profile.add", "Add SSH Connection")}
            </HeroButton>
          }
        />
      ) : (
        <div className="flex-1 overflow-y-auto scrollbar-overlay p-3 grid auto-rows-min gap-2">
          {profiles.map((profile) => (
            <SSHProfileRow
              key={profile.id}
              profile={profile}
              disabled={!project}
              labels={sshRowLabels}
              onConnect={() => void connectProfile(profile)}
              onCopy={() => undefined}
              onEdit={() => startProfileEdit(profile)}
              onDelete={() => void deleteProfile(profile)}
            />
          ))}
        </div>
      )}
      <SSHProfileDialog
        draft={draft}
        isSaving={isSaving}
        onChange={setDraft}
        onCancel={() => setDraft(null)}
        onPickPrivateKey={() => void pickPrivateKey()}
        onSubmit={() => {
          if (draft) void upsertProfile(draft);
        }}
      />
      {error && (
        <div className="mx-3 mb-3 rounded-md border border-brand-red/30 bg-brand-red/10 px-2.5 py-2 text-xs text-brand-red">
          {error}
        </div>
      )}
    </>
  );
}

type SSHProfileDraft = {
  id?: string;
  name: string;
  host: string;
  port: string;
  username: string;
  credentialKind: SSHCredentialKind;
  privateKeyPath: string;
  password: string;
  keyPassphrase: string;
};

type SSHProfileTestResult = {
  ok: boolean;
  message: string;
};

function SSHProfileDialog({
  draft,
  isSaving,
  onChange,
  onCancel,
  onPickPrivateKey,
  onSubmit,
}: {
  draft: SSHProfileDraft | null;
  isSaving: boolean;
  onChange: (draft: SSHProfileDraft | null) => void;
  onCancel: () => void;
  onPickPrivateKey: () => void;
  onSubmit: () => void;
}) {
  const validationError = draft ? sshDraftValidationError(draft) : null;
  const draftIdentity = draft ? (draft.id ?? "new") : "closed";
  const [showValidation, setShowValidation] = useState(false);
  const [isTesting, setTesting] = useState(false);
  const [testResult, setTestResult] = useState<SSHProfileTestResult | null>(null);
  const canSubmit = Boolean(draft) && !validationError && !isSaving;
  useEffect(() => {
    setShowValidation(false);
    setTestResult(null);
  }, [draftIdentity]);
  const set = <DraftKey extends keyof SSHProfileDraft>(key: DraftKey, value: SSHProfileDraft[DraftKey]) => {
    if (draft) {
      setShowValidation(false);
      setTestResult(null);
      onChange({ ...draft, [key]: value });
    }
  };
  const testConnection = async () => {
    if (!draft || isTesting || isSaving) return;
    if (validationError) {
      setShowValidation(true);
      setTestResult(null);
      return;
    }
    setTesting(true);
    setTestResult(null);
    try {
      setTestResult(
        await invoke<SSHProfileTestResult>("ssh_profile_test", {
          request: draftToSSHProfileRequest(draft),
        }),
      );
    } catch (error) {
      setTestResult({
        ok: false,
        message: error instanceof Error ? error.message : String(error),
      });
    } finally {
      setTesting(false);
    }
  };

  return (
    <Modal isOpen={Boolean(draft)} onOpenChange={(isOpen) => (!isOpen ? onCancel() : undefined)}>
      <Modal.Backdrop className="no-drag fixed inset-0 z-[9000] grid place-items-center bg-black/24 p-4 backdrop-blur-sm">
        <Modal.Container size="md" placement="center">
          <Modal.Dialog className="no-drag w-[min(520px,calc(100vw-32px))] rounded-[12px] border border-line-strong bg-surface-chrome p-4 text-ink shadow-pop outline-none">
            {draft && (
              <form
                noValidate
                className="grid gap-3"
                onSubmit={(event) => {
                  event.preventDefault();
                  if (validationError) {
                    setShowValidation(true);
                    setTestResult(null);
                    return;
                  }
                  if (canSubmit) onSubmit();
                }}
              >
                <Modal.Header className="p-0">
                  <div className="min-w-0">
                    <Modal.Heading className="text-sm font-semibold text-ink">
                      {draft.id
                        ? tm("ssh.profile.edit", "Edit SSH Connection")
                        : tm("ssh.profile.add", "Add SSH Connection")}
                    </Modal.Heading>
                    <p className="mt-1 text-sm leading-relaxed text-ink-faint">
                      {tm("ssh.profile.dialog.message", "Saved credentials are kept in Codux local app data.")}
                    </p>
                  </div>
                </Modal.Header>
                <div className="grid gap-2.5">
                  <SSHFormField label={tm("ssh.profile.name", "Name")}>
                    <TextInput
                      value={draft.name}
                      onChange={(event) => set("name", event.currentTarget.value)}
                      placeholder={tm("ssh.profile.name.placeholder", "Production Server")}
                      className="h-9 text-sm"
                    />
                  </SSHFormField>
                  <div className="grid grid-cols-[minmax(0,1fr)_96px] gap-2.5">
                    <SSHFormField label={tm("ssh.profile.host", "Host")} required>
                      <TextInput
                        value={draft.host}
                        onChange={(event) => set("host", event.currentTarget.value)}
                        className="h-9 text-sm"
                        required
                      />
                    </SSHFormField>
                    <SSHFormField label={tm("ssh.profile.port", "Port")}>
                      <TextInput
                        value={draft.port}
                        inputMode="numeric"
                        onChange={(event) => set("port", event.currentTarget.value.replace(/[^\d]/g, ""))}
                        className="h-9 text-sm"
                      />
                    </SSHFormField>
                  </div>
                  <SSHFormField label={tm("ssh.profile.username", "Username")} required>
                    <TextInput
                      value={draft.username}
                      onChange={(event) => set("username", event.currentTarget.value)}
                      className="h-9 text-sm"
                      required
                    />
                  </SSHFormField>
                  <SSHFormField label={tm("ssh.profile.credential", "Credential")}>
                    <Select
                      value={draft.credentialKind}
                      onChange={(value) => set("credentialKind", normalizeCredentialKind(value))}
                      options={[
                        { value: "none", label: tm("ssh.credential.none", "None / SSH Agent") },
                        { value: "password", label: tm("ssh.credential.password", "Password") },
                        { value: "privateKey", label: tm("ssh.credential.private_key", "Private Key") },
                      ]}
                      ariaLabel={tm("ssh.profile.credential", "Credential")}
                      className="w-full"
                    />
                  </SSHFormField>
                  {draft.credentialKind === "password" && (
                    <SSHFormField label={tm("ssh.profile.password", "Password")} required>
                      <TextInput
                        value={draft.password}
                        type="password"
                        onChange={(event) => set("password", event.currentTarget.value)}
                        placeholder={tm("ssh.profile.password.placeholder", "Stored locally")}
                        className="h-9 text-sm"
                      />
                    </SSHFormField>
                  )}
                  {draft.credentialKind === "privateKey" && (
                    <>
                      <SSHFormField label={tm("ssh.profile.private_key", "Private Key")} required>
                        <div className="flex gap-2">
                          <TextInput
                            value={draft.privateKeyPath}
                            onChange={(event) => set("privateKeyPath", event.currentTarget.value)}
                            className="h-9 text-sm"
                          />
                          <HeroButton
                            size="sm"
                            variant="secondary"
                            className="h-9 min-w-0 px-3 text-sm"
                            onPress={onPickPrivateKey}
                          >
                            {tm("common.choose", "Choose")}
                          </HeroButton>
                        </div>
                      </SSHFormField>
                      <SSHFormField label={tm("ssh.profile.key_passphrase", "Key Passphrase")}>
                        <TextInput
                          value={draft.keyPassphrase}
                          type="password"
                          onChange={(event) => set("keyPassphrase", event.currentTarget.value)}
                          placeholder={tm("ssh.profile.key_passphrase.placeholder", "Optional, stored locally")}
                          className="h-9 text-sm"
                        />
                      </SSHFormField>
                    </>
                  )}
                </div>
                {showValidation && validationError ? (
                  <div className="rounded-md border border-brand-red/25 bg-brand-red/10 px-2.5 py-2 text-sm text-brand-red">
                    {validationError}
                  </div>
                ) : null}
                {testResult ? (
                  <div
                    className={`rounded-md border px-2.5 py-2 text-sm ${
                      testResult.ok
                        ? "border-brand-green/25 bg-brand-green/10 text-brand-green"
                        : "border-brand-red/25 bg-brand-red/10 text-brand-red"
                    }`}
                  >
                    {testResult.ok
                      ? tm("ssh.profile.test.succeeded", "Connection test succeeded.")
                      : testResult.message}
                  </div>
                ) : null}
                <Modal.Footer className="flex justify-end gap-2 p-0 pt-1">
                  <HeroButton
                    size="sm"
                    variant="secondary"
                    className="mr-auto h-8 min-w-0 px-3 text-sm"
                    onPress={() => void testConnection()}
                    isDisabled={!draft || isTesting || isSaving}
                  >
                    {isTesting ? tm("ssh.profile.test.testing", "Testing...") : tm("ssh.profile.test", "Test")}
                  </HeroButton>
                  <HeroButton size="sm" variant="ghost" className="h-8 min-w-0 px-3 text-sm" onPress={onCancel}>
                    {tm("common.cancel", "Cancel")}
                  </HeroButton>
                  <HeroButton
                    size="sm"
                    variant="primary"
                    className="h-8 min-w-0 px-3 text-sm"
                    type="submit"
                    isDisabled={!draft || isSaving}
                  >
                    {isSaving ? tm("common.processing", "Processing") : tm("common.save", "Save")}
                  </HeroButton>
                </Modal.Footer>
              </form>
            )}
          </Modal.Dialog>
        </Modal.Container>
      </Modal.Backdrop>
    </Modal>
  );
}

function draftToSSHProfileRequest(draft: SSHProfileDraft) {
  const port = Math.max(1, Math.min(65535, Number(draft.port || 22) || 22));
  return {
    id: draft.id ?? null,
    name: draft.name.trim(),
    host: draft.host.trim(),
    port,
    username: draft.username.trim(),
    credentialKind: draft.credentialKind,
    privateKeyPath: draft.credentialKind === "privateKey" ? draft.privateKeyPath.trim() : "",
    password: draft.credentialKind === "password" ? draft.password.trim() : "",
    keyPassphrase: draft.credentialKind === "privateKey" ? draft.keyPassphrase.trim() : "",
  };
}

function SSHFormField({ label, required, children }: { label: ReactNode; required?: boolean; children: ReactNode }) {
  return (
    <label className="grid gap-1">
      <span className="text-sm font-medium text-ink-soft">
        {label}
        {required ? <span className="ml-0.5 text-brand-red">*</span> : null}
      </span>
      {children}
    </label>
  );
}

function SSHProfileRow({
  profile,
  disabled,
  labels,
  onConnect,
  onCopy,
  onEdit,
  onDelete,
}: {
  profile: SSHConnectionProfile;
  disabled: boolean;
  labels: SSHRowLabels;
  onConnect: () => void;
  onCopy: () => void;
  onEdit: () => void;
  onDelete: () => void;
}) {
  const contextMenu = useContextMenu();
  const tint =
    profile.credentialKind === "privateKey"
      ? "text-brand-blue bg-brand-blue/14"
      : profile.credentialKind === "password"
        ? "text-brand-amber bg-brand-amber/14"
        : "text-ink-mute bg-fill/[0.055]";
  return (
    <>
      <div
        className="grid grid-cols-[30px_minmax(0,1fr)] items-center gap-2.5 rounded-[8px] border border-line bg-fill/[0.035] p-2.5 transition-colors hover:bg-fill/[0.055]"
        onDoubleClick={() => {
          if (!disabled) onConnect();
        }}
        onContextMenu={contextMenu.openMenu}
      >
        <span className={`grid h-[30px] w-[30px] place-items-center rounded-[7px] ${tint}`}>
          {profile.credentialKind === "privateKey" ? <KeyRound size={13} /> : <Server size={13} />}
        </span>
        <div className="min-w-0">
          <div className="truncate text-sm font-semibold text-ink">{sshDisplayName(profile)}</div>
          <div className="truncate text-xs text-ink-faint">
            {profile.username}@{profile.host}:{profile.port}
          </div>
        </div>
      </div>
      <ContextMenu
        ariaLabel={`${sshDisplayName(profile)} ${labels.actions}`}
        menu={contextMenu.menu}
        onClose={contextMenu.closeMenu}
      >
        <ContextMenuItem disabled={disabled} label={labels.connect} onSelect={onConnect}>
          {labels.connect}
        </ContextMenuItem>
        <ContextMenuItem
          label={labels.copy}
          onSelect={() => {
            void navigator.clipboard?.writeText(sshCommandPreview(profile));
            onCopy();
          }}
        >
          {labels.copy}
        </ContextMenuItem>
        <ContextMenuItem label={labels.edit} onSelect={onEdit}>
          {labels.edit}
        </ContextMenuItem>
        <ContextMenuSeparator />
        <ContextMenuItem label={labels.remove} onSelect={onDelete}>
          {labels.remove}
        </ContextMenuItem>
      </ContextMenu>
    </>
  );
}

type SSHRowLabels = {
  connect: string;
  copy: string;
  edit: string;
  remove: string;
  actions: string;
};

function normalizeCredentialKind(value?: string): SSHCredentialKind {
  if (value === "password" || value === "privateKey") return value;
  return "none";
}

function sshDraftValidationError(draft: SSHProfileDraft) {
  if (!draft.host.trim()) return tm("ssh.profile.validation.host", "Host cannot be empty.");
  if (!draft.username.trim()) return tm("ssh.profile.validation.username", "Username cannot be empty.");
  const port = Number(draft.port || 22);
  if (!Number.isFinite(port) || port < 1 || port > 65535) {
    return tm("ssh.profile.validation.port", "Port must be between 1 and 65535.");
  }
  if (draft.credentialKind === "password" && !draft.password.trim()) {
    return tm("ssh.profile.validation.password", "Password cannot be empty.");
  }
  if (draft.credentialKind === "privateKey" && !draft.privateKeyPath.trim()) {
    return tm("ssh.profile.validation.private_key", "Private key path cannot be empty.");
  }
  return null;
}

function profileToDraft(profile?: SSHConnectionProfile): SSHProfileDraft {
  return {
    id: profile?.id,
    name: profile?.name ?? "",
    host: profile?.host ?? "",
    port: String(profile?.port ?? 22),
    username: profile?.username ?? "root",
    credentialKind: profile?.credentialKind ?? "none",
    privateKeyPath: profile?.privateKeyPath ?? "",
    password: profile?.password ?? "",
    keyPassphrase: profile?.keyPassphrase ?? "",
  };
}

function sshDisplayName(profile: SSHConnectionProfile) {
  return profile.name.trim() || `${profile.username}@${profile.host}`;
}

function sshCommandPreview(profile: SSHConnectionProfile) {
  const destination = `${profile.username}@${profile.host}`;
  return profile.port === 22 ? `ssh ${destination}` : `ssh -p ${profile.port} ${destination}`;
}
