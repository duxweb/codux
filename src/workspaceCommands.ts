import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

export type WorkspaceCommand =
  | {
      type: "add-top-terminal-split";
      title?: string;
      command?: string;
    }
  | {
      type: "add-bottom-terminal-tab";
      label?: string;
      command?: string;
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
    };

const WORKSPACE_COMMAND_EVENT = "codux:workspace-command";

export function dispatchWorkspaceCommand(command: WorkspaceCommand) {
  window.dispatchEvent(
    new CustomEvent<WorkspaceCommand>(WORKSPACE_COMMAND_EVENT, {
      detail: command,
    }),
  );
}

export function broadcastWorkspaceCommand(command: WorkspaceCommand) {
  if (window.__TAURI_INTERNALS__) {
    void emit(WORKSPACE_COMMAND_EVENT, command);
    return;
  }
  dispatchWorkspaceCommand(command);
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
