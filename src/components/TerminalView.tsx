import { FitAddon } from "@xterm/addon-fit";
import { SerializeAddon } from "@xterm/addon-serialize";
import { Unicode11Addon } from "@xterm/addon-unicode11";
import type { WebglAddon as XtermWebglAddon } from "@xterm/addon-webgl";
import { Terminal as XtermTerminal, type IDisposable, type ITheme } from "@xterm/xterm";
import { Copy, Maximize2, PanelBottomClose, Plus, RefreshCw, Square, TerminalSquare } from "../icons";
import { useCallback, useEffect, useRef, useState } from "react";
import {
  readAppSettings,
  readTerminalFontSize,
  readTerminalFontFamily,
  readTerminalScrollbackLines,
  subscribeAppSettings,
} from "../settings";
import {
  terminalReplayBuffer,
  terminalRuntime,
  type TerminalRuntimeEvent,
  type TerminalRuntimeSession,
} from "../terminal/runtime";
import { installTerminalTextInputAdapter } from "../terminal/inputAdapter";
import {
  isPlainTerminalCharacterEvent,
  terminalDeleteSequence,
  terminalLineNavigationSequence,
  terminalModifiedEnterSequence,
  terminalWordNavigationSequence,
} from "../terminal/keymap";
import { t } from "../i18n";
import { isMacPlatform, isWindowsPlatform } from "../platform";
import { runtimeTrace } from "../runtimeTrace";
import { broadcastWorkspaceCommand } from "../workspaceCommands";

type TerminalRendererAdapter = {
  write: (data: string | Uint8Array) => void;
  reset: (history?: string) => void;
  clear: () => void;
  focus: () => void;
  fit: () => void;
  refreshTheme: () => void;
  setFontSize: (fontSize: number) => void;
  setFontFamily: (fontFamily: string) => void;
  setScrollback: (lines: number) => void;
  setInputEnabled: (enabled: boolean) => void;
  copySelection: () => Promise<void>;
  pasteClipboard: () => Promise<void>;
  selectAll: () => void;
  hasSelection: () => boolean;
  dispose: () => void;
};

