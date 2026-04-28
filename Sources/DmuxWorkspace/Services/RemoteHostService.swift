import AppKit
import Combine
import CryptoKit
import Foundation

extension Notification.Name {
  static let coduxTerminalOutputDidReceive = Notification.Name("codux.terminalOutputDidReceive")
}

enum RemoteHostStatus: String, Codable, Equatable {
  case stopped
  case registering
  case connecting
  case connected
  case failed
}

struct RemoteHostDevice: Codable, Equatable, Identifiable {
  var id: String
  var hostId: String
  var name: String
  var publicKey: String
  var createdAt: Date
  var lastSeen: Date
  var revokedAt: Date?
  var online: Bool?
}

struct RemotePairingInfo: Codable, Equatable {
  var pairingId: String
  var code: String
  var secret: String
  var hostPublicKey: String?
  var cryptoVersion: Int?
  var expiresAt: Date
  var qrPayload: String
}

@MainActor
final class RemoteHostService: ObservableObject {
  struct PendingPairing: Equatable, Identifiable {
    var id: String
    var deviceName: String
    var devicePublicKey: String
    var code: String
  }

  struct Snapshot: Equatable {
    var status: RemoteHostStatus = .stopped
    var message: String = String(
      localized: "remote.status.stopped", defaultValue: "Remote Host stopped.", bundle: .module)
    var pairing: RemotePairingInfo?
    var devices: [RemoteHostDevice] = []
    var pendingPairings: [PendingPairing] = []
  }

  private weak var model: AppModel?
  private let logger = AppDebugLog.shared
  private let terminalEnvironmentService = AIRuntimeBridgeService()
  private var socket: URLSessionWebSocketTask?
  private var activeSocketURL: URL?
  private var isStarting = false
  private var outputObserver: NSObjectProtocol?
  private var pingTimer: Timer?
  private var reconnectTimer: Timer?
  private var reconnectAttempt = 0
  private var pairingPollTask: Task<Void, Never>?
  private var terminalOutputBuffer: [String: String] = [:]
  private var remoteSessions: [UUID: TerminalSession] = [:]
  private var remoteSessionMetadata: [UUID: RemoteTerminalMetadata] = [:]
  private var remoteProcessBridges: [UUID: GhosttyPTYProcessBridge] = [:]
  private var sendSeqByDevice: [String: Int64] = [:]
  private var receiveSeqByDevice: [String: Int64] = [:]
  private var e2eKeyCache: [String: SymmetricKey] = [:]
  private let maxTerminalBufferCharacters = 2_000_000
  private let metadataDateFormatter: ISO8601DateFormatter = {
    let formatter = ISO8601DateFormatter()
    formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
    return formatter
  }()
  private let encoder = JSONEncoder()
  private let decoder = JSONDecoder()
  @Published private(set) var snapshot = Snapshot()

  init(model: AppModel) {
    self.model = model
    snapshot.devices = model.appSettings.remote.displayCachedDevices
    encoder.dateEncodingStrategy = .iso8601
    decoder.dateDecodingStrategy = .custom { decoder in
      let container = try decoder.singleValueContainer()
      let value = try container.decode(String.self)
      let formatter = ISO8601DateFormatter()
      formatter.formatOptions = [.withInternetDateTime, .withFractionalSeconds]
      if let date = formatter.date(from: value) {
        return date
      }
      formatter.formatOptions = [.withInternetDateTime]
      if let date = formatter.date(from: value) {
        return date
      }
      throw DecodingError.dataCorruptedError(
        in: container, debugDescription: "Invalid ISO8601 date: \(value)")
    }
  }

