import { describe, expect, test } from "vitest";
import { workspacePathsMatch } from "./workspaceCommands";

describe("workspace command path matching", () => {
  test("matches Windows extended and regular project paths", () => {
    expect(workspacePathsMatch("\\\\?\\F:\\codux-tauri", "F:\\codux-tauri\\")).toBe(true);
    expect(workspacePathsMatch("//?/F:/codux-tauri", "f:/codux-tauri")).toBe(true);
  });

  test("matches normalized POSIX project paths", () => {
    expect(workspacePathsMatch("/Volumes/Web/codux-tauri/", "/Volumes/Web/codux-tauri")).toBe(true);
    expect(workspacePathsMatch("/Volumes/Web/codux-tauri", "/Volumes/Web/other")).toBe(false);
  });
});
