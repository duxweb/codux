import {
  BarChart3,
  Book,
  BrainCog,
  Columns2,
  Cpu,
  Fire,
  FileText,
  Folder,
  GitBranch,
  ListChecks,
  ListTree,
  MemoryStick,
  Minus,
  PanelLeft,
  PanelLeftClose,
  Server,
  ShieldCheck,
  Sparkles,
  Star,
  TerminalSquare,
  Trophy,
  Wifi,
  Zap,
  type AppIcon,
} from "../icons";
import { invoke } from "@tauri-apps/api/core";
import { Tabs } from "@heroui/react";
import { useEffect, useMemo, useState } from "react";
import { useAIGlobalHistorySnapshot } from "../ai/history";
import { useMemoryExtractionStatus, type MemoryExtractionStatusSnapshot } from "../ai/memory";
import { useAIRuntimeSnapshot } from "../ai/runtime";
import type { PetLedger } from "../ai/petState";
import { Button } from "./Button";
import { DesktopPopover } from "./DesktopPopover";
import { OpenInIDEButton } from "./OpenInIDEButton";
import { PetSprite } from "./PetSprite";
import { PressableButton } from "./PressableButton";
import { Tooltip } from "./Tooltip";
import { WindowsWindowControls } from "./WindowsWindowControls";
import type { ButtonSize } from "./Button";
import type { MainView, PerformanceSnapshot, RemoteStatus, RightPanelKind, WorkspaceProject } from "../types";
import { dispatchWorkspaceCommand } from "../workspaceCommands";
import { readAppSettings, subscribeAppSettings } from "../settings";
import { formatI18n, t, tm } from "../i18n";
import { openAppWindow } from "../windowing";
import { isMacPlatform, isWindowsPlatform } from "../platform";
import { startWindowDrag } from "../windowDrag";

type Props = {
  projects: WorkspaceProject[];
  selectedProject?: WorkspaceProject;
  mainView: MainView;
  setMainView: (view: MainView) => void;
  isSidebarExpanded: boolean;
  toggleSidebar: () => void;
  isTaskSidebarExpanded: boolean;
  toggleTaskSidebar: () => void;
  rightPanel: RightPanelKind | null;
  toggleRightPanel: (panel: RightPanelKind) => void;
  remoteStatus?: RemoteStatus | null;
  pet: PetLedger;
};

type TitlebarPopoverKey = "pet" | "daily-level" | "remote";

const TITLEBAR_ICON_BUTTON_CLASS = "no-drag !h-[28px] !w-[28px] !min-w-[28px] !px-0 rounded-[7px]";
const TITLEBAR_TOOL_ICON_SIZE = 14;
const TITLEBAR_BUTTON_ICON_SIZE = 11;
const TITLEBAR_MEMORY_ICON_SIZE = 10;
const TITLEBAR_NAV_ICON_SIZE = 12;

