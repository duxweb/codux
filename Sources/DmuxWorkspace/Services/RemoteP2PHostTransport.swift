import Foundation
@preconcurrency import LiveKitWebRTC

final class RemoteP2PHostTransport: NSObject, @unchecked Sendable {
  struct Signal {
    var deviceID: String
    var type: String
    var payload: [String: Any]
  }

  var onSignal: ((Signal) -> Void)?
  var onMessage: ((String, Data) -> Void)?
  var onState: ((String, String) -> Void)?

  fileprivate static let uploadChannelLabel = "codux-upload"

  private let factory: LKRTCPeerConnectionFactory
  private var peers: [String: Peer] = [:]

  override init() {
    LKRTCInitializeSSL()
    factory = LKRTCPeerConnectionFactory()
    super.init()
  }

  deinit {
    stop()
    LKRTCCleanupSSL()
  }

  func stop() {
    for peer in peers.values {
      peer.close()
    }
    peers.removeAll()
  }

  func close(deviceID: String) {
    peers.removeValue(forKey: deviceID)?.close()
    notifyState(deviceID: deviceID, state: "closed")
  }

  func isOpen(deviceID: String?) -> Bool {
    guard let deviceID else { return false }
    return peers[deviceID]?.isOpen == true
  }

  @discardableResult
  func send(data: Data, deviceID: String?, lane: Lane = .terminal) -> Bool {
    guard let deviceID, let peer = peers[deviceID], peer.isOpen else { return false }
    return peer.send(data: data, lane: lane)
  }

  func handleOffer(deviceID: String, payload: [String: Any]) {
    guard let sdp = payload["sdp"] as? String, sdp.isEmpty == false else { return }
    let peer = makePeer(deviceID: deviceID)
    let description = LKRTCSessionDescription(type: .offer, sdp: sdp)
    let constraints = LKRTCMediaConstraints(mandatoryConstraints: nil, optionalConstraints: nil)
    peer.connection.setRemoteDescription(description) { [weak self, weak peer] error in
      guard let self, let peer else { return }
      if let error {
        self.notifyState(deviceID: deviceID, state: "failed", error: error.localizedDescription)
        return
      }
      peer.connection.answer(for: constraints) { [weak self, weak peer] answer, error in
        guard let self, let peer else { return }
        if let error {
          self.notifyState(deviceID: deviceID, state: "failed", error: error.localizedDescription)
          return
        }
        guard let answer else { return }
        peer.connection.setLocalDescription(answer) { [weak self] error in
          guard let self else { return }
          if let error {
            self.notifyState(deviceID: deviceID, state: "failed", error: error.localizedDescription)
            return
          }
          self.onSignal?(
            Signal(
              deviceID: deviceID,
              type: "p2p.answer",
              payload: [
                "type": "answer",
                "sdp": answer.sdp,
              ]))
        }
      }
    }
  }

  func handleCandidate(deviceID: String, payload: [String: Any]) {
    guard let candidate = payload["candidate"] as? String, candidate.isEmpty == false else {
      return
    }
    let sdpMid = payload["sdpMid"] as? String
    let lineValue = payload["sdpMLineIndex"]
    let sdpMLineIndex =
      (lineValue as? Int32)
      ?? Int32((lineValue as? Int) ?? Int((lineValue as? Double) ?? 0))
    let ice = LKRTCIceCandidate(sdp: candidate, sdpMLineIndex: sdpMLineIndex, sdpMid: sdpMid)
    let peer = makePeer(deviceID: deviceID)
    peer.connection.add(ice) { [weak self] error in
      if let error {
        self?.notifyState(deviceID: deviceID, state: "failed", error: error.localizedDescription)
      }
    }
  }

  private func makePeer(deviceID: String) -> Peer {
    if let existing = peers[deviceID] { return existing }
    let configuration = LKRTCConfiguration()
    configuration.sdpSemantics = .unifiedPlan
    configuration.iceServers = [
      LKRTCIceServer(urlStrings: RemoteP2PIceServers.urls)
    ]
    configuration.bundlePolicy = .maxBundle
    configuration.rtcpMuxPolicy = .require
    configuration.continualGatheringPolicy = .gatherContinually
    let constraints = LKRTCMediaConstraints(
      mandatoryConstraints: nil,
      optionalConstraints: ["DtlsSrtpKeyAgreement": "true"]
    )
    let connection = factory.peerConnection(
      with: configuration,
      constraints: constraints,
      delegate: nil
    )!
    let peer = Peer(deviceID: deviceID, connection: connection, owner: self)
    connection.delegate = peer
    peers[deviceID] = peer
    notifyState(deviceID: deviceID, state: "connecting")
    return peer
  }

  private func notifyState(deviceID: String, state: String, error: String? = nil) {
    var payload: [String: Any] = ["state": state]
    if let error { payload["error"] = error }
    onState?(deviceID, state)
    onSignal?(Signal(deviceID: deviceID, type: "p2p.state", payload: payload))
  }

  fileprivate func handleOpen(deviceID: String) {
    notifyState(deviceID: deviceID, state: "connected")
  }

  fileprivate func handleClosed(deviceID: String) {
    notifyState(deviceID: deviceID, state: "closed")
  }

  fileprivate func handleMessage(deviceID: String, data: Data) {
    onMessage?(deviceID, data)
  }

  fileprivate func handleCandidate(deviceID: String, candidate: LKRTCIceCandidate) {
    onSignal?(
      Signal(
        deviceID: deviceID,
        type: "p2p.candidate",
        payload: [
          "candidate": candidate.sdp,
          "sdpMid": candidate.sdpMid as Any,
          "sdpMLineIndex": Int(candidate.sdpMLineIndex),
        ]))
  }

