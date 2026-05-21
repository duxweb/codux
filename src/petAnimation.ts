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

export type PetAnimationSpec = {
  row: number;
  frameDurationsMs: number[];
};

export const petAtlas = {
  columns: 8,
  rows: 9,
  cellWidth: 192,
  cellHeight: 208,
};

export const petAnimations: Record<PetAnimationState, PetAnimationSpec> = {
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

const contentAlphaThreshold = 3;
const scanStep = 4;
const activeFrameCountCache = new Map<string, Promise<number[]>>();

export function petFrameDelay(delayMs: number, state: PetAnimationState) {
  const leadingHold = state === "idle" || state === "waiting" || state === "review" ? 1.85 : 1.35;
  return Math.max(80, Math.round(delayMs * leadingHold));
}

export function activePetFrameCount(counts: number[] | null | undefined, animation: PetAnimationSpec) {
  const detected = counts?.[animation.row];
  const fallback = animation.frameDurationsMs.length;
  return Math.max(1, Math.min(fallback, detected && detected > 0 ? detected : fallback));
}

export function loadPetActiveFrameCounts(cacheKey: string, source: string) {
  const cached = activeFrameCountCache.get(cacheKey);
  if (cached) return cached;
  const promise = detectPetActiveFrameCounts(source).catch(() =>
    Array.from({ length: petAtlas.rows }, () => petAtlas.columns),
  );
  activeFrameCountCache.set(cacheKey, promise);
  return promise;
}

async function detectPetActiveFrameCounts(source: string) {
  if (typeof Image === "undefined" || typeof document === "undefined") {
    return Array.from({ length: petAtlas.rows }, () => petAtlas.columns);
  }
  const image = await loadImage(source);
  const canvas = document.createElement("canvas");
  canvas.width = image.naturalWidth || image.width;
  canvas.height = image.naturalHeight || image.height;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context || canvas.width < petAtlas.cellWidth || canvas.height < petAtlas.cellHeight) {
    return Array.from({ length: petAtlas.rows }, () => petAtlas.columns);
  }
  context.drawImage(image, 0, 0);
  const rows = Math.min(petAtlas.rows, Math.floor(canvas.height / petAtlas.cellHeight));
  return Array.from({ length: petAtlas.rows }, (_, row) =>
    row < rows ? activeFrameCountForRow(context, row, canvas.width, canvas.height) : petAtlas.columns,
  );
}

function loadImage(source: string) {
  return new Promise<HTMLImageElement>((resolve, reject) => {
    const image = new Image();
    image.decoding = "async";
    image.onload = () => resolve(image);
    image.onerror = () => reject(new Error("Failed to load pet spritesheet."));
    image.src = source;
  });
}

function activeFrameCountForRow(
  context: CanvasRenderingContext2D,
  row: number,
  width: number,
  height: number,
) {
  let count = 0;
  for (let column = 0; column < petAtlas.columns; column += 1) {
    if (cellHasContent(context, row, column, width, height)) {
      count = column + 1;
    }
  }
  return count;
}

function cellHasContent(
  context: CanvasRenderingContext2D,
  row: number,
  column: number,
  width: number,
  height: number,
) {
  const startX = column * petAtlas.cellWidth;
  const startY = row * petAtlas.cellHeight;
  const endX = Math.min(startX + petAtlas.cellWidth, width);
  const endY = Math.min(startY + petAtlas.cellHeight, height);
  for (let y = startY; y < endY; y += scanStep) {
    for (let x = startX; x < endX; x += scanStep) {
      const alpha = context.getImageData(x, y, 1, 1).data[3] ?? 0;
      if (alpha > contentAlphaThreshold) return true;
    }
  }
  return false;
}
