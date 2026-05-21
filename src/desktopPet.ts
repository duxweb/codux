import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import voidcatUrl from "./assets/pets/voidcat/spritesheet.png?url";
import dolphinUrl from "./assets/pets/dolphin/spritesheet.png?url";
import rusthoundUrl from "./assets/pets/rusthound/spritesheet.png?url";
import penguinUrl from "./assets/pets/penguin/spritesheet.png?url";
import sheepUrl from "./assets/pets/sheep/spritesheet.png?url";
import pandaUrl from "./assets/pets/panda/spritesheet.png?url";
import phoenixUrl from "./assets/pets/phoenix/spritesheet.png?url";
import oxUrl from "./assets/pets/ox/spritesheet.png?url";
import codeUrl from "./assets/pets/code/spritesheet.png?url";
import dragonUrl from "./assets/pets/dragon/spritesheet.png?url";
import chaosspriteUrl from "./assets/pets/chaossprite/spritesheet.png?url";
import gooseUrl from "./assets/pets/goose/spritesheet.png?url";
import {
  desktopPetActivityLine,
  desktopPetAnimationState,
  nextDesktopPetActivityRefreshMs,
  type AISessionSnapshot,
  type DesktopPetAnimationState,
  type DesktopPetActivityTone,
} from "./desktopPetActivity";
import { lockRuntimeLocale, syncI18nBundleFromRust, tm } from "./i18n";
import { syncAppSettingsFromRust } from "./settings";
import "./desktopPet.css";

type DesktopPetSide = "left" | "right";

type AppSettings = {
  pet: {
    staticMode: boolean;
    enabled: boolean;
    desktopWidget: boolean;
  };
};

type PetSnapshot = {
  claimedAt?: number | null;
  species: string;
  customPet?: {
    displayName?: string | null;
    spritesheetDataUrl?: string | null;
  } | null;
  customName?: string | null;
  dailyExperienceTokens: number;
};

type AIRuntimeStateSnapshot = {
  sessions: AISessionSnapshot[];
};

type PlacementSnapshot = {
  side: DesktopPetSide;
};

const atlas = { columns: 8, rows: 9, cellWidth: 192, cellHeight: 208 };
const spriteSize = 128;
const visibleWidth = (atlas.cellWidth * spriteSize) / atlas.cellHeight;
const spriteUrls: Record<string, string> = {
  voidcat: voidcatUrl,
  dolphin: dolphinUrl,
  rusthound: rusthoundUrl,
  penguin: penguinUrl,
  sheep: sheepUrl,
  panda: pandaUrl,
  phoenix: phoenixUrl,
  ox: oxUrl,
  code: codeUrl,
  dragon: dragonUrl,
  chaossprite: chaosspriteUrl,
  goose: gooseUrl,
};
const animations: Record<DesktopPetAnimationState, { row: number; frameDurationsMs: number[] }> = {
  idle: { row: 0, frameDurationsMs: [280, 110, 110, 140, 140, 320] },
  waving: { row: 3, frameDurationsMs: [140, 140, 140, 280] },
  failed: { row: 5, frameDurationsMs: [140, 140, 140, 140, 140, 140, 140, 240] },
  waiting: { row: 6, frameDurationsMs: [150, 150, 150, 150, 150, 260] },
  running: { row: 7, frameDurationsMs: [120, 120, 120, 120, 120, 220] },
  review: { row: 8, frameDurationsMs: [150, 150, 150, 150, 150, 280] },
};

const root = document.getElementById("pet-root") as HTMLDivElement;
const spriteHotspot = document.getElementById("sprite-hotspot") as HTMLDivElement;
const sprite = document.getElementById("pet-sprite") as HTMLDivElement;
const bubble = document.getElementById("speech-bubble") as HTMLDivElement;
const bubbleText = document.getElementById("speech-text") as HTMLDivElement;
const appWindow = getCurrentWindow();

let settings: AppSettings | null = null;
let pet: PetSnapshot | null = null;
let runtime: AIRuntimeStateSnapshot = { sessions: [] };
let side: DesktopPetSide = "left";
let frameTimer: number | null = null;
let activityTimer: number | null = null;
let currentFrame = 0;
let currentState: DesktopPetAnimationState = "idle";
let lastBubbleText = "";
let lastBubbleVisible = false;
let lastBubbleTone: DesktopPetActivityTone = "normal";

void boot();

async function boot() {
  installWindowEvents();
  await Promise.all([loadSettings(), loadPet(), loadRuntime(), syncI18nBundleFromRust()]);
  await loadPlacement();
  renderAll();
  if (shouldDisplayPet()) {
    await appWindow.show().catch(() => undefined);
  }
}

function installWindowEvents() {
  spriteHotspot.addEventListener("pointerdown", (event) => {
    if (event.button !== 0) return;
    event.preventDefault();
    event.stopPropagation();
    void invoke("desktop_pet_start_drag").catch(() => undefined);
  });
  spriteHotspot.addEventListener("contextmenu", openContextMenu);
  bubble.addEventListener("contextmenu", openContextMenu);

  void listen<AppSettings>("settings:updated", (event) => {
    settings = event.payload;
    renderAll();
  });
  void listen<PetSnapshot>("pet:updated", (event) => {
    pet = event.payload;
    renderAll();
  });
  void listen<AIRuntimeStateSnapshot>("ai-runtime:state", (event) => {
    runtime = event.payload;
    renderAll();
  });
  void appWindow.listen<PlacementSnapshot>("desktop-pet:placement", (event) => {
    side = event.payload.side === "right" ? "right" : "left";
    applySide();
  });
  void appWindow.listen("desktop-pet:skip-line", () => setBubbleLine(""));
}