const INTERACTIVE_WRITE_INTERVAL_MS = 16;
const STREAM_ANIMATION_WRITE_INTERVAL_MS = 16;
const STREAM_WRITE_INTERVAL_MS = 24;
const INACTIVE_WRITE_INTERVAL_MS = 200;
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
const isMacTerminal = isMacPlatform();

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
    let pendingFitForce = false;
    let lastFitWidth = -1;
    let lastFitHeight = -1;
    let isSelectingFromPointer = false;
    let writeTimer: number | null = null;
    let writeInFlight = false;
    let lastWriteFlushAt = 0;
    let lastLocalInputAt = 0;
    let queuedText = "";
    let stickToBottom = true;
    let preservingBottom = false;
    let bottomPreserveFrame: number | null = null;
    let bottomPreserveFramesRemaining = 0;
    let userScrollIntentUntil = 0;
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
      fontFamily: readTerminalFontFamily(),
      fontSize: readTerminalFontSize(),
      lineHeight: 1.25,
      macOptionIsMeta: true,
      rescaleOverlappingGlyphs: true,
      rightClickSelectsWord: false,
      scrollback: readTerminalScrollbackLines(),
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
    let webglAddon: XtermWebglAddon | null = null;
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
        `renderer_state reason=${reason} renderer=${webglAddon ? "webgl" : "default"} rows=${terminal.rows} cols=${terminal.cols} canvases=${terminal.element?.querySelectorAll("canvas").length ?? 0}`,
      );
    };

    const loadWebglRenderer = () => {
      if (isMacTerminal) {
        runtimeTrace("terminal-view", `webgl_disabled terminal=${terminalId} reason=macos-wkwebview-idle-cpu`);
        return;
      }
      void import("@xterm/addon-webgl")
        .then(({ WebglAddon }) => {
          if (disposed) return;
          try {
            const addon = new WebglAddon();
            const contextLossDisposable = addon.onContextLoss(() => {
              runtimeTrace("terminal-view", `webgl_context_loss terminal=${terminalId}`);
              addon.dispose();
              contextLossDisposable.dispose();
              if (webglAddon === addon) {
                webglAddon = null;
              }
            });
            terminal.loadAddon(addon);
            if (disposed) {
              addon.dispose();
              contextLossDisposable.dispose();
              return;
            }
            webglAddon = addon;
            traceRendererState("webgl-ready");
          } catch (error) {
            runtimeTrace(
              "terminal-view",
              `webgl_unavailable terminal=${terminalId} error=${error instanceof Error ? error.message : String(error)}`,
            );
          }
        })
        .catch((error) => {
          runtimeTrace(
            "terminal-view",
            `webgl_unavailable terminal=${terminalId} error=${error instanceof Error ? error.message : String(error)}`,
          );
        });
    };

    loadWebglRenderer();
    traceRendererState("ready");

    const releaseSelectionDrag = () => {
      if (!isSelectingFromPointer) return;
      isSelectingFromPointer = false;
      const selection = (terminal as XtermWithSelectionInternals)._core?._selectionService;
      selection?._removeMouseDownListeners?.();
    };
    const markSelectionStart = (event: MouseEvent) => {
      if (event.button === 0 && host.contains(event.target as Node | null)) {
        isSelectingFromPointer = true;
      }
    };
    const releaseSelectionWhenMouseEnds = () => releaseSelectionDrag();
    const releaseSelectionWhenButtonIsUp = (event: MouseEvent | PointerEvent) => {
      if (event.buttons === 0) releaseSelectionDrag();
    };

    host.addEventListener("mousedown", markSelectionStart, true);
    window.addEventListener("pointerup", releaseSelectionWhenMouseEnds, true);
    window.addEventListener("pointercancel", releaseSelectionWhenMouseEnds, true);
    window.addEventListener("mouseup", releaseSelectionWhenMouseEnds, true);
    window.addEventListener("pointermove", releaseSelectionWhenButtonIsUp, true);
    window.addEventListener("mousemove", releaseSelectionWhenButtonIsUp, true);
    window.addEventListener("blur", releaseSelectionWhenMouseEnds);

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
        preservingBottom = true;
        terminal.scrollToBottom();
        preservingBottom = false;
        stickToBottom = true;
      });
    };
    const isScrolledToBottom = () => {
      const buffer = terminal.buffer.active;
      return buffer.baseY - buffer.viewportY <= 1;
    };
    const keepBottomNow = () => {
      if (!stickToBottom) return;
      preservingBottom = true;
      terminal.scrollToBottom();
      preservingBottom = false;
    };
    const scheduleKeepBottom = (frames = 3) => {
      if (!stickToBottom) return;
      bottomPreserveFramesRemaining = Math.max(bottomPreserveFramesRemaining, frames);
      if (bottomPreserveFrame !== null) return;
      const preserve = () => {
        bottomPreserveFrame = null;
        if (disposed) return;
        keepBottomNow();
        bottomPreserveFramesRemaining -= 1;
        if (bottomPreserveFramesRemaining > 0) {
          bottomPreserveFrame = window.requestAnimationFrame(preserve);
        }
      };
      bottomPreserveFrame = window.requestAnimationFrame(preserve);
    };
    const markUserScrollIntent = () => {
      userScrollIntentUntil = performance.now() + 1200;
    };
    const markScrollbarScrollIntent = (event: MouseEvent) => {
      const target = event.target instanceof Element ? event.target : null;
      if (target?.closest(".xterm-viewport")) {
        markUserScrollIntent();
      }
    };

    host.addEventListener("wheel", markUserScrollIntent, true);
    host.addEventListener("mousedown", markScrollbarScrollIntent, true);

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
    const textInputAdapter = terminal.textarea
      ? installTerminalTextInputAdapter({
          textarea: terminal.textarea,
          isEnabled: () => inputEnabled,
          write: sendTerminalInput,
        })
      : null;

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
      const previousCols = terminal.cols;
      const previousRows = terminal.rows;
      stickToBottom = stickToBottom || isScrolledToBottom();
      fitAddon.fit();
      scheduleKeepBottom(4);
      if (terminal.cols !== previousCols || terminal.rows !== previousRows) {
        runtimeTrace(
          "terminal-view",
          `fit_resize terminal=${terminalId} force=${force} host=${width}x${height} from=${previousCols}x${previousRows} to=${terminal.cols}x${terminal.rows}`,
        );
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
      const sequence =
        terminalLineNavigationSequence(event, { isMac: isMacTerminal }) ??
        terminalWordNavigationSequence(event) ??
        terminalDeleteSequence(event, { isMac: isMacTerminal }) ??
        terminalModifiedEnterSequence(event, { isMac: isMacTerminal });
      if (!inputEnabled) {
        return !sequence && isPlainTerminalCharacterEvent(event);
      }
      if (!sequence) return true;
      event.preventDefault();
      if (event.type === "keydown") {
        sendTerminalInput(sequence);
      }
      return false;
    });

    disposables.push(
      terminal.onData((data) => {
        if (!inputEnabled) return;
        textInputAdapter?.noteNativeData(data);
        sendTerminalInput(data);
      }),
      terminal.onResize(({ cols, rows }) => onResize(cols, rows)),
    );

    disposables.push(
      terminal.onScroll(() => {
        if (preservingBottom) return;
        if (performance.now() <= userScrollIntentUntil) {
          stickToBottom = isScrolledToBottom();
          return;
        }
        scheduleKeepBottom(2);
      }),
    );

    const resizeObserver = new ResizeObserver(() => {
      scheduleKeepBottom(4);
      scheduleFit();
    });
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
      setFontSize: (fontSize) => {
        if (terminal.options.fontSize === fontSize) return;
        terminal.options.fontSize = fontSize;
        scheduleFit(true);
      },
      setFontFamily: (fontFamily) => {
        if (terminal.options.fontFamily === fontFamily) return;
        terminal.options.fontFamily = fontFamily;
        scheduleFit(true);
      },
      setScrollback: (lines) => {
        if (terminal.options.scrollback === lines) return;
        terminal.options.scrollback = lines;
      },
      setInputEnabled: (enabled) => {
        inputEnabled = enabled;
        terminal.options.disableStdin = !enabled;
        terminal.options.cursorBlink = false;
        host.toggleAttribute("data-input-disabled", !enabled);
        if (!enabled) terminal.blur();
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
        if (bottomPreserveFrame !== null) {
          window.cancelAnimationFrame(bottomPreserveFrame);
          bottomPreserveFrame = null;
        }
        if (writeTimer !== null) {
          window.clearTimeout(writeTimer);
          writeTimer = null;
        }
        clearQueuedWrites();
        textInputAdapter?.dispose();
        host.removeEventListener("mousedown", markSelectionStart, true);
        host.removeEventListener("wheel", markUserScrollIntent, true);
        host.removeEventListener("mousedown", markScrollbarScrollIntent, true);
        window.removeEventListener("pointerup", releaseSelectionWhenMouseEnds, true);
        window.removeEventListener("pointercancel", releaseSelectionWhenMouseEnds, true);
        window.removeEventListener("mouseup", releaseSelectionWhenMouseEnds, true);
        window.removeEventListener("pointermove", releaseSelectionWhenButtonIsUp, true);
        window.removeEventListener("mousemove", releaseSelectionWhenButtonIsUp, true);
        window.removeEventListener("blur", releaseSelectionWhenMouseEnds);
        for (const disposable of disposables) {
          disposable.dispose();
        }
        terminal.dispose();
        runtimeTrace("terminal-view", "dispose");
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
  const onDisposedRef = useRef(onDisposed);
  const sessionRef = useRef<TerminalRuntimeSession | undefined>(terminalRuntime.getSession(terminalId));
  const snapshotRunRef = useRef(0);
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
    const snapshotRun = snapshotRunRef.current + 1;
    snapshotRunRef.current = snapshotRun;
    const isCurrentSnapshotRun = () => !cancelled && snapshotRunRef.current === snapshotRun;
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
          if (!isCurrentSnapshotRun()) return;
          if (terminalRuntime.getSession(terminalId)?.id !== terminalId) return;
          completeInitialSnapshot("backend", resolveSnapshotHistory(history));
        })
        .catch(() => {
          if (!isCurrentSnapshotRun()) return;
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
      if (snapshotRunRef.current === snapshotRun) {
        snapshotRunRef.current += 1;
      }
      window.cancelAnimationFrame(fitFrame);
      unsubscribe();
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
      adapter.setFontFamily(readTerminalFontFamily(settings));
      adapter.setScrollback(readTerminalScrollbackLines(settings));
      adapter.refreshTheme();
    };
    applySettings();
    return subscribeAppSettings(applySettings);
  }, [adapterVersion]);

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
      terminalRuntime.clear(terminalId);
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
