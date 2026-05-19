import {
  ArrowDownToLine,
  ArrowUpFromLine,
  CheckCircle2,
  ChevronDown,
  ChevronRight,
  FileText,
  Folder,
  FolderPlus,
  KeyRound,
  Minus,
  MoreHorizontal,
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
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Button as HeroButton, Dropdown, ProgressBar, Spinner } from "@heroui/react";
import { useCallback, useEffect, useMemo, useRef, useState, type CSSProperties, type Key, type ReactNode } from "react";
import { useAIHistorySnapshot, type AIHeatmapDay, type AIHistorySessionSummary, type AITimeBucket, type AIUsageBreakdownItem } from "../ai/history";
import { aiIndexingPresentation, liveSessionTotalTokens } from "../ai/panelPresentation";
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
import { useGitStatusSnapshot, type GitCommitAction, type GitCommitSummary, type GitFileStatus, type GitStatusSnapshot } from "../git/status";
import { Button } from "./Button";
import { ContextMenu, ContextMenuItem, ContextMenuSeparator, useContextMenu } from "./ContextMenu";
import {
  DesktopMenu,
  DesktopMenuItem,
  DesktopMenuSectionLabel,
  DesktopMenuSeparator,
  DesktopSubmenu,
} from "./DesktopMenu";
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
import { formatI18n, localeFromSettings, tm } from "../i18n";

type Props = {
  panel: RightPanelKind;
  selectedProject?: WorkspaceProject;
};

