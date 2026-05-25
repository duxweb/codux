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

export type DesktopPetLlmContext = {
  event: "permission" | "needsInput" | "completed" | "failed" | "running";
  fallbackText: string;
  tone: DesktopPetActivityTone;
  tool: string;
  updatedAt: number;
};

type Translate = (key: string, fallback: string) => string;

const DESKTOP_PET_COMPLETED_STATUS_SECONDS = 30;

export function desktopPetActivityLine(
  sessions: AISessionSnapshot[],
  now: number,
  translate: Translate = (_key, fallback) => fallback,
): DesktopPetActivityLine {
  const permission = sessions
    .filter(
      (session) =>
        session.state === "needsInput" &&
        isPermissionRequestNotificationType(session.notificationType),
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

  const needsInput = sessions.filter((session) => session.state === "needsInput").sort(compareUpdatedDesc)[0];
  if (needsInput) {
    return {
      text:
        normalizedPreview(needsInput.message) ||
        formatActivity(translate("pet.activity.waiting_input_format", "%@ needs input"), needsInput.tool),
      tone: "attention",
    };
  }

  const completed = sessions.filter((session) => isVisibleCompleted(session, now)).sort(compareUpdatedDesc)[0];
  if (completed) {
    return {
      text: completed.wasInterrupted
        ? formatActivity(translate("pet.activity.failed_format", "%@ failed"), completed.tool)
        : formatActivity(translate("pet.activity.completed_format", "%@ completed"), completed.tool),
      tone: completed.wasInterrupted ? "warning" : "success",
    };
  }

  const running = sessions.filter((session) => session.state === "responding").sort(compareUpdatedDesc)[0];
  if (running) {
    return {
      text:
        normalizedPreview(running.latestAssistantPreview) ||
        formatActivity(translate("pet.activity.running_format", "%@ is running"), running.tool),
      tone: "normal",
    };
  }
  return emptyLine();
}

export function desktopPetLlmContext(
  sessions: AISessionSnapshot[],
  now: number,
  translate: Translate = (_key, fallback) => fallback,
): DesktopPetLlmContext | null {
  const permission = sessions
    .filter(
      (session) =>
        session.state === "needsInput" &&
        isPermissionRequestNotificationType(session.notificationType),
    )
    .sort(compareUpdatedDesc)[0];
  if (permission) {
    return {
      event: "permission",
      fallbackText: permission.targetToolName
        ? formatActivity(
            translate("pet.activity.permission_waiting_target_format", "%@ needs permission for %@"),
            permission.tool,
            permission.targetToolName,
          )
        : formatActivity(translate("pet.activity.permission_waiting_format", "%@ needs permission"), permission.tool),
      tone: "attention",
      tool: permission.tool,
      updatedAt: permission.updatedAt,
    };
  }

  const needsInput = sessions.filter((session) => session.state === "needsInput").sort(compareUpdatedDesc)[0];
  if (needsInput && !normalizedPreview(needsInput.message)) {
    return {
      event: "needsInput",
      fallbackText: formatActivity(translate("pet.activity.waiting_input_format", "%@ needs input"), needsInput.tool),
      tone: "attention",
      tool: needsInput.tool,
      updatedAt: needsInput.updatedAt,
    };
  }

  const completed = sessions.filter((session) => isVisibleCompleted(session, now)).sort(compareUpdatedDesc)[0];
  if (completed) {
    const failed = completed.wasInterrupted;
    return {
      event: failed ? "failed" : "completed",
      fallbackText: failed
        ? formatActivity(translate("pet.activity.failed_format", "%@ failed"), completed.tool)
        : formatActivity(translate("pet.activity.completed_format", "%@ completed"), completed.tool),
      tone: failed ? "warning" : "success",
      tool: completed.tool,
      updatedAt: completed.updatedAt,
    };
  }

  const running = sessions.filter((session) => session.state === "responding").sort(compareUpdatedDesc)[0];
  if (running && !normalizedPreview(running.latestAssistantPreview)) {
    return {
      event: "running",
      fallbackText: formatActivity(translate("pet.activity.running_format", "%@ is running"), running.tool),
      tone: "normal",
      tool: running.tool,
      updatedAt: running.updatedAt,
    };
  }
  return null;
}

export function nextDesktopPetActivityRefreshMs(sessions: AISessionSnapshot[], now: number) {
  const nextExpiry = [
    ...sessions
      .filter((session) => isVisibleCompleted(session, now))
      .map((session) => session.updatedAt + DESKTOP_PET_COMPLETED_STATUS_SECONDS),
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
  const needsInput = sessions.some((session) => session.state === "needsInput");
  if (needsInput) return "review";

  const completed = sessions.filter((session) => isVisibleCompleted(session, now)).sort(compareUpdatedDesc)[0];
  if (completed) return completed.wasInterrupted ? "failed" : "waving";

  if (sessions.some((session) => session.state === "responding")) return "running";

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

function isVisibleCompleted(session: AISessionSnapshot, now: number) {
  return (
    session.hasCompletedTurn &&
    session.state !== "responding" &&
    session.state !== "needsInput" &&
    now - session.updatedAt <= DESKTOP_PET_COMPLETED_STATUS_SECONDS
  );
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
