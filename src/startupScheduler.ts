export function runAfterFirstPaint(task: () => void) {
  window.requestAnimationFrame(() => {
    window.requestAnimationFrame(task);
  });
}

export function runWhenIdle(task: () => void, timeout = 700) {
  const idleCallback = window.requestIdleCallback;
  if (idleCallback) {
    idleCallback(task, { timeout });
    return;
  }
  window.setTimeout(task, 0);
}
