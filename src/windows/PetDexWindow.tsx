import { ArchiveBoxIcon, ArrowUturnLeftIcon, BookOpenIcon, PlusIcon } from "@heroicons/react/24/outline";
import { useEffect, useMemo, useState } from "react";
import { openExternalUrl } from "../appActions";
import { listenPetCustomPetInstalled } from "../ai/petCustomEvents";
import {
  loadPetCatalog,
  loadCustomPetSprite,
  usePetLedger,
  type PetCatalog,
  type PetCatalogItem,
  type PetCustomPet,
  type PetLegacyRecord,
  type PetStats,
} from "../ai/petState";
import { Button } from "../components/Button";
import { PetSprite } from "../components/PetSprite";
import { PressableButton } from "../components/PressableButton";
import { systemConfirm, systemMessage } from "../systemDialog";
import { formatI18n, tm } from "../i18n";
import { openAppWindow } from "../windowing";
import { WindowFrame } from "./WindowFrame";

const TRAIT_MAX = 330;
const petAccentColors: Record<string, string> = {
  voidcat: "#6A5CFF",
  rusthound: "#FF8A3D",
  goose: "#3E86F6",
  chaossprite: "#FF4FA3",
  code: "#2F8FFF",
  sheep: "#F28FB8",
  ox: "#F3B43F",
  dragon: "#E04435",
  phoenix: "#FF7A22",
  dolphin: "#1E9BFF",
  penguin: "#5C6D85",
  panda: "#6A6F78",
};

type SpotlightIdentity = { kind: "bundled"; item: PetCatalogItem } | { kind: "custom"; pet: PetCustomPet };