  func applySettings() {
    guard let model else { return }
    let serverURL = model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines)
    if model.appSettings.remote.isEnabled, serverURL.isEmpty == false {
      start()
    } else {
      stop()
    }
  }

  func start() {
    guard let model else { return }
    guard model.appSettings.remote.isEnabled else {
      stop()
      return
    }
    guard
      !model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else {
      stop()
      return
    }
    guard isStarting == false else { return }
    stopReconnectTimer()
    if socket != nil, snapshot.status == .connected {
      return
    }
    Task { await registerHostAndConnect() }
  }

  func stop() {
    stopPingTimer()
    stopReconnectTimer()
    socket?.cancel(with: .normalClosure, reason: nil)
    socket = nil
    activeSocketURL = nil
    isStarting = false
    stopPairingPoll()
    snapshot.pairing = nil
    snapshot.pendingPairings.removeAll()
    sendSeqByDevice.removeAll()
    receiveSeqByDevice.removeAll()
    e2eKeyCache.removeAll()
    update(
      status: .stopped,
      message: String(
        localized: "remote.status.stopped", defaultValue: "Remote Host stopped.",
        bundle: .module))
  }

  func createPairing() {
    Task { await requestPairing() }
  }

  func refreshDevices() {
    Task { await loadDevices() }
  }

  func confirmPairing(_ pairingID: String) {
    Task { await confirm(pairingID: pairingID) }
  }

  func rejectPairing(_ pairingID: String) {
    Task { await reject(pairingID: pairingID) }
  }

  func cancelPairing() {
    Task { await cancelActivePairing() }
  }

  func revokeDevice(_ deviceID: String) {
    Task { await revoke(deviceID: deviceID) }
  }

  private func registerHostAndConnect() async {
    guard let model else { return }
    guard isStarting == false else { return }
    isStarting = true
    update(
      status: .registering,
      message: String(
        localized: "remote.status.registering", defaultValue: "Registering Remote Host…",
        bundle: .module))
    var settings = model.appSettings.remote
    if settings.hostID.isEmpty { settings.hostID = UUID().uuidString }
    if settings.hostToken.isEmpty { settings.hostToken = randomToken() }
    do {
      let identity = try RemoteE2ECrypto.ensureHostIdentity(
        privateKey: settings.hostPrivateKey,
        publicKey: settings.hostPublicKey
      )
      settings.hostPrivateKey = identity.privateKey
      settings.hostPublicKey = identity.publicKey
    } catch {
      update(status: .failed, message: remoteErrorMessage(error))
      isStarting = false
      return
    }
    let body: [String: String] = [
      "hostId": settings.hostID, "name": Host.current().localizedName ?? "Codux Mac",
      "token": settings.hostToken, "publicKey": settings.hostPublicKey,
    ]
    do {
      struct RegisterResponse: Decodable {
        var hostId: String
        var token: String
      }
      let response: RegisterResponse = try await post(path: "/api/hosts/register", body: body)
      settings.hostID = response.hostId
      settings.hostToken = response.token
      model.updateRemoteSettings(settings, reconnect: false)
      connectSocket()
      await loadDevices()
    } catch {
      update(status: .failed, message: remoteErrorMessage(error))
      isStarting = false
      scheduleReconnect(reason: error.localizedDescription)
      return
    }
    isStarting = false
  }

  private func connectSocket() {
    guard let model,
      let url = remoteURL(
        path: "/ws/host",
        queryItems: [
          URLQueryItem(name: "hostId", value: model.appSettings.remote.hostID),
          URLQueryItem(name: "token", value: model.appSettings.remote.hostToken),
        ], websocket: true)
    else { return }
    if socket != nil, activeSocketURL == url, snapshot.status == .connected {
      return
    }
    update(
      status: .connecting,
      message: String(
        localized: "remote.status.connecting", defaultValue: "Connecting relay…",
        bundle: .module))
    socket?.cancel(with: .goingAway, reason: nil)
    activeSocketURL = url
    let task = URLSession.shared.webSocketTask(with: url)
    socket = task
    task.resume()
    update(
      status: .connected,
      message: String(
        localized: "remote.status.connected", defaultValue: "Remote Host connected.",
        bundle: .module))
    reconnectAttempt = 0
    startPingTimer()
    receiveLoop()
  }

  private func receiveLoop() {
    socket?.receive { [weak self] result in
      Task { @MainActor in
        guard let self else { return }
        switch result {
        case .success(let message):
          self.handle(message)
          self.receiveLoop()
        case .failure(let error):
          if self.socket != nil {
            self.stopPingTimer()
            self.socket = nil
            self.activeSocketURL = nil
            self.update(status: .failed, message: error.localizedDescription)
            self.scheduleReconnect(reason: error.localizedDescription)
          }
        }
      }
    }
  }

  private func startPingTimer() {
    stopPingTimer()
    pingTimer = Timer.scheduledTimer(withTimeInterval: 25, repeats: true) { [weak self] _ in
      Task { @MainActor in
        guard let self, let socket = self.socket else { return }
        socket.sendPing { [weak self] error in
          guard let error else { return }
          Task { @MainActor in
            guard let self, self.socket === socket else { return }
            self.stopPingTimer()
            self.socket = nil
            self.activeSocketURL = nil
            self.update(
              status: .failed,
              message: String(
                format: String(
                  localized: "remote.status.ping_failed_format",
                  defaultValue: "Remote ping failed: %@", bundle: .module),
                error.localizedDescription))
            socket.cancel(with: .goingAway, reason: nil)
            self.scheduleReconnect(reason: error.localizedDescription)
          }
        }
      }
    }
    if let pingTimer { RunLoop.main.add(pingTimer, forMode: .common) }
  }

  private func stopPingTimer() {
    pingTimer?.invalidate()
    pingTimer = nil
  }

  private func scheduleReconnect(reason: String) {
    guard let model else { return }
    guard model.appSettings.remote.isEnabled else { return }
    guard
      !model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    else { return }
    guard reconnectTimer == nil, isStarting == false else { return }
    reconnectAttempt += 1
    let delay = min(30, max(1, 1 << min(reconnectAttempt - 1, 5)))
    update(
      status: .failed,
      message: String(
        format: String(
          localized: "remote.status.reconnecting_format",
          defaultValue: "Remote disconnected. Reconnecting in %d seconds… %@",
          bundle: .module),
        delay,
        reason))
    reconnectTimer = Timer.scheduledTimer(withTimeInterval: TimeInterval(delay), repeats: false) {
      [weak self] _ in
      Task { @MainActor in
        guard let self else { return }
        self.reconnectTimer = nil
        self.start()
      }
    }
    if let reconnectTimer { RunLoop.main.add(reconnectTimer, forMode: .common) }
  }

  private func stopReconnectTimer() {
    reconnectTimer?.invalidate()
    reconnectTimer = nil
  }

  private func handle(_ message: URLSessionWebSocketTask.Message) {
    let data: Data?
    switch message {
    case .data(let value): data = value
    case .string(let value): data = value.data(using: .utf8)
    @unknown default: data = nil
    }
    guard let data, let rawEnvelope = try? decoder.decode(RemoteEnvelope.self, from: data) else {
      return
    }
    let envelope = decryptEnvelopeIfNeeded(rawEnvelope) ?? rawEnvelope
    switch envelope.type {
    case "pairing.request":
      let pairingID = envelope.payload?["pairingId"] as? String ?? ""
      let pairingCode =
        envelope.payload?["code"] as? String
        ?? (snapshot.pairing?.pairingId == pairingID ? snapshot.pairing?.code : nil) ?? ""
      let deviceName = envelope.payload?["deviceName"] as? String ?? "Mobile Device"
      let devicePublicKey = envelope.payload?["devicePublicKey"] as? String ?? ""
      let pairingSecret = snapshot.pairing?.pairingId == pairingID ? snapshot.pairing?.secret : nil
      showPendingPairing(
        pairingID: pairingID,
        deviceName: deviceName,
        devicePublicKey: devicePublicKey,
        pairingCode: pairingCode,
        pairingSecret: pairingSecret
      )
    case "host.info":
      send(
        type: "host.info", deviceID: envelope.deviceID,
        payload: ["name": Host.current().localizedName ?? "Codux Mac"])
    case "device.info":
      refreshDevices()
    case "device.connected":
      updateDeviceOnline(envelope.deviceID ?? envelope.payload?["deviceId"] as? String, online: true)
      refreshDevices()
    case "device.disconnected":
      updateDeviceOnline(envelope.deviceID ?? envelope.payload?["deviceId"] as? String, online: false)
    case "project.list":
      send(
        type: "project.list", deviceID: envelope.deviceID,
        payload: ["projects": model?.remoteProjects() ?? []])
    case "terminal.list":
      send(
        type: "terminal.list", deviceID: envelope.deviceID,
        payload: ["terminals": remoteTerminals()])
    case "file.list":
      let requestedPath = envelope.payload?["path"] as? String
      let purpose = envelope.payload?["purpose"] as? String
      send(
        type: "file.list", deviceID: envelope.deviceID,
        payload: remoteFileList(path: requestedPath, purpose: purpose)
      )
    case "file.read":
      if let path = envelope.payload?["path"] as? String {
        do {
          send(
            type: "file.read", deviceID: envelope.deviceID, payload: try remoteFileRead(path: path))
        } catch {
          send(
            type: "error", deviceID: envelope.deviceID,
            payload: ["message": error.localizedDescription])
        }
      }
    case "file.write":
      if let path = envelope.payload?["path"] as? String,
        let content = envelope.payload?["content"] as? String
      {
        do {
          try remoteFileWrite(path: path, content: content)
          send(type: "file.written", deviceID: envelope.deviceID, payload: ["path": path])
        } catch {
          send(
            type: "error", deviceID: envelope.deviceID,
            payload: ["message": error.localizedDescription])
        }
      }
    case "file.rename":
      if let path = envelope.payload?["path"] as? String,
        let newPath = envelope.payload?["newPath"] as? String
      {
        do {
          try remoteFileRename(path: path, newPath: newPath)
          send(
            type: "file.renamed", deviceID: envelope.deviceID,
            payload: ["path": path, "newPath": newPath])
        } catch {
          send(
            type: "error", deviceID: envelope.deviceID,
            payload: ["message": error.localizedDescription])
        }
      }
    case "file.delete":
      if let path = envelope.payload?["path"] as? String {
        do {
          try remoteFileDelete(path: path)
          send(type: "file.deleted", deviceID: envelope.deviceID, payload: ["path": path])
        } catch {
          send(
            type: "error", deviceID: envelope.deviceID,
            payload: ["message": error.localizedDescription])
        }
      }
    case "project.add":
      if let path = envelope.payload?["path"] as? String,
        let project = model?.remoteAddProject(
          path: path, name: envelope.payload?["name"] as? String)
      {
        send(
          type: "project.updated", deviceID: envelope.deviceID,
          payload: ["action": "add", "projectId": project.id.uuidString])
        send(
          type: "project.list", deviceID: envelope.deviceID,
          payload: ["projects": model?.remoteProjects() ?? []])
        send(
          type: "terminal.list", deviceID: envelope.deviceID,
          payload: ["terminals": remoteTerminals()])
      } else {
        send(
          type: "error", deviceID: envelope.deviceID, payload: ["message": "Unable to add project"])
      }
    case "project.edit":
      if let projectID = envelope.payload?["projectId"] as? String,
        let path = envelope.payload?["path"] as? String,
        let project = model?.remoteEditProject(
          projectID: projectID, path: path, name: envelope.payload?["name"] as? String)
      {
        send(
          type: "project.updated", deviceID: envelope.deviceID,
          payload: ["action": "edit", "projectId": project.id.uuidString])
        send(
          type: "project.list", deviceID: envelope.deviceID,
          payload: ["projects": model?.remoteProjects() ?? []])
        send(
          type: "terminal.list", deviceID: envelope.deviceID,
          payload: ["terminals": remoteTerminals()])
      } else {
        send(
          type: "error", deviceID: envelope.deviceID, payload: ["message": "Unable to edit project"]
        )
      }
    case "project.remove":
      if let projectID = envelope.payload?["projectId"] as? String,
        model?.remoteRemoveProject(projectID: projectID) == true
      {
        send(
          type: "project.updated", deviceID: envelope.deviceID,
          payload: ["action": "remove", "projectId": projectID])
        send(
          type: "project.list", deviceID: envelope.deviceID,
          payload: ["projects": model?.remoteProjects() ?? []])
        send(
          type: "terminal.list", deviceID: envelope.deviceID,
          payload: ["terminals": remoteTerminals()])
      } else {
        send(
          type: "error", deviceID: envelope.deviceID,
          payload: ["message": "Unable to remove project"])
      }
    case "ai.stats":
      if let projectID = envelope.payload?["projectId"] as? String,
        let stats = model?.remoteAIStats(projectID: projectID)
      {
        send(type: "ai.stats", deviceID: envelope.deviceID, payload: stats)
      } else {
        send(
          type: "error", deviceID: envelope.deviceID,
          payload: ["message": "Unable to load AI stats"])
      }
    case "terminal.buffer":
      if let sessionID = envelope.sessionID {
        sendTerminalBuffer(sessionID: sessionID, deviceID: envelope.deviceID)
      }
    case "terminal.create":
      let projectID = envelope.payload?["projectId"] as? String
      let command = envelope.payload?["command"] as? String ?? ""
      if let session = model?.remoteCreateTerminal(projectID: projectID, command: command) {
        remoteSessions[session.id] = session
        let sessionID = session.id.uuidString
        let now = Date()
        remoteSessionMetadata[session.id] = RemoteTerminalMetadata(
          ownerDeviceID: envelope.deviceID,
          ownerKind: "mobile",
          kind: "remote-mobile",
          resizeOwner: "mobile",
          columns: nil,
          rows: nil,
          createdAt: now,
          lastActiveAt: now,
          status: "starting"
        )
        logger.log(
          "remote-terminal",
          "created session=\(sessionID) project=\(session.projectID.uuidString) shell=\(session.shell) command=\(session.command) cwd=\(session.cwd)"
        )
        send(
          type: "terminal.created", deviceID: envelope.deviceID, sessionID: sessionID,
          payload: remoteTerminalPayload(for: session)
        )
        startRemoteProcessIfNeeded(for: session, deviceID: envelope.deviceID)
        sendTerminalBuffer(sessionID: sessionID, deviceID: envelope.deviceID)
      } else {
        send(
          type: "error", deviceID: envelope.deviceID,
          payload: ["message": "Unable to create terminal"])
      }
    case "terminal.input":
      if let sessionID = envelope.sessionID.flatMap(UUID.init(uuidString:)),
        let data = envelope.payload?["data"] as? String
      {
        logger.log(
          "remote-terminal",
          "input session=\(sessionID.uuidString) bytes=\(data.utf8.count) bridge=\(remoteProcessBridges[sessionID] != nil)"
        )
        updateRemoteTerminalActivity(
          sessionID: sessionID,
          status: remoteProcessBridges[sessionID] == nil ? "starting" : "running",
          ownerDeviceID: envelope.deviceID
        )
        ensureRemoteProcessBridge(for: sessionID, deviceID: envelope.deviceID)
        if let bridge = remoteProcessBridges[sessionID] {
          bridge.sendText(data)
        } else {
          let sent = DmuxTerminalBackend.shared.registry.sendText(data, to: sessionID)
          if sent == false {
            send(
              type: "error", deviceID: envelope.deviceID, sessionID: sessionID.uuidString,
              payload: [
                "message":
                  "Terminal is not ready yet. Please create it again or select a running terminal."
              ])
          }
        }
      }
    case "terminal.resize":
      if let sessionID = envelope.sessionID.flatMap(UUID.init(uuidString:)),
        let colsValue = envelope.payload?["cols"],
        let rowsValue = envelope.payload?["rows"]
      {
        let cols = UInt16(clamping: UInt((colsValue as? Int) ?? Int((colsValue as? Double) ?? 0)))
        let rows = UInt16(clamping: UInt((rowsValue as? Int) ?? Int((rowsValue as? Double) ?? 0)))
        if cols > 0, rows > 0 {
          upsertRemoteTerminalMetadata(
            sessionID: sessionID,
            ownerDeviceID: envelope.deviceID,
            columns: Int(cols),
            rows: Int(rows)
          )
          logger.log(
            "remote-terminal",
            "resize session=\(sessionID.uuidString) cols=\(cols) rows=\(rows) bridge=\(remoteProcessBridges[sessionID] != nil)"
          )
          ensureRemoteProcessBridge(for: sessionID, deviceID: envelope.deviceID)
          if let bridge = remoteProcessBridges[sessionID] {
            bridge.resize(columns: cols, rows: rows)
          } else {
            _ = DmuxTerminalBackend.shared.registry.resize(
              columns: cols, rows: rows, sessionID: sessionID)
          }
        }
      }
    case "terminal.upload":
      handleTerminalUpload(envelope)
    case "terminal.close":
      if let sessionID = envelope.sessionID.flatMap(UUID.init(uuidString:)) {
        if let bridge = remoteProcessBridges.removeValue(forKey: sessionID) {
          bridge.terminateProcessTree()
        }
        remoteSessions.removeValue(forKey: sessionID)
        remoteSessionMetadata.removeValue(forKey: sessionID)
        terminalOutputBuffer.removeValue(forKey: sessionID.uuidString)
        send(
          type: "terminal.closed", deviceID: envelope.deviceID, sessionID: sessionID.uuidString,
          payload: ["id": sessionID.uuidString])
        send(
          type: "terminal.list", deviceID: envelope.deviceID,
          payload: ["terminals": remoteTerminals()])
      }
    case "terminal.signal":
      if let sessionID = envelope.sessionID.flatMap(UUID.init(uuidString:)) {
        let signal = envelope.payload?["signal"] as? String
        if signal == "interrupt" {
          _ = DmuxTerminalBackend.shared.registry.sendInterrupt(to: sessionID)
        }
        if signal == "escape" { _ = DmuxTerminalBackend.shared.registry.sendEscape(to: sessionID) }
      }
    default:
      break
    }
  }

  private func remoteFileList(path: String?, purpose: String?) -> [String: Any] {
    let fileManager = FileManager.default
    let home = fileManager.homeDirectoryForCurrentUser.path
    let resolvedPath =
      (path?.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty == false ? path! : home)
    let url = URL(fileURLWithPath: resolvedPath, isDirectory: true).standardizedFileURL
    let isDirectory = (try? url.resourceValues(forKeys: [.isDirectoryKey]).isDirectory) == true
    let directoryURL = isDirectory ? url : url.deletingLastPathComponent()
    let keys: Set<URLResourceKey> = [.isDirectoryKey, .isHiddenKey, .nameKey]
    let entries =
      (try? fileManager.contentsOfDirectory(
        at: directoryURL, includingPropertiesForKeys: Array(keys),
        options: [.skipsPackageDescendants])) ?? []
    let mapped = entries.compactMap { entry -> [String: Any]? in
      guard let values = try? entry.resourceValues(forKeys: keys) else { return nil }
      if values.isHidden == true { return nil }
      let name = values.name ?? entry.lastPathComponent
      return [
        "name": name,
        "path": entry.path,
        "isDirectory": values.isDirectory == true,
      ]
    }
    .sorted { lhs, rhs in
      let lhsDirectory = lhs["isDirectory"] as? Bool == true
      let rhsDirectory = rhs["isDirectory"] as? Bool == true
      if lhsDirectory != rhsDirectory { return lhsDirectory && !rhsDirectory }
      return ((lhs["name"] as? String) ?? "").localizedStandardCompare(
        (rhs["name"] as? String) ?? "") == .orderedAscending
    }
    var payload: [String: Any] = [
      "path": directoryURL.path,
      "parent": directoryURL.deletingLastPathComponent().path,
      "entries": mapped,
    ]
    if let purpose { payload["purpose"] = purpose }
    return payload
  }

  private func remoteFileRead(path: String) throws -> [String: Any] {
    let url = URL(fileURLWithPath: path).standardizedFileURL
    let values = try url.resourceValues(forKeys: [.isDirectoryKey, .fileSizeKey])
    if values.isDirectory == true {
      throw NSError(
        domain: "CoduxRemote", code: 400,
        userInfo: [NSLocalizedDescriptionKey: "Cannot open a directory as a file"])
    }
    let maxBytes = 2 * 1024 * 1024
    if let fileSize = values.fileSize, fileSize > maxBytes {
      throw NSError(
        domain: "CoduxRemote", code: 413,
        userInfo: [
          NSLocalizedDescriptionKey: "File is larger than 2MB and cannot be opened on mobile yet"
        ])
    }
    let data = try Data(contentsOf: url)
    guard let content = String(data: data, encoding: .utf8) else {
      throw NSError(
        domain: "CoduxRemote", code: 415,
        userInfo: [NSLocalizedDescriptionKey: "Only UTF-8 text files can be edited on mobile"])
    }
    return ["path": url.path, "name": url.lastPathComponent, "content": content, "size": data.count]
  }

  private func remoteFileWrite(path: String, content: String) throws {
    let url = URL(fileURLWithPath: path).standardizedFileURL
    try content.data(using: .utf8)?.write(to: url, options: .atomic)
  }

  private func remoteFileRename(path: String, newPath: String) throws {
    let sourceURL = URL(fileURLWithPath: path).standardizedFileURL
    let destinationURL = URL(fileURLWithPath: newPath).standardizedFileURL
    guard sourceURL.deletingLastPathComponent().path == destinationURL.deletingLastPathComponent().path
    else {
      throw NSError(
        domain: "CoduxRemote", code: 400,
        userInfo: [NSLocalizedDescriptionKey: "Rename must stay in the same directory"])
    }
    if FileManager.default.fileExists(atPath: destinationURL.path) {
      throw NSError(
        domain: "CoduxRemote", code: 409,
        userInfo: [NSLocalizedDescriptionKey: "A file with this name already exists"])
    }
    try FileManager.default.moveItem(at: sourceURL, to: destinationURL)
  }

  private func remoteFileDelete(path: String) throws {
    let url = URL(fileURLWithPath: path).standardizedFileURL
    try FileManager.default.removeItem(at: url)
  }

  private func handleTerminalUpload(_ envelope: RemoteEnvelope) {
    guard let sessionID = envelope.sessionID,
      let payload = envelope.payload,
      let base64Value = payload["data"] as? String,
      let fileData = Data(base64Encoded: base64Value)
    else {
      send(
        type: "error", deviceID: envelope.deviceID, sessionID: envelope.sessionID,
        payload: ["message": "Invalid upload payload"])
      return
    }
    let rawName = (payload["name"] as? String) ?? "upload.png"
    let safeName = sanitizedUploadFileName(rawName)
    let directory = FileManager.default.temporaryDirectory
      .appendingPathComponent("CoduxUploads", isDirectory: true)
      .appendingPathComponent(sessionID, isDirectory: true)
    do {
      try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
      let url = uniqueUploadURL(directory: directory, fileName: safeName)
      try fileData.write(to: url, options: .atomic)
      let insertion = insertUploadedImage(
        url: url, data: fileData, mime: payload["mime"] as? String, sessionID: sessionID)
      send(
        type: "terminal.uploaded", deviceID: envelope.deviceID, sessionID: sessionID,
        payload: [
          "path": url.path, "name": url.lastPathComponent, "mode": insertion.mode,
          "tool": insertion.tool as Any, "inserted": true,
        ])
    } catch {
      send(
        type: "error", deviceID: envelope.deviceID, sessionID: sessionID,
        payload: ["message": "Upload failed: \(error.localizedDescription)"])
    }
  }

  private func insertUploadedImage(url: URL, data: Data, mime: String?, sessionID: String) -> (
    mode: String, tool: String?
  ) {
    let uuid = UUID(uuidString: sessionID)
    if let uuid {
      ensureRemoteProcessBridge(for: uuid, deviceID: nil)
    }
    let tool = uuid.flatMap { activeAITool(for: $0) }
    if let uuid, let bridge = remoteProcessBridges[uuid], let tool,
      supportsClipboardImagePaste(tool)
    {
      prepareImagePasteboard(url: url, data: data, mime: mime)
      bridge.sendText("\u{16}")
      return ("clipboard", tool)
    }
    let text = "\(url.path) "
    if let uuid, let bridge = remoteProcessBridges[uuid] {
      bridge.sendText(text)
    } else if let uuid {
      _ = DmuxTerminalBackend.shared.registry.sendText(text, to: uuid)
    }
    return ("path", tool)
  }

  private func activeAITool(for sessionID: UUID) -> String? {
    if let shellPID = remoteProcessBridges[sessionID]?.currentShellPID,
      let tool = TerminalProcessInspector().activeTool(forShellPID: shellPID)
    {
      return tool
    }
    guard let command = remoteSessions[sessionID]?.command.lowercased() else {
      return nil
    }
    if command.contains("claude") { return "claude" }
    if command.contains("codex") { return "codex" }
    if command.contains("gemini") { return "gemini" }
    if command.contains("opencode") { return "opencode" }
    return nil
  }

  private func supportsClipboardImagePaste(_ tool: String) -> Bool {
    ["claude", "codex", "gemini", "opencode"].contains(tool.lowercased())
  }

  private func prepareImagePasteboard(url: URL, data: Data, mime: String?) {
    let pasteboard = NSPasteboard.general
    pasteboard.clearContents()
    pasteboard.setString(url.absoluteString, forType: .fileURL)
    let normalizedMime = mime?.lowercased() ?? ""
    if normalizedMime.contains("png") {
      pasteboard.setData(data, forType: .png)
    } else if normalizedMime.contains("jpeg") || normalizedMime.contains("jpg") {
      pasteboard.setData(data, forType: NSPasteboard.PasteboardType("public.jpeg"))
    }
    if let image = NSImage(data: data) {
      pasteboard.writeObjects([image])
      if let tiff = image.tiffRepresentation {
        pasteboard.setData(tiff, forType: .tiff)
      }
    }
  }

  private func sanitizedUploadFileName(_ value: String) -> String {
    let fallback = "upload.png"
    let last = URL(fileURLWithPath: value).lastPathComponent
    let cleaned = last.map { char -> Character in
      if char.isLetter || char.isNumber || char == "." || char == "-" || char == "_" { return char }
      return "_"
    }
    let result = String(cleaned).trimmingCharacters(in: CharacterSet(charactersIn: "."))
    return result.isEmpty ? fallback : result
  }

  private func uniqueUploadURL(directory: URL, fileName: String) -> URL {
    let base = URL(fileURLWithPath: fileName).deletingPathExtension().lastPathComponent
    let ext = URL(fileURLWithPath: fileName).pathExtension
    var candidate = directory.appendingPathComponent(fileName)
    var index = 1
    while FileManager.default.fileExists(atPath: candidate.path) {
      let nextName = ext.isEmpty ? "\(base)-\(index)" : "\(base)-\(index).\(ext)"
      candidate = directory.appendingPathComponent(nextName)
      index += 1
    }
    return candidate
  }

  private func startRemoteProcessIfNeeded(for session: TerminalSession, deviceID: String?) {
    guard remoteProcessBridges[session.id] == nil else { return }
    let bridge = GhosttyPTYProcessBridge(sessionID: session.id, suppressPromptEolMark: true)
    remoteProcessBridges[session.id] = bridge
    updateRemoteTerminalActivity(
      sessionID: session.id,
      status: "running",
      ownerDeviceID: deviceID
    )
    let sessionID = session.id.uuidString
    bridge.onOutput = { [weak self] data in
      Task { @MainActor in
        guard let self, let text = String(data: data, encoding: .utf8) else { return }
        self.appendBuffer(sessionID: sessionID, text: text)
        self.updateRemoteTerminalActivity(
          sessionID: session.id,
          status: "running",
          ownerDeviceID: deviceID
        )
        self.logger.log(
          "remote-terminal",
          "output session=\(sessionID) bytes=\(data.count)"
        )
        self.send(
          type: "terminal.output", deviceID: deviceID, sessionID: sessionID, payload: ["data": text]
        )
      }
    }
    bridge.onProcessTerminated = { [weak self] exitCode in
      Task { @MainActor in
        guard let self else { return }
        self.remoteProcessBridges[session.id] = nil
        self.updateRemoteTerminalActivity(
          sessionID: session.id,
          status: "exited",
          ownerDeviceID: deviceID
        )
      }
    }
    guard let model else { return }
    let environment = terminalEnvironmentService
      .environmentResolution(for: session, aiSettings: model.appSettings.ai)
      .pairs
    bridge.start(
      shell: session.shell,
      shellName: URL(fileURLWithPath: session.shell).lastPathComponent,
      command: session.command,
      cwd: session.cwd,
      environment: environment
    )
  }

  private func ensureRemoteProcessBridge(for sessionID: UUID, deviceID: String?) {
    guard remoteProcessBridges[sessionID] == nil else { return }
    guard let session = remoteSessions[sessionID] else { return }
    logger.log(
      "remote-terminal",
      "restore bridge session=\(sessionID.uuidString) project=\(session.projectID.uuidString)"
    )
    startRemoteProcessIfNeeded(for: session, deviceID: deviceID)
  }

  private func remoteTerminals() -> [[String: Any]] {
    remoteSessions.values.map { remoteTerminalPayload(for: $0) }
  }

  private func remoteTerminalPayload(for session: TerminalSession) -> [String: Any] {
    let metadata = remoteSessionMetadata[session.id]
    let ownerDeviceID = metadata?.ownerDeviceID ?? ""
    return [
      "id": session.id.uuidString,
      "title": session.title,
      "displayTitle": "\(session.projectName) · \(session.title)",
      "projectId": session.projectID.uuidString,
      "projectName": session.projectName,
      "projectPath": session.cwd,
      "cwd": session.cwd,
      "shell": session.shell,
      "command": session.command,
      "kind": metadata?.kind ?? "remote-mobile",
      "ownerKind": metadata?.ownerKind ?? "mobile",
      "ownerDeviceId": ownerDeviceID,
      "ownerDeviceName": remoteDeviceName(for: ownerDeviceID),
      "resizeOwner": metadata?.resizeOwner ?? "mobile",
      "cols": metadata?.columns ?? 0,
      "rows": metadata?.rows ?? 0,
      "gridSource": metadata?.resizeOwner ?? "mobile",
      "status": metadata?.status ?? (remoteProcessBridges[session.id] == nil ? "idle" : "running"),
      "isRunning": remoteProcessBridges[session.id] != nil,
      "createdAt": formattedMetadataDate(metadata?.createdAt),
      "lastActiveAt": formattedMetadataDate(metadata?.lastActiveAt),
      "bufferCharacters": terminalOutputBuffer[session.id.uuidString]?.count ?? 0,
      "hasBuffer": terminalOutputBuffer[session.id.uuidString]?.isEmpty == false,
    ]
  }

  private func remoteDeviceName(for deviceID: String) -> String {
    guard deviceID.isEmpty == false else { return "" }
    return snapshot.devices.first { $0.id == deviceID }?.name ?? ""
  }

  private func formattedMetadataDate(_ date: Date?) -> String {
    guard let date else { return "" }
    return metadataDateFormatter.string(from: date)
  }

  private func updateDeviceOnline(_ deviceID: String?, online: Bool) {
    guard let deviceID, deviceID.isEmpty == false,
      let index = snapshot.devices.firstIndex(where: { $0.id == deviceID })
    else { return }
    snapshot.devices[index].online = online
    if online {
      snapshot.devices[index].lastSeen = Date()
    }
  }

  private func upsertRemoteTerminalMetadata(
    sessionID: UUID,
    ownerDeviceID: String?,
    columns: Int,
    rows: Int
  ) {
    let current = remoteSessionMetadata[sessionID]
    remoteSessionMetadata[sessionID] = RemoteTerminalMetadata(
      ownerDeviceID: current?.ownerDeviceID ?? ownerDeviceID,
      ownerKind: current?.ownerKind ?? "mobile",
      kind: current?.kind ?? "remote-mobile",
      resizeOwner: current?.resizeOwner ?? "mobile",
      columns: columns,
      rows: rows,
      createdAt: current?.createdAt ?? Date(),
      lastActiveAt: Date(),
      status: current?.status ?? "running"
    )
  }

  private func updateRemoteTerminalActivity(
    sessionID: UUID,
    status: String,
    ownerDeviceID: String?
  ) {
    let current = remoteSessionMetadata[sessionID]
    remoteSessionMetadata[sessionID] = RemoteTerminalMetadata(
      ownerDeviceID: current?.ownerDeviceID ?? ownerDeviceID,
      ownerKind: current?.ownerKind ?? "mobile",
      kind: current?.kind ?? "remote-mobile",
      resizeOwner: current?.resizeOwner ?? "mobile",
      columns: current?.columns,
      rows: current?.rows,
      createdAt: current?.createdAt ?? Date(),
      lastActiveAt: Date(),
      status: status
    )
  }

  private func appendBuffer(sessionID: String, text: String) {
    guard text.isEmpty == false else { return }
    guard isTerminalQueryResponse(text) == false else { return }
    var current = terminalOutputBuffer[sessionID] ?? ""
    current += text
    if current.count > maxTerminalBufferCharacters {
      current = String(current.suffix(maxTerminalBufferCharacters))
    }
    terminalOutputBuffer[sessionID] = current
  }

  private func isTerminalQueryResponse(_ text: String) -> Bool {
    guard text.count <= 64 else { return false }
    let pattern = #"^(?:\u{001B}\[\??[0-9;]*[Rcn]|\u{001B}\][0-9;]*[^\u{0007}\u{001B}]*(?:\u{0007}|\u{001B}\\))+$"#
    return text.range(of: pattern, options: .regularExpression) != nil
  }

  private func sendTerminalBuffer(sessionID: String, deviceID: String?) {
    if let uuid = UUID(uuidString: sessionID) {
      ensureRemoteProcessBridge(for: uuid, deviceID: deviceID)
    }
    let data = terminalOutputBuffer[sessionID] ?? ""
    send(
      type: "terminal.output", deviceID: deviceID, sessionID: sessionID,
      payload: ["data": data, "buffer": true])
  }

  private func requestPairing() async {
    guard let model else { return }
    do {
      let response: RemotePairingInfo = try await post(
        path: "/api/pairings",
        body: [
          "hostId": model.appSettings.remote.hostID, "token": model.appSettings.remote.hostToken,
        ])
      let pairing = pairingInfoWithLocalQR(response)
      snapshot.pairing = pairing
      startPairingPoll(pairing)
      snapshot.message = String(
        format: String(
          localized: "remote.status.pairing_code_format",
          defaultValue: "Pairing code: %@", bundle: .module),
        response.code)
    } catch {
      update(status: .failed, message: remoteErrorMessage(error))
    }
  }

  private func loadDevices() async {
    guard let model else { return }
    guard !model.appSettings.remote.hostID.isEmpty, !model.appSettings.remote.hostToken.isEmpty
    else {
      snapshot.devices = []
      return
    }
    if snapshot.devices.isEmpty {
      snapshot.devices = model.appSettings.remote.displayCachedDevices
    }
    guard
      let url = remoteURL(
        path: "/api/hosts/\(model.appSettings.remote.hostID)/devices",
        queryItems: [URLQueryItem(name: "token", value: model.appSettings.remote.hostToken)],
        websocket: false)
    else { return }
    do {
      struct DeviceList: Decodable { var devices: [RemoteHostDevice] }
      let data = try await requestData(url: url)
      let list = try decoder.decode(DeviceList.self, from: data)
      let devices = list.devices.filter { $0.revokedAt == nil }
      snapshot.devices = devices
      cacheDevices(devices)
    } catch {
      if snapshot.devices.isEmpty {
        snapshot.devices = model.appSettings.remote.displayCachedDevices
      }
      snapshot.message = remoteErrorMessage(error)
    }
  }

  private func confirm(pairingID: String) async {
    guard let model else { return }
    do {
      let _: [String: String] = try await post(
        path: "/api/pairings/confirm",
        body: [
          "hostId": model.appSettings.remote.hostID, "token": model.appSettings.remote.hostToken,
          "pairingId": pairingID,
        ])
      snapshot.pendingPairings.removeAll { $0.id == pairingID }
      if snapshot.pairing?.pairingId == pairingID {
        snapshot.pairing = nil
      }
      stopPairingPoll()
      snapshot.message = String(
        localized: "remote.status.device_paired", defaultValue: "Device paired.", bundle: .module)
      await loadDevices()
    } catch {
      snapshot.message = remoteErrorMessage(error)
    }
  }

  private func reject(pairingID: String) async {
    guard let model else { return }
    do {
      let _: [String: Bool] = try await post(
        path: "/api/pairings/reject",
        body: [
          "hostId": model.appSettings.remote.hostID, "token": model.appSettings.remote.hostToken,
          "pairingId": pairingID,
        ])
      snapshot.pendingPairings.removeAll { $0.id == pairingID }
      if snapshot.pairing?.pairingId == pairingID {
        snapshot.pairing = nil
      }
      stopPairingPoll()
      snapshot.message = String(
        localized: "remote.status.pairing_rejected", defaultValue: "Pairing rejected.",
        bundle: .module)
    } catch {
      snapshot.message = remoteErrorMessage(error)
    }
  }

  private func revoke(deviceID: String) async {
    guard let model else { return }
    let previousDevices = snapshot.devices
    snapshot.devices.removeAll { $0.id == deviceID }
    removeCachedDevice(id: deviceID)
    do {
      let _: [String: Bool] = try await post(
        path: "/api/devices/revoke",
        body: [
          "hostId": model.appSettings.remote.hostID, "token": model.appSettings.remote.hostToken,
          "deviceId": deviceID,
        ])
      await loadDevices()
    } catch {
      if snapshot.devices.isEmpty {
        snapshot.devices = previousDevices.filter { $0.id != deviceID }
      }
      snapshot.message = remoteErrorMessage(error)
    }
  }

  private func cacheDevices(_ devices: [RemoteHostDevice]) {
    guard let model else { return }
    var remote = model.appSettings.remote
    remote.cacheDevices(devices)
    model.updateRemoteSettings(remote, reconnect: false)
  }

  private func removeCachedDevice(id deviceID: String) {
    guard let model else { return }
    var remote = model.appSettings.remote
    remote.removeCachedDevice(id: deviceID)
    model.updateRemoteSettings(remote, reconnect: false)
  }

  private func post<T: Decodable>(path: String, body: Any) async throws -> T {
    guard let url = remoteURL(path: path, queryItems: [], websocket: false) else {
      throw URLError(.badURL)
    }
    var request = URLRequest(url: url)
    request.httpMethod = "POST"
    request.setValue("application/json", forHTTPHeaderField: "Content-Type")
    request.httpBody = try JSONSerialization.data(withJSONObject: body)
    let (data, response) = try await URLSession.shared.data(for: request)
    try validateHTTPResponse(response, data: data)
    do {
      return try decoder.decode(T.self, from: data)
    } catch {
      throw NSError(
        domain: "CoduxRemote", code: -2,
        userInfo: [
          NSLocalizedDescriptionKey:
            "Remote response decode failed: \(error.localizedDescription). Body: \(String(data: data, encoding: .utf8) ?? "<binary>")"
        ])
    }
  }

  private func requestData(url: URL) async throws -> Data {
    let (data, response) = try await URLSession.shared.data(from: url)
    try validateHTTPResponse(response, data: data)
    return data
  }

  private func validateHTTPResponse(_ response: URLResponse, data: Data) throws {
    guard let http = response as? HTTPURLResponse else { return }
    guard (200..<300).contains(http.statusCode) else {
      let body = String(data: data, encoding: .utf8) ?? "Remote request failed"
      if let error = try? decoder.decode(RemoteErrorResponse.self, from: data) {
        throw NSError(
          domain: "CoduxRemote", code: http.statusCode,
          userInfo: [NSLocalizedDescriptionKey: error.error])
      }
      throw NSError(
        domain: "CoduxRemote", code: http.statusCode, userInfo: [NSLocalizedDescriptionKey: body])
    }
  }

  private func cancelActivePairing() async {
    guard let model, let pairing = snapshot.pairing else { return }
    stopPairingPoll()
    snapshot.pairing = nil
    snapshot.pendingPairings.removeAll { $0.id == pairing.pairingId }
    do {
      let _: [String: Bool] = try await post(
        path: "/api/pairings/reject",
        body: [
          "hostId": model.appSettings.remote.hostID,
          "token": model.appSettings.remote.hostToken,
          "pairingId": pairing.pairingId,
        ])
      snapshot.message = String(
        localized: "remote.status.pairing_cancelled",
        defaultValue: "Pairing cancelled.",
        bundle: .module)
    } catch {
      snapshot.message = remoteErrorMessage(error)
    }
  }

  private func startPairingPoll(_ pairing: RemotePairingInfo) {
    stopPairingPoll()
    pairingPollTask = Task { [weak self] in
      while !Task.isCancelled {
        guard let self else { return }
        await self.pollPairingStatus(pairing)
        guard self.snapshot.pairing?.pairingId == pairing.pairingId else { return }
        try? await Task.sleep(nanoseconds: 1_000_000_000)
      }
    }
  }

  private func stopPairingPoll() {
    pairingPollTask?.cancel()
    pairingPollTask = nil
  }

  private func pollPairingStatus(_ pairing: RemotePairingInfo) async {
    do {
      let status: RemotePairingStatusResponse = try await post(
        path: "/api/pairings/status",
        body: ["code": pairing.code, "secret": pairing.secret])
      switch status.status {
      case "claimed":
        showPendingPairing(
          pairingID: status.pairingId ?? pairing.pairingId,
          deviceName: status.deviceName ?? "Mobile Device",
          devicePublicKey: status.devicePublicKey ?? "",
          pairingCode: status.code ?? pairing.code,
          pairingSecret: pairing.secret
        )
      case "rejected", "confirmed":
        if snapshot.pairing?.pairingId == pairing.pairingId {
          snapshot.pairing = nil
        }
        stopPairingPoll()
      default:
        break
      }
    } catch {
      snapshot.message = remoteErrorMessage(error)
      if snapshot.pairing?.pairingId == pairing.pairingId {
        snapshot.pairing = nil
      }
      stopPairingPoll()
    }
  }

  private func showPendingPairing(
    pairingID: String,
    deviceName: String,
    devicePublicKey: String,
    pairingCode: String,
    pairingSecret: String?
  ) {
    guard !pairingID.isEmpty else { return }
    let displayedCode = remotePairingMatchCode(
      pairingCode: pairingCode,
      pairingSecret: pairingSecret,
      devicePublicKey: devicePublicKey
    ) ?? pairingCode
    if snapshot.pairing?.pairingId == pairingID {
      snapshot.pairing = nil
      stopPairingPoll()
    }
    if let index = snapshot.pendingPairings.firstIndex(where: { $0.id == pairingID }) {
      snapshot.pendingPairings[index] = PendingPairing(
        id: pairingID,
        deviceName: deviceName,
        devicePublicKey: devicePublicKey,
        code: displayedCode)
    } else {
      snapshot.pendingPairings.append(
        PendingPairing(
          id: pairingID,
          deviceName: deviceName,
          devicePublicKey: devicePublicKey,
          code: displayedCode))
    }
    snapshot.message = String(
      format: String(
        localized: "remote.status.pairing_request_format",
        defaultValue: "Pairing request from %@.", bundle: .module),
      deviceName)
  }

  private func remoteErrorMessage(_ error: Error) -> String {
    let nsError = error as NSError
    guard nsError.domain == NSURLErrorDomain else {
      return error.localizedDescription
    }
    let code = URLError.Code(rawValue: nsError.code)
    switch code {
    case .cannotConnectToHost, .cannotFindHost, .dnsLookupFailed:
      return String(
        localized: "remote.error.cannot_connect_server",
        defaultValue: "Cannot connect to the relay server.",
        bundle: .module)
    case .networkConnectionLost:
      return String(
        localized: "remote.error.connection_lost",
        defaultValue: "Relay connection lost.",
        bundle: .module)
    case .notConnectedToInternet:
      return String(
        localized: "remote.error.not_connected_internet",
        defaultValue: "Network is offline.",
        bundle: .module)
    case .timedOut:
      return String(
        localized: "remote.error.timed_out",
        defaultValue: "Relay request timed out.",
        bundle: .module)
    default:
      return error.localizedDescription
    }
  }

  private func decryptEnvelopeIfNeeded(_ envelope: RemoteEnvelope) -> RemoteEnvelope? {
    guard envelope.type == "secure.message" else { return envelope }
    guard let deviceID = envelope.deviceID, deviceID.isEmpty == false,
      let encryptedPayload = envelope.payload,
      let model
    else { return nil }
    guard let device = snapshot.devices.first(where: { $0.id == deviceID }),
      device.publicKey.isEmpty == false
    else {
      logger.log("remote-e2e", "drop encrypted message device=\(deviceID) reason=missing_device_key")
      refreshDevices()
      return nil
    }
    do {
      let key = try e2eSymmetricKey(for: device)
      let plaintext = try RemoteE2ECrypto.decrypt(
        encryptedPayload: encryptedPayload,
        key: key,
        hostID: model.appSettings.remote.hostID,
        deviceID: deviceID
      )
      let inner = try decoder.decode(RemoteEnvelope.self, from: plaintext)
      if let seq = inner.seq {
        let previous = receiveSeqByDevice[deviceID] ?? 0
        guard seq > previous else {
          logger.log("remote-e2e", "drop replay device=\(deviceID) seq=\(seq) previous=\(previous)")
          return nil
        }
        receiveSeqByDevice[deviceID] = seq
      }
      return inner.withDeviceID(deviceID)
    } catch {
      logger.log("remote-e2e", "decrypt failed device=\(deviceID) error=\(error.localizedDescription)")
      return nil
    }
  }

  private func encryptedOutgoingEnvelope(_ inner: RemoteOutgoingEnvelope) -> RemoteOutgoingEnvelope? {
    guard let deviceID = inner.deviceID, deviceID.isEmpty == false else {
      return inner
    }
    guard let model,
      let device = snapshot.devices.first(where: { $0.id == deviceID }),
      device.publicKey.isEmpty == false
    else {
      return nil
    }
    do {
      let nextSeq = (sendSeqByDevice[deviceID] ?? 0) + 1
      sendSeqByDevice[deviceID] = nextSeq
      var securedInner = inner
      securedInner.seq = nextSeq
      let plaintext = try JSONSerialization.data(withJSONObject: securedInner.dictionary)
      let key = try e2eSymmetricKey(for: device)
      let encryptedPayload = try RemoteE2ECrypto.encrypt(
        plaintext: plaintext,
        key: key,
        hostID: model.appSettings.remote.hostID,
        deviceID: deviceID
      )
      return RemoteOutgoingEnvelope(
        type: "secure.message",
        deviceID: deviceID,
        sessionID: inner.sessionID,
        payload: encryptedPayload
      )
    } catch {
      logger.log("remote-e2e", "encrypt failed device=\(deviceID) error=\(error.localizedDescription)")
      return nil
    }
  }

  private func send(
    type: String, deviceID: String? = nil, sessionID: String? = nil, payload: [String: Any]
  ) {
    let inner = RemoteOutgoingEnvelope(
      type: type, deviceID: deviceID, sessionID: sessionID, payload: payload)
    let envelope = encryptedOutgoingEnvelope(inner) ?? RemoteOutgoingEnvelope(
      type: "secure.required",
      deviceID: deviceID,
      sessionID: sessionID,
      payload: ["message": "End-to-end encryption is required. Please pair this mobile device again."]
    )
    guard let data = try? JSONSerialization.data(withJSONObject: envelope.dictionary),
      let text = String(data: data, encoding: .utf8)
    else { return }
    socket?.send(.string(text)) { _ in }
  }

  private func e2eSymmetricKey(for device: RemoteHostDevice) throws -> SymmetricKey {
    guard let model else {
      throw NSError(domain: "CoduxRemoteE2E", code: -10)
    }
    let cacheKey = RemoteE2ECrypto.cacheKey(
      hostPrivateKey: model.appSettings.remote.hostPrivateKey,
      remotePublicKey: device.publicKey,
      hostID: model.appSettings.remote.hostID,
      deviceID: device.id
    )
    if let cached = e2eKeyCache[cacheKey] {
      return cached
    }
    let key = try RemoteE2ECrypto.symmetricKey(
      hostPrivateKey: model.appSettings.remote.hostPrivateKey,
      remotePublicKey: device.publicKey,
      hostID: model.appSettings.remote.hostID,
      deviceID: device.id
    )
    e2eKeyCache[cacheKey] = key
    return key
  }

  private func pairingInfoWithLocalQR(_ response: RemotePairingInfo) -> RemotePairingInfo {
    guard let model else { return response }
    var next = response
    next.hostPublicKey = model.appSettings.remote.hostPublicKey
    next.cryptoVersion = 1
    let payload: [String: Any] = [
      "server": model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines),
      "code": response.code,
      "secret": response.secret,
      "hostName": Host.current().localizedName ?? "Codux Mac",
      "hostPublicKey": model.appSettings.remote.hostPublicKey,
      "cryptoVersion": 1,
    ]
    if let data = try? JSONSerialization.data(withJSONObject: payload) {
      next.qrPayload = RemoteE2ECrypto.base64URLEncode(data)
    }
    return next
  }

  private func remotePairingMatchCode(
    pairingCode: String,
    pairingSecret: String?,
    devicePublicKey: String
  ) -> String? {
    guard let model, devicePublicKey.isEmpty == false else { return nil }
    return RemoteE2ECrypto.matchCode(
      hostPublicKey: model.appSettings.remote.hostPublicKey,
      devicePublicKey: devicePublicKey,
      pairingCode: pairingCode,
      pairingSecret: pairingSecret ?? ""
    )
  }

  private func remoteURL(path: String, queryItems: [URLQueryItem], websocket: Bool) -> URL? {
    guard let model else { return nil }
    var components = URLComponents(
      string: model.appSettings.remote.serverURL.trimmingCharacters(in: .whitespacesAndNewlines))
    components?.path = path
    components?.queryItems = queryItems.isEmpty ? nil : queryItems
    if websocket {
      if components?.scheme == "https" { components?.scheme = "wss" }
      if components?.scheme == "http" { components?.scheme = "ws" }
    }
    return components?.url
  }

  private func update(status: RemoteHostStatus, message: String) {
    snapshot.status = status
    snapshot.message = message
  }

  private func randomToken() -> String {
    UUID().uuidString.replacingOccurrences(of: "-", with: "")
      + UUID().uuidString.replacingOccurrences(of: "-", with: "")
  }
}

