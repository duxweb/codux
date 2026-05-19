import { describe, expect, it } from "vitest";
import { petSnapshotWithLiveTokens, type PetSnapshot } from "./petState";

describe("pet state", () => {
  it("layers live runtime tokens onto persisted pet state without history ownership", () => {
    const snapshot: PetSnapshot = {
      stateVersion: 7,
      statsModelVersion: 3,
      claimedAt: 1,
      species: "voidcat",
      customName: "",
      currentExperienceTokens: 100,
      currentStats: { wisdom: 1, chaos: 2, night: 3, stamina: 4, empathy: 5 },
      personaId: "balanced",
      progress: {
        level: 1,
        xpInLevel: 100,
        xpForLevel: 2_000_000,
        totalXp: 100,
        progress: 0.00005,
        isAtMaxLevel: false,
      },
      statsUpdatedDay: 1,
      globalNormalizedTotalWatermark: 100,
      projectNormalizedTokenWatermarks: {},
      totalNormalizedTokens: 100,
      dailyExperienceTokens: 10,
      dailyExperienceDay: 1,
      legacy: [],
      updatedAt: 1,
    };

    const visible = petSnapshotWithLiveTokens(snapshot, 25);

    expect(visible.currentExperienceTokens).toBe(125);
    expect(visible.progress.totalXp).toBe(125);
    expect(visible.totalNormalizedTokens).toBe(125);
    expect(visible.dailyExperienceTokens).toBe(35);
    expect(snapshot.currentExperienceTokens).toBe(100);
  });

  it("does not add live runtime xp before the pet is claimed", () => {
    const snapshot: PetSnapshot = {
      stateVersion: 8,
      statsModelVersion: 3,
      claimedAt: null,
      species: "voidcat",
      customName: "",
      currentExperienceTokens: 0,
      currentStats: { wisdom: 0, chaos: 0, night: 0, stamina: 0, empathy: 0 },
      personaId: "observer",
      progress: {
        level: 1,
        xpInLevel: 0,
        xpForLevel: 0,
        totalXp: 0,
        progress: 0,
        isAtMaxLevel: false,
      },
      statsUpdatedDay: null,
      globalNormalizedTotalWatermark: null,
      projectNormalizedTokenWatermarks: {},
      totalNormalizedTokens: 100,
      dailyExperienceTokens: 0,
      dailyExperienceDay: 1,
      legacy: [],
      updatedAt: 1,
    };

    const visible = petSnapshotWithLiveTokens(snapshot, 25);

    expect(visible.currentExperienceTokens).toBe(0);
    expect(visible.totalNormalizedTokens).toBe(125);
    expect(visible.dailyExperienceTokens).toBe(0);
  });
});