export function Inspector({ panel, selectedProject }: Props) {
  return (
    <aside className="h-full min-w-0 flex flex-col bg-surface-chrome/35 backdrop-blur-md">
      {panel === "git" && <GitPanel project={selectedProject} />}
      {panel === "files" && <FilesPanel project={selectedProject} />}
      {panel === "ai" && <AIPanel project={selectedProject} />}
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
    <div className="sticky top-0 z-10 h-[34px] flex items-center justify-between border-b border-line/80 bg-fill/[0.055] px-3.5 text-xs text-ink-soft">
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
        {count != null && (
          <span className="min-w-4 text-right text-xs text-ink-faint tabular-nums">{count}</span>
        )}
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
  const canSubmit = input.value.trim().length > 0 && (input.secondaryLabel ? (input.secondaryValue ?? "").trim().length > 0 : true);
  const controlClass = "w-full rounded-md border border-line bg-surface-chrome/65 px-2 text-xs text-ink outline-none focus:border-brand-blue/60";

  return (
    <div className="mx-3 mt-3 rounded-[10px] border border-line bg-fill/[0.04] p-3">
      <div className="text-xs font-semibold text-ink">{input.title}</div>
      {input.message ? (
        <div className="mt-1 text-[11px] leading-relaxed text-ink-faint">{input.message}</div>
      ) : null}
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
  const [selectedFileId, setSelectedFileId] = useState("");
  const [selectedCommitHash, setSelectedCommitHash] = useState("");
  const [gitInput, setGitInput] = useState<GitInputState | null>(null);
  const [branchMenuOpen, setBranchMenuOpen] = useState(false);
  const [commitMenuOpen, setCommitMenuOpen] = useState(false);
  const git = useGitStatusSnapshot(project);
  const [isManualRefreshing, refreshGit] = useRefreshFeedback(git.refresh);
  const isRefreshingGit = git.isLoading || isManualRefreshing;
  const snapshot = git.snapshot;
  const hasUpstream = Boolean(snapshot.upstream);
  const totalChanges =
    snapshot.staged.length + snapshot.unstaged.length + snapshot.untracked.length;
  const canCommit = snapshot.staged.length > 0 && commitMessage.trim().length > 0;
  const statusLabel = !snapshot.isRepository
    ? tm("git.repository.not_repository", "Current project is not a Git repository.")
    : hasUpstream
      ? snapshot.behind === 0 && snapshot.ahead === 0
        ? tm("git.remote.status.synced", "Remote Is Synced")
        : tm("git.remote.status.has_updates", "Remote Has Updates")
      : tm("git.remote.status.no_remote_branch", "No Remote Branch");
  const statusTone = snapshot.isRepository && hasUpstream ? "info" : "neutral";
  const statusButtonTone = statusTone === "info" ? "ghost" : "neutral";
  const StatusIcon = snapshot.isRepository && hasUpstream && snapshot.behind === 0 && snapshot.ahead === 0
    ? CheckCircle2
    : ChevronRight;
  const commitActionLabel = gitCommitActionLabel(commitAction);
  const remoteBranchesByRemote = useMemo(() => {
    return groupRemoteBranches(snapshot.remoteBranches);
  }, [snapshot.remoteBranches]);
  const localMergeCandidates = useMemo(
    () => snapshot.branches.filter((branch) => branch.name !== snapshot.branch),
    [snapshot.branch, snapshot.branches],
  );
  const remotePushBranchesByRemote = useMemo(() => {
    const upstream = snapshot.upstream;
    const targets = [...snapshot.remoteBranches];
    if (upstream && !targets.includes(upstream)) {
      targets.unshift(upstream);
    }
    return groupRemoteBranches(targets, upstream);
  }, [snapshot.remoteBranches, snapshot.upstream]);
  useEffect(() => {
    previousGitDirectoryPathsRef.current = {
      staged: new Set(),
      unstaged: new Set(),
      untracked: new Set(),
    };
    setSelectedCommitHash("");
    setExpandedGitFilePaths({
      staged: new Set(),
      unstaged: new Set(),
      untracked: new Set(),
    });
  }, [project?.path]);
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
    const seed = `task/${new Date().toISOString().slice(0, 10)}`;
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
        if (await systemConfirm(formatI18n(tm("git.remote.remove.confirm_format", "Remove remote %@?"), name), {
          title: tm("git.remote.remove", "Remove Remote"),
          kind: "warning",
          okLabel: tm("common.delete", "Delete"),
          cancelLabel: tm("common.cancel", "Cancel"),
        })) await git.removeRemote(name);
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
      const force = await systemConfirm(formatI18n(tm("git.branch.delete.confirm_format", "Delete local branch %@?"), branch), {
        title: tm("git.branch.delete_local", "Delete Local Branch"),
        kind: "warning",
        okLabel: tm("common.delete", "Delete"),
        cancelLabel: tm("common.cancel", "Cancel"),
      });
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

  const selectGitFile = (file: GitFileStatus, staged: boolean) => {
    setSelectedFileId(gitFileSelectionId(file, staged));
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
    await git.commitAction(message, commitAction);
    setCommitMessage("");
  };
  const runCommitAction = async (commit: GitCommitSummary, key: Key) => {
    switch (String(key)) {
      case "copy":
        await navigator.clipboard?.writeText(commit.hash);
        return;
      case "checkout":
        if (await systemConfirm(formatI18n(tm("git.history.checkout.message_format", "Check out commit %@?"), commit.hash.slice(0, 7)), {
          title: tm("git.history.checkout_commit", "Check Out This Commit"),
          kind: "warning",
          okLabel: tm("git.history.checkout_commit", "Check Out"),
          cancelLabel: tm("common.cancel", "Cancel"),
        })) await git.checkoutCommit(commit.hash);
        return;
      case "branch":
        createBranchFromCommit(commit);
        return;
      case "undo": {
        const pushed = await git.headCommitPushed();
        if (pushed && !(await systemConfirm(tm("git.history.undo_last_commit.remote_notice", "The last commit may already be pushed. Continue?"), {
          title: tm("git.history.undo_last_commit", "Undo Last Commit"),
          kind: "warning",
          okLabel: tm("common.continue", "Continue"),
          cancelLabel: tm("common.cancel", "Cancel"),
        }))) return;
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
            if (pushed && !(await systemConfirm(tm("git.commit.edit_last_message.remote_notice", "The last commit may already be pushed. Continue?"), {
              title: tm("git.history.edit_last_commit_message", "Edit Last Commit Message"),
              kind: "warning",
              okLabel: tm("common.continue", "Continue"),
              cancelLabel: tm("common.cancel", "Cancel"),
            }))) return;
            await git.amendLastCommitMessage(message);
          },
        });
        return;
      }
      case "revert":
        if (await systemConfirm(formatI18n(tm("git.history.revert.message_format", "Revert commit %@?"), commit.hash.slice(0, 7)), {
          title: tm("git.history.revert_commit", "Revert This Commit"),
          kind: "warning",
          okLabel: tm("git.history.revert_commit", "Revert"),
          cancelLabel: tm("common.cancel", "Cancel"),
        })) await git.revertCommit(commit.hash);
        return;
      case "restoreLocal":
        if (await systemConfirm(formatI18n(tm("git.history.restore_local.message_format", "Reset the current branch locally to %@?"), commit.hash.slice(0, 7)), {
          title: tm("git.history.restore_local", "Restore Locally"),
          kind: "warning",
          okLabel: tm("git.history.restore_local.action", "Restore Locally"),
          cancelLabel: tm("common.cancel", "Cancel"),
        })) await git.restoreCommit(commit.hash, false);
        return;
      case "restoreRemote":
        if (await systemConfirm(formatI18n(tm("git.history.restore_remote.message_format", "Reset the current branch and remote to %@?"), commit.hash.slice(0, 7)), {
          title: tm("git.history.restore_remote", "Restore Remote"),
          kind: "warning",
          okLabel: tm("git.history.restore_remote.action", "Restore Remote"),
          cancelLabel: tm("common.cancel", "Cancel"),
        })) await git.restoreCommit(commit.hash, true);
        return;
    }
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
              <button type="button" className="inline-flex items-center gap-1.5 text-sm font-semibold hover:text-ink/90">
              <span className="truncate">{snapshot.branch || project?.branch || "master"}</span>
              <ChevronDown size={12} className="flex-shrink-0 text-ink-mute" />
              </button>
            }
          >
                <DesktopMenuItem label={tm("git.branch.create_and_switch", "New Branch")} onSelect={() => void runBranchAction("create")}>{tm("git.branch.create_and_switch", "New Branch")}</DesktopMenuItem>
                {snapshot.branches.length > 0 && (
                  <>
                    <DesktopMenuSeparator />
                    <DesktopMenuSectionLabel>{tm("git.branch.local", "Local Branches")}</DesktopMenuSectionLabel>
                    {snapshot.branches.map((branch) => (
                      <DesktopMenuItem key={`checkoutLocal:${branch.name}`} label={branch.name} onSelect={() => void runBranchAction(`checkoutLocal:${branch.name}`)}>
                        <span className="inline-flex min-w-0 items-center gap-2">
                          <span className={`h-1.5 w-1.5 rounded-full ${branch.isCurrent ? "bg-brand-blue" : "bg-ink-faint/55"}`} />
                          <span className="truncate">{branch.name}</span>
                        </span>
                      </DesktopMenuItem>
                    ))}
                  </>
                )}
                {remoteBranchesByRemote.length > 0 && (
                  <>
                    <DesktopMenuSeparator />
                    <DesktopMenuSectionLabel>{tm("git.remote.branches", "Remote Branches")}</DesktopMenuSectionLabel>
                    {remoteBranchesByRemote.map(({ remote, branches }) => (
                      <DesktopSubmenu key={remote} label={remote}>
                            {branches.map((branch) => (
                              <DesktopMenuItem
                                key={`checkoutRemote:${remote}/${branch.name}`}
                                label={branch.name}
                                onSelect={() => void runBranchAction(`checkoutRemote:${remote}/${branch.name}`)}
                              >
                                {branch.name}
                              </DesktopMenuItem>
                            ))}
                      </DesktopSubmenu>
                    ))}
                  </>
                )}
                {localMergeCandidates.length > 0 && (
                  <>
                    <DesktopMenuSeparator />
                    <DesktopSubmenu label={tm("git.branch.merge.title", "Merge Branch")}>
                          {localMergeCandidates.map((branch) => (
                            <DesktopMenuItem key={`mergeLocal:${branch.name}`} label={branch.name} onSelect={() => void runBranchAction(`mergeLocal:${branch.name}`)}>
                              {branch.name}
                            </DesktopMenuItem>
                          ))}
                    </DesktopSubmenu>
                    <DesktopSubmenu label={tm("git.branch.squash_merge", "Squash Merge Branch")}>
                          {localMergeCandidates.map((branch) => (
                            <DesktopMenuItem key={`squashLocal:${branch.name}`} label={branch.name} onSelect={() => void runBranchAction(`squashLocal:${branch.name}`)}>
                              {branch.name}
                            </DesktopMenuItem>
                          ))}
                    </DesktopSubmenu>
                    <DesktopSubmenu label={tm("git.branch.delete_local", "Delete Local Branch")}>
                          {localMergeCandidates.map((branch) => (
                            <DesktopMenuItem key={`deleteLocal:${branch.name}`} label={branch.name} onSelect={() => void runBranchAction(`deleteLocal:${branch.name}`)}>
                              {branch.name}
                            </DesktopMenuItem>
                          ))}
                    </DesktopSubmenu>
                  </>
                )}
                <DesktopMenuSeparator />
                <DesktopMenuItem label={tm("git.remote.fetch", "Fetch")} onSelect={() => void runBranchAction("fetch")}>{tm("git.remote.fetch", "Fetch")}</DesktopMenuItem>
                {remotePushBranchesByRemote.length > 0 && (
                  <>
                    <DesktopSubmenu label={tm("git.remote.branch.push_here", "Push to Remote Branch")}>
                          {remotePushBranchesByRemote.map(({ remote, branches }) => (
                            <DesktopSubmenu key={remote} label={remote}>
                                  {branches.map((branch) => (
                                    <DesktopMenuItem
                                      key={`pushRemoteBranch:${remote}/${branch.name}`}
                                      label={branch.name}
                                      onSelect={() => void runBranchAction(`pushRemoteBranch:${remote}/${branch.name}`)}
                                    >
                                      <span className="inline-flex min-w-0 items-center gap-2">
                                        <span className={`h-1.5 w-1.5 rounded-full ${branch.isUpstream ? "bg-brand-blue" : "bg-ink-faint/55"}`} />
                                        <span className="truncate">{branch.name}</span>
                                      </span>
                                    </DesktopMenuItem>
                                  ))}
                            </DesktopSubmenu>
                          ))}
                    </DesktopSubmenu>
                  </>
                )}
                {snapshot.remotes.map((remote) => (
                  <DesktopMenuItem key={`pushRemote:${remote.name}`} label={remote.name} onSelect={() => void runBranchAction(`pushRemote:${remote.name}`)}>
                    {formatI18n(tm("git.remote.push_to_format", "Push to %@"), remote.name)}
                  </DesktopMenuItem>
                ))}
                {snapshot.remotes.length === 0 && <DesktopMenuItem label={tm("git.remote.push_set_upstream", "Push and Set Remote")} onSelect={() => void runBranchAction("pushRemote")}>{tm("git.remote.push_set_upstream", "Push and Set Remote")}</DesktopMenuItem>}
                <DesktopMenuItem label={tm("git.remote.add", "Add Remote")} onSelect={() => void runBranchAction("addRemote")}>{tm("git.remote.add", "Add Remote")}</DesktopMenuItem>
                <DesktopMenuItem label={tm("git.remote.remove", "Remove Remote")} onSelect={() => void runBranchAction("removeRemote")}>{tm("git.remote.remove", "Remove Remote")}</DesktopMenuItem>
          </DesktopMenu>
        }
        trailing={
          <>
            <PanelIconButton
              icon={Sparkles}
              tooltip={tm("git.commit.generate_message", "Generate Commit Message")}
              onClick={() => setCommitMessage(generateCommitMessage(snapshot))}
            />
            <PanelIconButton
              icon={RefreshCw}
              tooltip={isRefreshingGit ? tm("git.empty.reading_status", "Reading Git Status") : tm("git.status.refresh", "Refresh Git Status")}
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
          title={tm("git.empty.not_repository", "Current Directory Is Not a Git Repository")}
          description={git.error ?? snapshot.error ?? tm("git.empty.description", "Initialize a repository or clone a remote repository to view commits, diffs, and branches here.")}
          tone="warning"
          action={
            <div className="flex items-center gap-2">
              <HeroButton size="sm" variant="primary" onPress={() => void git.init()}>
                {tm("git.empty.initialize_repository", "Initialize Repository")}
              </HeroButton>
              <HeroButton
                size="sm"
                variant="secondary"
                onPress={() => openGitInput({
                  title: tm("git.empty.clone_remote_repository", "Clone Remote Repository"),
                  label: tm("git.remote.add.url_message", "Remote URL"),
                  value: "",
                  onSubmit: async (remoteUrl) => {
                    await git.cloneRepository(remoteUrl);
                  },
                })}
              >
                {tm("git.empty.clone_remote_repository", "Clone Remote Repository")}
              </HeroButton>
            </div>
          }
        />
      ) : (
        <div className="min-h-0 flex-1 flex flex-col">
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
                {commitActionLabel}{snapshot.staged.length > 0 ? ` ${snapshot.staged.length}` : ""}
              </Button>
              <Dropdown isOpen={commitMenuOpen} onOpenChange={setCommitMenuOpen}>
                <Dropdown.Trigger
                  isDisabled={!canCommit}
                  className="grid h-[34px] w-8 min-w-8 place-items-center rounded-l-none rounded-r-lg bg-brand-blue px-0 text-on-brand transition-colors hover:bg-brand-blue/90 disabled:cursor-default disabled:opacity-50"
                  aria-label={tm("git.commit.options", "Commit Options")}
                >
                  <ChevronDown size={13} strokeWidth={2.4} />
                </Dropdown.Trigger>
                <Dropdown.Popover placement="bottom end" className="min-w-[184px] rounded-[10px] border border-line-strong bg-surface-chrome p-1 shadow-pop backdrop-blur-2xl">
                  <Dropdown.Menu
                    aria-label={tm("git.commit.options", "Commit Options")}
                    onAction={(key) => setCommitAction(String(key) as GitCommitAction)}
                    className="grid gap-0.5"
                  >
                    <Dropdown.Item id="commit" className="menu-item">{tm("git.commit.action", "Commit")}</Dropdown.Item>
                    <Dropdown.Item id="commitAndPush" className="menu-item">{tm("git.commit.action_push", "Commit and Push")}</Dropdown.Item>
                    <Dropdown.Item id="commitAndSync" className="menu-item">{tm("git.commit.action_sync", "Commit and Sync")}</Dropdown.Item>
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
                  label={tm("git.files.unstage_all", "Unstage All")}
                  disabled={snapshot.staged.length === 0}
                  onPress={() => void git.unstage(snapshot.staged.map((file) => file.path))}
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
                selectedId={selectedFileId}
                primaryLabel={tm("git.files.unstage", "Unstage")}
                onPrimary={(file) => void git.unstage([file.path])}
                onSelect={(file) => selectGitFile(file, true)}
                onOpenDiff={(file) => void previewDiff(file, true)}
                onToggleDirectory={(path) => toggleGitDirectory("staged", path)}
                onDiscard={(file) => {
                  void systemConfirm(formatI18n(tm("git.files.discard.confirm_format", "Discard changes in %@?"), file.path), {
                    title: tm("git.files.discard_changes", "Discard Changes"),
                    kind: "warning",
                    okLabel: tm("git.files.discard_changes", "Discard"),
                    cancelLabel: tm("common.cancel", "Cancel"),
                  }).then((confirmed) => {
                    if (confirmed) void git.discard([file.path]);
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
                    label={tm("git.files.stage_all", "Stage All")}
                    disabled={snapshot.unstaged.length === 0}
                    onPress={() => void git.stage(snapshot.unstaged.map((file) => file.path))}
                  />
                  <HeaderActionButton
                    icon={Undo2}
                    label={tm("git.files.discard_all", "Discard All")}
                    disabled={snapshot.unstaged.length === 0}
                    onPress={() => {
                      void systemConfirm(tm("git.files.discard_all.confirm", "Discard all worktree changes?"), {
                        title: tm("git.files.discard_all", "Discard All"),
                        kind: "warning",
                        okLabel: tm("git.files.discard_all", "Discard All"),
                        cancelLabel: tm("common.cancel", "Cancel"),
                      }).then((confirmed) => {
                        if (confirmed) void git.discard(snapshot.unstaged.map((file) => file.path));
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
                selectedId={selectedFileId}
                primaryLabel={tm("git.files.stage", "Stage")}
                onPrimary={(file) => void git.stage([file.path])}
                onSelect={(file) => selectGitFile(file, false)}
                onOpenDiff={(file) => void previewDiff(file, false)}
                onToggleDirectory={(path) => toggleGitDirectory("unstaged", path)}
                onDiscard={(file) => {
                  void systemConfirm(formatI18n(tm("git.files.discard.confirm_format", "Discard changes in %@?"), file.path), {
                    title: tm("git.files.discard_changes", "Discard Changes"),
                    kind: "warning",
                    okLabel: tm("git.files.discard_changes", "Discard"),
                    cancelLabel: tm("common.cancel", "Cancel"),
                  }).then((confirmed) => {
                    if (confirmed) void git.discard([file.path]);
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
                    label={tm("git.files.stage_all", "Stage All")}
                    disabled={snapshot.untracked.length === 0}
                    onPress={() => void git.stage(snapshot.untracked.map((file) => file.path))}
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
                selectedId={selectedFileId}
                primaryLabel={tm("git.files.stage", "Stage")}
                onPrimary={(file) => void git.stage([file.path])}
                onSelect={(file) => selectGitFile(file, false)}
                onOpenDiff={(file) => void previewDiff(file, false)}
                onToggleDirectory={(path) => toggleGitDirectory("untracked", path)}
                onIgnore={(file) => void git.appendGitignore([file.path])}
                onDiscard={(file) => {
                  void systemConfirm(formatI18n(tm("git.files.delete_untracked.confirm_format", "Delete untracked file %@?"), file.path), {
                    title: tm("git.files.delete_file", "Delete File"),
                    kind: "warning",
                    okLabel: tm("common.delete", "Delete"),
                    cancelLabel: tm("common.cancel", "Cancel"),
                  }).then((confirmed) => {
                    if (confirmed) void git.discard([file.path]);
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

          <div className="h-[190px] flex-shrink-0 border-t border-line/80 bg-surface-chrome/25">
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
                    {git.isLoading ? tm("git.empty.reading_status", "Reading Git Status") : tm("git.history.empty", "No Commit History")}
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
            ) : (
              <StatusIcon size={12} className={snapshot.isRepository && hasUpstream ? "opacity-90" : "opacity-0"} />
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
            <PanelButton tone={statusButtonTone} leading={ArrowDownToLine} onClick={hasUpstream ? () => void git.pull() : undefined}>
              {tm("git.remote.pull", "Pull")}{snapshot.behind > 0 ? ` ${snapshot.behind}` : ""}
            </PanelButton>
            <PanelButton
              tone={statusButtonTone}
              leading={ArrowUpFromLine}
              onClick={hasUpstream ? () => void git.push() : () => void pushRemote()}
            >
              {tm("git.remote.push", "Push")}{snapshot.ahead > 0 ? ` ${snapshot.ahead}` : ""}
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
  depth = 0,
  onSelect,
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
  onSelect?: () => void;
  onOpenDiff?: () => void;
  onPrimary?: () => void;
  primaryLabel?: string;
  onDiscard?: () => void;
  onIgnore?: () => void;
}) {
  const contextMenu = useContextMenu();
  const toneClass =
    tone === "amber"
      ? "text-brand-amber"
      : tone === "green"
        ? "text-brand-green"
        : "text-brand-blue";
  return (
    <Tooltip label={path} placement="left" triggerClassName="block w-full">
      <div
        onContextMenu={contextMenu.openMenu}
        className={`group relative w-full h-[28px] pr-3 flex items-center gap-1.5 transition-colors text-xs text-ink-soft ${
          selected ? "bg-brand-blue/12 text-ink" : "hover:bg-fill/[0.04]"
        }`}
        style={{ paddingLeft: `${12 + depth * 14}px` }}
      >
        <PressableButton
          className="min-w-0 flex-1 h-full flex items-center gap-2 text-left"
          onPressUp={onSelect}
          onDoubleClick={onOpenDiff}
        >
          <span className="w-[13px] flex-shrink-0" />
          <FileText size={13} className="flex-shrink-0 text-ink-mute" />
          <span className="truncate flex-1 text-right" dir="rtl">{displayName ?? path}</span>
          <span className={`flex-shrink-0 text-xs font-bold ${toneClass}`}>{tag}</span>
        </PressableButton>
        <ContextMenu
          ariaLabel={formatI18n(tm("git.files.actions_format", "%@ Actions"), path)}
          menu={contextMenu.menu}
          onClose={contextMenu.closeMenu}
        >
          <ContextMenuItem label={tm("git.files.copy_path", "Copy Path")} onSelect={() => void navigator.clipboard?.writeText(path)}>
            {tm("git.files.copy_path", "Copy Path")}
          </ContextMenuItem>
          {rootPath && (
            <ContextMenuItem label={tm("git.files.show_in_finder", "Show in Finder")} onSelect={() => void revealFile(rootPath, path)}>
              {tm("git.files.show_in_finder", "Show in Finder")}
            </ContextMenuItem>
          )}
          <ContextMenuSeparator />
          {onPrimary && primaryLabel && (
            <ContextMenuItem label={primaryLabel} onSelect={onPrimary}>{primaryLabel}</ContextMenuItem>
          )}
          <ContextMenuItem label={tm("git.diff.open", "Open Diff")} onSelect={onOpenDiff}>
            {tm("git.diff.open", "Open Diff")}
          </ContextMenuItem>
          {onIgnore && <ContextMenuItem label={tm("git.ignore.add", "Add to .gitignore")} onSelect={onIgnore}>{tm("git.ignore.add", "Add to .gitignore")}</ContextMenuItem>}
          {onDiscard && <ContextMenuItem label={tm("git.files.discard_or_delete", "Discard / Delete")} onSelect={onDiscard}>{tm("git.files.discard_or_delete", "Discard / Delete")}</ContextMenuItem>}
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
  selectedId,
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
  selectedId?: string;
  primaryLabel?: string;
  onSelect?: (file: GitFileStatus) => void;
  onOpenDiff?: (file: GitFileStatus) => void;
  onToggleDirectory: (path: string) => void;
  onPrimary?: (file: GitFileStatus) => void;
  onDiscard?: (file: GitFileStatus) => void;
  onIgnore?: (file: GitFileStatus) => void;
}) {
  const tree = useMemo(() => buildGitFileTree(files), [files]);
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
          selectedId={selectedId}
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

function GitFileTreeRow({
  node,
  depth,
  sectionKind,
  expandedPaths,
  rootPath,
  selectedId,
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
  selectedId?: string;
  primaryLabel?: string;
  onToggleDirectory: (path: string) => void;
  onSelect?: (file: GitFileStatus) => void;
  onOpenDiff?: (file: GitFileStatus) => void;
  onPrimary?: (file: GitFileStatus) => void;
  onDiscard?: (file: GitFileStatus) => void;
  onIgnore?: (file: GitFileStatus) => void;
}) {
  if (node.kind === "directory") {
    const expanded = expandedPaths.has(node.path);
    return (
      <>
        <GitDirectoryRow node={node} depth={depth} expanded={expanded} onToggle={() => onToggleDirectory(node.path)} />
        {expanded && node.children.map((child) => (
          <GitFileTreeRow
            key={`${child.kind}:${child.path}`}
            node={child}
            depth={depth + 1}
            sectionKind={sectionKind}
            expandedPaths={expandedPaths}
            rootPath={rootPath}
            selectedId={selectedId}
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
  const id = `${sectionKind}:${node.file.path}`;
  return (
    <FileRow
      path={node.file.path}
      displayName={node.name}
      tag={meta.tag}
      tone={meta.tone}
      depth={depth}
      rootPath={rootPath}
      selected={selectedId === id}
      primaryLabel={primaryLabel}
      onSelect={() => onSelect?.(node.file)}
      onOpenDiff={() => onOpenDiff?.(node.file)}
      onPrimary={onPrimary ? () => onPrimary(node.file) : undefined}
      onDiscard={onDiscard ? () => onDiscard(node.file) : undefined}
      onIgnore={onIgnore ? () => onIgnore(node.file) : undefined}
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
        {expanded ? <ChevronDown size={12} className="text-ink-faint" /> : <ChevronRight size={12} className="text-ink-faint" />}
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
  return decorations.slice(0, 1).map((decoration) =>
    decoration
      .replace(/^HEAD -> /, "HEAD→")
      .replace(/^origin\//, "o/"),
  );
}

function generateCommitMessage(snapshot: GitStatusSnapshot) {
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
              <div className="text-ink-mute">{commit.author} · {commit.relativeTime}</div>
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
        <ContextMenuItem label={tm("git.history.copy_commit_hash", "Copy Commit Hash")} onSelect={() => onAction("copy")}>
          {tm("git.history.copy_commit_hash", "Copy Commit Hash")}
        </ContextMenuItem>
        <ContextMenuItem label={tm("git.history.checkout_commit", "Check Out This Commit")} onSelect={() => onAction("checkout")}>{tm("git.history.checkout_commit", "Check Out This Commit")}</ContextMenuItem>
        <ContextMenuItem label={tm("git.history.create_branch_from_commit", "Create Branch from This Commit")} onSelect={() => onAction("branch")}>{tm("git.history.create_branch_from_commit", "Create Branch from This Commit")}</ContextMenuItem>
        {isHead && <ContextMenuItem label={tm("git.history.undo_last_commit", "Undo Last Commit")} onSelect={() => onAction("undo")}>{tm("git.history.undo_last_commit", "Undo Last Commit")}</ContextMenuItem>}
        {isHead && <ContextMenuItem label={tm("git.history.edit_last_commit_message", "Edit Last Commit Message")} onSelect={() => onAction("amend")}>{tm("git.history.edit_last_commit_message", "Edit Last Commit Message")}</ContextMenuItem>}
        <ContextMenuSeparator />
        <ContextMenuItem label={tm("git.history.revert_commit", "Revert This Commit")} onSelect={() => onAction("revert")}>{tm("git.history.revert_commit", "Revert This Commit")}</ContextMenuItem>
        <ContextMenuItem label={tm("git.history.restore_local", "Restore Locally")} onSelect={() => onAction("restoreLocal")}>{tm("git.history.restore_local", "Restore Locally")}</ContextMenuItem>
        <ContextMenuItem label={tm("git.history.restore_remote", "Restore Remote")} onSelect={() => onAction("restoreRemote")}>{tm("git.history.restore_remote", "Restore Remote")}</ContextMenuItem>
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

function GitGraphToken({
  char,
  index,
  centerX,
}: {
  char: string;
  index: number;
  centerX: number;
}) {
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
  const [fileStatus, setFileStatus] = useState<{ tone: "neutral" | "success" | "warning" | "danger"; message: string }>({
    tone: "neutral",
    message: tm("files.panel.status.ready", "Ready"),
  });
  const [isDraggingExternalFiles, setDraggingExternalFiles] = useState(false);
  const [loadingPaths, setLoadingPaths] = useState<Set<string>>(new Set());
  const [error, setError] = useState<string | null>(null);
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

  const updateStatus = useCallback((message: string, tone: "neutral" | "success" | "warning" | "danger" = "neutral") => {
    setFileStatus({ tone, message });
  }, []);

  const handleFileError = useCallback((nextError: unknown) => {
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    setError(message);
    updateStatus(message, "danger");
  }, [updateStatus]);

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
      setError(null);
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
    const stored = fileTreeStateRef.current.get(rootPath);
    setChildrenByPath({});
    setSelectedPath(stored?.selectedPath ?? "");
    setSelectedPaths(stored?.selectedPath ? new Set([stored.selectedPath]) : new Set());
    setSelectionAnchorPath(stored?.selectedPath ?? "");
    setPendingDeletePaths([]);
    setCopiedPath("");
    setError(null);
    if (!rootPath) {
      setExpandedPaths(new Set());
      updateStatus(tm("files.panel.no_project", "No Project Selected"));
      return;
    }
    const nextExpanded = stored?.expandedPaths.size ? new Set(stored.expandedPaths) : new Set([rootPath]);
    setExpandedPaths(nextExpanded);
    updateStatus(tm("files.panel.status.ready", "Ready"));
    void Promise.all([
      loadChildren(),
      ...Array.from(nextExpanded)
        .filter((path) => path !== rootPath)
        .map((path) => loadChildren(path)),
    ]);
  }, [loadChildren, rootPath, updateStatus]);

  const rows = useMemo(
    () => flattenFileRows(rootPath, childrenByPath, expandedPaths),
    [childrenByPath, expandedPaths, rootPath],
  );
  const fileTreeLabels = useMemo<FileTreeLabels>(() => ({
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
  }), []);
  const selectedEntry = useMemo(
    () => rows.find((row) => row.entry.path === selectedPath)?.entry,
    [rows, selectedPath],
  );
  const selectedEntries = useMemo(
    () => rows.filter((row) => selectedPaths.has(row.entry.path)).map((row) => row.entry),
    [rows, selectedPaths],
  );
  const pendingDeleteEntries = useMemo(
    () => rows.filter((row) => pendingDeletePaths.includes(row.entry.path)).map((row) => row.entry),
    [pendingDeletePaths, rows],
  );
  const pendingDeleteMessage = useMemo(
    () => formatI18n(tm("files.panel.delete.pending_count_format", "%d item(s) marked for delete"), pendingDeletePaths.length),
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
        setError(null);
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
        setError(null);
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
      setError(null);
      setInlineEdit(null);
      updateStatus(formatI18n(tm("files.panel.status.created_format", "Created %@"), entry.name), "success");
      await loadChildren(inlineEdit.parentPath);
    } catch (nextError) {
      handleFileError(nextError);
    }
  };

  const renameEntry = async (entry?: FileEntry) => {
    const target = entry ?? selectedEntry;
    if (!rootPath || !target) return;
    setInlineEdit({
      mode: "rename",
      entry: target,
      parentPath: parentPath(target.path, rootPath),
      value: target.name,
    });
  };

  const stageDeleteEntries = (entries: FileEntry[]) => {
    if (!rootPath || entries.length === 0) return;
    setPendingDeletePaths(entries.map((entry) => entry.path));
  };

  const activateFilePanel = () => {
    filePanelActiveRef.current = true;
    fileTreeRef.current?.focus({ preventScroll: true });
  };

  const scrollFileRowIntoView = (path: string) => {
    window.requestAnimationFrame(() => {
      const row = fileTreeRef.current?.querySelector<HTMLElement>(`[data-file-path="${cssEscape(path)}"]`);
      row?.scrollIntoView({ block: "nearest" });
    });
  };

  const selectEntryRange = (entry: FileEntry, extend: boolean) => {
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
  };

  const entriesForContextAction = (entry: FileEntry) => (
    selectedPaths.has(entry.path) && selectedEntries.length > 1 ? selectedEntries : [entry]
  );

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

  const copyEntryPaths = (entries: FileEntry[]) => {
    if (entries.length === 0) return;
    void navigator.clipboard?.writeText(entries.map((entry) => entry.path).join("\n"));
    updateStatus(
      entries.length === 1
        ? formatI18n(tm("files.panel.status.copied_format", "Copied %@"), entries[0].name)
        : formatI18n(tm("files.panel.status.copied_paths_count_format", "Copied %d paths"), entries.length),
      "success",
    );
  };

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
      setError(null);
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

  const pasteCopiedPath = async () => {
    if (!rootPath || !copiedPath) return;
    try {
      const entry = await copyFile(rootPath, copiedPath, targetDirectory);
      setExpandedPaths((current) => new Set(current).add(targetDirectory));
      setSelectedPath(entry.path);
      setSelectedPaths(new Set([entry.path]));
      setSelectionAnchorPath(entry.path);
      setError(null);
      updateStatus(formatI18n(tm("files.panel.status.pasted_format", "Pasted %@"), entry.name), "success");
      await loadChildren(targetDirectory);
    } catch (nextError) {
      handleFileError(nextError);
    }
  };

  const importFilesIntoTarget = useCallback(
    async (paths: string[], targetDirectoryPath = targetDirectory) => {
      if (!rootPath || paths.length === 0) return;
      try {
        const imported = await importExternalFiles(rootPath, paths, targetDirectoryPath);
        setExpandedPaths((current) => new Set(current).add(targetDirectoryPath));
        setSelectedPath(imported[0]?.path ?? "");
        setSelectedPaths(imported[0]?.path ? new Set([imported[0].path]) : new Set());
        setSelectionAnchorPath(imported[0]?.path ?? "");
        setError(null);
        updateStatus(formatI18n(tm("files.panel.status.imported_count_format", "Imported %d item(s)"), imported.length), "success");
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
    void getCurrentWindow().onDragDropEvent((event) => {
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
    }).then((nextUnlisten) => {
      if (disposed) {
        nextUnlisten();
      } else {
        unlisten = nextUnlisten;
      }
    }).catch((nextError) => {
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

  const selectEntry = (entry: FileEntry, options?: { extend?: boolean; toggle?: boolean }) => {
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
        setSelectedPath(next.has(entry.path) ? entry.path : nextPaths[nextPaths.length - 1] ?? entry.path);
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
  };

  const openEntry = (entry: FileEntry) => {
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
  };

  const handleFileShortcutKeyDown = useCallback((event: KeyboardEvent) => {
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
    if (visibleRows.length > 0 && (event.key === "ArrowUp" || event.key === "ArrowDown" || event.key === "Home" || event.key === "End")) {
      event.preventDefault();
      const selectedIndex = visibleRows.findIndex((row) => row.entry.path === selectedPathRef.current);
      const currentIndex = selectedIndex >= 0 ? selectedIndex : event.key === "ArrowUp" ? visibleRows.length : -1;
      const nextIndex =
        event.key === "Home"
          ? 0
          : event.key === "End"
            ? visibleRows.length - 1
            : Math.min(
                visibleRows.length - 1,
                Math.max(0, currentIndex + (event.key === "ArrowDown" ? 1 : -1)),
              );
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
  }, [childrenByPath, loadChildren, openEntry, pasteCopiedPath, renameEntry, rootPath, selectionAnchorPath, selectEntryRange, stageDeleteEntries, updateStatus]);

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
            <PanelIconButton icon={FileText} tooltip={tm("files.panel.new_file", "New File")} onClick={() => void createItem("file")} />
            <PanelIconButton icon={FolderPlus} tooltip={tm("files.panel.new_folder", "New Folder")} onClick={() => void createItem("directory")} />
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
              <FileInlineEditRow edit={inlineEdit} depth={0} onChange={setInlineEdit} onCancel={() => setInlineEdit(null)} onSubmit={() => void submitInlineEdit()} />
            )}
            {rows.map((row) => (
              <FileTreeFragment
                key={row.entry.path}
                row={row}
                rootPath={rootPath}
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
                  if (event.key !== "Enter" && event.key !== "F2" && event.key !== "Delete" && event.key !== "Backspace") return;
                  event.preventDefault();
                  event.stopPropagation();
                  const targets = selectedPaths.has(row.entry.path) && selectedEntries.length > 1 ? selectedEntries : [row.entry];
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
                  updateStatus(formatI18n(tm("files.panel.status.copied_format", "Copied %@"), row.entry.name), "success");
                }}
                onReveal={() => {
                  setSelectedPath(row.entry.path);
                  const targets = entriesForContextAction(row.entry);
                  setSelectedPaths(new Set(targets.map((entry) => entry.path)));
                  setSelectionAnchorPath(row.entry.path);
                  void Promise.all(targets.map((entry) => revealFile(rootPath, entry.path))).catch(handleFileError);
                }}
                onPaste={copiedPath ? () => {
                  setSelectedPath(row.entry.path);
                  setSelectedPaths(new Set([row.entry.path]));
                  setSelectionAnchorPath(row.entry.path);
                  void copyFile(rootPath, copiedPath, row.entry.isDirectory ? row.entry.path : parentPath(row.entry.path, rootPath))
                    .then((entry) => {
                      const parent = parentPath(entry.path, rootPath);
                      setSelectedPath(entry.path);
                      setSelectedPaths(new Set([entry.path]));
                      setSelectionAnchorPath(entry.path);
                      setExpandedPaths((current) => new Set(current).add(parent));
                      setError(null);
                      updateStatus(formatI18n(tm("files.panel.status.pasted_format", "Pasted %@"), entry.name), "success");
                      return loadChildren(parent);
                    })
                    .catch((nextError) => {
                      handleFileError(nextError);
                    });
                } : undefined}
              />
            ))}
          </>
        ) : (
          <div className="px-2 py-3 text-xs text-ink-faint">
            {loadingPaths.has(rootPath) ? tm("files.panel.loading", "Reading files") : tm("files.panel.empty", "No Files")}
          </div>
        )}
        {error && (
          <div className="mx-1 mt-2 rounded-md border border-brand-red/30 bg-brand-red/10 px-2.5 py-2 text-xs text-brand-red">
            {error}
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

type FileInlineEdit = (
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
    }
);

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
  rootPath,
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
  rootPath: string;
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
  const editAfter =
    inlineEdit?.mode === "create" &&
    inlineEdit.parentPath === row.entry.path &&
    row.entry.isDirectory;
  const isRenaming =
    inlineEdit?.mode === "rename" &&
    inlineEdit.entry.path === row.entry.path;

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
        <ContextMenu ariaLabel={`${entry.name} ${labels.actions}`} menu={contextMenu.menu} onClose={contextMenu.closeMenu}>
          <ContextMenuItem label={labels.open} disabled={entry.isDirectory || isMultiContext} onSelect={onOpen}>{labels.open}</ContextMenuItem>
          <ContextMenuItem label={labels.edit} disabled={entry.isDirectory || isMultiContext} onSelect={onEdit}>{labels.edit}</ContextMenuItem>
          <ContextMenuItem label={labels.insertPathTerminal} onSelect={onInsertPathIntoTerminal}>{labels.insertPathTerminal}</ContextMenuItem>
          <ContextMenuItem label={labels.copyPath} onSelect={onCopyPath}>{labels.copyPath}</ContextMenuItem>
          <ContextMenuItem label={labels.copy} disabled={isMultiContext} onSelect={onCopy}>{labels.copy}</ContextMenuItem>
          <ContextMenuItem label={labels.cut} disabled>{labels.cut}</ContextMenuItem>
          <ContextMenuItem label={labels.rename} disabled={isMultiContext} onSelect={onRename}>{labels.rename}</ContextMenuItem>
          <ContextMenuItem label={labels.paste} disabled={!onPaste} onSelect={onPaste}>{labels.paste}</ContextMenuItem>
          <ContextMenuItem label={labels.reveal} onSelect={onReveal}>{labels.reveal}</ContextMenuItem>
          <ContextMenuSeparator />
          <ContextMenuItem label={labels.delete} onSelect={onDelete}>{labels.delete}</ContextMenuItem>
        </ContextMenu>
      </div>
    </Tooltip>
  );
}

function flattenFileRows(
  rootPath: string,
  childrenByPath: Record<string, FileEntry[]>,
  expandedPaths: Set<string>,
) {
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

function fileNameFromPath(path: string) {
  const normalized = normalizeInspectorPath(path);
  return normalized.split("/").filter(Boolean).pop() || normalized || path;
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

function AIPanel({ project }: { project?: WorkspaceProject }) {
  const { sessions, projectTotals, globalTotals } = useAIRuntimeSnapshot(project?.id);
  const history = useAIHistorySnapshot(project);
  const [isManualRefreshFeedbackVisible, setManualRefreshFeedbackVisible] = useState(false);
  const manualRefreshStartedAtRef = useRef(0);
  const isRefreshingAIHistory = history.isLoading || isManualRefreshFeedbackVisible;
  const isForegroundAIIndexing = history.isForegroundLoading || isManualRefreshFeedbackVisible;
  const displayedProgress =
    isManualRefreshFeedbackVisible && !history.isLoading ? 1 : history.progress;
  const historySnapshot = history.snapshot;
  const liveProjectTokens = projectTotals.totalTokens;
  const liveTodayTokens = globalTotals.totalTokens;
  const projectTotalTokens = historySnapshot.projectSummary.projectTotalTokens + liveProjectTokens;
  const todayTotalTokens = historySnapshot.projectSummary.todayTotalTokens + liveTodayTokens;
  const toolRankingRows = toolRows(sessions, historySnapshot.toolBreakdown);
  const modelRankingRows = modelRows(sessions, historySnapshot.modelBreakdown);
  const recentHistorySessions = historySnapshot.sessions.slice(0, MAX_VISIBLE_AI_SESSIONS);
  const refreshAIHistory = useCallback(async () => {
    manualRefreshStartedAtRef.current = Date.now();
    setManualRefreshFeedbackVisible(true);
    await history.refresh();
  }, [history.refresh]);

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

  return (
    <>
      <PanelHeader
        title={tm("ai.panel.title", "AI Assistant")}
        trailing={
          <PanelIconButton
            icon={RefreshCw}
            tooltip={isRefreshingAIHistory ? tm("ai.action.stop_refresh", "Stop the current AI stats refresh.") : tm("ai.action.refresh_current_project", "Refresh AI stats for the current project.")}
            busy={isRefreshingAIHistory}
            disabled={isRefreshingAIHistory}
            onClick={() => void refreshAIHistory()}
          />
        }
      />
      <div className="flex-1 overflow-y-auto scrollbar-overlay p-3 flex flex-col gap-3">
        <PanelCard title={tm("ai.live_sessions", "Current Session Totals")}>
          {sessions.length > 0 ? (
            <div className="flex flex-col gap-2">
              {sessions.map((session) => (
                <LiveSessionRow key={session.terminalId} session={session} />
              ))}
            </div>
          ) : (
            <div className="min-h-12 grid place-items-center text-xs font-medium text-ink-faint">
              {tm("ai.live_sessions.empty", "There are no current AI sessions right now")}
            </div>
          )}
        </PanelCard>

        <div className="grid grid-cols-2 gap-3">
          <Tooltip label={tm("ai.summary.current_project", "Current Project")} placement="bottom" triggerClassName="block w-full">
            <PanelCard>
              <div className="text-xs text-ink-mute">{tm("ai.summary.current_project", "Current Project")}</div>
              <div className="text-lg font-semibold mt-1 tabular-nums">
                {formatTokens(projectTotalTokens)}
              </div>
            </PanelCard>
          </Tooltip>
          <Tooltip label={tm("ai.summary.today_total", "Today's Total")} placement="bottom" triggerClassName="block w-full">
            <PanelCard>
              <div className="text-xs text-ink-mute">{tm("ai.summary.today_total", "Today's Total")}</div>
              <div className="text-lg font-semibold mt-1 tabular-nums">
                {formatTokens(todayTotalTokens)}
              </div>
            </PanelCard>
          </Tooltip>
        </div>

        <PanelCard title={tm("ai.today_usage", "Today's Usage")}>
          <BarsRow sessions={sessions} buckets={historySnapshot.todayTimeBuckets} />
          <div className="flex justify-between mt-1 text-xs text-ink-faint">
            <span>00:00</span>
            <span>06:00</span>
            <span>12:00</span>
            <span>18:00</span>
            <span>23:59</span>
          </div>
        </PanelCard>

        <PanelCard title={tm("ai.recent_usage", "Recent Usage")}>
          <HeatmapGrid sessions={sessions} days={historySnapshot.heatmap} />
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

        <PanelCard
          title={
            <span>
              {tm("ai.sessions.history", "Session History")}{" "}
              {historySnapshot.sessions.length > MAX_VISIBLE_AI_SESSIONS && (
                <span className="text-ink-faint font-normal ml-1">
                  {formatI18n(tm("ai.sessions.recent_limit_format", "Recent %d"), MAX_VISIBLE_AI_SESSIONS)}
                </span>
              )}
            </span>
          }
          bodyPadding={false}
        >
          {recentHistorySessions.map((session) => (
            <HistorySessionRow key={session.sessionId} session={session} />
          ))}
          {recentHistorySessions.length === 0 && (
            <div className="px-3 py-3 text-xs text-ink-faint">
              {history.isLoading
                ? tm("ai.indexing.reading_sources", "Reading index.")
                : history.error
                  ? tm("ai.session.storage.open_failed", "Unable to open session storage.")
                  : tm("ai.sessions.empty", "No Session History")}
            </div>
          )}
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

function LiveSessionRow({ session }: { session: AISessionSnapshot }) {
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
            {formatTokens(liveSessionTotalTokens(session))}
          </div>
          <div className="mt-1 text-xs text-ink-faint">{tm("ai.metric.session_total", "Session Total")}</div>
        </div>
      </div>
    </Tooltip>
  );
}

function toolRows(sessions: AISessionSnapshot[], historyRows: AIUsageBreakdownItem[]) {
  return rankRows(sessions, historyRows, (session) => session.tool);
}

function modelRows(sessions: AISessionSnapshot[], historyRows: AIUsageBreakdownItem[]) {
  return rankRows(sessions, historyRows, (session) => normalizeRankModelName(session.model));
}

function rankRows(
  sessions: AISessionSnapshot[],
  historyRows: AIUsageBreakdownItem[],
  keyOf: (session: AISessionSnapshot) => string | null,
) {
  const totals = new Map<string, number>();
  for (const row of historyRows) {
    if (!isDisplayableModelOrToolKey(row.key)) continue;
    totals.set(row.key, (totals.get(row.key) ?? 0) + row.totalTokens);
  }
  for (const session of sessions) {
    const key = keyOf(session);
    if (!key || !isDisplayableModelOrToolKey(key)) continue;
    const value = sessionDeltaTokens(session);
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

const MAX_VISIBLE_AI_SESSIONS = 20;

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

function sessionDeltaTokens(session: AISessionSnapshot) {
  return Math.max(0, session.totalTokens - session.baselineTotalTokens);
}

function BarsRow({ sessions, buckets }: { sessions: AISessionSnapshot[]; buckets: AITimeBucket[] }) {
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
      values[index].value += bucket.totalTokens;
      values[index].requestCount += bucket.requestCount;
    }
    for (const session of sessions) {
      const date = new Date(session.updatedAt * 1000);
      if (startOfLocalDay(date).getTime() !== today.getTime()) continue;
      values[todayBucketIndex(date)].value += sessionDeltaTokens(session);
    }
    return values;
  }, [buckets, sessions]);
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
                {d.requestCount > 0 ? ` · ${formatI18n(tm("common.requests_format", "Requests %@"), d.requestCount)}` : ""}
              </span>
            }
            placement="top"
            triggerClassName="flex flex-1 h-full min-w-0"
          >
            <div className="flex items-end h-full w-full">
              <div
                className={`w-full rounded-[3px] transition-colors ${
                  hasValue
                    ? "bg-brand-blue/70 hover:bg-brand-blue"
                    : "bg-brand-blue/18 hover:bg-brand-blue/35"
                }`}
                style={{ height: `${h}px` }}
              />
            </div>
          </Tooltip>
        );
      })}
    </div>
  );
}

const HEATMAP_GAP = 3;
const HEATMAP_BASE_CELL = 9;
const HEATMAP_DEFAULT_LAYOUT = { columns: 15, cellSize: 9 };

function HeatmapGrid({ sessions, days }: { sessions: AISessionSnapshot[]; days: AIHeatmapDay[] }) {
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
        Math.min(
          10,
          Math.floor((width - HEATMAP_GAP * Math.max(nextColumns - 1, 0)) / nextColumns),
        ),
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
        value: day.totalTokens,
        requestCount: day.requestCount,
      });
    }
    for (const session of sessions) {
      const day = startOfLocalDay(new Date(session.updatedAt * 1000));
      const existing = values.get(day.getTime()) ?? { value: 0, requestCount: 0 };
      values.set(day.getTime(), {
        value: existing.value + sessionDeltaTokens(session),
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
    const nonZero = cells.flat().map((item) => item.value).filter((value) => value > 0).sort((a, b) => a - b);
    return { cells, nonZero };
  }, [columns, days, sessions]);

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
                    {item.requestCount > 0 ? ` · ${formatI18n(tm("common.requests_format", "Requests %@"), item.requestCount)}` : ""}
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
}

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

function RankRow({
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
        <div
          className="h-full rounded-full bg-brand-blue/65"
          style={{ width: `${pct}%` }}
        />
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
}

function HistorySessionRow({ session }: { session: AIHistorySessionSummary }) {
  const tool = session.lastTool || "-";
  const model = normalizeRankModelName(session.lastModel) ?? "-";
  const lastSeenLabel = sessionTimeLabel(session.lastSeenAt);
  const todayLabel = formatI18n(tm("common.today_format", "Today %@"), formatTokens(session.todayTokens));

  return (
    <div className="px-3 py-2.5 flex items-start justify-between gap-3 text-xs border-b border-line last:border-b-0 hover:bg-fill/[0.03]">
      <div className="min-w-0 flex-1">
        <div className="text-[13px] font-semibold text-ink truncate">{session.sessionTitle}</div>
        <div className="mt-1 grid gap-0.5">
          <div className="text-xs font-medium text-ink-soft truncate">{tool}</div>
          <div className="text-[11px] font-medium text-ink-faint truncate">{model}</div>
        </div>
      </div>
      <div className="flex-shrink-0 text-right">
        <div className="text-[11px] font-medium text-ink-soft whitespace-nowrap">{lastSeenLabel}</div>
        <div className="mt-1 text-sm font-medium tabular-nums text-ink">{formatTokens(session.totalTokens)}</div>
        <div className="mt-0.5 text-[11px] font-medium text-ink-faint whitespace-nowrap">{todayLabel}</div>
      </div>
    </div>
  );
}

function sessionTimeLabel(timestamp: number) {
  if (!Number.isFinite(timestamp) || timestamp <= 0) return "-";
  return formatI18n(tm("common.last_format", "Last %@"), relativeSessionTime(new Date(timestamp * 1000)));
}

function relativeSessionTime(date: Date) {
  const formatter = new Intl.RelativeTimeFormat(localeFromSettings(), {
    numeric: "auto",
    style: "short",
  });
  const diffSeconds = Math.round((date.getTime() - Date.now()) / 1000);
  const divisions: Array<[Intl.RelativeTimeFormatUnit, number]> = [
    ["year", 60 * 60 * 24 * 365],
    ["month", 60 * 60 * 24 * 30],
    ["week", 60 * 60 * 24 * 7],
    ["day", 60 * 60 * 24],
    ["hour", 60 * 60],
    ["minute", 60],
    ["second", 1],
  ];
  for (const [unit, seconds] of divisions) {
    if (Math.abs(diffSeconds) >= seconds || unit === "second") {
      return formatter.format(Math.round(diffSeconds / seconds), unit);
    }
  }
  return formatter.format(0, "second");
}

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
  const [sshStatus, setSshStatus] = useState<{ tone: "neutral" | "success" | "warning" | "danger"; message: string }>({
    tone: "neutral",
    message: tm("ssh.panel.status.ready", "Ready"),
  });
  const sshRowLabels = useMemo<SSHRowLabels>(() => ({
    connect: tm("ssh.profile.connect", "Connect"),
    copy: tm("common.copy", "Copy"),
    edit: tm("ssh.profile.edit", "Edit SSH Connection"),
    remove: tm("common.remove", "Remove"),
    actions: tm("files.panel.actions", "Actions"),
  }), []);

  const updateSshStatus = useCallback((message: string, tone: "neutral" | "success" | "warning" | "danger" = "neutral") => {
    setSshStatus({ tone, message });
  }, []);

  const handleSshError = useCallback((nextError: unknown) => {
    const message = nextError instanceof Error ? nextError.message : String(nextError);
    setError(message);
    updateSshStatus(message, "danger");
  }, [updateSshStatus]);

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
      updateSshStatus(tm("ssh.panel.status.refreshed", "SSH connections refreshed"), "success");
    } catch (nextError) {
      handleSshError(nextError);
    } finally {
      setLoading(false);
    }
  }, [handleSshError, updateSshStatus]);

  useEffect(() => {
    void refresh();
  }, [refresh]);

  const startProfileEdit = (profile?: SSHConnectionProfile) => {
    setError(null);
    setDraft(profileToDraft(profile));
  };

  const pickPrivateKey = async () => {
    if (!window.__TAURI_INTERNALS__) return;
    const selected = await openDialog({
      title: tm("ssh.profile.private_key", "Private Key"),
      multiple: false,
      directory: false,
    });
    if (!selected || Array.isArray(selected)) return;
    setDraft((current) => current ? { ...current, privateKeyPath: selected } : current);
  };

  const upsertProfile = async (nextDraft: SSHProfileDraft) => {
    const host = nextDraft.host.trim();
    const username = nextDraft.username.trim();
    if (!host || !username) return;
    const port = Math.max(1, Math.min(65535, Number(nextDraft.port || 22) || 22));
    try {
      setSaving(true);
      const snapshot = await invoke<SSHProfilesSnapshot>("ssh_profile_upsert", {
        request: {
          id: nextDraft.id ?? null,
          name: nextDraft.name.trim(),
          host,
          port,
          username,
          credentialKind: nextDraft.credentialKind,
          privateKeyPath: nextDraft.credentialKind === "privateKey" ? nextDraft.privateKeyPath.trim() : "",
          password: nextDraft.credentialKind === "password" ? nextDraft.password.trim() : "",
          keyPassphrase: nextDraft.credentialKind === "privateKey" ? nextDraft.keyPassphrase.trim() : "",
        },
      });
      setProfiles(snapshot.profiles);
      setDraft(null);
      setError(null);
      updateSshStatus(tm("ssh.profile.saved", "Saved SSH connection."), "success");
    } catch (nextError) {
      handleSshError(nextError);
    } finally {
      setSaving(false);
    }
  };

  const deleteProfile = async (profile: SSHConnectionProfile) => {
    if (
      !(await systemConfirm(formatI18n(tm("ssh.profile.delete.message_format", "Delete %@? The saved local credential will also be removed."), sshDisplayName(profile)), {
        title: tm("ssh.profile.delete", "Delete SSH Connection"),
        kind: "warning",
        okLabel: tm("common.delete", "Delete"),
        cancelLabel: tm("common.cancel", "Cancel"),
      }))
    ) return;
    try {
      const snapshot = await invoke<SSHProfilesSnapshot>("ssh_profile_delete", {
        profileId: profile.id,
      });
      setProfiles(snapshot.profiles);
      setError(null);
      updateSshStatus(tm("ssh.profile.deleted", "Deleted SSH connection."), "warning");
    } catch (nextError) {
      handleSshError(nextError);
    }
  };

  const connectProfile = async (profile: SSHConnectionProfile) => {
    if (!project) {
      updateSshStatus(tm("ssh.panel.status.no_project", "Select a project before connecting."), "warning");
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
      updateSshStatus(formatI18n(tm("ssh.profile.connecting_format", "Connecting to %@."), sshDisplayName(profile)), "success");
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
          <>
            <PanelIconButton icon={Plus} tooltip={tm("ssh.profile.add", "Add SSH Connection")} onClick={() => startProfileEdit()} />
            <PanelIconButton icon={RefreshCw} tooltip={tm("common.refresh", "Refresh")} onClick={() => void refresh()} />
          </>
        }
      />
      {profiles.length === 0 && !draft ? (
        <PanelEmptyState
          icon={Server}
          title={isLoading ? tm("ssh.panel.loading", "Reading SSH connections") : tm("ssh.panel.empty.title", "No SSH Connections")}
          description={tm("ssh.panel.empty.help", "Add a global SSH profile and double-click it to connect in a terminal.")}
          action={
            <HeroButton size="sm" variant="primary" onPress={() => startProfileEdit()}>
              {tm("ssh.profile.add", "Add SSH Connection")}
            </HeroButton>
          }
        />
      ) : (
        <div className="flex-1 overflow-y-auto scrollbar-overlay p-3 grid auto-rows-min gap-2">
          {draft && (
            <SSHProfileEditor
              draft={draft}
              isSaving={isSaving}
              onChange={setDraft}
              onCancel={() => setDraft(null)}
              onPickPrivateKey={() => void pickPrivateKey()}
              onSubmit={() => void upsertProfile(draft)}
            />
          )}
          {profiles.map((profile) => (
            <SSHProfileRow
              key={profile.id}
              profile={profile}
              disabled={!project}
              labels={sshRowLabels}
              onConnect={() => void connectProfile(profile)}
              onCopy={() => {
                updateSshStatus(formatI18n(tm("files.panel.status.copied_format", "Copied %@"), sshDisplayName(profile)), "success");
              }}
              onEdit={() => startProfileEdit(profile)}
              onDelete={() => void deleteProfile(profile)}
            />
          ))}
        </div>
      )}
      {error && (
        <div className="mx-3 mb-3 rounded-md border border-brand-red/30 bg-brand-red/10 px-2.5 py-2 text-xs text-brand-red">
          {error}
        </div>
      )}
      <PanelStatusBar
        tone={error ? "danger" : sshStatus.tone}
        leading={
          <>
            {isLoading ? <Spinner size="sm" color="current" /> : <Server size={12} />}
            <span className="truncate">{error ?? sshStatus.message}</span>
          </>
        }
        trailing={<span className="tabular-nums text-current/75">{profiles.length}</span>}
      />
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

function SSHProfileEditor({
  draft,
  isSaving,
  onChange,
  onCancel,
  onPickPrivateKey,
  onSubmit,
}: {
  draft: SSHProfileDraft;
  isSaving: boolean;
  onChange: (draft: SSHProfileDraft) => void;
  onCancel: () => void;
  onPickPrivateKey: () => void;
  onSubmit: () => void;
}) {
  const canSubmit = draft.host.trim().length > 0 && draft.username.trim().length > 0 && !isSaving;
  const set = <DraftKey extends keyof SSHProfileDraft>(key: DraftKey, value: SSHProfileDraft[DraftKey]) => {
    onChange({ ...draft, [key]: value });
  };

  return (
    <PanelCard
      title={draft.id ? tm("ssh.profile.edit", "Edit SSH Connection") : tm("ssh.profile.add", "Add SSH Connection")}
      divider
      className="bg-fill/[0.045]"
    >
      <form
        className="grid gap-2"
        onSubmit={(event) => {
          event.preventDefault();
          if (canSubmit) onSubmit();
        }}
      >
        <SSHFormField label={tm("ssh.profile.name", "Name")}>
          <TextInput
            value={draft.name}
            onChange={(event) => set("name", event.currentTarget.value)}
            className="h-8 text-xs"
          />
        </SSHFormField>
        <div className="grid grid-cols-[minmax(0,1fr)_72px] gap-2">
          <SSHFormField label={tm("ssh.profile.host", "Host")} required>
            <TextInput
              value={draft.host}
              onChange={(event) => set("host", event.currentTarget.value)}
              className="h-8 text-xs"
              required
            />
          </SSHFormField>
          <SSHFormField label={tm("ssh.profile.port", "Port")}>
            <TextInput
              value={draft.port}
              inputMode="numeric"
              onChange={(event) => set("port", event.currentTarget.value.replace(/[^\d]/g, ""))}
              className="h-8 text-xs"
            />
          </SSHFormField>
        </div>
        <SSHFormField label={tm("ssh.profile.username", "Username")} required>
          <TextInput
            value={draft.username}
            onChange={(event) => set("username", event.currentTarget.value)}
            className="h-8 text-xs"
            required
          />
        </SSHFormField>
        <SSHFormField label={tm("ssh.profile.credential", "Credential")}>
          <Select
            value={draft.credentialKind}
            onChange={(value) => set("credentialKind", normalizeCredentialKind(value))}
            options={[
              { value: "none", label: tm("common.none", "None") },
              { value: "password", label: tm("ssh.profile.password", "Password") },
              { value: "privateKey", label: tm("ssh.profile.private_key", "Private Key") },
            ]}
            ariaLabel={tm("ssh.profile.credential", "Credential")}
            className="w-full"
          />
        </SSHFormField>
        {draft.credentialKind === "password" && (
          <SSHFormField label={tm("ssh.profile.password", "Password")}>
            <TextInput
              value={draft.password}
              type="password"
              onChange={(event) => set("password", event.currentTarget.value)}
              className="h-8 text-xs"
            />
          </SSHFormField>
        )}
        {draft.credentialKind === "privateKey" && (
          <>
            <SSHFormField label={tm("ssh.profile.private_key", "Private Key")}>
              <div className="flex gap-1.5">
                <TextInput
                  value={draft.privateKeyPath}
                  onChange={(event) => set("privateKeyPath", event.currentTarget.value)}
                  className="h-8 text-xs"
                />
                <HeroButton
                  size="sm"
                  variant="secondary"
                  className="h-8 min-w-0 px-2 text-xs"
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
                className="h-8 text-xs"
              />
            </SSHFormField>
          </>
        )}
        <div className="mt-1 flex justify-end gap-1.5">
          <HeroButton size="sm" variant="ghost" className="h-8 min-w-0 px-3 text-xs" onPress={onCancel}>
            {tm("common.cancel", "Cancel")}
          </HeroButton>
          <HeroButton
            size="sm"
            variant="primary"
            className="h-8 min-w-0 px-3 text-xs"
            type="submit"
            isDisabled={!canSubmit}
          >
            {isSaving ? tm("common.processing", "Processing") : tm("common.save", "Save")}
          </HeroButton>
        </div>
      </form>
    </PanelCard>
  );
}

function SSHFormField({
  label,
  required,
  children,
}: {
  label: ReactNode;
  required?: boolean;
  children: ReactNode;
}) {
  return (
    <label className="grid gap-1">
      <span className="text-[11px] font-semibold text-ink-soft">
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
  const [menuOpen, setMenuOpen] = useState(false);
  const tint =
    profile.credentialKind === "privateKey"
      ? "text-brand-blue bg-brand-blue/14"
      : profile.credentialKind === "password"
        ? "text-brand-amber bg-brand-amber/14"
        : "text-ink-mute bg-fill/[0.055]";
  return (
    <div
      className="group relative rounded-[8px] border border-line bg-fill/[0.035] p-2.5 pr-10 grid grid-cols-[30px_minmax(0,1fr)] items-center gap-2.5 hover:bg-fill/[0.055] transition-colors"
      onDoubleClick={() => {
        if (!disabled) onConnect();
      }}
    >
      <span className={`w-[30px] h-[30px] rounded-[7px] grid place-items-center ${tint}`}>
        {profile.credentialKind === "privateKey" ? <KeyRound size={13} /> : <Server size={13} />}
      </span>
      <div className="min-w-0">
        <div className="text-xs font-semibold text-ink truncate">{sshDisplayName(profile)}</div>
        <div className="text-xs text-ink-faint truncate">
          {profile.username}@{profile.host}:{profile.port}
        </div>
      </div>
      <div className="absolute right-2 top-1/2 flex -translate-y-1/2 items-center gap-0.5 rounded bg-surface-chrome/95 opacity-0 pointer-events-none transition-opacity group-hover:opacity-100 group-hover:pointer-events-auto">
        <PressableButton
          className="h-6 rounded-md px-2 text-xs font-semibold text-ink-soft hover:bg-fill/8 hover:text-ink disabled:opacity-45"
          disabled={disabled}
          onPressUp={onConnect}
        >
          {labels.connect}
        </PressableButton>
        <DesktopMenu
          ariaLabel={`${sshDisplayName(profile)} ${labels.actions}`}
          isOpen={menuOpen}
          onOpenChange={setMenuOpen}
          trigger={
            <button
              type="button"
              className="grid h-6 w-6 place-items-center rounded text-ink-faint hover:bg-fill/8 hover:text-ink"
              aria-label={`${sshDisplayName(profile)} ${labels.actions}`}
            >
              <MoreHorizontal size={13} />
            </button>
          }
        >
          <DesktopMenuItem disabled={disabled} label={labels.connect} onSelect={onConnect}>{labels.connect}</DesktopMenuItem>
          <DesktopMenuItem
            label={labels.copy}
            onSelect={() => {
              void navigator.clipboard?.writeText(`${profile.username}@${profile.host}:${profile.port}`);
              onCopy();
            }}
          >
            {labels.copy}
          </DesktopMenuItem>
          <DesktopMenuItem label={labels.edit} onSelect={onEdit}>{labels.edit}</DesktopMenuItem>
          <DesktopMenuItem label={labels.remove} onSelect={onDelete}>{labels.remove}</DesktopMenuItem>
        </DesktopMenu>
      </div>
    </div>
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
