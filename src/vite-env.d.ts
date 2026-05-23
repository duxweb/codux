/// <reference types="vite/client" />

declare module "@xterm/addon-canvas/lib/addon-canvas.js" {
  export { CanvasAddon } from "@xterm/addon-canvas";
}

interface Window {
  __TAURI_INTERNALS__?: unknown;
  requestIdleCallback?: (callback: IdleRequestCallback, options?: IdleRequestOptions) => number;
}
