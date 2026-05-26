import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { TerminalEvent, TerminalSession } from "../types";
import { readConfiguredShell } from "../settings";
import { runtimeTrace } from "../runtimeTrace";

const MAX_REPLAY_CHARS = 80_000;
const MAX_VIEW_SNAPSHOT_DELTA_BYTES = 2 * 1024 * 1024;

type TerminalViewSnapshot = {
  history: string;
  pendingOutputs: Array<string | Uint8Array>;
  pendingBytes: number;
  createdAt: number;
};

export type TerminalRuntimeSession = TerminalSession & {
  key: string;
  projectName?: string;
  backendId?: string;
  command?: string;
  tool?: string;
  replayBuffer: string;
  hasSnapshot: boolean;
};

export type TerminalRuntimeEvent =
  | { type: "output"; text: string; bytes: Uint8Array; session: TerminalRuntimeSessionSnapshot }
  | { type: "reset"; session: TerminalRuntimeSession; history?: string }
  | { type: "state"; session: TerminalRuntimeSession }
  | { type: "closed"; sessionId: string };

export type TerminalListener = (event: TerminalRuntimeEvent) => void;

export type TerminalRuntimeSessionSnapshot = Omit<TerminalRuntimeSession, "replayBuffer">;

type EnsureTerminalOptions = {
  projectId: string;
  slotId: string;
  title: string;
  cwd: string;
  projectName?: string;
  cols?: number;
  rows?: number;
  command?: string;
  tool?: string;
};

type AttachedSessionOptions = {
  backendId: string;
  terminalId: string;
  projectId: string;
  slotId: string;
  title?: string;
  cwd?: string;
  projectName?: string;
};

export class TerminalRuntime {
  private sessions = new Map<string, TerminalRuntimeSession>();
  private keyToSessionId = new Map<string, string>();
  private backendToSessionIds = new Map<string, Set<string>>();
  private preferredSizes = new Map<string, { cols: number; rows: number }>();
  private initialSizeResolvers = new Map<string, () => void>();
  private startOptions = new Map<string, EnsureTerminalOptions>();
  private startingSessions = new Set<string>();
  private listeners = new Map<string, Set<TerminalListener>>();
  private eventUnlisten?: UnlistenFn;
  private eventListenPromise?: Promise<void>;
  private backendStartQueue = Promise.resolve();
  private outputQueues = new Map<
    string,
    {
      chunks: Array<{ text: string; bytes?: Uint8Array }>;
      scheduled: boolean;
    }
  >();
  private outputTextDecoders = new Map<string, TextDecoder>();
  private viewSnapshots = new Map<string, TerminalViewSnapshot>();
  private sequence = 0;

  debugSnapshot() {
    let replayChars = 0;
    let running = 0;
    for (const session of this.sessions.values()) {
      replayChars += session.replayBuffer.length;
      if (session.state === "running") running += 1;
    }
    return {
      sessions: this.sessions.size,
      backends: this.backendToSessionIds.size,
      listeners: this.listeners.size,
      queues: this.outputQueues.size,
      replayChars,
      running,
      viewSnapshots: this.viewSnapshots.size,
    };
  }

  ensureTerminal(options: EnsureTerminalOptions) {
    const key = terminalSessionKey(options.projectId, options.slotId);
    const existingId = this.keyToSessionId.get(key);
    const existing = existingId ? this.sessions.get(existingId) : undefined;
    if (existing) {
      if (existing.cwd !== options.cwd || existing.title !== options.title) {
        this.updateSession(existing.id, {
          cwd: options.cwd,
          title: options.title,
          slotId: options.slotId,
          projectId: options.projectId,
          projectName: options.projectName,
          command: options.command,
          tool: options.tool,
        });
      }
      this.startOptions.set(existing.id, options);
      return existing;
    }

    const session = this.createRecord({ ...options, key });
    this.sessions.set(session.id, session);
    this.keyToSessionId.set(key, session.id);
    this.startOptions.set(session.id, options);
    this.traceRuntime("ensureTerminal created", session.id);
    return session;
  }

  getSession(sessionId: string) {
    return this.sessions.get(sessionId);
  }

  subscribe(sessionId: string, listener: TerminalListener) {
    let set = this.listeners.get(sessionId);
    if (!set) {
      set = new Set();
      this.listeners.set(sessionId, set);
    }
    set.add(listener);
    const current = this.sessions.get(sessionId);
    if (current) {
      listener({ type: "state", session: { ...current } });
    }
    return () => {
      const current = this.listeners.get(sessionId);
      current?.delete(listener);
      if (current?.size === 0) {
        this.listeners.delete(sessionId);
      }
    };
  }

