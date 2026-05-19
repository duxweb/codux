import fs from "node:fs"
import { spawnSync } from "node:child_process"

const socketPath = process.env.DMUX_RUNTIME_SOCKET ?? ""
const logFile = process.env.DMUX_LOG_FILE ?? ""
const opencodeSessionMapDir = process.env.DMUX_OPENCODE_SESSION_MAP_DIR ?? ""
const runtimeSessionID = process.env.DMUX_SESSION_ID ?? ""

function log(message, extra) {
  if (!logFile) return
  const suffix = extra ? ` ${JSON.stringify(extra)}` : ""
  fs.appendFileSync(
    logFile,
    `[${new Date().toISOString()}] [opencode-plugin] ${message}${suffix}\n`,
  )
}

function nowSeconds() {
  return Date.now() / 1000
}

function sessionMapPath() {
  if (!opencodeSessionMapDir || !runtimeSessionID) return null
  return `${opencodeSessionMapDir}/opencode-session-${runtimeSessionID}.json`
}

function writeSessionMap({ externalSessionID, model }) {
  const path = sessionMapPath()
  if (!path || !externalSessionID) return
  try {
    fs.mkdirSync(opencodeSessionMapDir, { recursive: true })
    fs.writeFileSync(
      path,
      JSON.stringify(
        {
          runtimeSessionID,
          externalSessionID,
          model: model ?? null,
          updatedAt: nowSeconds(),
        },
        null,
        2,
      ),
      "utf8",
    )
  } catch (error) {
    log("session-map-write-error", { message: error instanceof Error ? error.message : String(error), path })
  }
}

function clearSessionMap(sessionID) {
  const path = sessionMapPath()
  if (!path) return
  if (sessionID && currentSessionID && sessionID !== currentSessionID) return
  try {
    fs.rmSync(path, { force: true })
  } catch (error) {
    log("session-map-clear-error", { message: error instanceof Error ? error.message : String(error), path })
  }
}

function sendRuntimePayload(payload) {
  if (!socketPath) return
  const message = JSON.stringify({ kind: "opencode-runtime", payload })
  const result = spawnSync("/usr/bin/nc", ["-U", "-w", "1", socketPath], {
    input: `${message}\n`,
    encoding: "utf8",
    stdio: ["pipe", "ignore", "pipe"],
    timeout: 1000,
  })
  if (result.error) {
    log("socket-error", { message: result.error.message, socketPath })
    return
  }
  if (result.status === 0) {
    return
  }
  log("socket-close-error", {
    code: result.status,
    stderr: result.stderr?.trim() || null,
    signal: result.signal ?? null,
    socketPath,
  })
}

function sendAIHookPayload(payload) {
  if (!socketPath) return
  const message = JSON.stringify({ kind: "ai-hook", payload })
  const result = spawnSync("/usr/bin/nc", ["-U", "-w", "1", socketPath], {
    input: `${message}\n`,
    encoding: "utf8",
    stdio: ["pipe", "ignore", "pipe"],
    timeout: 1000,
  })
  if (result.error) {
    log("ai-hook-socket-error", { message: result.error.message, socketPath })
    return
  }
  if (result.status === 0) {
    return
  }
  log("ai-hook-socket-close-error", {
    code: result.status,
    stderr: result.stderr?.trim() || null,
    signal: result.signal ?? null,
    socketPath,
  })
}

function basePayload({ externalSessionID, responseState, model, status }) {
  const phase = status ?? (responseState === "idle" ? "idle" : "running")
  const currentTime = nowSeconds()
  return {
    sessionId: process.env.DMUX_SESSION_ID,
    sessionInstanceId: process.env.DMUX_SESSION_INSTANCE_ID ?? null,
    invocationId: process.env.DMUX_ACTIVE_AI_INVOCATION_ID ?? null,
    externalSessionID: externalSessionID ?? null,
    projectId: process.env.DMUX_PROJECT_ID,
    projectName: process.env.DMUX_PROJECT_NAME ?? "Workspace",
    projectPath: process.env.DMUX_PROJECT_PATH ?? "",
    sessionTitle: process.env.DMUX_SESSION_TITLE ?? "Terminal",
    tool: "opencode",
    model: model ?? null,
    status: phase,
    responseState: responseState ?? null,
    updatedAt: currentTime,
    startedAt: activePromptStartedAt ?? Number(process.env.DMUX_ACTIVE_AI_STARTED_AT ?? currentTime),
    finishedAt: null,
    inputTokens: 0,
    outputTokens: 0,
    totalTokens: 0,
    contextWindow: null,
    contextUsedTokens: null,
    contextUsagePercent: null,
  }
}

function readString(value) {
  return typeof value === "string" && value.trim() ? value.trim() : null
}

function readObject(value) {
  return value && typeof value === "object" ? value : null
}

