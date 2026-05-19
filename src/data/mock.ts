import type { WorkspaceProject } from "../types";

export const fallbackProjects: WorkspaceProject[] = [
  {
    id: "4c6a95e6-46c5-5e10-a5e4-4f001ccf0a01",
    name: "codux-tauri",
    path: "/Volumes/Web/codux-tauri",
    badge: "TA",
    status: "active",
    branch: "main",
    aiState: "running",
    terminals: 2,
    changes: 12,
  },
  {
    id: "8ad59c16-0242-5e34-ae34-4f001ccf0a02",
    name: "codux",
    path: "/Volumes/Web/codux",
    badge: "SW",
    status: "reference",
    branch: "main",
    aiState: "review",
    terminals: 6,
    changes: 4,
  },
  {
    id: "de3f9f37-3d2a-569b-91ec-4f001ccf0a03",
    name: "arsenal-api",
    path: "/Volumes/Web/arsenal-api",
    badge: "AR",
    status: "idle",
    branch: "master",
    aiState: "idle",
    terminals: 1,
    changes: 6,
  },
];
