import { ChevronDown, ChevronRight, Code2, Folder, SquareTerminal } from "../icons";
import { useEffect, useRef, useState } from "react";
import { Tooltip } from "./Tooltip";
import { tm } from "../i18n";

type IDE = {
  id: string;
  label: string;
  icon: typeof Code2;
};

const primaryItems: IDE[] = [
  { id: "vscode", label: "open.vscode", icon: Code2 },
  { id: "finder", label: "open.finder", icon: Folder },
  { id: "terminal", label: "open.terminal", icon: SquareTerminal },
  { id: "iterm", label: "open.iterm2", icon: SquareTerminal },
  { id: "ghostty", label: "open.ghostty", icon: SquareTerminal },
  { id: "xcode", label: "open.xcode", icon: Code2 },
];

const ideSubItems: IDE[] = [
  { id: "webstorm", label: "WebStorm", icon: Code2 },
  { id: "idea", label: "IntelliJ IDEA", icon: Code2 },
  { id: "pycharm", label: "PyCharm", icon: Code2 },
  { id: "goland", label: "GoLand", icon: Code2 },
  { id: "rustrover", label: "RustRover", icon: Code2 },
  { id: "rider", label: "Rider", icon: Code2 },
];

export function OpenInIDEButton() {
  const [open, setOpen] = useState(false);
  const [submenuOpen, setSubmenuOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (!open) return;
    const onClick = (event: MouseEvent) => {
      if (!wrapRef.current?.contains(event.target as Node)) {
        setOpen(false);
        setSubmenuOpen(false);
      }
    };
    window.addEventListener("mousedown", onClick);
    return () => window.removeEventListener("mousedown", onClick);
  }, [open]);

  const close = () => {
    setOpen(false);
    setSubmenuOpen(false);
  };

  return (
    <div ref={wrapRef} className="relative">
      <div className="flex items-center h-[26px] rounded-pill bg-fill/[0.055] border border-line hover:border-line-strong hover:bg-fill/10 transition-colors">
        <Tooltip label={tm("open.project.vscode", "Open Project in VS Code")} placement="bottom">
          <button className="flex items-center justify-center w-[30px] h-full rounded-l-pill">
            <Code2 size={14} className="text-ink-soft" />
          </button>
        </Tooltip>
        <div className="w-px h-3.5 bg-line-strong/60" />
        <Tooltip label={tm("open.ide", "Open in IDE")} placement="bottom">
          <button
            className="flex items-center justify-center w-[22px] h-full rounded-r-pill"
            onClick={() => setOpen((v) => !v)}
          >
            <ChevronDown size={11} className="text-ink-mute" strokeWidth={2.4} />
          </button>
        </Tooltip>
      </div>

      {open && (
        <div className="absolute right-0 top-[32px] z-50 min-w-[210px] rounded-[10px] bg-surface-chrome backdrop-blur-2xl border border-line-strong shadow-pop p-1.5">
          {primaryItems.map((item) => (
            <button key={item.id} className="menu-item" onClick={close}>
              <item.icon size={14} className="text-ink-mute" />
              <span className="flex-1 text-left">{tm(item.label, item.label)}</span>
            </button>
          ))}
          <div className="my-1 h-px bg-line" />
          <div
            className="relative"
            onMouseEnter={() => setSubmenuOpen(true)}
            onMouseLeave={() => setSubmenuOpen(false)}
          >
            <button className="menu-item w-full">
              <Code2 size={14} className="text-ink-mute" />
              <span className="flex-1 text-left">{tm("open.ide", "Open in IDE")}</span>
              <ChevronRight size={12} className="text-ink-faint" />
            </button>
            {submenuOpen && (
              <div className="absolute right-full top-0 mr-1 min-w-[180px] rounded-[10px] bg-surface-chrome backdrop-blur-2xl border border-line-strong shadow-pop p-1.5">
                {ideSubItems.map((item) => (
                  <button key={item.id} className="menu-item" onClick={close}>
                    <item.icon size={14} className="text-ink-mute" />
                    <span className="flex-1 text-left">{item.label}</span>
                  </button>
                ))}
              </div>
            )}
          </div>
        </div>
      )}
    </div>
  );
}