export function Titlebar({
  projects,
  selectedProject,
  mainView,
  setMainView,
  isSidebarExpanded,
  toggleSidebar,
  toggleTaskSidebar,
  rightPanel,
  toggleRightPanel,
  remoteStatus,
  pet,
}: Props) {
  const [settings, setSettings] = useState(readAppSettings);
  const [activePopover, setActivePopover] = useState<TitlebarPopoverKey | null>(null);
  const globalHistory = useAIGlobalHistorySnapshot(projects);
  const { globalTotals } = useAIRuntimeSnapshot();
  const todayLevelTokens =
    Math.max(0, globalHistory.snapshot.todayTotalTokens - globalHistory.snapshot.todayCachedInputTokens) +
    globalTotals.totalTokens;
  const leftChromeInset = isMacPlatform() ? "pl-[86px]" : "pl-4";
  const rightChromeInset = isWindowsPlatform() ? "pr-[150px]" : "pr-4";

  useEffect(() => subscribeAppSettings(setSettings), []);
  useEffect(() => setActivePopover(null), [mainView]);
  const setPopoverOpen = (key: TitlebarPopoverKey, isOpen: boolean) => {
    setActivePopover(isOpen ? key : null);
  };

  return (
    <header
      data-tauri-drag-region
      className="absolute top-0 left-0 right-0 h-[var(--titlebar-height)] z-30 drag-region"
      onPointerDown={startWindowDrag}
    >
      <div className="absolute inset-0 flex items-center justify-between drag-region" data-tauri-drag-region>
        <div className={`flex items-center gap-2.5 ${leftChromeInset} no-drag`}>
          <GlyphButton
            icon={isSidebarExpanded ? PanelLeftClose : PanelLeft}
            tooltip={t("projects", settings)}
            onPress={toggleSidebar}
          />
          <GlyphButton icon={ListTree} tooltip={t("tasks", settings)} onPress={toggleTaskSidebar} />
          <GlyphButton
            icon={Columns2}
            tooltip={t("split", settings)}
            onPress={() => {
              setMainView("terminal");
              dispatchWorkspaceCommand({ type: "add-top-terminal-split" });
            }}
          />

          <PerformanceHUD />

          <MemoryStatusButton />

          <RemotePill
            status={remoteStatus}
            isOpen={activePopover === "remote"}
            onOpenChange={(isOpen) => setPopoverOpen("remote", isOpen)}
          />
        </div>

        <div className={`flex items-center gap-2.5 ${rightChromeInset} no-drag`}>
          {settings.pet.enabled && (
            <PetPopoverButton
              pet={pet}
              isOpen={activePopover === "pet"}
              onOpenChange={(isOpen) => setPopoverOpen("pet", isOpen)}
            />
          )}

          {projects.length > 0 && (
            <DailyLevelPopoverButton
              tokens={todayLevelTokens}
              isOpen={activePopover === "daily-level"}
              onOpenChange={(isOpen) => setPopoverOpen("daily-level", isOpen)}
            />
          )}

          <OpenInIDEButton project={selectedProject ?? projects[0]} />

          <GlyphButton
            icon={BarChart3}
            tooltip={t("aiAssistant", settings)}
            active={rightPanel === "ai"}
            onPress={() => toggleRightPanel("ai")}
          />
          <GlyphButton
            icon={Server}
            tooltip={t("ssh", settings)}
            active={rightPanel === "ssh"}
            onPress={() => toggleRightPanel("ssh")}
          />
          <GlyphButton
            icon={GitBranch}
            tooltip={t("git", settings)}
            active={rightPanel === "git"}
            onPress={() => toggleRightPanel("git")}
          />
          <GlyphButton
            icon={Folder}
            tooltip={t("files", settings)}
            active={rightPanel === "files"}
            onPress={() => toggleRightPanel("files")}
          />
        </div>
      </div>

      {isWindowsPlatform() && <WindowsWindowControls className="z-10 h-[var(--titlebar-height)]" />}

      <div
        className="absolute inset-0 flex items-center justify-center pointer-events-none drag-region"
        data-tauri-drag-region
      >
        <div className="pointer-events-auto">
          <ModeSwitcher mainView={mainView} setMainView={setMainView} />
        </div>
      </div>
    </header>
  );
}

function PetPopoverButton({
  pet,
  isOpen,
  onOpenChange,
}: {
  pet: PetLedger;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
}) {
  const info = pet.snapshot.progress;
  const tooltip = pet.snapshot.claimedAt
    ? petDisplayName(pet.snapshot.species, pet.snapshot.customPet, pet.snapshot.customName)
    : tm("pet.tooltip.pet", "Pet");
  const label = pet.snapshot.claimedAt
    ? formatI18n(tm("titlebar.level.short_format", "Lv.%@"), info.level)
    : tm("pet.title.claim", "Claim");
  const triggerClassName =
    "no-drag inline-flex h-[28px] items-center gap-1.5 rounded-[7px] border border-line bg-fill/[0.06] py-0 pl-2 pr-2.5 text-[12.5px] font-semibold text-ink transition-colors hover:border-line-strong hover:bg-fill/10 data-[pressed]:border-brand-blue/30 data-[pressed]:bg-brand-blue/16";

  if (!pet.snapshot.claimedAt) {
    return (
      <Tooltip label={tooltip} placement="bottom">
        <PressableButton
          aria-label={t("pet")}
          className={triggerClassName}
          onPressUp={() => void openAppWindow("pet-claim")}
        >
          <PetTitlebarIcon isMaxLevel={false} />
          <span className="leading-none">{label}</span>
        </PressableButton>
      </Tooltip>
    );
  }

  return (
    <DesktopPopover
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      placement="bottom-end"
      trigger={
        <button type="button" aria-label={t("pet")} title={tooltip} className={triggerClassName}>
          <PetTitlebarIcon isMaxLevel={pet.snapshot.claimedAt ? info.isAtMaxLevel : false} />
          <span className="leading-none">{label}</span>
        </button>
      }
    >
      <PetPanel pet={pet} />
    </DesktopPopover>
  );
}

function PetTitlebarIcon({ isMaxLevel }: { isMaxLevel: boolean }) {
  return (
    <span
      className={`grid h-[19px] w-[19px] place-items-center rounded-full text-on-brand shadow-sm ${
        isMaxLevel
          ? "bg-gradient-to-br from-brand-amber to-brand-amber-deep"
          : "bg-gradient-to-br from-brand-blue to-brand-blue-deep"
      }`}
    >
      {isMaxLevel ? <PetCrownIcon className="h-2 w-2" /> : <PetPawIcon className="h-[9px] w-[9px]" />}
    </span>
  );
}

function PetPawIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={className} fill="currentColor">
      <path d="M7.3 9.4c1.25-.23 2.02-1.78 1.72-3.45C8.7 4.28 7.43 3.1 6.18 3.33 4.93 3.56 4.16 5.1 4.47 6.78c.3 1.67 1.58 2.85 2.83 2.62Zm9.4 0c1.25.23 2.52-.95 2.83-2.62.31-1.68-.46-3.22-1.71-3.45-1.25-.23-2.53.95-2.84 2.62-.3 1.67.47 3.22 1.72 3.45ZM12 9.1c1.34 0 2.42-1.38 2.42-3.08S13.34 2.94 12 2.94 9.58 4.32 9.58 6.02 10.66 9.1 12 9.1Zm-5.63 4.33c-1.14 1.08-2.28 2.18-2.28 3.92 0 1.8 1.43 3.15 3.36 3.15.88 0 1.58-.22 2.25-.43.7-.22 1.36-.42 2.3-.42s1.6.2 2.3.42c.67.21 1.37.43 2.25.43 1.93 0 3.36-1.35 3.36-3.15 0-1.74-1.14-2.84-2.28-3.92-.7-.66-1.42-1.35-1.88-2.16C14.98 9.91 13.62 9.3 12 9.3s-2.98.61-3.75 1.97c-.46.81-1.18 1.5-1.88 2.16Z" />
    </svg>
  );
}

function PetCrownIcon({ className }: { className?: string }) {
  return (
    <svg viewBox="0 0 24 24" aria-hidden="true" className={className} fill="currentColor">
      <path d="M4.35 19.1h15.3a.9.9 0 0 1 .9.9v.18a.9.9 0 0 1-.9.9H4.35a.9.9 0 0 1-.9-.9V20a.9.9 0 0 1 .9-.9Zm.38-2.1 1.22-8.56a.85.85 0 0 1 1.43-.49l3.1 2.9 2.77-5.75a.84.84 0 0 1 1.5 0l2.77 5.75 3.1-2.9a.85.85 0 0 1 1.43.49L23.27 17a1.08 1.08 0 0 1-1.07 1.23H5.8A1.08 1.08 0 0 1 4.73 17Z" />
    </svg>
  );
}

type DailyLevelTier = {
  id: string;
  min: number;
  color: string;
  icon: AppIcon;
};

const DAILY_LEVEL_TIERS: readonly DailyLevelTier[] = [
  { id: "iron", min: 0, color: "#5B616D", icon: Minus },
  { id: "bronze", min: 1_000_000, color: "#C98663", icon: Zap },
  { id: "silver", min: 3_000_000, color: "#C8D1E3", icon: ShieldCheck },
  { id: "gold", min: 6_000_000, color: "#E8AA34", icon: Star },
  { id: "platinum", min: 10_000_000, color: "#7ED6D8", icon: Star },
  { id: "diamond", min: 18_000_000, color: "#59A7FF", icon: Sparkles },
  { id: "master", min: 30_000_000, color: "#9A72FF", icon: Trophy },
  { id: "grandmaster", min: 50_000_000, color: "#FF5E8E", icon: Fire },
];

function DailyLevelPopoverButton({
  tokens,
  isOpen,
  onOpenChange,
}: {
  tokens: number;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
}) {
  const safeTokens = Math.max(0, Math.floor(tokens));
  const tier = dailyLevelTier(safeTokens);
  return (
    <DesktopPopover
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      placement="bottom-end"
      contentClassName="p-3"
      trigger={
        <button
          type="button"
          aria-label={tm("ai.today_level", "Today's Level")}
          title={tm("ai.today_level", "Today's Level")}
          className="no-drag inline-flex h-[28px] items-center gap-1.5 rounded-[7px] border border-line bg-fill/[0.06] py-0 pl-2 pr-2.5 text-[12.5px] font-semibold text-ink transition-colors hover:border-line-strong hover:bg-fill/10 data-[pressed]:bg-fill/12"
        >
          <DailyLevelBadge tier={tier} size="sm" />
          <span className="leading-none">{dailyLevelTitle(tier)}</span>
        </button>
      }
    >
      <DailyLevelPanel tokens={safeTokens} currentTier={tier} />
    </DesktopPopover>
  );
}

