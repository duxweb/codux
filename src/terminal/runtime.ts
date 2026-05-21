import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { TerminalEvent, TerminalSession } from "../types";
import { readConfiguredShell } from "../settings";

const MAX_HISTORY_CHARS = 2_000_000;

export type TerminalRuntimeSession = TerminalSession & {
  key: string;
  projectName?: string;
  backendId?: string;
  command?: string;
  tool?: string;
  history: string;
};

export type TerminalRuntimeEvent =
  | { type: "output"; data: string; session: TerminalRuntimeSession }
  | { type: "reset"; session: TerminalRuntimeSession }
  | { type: "state"; session: TerminalRuntimeSession }
  | { type: "closed"; sessionId: string };

export type TerminalListener = (event: TerminalRuntimeEvent) => void;

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
  private listeners = new Map<string, Set<TerminalListener>>();
  private eventUnlisten?: UnlistenFn;
  private eventListenPromise?: Promise<void>;
  private backendStartQueue = Promise.resolve();
  private sequence = 0;

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
      return existing;
    }

    const session = this.createRecord({ ...options, key });
    this.sessions.set(session.id, session);
    this.keyToSessionId.set(key, session.id);
    this.enqueueBackendStart(session.id, options);
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
    if (session.backendId) {
      this.unregisterBackendSession(session.backendId, sessionId);
    }
    this.emit(sessionId, { type: "closed", sessionId });
    this.listeners.delete(sessionId);
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
      history: "",
      state: "starting",
    });
    this.emit(sessionId, { type: "reset", session: this.sessions.get(sessionId)! });
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
    this.emit(sessionId, { type: "closed", sessionId });
    this.listeners.delete(sessionId);
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
      history: "",
    };
  }

  private enqueueBackendStart(sessionId: string, options: EnsureTerminalOptions) {
    this.backendStartQueue = this.backendStartQueue
      .catch(() => undefined)
      .then(() => nextAnimationFrame())
      .then(() => this.waitForInitialSize(sessionId))
      .then(() => {
        const size = this.preferredSizes.get(sessionId);
        return this.startBackend(sessionId, { ...options, ...size });
      });
  }

  private async startBackend(sessionId: string, options: EnsureTerminalOptions) {
    const session = this.sessions.get(sessionId);
    if (!session) return;
    const key = terminalSessionKey(options.projectId, options.slotId);

    if (!window.__TAURI_INTERNALS__) {
      const preview = [
        "\x1b[38;5;42mCodux Tauri terminal\x1b[0m",
        "Run `pnpm tauri dev` to attach the native PTY backend.",
        "",
        `${options.cwd} $ `,
      ].join("\r\n");
      this.updateSession(sessionId, { state: "running", history: preview });
      this.emit(sessionId, { type: "reset", session: this.sessions.get(sessionId)! });
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
          CODEX_WORKSPACE: "codux-tauri",
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
      this.updateSession(sessionId, { history, state: "running" });
      this.emit(sessionId, { type: "reset", session: this.sessions.get(sessionId)! });
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
      if (event.kind === "output" && event.data) {
        this.appendOutput(sessionId, event.data);
        continue;
      }

      if (event.kind === "exit") {
        this.updateSession(sessionId, {
          state: "exited",
          exitCode: event.exitCode,
        });
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

  private appendOutput(sessionId: string, data: string) {
    const session = this.sessions.get(sessionId);
    if (!session) return;

    const history = `${session.history}${data}`;
    session.history = history.length > MAX_HISTORY_CHARS ? history.slice(history.length - MAX_HISTORY_CHARS) : history;
    if (session.backendId && session.state === "starting") {
      session.state = "running";
      this.emit(sessionId, { type: "state", session: { ...session } });
    }
    this.emit(sessionId, { type: "output", data, session: { ...session } });
  }

  private updateSession(sessionId: string, patch: Partial<Omit<TerminalRuntimeSession, "id" | "key">>) {
    const session = this.sessions.get(sessionId);
    if (!session) return;
    Object.assign(session, patch);
    this.emit(sessionId, { type: "state", session: { ...session } });
  }

  private emit(sessionId: string, event: TerminalRuntimeEvent) {
    queueMicrotask(() => {
      const listeners = this.listeners.get(sessionId);
      if (!listeners?.size) return;
      for (const listener of [...listeners]) {
        listener(event);
      }
    });
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
