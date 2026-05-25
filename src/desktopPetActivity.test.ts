import { describe, expect, it } from "vitest";
import {
  desktopPetActivityLine,
  desktopPetLlmContext,
  desktopPetAnimationState,
  nextDesktopPetActivityRefreshMs,
  type AISessionSnapshot,
} from "./desktopPetActivity";

function session(patch: Partial<AISessionSnapshot>): AISessionSnapshot {
  return {
    state: "idle",
    tool: "codex",
    updatedAt: 100,
    hasCompletedTurn: false,
    wasInterrupted: false,
    ...patch,
  };
}

describe("desktop pet activity", () => {
  it("shows live assistant previews while an AI runtime is responding", () => {
    expect(
      desktopPetActivityLine(
        [
          session({
            state: "responding",
            latestAssistantPreview: "我先检查项目结构。\n然后确认入口和配置。",
          }),
        ],
        101,
      ),
    ).toEqual({ text: "我先检查项目结构。\n然后确认入口和配置。", tone: "normal" });
  });

  it("clamps live assistant previews to three non-empty lines", () => {
    expect(
      desktopPetActivityLine(
        [
          session({
            state: "responding",
            latestAssistantPreview: "第一行\n\n第二行\n第三行\n第四行",
          }),
        ],
        101,
      ),
    ).toEqual({ text: "第一行\n第二行\n第三行", tone: "normal" });
  });

  it("does not truncate long live assistant preview lines beyond the native three-line clamp", () => {
    const longLine = "正在执行一个很长的 AI 任务输出".repeat(8);
    expect(
      desktopPetActivityLine(
        [
          session({
            state: "responding",
            latestAssistantPreview: longLine,
          }),
        ],
        101,
      ),
    ).toEqual({ text: longLine, tone: "normal" });
  });

  it("keeps the bubble hidden when no runtime activity is visible", () => {
    expect(desktopPetActivityLine([session({ updatedAt: 10 })], 100)).toEqual({ text: "", tone: "normal" });
    expect(nextDesktopPetActivityRefreshMs([session({ updatedAt: 10 })], 100)).toBeNull();
  });

  it("keeps completed success messages visible for thirty seconds with success tone", () => {
    expect(
      desktopPetActivityLine(
        [
          session({
            updatedAt: 100,
            hasCompletedTurn: true,
          }),
        ],
        129.5,
      ),
    ).toEqual({ text: "codex completed", tone: "success" });
    expect(desktopPetActivityLine([session({ updatedAt: 100, hasCompletedTurn: true })], 131)).toEqual({
      text: "",
      tone: "normal",
    });
    expect(nextDesktopPetActivityRefreshMs([session({ updatedAt: 100, hasCompletedTurn: true })], 129.5)).toBe(500);
  });

  it("uses orange attention tone for permission requests until runtime leaves needs input", () => {
    const permission = session({
      state: "needsInput",
      updatedAt: 100,
      notificationType: "PermissionRequest",
      targetToolName: "Shell",
    });
    expect(desktopPetActivityLine([permission], 129.5)).toEqual({
      text: "codex needs permission for Shell",
      tone: "attention",
    });
    expect(desktopPetActivityLine([permission], 131)).toEqual({
      text: "codex needs permission for Shell",
      tone: "attention",
    });
    expect(desktopPetActivityLine([{ ...permission, state: "responding" }], 131)).toEqual({
      text: "codex is running",
      tone: "normal",
    });
    expect(desktopPetActivityLine([{ ...permission, state: "idle" }], 131)).toEqual({
      text: "",
      tone: "normal",
    });
    expect(nextDesktopPetActivityRefreshMs([permission], 129.5)).toBeNull();
  });

  it("prioritizes permission, then recent completion, then running sessions", () => {
    const running = session({
      state: "responding",
      updatedAt: 110,
      latestAssistantPreview: "正在处理",
    });
    const completed = session({
      updatedAt: 120,
      hasCompletedTurn: true,
    });
    const permission = session({
      state: "needsInput",
      updatedAt: 125,
      notificationType: "permission_request",
      targetToolName: "Shell",
    });
    expect(desktopPetActivityLine([running, completed, permission], 126)).toEqual({
      text: "codex needs permission for Shell",
      tone: "attention",
    });
    expect(desktopPetActivityLine([running, completed], 126)).toEqual({
      text: "codex completed",
      tone: "success",
    });
    expect(desktopPetActivityLine([running, completed], 151)).toEqual({
      text: "正在处理",
      tone: "normal",
    });
  });

  it("exposes LLM context only for safe template activity lines", () => {
    expect(desktopPetLlmContext([session({ state: "responding", latestAssistantPreview: "正在处理具体代码" })], 101)).toBeNull();
    expect(desktopPetLlmContext([session({ state: "responding", latestAssistantPreview: "" })], 101)).toEqual({
      event: "running",
      fallbackText: "codex is running",
      tone: "normal",
      tool: "codex",
      updatedAt: 100,
    });
    expect(desktopPetLlmContext([session({ hasCompletedTurn: true, updatedAt: 100 })], 101)).toEqual({
      event: "completed",
      fallbackText: "codex completed",
      tone: "success",
      tool: "codex",
      updatedAt: 100,
    });
  });

  it("maps AI runtime activity to desktop pet animation states like the native app", () => {
    expect(
      desktopPetAnimationState({
        claimed: false,
        dailyExperienceTokens: 0,
        sessions: [],
        now: 100,
      }),
    ).toBe("waiting");
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [session({ state: "responding" })],
        now: 100,
      }),
    ).toBe("running");
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [session({ state: "needsInput" })],
        now: 100,
      }),
    ).toBe("review");
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [session({ hasCompletedTurn: true, updatedAt: 95 })],
        now: 100,
      }),
    ).toBe("waving");
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [session({ hasCompletedTurn: true, wasInterrupted: true, updatedAt: 95 })],
        now: 100,
      }),
    ).toBe("failed");
  });

  it("uses global pet animation priority across all sessions", () => {
    const running = session({ state: "responding", updatedAt: 100 });
    const completed = session({ hasCompletedTurn: true, updatedAt: 105 });
    const permission = session({
      state: "needsInput",
      updatedAt: 110,
      notificationType: "PermissionRequest",
    });
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [running, completed, permission],
        now: 111,
      }),
    ).toBe("review");
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [running, completed],
        now: 111,
      }),
    ).toBe("waving");
    expect(
      desktopPetAnimationState({
        claimed: true,
        dailyExperienceTokens: 0,
        sessions: [running, completed],
        now: 136,
      }),
    ).toBe("running");
  });
});
