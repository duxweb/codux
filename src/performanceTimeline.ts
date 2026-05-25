import { runtimeTrace } from "./runtimeTrace";

const CLEANUP_INTERVAL_MS = 5000;
const MAX_MEASURES_BEFORE_CLEANUP = 500;
const TELEMETRY_INTERVAL_MS = 10_000;
const UA_MEMORY_INTERVAL_MS = 30_000;
const RESOURCE_TIMING_BUFFER_SIZE = 128;

let installed = false;
let measureCount = 0;
let cleanupTimer: number | undefined;
let telemetryTimer: number | undefined;
let longTaskObserver: PerformanceObserver | undefined;
let longTaskCount = 0;
let longTaskDurationMs = 0;
let lastUaMemorySampleAt = 0;
let previousHeapUsedBytes = 0;
let previousHeapTotalBytes = 0;

export const uninstallPerformanceTimelineCleanup = installPerformanceTimelineCleanup();
export const uninstallPerformanceTelemetry = installPerformanceTelemetry();

function installPerformanceTimelineCleanup() {
  if (installed || typeof performance === "undefined") return () => undefined;
  installed = true;

  const originalMeasure = performance.measure.bind(performance);
  const originalClearMeasures = performance.clearMeasures?.bind(performance);

  if (originalClearMeasures) {
    performance.measure = ((...args: Parameters<Performance["measure"]>) => {
      const entry = originalMeasure(...args);
      measureCount += 1;
      if (measureCount >= MAX_MEASURES_BEFORE_CLEANUP) {
        measureCount = 0;
        cleanupPerformanceTimeline(originalClearMeasures);
      }
      return entry;
    }) as Performance["measure"];
  }
  try {
    performance.setResourceTimingBufferSize?.(RESOURCE_TIMING_BUFFER_SIZE);
  } catch {
    // Best effort only. Some WebView builds expose the method but reject calls.
  }

  cleanupTimer = window.setInterval(() => {
    cleanupPerformanceTimeline(originalClearMeasures);
  }, CLEANUP_INTERVAL_MS);

  return () => {
    if (cleanupTimer !== undefined) {
      window.clearInterval(cleanupTimer);
      cleanupTimer = undefined;
    }
    performance.measure = originalMeasure as Performance["measure"];
    cleanupPerformanceTimeline(originalClearMeasures);
    installed = false;
    measureCount = 0;
  };
}

function installPerformanceTelemetry() {
  if (typeof window === "undefined" || typeof performance === "undefined") return () => undefined;

  try {
    if ("PerformanceObserver" in window) {
      longTaskObserver = new PerformanceObserver((list) => {
        for (const entry of list.getEntries()) {
          longTaskCount += 1;
          longTaskDurationMs += entry.duration;
        }
      });
      longTaskObserver.observe({ entryTypes: ["longtask"] });
    }
  } catch {
    longTaskObserver = undefined;
  }

  const sample = () => {
    void emitFrontendPerformanceSample();
  };
  telemetryTimer = window.setInterval(sample, TELEMETRY_INTERVAL_MS);
  sample();

  return () => {
    if (telemetryTimer !== undefined) {
      window.clearInterval(telemetryTimer);
      telemetryTimer = undefined;
    }
    longTaskObserver?.disconnect();
    longTaskObserver = undefined;
  };
}

function cleanupPerformanceTimeline(clearMeasures: (() => void) | undefined) {
  try {
    clearMeasures?.();
    performance.clearMarks?.();
    performance.clearResourceTimings?.();
  } catch (error) {
    console.warn("failed to clean performance timeline", error);
  }
}

async function emitFrontendPerformanceSample() {
  const memory = readJsHeapMemory();
  const uaMemoryBytes = await readUserAgentSpecificMemory();
  const resourceCount = performance.getEntriesByType("resource").length;
  const measureCount = performance.getEntriesByType("measure").length;
  const usedDelta = memory.usedBytes - previousHeapUsedBytes;
  const totalDelta = memory.totalBytes - previousHeapTotalBytes;
  previousHeapUsedBytes = memory.usedBytes;
  previousHeapTotalBytes = memory.totalBytes;

  runtimeTrace(
    "performance-frontend",
    [
      `route=${shortRouteLabel()}`,
      `visible=${document.visibilityState}`,
      `jsHeap=${formatBytes(memory.usedBytes)}/${formatBytes(memory.totalBytes)}`,
      `heapDelta=${formatSignedBytes(usedDelta)}/${formatSignedBytes(totalDelta)}`,
      `uaMemory=${formatBytes(uaMemoryBytes ?? 0)}`,
      `resources=${resourceCount}`,
      `measures=${measureCount}`,
      `longTasks=${longTaskCount}`,
      `longTaskMs=${Math.round(longTaskDurationMs)}`,
      `canvases=${document.querySelectorAll("canvas").length}`,
    ].join(" "),
  );

  longTaskCount = 0;
  longTaskDurationMs = 0;
}

function readJsHeapMemory() {
  const memory = (performance as Performance & {
    memory?: {
      usedJSHeapSize: number;
      totalJSHeapSize: number;
    };
  }).memory;

  return {
    usedBytes: memory?.usedJSHeapSize ?? 0,
    totalBytes: memory?.totalJSHeapSize ?? 0,
  };
}

async function readUserAgentSpecificMemory() {
  const now = performance.now();
  if (now - lastUaMemorySampleAt < UA_MEMORY_INTERVAL_MS) return null;
  lastUaMemorySampleAt = now;

  const measure = (performance as Performance & {
    measureUserAgentSpecificMemory?: () => Promise<{ bytes: number }>;
  }).measureUserAgentSpecificMemory;
  if (!measure) return null;

  try {
    const result = await measure.call(performance);
    return Number.isFinite(result.bytes) ? result.bytes : null;
  } catch {
    return null;
  }
}

function formatBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes <= 0) return "0B";
  const mb = 1_048_576;
  const gb = 1_073_741_824;
  if (bytes >= gb) return `${(bytes / gb).toFixed(2)}G`;
  if (bytes >= mb) return `${Math.round(bytes / mb)}M`;
  return `${Math.round(bytes / 1024)}K`;
}

function formatSignedBytes(bytes: number) {
  if (!Number.isFinite(bytes) || bytes === 0) return "0B";
  const sign = bytes > 0 ? "+" : "-";
  return `${sign}${formatBytes(Math.abs(bytes))}`;
}

function shortRouteLabel() {
  if (document.documentElement.classList.contains("desktop-pet-page")) return "desktop-pet";
  const route = window.location.hash.replace(/^#/, "") || "/";
  const routePath = route.split("?")[0] || route;
  if (routePath === "/") return "main";
  return routePath.replace(/[^a-zA-Z0-9_-]+/g, "_").replace(/^_+|_+$/g, "") || "main";
}