export function PetDexWindow() {
  const pet = usePetLedger([]);
  const [catalog, setCatalog] = useState<PetCatalog | null>(null);
  const [isWorking, setWorking] = useState(false);
  const [spotlight, setSpotlight] = useState<SpotlightIdentity | null>(null);
  const currentCustomPet = useHydratedCustomPet(pet.snapshot.customPet ?? null);

  useEffect(() => {
    void reloadCatalog();
  }, []);

  useEffect(() => {
    let isDisposed = false;
    let unlisten: (() => void) | undefined;
    void listenPetCustomPetInstalled(async (installed) => {
      await reloadCatalog();
      if (!isDisposed) setSpotlight({ kind: "custom", pet: installed });
    }).then((dispose) => {
      if (isDisposed) {
        dispose();
        return;
      }
      unlisten = dispose;
    });
    return () => {
      isDisposed = true;
      unlisten?.();
    };
  }, []);

  const reloadCatalog = async () => {
    try {
      setCatalog(await loadPetCatalog());
    } catch (error) {
      console.error("failed to load pet catalog", error);
    }
  };

  const restore = async (record: PetLegacyRecord) => {
    setWorking(true);
    try {
      await pet.restoreArchived(record.id);
      await systemMessage(tm("pet.archive.restore.success", "Restored archived pet."), {
        title: tm("pet.dex.title", "Pet Dex"),
        kind: "info",
        buttons: { ok: "OK" },
      });
    } catch (error) {
      await systemMessage(error instanceof Error ? error.message : String(error), {
        title: tm("settings.pet.action_failed", "Pet Action Failed"),
        kind: "warning",
        buttons: { ok: "OK" },
      });
    } finally {
      setWorking(false);
    }
  };

  const archiveCurrent = async () => {
    const confirmed = await systemConfirm(
      tm("pet.archive.alert.message", "Archive this pet into the dex and choose a new companion."),
      {
        title: tm("pet.archive.alert.title", "Archive Current Pet"),
        kind: "warning",
        okLabel: tm("pet.archive.confirm", "Confirm Archive"),
        cancelLabel: tm("common.cancel", "Cancel"),
      },
    );
    if (!confirmed) return;
    setWorking(true);
    try {
      await pet.archiveCurrent();
      await systemMessage(tm("pet.archive.success", "Archived current pet."), {
        title: tm("pet.dex.title", "Pet Dex"),
        kind: "info",
        buttons: { ok: "OK" },
      });
    } catch (error) {
      await systemMessage(error instanceof Error ? error.message : String(error), {
        title: tm("settings.pet.action_failed", "Pet Action Failed"),
        kind: "warning",
        buttons: { ok: "OK" },
      });
    } finally {
      setWorking(false);
    }
  };

  const unlockedSpecies = useMemo(() => {
    const values = new Set<string>();
    if (pet.snapshot.claimedAt && !pet.snapshot.customPet) values.add(pet.snapshot.species);
    for (const record of pet.snapshot.legacy) {
      if (!record.customPet) values.add(record.species);
    }
    return values;
  }, [pet.snapshot.claimedAt, pet.snapshot.customPet, pet.snapshot.legacy, pet.snapshot.species]);

  const species = catalog?.species ?? [];
  const customPets = catalog?.customPets ?? [];
  const unlockedCount = unlockedSpecies.size + customPets.length;
  const totalCount = species.length + customPets.length;
  const currentInfo = pet.snapshot.progress;
  const currentName = petDisplayName(pet.snapshot.species, pet.snapshot.customPet, pet.snapshot.customName);

  return (
    <WindowFrame title={tm("pet.dex.title", "Petdex")} mainClassName="px-0 py-0" mainScrollable={false}>
      <div className="relative grid h-full min-h-0 grid-cols-[270px_minmax(0,1fr)] overflow-hidden">
        <aside className="min-h-0 border-r border-line/60 bg-fill/[0.02]">
          <div className="flex h-full flex-col">
            <div className="min-h-0 flex-1 overflow-y-auto scrollbar-overlay p-4">
              <div className="flex items-center gap-2">
                <BookOpenIcon className="h-[18px] w-[18px] text-ink-mute" />
                <div className="text-[17px] font-bold">{tm("pet.dex.title", "Pet Dex")}</div>
              </div>
              <div className="mt-1 text-xs text-ink-mute">
                {tm("pet.dex.subtitle", "A record of every coding companion you've raised")}
              </div>

              <div className="my-4 h-px bg-line" />

              <div className="grid gap-2">
                <DexStat
                  label={tm("pet.dex.current_companion", "Current Companion")}
                  sub={
                    pet.snapshot.claimedAt
                      ? formatI18n(tm("pet.dex.current_level_format", "Lv.%@"), currentInfo.level)
                      : tm("pet.dex.unclaimed", "Not Claimed")
                  }
                  value={pet.snapshot.claimedAt ? currentName : tm("pet.dex.unclaimed", "Not Claimed")}
                />
                <DexStat
                  label={tm("pet.dex.archived", "Archived")}
                  sub={
                    pet.snapshot.legacy.length === 0
                      ? tm("pet.dex.archived.none", "No archived pets yet")
                      : tm("pet.dex.archived.history", "Past companions")
                  }
                  value={String(pet.snapshot.legacy.length)}
                />
                <DexStat
                  label={tm("pet.dex.collection", "Dex Collection")}
                  sub={
                    unlockedCount === totalCount
                      ? tm("pet.dex.collection.complete", "All companions unlocked")
                      : tm("pet.dex.collection.continue", "Keep exploring")
                  }
                  value={`${unlockedCount}/${totalCount || species.length}`}
                />
              </div>

              <div className="my-4 h-px bg-line" />

              {pet.snapshot.claimedAt ? (
                <CurrentPetDetail
                  name={currentName}
                  species={pet.snapshot.species}
                  customPet={currentCustomPet}
                  stats={pet.snapshot.currentStats}
                  totalXp={currentInfo.totalXp}
                  level={currentInfo.level}
                  staticMode
                  claimedAt={pet.snapshot.claimedAt}
                />
              ) : (
                <div className="grid min-h-[140px] place-items-center text-center text-xs text-ink-mute">
                  {tm("pet.dex.no_current_pet", "No active pet yet")}
                </div>
              )}
            </div>

            <div className="border-t border-line/60 p-4">
              {pet.snapshot.claimedAt ? (
                <Button
                  block
                  variant="primary"
                  leading={ArchiveBoxIcon}
                  disabled={isWorking}
                  onPress={() => void archiveCurrent()}
                >
                  {tm("pet.archive.action", "Archive")}
                </Button>
              ) : (
                <Button block variant="primary" onPress={() => void openAppWindow("pet-claim")}>
                  {tm("pet.claim.action", "Claim Pet")}
                </Button>
              )}
              <Button
                block
                className="mt-2"
                variant="secondary"
                leading={PlusIcon}
                onPress={() => void openAppWindow("pet-custom-install")}
              >
                {tm("pet.custom.install.action", "Add Custom Pet")}
              </Button>
            </div>
          </div>
        </aside>

        <section className="min-h-0 overflow-y-auto scrollbar-overlay p-5">
          <div className="grid gap-6">
            <div>
              <div className="mb-3 flex items-center justify-between">
                <div className="text-[15px] font-bold">{tm("pet.dex.bundled.section", "Bundled Pets")}</div>
                <div className="text-xs font-medium text-ink-mute">
                  {formatI18n(tm("pet.dex.unlocked_count", "%@/%@ unlocked"), unlockedSpecies.size, species.length)}
                </div>
              </div>
              <div className="grid grid-cols-4 gap-3">
                {species.map((item) => (
                  <SpeciesCard
                    key={item.species}
                    item={item}
                    unlocked={unlockedSpecies.has(item.species)}
                    onSelect={() => {
                      if (unlockedSpecies.has(item.species)) setSpotlight({ kind: "bundled", item });
                    }}
                  />
                ))}
              </div>
            </div>

            <div>
              <div className="mb-3 flex items-center justify-between">
                <div className="text-[15px] font-bold">{tm("pet.claim.custom.section", "Custom Pets")}</div>
                <div className="text-xs font-medium text-ink-mute">
                  {formatI18n(tm("pet.custom.installed_count", "%@ installed"), customPets.length)}
                </div>
              </div>
              {customPets.length === 0 ? (
                <div className="rounded-[10px] border border-line bg-fill/[0.02] px-4 py-6 text-center text-xs text-ink-faint">
                  {tm(
                    "pet.custom.install.subtitle",
                    "Paste a Petdex page, verify the package, then install it into Codux.",
                  )}
                </div>
              ) : (
                <div className="grid grid-cols-4 gap-3">
                  {customPets.map((item) => (
                    <CustomPetCard
                      key={item.id}
                      pet={item}
                      onSelect={() => setSpotlight({ kind: "custom", pet: item })}
                    />
                  ))}
                </div>
              )}
            </div>

            <div>
              <div className="mb-3 text-[15px] font-bold">{tm("pet.archive.history", "Archive History")}</div>
              {pet.snapshot.legacy.length === 0 ? (
                <div className="rounded-[10px] border border-line bg-fill/[0.02] px-4 py-6 text-center text-xs text-ink-faint">
                  {tm("pet.dex.archived.none", "No archived pets yet")}
                </div>
              ) : (
                <div className="grid gap-2">
                  {pet.snapshot.legacy.map((record) => (
                    <HistoryRow
                      key={record.id}
                      record={record}
                      disabled={isWorking}
                      onRestore={() => void restore(record)}
                    />
                  ))}
                </div>
              )}
            </div>
          </div>
        </section>

        {spotlight && (
          <DexSpotlightOverlay
            identity={spotlight}
            staticMode
            onClose={() => setSpotlight(null)}
          />
        )}
      </div>
    </WindowFrame>
  );
}

