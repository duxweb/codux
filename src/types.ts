export type ProjectStatus = "active" | "reference" | "idle";

export interface ProjectSummary {
  id: string;
  name: string;
  path: string;
  badge: string;
  status: ProjectStatus;
  branch?: string;
  terminals?: number;
  changes?: number;
  badgeSymbol?: string | null;
  badgeColorHex?: string | null;
}

export interface ProjectListSnapshot {
  projects: ProjectSummary[];
  selectedProjectId?: string | null;
  selectedWorktreeIdByProject: Record<string, string>;
}

export interface WorkspaceProject {
  id: string;
  rootProjectId?: string;
  worktreeId?: string;
  name: string;
  path: string;
  badge: string;
  status: ProjectStatus;
  branch: string;
  baseBranch?: string | null;
  isDefaultWorktree?: boolean;
  aiState: "idle" | "running" | "review" | "done";
  terminals: number;
  changes: number;
  badgeSymbol?: string | null;
  badgeColorHex?: string | null;
}

export interface TerminalSession {
  id: string;
  projectId: string;
  slotId: string;
  title: string;
  cwd: string;
  shell: string;
  state: "starting" | "running" | "exited" | "error";
  exitCode?: number | null;
}

export interface TerminalEvent {
  kind: "output" | "exit" | "error";
  sessionId: string;
  data?: string;
  exitCode?: number | null;
  message?: string;
}

export interface RemoteStatus {
  enabled: boolean;
  relay: string;
  devices: number;
  encryption: string;
}

export interface PerformanceSnapshot {
  cpuPercent: number;
  memoryBytes: number;
  graphicsBytes: number;
}

export type MainView = "terminal" | "files" | "review";
export type RightPanelKind = "git" | "files" | "ai" | "ssh";

export interface FileTabModel {
  path: string;
  language: string;
  dirty: boolean;
  readOnly: boolean;
}
