import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import {
  desktopPetActivityLine,
  desktopPetAnimationState,
  nextDesktopPetActivityRefreshMs,
  type AISessionSnapshot,
  type DesktopPetAnimationState,
  type DesktopPetActivityTone,
} from "./desktopPetActivity";
import { lockRuntimeLocale, syncI18nBundleFromRust, tm } from "./i18n";
import {
  activePetFrameCount,
  loadPetActiveFrameCounts,
  petAnimations,
  petAtlas,
  petFrameDelay,
} from "./petAnimation";
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

const spriteSize = 128;
const visibleWidth = (petAtlas.cellWidth * spriteSize) / petAtlas.cellHeight;
const spriteLoaders = import.meta.glob("./assets/pets/*/spritesheet.png", {
  query: "?url",
  import: "default",
}) as Record<string, () => Promise<string>>;
const spriteUrlCache = new Map<string, string>();
const animations = petAnimations;

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
let currentSpriteKey = "";
let currentSpriteUrl = "";
let currentActiveFrameCounts: number[] | null = null;
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
  sprite.style.width = `${visibleWidth}px`;
  sprite.style.height = `${spriteSize}px`;
  sprite.style.backgroundSize = `${petAtlas.columns * visibleWidth}px ${petAtlas.rows * spriteSize}px`;
  spriteHotspot.style.width = `${spriteSize}px`;
  spriteHotspot.style.height = `${spriteSize}px`;

  const customSource = pet?.customPet?.spritesheetDataUrl;
  if (customSource) {
    applySpriteSource("custom", customSource);
    return;
  }

  const species = pet?.species || "voidcat";
  const key = `./assets/pets/${species}/spritesheet.png`;
  const fallbackKey = "./assets/pets/voidcat/spritesheet.png";
  const cached = spriteUrlCache.get(key) || spriteUrlCache.get(fallbackKey);
  if (cached) {
    applySpriteSource(key, cached);
    return;
  }

  const loader = spriteLoaders[key] ?? spriteLoaders[fallbackKey];
  if (!loader) return;
  const requestKey = key;
  void loader()
    .then((url) => {
      spriteUrlCache.set(requestKey, url);
      if ((pet?.species || "voidcat") === species) {
        applySpriteSource(requestKey, url);
      }
    })
    .catch(() => undefined);
}

function applySpriteSource(key: string, source: string) {
  if (currentSpriteKey === key && currentSpriteUrl === source) return;
  currentSpriteKey = key;
  currentSpriteUrl = source;
  currentActiveFrameCounts = null;
  sprite.style.backgroundImage = `url("${source}")`;
  void loadPetActiveFrameCounts(key, source)
    .then((counts) => {
      if (currentSpriteKey !== key || currentSpriteUrl !== source) return;
      currentActiveFrameCounts = counts;
      currentFrame = Math.min(currentFrame, currentFrameCount() - 1);
      applyFrame();
    })
    .catch(() => undefined);
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
  const frameCount = currentFrameCount();
  const delay = frameDelay(
    animation.frameDurationsMs[currentFrame % frameCount] ?? 180,
    currentState,
  );
  frameTimer = window.setTimeout(() => {
    currentFrame = (currentFrame + 1) % frameCount;
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

function currentFrameCount() {
  const animation = animations[currentState] ?? animations.idle;
  return activePetFrameCount(currentActiveFrameCounts, animation);
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
  return petFrameDelay(delayMs, state);
}
