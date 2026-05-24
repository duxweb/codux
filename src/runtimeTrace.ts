import { invoke } from "@tauri-apps/api/core";

let traceQueue: Promise<void> = Promise.resolve();
const traceDedupWindowMs = 1000;
const recentTraceByKey = new Map<string, number>();

export function runtimeTrace(category: string, message: string) {
  if (!window.__TAURI_INTERNALS__) return;
  const key = `${category}:${message}`;
  const now = Date.now();
  const previous = recentTraceByKey.get(key);
  if (previous !== undefined && now - previous < traceDedupWindowMs) {
    return;
  }
  recentTraceByKey.set(key, now);
  traceQueue = traceQueue.catch(() => undefined).then(async () => {
    await invoke("runtime_trace_frontend", {
      category,
      message,
    }).catch(() => undefined);
  });
}

export function approximateJsonBytes(value: unknown) {
  try {
    return new TextEncoder().encode(JSON.stringify(value)).length;
  } catch {
    return -1;
  }
}
