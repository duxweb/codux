import { FitAddon } from "@xterm/addon-fit";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import type { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XtermTerminal, type IDisposable, type ITerminalAddon, type ITheme } from "@xterm/xterm";
import { Copy, Maximize2, PanelBottomClose, Plus, RefreshCw, Square, TerminalSquare } from "../icons";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  readAppSettings,
  readTerminalFontSize,
  readTerminalRendererMode,
  subscribeAppSettings,
  type TerminalRendererMode,
} from "../settings";
import { registerTerminalInput } from "../terminal/focus";
import { installTerminalTextInputAdapter, type TerminalTextInputAdapter } from "../terminal/inputAdapter";
import {
  terminalReplayBuffer,
  terminalRuntime,
  type TerminalRuntimeEvent,
  type TerminalRuntimeSession,
} from "../terminal/runtime";
import { terminalControlSequence } from "../terminal/keymap";
import { t } from "../i18n";
import { isWindowsPlatform } from "../platform";
import { broadcastWorkspaceCommand } from "../workspaceCommands";

type TerminalRendererAdapter = {
  write: (data: string | Uint8Array) => void;
  reset: (history?: string) => void;
  clear: () => void;
  focus: () => void;
  blur: () => void;
  fit: () => void;
  refreshTheme: () => void;
  setWebglEnabled: (enabled: boolean) => void;
  setFontSize: (fontSize: number) => void;
  setInputEnabled: (enabled: boolean) => void;
  copySelection: () => Promise<void>;
  pasteClipboard: () => Promise<void>;
  selectAll: () => void;
  hasSelection: () => boolean;
  dispose: () => void;
};

const MIN_COLS = 20;
const MIN_ROWS = 8;
const isWindowsTerminal = isWindowsPlatform();
const TERMINAL_FONT_FAMILY =
  '"Berkeley Mono", "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei UI", "Microsoft YaHei", "Noto Sans CJK SC", monospace';

function resolveTerminalRenderer(mode: TerminalRendererMode) {
  if (mode === "auto") return isWindowsTerminal ? "webgl" : "dom";
  return mode;
}

function shouldUseWebgl(mode: TerminalRendererMode, active: boolean) {
  return active && resolveTerminalRenderer(mode) === "webgl";
}

function cssVar(style: CSSStyleDeclaration, name: string, fallback: string) {
  return style.getPropertyValue(name).trim() || fallback;
}

function xtermTheme(host: HTMLElement): ITheme {
  const style = window.getComputedStyle(host);
  return {
    background: cssVar(style, "--terminal-bg", "#101010"),
    foreground: cssVar(style, "--terminal-fg", "#d8dee9"),
    cursor: cssVar(style, "--terminal-cursor", "#d8dee9"),
    cursorAccent: cssVar(style, "--terminal-bg", "#101010"),
    selectionBackground: "rgba(120, 160, 255, 0.28)",
    black: cssVar(style, "--terminal-black", "#1f2328"),
    red: cssVar(style, "--terminal-red", "#ff6b6b"),
    green: cssVar(style, "--terminal-green", "#7bd88f"),
    yellow: cssVar(style, "--terminal-yellow", "#f7d774"),
    blue: cssVar(style, "--terminal-blue", "#82aaff"),
    magenta: cssVar(style, "--terminal-magenta", "#c792ea"),
    cyan: cssVar(style, "--terminal-cyan", "#89ddff"),
    white: cssVar(style, "--terminal-white", "#d8dee9"),
    brightBlack: cssVar(style, "--terminal-bright-black", "#6b7280"),
    brightRed: cssVar(style, "--terminal-bright-red", "#ff8787"),
    brightGreen: cssVar(style, "--terminal-bright-green", "#9be7aa"),
    brightYellow: cssVar(style, "--terminal-bright-yellow", "#ffe08a"),
    brightBlue: cssVar(style, "--terminal-bright-blue", "#9bbcff"),
    brightMagenta: cssVar(style, "--terminal-bright-magenta", "#d7a8ff"),
    brightCyan: cssVar(style, "--terminal-bright-cyan", "#a6eaff"),
    brightWhite: cssVar(style, "--terminal-bright-white", "#ffffff"),
  };
}

function terminalTextarea(host: HTMLElement) {
  return host.querySelector(".xterm-helper-textarea") as HTMLTextAreaElement | null;
}

type XtermInternalSelection = {
  _removeMouseDownListeners?: () => void;
};