function DailyLevelPanel({ tokens, currentTier }: { tokens: number; currentTier: DailyLevelTier }) {
  return (
    <div className="w-[280px]">
      <div className="flex items-center gap-2.5">
        <DailyLevelBadge tier={currentTier} size="lg" />
        <div className="min-w-0">
          <div className="text-xs font-medium text-ink-faint">{tm("ai.today_level", "Today's Level")}</div>
          <div className="mt-0.5 text-[15px] font-bold text-ink">{dailyLevelTitle(currentTier)}</div>
        </div>
        <div className="ml-auto text-right">
          <div className="text-[11px] font-medium text-ink-faint">{tm("ai.today_tokens", "Today's Tokens")}</div>
          <div className="mt-0.5 text-[15px] font-bold tabular-nums text-ink">{formatTokens(tokens)}</div>
        </div>
      </div>

      <div className="mt-3 grid gap-1.5">
        {DAILY_LEVEL_TIERS.map((tier) => {
          const isCurrent = tier.id === currentTier.id;
          return (
            <div
              key={tier.id}
              className={`flex items-center gap-2.5 rounded-[10px] px-2.5 py-2 ${isCurrent ? "bg-fill/[0.075]" : ""}`}
              style={isCurrent ? { boxShadow: `inset 0 0 0 1px ${tier.color}45` } : undefined}
            >
              <DailyLevelBadge tier={tier} size="md" />
              <div className="min-w-0 flex-1">
                <div className="truncate text-[13px] font-semibold text-ink">{dailyLevelTitle(tier)}</div>
                <div className="mt-0.5 text-[11px] font-medium text-ink-faint">
                  {formatI18n(tm("common.need_format", "Need %@"), formatTokens(tier.min))}
                </div>
              </div>
              {isCurrent && (
                <span
                  className="rounded-full px-2 py-1 text-[11px] font-bold"
                  style={{ backgroundColor: `${tier.color}24`, color: tier.color }}
                >
                  {tm("common.current", "Current")}
                </span>
              )}
            </div>
          );
        })}
      </div>
    </div>
  );
}

function DailyLevelBadge({ tier, size }: { tier: DailyLevelTier; size: ButtonSize }) {
  const Icon = tier.icon;
  const metrics = size === "lg" ? { box: 34, icon: 14 } : size === "md" ? { box: 24, icon: 10 } : { box: 18, icon: 8 };
  return (
    <span
      className="grid flex-shrink-0 place-items-center rounded-full text-on-brand"
      style={{
        width: metrics.box,
        height: metrics.box,
        background: `linear-gradient(135deg, ${tier.color}, ${mixHex(tier.color, "#000000", 0.22)})`,
      }}
    >
      <Icon size={metrics.icon} strokeWidth={2.5} />
    </span>
  );
}

function dailyLevelTier(tokens: number) {
  for (let index = DAILY_LEVEL_TIERS.length - 1; index >= 0; index -= 1) {
    const tier = DAILY_LEVEL_TIERS[index];
    if (tokens >= tier.min) return tier;
  }
  return DAILY_LEVEL_TIERS[0];
}

function dailyLevelTitle(tier: DailyLevelTier) {
  return tm(`rank.${tier.id}`, tier.id);
}

function mixHex(hex: string, otherHex: string, amount: number) {
  const left = parseHexColor(hex);
  const right = parseHexColor(otherHex);
  if (!left || !right) return hex;
  const mix = (a: number, b: number) => Math.round(a + (b - a) * amount);
  return `#${[mix(left.r, right.r), mix(left.g, right.g), mix(left.b, right.b)]
    .map((value) => value.toString(16).padStart(2, "0"))
    .join("")}`;
}