function DexStat({ label, sub, value }: { label: string; sub: string; value: string }) {
  return (
    <div className="flex items-center justify-between gap-3 rounded-[8px] bg-fill/[0.04] px-3 py-2.5">
      <div className="min-w-0">
        <div className="text-xs font-semibold text-ink-mute">{label}</div>
        <div className="mt-0.5 truncate text-xs text-ink-faint">{sub}</div>
      </div>
      <div className="max-w-[96px] truncate text-right text-sm font-bold text-ink">{value}</div>
    </div>
  );
}

function CurrentPetDetail({
  name,
  species,
  customPet,
  stats,
  totalXp,
  level,
  staticMode,
  claimedAt,
}: {
  name: string;
  species: string;
  customPet?: PetCustomPet | null;
  stats: PetStats;
  totalXp: number;
  level: number;
  staticMode: boolean;
  claimedAt?: number | null;
}) {
  return (
    <div>
      <div className="mb-2 text-xs font-semibold text-ink-mute">{tm("pet.dex.current_pet", "Current Pet")}</div>
      <div className="grid justify-items-center gap-2">
        <PetSprite species={species} src={customPet?.spritesheetDataUrl} size={84} staticMode={staticMode} />
        <div className="text-sm font-bold">{name}</div>
        <div className="text-xs text-ink-mute">
          {customPet?.displayName ? `${customPet.displayName} · ` : ""}
          {formatI18n(tm("pet.dex.current_level_format", "Lv.%@"), level)}
        </div>
      </div>
      <div className="mt-4 grid gap-2">
        <TraitBar emoji="🧠" label={tm("pet.attribute.wisdom", "Wisdom")} value={stats.wisdom} color="#2F8FFF" />
        <TraitBar emoji="🔥" label={tm("pet.attribute.chaos", "Chaos")} value={stats.chaos} color="#FF6030" />
        <TraitBar emoji="🌙" label={tm("pet.attribute.night", "Night")} value={stats.night} color="#6060CC" />
        <TraitBar emoji="💪" label={tm("pet.attribute.stamina", "Stamina")} value={stats.stamina} color="#20A060" />
        <TraitBar emoji="🩹" label={tm("pet.attribute.empathy", "Empathy")} value={stats.empathy} color="#E060A0" />
      </div>
      <div className="mt-3 flex items-center justify-between gap-2">
        <span className="rounded-full bg-brand-blue/12 px-2 py-1 text-xs font-medium text-brand-blue">
          {tm("pet.stage.companion", "Companion")}
        </span>
        {claimedAt && <span className="text-xs text-ink-faint">{formatDate(claimedAt)}</span>}
      </div>
      <div className="mt-3 flex items-center justify-between rounded-[8px] bg-fill/[0.04] px-3 py-2 text-xs">
        <span className="text-ink-mute">{tm("pet.total_xp", "Total XP")}</span>
        <span className="font-semibold text-ink">{compactNumber(totalXp)}</span>
      </div>
    </div>
  );
}