  ensureStarted(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session || session.backendId || session.state !== "starting" || this.startingSessions.has(sessionId)) {
      return;
    }
    const options = this.startOptions.get(sessionId) ?? {
      projectId: session.projectId,
      slotId: session.slotId,
      title: session.title,
      cwd: session.cwd,
      projectName: session.projectName,
      command: session.command,
      tool: session.tool,
    };
    this.enqueueBackendStart(sessionId, options);
  }

  write(sessionId: string, data: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    if (!window.__TAURI_INTERNALS__) {
      this.appendOutput(sessionId, data === "\r" ? "\r\n" : data);
      return;
    }

    if (!session.backendId || session.state !== "running") return;
    void invoke("terminal_write", {
      sessionId: session.backendId,
      data,
    });
  }

  resize(sessionId: string, cols: number, rows: number) {
    const safeSize = {
      cols: Math.max(20, Math.floor(cols)),
      rows: Math.max(8, Math.floor(rows)),
    };
    const previousSize = this.preferredSizes.get(sessionId);
    this.preferredSizes.set(sessionId, safeSize);
    this.initialSizeResolvers.get(sessionId)?.();
    this.initialSizeResolvers.delete(sessionId);

    if (previousSize && previousSize.cols === safeSize.cols && previousSize.rows === safeSize.rows) {
      return;
    }

    const session = this.sessions.get(sessionId);
    if (!session?.backendId || !window.__TAURI_INTERNALS__) return;
    void invoke("terminal_resize", {
      sessionId: session.backendId,
      cols: safeSize.cols,
      rows: safeSize.rows,
    });
  }

  interrupt(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session?.backendId || !window.__TAURI_INTERNALS__) return;
    void invoke("terminal_interrupt", { sessionId: session.backendId });
  }

  clear(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    session.replayBuffer = "";
    session.hasSnapshot = false;
    this.viewSnapshots.delete(sessionId);
    this.outputTextDecoders.delete(sessionId);
    this.flushOutputQueue(sessionId);
    if (session.backendId && window.__TAURI_INTERNALS__) {
      void invoke("terminal_clear_history", { sessionId: session.backendId }).catch(() => undefined);
    }
    this.emit(sessionId, { type: "reset", session, history: "" });
  }

  saveViewSnapshot(sessionId: string, history: string, pendingOutputs: Array<string | Uint8Array> = []) {
    if (!this.sessions.has(sessionId)) return;
    const pendingBytes = terminalOutputBytes(pendingOutputs);
    this.viewSnapshots.set(sessionId, {
      history,
      pendingOutputs: pendingOutputs.map(cloneTerminalOutput),
      pendingBytes,
      createdAt: Date.now(),
    });
    runtimeTrace(
      "terminal-runtime",
      `view_snapshot save session=${sessionId} historyChars=${history.length} pendingOutputs=${pendingOutputs.length} pendingBytes=${pendingBytes}`,
    );
  }

  takeViewSnapshot(sessionId: string) {
    const snapshot = this.viewSnapshots.get(sessionId);
    if (!snapshot) return undefined;
    this.viewSnapshots.delete(sessionId);
    runtimeTrace(
      "terminal-runtime",
      `view_snapshot take session=${sessionId} ageMs=${Date.now() - snapshot.createdAt} historyChars=${snapshot.history.length} pendingOutputs=${snapshot.pendingOutputs.length} pendingBytes=${snapshot.pendingBytes}`,
    );
    return {
      history: snapshot.history,
      pendingOutputs: snapshot.pendingOutputs.map(cloneTerminalOutput),
    };
  }

  async snapshot(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return undefined;
    if (!session.backendId || !window.__TAURI_INTERNALS__) {
      if (session.replayBuffer) {
        runtimeTrace(
          "terminal-runtime",
          `snapshot local session=${sessionId} chars=${session.replayBuffer.length}`,
        );
      }
      return session.replayBuffer;
    }
    try {
      const history = await invoke<string>("terminal_snapshot", { sessionId: session.backendId });
      runtimeTrace(
        "terminal-runtime",
        `snapshot backend session=${sessionId} backend=${session.backendId} chars=${history.length}`,
      );
      return history;
    } catch (error) {
      runtimeTrace(
        "terminal-runtime",
        `snapshot backend failed session=${sessionId} backend=${session.backendId} fallbackChars=${session.replayBuffer.length} error=${error instanceof Error ? error.message : String(error)}`,
      );
      return session.replayBuffer;
    }
  }

  ensureAttachedSession(options: AttachedSessionOptions) {
    const key = `attached:${options.backendId}`;
    const existing = [...(this.backendToSessionIds.get(options.backendId) ?? [])]
      .map((sessionId) => this.sessions.get(sessionId))
      .find((session) => session?.key === key);
    if (existing) return existing;

    const session = this.createRecord({
      key,
      terminalId: options.terminalId,
      projectId: options.projectId,
      projectName: options.projectName,
      slotId: options.slotId,
      title: options.title || "Terminal",
      cwd: options.cwd || "",
    });
    session.backendId = options.backendId;
    session.state = "running";
    this.sessions.set(session.id, session);
    this.keyToSessionId.set(session.key, session.id);
    this.registerBackendSession(options.backendId, session.id);
    void this.attachBackendSnapshot(session.id, options.backendId);
    return session;
  }

  detachView(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return;
    this.sessions.delete(sessionId);
    this.keyToSessionId.delete(session.key);
    this.preferredSizes.delete(sessionId);
    this.initialSizeResolvers.delete(sessionId);
    this.startOptions.delete(sessionId);
    this.startingSessions.delete(sessionId);
    this.outputTextDecoders.delete(sessionId);
    this.viewSnapshots.delete(sessionId);
    this.flushOutputQueue(sessionId);
    if (session.backendId) {
      this.unregisterBackendSession(session.backendId, sessionId);
    }
    this.emit(sessionId, { type: "closed", sessionId });
    this.listeners.delete(sessionId);
    this.traceRuntime("detachView", sessionId);
  }

  async closeDetachedBackend(backendId: string) {
    if (!window.__TAURI_INTERNALS__) return;
    await invoke("terminal_kill", { sessionId: backendId }).catch(() => undefined);
    const sessionIds = [...(this.backendToSessionIds.get(backendId) ?? [])];
    for (const sessionId of sessionIds) {
      this.detachView(sessionId);
    }
  }

  async restart(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    if (session.backendId && window.__TAURI_INTERNALS__) {
      await invoke("terminal_kill", { sessionId: session.backendId }).catch(() => undefined);
      this.unregisterBackendSession(session.backendId, sessionId);
    }

    this.updateSession(sessionId, {
      backendId: undefined,
      exitCode: undefined,
      replayBuffer: "",
      hasSnapshot: false,
      state: "starting",
    });
    this.outputTextDecoders.delete(sessionId);
    this.viewSnapshots.delete(sessionId);
    this.emit(sessionId, { type: "reset", session: this.sessions.get(sessionId)!, history: "" });
    this.enqueueBackendStart(sessionId, {
      projectId: session.projectId,
      slotId: session.slotId,
      title: session.title,
      cwd: session.cwd,
      projectName: session.projectName,
      command: session.command,
      tool: session.tool,
    });
  }

  async close(sessionId: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    if (session.backendId && window.__TAURI_INTERNALS__) {
      await invoke("terminal_kill", { sessionId: session.backendId }).catch(() => undefined);
      const sessionIds = [...(this.backendToSessionIds.get(session.backendId) ?? [sessionId])];
      for (const item of sessionIds) {
        if (item !== sessionId) {
          this.detachView(item);
        }
      }
      this.unregisterBackendSession(session.backendId, sessionId);
    }

    this.sessions.delete(sessionId);
    this.keyToSessionId.delete(session.key);
    this.preferredSizes.delete(sessionId);
    this.initialSizeResolvers.delete(sessionId);
    this.startOptions.delete(sessionId);
    this.startingSessions.delete(sessionId);
    this.outputTextDecoders.delete(sessionId);
    this.viewSnapshots.delete(sessionId);
    this.flushOutputQueue(sessionId);
    this.emit(sessionId, { type: "closed", sessionId });
    this.listeners.delete(sessionId);
    this.traceRuntime("close", sessionId);
  }

  private createRecord(options: EnsureTerminalOptions & { key: string; terminalId?: string }): TerminalRuntimeSession {
    const id = options.terminalId ?? createTerminalId(++this.sequence);
    return {
      id,
      key: options.key,
      projectId: options.projectId,
      slotId: options.slotId,
      projectName: options.projectName,
      title: options.title,
      cwd: options.cwd,
      command: options.command,
      tool: options.tool,
      shell: "login shell",
      state: "starting",
      replayBuffer: "",
      hasSnapshot: false,
    };
  }

  private enqueueBackendStart(sessionId: string, options: EnsureTerminalOptions) {
    if (this.startingSessions.has(sessionId)) return;
    this.startingSessions.add(sessionId);
    this.backendStartQueue = this.backendStartQueue
      .catch(() => undefined)
      .then(() => nextAnimationFrame())
      .then(() => this.waitForInitialSize(sessionId))
      .then(() => {
        const size = this.preferredSizes.get(sessionId);
        return this.startBackend(sessionId, { ...options, ...size });
      })
      .finally(() => {
        this.startingSessions.delete(sessionId);
      });
  }

  private async startBackend(sessionId: string, options: EnsureTerminalOptions) {
    const session = this.sessions.get(sessionId);
    if (!session) return;
    const key = terminalSessionKey(options.projectId, options.slotId);

    if (!window.__TAURI_INTERNALS__) {
      const preview = [
        "\x1b[38;5;42mCodux terminal\x1b[0m",
        "Run `pnpm tauri dev` to attach the native PTY backend.",
        "",
        `${options.cwd} $ `,
      ].join("\r\n");
      this.updateSession(sessionId, { state: "running", replayBuffer: preview, hasSnapshot: true });
      this.emit(sessionId, { type: "reset", session: this.sessions.get(sessionId)!, history: preview });
      return;
    }

    try {
      await this.ensureEventListener();
      const config: Record<string, unknown> = {
        cwd: options.cwd,
        shell: readConfiguredShell(),
        cols: options.cols ?? 100,
        rows: options.rows ?? 30,
        projectId: options.projectId,
        projectName: options.projectName,
        terminalId: session.id,
        slotId: options.slotId,
        sessionKey: key,
        title: options.title,
        command: options.command,
        tool: options.tool ?? "auto",
        env: {
          CODEX_WORKSPACE: "codux",
          CODUX_PROJECT_ID: options.projectId,
          CODUX_SLOT_ID: options.slotId,
          CODUX_TERMINAL_ID: session.id,
        },
      };

      const backendId = await invoke<string>("terminal_create", { config });
      if (!this.sessions.has(sessionId)) {
        await invoke("terminal_kill", { sessionId: backendId }).catch(() => undefined);
        return;
      }
      this.registerBackendSession(backendId, sessionId);
      this.updateSession(sessionId, {
        backendId,
        shell: "login shell",
      });
      void this.attachBackendSnapshot(sessionId, backendId);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.updateSession(sessionId, { state: "error" });
      this.appendOutput(sessionId, `\r\n[terminal error] ${message}`);
    }
  }

  private async attachBackendSnapshot(sessionId: string, backendId: string) {
    if (!window.__TAURI_INTERNALS__) return;

    try {
      await this.ensureEventListener();
      const history = await invoke<string>("terminal_snapshot", { sessionId: backendId });
      if (!this.sessions.has(sessionId)) return;
      runtimeTrace(
        "terminal-runtime",
        `attach_snapshot session=${sessionId} backend=${backendId} chars=${history.length} bytes=${new TextEncoder().encode(history).length}`,
      );
      if (history) {
        const replayBuffer = trimReplayBuffer(history);
        this.updateSession(sessionId, {
          replayBuffer,
          hasSnapshot: true,
          state: "running",
        });
        this.emit(sessionId, {
          type: "reset",
          session: this.sessions.get(sessionId)!,
          history,
        });
      }
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      this.updateSession(sessionId, { state: "error" });
      this.appendOutput(sessionId, `\r\n[terminal error] ${message}`);
    }
  }

  private async ensureEventListener() {
    if (this.eventUnlisten) return;
    if (this.eventListenPromise) return this.eventListenPromise;
    this.eventListenPromise = listen<TerminalEvent>("terminal:event", (event) => {
      this.handleBackendEvent(event.payload);
    }).then((unlisten) => {
      this.eventUnlisten = unlisten;
      this.eventListenPromise = undefined;
    });
    return this.eventListenPromise;
  }

  private async waitForInitialSize(sessionId: string) {
    if (this.preferredSizes.has(sessionId)) return;
    await Promise.race([
      new Promise<void>((resolve) => {
        this.initialSizeResolvers.set(sessionId, resolve);
      }),
      delay(120),
    ]);
    this.initialSizeResolvers.delete(sessionId);
  }

  private handleBackendEvent(event: TerminalEvent) {
    const sessionIds = this.backendToSessionIds.get(event.sessionId);
    if (!sessionIds?.size) return;

    for (const sessionId of sessionIds) {
      if (event.kind === "output" && (event.text || event.bytesBase64)) {
        this.appendOutput(sessionId, event.text ?? "", decodeBase64Bytes(event.bytesBase64));
        continue;
      }

      if (event.kind === "exit") {
        this.updateSession(sessionId, {
          state: "exited",
          exitCode: event.exitCode,
        });
        this.flushTextDecoder(sessionId);
        this.appendOutput(sessionId, `\r\n[process exited${event.exitCode == null ? "" : `: ${event.exitCode}`}]`);
        continue;
      }

      if (event.kind === "error") {
        this.updateSession(sessionId, { state: "error" });
        this.appendOutput(sessionId, `\r\n[terminal error] ${event.message ?? "unknown error"}`);
      }
    }
  }

  private registerBackendSession(backendId: string, sessionId: string) {
    let sessionIds = this.backendToSessionIds.get(backendId);
    if (!sessionIds) {
      sessionIds = new Set();
      this.backendToSessionIds.set(backendId, sessionIds);
    }
    sessionIds.add(sessionId);
  }

  private unregisterBackendSession(backendId: string, sessionId: string) {
    const sessionIds = this.backendToSessionIds.get(backendId);
    if (!sessionIds) return;
    sessionIds.delete(sessionId);
    if (sessionIds.size === 0) {
      this.backendToSessionIds.delete(backendId);
    }
  }

  private appendOutput(sessionId: string, text: string, bytes?: Uint8Array) {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    const shouldDecodeForReplay = !session.backendId || !window.__TAURI_INTERNALS__;
    const decodedText = shouldDecodeForReplay ? this.decodeOutputText(sessionId, bytes) : "";
    const outputText = text || decodedText;
    if (outputText) {
      session.replayBuffer = trimReplayBuffer(session.replayBuffer + outputText);
    }
    this.appendViewSnapshotDelta(sessionId, bytes?.length ? bytes : outputText);
    if (session.backendId && session.state === "starting") {
      session.state = "running";
      this.emit(sessionId, { type: "state", session: { ...session } });
    }
    this.enqueueOutput(sessionId, outputText, bytes);
  }

  private enqueueOutput(sessionId: string, text: string, bytes?: Uint8Array) {
    let queue = this.outputQueues.get(sessionId);
    if (!queue) {
      queue = { chunks: [], scheduled: false };
      this.outputQueues.set(sessionId, queue);
    }
    queue.chunks.push({ text, bytes });
    if (queue.scheduled) return;
    queue.scheduled = true;
    queueMicrotask(() => this.flushOutputQueue(sessionId));
  }

  private flushOutputQueue(sessionId: string) {
    const queue = this.outputQueues.get(sessionId);
    if (!queue) return;
    this.outputQueues.delete(sessionId);
    const session = this.sessions.get(sessionId);
    if (!session || queue.chunks.length === 0) return;
    const { text, bytes } = combineTerminalChunks(queue.chunks);
    this.emit(sessionId, {
      type: "output",
      text,
      bytes,
      session: sessionSnapshot(session),
    });
  }

  private updateSession(sessionId: string, patch: Partial<Omit<TerminalRuntimeSession, "id" | "key">>) {
    const session = this.sessions.get(sessionId);
    if (!session) return;
    Object.assign(session, patch);
    this.emit(sessionId, { type: "state", session: { ...session } });
  }

  private emit(sessionId: string, event: TerminalRuntimeEvent) {
    const listeners = this.listeners.get(sessionId);
    if (!listeners?.size) return;
    for (const listener of [...listeners]) {
      listener(event);
    }
  }

  private traceRuntime(action: string, sessionId: string) {
    const snapshot = this.debugSnapshot();
    runtimeTrace(
      "terminal-runtime",
      `${action} session=${sessionId} sessions=${snapshot.sessions} backends=${snapshot.backends} listeners=${snapshot.listeners} queues=${snapshot.queues} replayChars=${snapshot.replayChars} running=${snapshot.running} viewSnapshots=${snapshot.viewSnapshots}`,
    );
  }

  private decodeOutputText(sessionId: string, bytes?: Uint8Array) {
    if (!bytes?.length) return "";
    let decoder = this.outputTextDecoders.get(sessionId);
    if (!decoder) {
      decoder = new TextDecoder();
      this.outputTextDecoders.set(sessionId, decoder);
    }
    return decoder.decode(bytes, { stream: true });
  }

  private flushTextDecoder(sessionId: string) {
    const decoder = this.outputTextDecoders.get(sessionId);
    if (!decoder) return;
    const remaining = decoder.decode();
    this.outputTextDecoders.delete(sessionId);
    if (remaining) {
      this.appendOutput(sessionId, remaining);
    }
  }

  private appendViewSnapshotDelta(sessionId: string, output?: string | Uint8Array) {
    if (!output || output.length === 0) return;
    const snapshot = this.viewSnapshots.get(sessionId);
    if (!snapshot) return;
    const cloned = cloneTerminalOutput(output);
    const bytes = terminalOutputBytes([cloned]);
    const nextBytes = snapshot.pendingBytes + bytes;
    if (nextBytes > MAX_VIEW_SNAPSHOT_DELTA_BYTES) {
      this.viewSnapshots.delete(sessionId);
      runtimeTrace(
        "terminal-runtime",
        `view_snapshot drop session=${sessionId} reason=delta_limit pendingBytes=${nextBytes}`,
      );
      return;
    }
    snapshot.pendingOutputs.push(cloned);
    snapshot.pendingBytes = nextBytes;
  }
}

