import { memo, useEffect, useMemo, useRef, useState, type CSSProperties } from "react";
import { Smile } from "../icons";
import { readAppSettings, subscribeAppSettings } from "../settings";

export type PetAnimationState =
  | "idle"
  | "running-right"
  | "running-left"
  | "waving"
  | "jumping"
  | "failed"
  | "waiting"
  | "running"
  | "review";

type Props = {
  species?: string;
  src?: string | null;
  state?: PetAnimationState;
  size?: number;
  staticMode?: boolean;
  className?: string;
};

const atlas = {
  columns: 8,
  rows: 9,
  cellWidth: 192,
  cellHeight: 208,
};

const animations: Record<PetAnimationState, { row: number; frameDurationsMs: number[] }> = {
  idle: { row: 0, frameDurationsMs: [280, 110, 110, 140, 140, 320] },
  "running-right": { row: 1, frameDurationsMs: [120, 120, 120, 120, 120, 120, 120, 220] },
  "running-left": { row: 2, frameDurationsMs: [120, 120, 120, 120, 120, 120, 120, 220] },
  waving: { row: 3, frameDurationsMs: [140, 140, 140, 280] },
  jumping: { row: 4, frameDurationsMs: [140, 140, 140, 140, 280] },
  failed: { row: 5, frameDurationsMs: [140, 140, 140, 140, 140, 140, 140, 240] },
  waiting: { row: 6, frameDurationsMs: [150, 150, 150, 150, 150, 260] },
  running: { row: 7, frameDurationsMs: [120, 120, 120, 120, 120, 220] },
  review: { row: 8, frameDurationsMs: [150, 150, 150, 150, 150, 280] },
};

const petSpriteUrls = import.meta.glob("../assets/pets/*/spritesheet.png", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

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
  const spriteUrl = src || petSpriteUrls[`../assets/pets/${normalizedSpecies}/spritesheet.png`];
  const resolvedStaticMode = staticMode ?? settings?.pet.staticMode ?? false;
  const frameCount = frameDurations.length;
  const scale = size / atlas.cellHeight;
  const visibleWidth = atlas.cellWidth * scale;

  useEffect(() => {
    if (staticMode !== undefined) return;
    setSettings(readAppSettings());
    return subscribeAppSettings(setSettings);
  }, [staticMode]);

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
  const leadingHold = state === "idle" || state === "waiting" || state === "review" ? 1.85 : 1.35;
  return Math.max(80, Math.round(delayMs * leadingHold));
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