function TraitBar({ emoji, label, value, color }: { emoji: string; label: string; value: number; color: string }) {
  return (
    <div className="grid grid-cols-[18px_34px_minmax(0,1fr)_34px] items-center gap-1.5 text-xs">
      <span>{emoji}</span>
      <span className="font-medium text-ink-mute">{label}</span>
      <div className="h-1 overflow-hidden rounded-full" style={{ backgroundColor: `${color}20` }}>
        <div
          className="h-full rounded-full"
          style={{ width: `${Math.min(100, (value / TRAIT_MAX) * 100)}%`, backgroundColor: `${color}bf` }}
        />
      </div>
      <span className="text-right font-mono font-semibold text-ink-soft">{compactNumber(value)}</span>
    </div>
  );
}

function SpeciesCard({ item, unlocked, onSelect }: { item: PetCatalogItem; unlocked: boolean; onSelect: () => void }) {
  const accent = petAccentColors[item.species] ?? "#2F8FFF";
  return (
    <PressableButton
      onPressUp={onSelect}
      className={`grid min-h-[136px] justify-items-center rounded-[8px] border px-2.5 py-3 text-center transition-colors ${
        unlocked
          ? "border-brand-blue/25 bg-fill/[0.035] hover:bg-fill/[0.06]"
          : "border-line bg-fill/[0.025] opacity-80"
      }`}
    >
      <div
        className="grid h-14 w-14 place-items-center rounded-full"
        style={{ backgroundColor: unlocked ? `${accent}1f` : "rgba(255,255,255,0.04)" }}
      >
        {unlocked ? (
          <PetSprite species={item.species} size={44} staticMode />
        ) : (
          <span className="text-2xl font-bold text-ink-faint">?</span>
        )}
      </div>
      <div className="mt-2 max-w-full truncate text-xs font-semibold">
        {unlocked ? tm(item.nameKey, item.species) : tm("pet.dex.unknown", "???")}
      </div>
      <div className="mt-1 text-xs text-ink-mute">
        {unlocked ? tm("pet.stage.companion", "Companion") : tm("pet.dex.locked", "Locked")}
      </div>
    </PressableButton>
  );
}

