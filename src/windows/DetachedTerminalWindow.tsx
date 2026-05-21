import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import { TerminalView } from "../components/TerminalView";
import { WindowsWindowControls } from "../components/WindowsWindowControls";
import { tm } from "../i18n";
import { isWindowsPlatform } from "../platform";
import { terminalRuntime } from "../terminal/runtime";
import { destroyCurrentAppWindow, revealCurrentAppWindow } from "../windowing";
import { broadcastWorkspaceCommand } from "../workspaceCommands";

type TerminalWindowParams = {
  terminalId: string;
  backendId: string;
  projectId: string;
  slotId: string;
  paneId: string;
  title: string;
  cwd: string;
  projectName: string;
};

export function DetachedTerminalWindow() {
  const params = useMemo(readTerminalWindowParams, []);
  const [localTerminalId, setLocalTerminalId] = useState<string | null>(null);
  const [isClosing, setIsClosing] = useState(false);
  const didReattachRef = useRef(false);
  const isClosingRef = useRef(false);
  const didRequestNativeCloseRef = useRef(false);

  const reattachPane = useCallback(() => {
    if (didReattachRef.current) return;
    if (!params?.paneId || !params.terminalId) return;
    didReattachRef.current = true;
    broadcastWorkspaceCommand({
      type: "reattach-terminal-pane",
      paneId: params.paneId,
      terminalId: params.terminalId,
    });
  }, [params]);

  const closeAfterTerminalDisposed = useCallback(() => {
    if (!isClosingRef.current || didRequestNativeCloseRef.current) return;
    didRequestNativeCloseRef.current = true;
    void destroyCurrentAppWindow().catch((error) => {
      console.error("failed to destroy detached terminal window", error);
    });
  }, []);

  const requestWindowClose = useCallback(() => {
    reattachPane();
    if (isClosingRef.current) return;
    isClosingRef.current = true;
    setIsClosing(true);
    setLocalTerminalId((current) => {
      if (!current) {
        closeAfterTerminalDisposed();
      }
      return null;
    });
  }, [closeAfterTerminalDisposed, reattachPane]);

  useEffect(() => {
    if (!params?.backendId) return;
    const session = terminalRuntime.ensureAttachedSession({
      backendId: params.backendId,
      terminalId: params.terminalId,
      projectId: params.projectId,
      projectName: params.projectName,
      slotId: params.slotId,
      title: params.title || "Terminal",
      cwd: params.cwd,
    });
    setLocalTerminalId(session.id);
    return () => {
      terminalRuntime.detachView(session.id);
    };
  }, [params]);

  useEffect(() => {
    void revealCurrentAppWindow();
  }, []);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__ || !params?.backendId) return;
    const currentWindow = getCurrentWebviewWindow();

    const onBeforeUnload = () => {
      reattachPane();
    };

    const unlistenPromise = currentWindow.onCloseRequested((event) => {
      reattachPane();
      if (isClosingRef.current) return;
      event.preventDefault();
      requestWindowClose();
    });

    window.addEventListener("beforeunload", onBeforeUnload);
    return () => {
      window.removeEventListener("beforeunload", onBeforeUnload);
      void unlistenPromise.then((unlisten) => unlisten());
    };
  }, [params, reattachPane, requestWindowClose]);

  const closeWindow = () => {
    requestWindowClose();
  };

  if (!params) {
    return (
      <main className="h-screen grid place-items-center text-sm text-ink-mute">
        {tm("terminal.detached.missing_session", "Missing terminal session.")}
      </main>
    );
  }

  return (
    <main className="h-screen min-w-0 min-h-0 overflow-hidden bg-[var(--terminal-bg)] text-ink">
      {isWindowsPlatform() && <WindowsWindowControls className="h-12" />}
      <section className="h-full min-w-0 min-h-0 bg-[var(--terminal-bg)] p-4">
        {localTerminalId && !isClosing ? (
          <TerminalView
            terminalId={localTerminalId}
            chrome={false}
            onClose={closeWindow}
            onDisposed={closeAfterTerminalDisposed}
          />
        ) : isClosing ? (
          <div className="h-full bg-[var(--terminal-bg)]" />
        ) : (
          <div className="h-full grid place-items-center text-xs text-ink-mute">
            {tm("terminal.detached.mounting", "Mounting terminal...")}
          </div>
        )}
      </section>
    </main>
  );
}

function readTerminalWindowParams(): TerminalWindowParams | null {
  const route = window.location.hash.replace(/^#/, "");
  const queryIndex = route.indexOf("?");
  if (!route.startsWith("/terminal") || queryIndex < 0) return null;

  const params = new URLSearchParams(route.slice(queryIndex + 1));
  const terminalId = params.get("terminalId") || "";
  const backendId = params.get("backendId") || "";
  const projectId = params.get("projectId") || "";
  const slotId = params.get("slotId") || "";
  const paneId = params.get("paneId") || "";
  if (!terminalId || !backendId || !projectId || !slotId) return null;

  return {
    terminalId,
    backendId,
    projectId,
    slotId,
    paneId,
    title: params.get("title") || "Terminal",
    cwd: params.get("cwd") || "",
    projectName: params.get("projectName") || "",
  };
}