function pickFirstString(input, keys) {
  if (!input || typeof input !== "object") return null
  for (const key of keys) {
    const direct = readString(input[key])
    if (direct) return direct
  }
  for (const value of Object.values(input)) {
    const nested = readObject(value)
    if (!nested) continue
    const found = pickFirstString(nested, keys)
    if (found) return found
  }
  return null
}

function pickDirectString(input, keys) {
  if (!input || typeof input !== "object") return null
  for (const key of keys) {
    const direct = readString(input[key])
    if (direct) return direct
  }
  return null
}

function sessionIDFrom(properties) {
  return pickDirectString(properties, ["sessionID", "sessionId"])
    ?? pickDirectString(readObject(properties.session), ["id", "sessionID", "sessionId"])
    ?? pickDirectString(properties, ["id"])
}

function messageRoleFrom(properties) {
  return pickDirectString(properties, ["role"])
    ?? pickDirectString(readObject(properties.message), ["role"])
}

function modelFrom(properties) {
  return pickDirectString(properties, ["modelID", "modelId", "model"])
    ?? pickDirectString(readObject(properties.model), ["id", "modelID", "modelId", "model"])
    ?? pickDirectString(readObject(properties.message), ["modelID", "modelId", "model"])
}

function messageIDFrom(properties) {
  return pickDirectString(properties, ["messageID", "messageId", "id"])
    ?? pickDirectString(readObject(properties.message), ["id", "messageID", "messageId"])
}

function commandNameFrom(properties) {
  return pickDirectString(properties, ["command", "name"])
}

function messageFrom(properties) {
  return pickDirectString(properties, ["message", "prompt", "description"])
}

function toolNameFrom(properties) {
  return pickDirectString(properties, ["toolName", "tool", "name"])
}

let currentSessionID = readString(process.env.DMUX_EXTERNAL_SESSION_ID)
let currentModel = null
let lastUserMessageID = null
let lastSignature = null
let hasActivePrompt = false
let activePromptStartedAt = null

function dispatchUpdate({ externalSessionID, responseState, model, status, reason }) {
  const nextExternalSessionID = readString(externalSessionID) ?? currentSessionID
  if (!nextExternalSessionID) {
    log("skip-dispatch-missing-session", { reason })
    return
  }
  currentSessionID = nextExternalSessionID
  currentModel = readString(model) ?? currentModel
  writeSessionMap({ externalSessionID: currentSessionID, model: currentModel })
  const signature = `${nextExternalSessionID}|${responseState ?? "nil"}|${currentModel ?? "nil"}`
  if (signature == lastSignature) {
    return
  }
  lastSignature = signature
  sendRuntimePayload(
    basePayload({
      externalSessionID: nextExternalSessionID,
      responseState,
      model: currentModel,
      status,
    }),
  )
  log("dispatch", { reason, externalSessionID: nextExternalSessionID, responseState: responseState ?? null, model: currentModel })
}

function dispatchAIHook({
  kind,
  externalSessionID,
  model,
  totalTokens,
  notificationType,
  reason,
  targetToolName,
  message,
}) {
  const nextExternalSessionID = readString(externalSessionID) ?? currentSessionID
  const nextModel = readString(model) ?? currentModel
  sendAIHookPayload({
    kind,
    terminalID: process.env.DMUX_SESSION_ID,
    terminalInstanceID: process.env.DMUX_SESSION_INSTANCE_ID ?? null,
    projectID: process.env.DMUX_PROJECT_ID,
    projectName: process.env.DMUX_PROJECT_NAME ?? "Workspace",
    projectPath: process.env.DMUX_PROJECT_PATH ?? "",
    sessionTitle: process.env.DMUX_SESSION_TITLE ?? "Terminal",
    tool: "opencode",
    aiSessionID: nextExternalSessionID ?? null,
    model: nextModel ?? null,
    totalTokens: Number.isInteger(totalTokens) ? totalTokens : null,
    updatedAt: nowSeconds(),
    metadata: {
      transcriptPath: null,
      notificationType: notificationType ?? null,
      source: null,
      reason: reason ?? null,
      targetToolName: targetToolName ?? null,
      message: message ?? null,
    },
  })
}

