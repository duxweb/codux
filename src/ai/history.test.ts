import { describe, expect, it } from "vitest";
import { shouldApplyAIHistoryProjectState, shouldLoadGlobalHistory } from "./history";

describe("ai history", () => {
  it("does not load global history while the owning view is collapsed", () => {
    expect(shouldLoadGlobalHistory(false, 2)).toBe(false);
    expect(shouldLoadGlobalHistory(true, 0)).toBe(false);
    expect(shouldLoadGlobalHistory(true, 2)).toBe(true);
  });

  it("rejects stale project states so queued responses cannot overwrite completion", () => {
    expect(shouldApplyAIHistoryProjectState({ version: 3 }, 4)).toBe(false);
    expect(shouldApplyAIHistoryProjectState({ version: 4 }, 4)).toBe(true);
    expect(shouldApplyAIHistoryProjectState({ version: 5 }, 4)).toBe(true);
  });
});
