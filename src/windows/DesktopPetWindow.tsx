import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { useEffect, useMemo, useState, type MouseEvent, type PointerEvent } from "react";
import { aiRuntime } from "../ai/runtime";
import type { AISessionSnapshot } from "../ai/types";
import { usePetLedger } from "../ai/petState";
import { PetSprite, type PetAnimationState } from "../components/PetSprite";
import { readAppSettings, subscribeAppSettings, syncAppSettingsFromRust } from "../settings";
import { destroyCurrentAppWindow, revealCurrentAppWindow } from "../windowing";
import { formatI18n, tm } from "../i18n";

type PetIdleSpeechResponse = {
  text: string;
};

type DesktopPetPlacementSnapshot = {
  side: DesktopPetSide;
};

type DesktopPetSide = "left" | "right";

const DESKTOP_PET_COMPLETED_STATUS_SECONDS = 8;

export function DesktopPetWindow() {
  const pet = usePetLedger([]);
  const [settings, setSettings] = useState(readAppSettings);
  const [isSettingsHydrated, setSettingsHydrated] = useState(!window.__TAURI_INTERNALS__);
  const [line, setLine] = useState("");
  const [activityLine, setActivityLine] = useState("");
  const [aiVersion, setAIVersion] = useState(0);
  const [side, setSide] = useState<DesktopPetSide>("left");

  useEffect(() => {
    void syncAppSettingsFromRust().then((next) => {
      setSettings(next);
      setSettingsHydrated(true);
    });
    return subscribeAppSettings((next) => {
      setSettings(next);
      setSettingsHydrated(true);
    });
  }, []);
  useEffect(() => {
    void revealCurrentAppWindow();
  }, []);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    const appWindow = getCurrentWindow();
    let disposed = false;
    void invoke<DesktopPetPlacementSnapshot>("desktop_pet_placement")
      .then((snapshot) => {
        if (!disposed) setSide(normalizeDesktopPetSide(snapshot.side));
      })
      .catch((error) => console.error("failed to load desktop pet placement", error));
    let unlisten: (() => void) | undefined;
    void appWindow.listen<DesktopPetPlacementSnapshot>("desktop-pet:placement", (event) => {
      setSide(normalizeDesktopPetSide(event.payload.side));
    }).then((nextUnlisten) => {
      if (disposed) nextUnlisten();
      else unlisten = nextUnlisten;
    });
    let unlistenSkip: (() => void) | undefined;
    void appWindow.listen("desktop-pet:skip-line", () => {
      setLine("");
    }).then((nextUnlisten) => {
      if (disposed) nextUnlisten();
      else unlistenSkip = nextUnlisten;
    });
    return () => {
      disposed = true;
      unlisten?.();
      unlistenSkip?.();
    };
  }, []);

  useEffect(() => {
    if (!isSettingsHydrated || pet.isLoading) return;
    if (!settings.pet.enabled || !settings.pet.desktopWidget || !pet.snapshot.claimedAt) {
      void destroyCurrentAppWindow();
    }
  }, [isSettingsHydrated, pet.isLoading, pet.snapshot.claimedAt, settings.pet.desktopWidget, settings.pet.enabled]);

  useEffect(() => aiRuntime.subscribe(() => setAIVersion((current) => current + 1)), []);
  useEffect(() => {
    const sessions = aiRuntime.snapshots();
    const now = Date.now() / 1000;
    setActivityLine(desktopPetActivityLine(sessions, now));
    const refreshMs = nextDesktopPetActivityRefreshMs(sessions, now);
    if (refreshMs == null) return;
    const timer = window.setTimeout(() => {
      setActivityLine(desktopPetActivityLine(aiRuntime.snapshots(), Date.now() / 1000));
    }, refreshMs);
    return () => window.clearTimeout(timer);
  }, [aiVersion]);

  const state: PetAnimationState = useMemo(() => {
    if (!pet.snapshot.claimedAt) return "waiting";
    if (pet.dailyTokens > 0) return "running";
    return "idle";
  }, [pet.dailyTokens, pet.snapshot.claimedAt]);
  const displayName =
    pet.snapshot.customName ||
    pet.snapshot.customPet?.displayName ||
    tm(`pet.species.${pet.snapshot.species}.base`, pet.snapshot.species.replace(/^custom:/, ""));
  const bubbleText = activityLine || line;

  useEffect(() => {
    if (!pet.snapshot.claimedAt) return;
    setLine("");
    if (!settings.ai.pet.speechLlmEnabled || settings.ai.pet.speechMode === "off" || !window.__TAURI_INTERNALS__) {
      return;
    }
    let cancelled = false;
    let timeoutId: number | undefined;
    void invoke<PetIdleSpeechResponse>("pet_idle_speech", {
      request: {
        petName: displayName,
      },
    })
      .then((response) => {
        if (!cancelled && response.text.trim()) {
          setLine(response.text.trim());
          timeoutId = window.setTimeout(() => {
            if (!cancelled) setLine("");
          }, 12_000);
        }
      })
      .catch((error) => {
        console.debug("pet llm line skipped", error);
      });
    return () => {
      cancelled = true;
      if (timeoutId != null) window.clearTimeout(timeoutId);
    };
  }, [
    displayName,
    pet.snapshot.claimedAt,
    settings.ai.pet.speechLlmEnabled,
    settings.ai.pet.speechProviderId,
    settings.ai.pet.speechMode,
  ]);

  useEffect(() => {
    if (!window.__TAURI_INTERNALS__) return;
    void invoke("desktop_pet_set_bubble_visible", { visible: Boolean(bubbleText) }).catch((error) => {
      console.error("failed to update desktop pet bubble hit state", error);
    });
    return () => {
      void invoke("desktop_pet_set_bubble_visible", { visible: false }).catch(() => undefined);
    };
  }, [bubbleText]);

  const startDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (event.button !== 0 || !window.__TAURI_INTERNALS__) return;
    event.preventDefault();
    event.stopPropagation();
    void invoke("desktop_pet_start_drag").catch((error) => console.error("failed to drag desktop pet", error));
  };
  const openMenu = (event: MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    event.stopPropagation();
    if (!window.__TAURI_INTERNALS__) return;
    void invoke("desktop_pet_show_context_menu").catch((error) => console.error("failed to open desktop pet menu", error));
  };

  return (
    <div
      className="h-screen w-screen overflow-hidden bg-transparent pointer-events-none"
    >
      <div className="relative h-full w-full">
        {bubbleText ? (
          <DesktopPetSpeechBubble text={bubbleText} side={side} onContextMenu={openMenu} />
        ) : null}
        <div
          className={`${desktopPetSpriteClass(side)} pointer-events-auto cursor-grab active:cursor-grabbing`}
          onPointerDown={startDrag}
          onContextMenu={openMenu}
        >
          <PetSprite
            species={pet.snapshot.species}
            src={pet.snapshot.customPet?.spritesheetDataUrl}
            state={state}
            size={128}
            staticMode={settings.pet.staticMode}
          />
        </div>
      </div>
    </div>
  );
}

