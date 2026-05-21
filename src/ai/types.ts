export type AIHookKind = "sessionStarted" | "promptSubmitted" | "needsInput" | "turnCompleted" | "sessionEnded";

export type AIState = "idle" | "responding" | "needsInput";

export type AIResponseState = "idle" | "responding";

export type AIRuntimeUpdateSource = "socket" | "hook" | "probe";

export type AIRuntimeSessionOrigin = "unknown" | "fresh" | "restored";

export type AIHookEventMetadata = {
  transcriptPath?: string | null;
  notificationType?: string | null;
  source?: string | null;
  reason?: string | null;
  cwd?: string | null;
  targetToolName?: string | null;
  message?: string | null;
  wasInterrupted?: boolean | null;
  hasCompletedTurn?: boolean | null;
};

export type AIHookEventPayload = {
  kind: AIHookKind;
  terminalID: string;
  terminalInstanceID?: string | null;
  projectID: string;
  projectName: string;
  projectPath?: string | null;
  sessionTitle: string;
  tool: string;
  aiSessionID?: string | null;
  model?: string | null;
  inputTokens?: number | null;
  outputTokens?: number | null;
  cachedInputTokens?: number | null;
  totalTokens?: number | null;
  updatedAt: number;
  metadata?: AIHookEventMetadata | null;
};

export type AIRuntimeEvent = {
  kind: "hook";
  payload: AIHookEventPayload;
};

export type AIRuntimeTerminalState = {
  terminalId: string;
  projectId: string;
  slotId: string;
  title: string;
  cwd: string;
  tool?: string | null;
  isActive: boolean;
  sessionKey?: string | null;
  terminalInstanceId?: string | null;
};

export type AIRuntimeBridgeSnapshot = {
  socketPath: string;
  wrapperBinPath: string;
  zdotdirPath: string;
  hookScriptPath: string;
  managedHookScriptPath: string;
  hookConfig: AIRuntimeHookConfigStatus;
  terminals: AIRuntimeTerminalState[];
};

export type AIRuntimeHookConfigStatus = {
  codex: AIRuntimeToolHookConfigStatus;
  claude: AIRuntimeToolHookConfigStatus;
  gemini: AIRuntimeToolHookConfigStatus;
  opencode: AIRuntimeToolHookConfigStatus;
};

export type AIRuntimeToolHookConfigStatus = {
  configured: boolean;
  configPath: string;
  missing: string[];
};

export type AIRuntimeContextSnapshot = {
  tool: string;
  externalSessionID?: string | null;
  model?: string | null;
  assistantPreview?: string | null;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
  totalTokens: number;
  updatedAt: number;
  startedAt?: number | null;
  completedAt?: number | null;
  responseState?: AIResponseState | null;
  wasInterrupted?: boolean;
  hasCompletedTurn?: boolean;
  sessionOrigin?: AIRuntimeSessionOrigin;
  source?: AIRuntimeUpdateSource;
};

export type AIRuntimeProbeRequest = {
  terminalId: string;
  terminalInstanceId?: string | null;
  projectId: string;
  projectPath?: string | null;
  tool: string;
  externalSessionId?: string | null;
  transcriptPath?: string | null;
  startedAt?: number | null;
  updatedAt: number;
};

export type AISessionSnapshot = {
  terminalId: string;
  terminalInstanceId?: string | null;
  projectId: string;
  projectName: string;
  projectPath?: string | null;
  sessionTitle: string;
  tool: string;
  aiSessionId?: string | null;
  model?: string | null;
  state: AIState;
  status: "idle" | "running" | "needs-input";
  isRunning: boolean;
  inputTokens: number;
  outputTokens: number;
  cachedInputTokens: number;
  totalTokens: number;
  baselineTotalTokens: number;
  startedAt?: number;
  updatedAt: number;
  activeTurnStartedAt?: number;
  runtimeTurnStartedAt?: number;
  hasCompletedTurn: boolean;
  wasInterrupted: boolean;
  transcriptPath?: string | null;
  notificationType?: string | null;
  targetToolName?: string | null;
  message?: string | null;
  latestAssistantPreview?: string | null;
};

export type AIProjectPhase =
  | { kind: "idle" }
  | { kind: "running"; tool: string }
  | { kind: "needsInput"; tool: string }
  | { kind: "completed"; tool: string; wasInterrupted: boolean; updatedAt: number };

export type AIProjectTotals = {
  totalTokens: number;
  cachedInputTokens: number;
  running: number;
  needsInput: number;
  completed: number;
};

export type AIProjectStateSnapshot = {
  projectId: string;
  projectPhase: AIProjectPhase;
  completedPhase: AIProjectPhase;
  totals: AIProjectTotals;
};

export type AILatestCompletion = {
  id: string;
  projectId: string;
  projectName: string;
  tool: string;
  wasInterrupted: boolean;
  updatedAt: number;
};

export type AIRuntimeStateSnapshot = {
  sessions: AISessionSnapshot[];
  projects: AIProjectStateSnapshot[];
  globalTotals: AIProjectTotals;
  needsInputCount: number;
  runningCount: number;
  completionCount: number;
  latestCompletion?: AILatestCompletion | null;
  updatedAt: number;
};
