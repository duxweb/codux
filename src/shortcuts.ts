import type { MainView, RightPanelKind } from "./types";
import { readAppSettings } from "./settings";
import { tm } from "./i18n";

export type ShortcutScope = "global" | "workspace" | "project-sidebar" | "task-sidebar" | "right-sidebar";

export type ShortcutContext = {
  focusScope: ShortcutScope;
  mainView: MainView;
  rightPanel: RightPanelKind | null;
};

export type ShortcutHandler = (event: KeyboardEvent, context: ShortcutContext) => boolean;

export type ShortcutDefinition = {
  id: string;
  label: string;
  labelKey: string;
  keys: Array<{
    key: string;
    meta?: boolean;
    ctrl?: boolean;
    alt?: boolean;
    shift?: boolean;
  }>;
  scope: ShortcutScope;
};

type ShortcutRegistration = {
  id: number;
  scope: ShortcutScope;
  handler: ShortcutHandler;
};

export const appShortcutDefinitions: ShortcutDefinition[] = [
  {
    id: "view.terminal",
    label: "Terminal View",
    labelKey: "shortcut.view.terminal",
    scope: "global",
    keys: [{ key: "1", meta: true }],
  },
  {
    id: "view.files",
    label: "Files View",
    labelKey: "shortcut.view.files",
    scope: "global",
    keys: [{ key: "2", meta: true }],
  },
  {
    id: "view.review",
    label: "Review View",
    labelKey: "shortcut.view.review",
    scope: "global",
    keys: [{ key: "3", meta: true }],
  },
  {
    id: "project.create",
    label: "New Project",
    labelKey: "shortcut.project.create",
    scope: "project-sidebar",
    keys: [{ key: "n", meta: true }],
  },
  {
    id: "settings.open",
    label: "Open Settings",
    labelKey: "shortcut.settings.open",
    scope: "project-sidebar",
    keys: [{ key: ",", meta: true }],
  },
  {
    id: "task.create",
    label: "New Worktree",
    labelKey: "shortcut.task.create",
    scope: "task-sidebar",
    keys: [{ key: "n", meta: true }],
  },
  {
    id: "editor.save",
    label: "Save File",
    labelKey: "shortcut.editor.save",
    scope: "workspace",
    keys: [{ key: "s", meta: true }],
  },
  {
    id: "editor.search",
    label: "Search Files",
    labelKey: "shortcut.editor.search",
    scope: "workspace",
    keys: [{ key: "f", meta: true }],
  },
  {
    id: "close.active",
    label: "Close Current Project",
    labelKey: "shortcut.close.active",
    scope: "workspace",
    keys: [{ key: "w", meta: true }],
  },
];

const registrations: ShortcutRegistration[] = [];
let nextId = 1;

export function registerShortcutHandler(scope: ShortcutScope, handler: ShortcutHandler) {
  const registration = {
    id: nextId,
    scope,
    handler,
  };
  nextId += 1;
  registrations.push(registration);

  return () => {
    const index = registrations.findIndex((item) => item.id === registration.id);
    if (index >= 0) {
      registrations.splice(index, 1);
    }
  };
}

export function dispatchShortcut(event: KeyboardEvent, context: ShortcutContext) {
  const scopes = context.focusScope === "global" ? ["global"] : [context.focusScope, "global"];

  for (const scope of scopes) {
    const handlers = registrations.filter((registration) => registration.scope === scope).sort((a, b) => b.id - a.id);

    for (const registration of handlers) {
      if (registration.handler(event, context)) {
        event.preventDefault();
        event.stopPropagation();
        return true;
      }
    }
  }

  return false;
}

export function configuredShortcutKeys(id: string) {
  const configured = parseShortcutSequence(readAppSettings().shortcuts[id]);
  if (configured.length > 0) return configured;
  return appShortcutDefinitions.find((shortcut) => shortcut.id === id)?.keys ?? [];
}

export function isConfiguredShortcut(event: KeyboardEvent, id: string) {
  return configuredShortcutKeys(id).some((shortcut) => isShortcut(event, shortcut));
}

export function shortcutDisplayValue(id: string, fallback?: string) {
  const keys = configuredShortcutKeys(id);
  return keys.length > 0 ? keys.map(formatShortcutKeys).join(" / ") : (fallback ?? tm("settings.shortcut.unset", "Not Set"));
}

export function isShortcut(
  event: KeyboardEvent,
  shortcut: {
    key: string;
    meta?: boolean;
    ctrl?: boolean;
    alt?: boolean;
    shift?: boolean;
  },
) {
  return (
    event.key.toLowerCase() === shortcut.key.toLowerCase() &&
    event.metaKey === Boolean(shortcut.meta) &&
    event.ctrlKey === Boolean(shortcut.ctrl) &&
    event.altKey === Boolean(shortcut.alt) &&
    event.shiftKey === Boolean(shortcut.shift)
  );
}

export function formatShortcutKeys(shortcut: ShortcutDefinition["keys"][number]) {
  const parts = [
    shortcut.meta ? "⌘" : "",
    shortcut.ctrl ? "⌃" : "",
    shortcut.alt ? "⌥" : "",
    shortcut.shift ? "⇧" : "",
    shortcut.key.length === 1 ? shortcut.key.toUpperCase() : shortcut.key,
  ];
  return parts.filter(Boolean).join("");
}

export function parseShortcutSequence(value?: string) {
  const raw = value?.trim();
  if (!raw) return [];
  return raw
    .split("/")
    .map((item) => parseShortcutText(item.trim()))
    .filter((item): item is ShortcutDefinition["keys"][number] => Boolean(item));
}

export function serializeShortcutSequence(keys: ShortcutDefinition["keys"]) {
  return keys.map(formatShortcutKeys).join(" / ");
}

function parseShortcutText(value: string): ShortcutDefinition["keys"][number] | null {
  if (!value) return null;
  let rest = value
    .replace(/Command|Cmd|⌘/gi, "Meta+")
    .replace(/Control|Ctrl|⌃/gi, "Ctrl+")
    .replace(/Option|Alt|⌥/gi, "Alt+")
    .replace(/Shift|⇧/gi, "Shift+")
    .replace(/\s+/g, "");
  const meta = rest.includes("Meta+");
  const ctrl = rest.includes("Ctrl+");
  const alt = rest.includes("Alt+");
  const shift = rest.includes("Shift+");
  rest = rest.replace(/Meta\+|Ctrl\+|Alt\+|Shift\+/g, "");
  if (!rest) return null;
  return {
    key: rest.length === 1 ? rest.toLowerCase() : rest,
    meta,
    ctrl,
    alt,
    shift,
  };
}
