import type { WorkspaceProject } from "../types";

export const fallbackProjects: WorkspaceProject[] = [
  {
    id: "00000000-0000-5000-8000-000000000001",
    name: "Preview Workspace",
    path: "/preview/workspace",
    badge: "PW",
    status: "active",
    branch: "main",
    aiState: "idle",
    terminals: 0,
    changes: 0,
  },
];