function CustomPetCard({ pet, onSelect }: { pet: PetCustomPet; onSelect: () => void }) {
  return (
    <PressableButton
      onPressUp={onSelect}
      className="grid min-h-[136px] justify-items-center rounded-[8px] border border-brand-blue/25 bg-fill/[0.035] px-2.5 py-3 text-center transition-colors hover:bg-fill/[0.06]"
    >
      <div className="grid h-14 w-14 place-items-center rounded-full bg-brand-blue/12">
        <PetSprite species="voidcat" src={pet.spritesheetDataUrl} size={44} staticMode />
      </div>
      <div className="mt-2 max-w-full truncate text-xs font-semibold">{pet.displayName}</div>
      <div className="mt-1 text-xs text-brand-blue">{tm("pet.custom.installed", "Custom pet")}</div>
    </PressableButton>
  );
}

function HistoryRow({
  record,
  disabled,
  onRestore,
}: {
  record: PetLegacyRecord;
  disabled: boolean;
  onRestore: () => void;
}) {
  const info = record.progress;
  const name = petDisplayName(record.species, record.customPet, record.customName);
  return (
    <div className="grid grid-cols-[44px_minmax(0,1fr)_110px_30px] items-center gap-3 rounded-[9px] bg-fill/[0.035] px-3 py-2">
      <div className="grid h-11 w-11 place-items-center rounded-[9px] bg-fill/[0.05]">
        <PetSprite
          species={record.species}
          src={record.customPet?.spritesheetDataUrl}
          state="idle"
          size={38}
          staticMode
        />
      </div>
      <div className="min-w-0">
        <div className="flex min-w-0 items-center gap-2">
          <span className="truncate text-sm font-semibold text-ink">{name}</span>
          <span className="rounded-full bg-brand-blue/12 px-2 py-0.5 text-xs font-medium text-brand-blue">
            {tm("pet.stage.companion", "Companion")}
          </span>
        </div>
        <div className="mt-0.5 truncate text-xs text-ink-faint">
          {formatI18n(tm("pet.archive.xp_format", "%@ XP"), compactNumber(record.totalXp))} ·{" "}
          {formatI18n(tm("pet.dex.current_level_format", "Lv.%@"), info.level)}
        </div>
      </div>
      <div className="text-right text-xs text-ink-faint">{formatDate(record.retiredAt)}</div>
      <Button
        isIconOnly
        size="sm"
        variant="ghost"
        disabled={disabled}
        aria-label={tm("pet.archive.restore.action", "Restore")}
        onPress={onRestore}
      >
        <ArrowUturnLeftIcon className="h-4 w-4" />
      </Button>
    </div>
  );
}