function openContextMenu(event: MouseEvent) {
  event.preventDefault();
  event.stopPropagation();
  void invoke("desktop_pet_show_context_menu").catch(() => undefined);
}

async function loadSettings() {
  settings = await syncAppSettingsFromRust()
    .then((next) => {
      lockRuntimeLocale(next);
      return next;
    })
    .catch(() => invoke<AppSettings>("app_settings_get").catch(() => null));
}

async function loadPet() {
  pet = await invoke<PetSnapshot>("pet_snapshot").catch(() => null);
}

async function loadRuntime() {
  runtime = await invoke<AIRuntimeStateSnapshot>("ai_runtime_state_snapshot").catch(() => ({ sessions: [] }));
}

async function loadPlacement() {
  const placement = await invoke<PlacementSnapshot>("desktop_pet_placement").catch(() => null);
  side = placement?.side === "right" ? "right" : "left";
}

function renderAll() {
  if (!shouldDisplayPet()) {
    stopFrameTimer();
    if (activityTimer != null) {
      window.clearTimeout(activityTimer);
      activityTimer = null;
    }
    void appWindow.hide().catch(() => undefined);
    return;
  }
  applySide();
  updateSpriteSource();
  updateSpriteAnimation();
  updateActivityLine();
}

function shouldDisplayPet() {
  return Boolean(settings?.pet.enabled && settings.pet.desktopWidget && pet?.claimedAt);
}

function applySide() {
  root.dataset.side = side;
  bubble.classList.toggle("speech-bubble--left-tail", side === "right");
  bubble.classList.toggle("speech-bubble--right-tail", side !== "right");
}

function updateSpriteSource() {
  const source = pet?.customPet?.spritesheetDataUrl || spriteUrls[pet?.species || ""] || voidcatUrl;
  sprite.style.width = `${visibleWidth}px`;
  sprite.style.height = `${spriteSize}px`;
  sprite.style.backgroundImage = `url("${source}")`;
  sprite.style.backgroundSize = `${atlas.columns * visibleWidth}px ${atlas.rows * spriteSize}px`;
  spriteHotspot.style.width = `${spriteSize}px`;
  spriteHotspot.style.height = `${spriteSize}px`;
}

function updateSpriteAnimation() {
  const nextState = desktopPetAnimationState({
    claimed: Boolean(pet?.claimedAt),
    dailyExperienceTokens: pet?.dailyExperienceTokens ?? 0,
    sessions: runtime.sessions,
    now: Date.now() / 1000,
  });
  if (nextState === currentState && frameTimer != null) return;
  currentState = nextState;
  currentFrame = 0;
  stopFrameTimer();
  applyFrame();
  if (settings?.pet.staticMode) return;
  scheduleFrame();
}

function scheduleFrame() {
  const animation = animations[currentState] ?? animations.idle;
  const delay = frameDelay(
    animation.frameDurationsMs[currentFrame % animation.frameDurationsMs.length] ?? 180,
    currentState,
  );
  frameTimer = window.setTimeout(() => {
    currentFrame = (currentFrame + 1) % animation.frameDurationsMs.length;
    applyFrame();
    scheduleFrame();
  }, delay);
}

function stopFrameTimer() {
  if (frameTimer != null) {
    window.clearTimeout(frameTimer);
    frameTimer = null;
  }
}

function applyFrame() {
  const animation = animations[currentState] ?? animations.idle;
  sprite.style.backgroundPosition = `-${currentFrame * visibleWidth}px -${animation.row * spriteSize}px`;
}

function updateActivityLine() {
  const now = Date.now() / 1000;
  const line = desktopPetActivityLine(runtime.sessions, now, tm);
  setBubbleLine(line.text, line.tone);
  if (activityTimer != null) window.clearTimeout(activityTimer);
  const refreshMs = nextDesktopPetActivityRefreshMs(runtime.sessions, now);
  activityTimer = refreshMs == null ? null : window.setTimeout(updateActivityLine, refreshMs);
}

function setBubbleLine(text: string, tone: DesktopPetActivityTone = "normal") {
  const nextText = text.trim();
  const nextTone = nextText ? tone : "normal";
  const isVisible = nextText.length > 0;
  if (nextText === lastBubbleText && isVisible === lastBubbleVisible && nextTone === lastBubbleTone) return;
  lastBubbleText = nextText;
  lastBubbleVisible = isVisible;
  lastBubbleTone = nextTone;
  bubble.hidden = !isVisible;
  bubble.style.display = isVisible ? "grid" : "none";
  bubble.dataset.tone = nextTone;
  bubbleText.textContent = nextText;
  void invoke("desktop_pet_set_bubble_visible", { visible: isVisible }).catch(() => undefined);
}

function frameDelay(delayMs: number, state: DesktopPetAnimationState) {
  const leadingHold = state === "idle" || state === "waiting" || state === "review" ? 1.85 : 1.35;
  return Math.max(80, Math.round(delayMs * leadingHold));
}