export const DmuxRuntimePlugin = async ({ client }) => {
  await client.app.log({
    body: {
      service: "dmux-opencode-plugin",
      level: "info",
      message: "Plugin initialized",
      extra: {
        hasSocket: Boolean(socketPath),
        hasSession: Boolean(process.env.DMUX_SESSION_ID),
      },
    },
  })

  log("initialized", {
    hasSocket: Boolean(socketPath),
    sessionID: process.env.DMUX_SESSION_ID ?? null,
    externalSessionID: currentSessionID,
  })
  if (currentSessionID) {
    writeSessionMap({ externalSessionID: currentSessionID, model: currentModel })
  }

  return {
    event: async ({ event }) => {
      const type = event?.type
      const properties = readObject(event?.properties) ?? {}

      switch (type) {
        case "session.created": {
          dispatchAIHook({
            kind: "sessionStarted",
            externalSessionID: sessionIDFrom(properties),
            model: modelFrom(properties),
            totalTokens: null,
            reason: type,
          })
          dispatchUpdate({
            externalSessionID: sessionIDFrom(properties),
            responseState: null,
            model: modelFrom(properties),
            reason: type,
          })
          return
        }
        case "session.updated": {
          const sessionID = sessionIDFrom(properties)
          const model = modelFrom(properties)
          if (sessionID) {
            currentSessionID = sessionID
            currentModel = model ?? currentModel
            writeSessionMap({ externalSessionID: currentSessionID, model: currentModel })
          }
          log("session-updated", { externalSessionID: sessionID ?? null, model: model ?? null })
          return
        }
        case "session.status": {
          const sessionID = sessionIDFrom(properties)
          const status = readObject(properties.status)
          const statusType = readString(status?.type)
          if (statusType === "busy" || statusType === "retry") {
            hasActivePrompt = true
            activePromptStartedAt = activePromptStartedAt ?? nowSeconds()
            dispatchUpdate({
              externalSessionID: sessionID,
              responseState: "responding",
              model: modelFrom(properties),
              reason: `status:${statusType}`,
            })
            return
          }
          if (statusType === "idle") {
            dispatchUpdate({
              externalSessionID: sessionID,
              responseState: "idle",
              model: modelFrom(properties),
              status: "idle",
              reason: `status:${statusType}`,
            })
            log("status-idle", { externalSessionID: sessionID ?? null })
          }
          return
        }
        case "session.idle": {
          const sessionID = sessionIDFrom(properties)
          if (!hasActivePrompt) {
            log("ignore-session-idle", { externalSessionID: sessionID ?? null, reason: "no-active-prompt" })
            return
          }
          hasActivePrompt = false
          activePromptStartedAt = null
          dispatchAIHook({
            kind: "turnCompleted",
            externalSessionID: sessionID,
            model: modelFrom(properties),
            totalTokens: null,
            reason: type,
          })
          dispatchUpdate({
            externalSessionID: sessionID,
            responseState: "idle",
            model: modelFrom(properties),
            status: "completed",
            reason: type,
          })
          return
        }
        case "session.deleted": {
          dispatchAIHook({
            kind: "sessionEnded",
            externalSessionID: sessionIDFrom(properties),
            model: modelFrom(properties),
            totalTokens: null,
            reason: type,
          })
          clearSessionMap(sessionIDFrom(properties))
          return
        }
        case "message.updated": {
          const role = messageRoleFrom(properties)
          const model = modelFrom(properties)
          const sessionID = sessionIDFrom(properties)
          if (role === "user") {
            const messageID = messageIDFrom(properties) ?? `user-${Date.now()}`
            if (messageID === lastUserMessageID) return
            lastUserMessageID = messageID
            hasActivePrompt = true
            activePromptStartedAt = nowSeconds()
            dispatchUpdate({
              externalSessionID: sessionID,
              responseState: "responding",
              model,
              reason: "message:user",
            })
            dispatchAIHook({
              kind: "promptSubmitted",
              externalSessionID: sessionID,
              model,
              totalTokens: null,
              reason: "message:user",
            })
            return
          }
          if (role) {
            log("ignore-message-updated", { role, externalSessionID: sessionID ?? null })
          }
          if (model) {
            currentModel = model
          }
          return
        }
        case "tui.command.execute": {
          const commandName = commandNameFrom(properties)
          if (commandName === "sessions" || commandName === "resume" || commandName === "continue") {
            dispatchAIHook({
              kind: "sessionStarted",
              externalSessionID: sessionIDFrom(properties),
              model: modelFrom(properties),
              totalTokens: null,
              reason: `command:${commandName}`,
            })
            dispatchUpdate({
              externalSessionID: sessionIDFrom(properties),
              responseState: null,
              model: modelFrom(properties),
              reason: `command:${commandName}`,
            })
          }
          return
        }
        case "permission.asked": {
          dispatchAIHook({
            kind: "needsInput",
            externalSessionID: sessionIDFrom(properties),
            model: modelFrom(properties),
            totalTokens: null,
            notificationType: "permission-request",
            reason: type,
            targetToolName: toolNameFrom(properties),
            message: messageFrom(properties),
          })
          return
        }
        case "permission.replied": {
          dispatchAIHook({
            kind: "promptSubmitted",
            externalSessionID: sessionIDFrom(properties),
            model: modelFrom(properties),
            totalTokens: null,
            reason: type,
          })
          return
        }
        default:
          return
      }
    },
  }
}
