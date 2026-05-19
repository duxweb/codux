import { describe, expect, it } from "vitest";
import { aiIndexingPresentation, liveSessionTotalTokens } from "./panelPresentation";
import type { AISessionSnapshot } from "./types";

describe("ai panel presentation", () => {
  it("keeps queued indexing visibly queued", () => {
    expect(
      aiIndexingPresentation({
        error: null,
        isLoading: true,
        isForegroundIndexing: false,
        statusDetail: "queued",
        progress: 0,
        indexedAt: 0,
      }),
    ).toMatchObject({
      statusKey: "queued",
      text: "Queued for indexing",
      indicator: "spinner",
      showRefreshAction: false,
    });
  });

  it("shows determinate progress for foreground queued indexing", () => {
    expect(
      aiIndexingPresentation({
        error: null,
        isLoading: true,
        isForegroundIndexing: true,
        statusDetail: "queued",
        progress: 0,
        indexedAt: 1_800_000_000,
      }),
    ).toMatchObject({
      statusKey: "queued",
      text: "Queued for indexing",
      indicator: "progress",
      progressValue: 0,
    });
  });

  it("shows determinate progress for foreground full indexing", () => {
    expect(
      aiIndexingPresentation({
        error: null,
        isLoading: true,
        isForegroundIndexing: true,
        statusDetail: "readingSources",
        progress: 0.38,
        indexedAt: 0,
      }),
    ).toMatchObject({
      statusKey: "fullIndexing",
      text: "Indexing all usage",
      indicator: "progress",
      progressValue: 38,
    });
  });

  it("shows determinate progress for manual refresh over a cached snapshot", () => {
    expect(
      aiIndexingPresentation({
        error: null,
        isLoading: true,
        isForegroundIndexing: true,
        statusDetail: "readingSources",
        progress: 0.58,
        indexedAt: 1_800_000_000,
      }),
    ).toMatchObject({
      statusKey: "manualRefreshing",
      text: "Refreshing stats",
      indicator: "progress",
      progressValue: 58,
    });
  });

  it("keeps silent cached indexing as a loading spinner", () => {
    expect(
      aiIndexingPresentation({
        error: null,
        isLoading: true,
        isForegroundIndexing: false,
        statusDetail: "readingSources",
        progress: 0.74,
        indexedAt: 1_800_000_000,
      }),
    ).toMatchObject({
      statusKey: "silentIndexing",
      text: "Indexing in background",
      indicator: "spinner",
    });
  });

  it("uses current live session totals without subtracting the runtime baseline", () => {
    const session = {
      totalTokens: 1_420,
      baselineTotalTokens: 900,
    } as AISessionSnapshot;

    expect(liveSessionTotalTokens(session)).toBe(1_420);
  });
});
