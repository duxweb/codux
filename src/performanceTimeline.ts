import { runtimeTrace } from "./runtimeTrace";

const CLEANUP_INTERVAL_MS = 5000;
const MAX_MEASURES_BEFORE_CLEANUP = 500;
const TELEMETRY_INTERVAL_MS = 10_000;
const UA_MEMORY_INTERVAL_MS = 30_000;

let installed = false;
let measureCount = 0;
let cleanupTimer: number | undefined;
let telemetryTimer: number | undefined;
let longTaskObserver: PerformanceObserver | undefined;
let longTaskCount = 0;
let longTaskDurationMs = 0;
let lastUaMemorySampleAt = 0;

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
  } catch (error) {
    console.warn("failed to clean performance timeline", error);
  }
}

async function emitFrontendPerformanceSample() {
  const memory = readJsHeapMemory();
  const uaMemoryBytes = await readUserAgentSpecificMemory();
  const resourceCount = performance.getEntriesByType("resource").length;
  const measureCount = performance.getEntriesByType("measure").length;

  runtimeTrace(
    "performance-frontend",
    [
      `jsHeap=${formatBytes(memory.usedBytes)}/${formatBytes(memory.totalBytes)}`,
      `uaMemory=${formatBytes(uaMemoryBytes ?? 0)}`,
      `resources=${resourceCount}`,
      `measures=${measureCount}`,
      `longTasks=${longTaskCount}`,
      `longTaskMs=${Math.round(longTaskDurationMs)}`,
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
