export type AISessionSnapshot = {
  state: "idle" | "responding" | "needsInput";
  tool: string;
  updatedAt: number;
  hasCompletedTurn: boolean;
  wasInterrupted: boolean;
  notificationType?: string | null;
  targetToolName?: string | null;
  message?: string | null;
  latestAssistantPreview?: string | null;
};

export type DesktopPetActivityTone = "normal" | "attention" | "success" | "warning";
export type DesktopPetAnimationState = "idle" | "running" | "waiting" | "review" | "waving" | "failed";

export type DesktopPetActivityLine = {
  text: string;
  tone: DesktopPetActivityTone;
};

type Translate = (key: string, fallback: string) => string;

const DESKTOP_PET_COMPLETED_STATUS_SECONDS = 30;
const DESKTOP_PET_PERMISSION_STATUS_SECONDS = 12;

export function desktopPetActivityLine(
  sessions: AISessionSnapshot[],
  now: number,
  translate: Translate = (_key, fallback) => fallback,
): DesktopPetActivityLine {
  const permission = sessions
    .filter(
      (session) =>
        session.state === "needsInput" &&
        isPermissionRequestNotificationType(session.notificationType) &&
        now - session.updatedAt <= DESKTOP_PET_PERMISSION_STATUS_SECONDS,
    )
    .sort(compareUpdatedDesc)[0];
  if (permission) {
    return {
      text: permission.targetToolName
        ? formatActivity(
            translate("pet.activity.permission_waiting_target_format", "%@ needs permission for %@"),
            permission.tool,
            permission.targetToolName,
          )
        : formatActivity(translate("pet.activity.permission_waiting_format", "%@ needs permission"), permission.tool),
      tone: "attention",
    };
  }

  const visibleSessions = sessions.filter((session) => {
    if (session.state === "responding" || session.state === "needsInput") return true;
    return session.hasCompletedTurn && now - session.updatedAt <= DESKTOP_PET_COMPLETED_STATUS_SECONDS;
  });
  if (!visibleSessions.length) return emptyLine();
  const needsInput = visibleSessions.filter((session) => session.state === "needsInput").sort(compareUpdatedDesc)[0];
  if (needsInput) {
    return {
      text:
        normalizedPreview(needsInput.message) ||
        formatActivity(translate("pet.activity.waiting_input_format", "%@ needs input"), needsInput.tool),
      tone: "attention",
    };
  }
  const running = visibleSessions.filter((session) => session.state === "responding").sort(compareUpdatedDesc)[0];
  if (running) {
    return {
      text:
        normalizedPreview(running.latestAssistantPreview) ||
        formatActivity(translate("pet.activity.running_format", "%@ is running"), running.tool),
      tone: "normal",
    };
  }
  const completed = visibleSessions.filter((session) => session.hasCompletedTurn).sort(compareUpdatedDesc)[0];
  if (completed) {
    return {
      text: completed.wasInterrupted
        ? formatActivity(translate("pet.activity.failed_format", "%@ failed"), completed.tool)
        : formatActivity(translate("pet.activity.completed_format", "%@ completed"), completed.tool),
      tone: completed.wasInterrupted ? "warning" : "success",
    };
  }
  return emptyLine();
}

export function nextDesktopPetActivityRefreshMs(sessions: AISessionSnapshot[], now: number) {
  const nextExpiry = [
    ...sessions
      .filter((session) => session.hasCompletedTurn && session.state !== "responding" && session.state !== "needsInput")
      .map((session) => session.updatedAt + DESKTOP_PET_COMPLETED_STATUS_SECONDS),
    ...sessions
      .filter(
        (session) => session.state === "needsInput" && isPermissionRequestNotificationType(session.notificationType),
      )
      .map((session) => session.updatedAt + DESKTOP_PET_PERMISSION_STATUS_SECONDS),
  ]
    .filter((expiresAt) => expiresAt > now)
    .sort((left, right) => left - right)[0];
  return nextExpiry ? Math.max(250, Math.ceil((nextExpiry - now) * 1000)) : null;
}

export function desktopPetAnimationState({
  claimed,
  dailyExperienceTokens,
  sessions,
  now,
}: {
  claimed: boolean;
  dailyExperienceTokens: number;
  sessions: AISessionSnapshot[];
  now: number;
}): DesktopPetAnimationState {
  if (!claimed) return "waiting";
  const activeSessions = sessions.filter((session) => session.state === "responding" || session.state === "needsInput");
  if (activeSessions.some((session) => session.state === "needsInput")) return "review";
  if (activeSessions.length > 0) return "running";

  const completed = sessions
    .filter(
      (session) =>
        session.hasCompletedTurn &&
        session.state !== "responding" &&
        session.state !== "needsInput" &&
        now - session.updatedAt <= DESKTOP_PET_COMPLETED_STATUS_SECONDS,
    )
    .sort(compareUpdatedDesc)[0];
  if (completed) return completed.wasInterrupted ? "failed" : "waving";

  return dailyExperienceTokens > 0 ? "running" : "idle";
}

function emptyLine(): DesktopPetActivityLine {
  return { text: "", tone: "normal" };
}

function compareUpdatedDesc(left: AISessionSnapshot, right: AISessionSnapshot) {
  return right.updatedAt - left.updatedAt;
}

function isPermissionRequestNotificationType(value?: string | null) {
  return value === "PermissionRequest" || value === "permission-request" || value === "permission_request";
}

function formatActivity(template: string, ...values: string[]) {
  let index = 0;
  return template.replace(/%@/g, () => values[index++] ?? "");
}

function normalizedPreview(value?: string | null) {
  const preview = (value ?? "")
    .replace(/\r\n?/g, "\n")
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean)
    .slice(0, 3)
    .join("\n")
    .trim();
  return preview;
}