export const terminalRuntime = new TerminalRuntime();

export function terminalSessionKey(projectId: string, slotId: string) {
  return `${projectId}:${slotId}`;
}

function createTerminalId(sequence: number) {
  const cryptoApi = globalThis.crypto;
  if (cryptoApi && "randomUUID" in cryptoApi) {
    return `term-${cryptoApi.randomUUID()}`;
  }
  return `term-${Date.now()}-${sequence}`;
}

function sessionSnapshot(session: TerminalRuntimeSession): TerminalRuntimeSessionSnapshot {
  const { replayBuffer: _replayBuffer, ...snapshot } = session;
  return { ...snapshot };
}

export function terminalReplayBuffer(session?: TerminalRuntimeSession) {
  return session?.replayBuffer;
}

function trimReplayBuffer(value: string) {
  if (value.length <= MAX_REPLAY_CHARS) return value;
  return value.slice(value.length - MAX_REPLAY_CHARS);
}

function cloneTerminalOutput(output: string | Uint8Array) {
  if (typeof output === "string") return output;
  return output.slice();
}

function terminalOutputBytes(outputs: Array<string | Uint8Array>) {
  let total = 0;
  for (const output of outputs) {
    total += typeof output === "string" ? encodeTerminalText(output).byteLength : output.byteLength;
  }
  return total;
}

