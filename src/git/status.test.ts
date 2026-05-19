import { describe, expect, it } from "vitest";
import { isGitChangeForProject, normalizeGitEventPath } from "./status";

describe("git status event helpers", () => {
  it("normalizes platform path separators", () => {
    expect(normalizeGitEventPath("C:\\work\\repo\\")).toBe("C:/work/repo");
  });

  it("matches repository events for the current project", () => {
    expect(
      isGitChangeForProject(
        {
          projectPath: "/work/repo",
          repositoryPath: "/work/repo",
          changedPaths: ["/work/repo/.git/index"],
        },
        "/work/repo",
      ),
    ).toBe(true);
  });

  it("matches subdirectory projects inside the repository", () => {
    expect(
      isGitChangeForProject(
        {
          projectPath: "/work/repo/packages/app",
          repositoryPath: "/work/repo",
          changedPaths: ["/work/repo/src/main.ts"],
        },
        "/work/repo/packages/app",
      ),
    ).toBe(true);
  });

  it("ignores changes from another repository", () => {
    expect(
      isGitChangeForProject(
        {
          projectPath: "/work/other",
          repositoryPath: "/work/other",
          changedPaths: ["/work/other/.git/index"],
        },
        "/work/repo",
      ),
    ).toBe(false);
  });
});