function parseHexColor(hex: string) {
  const value = hex.replace(/^#/, "");
  if (!/^[\da-f]{6}$/i.test(value)) return null;
  return {
    r: Number.parseInt(value.slice(0, 2), 16),
    g: Number.parseInt(value.slice(2, 4), 16),
    b: Number.parseInt(value.slice(4, 6), 16),
  };
}

function GlyphButton({
  icon: Icon,
  tooltip,
  active,
  onPress,
}: {
  icon: typeof PanelLeft;
  tooltip: string;
  active?: boolean;
  onPress?: () => void;
}) {
  return (
    <Tooltip label={tooltip} placement="bottom">
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        onPress={onPress}
        aria-label={tooltip}
        className={`${TITLEBAR_ICON_BUTTON_CLASS} ${active ? "bg-fill/10 text-ink" : "text-ink-soft"}`}
      >
        <Icon size={TITLEBAR_TOOL_ICON_SIZE} strokeWidth={1.9} />
      </Button>
    </Tooltip>
  );
}

function MemoryStatusButton() {
  const snapshot = useMemoryExtractionStatus();
  const isProcessing = snapshot.status === "processing";
  const isQueued = snapshot.status === "queued";
  const isFailed = snapshot.status === "failed";
  const isActive = isProcessing || isQueued || isFailed;
  const tone = isProcessing
    ? "text-brand-blue"
    : isQueued
      ? "text-brand-amber"
      : isFailed
        ? "text-brand-red"
        : "text-ink-soft";
  const activeSurface = isProcessing
    ? "border-brand-blue/45 bg-brand-blue/14 shadow-[0_0_0_1px_color-mix(in_oklab,var(--color-brand-blue)_18%,transparent)]"
    : isQueued
      ? "border-brand-amber/45 bg-brand-amber/12 shadow-[0_0_0_1px_color-mix(in_oklab,var(--color-brand-amber)_18%,transparent)]"
      : isFailed
        ? "border-brand-red/45 bg-brand-red/12 shadow-[0_0_0_1px_color-mix(in_oklab,var(--color-brand-red)_18%,transparent)]"
        : "border-line bg-fill/[0.06]";
  return (
    <Tooltip label={<MemoryStatusTooltip snapshot={snapshot} />} placement="bottom">
      <button
        type="button"
        aria-label={tm("memory.manager.window.title", "Memory Manager")}
        className={`no-drag relative inline-grid h-[28px] w-[28px] place-items-center rounded-[7px] border outline-none transition-colors hover:border-line-strong hover:bg-fill/10 ${activeSurface} ${tone}`}
        data-active={isActive ? "true" : "false"}
        onClick={() => void openAppWindow("memory-manager")}
      >
        <BrainCog size={TITLEBAR_MEMORY_ICON_SIZE} strokeWidth={2.1} />
        {isProcessing ? (
          <span
            className="absolute -right-0.5 -top-0.5 h-2.5 w-2.5 rounded-full border border-surface-chrome border-t-brand-blue bg-surface-chrome motion-safe:animate-spin"
            aria-hidden="true"
          />
        ) : isActive ? (
          <span
            className={`absolute -right-0.5 -top-0.5 h-2.5 w-2.5 rounded-full border border-surface-chrome ${
              isQueued ? "bg-brand-amber" : "bg-brand-red"
            }`}
            aria-hidden="true"
          />
        ) : null}
      </button>
    </Tooltip>
  );
}

function MemoryStatusTooltip({ snapshot }: { snapshot: MemoryExtractionStatusSnapshot }) {
  const title =
    snapshot.status === "processing"
      ? tm("memory.status.processing", "Remembering")
      : snapshot.status === "queued"
        ? tm("memory.status.queued", "Memory queued")
        : snapshot.status === "failed"
          ? tm("memory.status.failed", "Memory failed")
          : tm("memory.status.idle", "Memory idle");
  return (
    <div className="w-[210px] text-[12px]">
      <div className="font-semibold text-ink">{title}</div>
      <div className="mt-1 text-ink-faint">
        {formatI18n(
          tm("memory.status.detail", "Memory queue: %@ pending, %@ running"),
          snapshot.pendingCount,
          snapshot.runningCount,
        )}
      </div>
      {snapshot.lastError && (
        <div className="mt-2 rounded-md bg-brand-red/10 px-2 py-1.5 text-brand-red">{snapshot.lastError}</div>
      )}
    </div>
  );
}

function PerformanceHUD() {
  const [settings, setSettings] = useState(readAppSettings);
  const [snapshot, setSnapshot] = useState<PerformanceSnapshot>({
    cpuPercent: 0,
    memoryBytes: 0,
  });

  useEffect(() => {
    return subscribeAppSettings(setSettings);
  }, []);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || !settings.developerHud) {
      return;
    }

    let isMounted = true;
    const refresh = () => {
      void invoke<PerformanceSnapshot>("performance_snapshot")
        .then((next) => {
          if (isMounted) {
            setSnapshot(next);
          }
        })
        .catch((error) => console.error("failed to load performance snapshot", error));
    };
    refresh();
    const minimumIntervalMs = isWindowsPlatform() ? 5000 : 1000;
    const intervalMs = Math.max(minimumIntervalMs, Number(settings.developerRefresh || 3) * 1000);
    const timer = window.setInterval(refresh, intervalMs);
    return () => {
      isMounted = false;
      window.clearInterval(timer);
    };
  }, [settings.developerHud, settings.developerRefresh]);

  if (!settings.developerHud) return null;

  const cpuText = formatCpu(snapshot.cpuPercent);
  const memoryText = formatBytes(snapshot.memoryBytes);
  const cpuTone =
    snapshot.cpuPercent >= 85 ? "text-brand-red" : snapshot.cpuPercent >= 60 ? "text-brand-amber" : "text-ink-soft";

  const tooltip = [
    formatI18n(tm("performance.monitor.cpu_format", "CPU %@"), cpuText),
    formatI18n(tm("performance.monitor.memory_format", "MEM %@"), memoryText),
  ].join(" · ");

  return (
    <Tooltip label={tooltip} placement="bottom">
      <div className="flex items-center h-[26px] px-2 rounded-pill bg-fill/[0.05] border border-line text-xs font-medium text-ink-soft font-mono cursor-default">
        <span className={`flex items-center gap-1.5 px-1.5 ${cpuTone}`}>
          <Cpu size={TITLEBAR_BUTTON_ICON_SIZE} strokeWidth={2.2} />
          <span className="tabular-nums">{cpuText}</span>
        </span>
        <span className="w-px h-3 bg-line" />
        <span className="flex items-center gap-1.5 px-1.5">
          <MemoryStick size={TITLEBAR_BUTTON_ICON_SIZE} strokeWidth={2.2} />
          <span className="tabular-nums">{memoryText}</span>
        </span>
      </div>
    </Tooltip>
  );
}

