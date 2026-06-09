#!/usr/bin/env bash
set -euo pipefail

APP_NAME="${APP_NAME:-codux}"
SAMPLE_SECONDS="${SAMPLE_SECONDS:-5}"
SCROLL_EVENTS="${SCROLL_EVENTS:-240}"
SCROLL_DELAY="${SCROLL_DELAY:-0.01}"
OUT_DIR="${OUT_DIR:-/tmp}"
STAMP="$(date +%Y%m%d-%H%M%S)"

PID="$(pgrep -x "$APP_NAME" | head -n 1 || true)"
if [[ -z "$PID" ]]; then
  echo "process not found: $APP_NAME" >&2
  exit 1
fi

CPU_OUT="$OUT_DIR/${APP_NAME}-${STAMP}-cpu.txt"
SAMPLE_OUT="$OUT_DIR/${APP_NAME}-${STAMP}.sample.txt"

osascript >/dev/null <<OSA
tell application "System Events"
  set frontmost of first process whose unix id is $PID to true
end tell
OSA

swift - "$PID" "$SCROLL_EVENTS" "$SCROLL_DELAY" <<'SWIFT' &
import CoreGraphics
import Foundation

let pid = Int32(CommandLine.arguments[1])!
let events = Int(CommandLine.arguments[2])!
let delay = Double(CommandLine.arguments[3])!

let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    as? [[String: Any]] ?? []
let bounds = windows.first { window in
    (window[kCGWindowOwnerPID as String] as? Int32) == pid
        && (window[kCGWindowLayer as String] as? Int) == 0
}.flatMap { window -> CGRect? in
    guard let dict = window[kCGWindowBounds as String] as? [String: Any] else { return nil }
    return CGRect(dictionaryRepresentation: dict as CFDictionary)
}

if let bounds {
    CGWarpMouseCursorPosition(CGPoint(x: bounds.minX + 120, y: bounds.minY + 180))
}

for _ in 0..<events {
    CGEvent(
        scrollWheelEvent2Source: nil,
        units: .pixel,
        wheelCount: 1,
        wheel1: -80,
        wheel2: 0,
        wheel3: 0
    )?.post(tap: .cghidEventTap)
    Thread.sleep(forTimeInterval: delay)
}
SWIFT
SCROLL_PID="$!"

(
  END=$((SECONDS + SAMPLE_SECONDS))
  while [[ $SECONDS -lt $END ]]; do
    ps -p "$PID" -o %cpu=,%mem=,rss= >>"$CPU_OUT"
    sleep 0.2
  done
) &
CPU_PID="$!"

sample "$PID" "$SAMPLE_SECONDS" -file "$SAMPLE_OUT" >/dev/null 2>&1 || true
wait "$CPU_PID" || true
wait "$SCROLL_PID" || true

awk '
  { if ($1 > max_cpu) max_cpu = $1; if ($2 > max_mem) max_mem = $2; samples += 1; cpu += $1 }
  END {
    if (samples == 0) {
      print "no cpu samples"
      exit 1
    }
    printf("pid=%s samples=%d avg_cpu=%.1f max_cpu=%.1f max_mem=%.1f cpu_log=%s sample=%s\n",
      "'"$PID"'", samples, cpu / samples, max_cpu, max_mem, "'"$CPU_OUT"'", "'"$SAMPLE_OUT"'")
  }
' "$CPU_OUT"
