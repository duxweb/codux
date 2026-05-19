import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type { AIHookEventPayload, AIRuntimeEvent } from "./types";
import { AISessionStore } from "./sessionStore";
import { aiToolDriverFactory, type AIToolDriverFactory } from "./toolDrivers";

export class AIRuntimeIngressService {
  private unlisten?: UnlistenFn;
  private listenPromise?: Promise<void>;
  private recentRuntimeEventAtByKey = new Map<string, number>();

  constructor(
    private readonly sessionStore: AISessionStore,
    private readonly toolDriverFactory: AIToolDriverFactory = aiToolDriverFactory,
    private readonly onHookApplied?: (terminalId: string, reason: string) => void,
  ) {}

  async start() {
    if (!window.__TAURI_INTERNALS__) return;
    if (this.unlisten) return;
    if (this.listenPromise) return this.listenPromise;
    this.listenPromise = listen<AIRuntimeEvent>("ai-runtime:event", (event) => {
      void this.processRuntimeEvent(event.payload);
    }).then((unlisten) => {
      this.unlisten = unlisten;
      this.listenPromise = undefined;
    });
    return this.listenPromise;
  }

  applyHookForTesting(event: AIHookEventPayload) {
    return this.sessionStore.applyHook(event);
  }

  async processRuntimeEvent(event: AIRuntimeEvent) {
    if (event.kind !== "hook") return;
    const key = this.runtimeEventKey(event);
    if (!this.shouldAcceptRuntimeEvent(key, 350)) return;

    const currentSession = this.sessionStore.sessionForTerminal(event.payload.terminalID);
    const resolved = await this.toolDriverFactory.resolveHookEvent(event.payload, currentSession);
    const didChange = this.sessionStore.applyHook(resolved);
    if (didChange) {
      this.onHookApplied?.(resolved.terminalID, resolved.kind);
    }
  }

  private shouldAcceptRuntimeEvent(key: string, ttlMs: number) {
    const now = Date.now();
    for (const [storedKey, seenAt] of this.recentRuntimeEventAtByKey) {
      if (now - seenAt > Math.max(ttlMs * 4, 2000)) {
        this.recentRuntimeEventAtByKey.delete(storedKey);
      }
    }
    const previous = this.recentRuntimeEventAtByKey.get(key);
    if (previous && now - previous < ttlMs) return false;
    this.recentRuntimeEventAtByKey.set(key, now);
    return true;
  }

  private runtimeEventKey(event: AIRuntimeEvent) {
    const payload = event.payload;
    const sessionId = payload.aiSessionID || payload.terminalID || "unknown";
    const timeBucket = Math.floor((payload.updatedAt || Date.now() / 1000) * 10);
    return `ai-hook|${sessionId}|${payload.kind}|${timeBucket}`;
  }
}