function DesktopPetSpeechBubble({
  text,
  side,
  onContextMenu,
}: {
  text: string;
  side: DesktopPetSide;
  onContextMenu: (event: MouseEvent<HTMLDivElement>) => void;
}) {
  return (
    <div
      className={`pointer-events-auto absolute w-[214px] min-h-[58px] ${desktopPetBubbleClass(side)}`}
      onContextMenu={onContextMenu}
    >
      <div className={`pixel-speech-bubble ${desktopPetTailClass(side)}`}>
        <div className="pixel-speech-bubble__text">{text}</div>
      </div>
    </div>
  );
}

function normalizeDesktopPetSide(value: string): DesktopPetSide {
  return value === "right" ? "right" : "left";
}

function desktopPetBubbleClass(side: DesktopPetSide) {
  return side === "right" ? "right-2 top-14" : "left-2 top-14";
}

function desktopPetSpriteClass(side: DesktopPetSide) {
  return side === "right" ? "absolute left-6 bottom-2" : "absolute right-6 bottom-2";
}

function desktopPetTailClass(side: DesktopPetSide) {
  return side === "right" ? "pixel-speech-bubble--left-tail" : "pixel-speech-bubble--right-tail";
}

function desktopPetActivityLine(sessions: AISessionSnapshot[], now: number) {
  const visibleSessions = sessions.filter((session) => {
    if (session.state === "responding" || session.state === "needsInput") return true;
    return session.hasCompletedTurn && now - session.updatedAt <= DESKTOP_PET_COMPLETED_STATUS_SECONDS;
  });
  if (!visibleSessions.length) return "";

  const permission = visibleSessions
    .filter((session) => session.state === "needsInput" && isPermissionRequestNotificationType(session.notificationType))
    .sort(compareUpdatedDesc)[0];
  if (permission) {
    return permission.targetToolName
      ? formatI18n(
          tm("pet.activity.permission_waiting_target_format", "%@ needs permission for %@"),
          permission.tool,
          permission.targetToolName,
        )
      : formatI18n(tm("pet.activity.permission_waiting_format", "%@ needs permission"), permission.tool);
  }

  const needsInput = visibleSessions
    .filter((session) => session.state === "needsInput")
    .sort(compareUpdatedDesc)[0];
  if (needsInput) {
    return normalizedPreview(needsInput.latestAssistantPreview) ||
      normalizedPreview(needsInput.message) ||
      formatI18n(tm("pet.activity.waiting_input_format", "%@ needs input"), needsInput.tool);
  }

  const running = visibleSessions
    .filter((session) => session.state === "responding")
    .sort(compareUpdatedDesc)[0];
  if (running) {
    return normalizedPreview(running.latestAssistantPreview) ||
      formatI18n(tm("pet.activity.running_format", "%@ is running"), running.tool);
  }

  const completed = visibleSessions
    .filter((session) => session.hasCompletedTurn)
    .sort(compareUpdatedDesc)[0];
  if (completed) {
    return completed.wasInterrupted
      ? formatI18n(tm("pet.activity.failed_format", "%@ failed"), completed.tool)
      : formatI18n(tm("pet.activity.completed_format", "%@ completed"), completed.tool);
  }

  return "";
}

function nextDesktopPetActivityRefreshMs(sessions: AISessionSnapshot[], now: number) {
  const nextExpiry = sessions
    .filter((session) => session.hasCompletedTurn && session.state !== "responding" && session.state !== "needsInput")
    .map((session) => session.updatedAt + DESKTOP_PET_COMPLETED_STATUS_SECONDS)
    .filter((expiresAt) => expiresAt > now)
    .sort((left, right) => left - right)[0];
  if (!nextExpiry) return null;
  return Math.max(250, Math.ceil((nextExpiry - now) * 1000));
}

function compareUpdatedDesc(left: AISessionSnapshot, right: AISessionSnapshot) {
  return right.updatedAt - left.updatedAt;
}

function isPermissionRequestNotificationType(value?: string | null) {
  return value === "PermissionRequest" || value === "permission-request" || value === "permission_request";
}

function normalizedPreview(value?: string | null) {
  const lines = (value ?? "")
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 3);
  return lines.join("\n").trim();
}
