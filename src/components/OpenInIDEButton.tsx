import { convertFileSrc } from "@tauri-apps/api/core";
import { Dropdown } from "@heroui/react";
import { useEffect, useState } from "react";
import { ArrowTopRight, Box, ChevronDown, Folder, SquareTerminal } from "../icons";
import {
  listProjectOpenApplications,
  openProjectInApplication,
  revealProjectInFileManager,
  type ProjectOpenApplication,
} from "../ide";
import { tm } from "../i18n";
import type { WorkspaceProject } from "../types";
import { Tooltip } from "./Tooltip";

const appColors: Record<string, string> = {
  vscode: "#2f80ed",
  finder: "#4f9cff",
  terminal: "#1f2937",
  iterm: "#111827",
  ghostty: "#7c3aed",
  xcode: "#1d9bf0",
  intellijIdea: "#f43f5e",
  webStorm: "#38bdf8",
  phpStorm: "#a855f7",
  pyCharm: "#22c55e",
  goLand: "#06b6d4",
  clion: "#ef4444",
  rider: "#ec4899",
  androidStudio: "#3ddc84",
  cursor: "#111827",
  zed: "#f97316",
  sublimeText: "#ff9800",
  windsurf: "#06b6d4",
};

export function OpenInIDEButton({ project }: { project?: WorkspaceProject }) {
  const [open, setOpen] = useState(false);
  const [applications, setApplications] = useState<ProjectOpenApplication[]>([]);
  const enabled = Boolean(project?.path);

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

  const openWith = (applicationId: string) => {
    if (!project?.path) return;
    void openProjectInApplication(project.path, applicationId).catch((error) =>
      console.error("failed to open project in application", error),
    );
  };

  return (
    <div className="relative">
      <Tooltip label={tm("open.ide", "Open in IDE")} placement="bottom">
        <Dropdown
          isOpen={open}
          onOpenChange={setOpen}
        >
          <Dropdown.Trigger
            isDisabled={!enabled}
            className="flex h-[26px] items-center gap-1 rounded-pill border border-line bg-fill/[0.055] px-2 text-ink-soft transition-colors hover:border-line-strong hover:bg-fill/10 hover:text-ink disabled:opacity-45"
            aria-label={tm("open.ide", "Open in IDE")}
          >
            <ArrowTopRight size={13} strokeWidth={2.2} />
            <ChevronDown size={11} className="text-ink-mute" strokeWidth={2.4} />
          </Dropdown.Trigger>
          <Dropdown.Popover placement="bottom end" className="min-w-[210px] rounded-[10px] border border-line-strong bg-surface-chrome p-1 shadow-pop backdrop-blur-2xl">
            <Dropdown.Menu
              aria-label={tm("open.ide", "Open in IDE")}
              className="grid gap-0.5"
              onAction={(key) => {
                const id = String(key);
                if (id === "finder") {
                  if (project?.path) void revealProjectInFileManager(project.path);
                  return;
                }
                openWith(id);
              }}
            >
              <Dropdown.Item id="finder" className="menu-item">
                <Folder size={14} className="text-ink-mute" />
                <span className="truncate">{tm("open.finder", "Open in Finder")}</span>
              </Dropdown.Item>
              {applications.map((item) => (
                <Dropdown.Item key={item.id} id={item.id} className="menu-item" textValue={formatOpenTitle(item.label)}>
                  <ApplicationIcon application={item} size={14} />
                  <span className="truncate">{formatOpenTitle(item.label)}</span>
                </Dropdown.Item>
              ))}
            </Dropdown.Menu>
          </Dropdown.Popover>
        </Dropdown>
      </Tooltip>
    </div>
  );
}

function ApplicationIcon({
  application,
  size,
}: {
  application: ProjectOpenApplication;
  size: number;
}) {
  if (application.iconPath) {
    return (
      <img
        alt=""
        className="rounded-[4px] object-contain"
        height={size}
        src={convertFileSrc(application.iconPath)}
        width={size}
      />
    );
  }
  if (application.id === "terminal" || application.id === "iterm" || application.id === "ghostty") {
    return <SquareTerminal size={size} className="text-ink-soft" />;
  }
  const color = appColors[application.id] ?? "#64748b";
  const initials = application.label
    .split(/\s+/)
    .map((part) => part[0])
    .join("")
    .slice(0, 2)
    .toUpperCase();
  return (
    <span
      className="grid place-items-center rounded-[4px] text-white shadow-sm"
      style={{
        width: size,
        height: size,
        background: `linear-gradient(135deg, ${color}, color-mix(in oklab, ${color} 68%, black))`,
      }}
    >
      {initials ? (
        <span className="text-[7px] font-bold leading-none">{initials}</span>
      ) : (
        <Box size={Math.max(10, size - 3)} strokeWidth={2} />
      )}
    </span>
  );
}

function formatOpenTitle(label: string) {
  return tm("open.application.format", "Open in %@").replace("%@", label);
}