function formatCpu(percent: number) {
  if (!Number.isFinite(percent)) return "0%";
  return `${Math.max(0, Math.round(percent))}%`;
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0M";
  const gb = 1_073_741_824;
  const mb = 1_048_576;
  if (bytes >= gb) {
    return `${(bytes / gb).toFixed(2)}G`;
  }
  return `${Math.round(bytes / mb)}M`;
}

function RemotePill({
  status,
  isOpen,
  onOpenChange,
}: {
  status?: RemoteStatus | null;
  isOpen: boolean;
  onOpenChange: (isOpen: boolean) => void;
}) {
  const enabled = Boolean(status?.enabled);
  const tooltip = tm("remote.connection", "远程连接");
  const label = enabled
    ? tm("remote.status.connected_label", "Connected")
    : tm("remote.status.disconnected_label", "Disconnected");
  return (
    <DesktopPopover
      isOpen={isOpen}
      onOpenChange={onOpenChange}
      placement="bottom-start"
      trigger={
        <button
          type="button"
          aria-label={tooltip}
          title={tooltip}
          className={`pill-base border transition-colors gap-1.5 no-drag ${
            enabled
              ? "bg-brand-green/12 border-brand-green/30 text-brand-green hover:bg-brand-green/16"
              : "bg-fill/[0.05] border-line text-ink-soft hover:bg-fill/10"
          }`}
        >
          <Wifi size={TITLEBAR_BUTTON_ICON_SIZE} strokeWidth={2.25} />
          <span>{label}</span>
        </button>
      }
    >
      <RemotePanel status={status} />
    </DesktopPopover>
  );
}

function RemotePanel({ status }: { status?: RemoteStatus | null }) {
  const devices = status?.deviceList ?? [];

  return (
    <div className="w-[276px] p-2.5">
      <div className="px-1 pb-2">
        <div className="truncate text-sm font-semibold text-ink">{tm("remote.connection", "远程连接")}</div>
      </div>
      <div className="max-h-[240px] overflow-y-auto scrollbar-overlay rounded-[8px] border border-line bg-fill/[0.025]">
        {devices.length > 0 ? (
          devices.map((device) => (
            <div
              key={device.id}
              className="flex items-center justify-between gap-3 border-b border-line px-2.5 py-2 last:border-b-0"
            >
              <div className="min-w-0">
                <div className="truncate text-sm font-semibold text-ink-soft">{device.name || device.id}</div>
                <div className="mt-0.5 truncate text-[11px] font-medium text-ink-faint">
                  {device.online ? tm("remote.devices.online", "Online") : tm("remote.devices.offline", "Offline")}
                </div>
              </div>
              <span className={`h-2 w-2 flex-none rounded-full ${device.online ? "bg-brand-green" : "bg-fill/25"}`} />
            </div>
          ))
        ) : (
          <div className="px-2.5 py-3 text-xs text-ink-faint">{tm("remote.devices.empty", "No paired devices")}</div>
        )}
      </div>
    </div>
  );
}