private struct RemoteErrorResponse: Decodable {
  var error: String
}

private struct RemotePairingStatusResponse: Decodable {
  var status: String
  var pairingId: String?
  var code: String?
  var deviceName: String?
  var devicePublicKey: String?
}

private struct RemoteEnvelope: Decodable {
  var type: String
  var deviceID: String?
  var sessionID: String?
  var seq: Int64?
  var payload: [String: Any]?

  enum CodingKeys: String, CodingKey {
    case type
    case deviceID = "deviceId"
    case sessionID = "sessionId"
    case seq
    case payload
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.container(keyedBy: CodingKeys.self)
    type = try container.decode(String.self, forKey: .type)
    deviceID = try container.decodeIfPresent(String.self, forKey: .deviceID)
    sessionID = try container.decodeIfPresent(String.self, forKey: .sessionID)
    seq = try container.decodeIfPresent(Int64.self, forKey: .seq)
    if let data = try? container.decodeIfPresent(JSONValue.self, forKey: .payload) {
      payload = data.objectValue
    }
  }

  func withDeviceID(_ value: String) -> RemoteEnvelope {
    var next = self
    next.deviceID = value
    return next
  }
}

private struct RemoteOutgoingEnvelope {
  var type: String
  var deviceID: String?
  var sessionID: String?
  var seq: Int64?
  var payload: [String: Any]

