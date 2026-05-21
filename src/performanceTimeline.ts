const CLEANUP_INTERVAL_MS = 5000;
const MAX_MEASURES_BEFORE_CLEANUP = 500;

let installed = false;
let measureCount = 0;
let cleanupTimer: number | undefined;

export const uninstallPerformanceTimelineCleanup = installPerformanceTimelineCleanup();

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

function cleanupPerformanceTimeline(clearMeasures: (() => void) | undefined) {
  try {
    clearMeasures?.();
  } catch (error) {
    console.warn("failed to clean performance timeline", error);
  }
}
