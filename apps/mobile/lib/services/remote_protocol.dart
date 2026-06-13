import '../models/remote_models.dart';
import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_protocol_ffi;

final String remoteProtocolVersion = codux_protocol_ffi.protocolVersion();

abstract final class RemoteResourceType {
  static final projects = codux_protocol_ffi.resourceType('projects');
  static final terminals = codux_protocol_ffi.resourceType('terminals');
  static final worktrees = codux_protocol_ffi.resourceType('worktrees');
  static final gitStatus = codux_protocol_ffi.resourceType('gitStatus');
  static final aiStats = codux_protocol_ffi.resourceType('aiStats');
  static final files = codux_protocol_ffi.resourceType('files');
}

abstract final class RemoteMessageType {
  static final hello = codux_protocol_ffi.messageType('hello');
  static final error = codux_protocol_ffi.messageType('error');
  static final secureMessage = codux_protocol_ffi.messageType('secureMessage');
  static final hostInfo = codux_protocol_ffi.messageType('hostInfo');
  static final hostOffline = codux_protocol_ffi.messageType('hostOffline');
  static final secureRequired = codux_protocol_ffi.messageType(
    'secureRequired',
  );
  static final deviceInfo = codux_protocol_ffi.messageType('deviceInfo');
  static final deviceDisconnected = codux_protocol_ffi.messageType(
    'deviceDisconnected',
  );
  static final pairingRequest = codux_protocol_ffi.messageType(
    'pairingRequest',
  );
  static final pairingConfirmed = codux_protocol_ffi.messageType(
    'pairingConfirmed',
  );
  static final pairingRejected = codux_protocol_ffi.messageType(
    'pairingRejected',
  );
  static final transportPing = codux_protocol_ffi.messageType('transportPing');
  static final transportPong = codux_protocol_ffi.messageType('transportPong');
  static final resourceSubscribe = codux_protocol_ffi.messageType(
    'resourceSubscribe',
  );
  static final resourceUnsubscribe = codux_protocol_ffi.messageType(
    'resourceUnsubscribe',
  );
  static final projectList = codux_protocol_ffi.messageType('projectList');
  static final projectSelect = codux_protocol_ffi.messageType('projectSelect');
  static final projectSelected = codux_protocol_ffi.messageType(
    'projectSelected',
  );
  static final projectAdd = codux_protocol_ffi.messageType('projectAdd');
  static final projectEdit = codux_protocol_ffi.messageType('projectEdit');
  static final projectRemove = codux_protocol_ffi.messageType('projectRemove');
  static final projectUpdated = codux_protocol_ffi.messageType(
    'projectUpdated',
  );
  static final worktreeList = codux_protocol_ffi.messageType('worktreeList');
  static final worktreeSelect = codux_protocol_ffi.messageType(
    'worktreeSelect',
  );
  static final worktreeCreate = codux_protocol_ffi.messageType(
    'worktreeCreate',
  );
  static final worktreeMerge = codux_protocol_ffi.messageType('worktreeMerge');
  static final worktreeDelete = codux_protocol_ffi.messageType(
    'worktreeDelete',
  );
  static final worktreeUpdated = codux_protocol_ffi.messageType(
    'worktreeUpdated',
  );
  static final terminalList = codux_protocol_ffi.messageType('terminalList');
  static final terminalSubscribe = codux_protocol_ffi.messageType(
    'terminalSubscribe',
  );
  static final terminalUnsubscribe = codux_protocol_ffi.messageType(
    'terminalUnsubscribe',
  );
  static final terminalCreate = codux_protocol_ffi.messageType(
    'terminalCreate',
  );
  static final terminalCreated = codux_protocol_ffi.messageType(
    'terminalCreated',
  );
  static final terminalClose = codux_protocol_ffi.messageType('terminalClose');
  static final terminalClosed = codux_protocol_ffi.messageType(
    'terminalClosed',
  );
  static final terminalBuffer = codux_protocol_ffi.messageType(
    'terminalBuffer',
  );
  static final terminalOutput = codux_protocol_ffi.messageType(
    'terminalOutput',
  );
  static final terminalOutputAck = codux_protocol_ffi.messageType(
    'terminalOutputAck',
  );
  static final terminalInput = codux_protocol_ffi.messageType('terminalInput');
  static final terminalInputAck = codux_protocol_ffi.messageType(
    'terminalInputAck',
  );
  static final terminalViewportClaim = codux_protocol_ffi.messageType(
    'terminalViewportClaim',
  );
  static final terminalViewportResize = codux_protocol_ffi.messageType(
    'terminalViewportResize',
  );
  static final terminalViewportRelease = codux_protocol_ffi.messageType(
    'terminalViewportRelease',
  );
  static final terminalViewportState = codux_protocol_ffi.messageType(
    'terminalViewportState',
  );
  static final terminalUploadStart = codux_protocol_ffi.messageType(
    'terminalUploadStart',
  );
  static final terminalUploadChunk = codux_protocol_ffi.messageType(
    'terminalUploadChunk',
  );
  static final terminalUploadFinish = codux_protocol_ffi.messageType(
    'terminalUploadFinish',
  );
  static final terminalUploadAck = codux_protocol_ffi.messageType(
    'terminalUploadAck',
  );
  static final terminalUploaded = codux_protocol_ffi.messageType(
    'terminalUploaded',
  );
  static final fileList = codux_protocol_ffi.messageType('fileList');
  static final fileRead = codux_protocol_ffi.messageType('fileRead');
  static final fileWrite = codux_protocol_ffi.messageType('fileWrite');
  static final fileWritten = codux_protocol_ffi.messageType('fileWritten');
  static final fileRename = codux_protocol_ffi.messageType('fileRename');
  static final fileRenamed = codux_protocol_ffi.messageType('fileRenamed');
  static final fileDelete = codux_protocol_ffi.messageType('fileDelete');
  static final fileDeleted = codux_protocol_ffi.messageType('fileDeleted');
  static final gitStatus = codux_protocol_ffi.messageType('gitStatus');
  static final aiStats = codux_protocol_ffi.messageType('aiStats');
}