  var dictionary: [String: Any] {
    var value: [String: Any] = ["type": type, "payload": payload]
    if let deviceID { value["deviceId"] = deviceID }
    if let sessionID { value["sessionId"] = sessionID }
    if let seq { value["seq"] = seq }
    return value
  }
}

private struct RemoteTerminalMetadata {
  var ownerDeviceID: String?
  var ownerKind: String
  var kind: String
  var resizeOwner: String
  var columns: Int?
  var rows: Int?
  var createdAt: Date
  var lastActiveAt: Date
  var status: String
}

private enum JSONValue: Decodable {
  case string(String)
  case number(Double)
  case bool(Bool)
  case object([String: Any])
  case array([Any])
  case null

  var objectValue: [String: Any]? {
    if case .object(let value) = self { return value }
    return nil
  }

  init(from decoder: Decoder) throws {
    let container = try decoder.singleValueContainer()
    if container.decodeNil() {
      self = .null
    } else if let value = try? container.decode(String.self) {
      self = .string(value)
    } else if let value = try? container.decode(Double.self) {
      self = .number(value)
    } else if let value = try? container.decode(Bool.self) {
      self = .bool(value)
    } else if let value = try? container.decode([String: JSONValue].self) {
      self = .object(value.mapValues(\.anyValue))
    } else if let value = try? container.decode([JSONValue].self) {
      self = .array(value.map(\.anyValue))
    } else {
      self = .null
    }
  }