type XtermWithSelectionInternals = XtermTerminal & {
  _core?: {
    _selectionService?: XtermInternalSelection;
  };
};

function XtermRenderer({
  className,
  webglActive,
  onAdapter,
  onData,
  onResize,
}: {
  className: string;
  webglActive: boolean;
  onAdapter: (adapter: TerminalRendererAdapter | null) => void;
  onData: (data: string) => void;
  onResize: (cols: number, rows: number) => void;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    let inputEnabled = false;
    let resizeFrame: number | null = null;
    let pendingFitForce = false;
    let lastFitWidth = -1;
    let lastFitHeight = -1;
    let isSelecting = false;
    let unregisterInput: (() => void) | undefined;
    let textInputAdapter: TerminalTextInputAdapter | null = null;
    let webglAddon: WebglAddon | null = null;
    let webglContextLossDisposable: IDisposable | null = null;
    let webglLoadVersion = 0;

    const terminal = new XtermTerminal({
      allowProposedApi: true,
      allowTransparency: false,
      altClickMovesCursor: false,
      convertEol: isWindowsTerminal,
      cursorBlink: true,
      cursorInactiveStyle: "outline",
      disableStdin: true,
      drawBoldTextInBrightColors: true,
      fontFamily: TERMINAL_FONT_FAMILY,
      fontSize: readTerminalFontSize(),
      lineHeight: 1.25,
      macOptionIsMeta: true,
      rescaleOverlappingGlyphs: true,
      rightClickSelectsWord: false,
      scrollback: 5000,
      showCursorImmediately: true,
      theme: xtermTheme(host),
      windowsPty: isWindowsTerminal
        ? {
            backend: "conpty",
            buildNumber: 26200,
          }
        : undefined,
    });
    const fitAddon = new FitAddon();
    const unicode11Addon = new Unicode11Addon();
    const disposables: IDisposable[] = [];
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(unicode11Addon);
    terminal.unicode.activeVersion = "11";
    terminal.open(host);

    const disposeWebgl = () => {
      webglLoadVersion += 1;
      webglContextLossDisposable?.dispose();
      webglContextLossDisposable = null;
      if (!webglAddon) return;
      try {
        webglAddon.dispose();
      } finally {
        webglAddon = null;
      }
    };

    const enableWebgl = () => {
      if (disposed || webglAddon) return;
      const version = ++webglLoadVersion;
      void import("@xterm/addon-webgl")
        .then(({ WebglAddon }) => {
          if (disposed || version !== webglLoadVersion || webglAddon) return;
          const nextWebglAddon = new WebglAddon();
          terminal.loadAddon(nextWebglAddon as ITerminalAddon);
          webglContextLossDisposable = nextWebglAddon.onContextLoss(() => {
            console.warn("xterm webgl context lost; falling back to DOM renderer");
            disposeWebgl();
          });
          webglAddon = nextWebglAddon;
        })
        .catch((error) => {
          console.warn("failed to load xterm webgl renderer", error);
        });
    };

    const setWebglEnabled = (enabled: boolean) => {
      if (enabled) {
        enableWebgl();
        return;
      }
      disposeWebgl();
    };

    setWebglEnabled(shouldUseWebgl(readTerminalRendererMode(), webglActive && document.visibilityState !== "hidden"));

    const releaseSelectionDrag = (reason: string) => {
      if (!isSelecting) return;
      isSelecting = false;
      const selection = (terminal as XtermWithSelectionInternals)._core?._selectionService;
      selection?._removeMouseDownListeners?.();
      void reason;
    };

    const markSelectionStart = (event: MouseEvent) => {
      if (event.button === 0 && host.contains(event.target as Node | null)) {
        isSelecting = true;
      }
    };
    const releaseSelectionAfterMouseEnd = () => releaseSelectionDrag("pointer-end");
    const releaseSelectionAfterWindowBlur = () => releaseSelectionDrag("window-blur");
    const releaseSelectionWhenMouseIsUp = (event: MouseEvent | PointerEvent) => {
      if (event.buttons !== 0) return;
      releaseSelectionDrag("move-without-button");
    };
    const focusTerminalFromPointer = (event: PointerEvent) => {
      if (event.button !== 0 || !inputEnabled) return;
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest("[data-terminal-control]")) return;
      terminal.focus();
    };

    host.addEventListener("mousedown", markSelectionStart, true);
    host.addEventListener("pointerdown", focusTerminalFromPointer, true);
    window.addEventListener("pointerup", releaseSelectionAfterMouseEnd, true);
    window.addEventListener("pointercancel", releaseSelectionAfterMouseEnd, true);
    window.addEventListener("mouseup", releaseSelectionAfterMouseEnd, true);
    window.addEventListener("pointermove", releaseSelectionWhenMouseIsUp, true);
    window.addEventListener("mousemove", releaseSelectionWhenMouseIsUp, true);
    window.addEventListener("blur", releaseSelectionAfterWindowBlur);

    const textarea = terminal.textarea ?? terminalTextarea(host);
    if (textarea) {
      textarea.dataset.coduxTerminalInput = "true";
      unregisterInput = registerTerminalInput({
        host,
        textarea,
        blur: () => {
          terminal.blur();
          host.classList.remove("focused");
        },
      });
      textInputAdapter = installTerminalTextInputAdapter({
        textarea,
        isEnabled: () => inputEnabled,
        write: onData,
      });
    }

    const refreshTheme = () => {
      if (disposed || !host.isConnected) return;
      terminal.options.theme = xtermTheme(host);
    };

    const fit = (force = false) => {
      if (disposed || !host.isConnected) return;
      const width = host.clientWidth;
      const height = host.clientHeight;
      if (!force && width === lastFitWidth && height === lastFitHeight) {
        return;
      }
      lastFitWidth = width;
      lastFitHeight = height;
      const proposed = fitAddon.proposeDimensions();
      if (!proposed) return;
      const proposedCols = Math.floor(proposed.cols);
      const proposedRows = Math.floor(proposed.rows);
      if (!Number.isFinite(proposedCols) || !Number.isFinite(proposedRows)) return;
      const cols = Math.max(MIN_COLS, proposedCols);
      const rows = Math.max(MIN_ROWS, proposedRows);
      if (terminal.cols !== cols || terminal.rows !== rows) {
        terminal.resize(cols, rows);
      }
    };

    const scheduleFit = (force = false) => {
      pendingFitForce = pendingFitForce || force;
      if (resizeFrame !== null) return;
      resizeFrame = window.requestAnimationFrame(() => {
        const forceFit = pendingFitForce;
        pendingFitForce = false;
        resizeFrame = null;
        fit(forceFit);
      });
    };

    terminal.attachCustomKeyEventHandler((event) => {
      if (!inputEnabled) return false;
      const sequence = terminalControlSequence(event);
      if (sequence) {
        event.preventDefault();
        event.stopPropagation();
        onData(sequence);
        return false;
      }
      return true;
    });

    disposables.push(
      terminal.onData((data) => {
        if (!inputEnabled) return;
        textInputAdapter?.noteNativeData(data);
        onData(data);
      }),
      terminal.onResize(({ cols, rows }) => onResize(cols, rows)),
    );

    const resizeObserver = new ResizeObserver(() => scheduleFit());
    resizeObserver.observe(host);
    scheduleFit(true);

    const adapter: TerminalRendererAdapter = {
      write: (data) => {
        terminal.write(data);
      },
      reset: (history) => {
        terminal.reset();
        if (history) terminal.write(history);
        scheduleFit(true);
      },
      clear: () => {
        terminal.reset();
      },
      focus: () => {
        if (inputEnabled) terminal.focus();
      },
      blur: () => {
        terminal.blur();
        host.classList.remove("focused");
      },
      fit: () => fit(),
      refreshTheme,
      setWebglEnabled,
      setFontSize: (fontSize) => {
        if (terminal.options.fontSize === fontSize) return;
        terminal.options.fontSize = fontSize;
        scheduleFit(true);
      },
      setInputEnabled: (enabled) => {
        inputEnabled = enabled;
        terminal.options.disableStdin = !enabled;
        host.toggleAttribute("data-input-disabled", !enabled);
        if (!enabled) {
          terminal.blur();
          host.classList.remove("focused");
        }
      },
      copySelection: async () => {
        const selection = terminal.getSelection();
        if (!selection) return;
        await navigator.clipboard.writeText(selection);
        terminal.clearSelection();
      },
      pasteClipboard: async () => {
        if (!inputEnabled) return;
        const data = await navigator.clipboard.readText();
        if (!data) return;
        terminal.paste(data);
      },
      selectAll: () => {
        terminal.selectAll();
      },
      hasSelection: () => Boolean(terminal.getSelection()),
      dispose: () => {
        if (disposed) return;
        disposed = true;
        resizeObserver.disconnect();
        if (resizeFrame !== null) {
          window.cancelAnimationFrame(resizeFrame);
          resizeFrame = null;
        }
        host.removeEventListener("mousedown", markSelectionStart, true);
        host.removeEventListener("pointerdown", focusTerminalFromPointer, true);
        window.removeEventListener("pointerup", releaseSelectionAfterMouseEnd, true);
        window.removeEventListener("pointercancel", releaseSelectionAfterMouseEnd, true);
        window.removeEventListener("mouseup", releaseSelectionAfterMouseEnd, true);
        window.removeEventListener("pointermove", releaseSelectionWhenMouseIsUp, true);
        window.removeEventListener("mousemove", releaseSelectionWhenMouseIsUp, true);
        window.removeEventListener("blur", releaseSelectionAfterWindowBlur);
        textInputAdapter?.dispose();
        unregisterInput?.();
        disposeWebgl();
        for (const disposable of disposables) {
          disposable.dispose();
        }
        terminal.dispose();
      },
    };

    onAdapter(adapter);

    return () => {
      onAdapter(null);
    };
  }, [onAdapter, onData, onResize, webglActive]);

  return <div ref={hostRef} className={className} />;
}

