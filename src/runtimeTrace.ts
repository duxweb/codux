import { invoke } from "@tauri-apps/api/core";

let traceQueue: Promise<void> = Promise.resolve();
const traceDedupWindowMs = 1000;
const maxRecentTraceKeys = 512;
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
  pruneRecentTraceKeys(now);
  traceQueue = traceQueue.catch(() => undefined).then(async () => {
    await invoke("runtime_trace_frontend", {
      category,
      message,
    }).catch(() => undefined);
  });
}

function pruneRecentTraceKeys(now: number) {
  if (recentTraceByKey.size <= maxRecentTraceKeys) return;
  for (const [key, timestamp] of recentTraceByKey) {
    if (now - timestamp > traceDedupWindowMs) {
      recentTraceByKey.delete(key);
    }
    if (recentTraceByKey.size <= maxRecentTraceKeys) return;
  }
  while (recentTraceByKey.size > maxRecentTraceKeys) {
    const oldestKey = recentTraceByKey.keys().next().value;
    if (oldestKey === undefined) return;
    recentTraceByKey.delete(oldestKey);
  }
}

export function approximateJsonBytes(value: unknown) {
  try {
    return new TextEncoder().encode(JSON.stringify(value)).length;
  } catch {
    return -1;
  }
}