function decodeBase64Bytes(value?: string) {
  if (!value) return undefined;
  const fromBase64 = (Uint8Array as typeof Uint8Array & { fromBase64?: (value: string) => Uint8Array }).fromBase64;
  if (fromBase64) {
    try {
      return fromBase64(value);
    } catch {
      // Fall back to atob for older WebKit/WebView2 builds.
    }
  }
  try {
    const binary = atob(value);
    const bytes = new Uint8Array(binary.length);
    for (let index = 0; index < binary.length; index += 1) {
      bytes[index] = binary.charCodeAt(index);
    }
    return bytes;
  } catch {
    return undefined;
  }
}

function encodeTerminalText(value: string) {
  return new TextEncoder().encode(value);
}

function combineTerminalChunks(chunks: Array<{ text: string; bytes?: Uint8Array }>) {
  if (chunks.length === 1) {
    const chunk = chunks[0];
    return {
      text: "",
      bytes: chunk.bytes ?? encodeTerminalText(chunk.text),
    };
  }

  let totalLength = 0;
  const encodedChunks: Uint8Array[] = [];
  for (const chunk of chunks) {
    const bytes = chunk.bytes ?? encodeTerminalText(chunk.text);
    encodedChunks.push(bytes);
    totalLength += bytes.length;
  }

  const combined = new Uint8Array(totalLength);
  let offset = 0;
  for (const chunk of encodedChunks) {
    combined.set(chunk, offset);
    offset += chunk.length;
  }
  return { text: "", bytes: combined };
}

function nextAnimationFrame() {
  if (typeof window === "undefined") {
    return Promise.resolve();
  }
  return new Promise<void>((resolve) => {
    window.requestAnimationFrame(() => resolve());
  });
}

function delay(ms: number) {
  return new Promise<void>((resolve) => window.setTimeout(resolve, ms));
}
