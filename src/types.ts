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
  gitDefaultPushRemoteName?: string | null;
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
  gitDefaultPushRemoteName?: string | null;
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
  text?: string;
  bytesBase64?: string;
  exitCode?: number | null;
  message?: string;
}

export interface RemoteStatus {
  enabled: boolean;
  relay: string;
  devices: number;
  encryption: string;
  status: "stopped" | "registering" | "connecting" | "connected" | "failed";
  message: string;
  hostId: string;
  pairing?: RemotePairingInfo | null;
  deviceList: RemoteHostDevice[];
  pendingPairings: RemotePendingPairing[];
}

export interface RemoteHostDevice {
  id: string;
  hostId: string;
  name: string;
  publicKey: string;
  createdAt: string;
  lastSeen: string;
  revokedAt?: string | null;
  online?: boolean | null;
}

export interface RemotePairingInfo {
  pairingId: string;
  code: string;
  secret: string;
  hostPublicKey?: string | null;
  cryptoVersion?: number | null;
  expiresAt: string;
  qrPayload: string;
}

export interface RemotePendingPairing {
  id: string;
  deviceName: string;
  devicePublicKey: string;
  code: string;
}

export interface PerformanceSnapshot {
  cpuPercent: number;
  memoryBytes: number;
  memory: PerformanceMemorySnapshot;
}

export interface PerformanceMemorySnapshot {
  mainBytes: number;
  webBytes: number;
  gpuBytes: number;
  otherBytes: number;
}

export type MainView = "terminal" | "files" | "review";
export type RightPanelKind = "git" | "files" | "ai" | "ssh";

export interface FileTabModel {
  path: string;
  language: string;
  dirty: boolean;
  readOnly: boolean;
}
