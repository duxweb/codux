import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import type { WebglAddon } from "@xterm/addon-webgl";
import { Terminal as XtermTerminal, type IDisposable, type ITerminalAddon, type ITheme } from "@xterm/xterm";
import { Copy, Maximize2, PanelBottomClose, Plus, RefreshCw, Square, TerminalSquare } from "../icons";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  readAppSettings,
  readTerminalFontSize,
  subscribeAppSettings,
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
import { runtimeTrace } from "../runtimeTrace";
import { broadcastWorkspaceCommand } from "../workspaceCommands";

type TerminalRendererAdapter = {
  write: (data: string | Uint8Array) => void;
  reset: (history?: string) => void;
  clear: () => void;
  focus: () => void;
  fit: () => void;
  refreshTheme: () => void;
  setRenderer: (renderer: TerminalResolvedRenderer) => void;
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
const INTERACTIVE_WRITE_INTERVAL_MS = 16;
const STREAM_ANIMATION_WRITE_INTERVAL_MS = 16;
const STREAM_WRITE_INTERVAL_MS = 24;
const INACTIVE_WRITE_INTERVAL_MS = 200;
const FIT_DEBOUNCE_MS = 80;
const FIT_DIMENSION_EPSILON_PX = 2;
const MAX_QUEUED_WRITE_BYTES = 256 * 1024;
const ACTIVE_WRITE_BYTES_PER_FLUSH = 32 * 1024;
const ACTIVE_WRITE_TEXT_PER_FLUSH = 32 * 1024;
const INACTIVE_WRITE_BYTES_PER_FLUSH = 96 * 1024;
const INACTIVE_WRITE_TEXT_PER_FLUSH = 96 * 1024;
const INTERACTIVE_WRITE_MIN_DELAY_MS = 4;
const STREAM_ANIMATION_WRITE_MIN_DELAY_MS = 4;
const STREAM_WRITE_MIN_DELAY_MS = 10;
const INACTIVE_WRITE_MIN_DELAY_MS = 96;
const OVERFLOW_WRITE_MIN_DELAY_MS = 4;
const LOCAL_INPUT_LOW_LATENCY_MS = 700;
const STREAM_ANIMATION_QUEUE_BYTES = 64 * 1024;
const isWindowsTerminal = isWindowsPlatform();
const TERMINAL_FONT_FAMILY =
  '"Berkeley Mono", "SF Mono", Menlo, Monaco, Consolas, "Liberation Mono", "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei UI", "Microsoft YaHei", "Noto Sans CJK SC", monospace';

type TerminalResolvedRenderer = "webgl";

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
  terminalId,
  writeActive,
  onAdapter,
  onData,
  onResize,
}: {
  className: string;
  terminalId: string;
  writeActive: boolean;
  onAdapter: (adapter: TerminalRendererAdapter | null) => void;
  onData: (data: string) => void;
  onResize: (cols: number, rows: number) => void;
}) {
  const hostRef = useRef<HTMLDivElement | null>(null);
  const writeActiveRef = useRef(writeActive);
  writeActiveRef.current = writeActive;

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    let disposed = false;
    let inputEnabled = false;
    let resizeFrame: number | null = null;
    let fitTimer: number | null = null;
    let pendingFitForce = false;
    let lastFitWidth = -1;
    let lastFitHeight = -1;
    let isSelecting = false;
    let unregisterInput: (() => void) | undefined;
    let textInputAdapter: TerminalTextInputAdapter | null = null;
    let webglAddon: WebglAddon | null = null;
    let webglContextLossDisposable: IDisposable | null = null;
    let webglLoadVersion = 0;
    let writeTimer: number | null = null;
    let writeInFlight = false;
    let lastWriteFlushAt = 0;
    let lastLocalInputAt = 0;
    let queuedText = "";
    const queuedBytes: Uint8Array[] = [];
    let queuedByteLength = 0;

    const terminal = new XtermTerminal({
      allowProposedApi: true,
      allowTransparency: false,
      altClickMovesCursor: false,
      convertEol: isWindowsTerminal,
      cursorBlink: false,
      cursorInactiveStyle: "outline",
      disableStdin: true,
      drawBoldTextInBrightColors: true,
      fontFamily: TERMINAL_FONT_FAMILY,
      fontSize: readTerminalFontSize(),
      lineHeight: 1.25,
      macOptionIsMeta: true,
      rescaleOverlappingGlyphs: true,
      rightClickSelectsWord: false,
      scrollback: 2000,
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
    const serializeAddon = new SerializeAddon();
    const unicode11Addon = new Unicode11Addon();
    const disposables: IDisposable[] = [];
    terminal.loadAddon(fitAddon);
    terminal.loadAddon(serializeAddon);
    terminal.loadAddon(unicode11Addon);
    terminal.unicode.activeVersion = "11";
    terminal.open(host);
    runtimeTrace("terminal-view", `mount rows=${terminal.rows} cols=${terminal.cols}`);

    const traceRendererState = (reason: string) => {
      runtimeTrace(
        "terminal-view",
        `renderer_state reason=${reason} rows=${terminal.rows} cols=${terminal.cols} canvases=${terminal.element?.querySelectorAll("canvas").length ?? 0}`,
      );
    };

    const disposeWebgl = () => {
      webglLoadVersion += 1;
      webglContextLossDisposable?.dispose();
      webglContextLossDisposable = null;
      if (webglAddon) {
        const addon = webglAddon;
        try {
          addon.dispose();
        } finally {
          webglAddon = null;
        }
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
          traceRendererState("webgl-ready");
        })
        .catch((error) => {
          console.warn("failed to load xterm webgl renderer", error);
        });
    };

    const setRenderer = (_renderer: TerminalResolvedRenderer) => {
      enableWebgl();
    };

    setRenderer("webgl");

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

    const hasQueuedWrites = () => queuedBytes.length > 0 || queuedText.length > 0;

    const takeQueuedBytes = (budget: number) => {
      const length = Math.min(Math.max(0, budget), queuedByteLength);
      const chunk = new Uint8Array(length);
      let offset = 0;
      while (offset < length && queuedBytes.length > 0) {
        const source = queuedBytes[0];
        const remaining = length - offset;
        if (source.length <= remaining) {
          chunk.set(source, offset);
          offset += source.length;
          queuedBytes.shift();
          queuedByteLength -= source.length;
          continue;
        }
        chunk.set(source.subarray(0, remaining), offset);
        queuedBytes[0] = source.subarray(remaining);
        queuedByteLength -= remaining;
        offset = length;
      }
      return chunk;
    };

    const takeQueuedText = (budget: number) => {
      const chunk = queuedText.slice(0, budget);
      queuedText = queuedText.slice(chunk.length);
      return chunk;
    };

    const clearQueuedWrites = () => {
      queuedText = "";
      queuedBytes.length = 0;
      queuedByteLength = 0;
    };

    const pendingQueuedWrites = () => {
      const outputs: Array<string | Uint8Array> = [];
      for (const chunk of queuedBytes) {
        if (chunk.length > 0) outputs.push(chunk.slice());
      }
      if (queuedText) outputs.push(queuedText);
      return outputs;
    };

    const scrollToBottomAfterPaint = () => {
      window.requestAnimationFrame(() => {
        if (disposed) return;
        terminal.scrollToBottom();
      });
    };

    const flushQueuedWrites = () => {
      writeTimer = null;
      if (disposed) return;
      if (writeInFlight) return;
      if (!hasQueuedWrites()) return;

      writeInFlight = true;
      lastWriteFlushAt = performance.now();
      const finishWrite = () => {
        writeInFlight = false;
        if (!disposed && hasQueuedWrites()) {
          scheduleQueuedWrite();
        }
      };

      const byteBudget = writeActiveRef.current ? ACTIVE_WRITE_BYTES_PER_FLUSH : INACTIVE_WRITE_BYTES_PER_FLUSH;
      const textBudget = writeActiveRef.current ? ACTIVE_WRITE_TEXT_PER_FLUSH : INACTIVE_WRITE_TEXT_PER_FLUSH;

      if (queuedBytes.length > 0) {
        terminal.write(takeQueuedBytes(byteBudget), finishWrite);
        return;
      }
      terminal.write(takeQueuedText(textBudget), finishWrite);
    };

    const scheduleQueuedWrite = () => {
      if (writeTimer !== null || writeInFlight) return;
      const now = performance.now();
      const lowLatency = now - lastLocalInputAt < LOCAL_INPUT_LOW_LATENCY_MS;
      const queuedLength = queuedByteLength + queuedText.length;
      const animationRate = writeActiveRef.current && queuedLength < STREAM_ANIMATION_QUEUE_BYTES;
      const interval = lowLatency
        ? INTERACTIVE_WRITE_INTERVAL_MS
        : animationRate
          ? STREAM_ANIMATION_WRITE_INTERVAL_MS
        : writeActiveRef.current
          ? STREAM_WRITE_INTERVAL_MS
          : INACTIVE_WRITE_INTERVAL_MS;
      const elapsed = performance.now() - lastWriteFlushAt;
      const minDelay = lowLatency
        ? INTERACTIVE_WRITE_MIN_DELAY_MS
        : animationRate
          ? STREAM_ANIMATION_WRITE_MIN_DELAY_MS
        : writeActiveRef.current
          ? STREAM_WRITE_MIN_DELAY_MS
          : INACTIVE_WRITE_MIN_DELAY_MS;
      const delay =
        queuedByteLength >= MAX_QUEUED_WRITE_BYTES
          ? OVERFLOW_WRITE_MIN_DELAY_MS
          : Math.max(minDelay, interval - elapsed);
      writeTimer = window.setTimeout(flushQueuedWrites, delay);
    };

    const writeQueued = (data: string | Uint8Array) => {
      if (disposed) return;
      if (typeof data === "string") {
        if (!data) return;
        queuedText += data;
      } else {
        if (data.length === 0) return;
        queuedBytes.push(data);
        queuedByteLength += data.length;
      }
      scheduleQueuedWrite();
    };

    const sendTerminalInput = (data: string) => {
      lastLocalInputAt = performance.now();
      onData(data);
    };

    const flushQueuedWritesNow = () => {
      if (writeTimer !== null) {
        window.clearTimeout(writeTimer);
        writeTimer = null;
      }
      flushQueuedWrites();
    };

    const fit = (force = false) => {
      if (disposed || !host.isConnected) return;
      const width = host.clientWidth;
      const height = host.clientHeight;
      if (
        !force &&
        Math.abs(width - lastFitWidth) <= FIT_DIMENSION_EPSILON_PX &&
        Math.abs(height - lastFitHeight) <= FIT_DIMENSION_EPSILON_PX
      ) {
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
        runtimeTrace(
          "terminal-view",
          `fit_resize terminal=${terminalId} force=${force} host=${width}x${height} from=${terminal.cols}x${terminal.rows} to=${cols}x${rows}`,
        );
        terminal.resize(cols, rows);
      }
    };

    const scheduleFit = (force = false) => {
      pendingFitForce = pendingFitForce || force;
      if (resizeFrame !== null) return;
      if (fitTimer !== null) {
        window.clearTimeout(fitTimer);
        fitTimer = null;
      }
      fitTimer = window.setTimeout(() => {
        fitTimer = null;
        resizeFrame = window.requestAnimationFrame(() => {
          const forceFit = pendingFitForce;
          pendingFitForce = false;
          resizeFrame = null;
          fit(forceFit);
        });
      }, force ? 0 : FIT_DEBOUNCE_MS);
    };

    terminal.attachCustomKeyEventHandler((event) => {
      if (!inputEnabled) return false;
      const sequence = terminalControlSequence(event);
      if (sequence) {
        event.preventDefault();
        event.stopPropagation();
        sendTerminalInput(sequence);
        return false;
      }
      return true;
    });

    disposables.push(
      terminal.onData((data) => {
        if (!inputEnabled) return;
        textInputAdapter?.noteNativeData(data);
        sendTerminalInput(data);
      }),
      terminal.onResize(({ cols, rows }) => onResize(cols, rows)),
    );

    const resizeObserver = new ResizeObserver(() => scheduleFit());
    resizeObserver.observe(host);
    scheduleFit(true);

    const adapter: TerminalRendererAdapter = {
      write: (data) => {
        writeQueued(data);
      },
      reset: (history) => {
        runtimeTrace(
          "terminal-view",
          `reset terminal=${terminalId} historyChars=${history?.length ?? 0} rows=${terminal.rows} cols=${terminal.cols}`,
        );
        flushQueuedWritesNow();
        clearQueuedWrites();
        terminal.reset();
        fit(true);
        if (history) {
          terminal.write(history, () => {
            if (disposed) return;
            scrollToBottomAfterPaint();
            traceRendererState("reset-history");
          });
        } else {
          scrollToBottomAfterPaint();
        }
        traceRendererState("reset");
      },
      clear: () => {
        flushQueuedWritesNow();
        clearQueuedWrites();
        terminal.reset();
      },
      focus: () => {
        if (!inputEnabled) return;
        terminal.options.cursorBlink = false;
        terminal.focus();
      },
      fit: () => fit(),
      refreshTheme,
      setRenderer,
      setFontSize: (fontSize) => {
        if (terminal.options.fontSize === fontSize) return;
        terminal.options.fontSize = fontSize;
        scheduleFit(true);
      },
      setInputEnabled: (enabled) => {
        inputEnabled = enabled;
        terminal.options.disableStdin = !enabled;
        terminal.options.cursorBlink = false;
        host.toggleAttribute("data-input-disabled", !enabled);
        if (!enabled) {
          terminal.options.cursorBlink = false;
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
        const pendingOutputs = pendingQueuedWrites();
        try {
          terminalRuntime.saveViewSnapshot(terminalId, serializeAddon.serialize(), pendingOutputs);
        } catch (error) {
          runtimeTrace(
            "terminal-view",
            `serialize_failed terminal=${terminalId} error=${error instanceof Error ? error.message : String(error)}`,
          );
        }
        disposed = true;
        resizeObserver.disconnect();
        if (resizeFrame !== null) {
          window.cancelAnimationFrame(resizeFrame);
          resizeFrame = null;
        }
        if (fitTimer !== null) {
          window.clearTimeout(fitTimer);
          fitTimer = null;
        }
        if (writeTimer !== null) {
          window.clearTimeout(writeTimer);
          writeTimer = null;
        }
        clearQueuedWrites();
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
        const canvasCount = terminal.element?.querySelectorAll("canvas").length ?? 0;
        disposeWebgl();
        for (const disposable of disposables) {
          disposable.dispose();
        }
        terminal.dispose();
        runtimeTrace("terminal-view", `dispose canvases=${canvasCount}`);
      },
    };

    onAdapter(adapter);

    return () => {
      onAdapter(null);
    };
  }, [onAdapter, onData, onResize, terminalId]);

  return <div ref={hostRef} className={className} />;
}

