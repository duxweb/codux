import { invoke } from "@tauri-apps/api/core";

let traceQueue: Promise<void> = Promise.resolve();

export function runtimeTrace(category: string, message: string) {
  if (!window.__TAURI_INTERNALS__) return;
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