  var anyValue: Any {
    switch self {
    case .string(let value): return value
    case .number(let value): return value
    case .bool(let value): return value
    case .object(let value): return value
    case .array(let value): return value
    case .null: return NSNull()
    }
  }
}

enum RemoteE2ECrypto {
  private static let saltPrefix = "codux-e2e-v1"
  private static let sharedInfo = Data("codux-remote-payload-v1".utf8)

  static func ensureHostIdentity(privateKey: String, publicKey: String) throws -> (
    privateKey: String, publicKey: String
  ) {
    if let privateData = base64URLDecode(privateKey),
      let key = try? Curve25519.KeyAgreement.PrivateKey(rawRepresentation: privateData)
    {
      let derivedPublicKey = base64URLEncode(key.publicKey.rawRepresentation)
      if publicKey.isEmpty || publicKey == derivedPublicKey {
        return (privateKey, derivedPublicKey)
      }
    }
    let key = Curve25519.KeyAgreement.PrivateKey()
    return (
      base64URLEncode(key.rawRepresentation),
      base64URLEncode(key.publicKey.rawRepresentation)
    )
  }

  static func encrypt(
    plaintext: Data,
    key: SymmetricKey,
    hostID: String,
    deviceID: String
  ) throws -> [String: Any] {
    let nonceData = randomData(count: 12)
    let nonce = try AES.GCM.Nonce(data: nonceData)
    let sealed = try AES.GCM.seal(plaintext, using: key, nonce: nonce, authenticating: aad(hostID: hostID, deviceID: deviceID))
    return [
      "v": 1,
      "alg": "X25519-HKDF-SHA256-AES-256-GCM",
      "nonce": base64URLEncode(nonceData),
      "ciphertext": base64URLEncode(sealed.ciphertext),
      "tag": base64URLEncode(sealed.tag),
    ]
  }