type Props = {
  terminalId: string;
  chrome?: boolean;
  active?: boolean;
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

    let snapshotPending = true;
    let pendingReset:
      | {
          history?: string;
          session: TerminalRuntimeSession;
        }
      | undefined;
    const pendingOutput: Array<string | Uint8Array> = [];
    const writeOrQueueOutput = (data: string | Uint8Array) => {
      if (snapshotPending) {
        pendingOutput.push(data);
        return;
      }
      adapter.write(data);
    };
    const flushPendingOutput = () => {
      snapshotPending = false;
      for (const data of pendingOutput) {
        adapter.write(data);
      }
      pendingOutput.length = 0;
    };
    const resolveSnapshotHistory = (history?: string | null) => {
      const fallback = terminalReplayBuffer(terminalRuntime.getSession(terminalId)) ?? "";
      const pendingHistory = pendingReset?.history;
      let resolved = history ?? fallback;
      if (pendingHistory && pendingHistory.length > resolved.length) {
        resolved = pendingHistory;
      }
      return resolved;
    };
    const completeInitialSnapshot = (source: string, history: string, restoredOutputs: Array<string | Uint8Array> = []) => {
      if (cancelled) return;
      runtimeTrace(
        "terminal-view",
        `snapshot_apply source=${source} terminal=${terminalId} historyChars=${history.length} restoredOutputs=${restoredOutputs.length} queuedOutputs=${pendingOutput.length}`,
      );
      adapter.reset(history);
      const livePending = pendingOutput.splice(0);
      pendingOutput.push(...restoredOutputs, ...livePending);
      if (pendingReset) {
        adapter.setInputEnabled(pendingReset.session.state === "running");
        sessionRef.current = pendingReset.session;
        setSession(pendingReset.session);
      }
      if (pendingOutput.length > 0) {
        const queuedBytes = pendingOutput.reduce((total, item) => total + (typeof item === "string" ? item.length : item.length), 0);
        runtimeTrace(
          "terminal-view",
          `snapshot_flush_pending terminal=${terminalId} queuedOutputs=${pendingOutput.length} queuedBytes=${queuedBytes}`,
        );
      }
      flushPendingOutput();
    };

    const applyEvent = (event: TerminalRuntimeEvent) => {
      if (event.type === "output") {
        writeOrQueueOutput(event.bytes);
        return;
      }
      if (event.type === "reset") {
        if (snapshotPending) {
          pendingReset = {
            history: event.history ?? terminalReplayBuffer(event.session),
            session: event.session,
          };
          adapter.setInputEnabled(event.session.state === "running");
          sessionRef.current = event.session;
          setSession(event.session);
          return;
        }
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
    adapter.setInputEnabled(current?.state === "running");
    terminalRuntime.ensureStarted(terminalId);
    const unsubscribe = terminalRuntime.subscribe(terminalId, applyEvent);
    const viewSnapshot = terminalRuntime.takeViewSnapshot(terminalId);
    if (viewSnapshot) {
      completeInitialSnapshot("view", viewSnapshot.history, viewSnapshot.pendingOutputs);
    } else {
      void terminalRuntime
        .snapshot(terminalId)
        .then((history) => {
          if (cancelled) return;
          if (terminalRuntime.getSession(terminalId)?.id !== terminalId) return;
          completeInitialSnapshot("backend", resolveSnapshotHistory(history));
        })
        .catch(() => {
          if (cancelled) return;
          const latest = terminalRuntime.getSession(terminalId);
          runtimeTrace(
            "terminal-view",
            `snapshot_fallback terminal=${terminalId} replayChars=${terminalReplayBuffer(latest ?? current)?.length ?? 0} queuedOutputs=${pendingOutput.length}`,
          );
          const fallback = terminalReplayBuffer(latest ?? current) ?? "";
          const pendingHistory = pendingReset?.history;
          completeInitialSnapshot(
            "fallback",
            pendingHistory && pendingHistory.length > fallback.length ? pendingHistory : fallback,
          );
        });
    }
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

    if (!active || !canAcceptInput) {
      adapter.setInputEnabled(false);
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
      adapter.setRenderer("webgl");
    };
    applySettings();
    return subscribeAppSettings(applySettings);
  }, [adapterVersion]);

  useEffect(() => {
    const adapter = adapterRef.current;
    if (!adapter) return;
    const updateRendererActivity = () => {
      adapter.setRenderer("webgl");
    };
    updateRendererActivity();
    document.addEventListener("visibilitychange", updateRendererActivity);
    return () => document.removeEventListener("visibilitychange", updateRendererActivity);
  }, [adapterVersion]);

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
        key="xterm"
        className="terminal-host no-drag"
        terminalId={terminalId}
        writeActive={active && canAcceptInput}
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