RelayEnvelope remoteResourceSubscribeEnvelope({
  required String resource,
  String? projectId,
  String? sessionId,
  bool baseline = true,
  int? maxChars,
  int? chunkChars,
  String? requestId,
}) {
  final envelope = RelayEnvelope.fromJson(
    codux_protocol_ffi.resourceSubscribeEnvelope(
      resource: resource,
      projectId: projectId,
      sessionId: sessionId,
      baseline: baseline,
      maxChars: maxChars,
      chunkChars: chunkChars,
    ),
  );
  final cleanRequestId = requestId?.trim();
  if (cleanRequestId != null && cleanRequestId.isNotEmpty) {
    final payload = envelope.payload;
    if (payload is Map) {
      payload['requestId'] = cleanRequestId;
    }
  }
  return envelope;
}

RelayEnvelope remoteResourceUnsubscribeEnvelope({
  required String resource,
  String? projectId,
  String? sessionId,
}) {
  return RelayEnvelope.fromJson(
    codux_protocol_ffi.resourceUnsubscribeEnvelope(
      resource: resource,
      projectId: projectId,
      sessionId: sessionId,
    ),
  );
}

bool remoteRelayBlocksMessage(String kind) {
  return codux_protocol_ffi.relayBlocksMessage(kind);
}

String remoteTransportServerUrl(String base) {
  return codux_protocol_ffi.transportServerUrl(base);
}

String remoteTransportRelayUrlForPreset({
  required String preset,
  String customUrl = '',
}) {
  return codux_protocol_ffi.transportRelayUrlForPreset(
    preset: preset,
    customUrl: customUrl,
  );
}

String remoteTransportPairingTicketUrl({
  required String base,
  required String ticket,
}) {
  return codux_protocol_ffi.transportPairingTicketUrl(
    base: base,
    ticket: ticket,
  );
}

String remoteTransportPairingCodeUrl({
  required String base,
  required String code,
}) {
  return codux_protocol_ffi.transportPairingCodeUrl(base: base, code: code);
}

String remoteTransportPairingWebSocketUrl({
  required String base,
  required String hostId,
  required String devicePublicKey,
}) {
  return codux_protocol_ffi.transportPairingWebSocketUrl(
    base: base,
    hostId: hostId,
    devicePublicKey: devicePublicKey,
  );
}

String remoteTransportClientWebSocketUrl({
  required String base,
  required String hostId,
  required String deviceId,
  String token = '',
}) {
  return codux_protocol_ffi.transportClientWebSocketUrl(
    base: base,
    hostId: hostId,
    deviceId: deviceId,
    token: token,
  );
}

List<Map<String, dynamic>> remoteTransportDefaultIceServers() {
  return codux_protocol_ffi.transportDefaultIceServers();
}

String remotePreferredTransportKind(
  List<RemoteTransportCandidate> transports, {
  required bool pairing,
}) {
  return codux_protocol_ffi.preferredTransportKind(
    transports.map((item) => item.toJson()).toList(),
    pairing: pairing,
  );
}
