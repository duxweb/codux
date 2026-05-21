import { describe, expect, it } from "vitest";
import { activePetFrameCount } from "./petAnimation";

describe("pet animation", () => {
  it("uses detected non-empty frames before transparent atlas padding", () => {
    expect(activePetFrameCount([0, 0, 0, 3], { row: 3, frameDurationsMs: [140, 140, 140, 280] })).toBe(3);
  });

  it("falls back to configured frame count when detection is unavailable", () => {
    expect(activePetFrameCount(null, { row: 3, frameDurationsMs: [140, 140, 140, 280] })).toBe(4);
    expect(activePetFrameCount([0, 0, 0, 0], { row: 3, frameDurationsMs: [140, 140, 140, 280] })).toBe(4);
  });

  it("never plays more frames than the animation defines", () => {
    expect(activePetFrameCount([0, 0, 0, 8], { row: 3, frameDurationsMs: [140, 140, 140, 280] })).toBe(4);
  });
});