function PetPanel({ pet }: { pet: PetLedger }) {
  const [isEditingName, setEditingName] = useState(false);
  const [draftName, setDraftName] = useState(pet.snapshot.customName);
  const info = pet.snapshot.progress;
  const stats = pet.snapshot.currentStats;
  const personaLabel = tm(`pet.persona.${pet.snapshot.personaId}`, pet.snapshot.personaId);
  const speciesName =
    pet.snapshot.customPet?.displayName || tm(`pet.species.${pet.snapshot.species}.base`, pet.snapshot.species);
  const trimmedName = pet.snapshot.customName.trim();
  const displayName = trimmedName || speciesName;
  const subtitle = trimmedName ? speciesName : null;
  const statRows = useMemo(
    () => [
      {
        key: "wisdom",
        emoji: "🧠",
        label: tm("pet.attribute.wisdom", "Wisdom"),
        value: stats.wisdom,
        color: "#2F8FFF",
        help: tm("pet.attribute.wisdom.help", "Reflects deeper, denser sessions with more substantial exchanges."),
      },
      {
        key: "chaos",
        emoji: "🔥",
        label: tm("pet.attribute.chaos", "Chaos"),
        value: stats.chaos,
        color: "#FF6030",
        help: tm("pet.attribute.chaos.help", "Reflects fast, jumpy, high-tempo sessions with frequent bursts."),
      },
      {
        key: "night",
        emoji: "🌙",
        label: tm("pet.attribute.night", "Night"),
        value: stats.night,
        color: "#6060CC",
        help: tm("pet.attribute.night.help", "Reflects how much of your recent activity leans into late-night hours."),
      },
      {
        key: "stamina",
        emoji: "💪",
        label: tm("pet.attribute.stamina", "Stamina"),
        value: stats.stamina,
        color: "#20A060",
        help: tm(
          "pet.attribute.stamina.help",
          "Reflects steadier sessions that hold focus across more sustained back-and-forth.",
        ),
      },
      {
        key: "empathy",
        emoji: "🩹",
        label: tm("pet.attribute.empathy", "Empathy"),
        value: stats.empathy,
        color: "#E060A0",
        help: tm(
          "pet.attribute.empathy.help",
          "Reflects patient repair work, iterative debugging, and careful refinement.",
        ),
      },
    ],
    [stats.chaos, stats.empathy, stats.night, stats.stamina, stats.wisdom],
  );
  const widestStatText = useMemo(
    () => statRows.map((row) => formatTokens(row.value)).sort((left, right) => right.length - left.length)[0] ?? "0",
    [statRows],
  );

  useEffect(() => {
    setDraftName(pet.snapshot.customName);
  }, [pet.snapshot.customName, pet.snapshot.species]);

  const commitName = () => {
    setEditingName(false);
    if (draftName !== pet.snapshot.customName) {
      void pet.rename(draftName).catch((error) => console.error("pet rename failed", error));
    }
  };

  return (
    <div className="w-[300px]">
      <section className="relative flex flex-col items-center px-3.5 pb-3.5 pt-[18px] text-center">
        <div className="grid h-[110px] w-[110px] place-items-center">
          <PetSprite
            species={pet.snapshot.species}
            src={pet.snapshot.customPet?.spritesheetDataUrl}
            state="idle"
            size={110}
            staticMode={false}
          />
        </div>

        <Tooltip label={tm("pet.dex.open", "Open Dex")} placement="left">
          <Button
            isIconOnly
            size="sm"
            variant="ghost"
            aria-label={tm("pet.dex.open", "Open Dex")}
            className="absolute right-3.5 top-3.5 h-7 w-7 min-w-7 rounded-[7px] bg-brand-blue/10 text-brand-blue hover:bg-brand-blue/14"
            onPress={() => void openAppWindow("pet-dex")}
          >
            <Book size={13} strokeWidth={2.2} />
          </Button>
        </Tooltip>

        <div className="mt-3.5 flex min-w-0 max-w-full items-baseline justify-center gap-1.5">
          {isEditingName ? (
            <input
              autoFocus
              value={draftName}
              placeholder={tm("pet.name.placeholder", "Pet Name")}
              onBlur={commitName}
              onChange={(event) => setDraftName(event.currentTarget.value)}
              onKeyDown={(event) => {
                if (event.key === "Enter") {
                  event.currentTarget.blur();
                }
                if (event.key === "Escape") {
                  setDraftName(pet.snapshot.customName);
                  setEditingName(false);
                }
              }}
              className="h-8 w-[140px] rounded-[7px] border border-line-strong bg-fill/[0.06] px-2 text-center text-[17px] font-bold text-ink outline-none focus:border-brand-blue/65"
            />
          ) : (
            <button
              type="button"
              tabIndex={-1}
              className="min-w-0 truncate text-[17px] font-bold leading-6 text-ink"
              onClick={() => {
                setDraftName(pet.snapshot.customName);
                setEditingName(true);
              }}
            >
              {displayName}
            </button>
          )}
          {subtitle && !isEditingName && (
            <span className="max-w-[92px] truncate text-sm font-medium leading-5 text-ink-mute">{subtitle}</span>
          )}
        </div>

        <div className="mt-2 rounded-full bg-brand-blue/14 px-2.5 py-1 text-xs font-medium leading-none text-brand-blue">
          {personaLabel}
        </div>

        <div className="mt-2.5 text-[26px] font-black leading-8 text-ink">Lv.{info.level}</div>
      </section>

      <div className="mx-3.5 h-px bg-line" />

      <section className="px-3.5 py-3">
        <PetMeter
          label={tm("pet.xp.label", "Experience")}
          value={`${formatTokens(info.xpInLevel)} / ${formatTokens(info.xpForLevel)}`}
          progress={info.progress}
          color="#2F8FFF"
        />
      </section>

      <div className="mx-3.5 h-px bg-line" />

      <section className="px-3.5 py-3">
        <div className="mb-[7px] text-left text-xs font-medium text-ink-faint">{tm("pet.stats.title", "Traits")}</div>
        <div className="grid gap-[7px]">
          {statRows.map((row) => (
            <PetTrait
              key={row.key}
              emoji={row.emoji}
              label={row.label}
              value={row.value}
              color={row.color}
              widestValueText={widestStatText}
              help={row.help}
            />
          ))}
        </div>
      </section>

      <div className="mx-3.5 h-px bg-line" />

      <div className="py-2.5">
        <div className="grid gap-0.5 text-center">
          <div className="text-xs font-medium text-ink-faint">{tm("pet.total_xp", "Total XP")}</div>
          <div className="text-[13px] font-semibold text-ink">{formatTokens(info.totalXp)}</div>
        </div>
      </div>
    </div>
  );
}