  static func decrypt(
    encryptedPayload: [String: Any],
    key: SymmetricKey,
    hostID: String,
    deviceID: String
  ) throws -> Data {
    guard (encryptedPayload["v"] as? Int ?? Int((encryptedPayload["v"] as? Double) ?? 0)) == 1,
      let nonceText = encryptedPayload["nonce"] as? String,
      let ciphertextText = encryptedPayload["ciphertext"] as? String,
      let tagText = encryptedPayload["tag"] as? String,
      let nonceData = base64URLDecode(nonceText),
      let ciphertext = base64URLDecode(ciphertextText),
      let tag = base64URLDecode(tagText)
    else {
      throw NSError(domain: "CoduxRemoteE2E", code: -1)
    }
    let box = try AES.GCM.SealedBox(
      nonce: AES.GCM.Nonce(data: nonceData),
      ciphertext: ciphertext,
      tag: tag
    )
    return try AES.GCM.open(box, using: key, authenticating: aad(hostID: hostID, deviceID: deviceID))
  }

  static func matchCode(
    hostPublicKey: String,
    devicePublicKey: String,
    pairingCode: String,
    pairingSecret: String
  ) -> String {
    let material = "codux-e2e-match-v1|\(hostPublicKey)|\(devicePublicKey)|\(pairingCode)|\(pairingSecret)"
    let digest = SHA256.hash(data: Data(material.utf8))
    let prefix = digest.prefix(3).map { String(format: "%02X", $0) }.joined()
    let split = prefix.index(prefix.startIndex, offsetBy: 3)
    return "\(prefix[..<split])-\(prefix[split...])"
  }

