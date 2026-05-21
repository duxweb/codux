import { memo, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Smile } from "../icons";
import {
  activePetFrameCount,
  loadPetActiveFrameCounts,
  petAnimations,
  petAtlas,
  petFrameDelay,
  type PetAnimationState,
} from "../petAnimation";
import { readAppSettings, subscribeAppSettings } from "../settings";

type Props = {
  species?: string;
  src?: string | null;
  state?: PetAnimationState;
  size?: number;
  staticMode?: boolean;
  className?: string;
};

const atlas = petAtlas;
const animations = petAnimations;

const petSpriteLoaders = import.meta.glob("../assets/pets/*/spritesheet.png", {
  query: "?url",
  import: "default",
}) as Record<string, () => Promise<string>>;

const loadedPetSpriteUrls = new Map<string, string>();

const speciesFallbacks = new Set([
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
]);

export const PetSprite = memo(function PetSprite({
  species = "voidcat",
  src,
  state = "idle",
  size = 96,
  staticMode,
  className,
}: Props) {
  const spriteRef = useRef<HTMLDivElement | null>(null);
  const [settings, setSettings] = useState(() => (staticMode === undefined ? readAppSettings() : null));
  const animation = animations[state] ?? animations.idle;
  const frameDurations = animation.frameDurationsMs;
  const normalizedSpecies = speciesFallbacks.has(species) ? species : "voidcat";
  const spriteKey = `../assets/pets/${normalizedSpecies}/spritesheet.png`;
  const [loadedSpriteUrl, setLoadedSpriteUrl] = useState(() => loadedPetSpriteUrls.get(spriteKey) ?? "");
  const spriteUrl = src || loadedSpriteUrl;
  const spriteFrameKey = src ? `custom:${src}` : spriteKey;
  const [activeFrameCounts, setActiveFrameCounts] = useState<number[] | null>(null);
  const resolvedStaticMode = staticMode ?? settings?.pet.staticMode ?? false;
  const frameCount = activePetFrameCount(activeFrameCounts, animation);
  const scale = size / atlas.cellHeight;
  const visibleWidth = atlas.cellWidth * scale;

  useEffect(() => {
    if (staticMode !== undefined) return;
    setSettings(readAppSettings());
    return subscribeAppSettings(setSettings);
  }, [staticMode]);

  useEffect(() => {
    if (src) {
      setLoadedSpriteUrl("");
      return;
    }
    const cached = loadedPetSpriteUrls.get(spriteKey);
    if (cached) {
      setLoadedSpriteUrl(cached);
      return;
    }
    let cancelled = false;
    const loader = petSpriteLoaders[spriteKey] ?? petSpriteLoaders["../assets/pets/voidcat/spritesheet.png"];
    if (!loader) {
      setLoadedSpriteUrl("");
      return;
    }
    setLoadedSpriteUrl("");
    void loader()
      .then((url) => {
        loadedPetSpriteUrls.set(spriteKey, url);
        if (!cancelled) setLoadedSpriteUrl(url);
      })
      .catch(() => {
        if (!cancelled) setLoadedSpriteUrl("");
      });
    return () => {
      cancelled = true;
    };
  }, [spriteKey, src]);

  useEffect(() => {
    if (!spriteUrl) {
      setActiveFrameCounts(null);
      return;
    }
    let cancelled = false;
    setActiveFrameCounts(null);
    void loadPetActiveFrameCounts(spriteFrameKey, spriteUrl).then((counts) => {
      if (!cancelled) setActiveFrameCounts(counts);
    });
    return () => {
      cancelled = true;
    };
  }, [spriteFrameKey, spriteUrl]);

  useEffect(() => {
    const sprite = spriteRef.current;
    let currentFrame = 0;
    applySpriteFrame(sprite, currentFrame, visibleWidth, animation.row, size);
    if (resolvedStaticMode || frameCount <= 1) return;
    let cancelled = false;
    let timer: number | null = null;
    const tick = () => {
      const delay = frameDelay(frameDurations[currentFrame % frameCount] ?? 180, state);
      timer = window.setTimeout(() => {
        if (cancelled) return;
        currentFrame = (currentFrame + 1) % frameCount;
        applySpriteFrame(sprite, currentFrame, visibleWidth, animation.row, size);
        tick();
      }, delay);
    };
    tick();
    return () => {
      cancelled = true;
      if (timer !== null) window.clearTimeout(timer);
    };
  }, [animation.row, frameCount, frameDurations, resolvedStaticMode, size, state, visibleWidth]);

  const style = useMemo<CSSProperties | undefined>(() => {
    if (!spriteUrl) return undefined;
    return {
      width: `${visibleWidth}px`,
      height: `${size}px`,
      backgroundImage: `url("${spriteUrl}")`,
      backgroundSize: `${atlas.columns * visibleWidth}px ${atlas.rows * size}px`,
      backgroundPosition: `0px -${animation.row * size}px`,
    };
  }, [animation.row, size, spriteUrl, visibleWidth]);

  if (!spriteUrl) {
    return (
      <div
        className={`grid place-items-center rounded-lg bg-brand-blue/14 text-brand-blue ${className ?? ""}`}
        style={{ width: size, height: size }}
      >
        <Smile size={Math.round(size * 0.42)} />
      </div>
    );
  }

  return (
    <div className={`overflow-hidden ${className ?? ""}`} style={{ width: size, height: size }}>
      <div ref={spriteRef} aria-hidden="true" className="bg-no-repeat [image-rendering:auto]" style={style} />
    </div>
  );
});

function frameDelay(delayMs: number, state: PetAnimationState) {
  return petFrameDelay(delayMs, state);
}

function applySpriteFrame(
  sprite: HTMLDivElement | null,
  frame: number,
  visibleWidth: number,
  row: number,
  size: number,
) {
  if (!sprite) return;
  sprite.style.backgroundPosition = `-${frame * visibleWidth}px -${row * size}px`;
}