function petDisplayName(species: string, customPet?: { displayName?: string | null } | null, customName?: string) {
  const base = customPet?.displayName || tm(`pet.species.${species}.base`, species.replace(/^custom:/, ""));
  const trimmed = customName?.trim();
  return trimmed || base;
}

function PetMeter({
  label,
  value,
  progress,
  color,
}: {
  label: string;
  value: string;
  progress: number;
  color: string;
}) {
  return (
    <div>
      <div className="flex items-center justify-between gap-2 text-xs">
        <span className="font-medium text-ink-mute">{label}</span>
        <span className="font-medium tabular-nums text-ink-soft">{value}</span>
      </div>
      <div className="mt-1.5 h-[7px] overflow-hidden rounded-full" style={{ backgroundColor: `${color}26` }}>
        <div
          className="h-full rounded-full transition-[width] duration-300 ease-out"
          style={{
            width: `${Math.round(Math.max(0, Math.min(1, progress)) * 100)}%`,
            background: `linear-gradient(90deg, ${color}, ${mixHex(color, "#000000", 0.15)})`,
          }}
        />
      </div>
    </div>
  );
}

function PetTrait({
  emoji,
  label,
  value,
  color,
  widestValueText,
  help,
}: {
  emoji: string;
  label: string;
  value: number;
  color: string;
  widestValueText: string;
  help?: string;
}) {
  const valueText = formatTokens(value);
  const ratio = Math.max(0, Math.min(1, value / 330));
  return (
    <Tooltip label={help} placement="right" disabled={!help}>
      <div className="grid grid-cols-[16px_32px_minmax(0,1fr)_auto] items-center gap-1.5 text-xs">
        <span className="grid w-4 place-items-center text-xs leading-none">{emoji}</span>
        <span className="truncate text-left font-medium text-ink-mute">{label}</span>
        <div className="h-[5px] overflow-hidden rounded-full" style={{ backgroundColor: `${color}1f` }}>
          <div
            className="h-full rounded-full transition-[width] duration-500 ease-out"
            style={{ width: `${ratio * 100}%`, backgroundColor: `${color}bf` }}
          />
        </div>
        <span className="relative min-w-[2ch] text-right font-mono text-xs font-semibold tabular-nums text-ink-soft">
          <span className="invisible">{widestValueText}</span>
          <span className="absolute inset-0 text-right">{valueText}</span>
        </span>
      </div>
    </Tooltip>
  );
}

function formatTokens(value: number) {
  const absolute = Math.abs(Math.floor(value));
  const sign = value < 0 ? "-" : "";
  const compact = (divisor: number, suffix: string) => {
    const scaled = absolute / divisor;
    const digits = scaled >= 100 ? scaled.toFixed(0) : scaled >= 10 ? scaled.toFixed(1) : scaled.toFixed(2);
    return `${sign}${digits.replace(/\.?0+$/, "")}${suffix}`;
  };
  if (absolute >= 1_000_000_000) return compact(1_000_000_000, "B");
  if (absolute >= 1_000_000) return compact(1_000_000, "M");
  if (absolute >= 1_000) return compact(1_000, "K");
  return `${Math.floor(value)}`;
}

function ModeSwitcher({ mainView, setMainView }: { mainView: MainView; setMainView: (view: MainView) => void }) {
  const items: Array<{ id: MainView; icon: typeof TerminalSquare; label: string }> = [
    { id: "terminal", icon: TerminalSquare, label: tm("workspace.create_split.terminal", "Terminal") },
    { id: "files", icon: FileText, label: t("files") },
    { id: "review", icon: ListChecks, label: t("review") },
  ];
  return (
    <Tabs
      selectedKey={mainView}
      onSelectionChange={(key) => setMainView(key as MainView)}
      aria-label={tm("titlebar.view_switcher", "View Switcher")}
      className="no-drag"
    >
      <Tabs.List className="flex h-[28px] items-center gap-0.5 rounded-full border border-line bg-fill/[0.055] p-[3px]">
        {items.map(({ id, icon: Icon, label }) => (
          <Tabs.Tab
            key={id}
            id={id}
            className="h-[22px] min-w-[76px] rounded-full px-3 text-xs font-semibold text-ink-soft outline-none transition-colors hover:bg-fill/8 hover:text-ink data-[selected]:bg-brand-blue data-[selected]:text-white"
          >
            <span className="inline-flex items-center justify-center gap-1.5">
              <Icon size={TITLEBAR_NAV_ICON_SIZE} strokeWidth={2.15} />
              <span>{label}</span>
            </span>
          </Tabs.Tab>
        ))}
      </Tabs.List>
    </Tabs>
  );
}
