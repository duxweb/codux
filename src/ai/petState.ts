import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { useCallback, useEffect, useMemo, useState } from "react";
import { aiRuntime, useAIRuntimeSnapshot } from "./runtime";
import type { WorkspaceProject } from "../types";

export type PetStats = {
  wisdom: number;
  chaos: number;
  night: number;
  stamina: number;
  empathy: number;
};

export type PetPersonaId =
  | "observer"
  | "balanced"
  | "midnight_thinker"
  | "philosopher"
  | "mad_scientist"
  | "night_companion"
  | "debug_comrade"
  | "night_owl"
  | "firebrand"
  | "action_seeker"
  | "marathoner"
  | "steady_type"
  | "debug_buddy"
  | "wise_type";

export type PetSnapshot = {
  stateVersion: number;
  statsModelVersion: number;
  claimedAt?: number | null;
  species: string;
  customPet?: PetCustomPet | null;
  customName: string;
  currentExperienceTokens: number;
  currentStats: PetStats;
  personaId: PetPersonaId;
  progress: PetProgressInfo;
  statsUpdatedDay?: number | null;
  globalNormalizedTotalWatermark?: number | null;
  projectNormalizedTokenWatermarks: Record<string, number>;
  totalNormalizedTokens: number;
  dailyExperienceTokens: number;
  dailyExperienceDay?: number | null;
  legacy: PetLegacyRecord[];
  updatedAt: number;
};

export type PetLegacyRecord = {
  id: string;
  species: string;
  customPet?: PetCustomPet | null;
  customName: string;
  totalXp: number;
  stats: PetStats;
  personaId: PetPersonaId;
  progress: PetProgressInfo;
  retiredAt: number;
};

export type PetProgressInfo = {
  level: number;
  xpInLevel: number;
  xpForLevel: number;
  totalXp: number;
  progress: number;
  isAtMaxLevel: boolean;
};

export type PetCatalogItem = {
  species: string;
  assetFolder: string;
  manifestId: string;
  nameKey: string;
  claimTitleKey: string;
  subtitleKey: string;
  descriptionKey: string;
};

export type PetCustomPet = {
  id: string;
  displayName: string;
  description: string;
  spritesheetPath: string;
  directoryName: string;
  spritesheetDataUrl?: string | null;
  sourcePageUrl?: string | null;
  sourceZipUrl?: string | null;
  installedAt?: number | null;
};

export type PetCustomPetInstallPreview = {
  pageUrl: string;
  zipUrl: string;
  slug: string;
  displayName: string;
  description: string;
  imageUrl?: string | null;
};

export type PetAnimationSpec = {
  state: string;
  row: number;
  frameDurationsMs: number[];
};

export type PetCatalog = {
  species: PetCatalogItem[];
  customPets: PetCustomPet[];
  atlas: {
    columns: number;
    rows: number;
    cellWidth: number;
    cellHeight: number;
    animations: PetAnimationSpec[];
  };
};

export type PetLedger = {
  snapshot: PetSnapshot;
  dailyTokens: number;
  isLoading: boolean;
  error: string | null;
  refresh: () => Promise<void>;
  claim: (species: string, customName?: string, customPet?: PetCustomPet | null) => Promise<void>;
  rename: (customName: string) => Promise<void>;
  archiveCurrent: () => Promise<void>;
  restoreArchived: (legacyId: string) => Promise<void>;
};

const emptyStats: PetStats = {
  wisdom: 0,
  chaos: 0,
  night: 0,
  stamina: 0,
  empathy: 0,
};

