import { CheckCircle2, Plus, Sparkles } from "../icons";
import { useEffect, useMemo, useState } from "react";
import { listenPetCustomPetInstalled } from "../ai/petCustomEvents";
import { defaultPetCatalog, loadPetCatalog, usePetLedger, type PetCatalog, type PetCatalogItem, type PetCustomPet } from "../ai/petState";
import { Button } from "../components/Button";
import { TextInput } from "../components/Form";
import { PetSprite } from "../components/PetSprite";
import { PressableButton } from "../components/PressableButton";
import { readAppSettings, subscribeAppSettings } from "../settings";
import { systemMessage } from "../systemDialog";
import { closeCurrentAppWindow, openAppWindow } from "../windowing";
import { tm } from "../i18n";
import { WindowFrame } from "./WindowFrame";

const petAccentColors: Record<string, string> = {
  voidcat: "#2A80FF",
  rusthound: "#FF6030",
  goose: "#D2A04F",
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

const petThumbUrls = import.meta.glob("../assets/pets/*/thumb.png", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

type PetSelection =
  | { kind: "bundled"; id: string; item: PetCatalogItem }
  | { kind: "random"; id: "bundled:random" }
  | { kind: "custom"; id: string; pet: PetCustomPet };

export function PetClaimWindow() {
  const pet = usePetLedger([]);
  const [settings, setSettings] = useState(readAppSettings);
  const [catalog, setCatalog] = useState<PetCatalog>(() => defaultPetCatalog());
  const [selectedId, setSelectedId] = useState("bundled:voidcat");
  const [customName, setCustomName] = useState("");
  const [isSubmitting, setSubmitting] = useState(false);

  useEffect(() => subscribeAppSettings(setSettings), []);
  useEffect(() => {
    let timeoutId: number | undefined;
    const frameId = window.requestAnimationFrame(() => {
      timeoutId = window.setTimeout(() => void reloadCatalog(), 0);
    });
    return () => {
      window.cancelAnimationFrame(frameId);
      if (timeoutId !== undefined) window.clearTimeout(timeoutId);
    };
  }, []);

  useEffect(() => {
    let isDisposed = false;
    let unlisten: (() => void) | undefined;
    void listenPetCustomPetInstalled(async (installed) => {
      await reloadCatalog();
      if (!isDisposed) setSelectedId(`custom:${installed.id}`);
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

  useEffect(() => {
    if (pet.snapshot.claimedAt) {
      void closeCurrentAppWindow();
    }
  }, [pet.snapshot.claimedAt]);

  const reloadCatalog = async () => {
    const next = await loadPetCatalog();
    setCatalog(next);
    setSelectedId((current) => current || `bundled:${next.species[0]?.species || "voidcat"}`);
  };

  const selections = useMemo<PetSelection[]>(() => {
    const bundled = catalog.species.map((item) => ({
      kind: "bundled" as const,
      id: `bundled:${item.species}`,
      item,
    }));
    const random: PetSelection = { kind: "random", id: "bundled:random" };
    const custom = catalog.customPets.map((item) => ({
      kind: "custom" as const,
      id: `custom:${item.id}`,
      pet: item,
    }));
    return [...bundled, random, ...custom];
  }, [catalog]);

  const selected = selections.find((item) => item.id === selectedId) ?? selections[0] ?? fallbackSelection();
  const accent = selected.kind === "bundled"
    ? (petAccentColors[selected.item.species] ?? "#2A80FF")
    : selected.kind === "random"
      ? "#8B5CF6"
      : "#2A80FF";
  const selectedTitle = selected.kind === "bundled"
    ? tm(selected.item.nameKey, selected.item.species)
    : selected.kind === "random"
      ? tm("pet.claim.random.title", "Random")
      : selected.pet.displayName;
  const selectedSubtitle = selected.kind === "bundled"
    ? tm(selected.item.subtitleKey, "")
    : selected.kind === "random"
      ? tm("pet.claim.random.subtitle", "Let Codux choose a companion")
      : tm("pet.claim.custom.subtitle", "Installed custom pet");
  const selectedDescription = selected.kind === "bundled"
    ? (tm(selected.item.descriptionKey, "") || selectedSubtitle)
    : selected.kind === "random"
      ? tm("pet.claim.random.description", "Let Codux choose one companion for you.")
      : (selected.pet.description || tm("pet.claim.custom.description", "A custom Codex-format companion installed from Petdex."));

  const confirm = async () => {
    if (isSubmitting) return;
    setSubmitting(true);
    try {
      if (selected.kind === "custom") {
        await pet.claim(`custom:${selected.pet.id}`, customName, selected.pet);
      } else if (selected.kind === "random") {
        await pet.claim(randomSpecies(catalog), customName);
      } else {
        await pet.claim(selected.item.species, customName);
      }
      await closeCurrentAppWindow();
    } catch (error) {
      await systemMessage(error instanceof Error ? error.message : String(error), {
        title: tm("settings.pet.action_failed", "Pet Action Failed"),
        kind: "warning",
        buttons: { ok: "OK" },
      });
      setSubmitting(false);
    }
  };

  return (
    <WindowFrame
      title={tm("pet.claim.dialog.title", "Choose a Pet")}
      mainClassName="px-0 py-0"
      mainScrollable={false}
      footer={
        <>
          <Button variant="secondary" leading={Plus} onPress={() => void openAppWindow("pet-custom-install")}>
            {tm("pet.custom.install.action", "Add Custom Pet")}
          </Button>
          <div className="flex-1" />
          <Button variant="ghost" onPressUp={() => void closeCurrentAppWindow()}>
            {tm("common.cancel", "Cancel")}
          </Button>
          <Button variant="primary" disabled={isSubmitting || selections.length === 0} onPress={() => void confirm()}>
            {isSubmitting ? tm("common.processing", "Processing") : tm("pet.claim.confirm", "Confirm Claim")}
          </Button>
        </>
      }
    >
      <div className="grid h-full min-h-0 grid-cols-[220px_minmax(0,1fr)] overflow-hidden">
        <div className="min-h-0 overflow-y-auto scrollbar-overlay border-r border-line/60 p-3.5">
          <div className="grid gap-2">
            {catalog.species.map((item) => (
              <PetOptionRow
                key={item.species}
                selection={{ kind: "bundled", id: `bundled:${item.species}`, item }}
                selected={selectedId === `bundled:${item.species}`}
                onSelect={() => setSelectedId(`bundled:${item.species}`)}
              />
            ))}
            <PetOptionRow
              selection={{ kind: "random", id: "bundled:random" }}
              selected={selectedId === "bundled:random"}
              onSelect={() => setSelectedId("bundled:random")}
            />
            {catalog.customPets.length > 0 && (
              <div className="px-1 pt-2 text-xs font-semibold text-ink-faint">
                {tm("pet.claim.custom.section", "Custom Pets")}
              </div>
            )}
            {catalog.customPets.map((item) => (
              <PetOptionRow
                key={item.id}
                selection={{ kind: "custom", id: `custom:${item.id}`, pet: item }}
                selected={selectedId === `custom:${item.id}`}
                onSelect={() => setSelectedId(`custom:${item.id}`)}
              />
            ))}
          </div>
        </div>

        <section className="flex min-h-0 flex-col overflow-hidden p-4 text-center">
          <div className="flex min-h-0 flex-1 flex-col items-center justify-center">
            <div className="grid flex-shrink-0 place-items-center">
              <div
                className="grid h-[100px] w-[100px] place-items-center rounded-full"
                style={{ backgroundColor: `${accent}14` }}
              >
                <PetSelectionSprite selection={selected} size={84} staticMode={settings.pet.staticMode} />
              </div>
            </div>

            <div className="mt-3 flex-shrink-0 text-[15px] font-bold">{selectedTitle}</div>
            <p className="mx-auto mt-2 max-h-[72px] min-h-[42px] max-w-[330px] flex-shrink-0 overflow-y-auto scrollbar-overlay text-xs leading-5 text-ink-mute">
              {selectedDescription}
            </p>
          </div>

          <div className="flex-shrink-0 pt-3 text-left">
            <TextInput
              className="block w-full"
              value={customName}
              onChange={(event) => setCustomName(event.currentTarget.value)}
              placeholder={tm("pet.claim.name.placeholder", "Leave empty to use the species name")}
              autoFocus
            />
          </div>
        </section>
      </div>
    </WindowFrame>
  );
}

function PetOptionRow({
  selection,
  selected,
  onSelect,
}: {
  selection: PetSelection;
  selected: boolean;
  onSelect: () => void;
}) {
  const title = selection.kind === "bundled"
    ? tm(selection.item.nameKey, selection.item.species)
    : selection.kind === "random"
      ? tm("pet.claim.random.title", "Random")
      : selection.pet.displayName;
  const subtitle = selection.kind === "bundled"
    ? tm(selection.item.subtitleKey, "")
    : selection.kind === "random"
      ? tm("pet.claim.random.subtitle", "Let Codux choose a companion")
      : tm("pet.claim.custom.subtitle", "Installed custom pet");
  return (
    <PressableButton
      onPressUp={onSelect}
      className={`grid grid-cols-[44px_minmax(0,1fr)_18px] items-center gap-2.5 rounded-[10px] border px-2.5 py-[7px] text-left transition-colors ${
        selected
          ? "border-brand-blue/60 bg-brand-blue/10"
          : "border-transparent bg-fill/[0.035] hover:bg-fill/[0.07]"
      }`}
    >
      <PetOptionSprite selection={selection} />
      <div className="min-w-0">
        <div className="truncate text-[13px] font-semibold text-ink">{title}</div>
        <div className="mt-0.5 truncate text-xs text-ink-mute">{subtitle}</div>
      </div>
      {selected ? <CheckCircle2 size={15} className="text-brand-blue" /> : <span />}
    </PressableButton>
  );
}

function PetSelectionSprite({
  selection,
  size,
  staticMode,
}: {
  selection: PetSelection;
  size: number;
  staticMode?: boolean;
}) {
  if (selection.kind === "custom") {
    return (
      <PetSprite
        species="voidcat"
        src={selection.pet.spritesheetDataUrl}
        size={size}
        staticMode={staticMode}
      />
    );
  }
  if (selection.kind === "random") {
    return (
      <span
        className="grid place-items-center rounded-full bg-brand-blue/12 text-brand-blue"
        style={{ width: size, height: size }}
      >
        <Sparkles size={Math.max(18, Math.round(size * 0.5))} strokeWidth={2.2} />
      </span>
    );
  }
  return <PetSprite species={selection.item.species} size={size} staticMode={staticMode} />;
}

function PetOptionSprite({ selection }: { selection: PetSelection }) {
  if (selection.kind === "random") {
    return (
      <span className="grid h-11 w-11 place-items-center rounded-full bg-brand-blue/12 text-brand-blue">
        <Sparkles size={20} strokeWidth={2.2} />
      </span>
    );
  }

  if (selection.kind === "custom") {
    return <PetSelectionSprite selection={selection} size={44} staticMode />;
  }

  const src = petThumbUrls[`../assets/pets/${selection.item.species}/thumb.png`];
  if (!src) {
    return <PetSelectionSprite selection={selection} size={44} staticMode />;
  }

  return (
    <span className="grid h-11 w-11 place-items-center overflow-hidden rounded-[8px] bg-fill/[0.035]">
      <img
        src={src}
        alt=""
        className="h-full w-full object-contain"
        draggable={false}
      />
    </span>
  );
}

function randomSpecies(catalog: PetCatalog) {
  const pool = catalog.species.map((item) => item.species).filter(Boolean);
  if (pool.length === 0) return "voidcat";
  return pool[Math.floor(Math.random() * pool.length)] ?? "voidcat";
}

function fallbackSelection(): PetSelection {
  return {
    kind: "bundled",
    id: "bundled:voidcat",
    item: {
      species: "voidcat",
      assetFolder: "voidcat",
      manifestId: "voidcat-default",
      nameKey: "pet.species.voidcat.base",
      claimTitleKey: "pet.claim.voidcat.title",
      subtitleKey: "pet.claim.voidcat.subtitle",
      descriptionKey: "pet.claim.voidcat.description",
    },
  };
}