  fileprivate func handleConnectionState(deviceID: String, state: LKRTCPeerConnectionState) {
    switch state {
    case .connected:
      notifyState(deviceID: deviceID, state: "connected")
    case .failed:
      notifyState(deviceID: deviceID, state: "failed")
    case .disconnected:
      notifyState(deviceID: deviceID, state: "disconnected")
    case .closed:
      notifyState(deviceID: deviceID, state: "closed")
    default:
      break
    }
  }
}

extension RemoteP2PHostTransport {
  enum Lane {
    case terminal
    case upload
  }
}

private enum RemoteP2PIceServers {
  private static let domesticSTUNURLs = [
    "stun:stun.miwifi.com:3478"
  ]

  private static let globalSTUNURLs = [
    "stun:stun.l.google.com:19302",
    "stun:global.stun.twilio.com:3478",
  ]

  static var urls: [String] {
    prefersDomesticSTUN ? domesticSTUNURLs + globalSTUNURLs : globalSTUNURLs + domesticSTUNURLs
  }

  private static var prefersDomesticSTUN: Bool {
    let language = Locale.preferredLanguages.first?.lowercased() ?? ""
    return language.hasPrefix("zh")
  }
}

private final class Peer: NSObject, LKRTCPeerConnectionDelegate, LKRTCDataChannelDelegate,
  @unchecked Sendable
{
  private static let terminalBufferedAmountHighWatermark: UInt64 = 192 * 1024
  private static let uploadBufferedAmountHighWatermark: UInt64 = 512 * 1024

  let deviceID: String
  let connection: LKRTCPeerConnection
  weak var owner: RemoteP2PHostTransport?
  private var terminalChannel: LKRTCDataChannel?
  private var uploadChannel: LKRTCDataChannel?

  init(deviceID: String, connection: LKRTCPeerConnection, owner: RemoteP2PHostTransport) {
    self.deviceID = deviceID
    self.connection = connection
    self.owner = owner
    super.init()
  }

  var isOpen: Bool {
    terminalChannel?.readyState == .open
  }

  func send(data: Data, lane: RemoteP2PHostTransport.Lane) -> Bool {
    guard let channel = channel(for: lane), channel.readyState == .open else {
      return false
    }
    guard channel.bufferedAmount < highWatermark(for: channel) else { return false }
    channel.sendData(LKRTCDataBuffer(data: data, isBinary: false))
    return true
  }

  func close() {
    terminalChannel?.close()
    uploadChannel?.close()
    terminalChannel = nil
    uploadChannel = nil
    connection.close()
  }

  private func channel(for lane: RemoteP2PHostTransport.Lane) -> LKRTCDataChannel? {
    if lane == .upload, uploadChannel?.readyState == .open {
      return uploadChannel
    }
    return terminalChannel
  }

  private func highWatermark(for channel: LKRTCDataChannel) -> UInt64 {
    if channel === uploadChannel {
      return Self.uploadBufferedAmountHighWatermark
    }
    return Self.terminalBufferedAmountHighWatermark
  }

  func peerConnection(
    _ peerConnection: LKRTCPeerConnection, didChange stateChanged: LKRTCSignalingState
  ) {}
  func peerConnection(_ peerConnection: LKRTCPeerConnection, didAdd stream: LKRTCMediaStream) {}
  func peerConnection(_ peerConnection: LKRTCPeerConnection, didRemove stream: LKRTCMediaStream) {}
  func peerConnectionShouldNegotiate(_ peerConnection: LKRTCPeerConnection) {}
  func peerConnection(
    _ peerConnection: LKRTCPeerConnection, didChange newState: LKRTCIceConnectionState
  ) {}
  func peerConnection(
    _ peerConnection: LKRTCPeerConnection, didChange newState: LKRTCIceGatheringState
  ) {}
  func peerConnection(
    _ peerConnection: LKRTCPeerConnection, didRemove candidates: [LKRTCIceCandidate]
  ) {}

  func peerConnection(
    _ peerConnection: LKRTCPeerConnection, didGenerate candidate: LKRTCIceCandidate
  ) {
    owner?.handleCandidate(deviceID: deviceID, candidate: candidate)
  }

  func peerConnection(_ peerConnection: LKRTCPeerConnection, didOpen dataChannel: LKRTCDataChannel)
  {
    switch dataChannel.label {
    case RemoteP2PHostTransport.uploadChannelLabel:
      uploadChannel = dataChannel
    default:
      terminalChannel = dataChannel
    }
    dataChannel.delegate = self
    if dataChannel.readyState == .open {
      handleDataChannelOpen(dataChannel)
    }
  }

  func peerConnection(
    _ peerConnection: LKRTCPeerConnection, didChange newState: LKRTCPeerConnectionState
  ) {
    owner?.handleConnectionState(deviceID: deviceID, state: newState)
  }

  func dataChannelDidChangeState(_ dataChannel: LKRTCDataChannel) {
    switch dataChannel.readyState {
    case .open:
      handleDataChannelOpen(dataChannel)
    case .closed:
      handleDataChannelClosed(dataChannel)
    default:
      break
    }
  }

  func dataChannel(_ dataChannel: LKRTCDataChannel, didReceiveMessageWith buffer: LKRTCDataBuffer) {
    owner?.handleMessage(deviceID: deviceID, data: buffer.data)
  }

  private func handleDataChannelOpen(_ dataChannel: LKRTCDataChannel) {
    if dataChannel === terminalChannel {
      owner?.handleOpen(deviceID: deviceID)
    }
  }

  private func handleDataChannelClosed(_ dataChannel: LKRTCDataChannel) {
    if dataChannel === terminalChannel {
      terminalChannel = nil
      owner?.handleClosed(deviceID: deviceID)
    } else if dataChannel === uploadChannel {
      uploadChannel = nil
    }
  }
}