  static func base64URLEncode(_ data: Data) -> String {
    data.base64EncodedString()
      .replacingOccurrences(of: "+", with: "-")
      .replacingOccurrences(of: "/", with: "_")
      .replacingOccurrences(of: "=", with: "")
  }

  static func base64URLDecode(_ value: String) -> Data? {
    var normalized = value.replacingOccurrences(of: "-", with: "+")
      .replacingOccurrences(of: "_", with: "/")
    let remainder = normalized.count % 4
    if remainder > 0 {
      normalized += String(repeating: "=", count: 4 - remainder)
    }
    return Data(base64Encoded: normalized)
  }

  static func symmetricKey(
    hostPrivateKey: String,
    remotePublicKey: String,
    hostID: String,
    deviceID: String
  ) throws -> SymmetricKey {
    guard let privateData = base64URLDecode(hostPrivateKey),
      let publicData = base64URLDecode(remotePublicKey)
    else {
      throw NSError(domain: "CoduxRemoteE2E", code: -2)
    }
    let privateKey = try Curve25519.KeyAgreement.PrivateKey(rawRepresentation: privateData)
    let publicKey = try Curve25519.KeyAgreement.PublicKey(rawRepresentation: publicData)
    let shared = try privateKey.sharedSecretFromKeyAgreement(with: publicKey)
    return shared.hkdfDerivedSymmetricKey(
      using: SHA256.self,
      salt: salt(hostID: hostID, deviceID: deviceID),
      sharedInfo: sharedInfo,
      outputByteCount: 32
    )
  }

  static func cacheKey(
    hostPrivateKey: String,
    remotePublicKey: String,
    hostID: String,
    deviceID: String
  ) -> String {
    let material = "codux-e2e-cache-v1|\(hostPrivateKey)|\(remotePublicKey)|\(hostID)|\(deviceID)"
    return SHA256.hash(data: Data(material.utf8)).map { String(format: "%02x", $0) }.joined()
  }

  private static func salt(hostID: String, deviceID: String) -> Data {
    Data("\(saltPrefix)|\(hostID)|\(deviceID)".utf8)
  }

  private static func aad(hostID: String, deviceID: String) -> Data {
    Data("codux-e2e-aad-v1|\(hostID)|\(deviceID)".utf8)
  }

  private static func randomData(count: Int) -> Data {
    var data = Data(count: count)
    _ = data.withUnsafeMutableBytes { buffer in
      SecRandomCopyBytes(kSecRandomDefault, count, buffer.baseAddress!)
    }
    return data
  }
}