export function usePetLedger(projects: WorkspaceProject[] = []): PetLedger {
  const runtime = useAIRuntimeSnapshot();
  const [snapshot, setSnapshot] = useState<PetSnapshot>(() => emptyPetSnapshot(0, emptyStats));
  const [isSnapshotLoading, setSnapshotLoading] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const liveTokens = runtime.globalTotals.totalTokens;
  const visibleSnapshot = useMemo(() => petSnapshotWithLiveTokens(snapshot, liveTokens), [liveTokens, snapshot]);

  const loadSnapshot = useCallback(async () => {
    const runtimeFallbackSnapshot = emptyPetSnapshot(aiRuntime.projectTotals().totalTokens, emptyStats);
    if (!window.__TAURI_INTERNALS__) {
      setSnapshot(runtimeFallbackSnapshot);
      setError(null);
      return;
    }
    setSnapshotLoading(true);
    setError(null);
    try {
      const next = projects.length > 0 ? await refreshPetLedger(projects) : await invoke<PetSnapshot>("pet_snapshot");
      setSnapshot(next);
    } catch (reason) {
      setError(reason instanceof Error ? reason.message : String(reason));
      setSnapshot(runtimeFallbackSnapshot);
    } finally {
      setSnapshotLoading(false);
    }
  }, [projects]);

  const refresh = useCallback(async () => {
    await loadSnapshot();
  }, [loadSnapshot]);

  const claim = useCallback(
    async (species: string, customName = "", customPet: PetCustomPet | null = null) => {
      if (!window.__TAURI_INTERNALS__) {
        setSnapshot((current) => ({
          ...current,
          claimedAt: Math.floor(Date.now() / 1000),
          species,
          customPet,
          customName,
          currentExperienceTokens: 0,
          progress: emptyProgress(),
        }));
        return;
      }
      setSnapshotLoading(true);
      setError(null);
      try {
        const next = await invoke<PetSnapshot>("pet_claim", {
          request: {
            species,
            customName,
            customPet,
            projects: projects.map(projectRequest),
          },
        });
        setSnapshot(next);
      } catch (reason) {
        setError(reason instanceof Error ? reason.message : String(reason));
      } finally {
        setSnapshotLoading(false);
      }
    },
    [projects],
  );

  const rename = useCallback(async (customName: string) => {
    if (!window.__TAURI_INTERNALS__) {
      setSnapshot((current) => ({ ...current, customName }));
      return;
    }
    const next = await invoke<PetSnapshot>("pet_rename", { request: { customName } });
    setSnapshot(next);
  }, []);

  const archiveCurrent = useCallback(async () => {
    if (!window.__TAURI_INTERNALS__) {
      setSnapshot((current) => ({
        ...emptyPetSnapshot(current.totalNormalizedTokens, emptyStats),
        legacy: [
          {
            id: `legacy-${Date.now()}`,
            species: current.species,
            customPet: current.customPet ?? null,
            customName: current.customName,
            totalXp: current.currentExperienceTokens,
            stats: current.currentStats,
            personaId: current.personaId,
            progress: current.progress,
            retiredAt: Math.floor(Date.now() / 1000),
          },
          ...current.legacy,
        ],
      }));
      return;
    }
    const next = await invoke<PetSnapshot>("pet_archive_current");
    setSnapshot(next);
  }, []);

  const restoreArchived = useCallback(async (legacyId: string) => {
    if (!window.__TAURI_INTERNALS__) return;
    const next = await invoke<PetSnapshot>("pet_restore_archived", {
      request: { legacyId },
    });
    setSnapshot(next);
  }, []);

  useEffect(() => {
    void loadSnapshot();
  }, [loadSnapshot]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    let isDisposed = false;
    let unlisten: (() => void) | undefined;
    void listen<PetSnapshot>("pet:updated", (event) => {
      if (!isDisposed) {
        setSnapshot(event.payload);
      }
    })
      .then((dispose) => {
        if (isDisposed) {
          dispose();
          return;
        }
        unlisten = dispose;
      })
      .catch((reason) => console.error("failed to listen for pet updates", reason));
    return () => {
      isDisposed = true;
      unlisten?.();
    };
  }, []);

  return {
    snapshot: visibleSnapshot,
    dailyTokens: Math.max(0, visibleSnapshot.dailyExperienceTokens),
    isLoading: isSnapshotLoading,
    error,
    refresh,
    claim,
    rename,
    archiveCurrent,
    restoreArchived,
  };
}

export async function loadPetCatalog(): Promise<PetCatalog> {
  if (!window.__TAURI_INTERNALS__) return defaultPetCatalog();
  return invoke<PetCatalog>("pet_catalog");
}

export async function previewCustomPetInstall(pageUrl: string, displayName = ""): Promise<PetCustomPetInstallPreview> {
  return invoke<PetCustomPetInstallPreview>("pet_custom_install_preview", {
    request: { pageUrl, displayName },
  });
}

export async function installCustomPet(pageUrl: string, displayName = ""): Promise<PetCustomPet> {
  return invoke<PetCustomPet>("pet_custom_install", {
    request: { pageUrl, displayName },
  });
}