function DexSpotlightOverlay({
  identity,
  staticMode,
  onClose,
}: {
  identity: SpotlightIdentity;
  staticMode: boolean;
  onClose: () => void;
}) {
  const isCustom = identity.kind === "custom";
  const title = isCustom ? identity.pet.displayName : tm(identity.item.nameKey, identity.item.species);
  const subtitle = isCustom ? tm("pet.custom.installed", "Custom pet") : tm("pet.stage.companion", "Companion");
  const description = isCustom
    ? identity.pet.description
    : tm(identity.item.descriptionKey, tm(identity.item.subtitleKey, ""));
  const accent = isCustom ? "#2F8FFF" : (petAccentColors[identity.item.species] ?? "#2F8FFF");
  const sourcePageUrl = isCustom ? identity.pet.sourcePageUrl : null;

  return (
    <div className="absolute inset-0 z-30 grid place-items-center bg-black/35 p-6 no-drag" onMouseDown={onClose}>
      <div
        className="w-[360px] rounded-[14px] border border-line-strong bg-surface-chrome p-5 text-center text-ink shadow-pop"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <div
          className="mx-auto grid h-[116px] w-[116px] place-items-center rounded-full"
          style={{ backgroundColor: `${accent}16` }}
        >
          {isCustom ? (
            <PetSprite species="voidcat" src={identity.pet.spritesheetDataUrl} size={94} staticMode={staticMode} />
          ) : (
            <PetSprite species={identity.item.species} size={94} staticMode={staticMode} />
          )}
        </div>
        <div className="mt-4 text-[18px] font-bold">{title}</div>
        <div className="mt-1 text-xs font-medium" style={{ color: accent }}>
          {subtitle}
        </div>
        {description && <div className="mt-3 text-xs leading-5 text-ink-mute">{description}</div>}
        <div className="mt-5 flex justify-center gap-2">
          {sourcePageUrl && (
            <Button variant="secondary" onPress={() => void openExternalUrl(sourcePageUrl)}>
              {tm("pet.custom.market.action", "Get Pets")}
            </Button>
          )}
          <Button variant="primary" onPress={onClose}>
            {tm("common.close", "Close")}
          </Button>
        </div>
      </div>
    </div>
  );
}

function useHydratedCustomPet(pet: PetCustomPet | null): PetCustomPet | null {
  const [hydrated, setHydrated] = useState<PetCustomPet | null>(pet?.spritesheetDataUrl ? pet : null);
  const key = pet ? `${pet.directoryName}/${pet.spritesheetPath}` : "";

  useEffect(() => {
    let cancelled = false;
    if (!pet) {
      setHydrated(null);
      return;
    }
    if (pet.spritesheetDataUrl) {
      setHydrated(pet);
      return;
    }
    setHydrated(pet);
    void loadCustomPetSprite(pet).then((next) => {
      if (!cancelled) setHydrated(next);
    });
    return () => {
      cancelled = true;
    };
  }, [key, pet]);

  return hydrated;
}

function petDisplayName(species: string, customPet?: PetCustomPet | null, customName?: string) {
  const base = customPet?.displayName || tm(`pet.species.${species}.base`, species.replace(/^custom:/, ""));
  const trimmed = customName?.trim();
  return trimmed || base;
}

function compactNumber(value: number) {
  const absolute = Math.abs(Math.floor(value));
  const sign = value < 0 ? "-" : "";
  const compact = (divisor: number, suffix: string) => {
    const scaled = absolute / divisor;
    const digits = scaled >= 100 ? scaled.toFixed(0) : scaled >= 10 ? scaled.toFixed(1) : scaled.toFixed(2);
    return `${sign}${digits.replace(/\.?0+$/, "")}${suffix}`;
  };
  if (absolute >= 1_000_000_000) return compact(1_000_000_000, "B");
  if (absolute >= 1_000_000) return compact(1_000_000, "M");
  if (absolute >= 1_000) return compact(1_000, "K");
  return `${Math.floor(value)}`;
}

function formatDate(timestamp: number) {
  return new Date(timestamp * 1000).toLocaleDateString();
}
