import { Dropdown } from "@heroui/react";
import { useEffect, useState } from "react";
import { ArrowTopRight, ChevronDown } from "../icons";
import {
  listProjectOpenApplications,
  openProjectInApplication,
  revealProjectInFileManager,
  type ProjectOpenApplication,
} from "../ide";
import { tm } from "../i18n";
import type { WorkspaceProject } from "../types";
import { Tooltip } from "./Tooltip";

const OPEN_BUTTON_ICON_SIZE = 11;

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
      <Tooltip label={tm("open.ide", "Open in IDE")} placement="bottom" disabled={open}>
        <Dropdown isOpen={open} onOpenChange={setOpen}>
          <Dropdown.Trigger
            isDisabled={!enabled}
            className="flex h-[26px] items-center rounded-pill border border-border-subtle bg-fill/[0.055] text-ink-soft transition-colors hover:border-border hover:bg-fill/10 hover:text-ink disabled:opacity-45"
            aria-label={tm("open.ide", "Open in IDE")}
          >
            <span className="grid h-full w-[30px] place-items-center rounded-l-pill">
              <ArrowTopRight size={OPEN_BUTTON_ICON_SIZE} strokeWidth={2.35} />
            </span>
            <span className="h-3.5 w-px bg-border/60" />
            <span className="grid h-full w-[22px] place-items-center rounded-r-pill">
              <ChevronDown size={OPEN_BUTTON_ICON_SIZE} className="text-ink-mute" strokeWidth={2.4} />
            </span>
          </Dropdown.Trigger>
          <Dropdown.Popover
            placement="bottom end"
            className="min-w-[210px] rounded-[10px] border border-border-subtle bg-surface-popover p-1 shadow-floating"
          >
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
              <Dropdown.Item id="finder">
                <span className="truncate">{tm("open.finder", "Open in Finder")}</span>
              </Dropdown.Item>
              {applications.map((item) => (
                <Dropdown.Item key={item.id} id={item.id} textValue={formatOpenTitle(item.label)}>
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

function formatOpenTitle(label: string) {
  return tm("open.application.format", "Open in %@").replace("%@", label);
}