async function refreshPetLedger(projects: WorkspaceProject[]) {
  return invoke<PetSnapshot>("pet_refresh", {
    request: {
      projects: projects.map(projectRequest),
    },
  });
}

function projectRequest(project: WorkspaceProject) {
  const id = project.rootProjectId ?? project.id;
  const name = project.name.split(" · ")[0] || project.name;
  return {
    id,
    name,
    path: project.path,
  };
}

export function petSnapshotWithLiveTokens(snapshot: PetSnapshot, liveTokens: number): PetSnapshot {
  const safeLiveTokens = Math.max(0, Math.floor(liveTokens));
  if (safeLiveTokens <= 0) return snapshot;
  return {
    ...snapshot,
    currentExperienceTokens: snapshot.claimedAt
      ? snapshot.currentExperienceTokens + safeLiveTokens
      : snapshot.currentExperienceTokens,
    progress: snapshot.claimedAt
      ? {
          ...snapshot.progress,
          totalXp: snapshot.progress.totalXp + safeLiveTokens,
        }
      : snapshot.progress,
    totalNormalizedTokens: snapshot.totalNormalizedTokens + safeLiveTokens,
    dailyExperienceTokens: snapshot.claimedAt
      ? Math.max(0, snapshot.dailyExperienceTokens) + safeLiveTokens
      : Math.max(0, snapshot.dailyExperienceTokens),
  };
}

function emptyPetSnapshot(totalTokens: number, stats: PetStats): PetSnapshot {
  const now = Math.floor(Date.now() / 1000);
  return {
    stateVersion: 8,
    statsModelVersion: 3,
    claimedAt: null,
    species: "voidcat",
    customPet: null,
    customName: "",
    currentExperienceTokens: 0,
    currentStats: stats,
    personaId: "observer",
    progress: emptyProgress(),
    statsUpdatedDay: null,
    globalNormalizedTotalWatermark: null,
    projectNormalizedTokenWatermarks: {},
    totalNormalizedTokens: Math.max(0, Math.floor(totalTokens)),
    dailyExperienceTokens: 0,
    dailyExperienceDay: Math.floor(now / 86_400),
    legacy: [],
    updatedAt: now,
  };
}

function emptyProgress(): PetProgressInfo {
  return {
    level: 1,
    xpInLevel: 0,
    xpForLevel: 0,
    totalXp: 0,
    progress: 0,
    isAtMaxLevel: false,
  };
}

export function defaultPetCatalog(): PetCatalog {
  const species = [
    "voidcat",
    "rusthound",
    "goose",
    "chaossprite",
    "code",
    "sheep",
    "ox",
    "dragon",
    "phoenix",
    "dolphin",
    "penguin",
    "panda",
  ];
  return {
    species: species.map((item) => ({
      species: item,
      assetFolder: item,
      manifestId: `${item}-default`,
      nameKey: `pet.species.${item}.base`,
      claimTitleKey: `pet.claim.${item}.title`,
      subtitleKey: `pet.claim.${item}.subtitle`,
      descriptionKey: `pet.claim.${item}.description`,
    })),
    customPets: [],
    atlas: {
      columns: 8,
      rows: 9,
      cellWidth: 192,
      cellHeight: 208,
      animations: [
        { state: "idle", row: 0, frameDurationsMs: [280, 110, 110, 140, 140, 320] },
        { state: "running-right", row: 1, frameDurationsMs: [120, 120, 120, 120, 120, 120, 120, 220] },
        { state: "running-left", row: 2, frameDurationsMs: [120, 120, 120, 120, 120, 120, 120, 220] },
        { state: "waving", row: 3, frameDurationsMs: [140, 140, 140, 280] },
        { state: "jumping", row: 4, frameDurationsMs: [140, 140, 140, 140, 280] },
        { state: "failed", row: 5, frameDurationsMs: [140, 140, 140, 140, 140, 140, 140, 240] },
        { state: "waiting", row: 6, frameDurationsMs: [150, 150, 150, 150, 150, 260] },
        { state: "running", row: 7, frameDurationsMs: [120, 120, 120, 120, 120, 220] },
        { state: "review", row: 8, frameDurationsMs: [150, 150, 150, 150, 150, 280] },
      ],
    },
  };
}
