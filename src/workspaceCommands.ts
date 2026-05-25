import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

export type WorkspaceCommand =
  | {
      type: "add-top-terminal-split";
      title?: string;
      command?: string;
      deferredCommand?: string;
      projectId?: string;
      projectPath?: string;
      projectName?: string;
    }
  | {
      type: "add-bottom-terminal-tab";
      label?: string;
      command?: string;
      deferredCommand?: string;
      projectId?: string;
      projectPath?: string;
      projectName?: string;
    }
  | {
      type: "open-file";
      rootPath: string;
      path: string;
    }
  | {
      type: "insert-terminal-text";
      text: string;
    }
  | {
      type: "reattach-terminal-pane";
      paneId: string;
      terminalId: string;
    }
  | {
      type: "editor-save";
    }
  | {
      type: "editor-search";
    }
  | {
      type: "close-active";
    }
  | {
      type: "open-right-panel";
      panel: "git" | "files" | "ai" | "ssh";
    };

const WORKSPACE_COMMAND_EVENT = "codux:workspace-command";
let pendingOpenFileCommand: Extract<WorkspaceCommand, { type: "open-file" }> | null = null;

function workspacePathKey(value: string) {
  let normalized = value.trim().replace(/\\/g, "/");
  normalized = normalized.replace(/^\/\/\?\//, "");
  while (normalized.length > 1 && normalized.endsWith("/")) {
    normalized = normalized.slice(0, -1);
  }
  if (/^[a-z]:/i.test(normalized)) {
    normalized = normalized.toLowerCase();
  }
  return normalized;
}

export function workspacePathsMatch(left?: string, right?: string) {
  if (!left || !right) return false;
  return workspacePathKey(left) === workspacePathKey(right);
}

export function dispatchWorkspaceCommand(command: WorkspaceCommand) {
  window.dispatchEvent(
    new CustomEvent<WorkspaceCommand>(WORKSPACE_COMMAND_EVENT, {
      detail: command,
    }),
  );
}

export function broadcastWorkspaceCommand(command: WorkspaceCommand) {
  if (command.type === "open-file") {
    pendingOpenFileCommand = command;
  }
  if (window.__TAURI_INTERNALS__) {
    void emit(WORKSPACE_COMMAND_EVENT, command);
    return;
  }
  dispatchWorkspaceCommand(command);
}

export function consumePendingOpenFileCommand(rootPath?: string) {
  const command = pendingOpenFileCommand;
  if (!command) return null;
  if (rootPath && !workspacePathsMatch(command.rootPath, rootPath)) return null;
  pendingOpenFileCommand = null;
  return command;
}

export function clearPendingOpenFileCommand(command: Extract<WorkspaceCommand, { type: "open-file" }>) {
  if (pendingOpenFileCommand?.rootPath === command.rootPath && pendingOpenFileCommand.path === command.path) {
    pendingOpenFileCommand = null;
  }
}

export function listenWorkspaceCommand(listener: (command: WorkspaceCommand) => void) {
  const handler = (event: Event) => {
    listener((event as CustomEvent<WorkspaceCommand>).detail);
  };
  window.addEventListener(WORKSPACE_COMMAND_EVENT, handler);
  let tauriUnlisten: UnlistenFn | undefined;
  let disposed = false;
  if (window.__TAURI_INTERNALS__) {
    void listen<WorkspaceCommand>(WORKSPACE_COMMAND_EVENT, (event) => {
      listener(event.payload);
    }).then((unlisten) => {
      if (disposed) {
        unlisten();
        return;
      }
      tauriUnlisten = unlisten;
    });
  }
  return () => {
    disposed = true;
    window.removeEventListener(WORKSPACE_COMMAND_EVENT, handler);
    tauriUnlisten?.();
  };
}
