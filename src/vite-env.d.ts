/// <reference types="vite/client" />

interface Window {
  __TAURI_INTERNALS__?: unknown;
  requestIdleCallback?: (callback: IdleRequestCallback, options?: IdleRequestOptions) => number;
}
