import { describe, expect, it } from "vitest";
import { sanitizeProjectListSnapshot } from "./projectSnapshotCache";

describe("project snapshot cache", () => {
  it("keeps a valid cached snapshot for first paint", () => {
    const snapshot = sanitizeProjectListSnapshot({
      projects: [
        {
          id: "project-a",
          name: "Project A",
          path: "/tmp/project-a",
          badge: "PA",
          status: "active",
          branch: "main",
          changes: 3,
        },
      ],
      selectedProjectId: "project-a",
      selectedWorktreeIdByProject: { "project-a": "project-a" },
    });

    expect(snapshot?.projects[0]?.name).toBe("Project A");
    expect(snapshot?.selectedProjectId).toBe("project-a");
  });

  it("rejects empty cached project lists", () => {
    expect(sanitizeProjectListSnapshot({ projects: [] })).toBeNull();
    expect(sanitizeProjectListSnapshot({ projects: [{ id: "", name: "", path: "" }] })).toBeNull();
  });
});
