import fs from "node:fs"
import { spawnSync } from "node:child_process"

const socketPath = process.env.DMUX_RUNTIME_SOCKET ?? ""
const logFile = process.env.DMUX_LOG_FILE ?? ""

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

function basePayload({ externalSessionID, responseState, model }) {
  const phase = "running"
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
    startedAt: Number(process.env.DMUX_ACTIVE_AI_STARTED_AT ?? currentTime),
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

function sessionIDFrom(properties) {
  return pickFirstString(properties, ["sessionID", "sessionId"])
    ?? pickFirstString(properties, ["id"])
}

function messageRoleFrom(properties) {
  return pickFirstString(properties, ["role"])
}

function modelFrom(properties) {
  return pickFirstString(properties, ["modelID", "modelId", "model"])
}

function messageIDFrom(properties) {
  return pickFirstString(properties, ["messageID", "messageId", "id"])
}

function commandNameFrom(properties) {
  return pickFirstString(properties, ["command", "name"])
}

let currentSessionID = readString(process.env.DMUX_EXTERNAL_SESSION_ID)
let currentModel = null
let lastUserMessageID = null
let lastSignature = null

function dispatchUpdate({ externalSessionID, responseState, model, reason }) {
  const nextExternalSessionID = readString(externalSessionID) ?? currentSessionID
  if (!nextExternalSessionID) {
    log("skip-dispatch-missing-session", { reason })
    return
  }
  currentSessionID = nextExternalSessionID
  currentModel = readString(model) ?? currentModel
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
    }),
  )
  log("dispatch", { reason, externalSessionID: nextExternalSessionID, responseState: responseState ?? null, model: currentModel })
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

  return {
    event: async ({ event }) => {
      const type = event?.type
      const properties = readObject(event?.properties) ?? {}

      switch (type) {
        case "session.created":
        case "session.updated": {
          dispatchUpdate({
            externalSessionID: sessionIDFrom(properties),
            responseState: null,
            model: modelFrom(properties),
            reason: type,
          })
          return
        }
        case "session.status": {
          const sessionID = sessionIDFrom(properties)
          const status = readObject(properties.status)
          const statusType = readString(status?.type)
          if (statusType === "busy" || statusType === "retry") {
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
              reason: "status:idle",
            })
          }
          return
        }
        case "session.idle": {
          dispatchUpdate({
            externalSessionID: sessionIDFrom(properties),
            responseState: "idle",
            model: modelFrom(properties),
            reason: type,
          })
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
            dispatchUpdate({
              externalSessionID: sessionID,
              responseState: "responding",
              model,
              reason: "message:user",
            })
            return
          }
          if (model) {
            currentModel = model
          }
          return
        }
        case "tui.command.execute": {
          const commandName = commandNameFrom(properties)
          if (commandName === "sessions" || commandName === "resume" || commandName === "continue") {
            dispatchUpdate({
              externalSessionID: sessionIDFrom(properties),
              responseState: null,
              model: modelFrom(properties),
              reason: `command:${commandName}`,
            })
          }
          return
        }
        default:
          return
      }
    },
  }
}