type Props = {
  terminalId: string;
  chrome?: boolean;
  active?: boolean;
  webglActive?: boolean;
  focusRequest?: number;
  onClose?: () => void;
  onDetach?: () => void;
  onDisposed?: () => void;
};

type ContextMenuState = {
  x: number;
  y: number;
  hasSelection: boolean;
};

export function TerminalView({
  terminalId,
  chrome = true,
  active = true,
  webglActive = active,
  focusRequest = 0,
  onClose,
  onDetach,
  onDisposed,
}: Props) {
  const adapterRef = useRef<TerminalRendererAdapter | null>(null);
  const resizeFrameRef = useRef<number | null>(null);
  const onDisposedRef = useRef(onDisposed);
  const sessionRef = useRef<TerminalRuntimeSession | undefined>(terminalRuntime.getSession(terminalId));
  const shellRef = useRef<HTMLElement | null>(null);
  const [adapterVersion, setAdapterVersion] = useState(0);
  const [contextMenu, setContextMenu] = useState<ContextMenuState | null>(null);
  const [session, setSession] = useState<TerminalRuntimeSession | undefined>(() => sessionRef.current);
  const canAcceptInput = session?.state === "running" && Boolean(session.backendId || !window.__TAURI_INTERNALS__);
  onDisposedRef.current = onDisposed;

  const setAdapter = useCallback((adapter: TerminalRendererAdapter | null) => {
    if (adapterRef.current && adapterRef.current !== adapter) {
      adapterRef.current.dispose();
    }
    adapterRef.current = adapter;
    setAdapterVersion((current) => current + 1);
  }, []);

  const writeTerminalInput = useCallback(
    (data: string) => {
      if (sessionRef.current?.state !== "running") return;
      terminalRuntime.write(terminalId, data);
    },
    [terminalId],
  );

  const resizeTerminal = useCallback(
    (cols: number, rows: number) => {
      terminalRuntime.resize(terminalId, cols, rows);
    },
    [terminalId],
  );

  const fitAndResize = useCallback(() => {
    adapterRef.current?.fit();
  }, []);

  const scheduleFitAndResize = useCallback(() => {
    if (resizeFrameRef.current !== null) {
      window.cancelAnimationFrame(resizeFrameRef.current);
    }
    resizeFrameRef.current = window.requestAnimationFrame(() => {
      resizeFrameRef.current = null;
      fitAndResize();
    });
  }, [fitAndResize]);

  useEffect(() => {
    const current = terminalRuntime.getSession(terminalId);
    sessionRef.current = current;
    setSession(current);
    terminalRuntime.ensureStarted(terminalId);
  }, [terminalId]);

  useEffect(() => {
    const adapter = adapterRef.current;
    if (!adapter) return;

    const applyEvent = (event: TerminalRuntimeEvent) => {
      if (event.type === "output") {
        adapter.write(event.bytes);
        return;
      }
      if (event.type === "reset") {
        adapter.reset(event.history ?? terminalReplayBuffer(event.session));
        adapter.setInputEnabled(event.session.state === "running");
        sessionRef.current = event.session;
        setSession(event.session);
        return;
      }
      if (event.type === "state") {
        adapter.setInputEnabled(event.session.state === "running");
        sessionRef.current = event.session;
        setSession(event.session);
        return;
      }
      if (event.type === "closed") {
        adapter.setInputEnabled(false);
        sessionRef.current = undefined;
        adapter.clear();
        setSession(undefined);
      }
    };

    const current = terminalRuntime.getSession(terminalId);
    sessionRef.current = current;
    setSession(current);
    let cancelled = false;
    void terminalRuntime
      .snapshot(terminalId)
      .then((history) => {
        if (cancelled) return;
        if (terminalRuntime.getSession(terminalId)?.id !== terminalId) return;
        adapter.reset(history ?? terminalReplayBuffer(terminalRuntime.getSession(terminalId)));
      })
      .catch(() => {
        if (cancelled) return;
        adapter.reset(terminalReplayBuffer(current));
      });
    adapter.setInputEnabled(current?.state === "running");
    terminalRuntime.ensureStarted(terminalId);
    const unsubscribe = terminalRuntime.subscribe(terminalId, applyEvent);
    const fitFrame = window.requestAnimationFrame(() => {
      fitAndResize();
    });

    return () => {
      cancelled = true;
      window.cancelAnimationFrame(fitFrame);
      unsubscribe();
      if (resizeFrameRef.current !== null) {
        window.cancelAnimationFrame(resizeFrameRef.current);
        resizeFrameRef.current = null;
      }
    };
  }, [adapterVersion, fitAndResize, terminalId]);

  useEffect(() => {
    return () => {
      adapterRef.current?.dispose();
      adapterRef.current = null;
      onDisposedRef.current?.();
    };
  }, []);

  useEffect(() => {
    const adapter = adapterRef.current;
    if (!adapter) return;
    fitAndResize();

    if (!active || !canAcceptInput) {
      adapter.setInputEnabled(false);
      adapter.blur();
      return;
    }
    adapter.setInputEnabled(true);

    const frame = window.requestAnimationFrame(() => {
      adapter.focus();
      fitAndResize();
    });

    return () => window.cancelAnimationFrame(frame);
  }, [active, adapterVersion, canAcceptInput, fitAndResize, focusRequest]);

  useEffect(() => {
    const applySettings = () => {
      const adapter = adapterRef.current;
      if (!adapter) return;
      const settings = readAppSettings();
      adapter.setFontSize(readTerminalFontSize(settings));
      adapter.refreshTheme();
      adapter.setWebglEnabled(
        shouldUseWebgl(readTerminalRendererMode(settings), webglActive && active && canAcceptInput),
      );
    };
    applySettings();
    return subscribeAppSettings(applySettings);
  }, [active, adapterVersion, canAcceptInput, webglActive]);

  useEffect(() => {
    const adapter = adapterRef.current;
    if (!adapter) return;
    const updateRendererActivity = () => {
      adapter.setWebglEnabled(
        shouldUseWebgl(
          readTerminalRendererMode(),
          webglActive && active && canAcceptInput && document.visibilityState !== "hidden",
        ),
      );
    };
    updateRendererActivity();
    document.addEventListener("visibilitychange", updateRendererActivity);
    return () => document.removeEventListener("visibilitychange", updateRendererActivity);
  }, [active, adapterVersion, canAcceptInput, webglActive]);

  useEffect(() => {
    window.addEventListener("resize", scheduleFitAndResize);
    const visualViewport = window.visualViewport;
    visualViewport?.addEventListener("resize", scheduleFitAndResize);
    return () => {
      window.removeEventListener("resize", scheduleFitAndResize);
      visualViewport?.removeEventListener("resize", scheduleFitAndResize);
    };
  }, [scheduleFitAndResize]);

  const close = () => {
    onClose?.();
  };

  const openContextMenu = (event: React.MouseEvent<HTMLElement>) => {
    event.preventDefault();
    event.stopPropagation();
    const adapter = adapterRef.current;
    if (!adapter) return;
    const rect = shellRef.current?.getBoundingClientRect();
    const menuWidth = 174;
    const menuHeight = 184;
    const rawX = event.clientX - (rect?.left ?? 0);
    const rawY = event.clientY - (rect?.top ?? 0);
    const maxX = Math.max(8, (rect?.width ?? window.innerWidth) - menuWidth - 8);
    const maxY = Math.max(8, (rect?.height ?? window.innerHeight) - menuHeight - 8);
    setContextMenu({
      x: Math.min(Math.max(8, rawX), maxX),
      y: Math.min(Math.max(8, rawY), maxY),
      hasSelection: adapter.hasSelection(),
    });
  };

  const runContextAction = async (action: "copy" | "paste" | "clear" | "selectAll" | "split" | "tab") => {
    const adapter = adapterRef.current;
    setContextMenu(null);
    if (!adapter) return;
    if (action === "copy") {
      await adapter.copySelection();
      return;
    }
    if (action === "paste") {
      await adapter.pasteClipboard();
      return;
    }
    if (action === "clear") {
      adapter.clear();
      return;
    }
    if (action === "selectAll") {
      adapter.selectAll();
      return;
    }
    if (action === "split") {
      broadcastWorkspaceCommand({ type: "add-top-terminal-split" });
      return;
    }
    broadcastWorkspaceCommand({ type: "add-bottom-terminal-tab" });
  };

  useEffect(() => {
    if (!contextMenu) return;
    const closeMenu = () => setContextMenu(null);
    const closeFromKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") closeMenu();
    };
    window.addEventListener("pointerdown", closeMenu, true);
    window.addEventListener("wheel", closeMenu, true);
    window.addEventListener("resize", closeMenu);
    window.addEventListener("keydown", closeFromKey, true);
    return () => {
      window.removeEventListener("pointerdown", closeMenu, true);
      window.removeEventListener("wheel", closeMenu, true);
      window.removeEventListener("resize", closeMenu);
      window.removeEventListener("keydown", closeFromKey, true);
    };
  }, [contextMenu]);

  return (
    <section
      ref={shellRef}
      className={`terminal-view ${chrome ? "with-chrome" : "bare"}`}
      data-renderer="xterm"
      onContextMenu={openContextMenu}
    >
      {chrome && (
        <div className="terminal-view-header">
          <div className="terminal-title">
            <TerminalSquare size={16} />
            <span>{session?.title ?? "Local Shell"}</span>
            <small>{session?.cwd ?? ""}</small>
          </div>
          <div className="terminal-view-actions">
            <span>{session?.state ?? "closed"}</span>
            <button onClick={() => terminalRuntime.interrupt(terminalId)} title="Interrupt">
              <Square size={13} />
            </button>
            <button onClick={onDetach} title="Detach">
              <Maximize2 size={13} />
            </button>
            <button onClick={close} title="Close">
              <PanelBottomClose size={14} />
            </button>
          </div>
        </div>
      )}
      <XtermRenderer
        className="terminal-host no-drag"
        webglActive={webglActive && active && canAcceptInput}
        onAdapter={setAdapter}
        onData={writeTerminalInput}
        onResize={resizeTerminal}
      />
      {contextMenu && (
        <div
          className="terminal-context-menu no-drag"
          data-native-context-menu
          style={{ left: contextMenu.x, top: contextMenu.y }}
          onPointerDown={(event) => event.stopPropagation()}
          onContextMenu={(event) => event.preventDefault()}
        >
          <TerminalContextMenuItem
            icon={Copy}
            label={t("terminal.copy")}
            disabled={!contextMenu.hasSelection}
            onPress={() => void runContextAction("copy")}
          />
          <TerminalContextMenuItem label={t("terminal.paste")} onPress={() => void runContextAction("paste")} />
          <TerminalContextMenuSeparator />
          <TerminalContextMenuItem
            icon={RefreshCw}
            label={t("terminal.clear")}
            onPress={() => void runContextAction("clear")}
          />
          <TerminalContextMenuItem label={t("terminal.selectAll")} onPress={() => void runContextAction("selectAll")} />
          <TerminalContextMenuSeparator />
          <TerminalContextMenuItem
            icon={Plus}
            label={t("terminal.split")}
            onPress={() => void runContextAction("split")}
          />
          <TerminalContextMenuItem
            icon={Plus}
            label={t("terminal.newTab")}
            onPress={() => void runContextAction("tab")}
          />
        </div>
      )}
    </section>
  );
}

function TerminalContextMenuItem({
  icon: Icon,
  label,
  disabled,
  onPress,
}: {
  icon?: typeof Copy;
  label: string;
  disabled?: boolean;
  onPress: () => void;
}) {
  return (
    <button type="button" disabled={disabled} className="terminal-context-menu-item" onClick={onPress}>
      <span className="terminal-context-menu-icon">{Icon ? <Icon size={13} strokeWidth={2.1} /> : null}</span>
      <span className="truncate">{label}</span>
    </button>
  );
}

function TerminalContextMenuSeparator() {
  return <div className="terminal-context-menu-separator" />;
}
