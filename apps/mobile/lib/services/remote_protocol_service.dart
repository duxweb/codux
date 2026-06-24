import 'dart:async';
import 'dart:convert';
import 'dart:math';
import '../i18n.dart';
import '../models/remote_models.dart';
import 'log_service.dart';
import 'remote_protocol.dart';
import 'remote_transport.dart';
export 'remote_protocol.dart';

Future<PairingPayload> parsePairingPayload(String input) async {
  final parsed = _parsePairingTokenPayload(input);
  return _pairingPayloadFromJson(parsed.payload, server: parsed.server);
}

class _FetchedPairingPayload {
  const _FetchedPairingPayload({required this.server, required this.payload});

  final String server;
  final Map<String, dynamic> payload;
}

Future<PairingPayload> _pairingPayloadFromJson(
  Map<String, dynamic> parsed, {
  required String server,
}) async {
  final normalizedServer = remoteTransportRelayUrl(server);
  final code = parsed['code']?.toString();
  final secret = parsed['secret']?.toString();
  final hostId = parsed['hostId']?.toString();
  final transports = _normalizedPairingTransports(parsed, normalizedServer);
  final hasSupportedTransport = _irohTransport(transports) != null;
  final missingFields = <String>[
    if (code == null || code.isEmpty) 'code',
    if (secret == null || secret.isEmpty) 'secret',
    if (parsed['pairingId']?.toString().trim().isEmpty != false) 'pairingId',
    if (!hasSupportedTransport) 'transports.iroh',
  ];
  if (missingFields.isNotEmpty) {
    throw Exception(
      '${tr('remote.qrMissingFields', LocaleChoices.system.id)} (${missingFields.join(', ')})',
    );
  }
  final pairingCode = code!;
  final pairingSecret = secret!;
  final deviceId = _newDeviceId();
  CoduxLog.info(
    '[codux-flutter-pairing] payload ready server=$normalizedServer host=${hostId ?? ''} pair=${parsed['pairingId']?.toString() ?? ''} transports=${_transportLogSummary(transports)}',
  );
  return PairingPayload(
    server: normalizedServer,
    code: pairingCode,
    secret: pairingSecret,
    deviceId: deviceId,
    hostName: parsed['hostName']?.toString(),
    hostId: hostId,
    transports: transports,
    pairingId: parsed['pairingId']?.toString(),
  );
}

List<RemoteTransportCandidate> _normalizedPairingTransports(
  Map<String, dynamic> parsed,
  String server,
) {
  return remoteTransportCandidatesFromJson(parsed['transports']).map((
    candidate,
  ) {
    if (candidate.kind != RemoteTransportKind.iroh) return candidate;
    return RemoteTransportCandidate(
      kind: candidate.kind,
      role: candidate.role,
      url: server,
      nodeId: candidate.nodeId,
      relayUrl: candidate.relayUrl,
      ticket: candidate.ticket,
      relayAuthentication: candidate.relayAuthentication,
    );
  }).toList();
}

List<RemoteTransportCandidate> _confirmedTransports(
  Object? value,
  String fallbackServer,
) {
  final transports = remoteTransportCandidatesFromJson(value);
  if (transports.isEmpty) return const [];
  return transports.map((candidate) {
    if (candidate.kind != RemoteTransportKind.iroh) {
      return candidate;
    }
    return RemoteTransportCandidate(
      kind: candidate.kind,
      role: candidate.role,
      url: candidate.url.trim().isEmpty ? fallbackServer : candidate.url,
      nodeId: candidate.nodeId,
      relayUrl: candidate.relayUrl,
      relayAuthentication: candidate.relayAuthentication,
    );
  }).toList();
}

RemoteTransportCandidate? _irohTransport(
  List<RemoteTransportCandidate> transports,
) {
  for (final candidate in transports) {
    if (candidate.kind == RemoteTransportKind.iroh &&
        (candidate.ticket.trim().isNotEmpty ||
            (candidate.nodeId.trim().isNotEmpty &&
                candidate.relayUrl.trim().isNotEmpty))) {
      return candidate;
    }
  }
  return null;
}

_FetchedPairingPayload _parsePairingTokenPayload(String input) {
  final value = input.trim();
  if (value.isEmpty) {
    throw Exception(tr('remote.qrEmpty', LocaleChoices.system.id));
  }
  final uri = Uri.tryParse(value);
  if (uri != null && uri.scheme == 'codux' && uri.host == 'pair') {
    final encodedPayload = uri.queryParameters['payload']?.trim() ?? '';
    if (encodedPayload.isNotEmpty) {
      return _decodeEmbeddedPairingPayload(encodedPayload);
    }
  }
  return _decodeEmbeddedPairingPayload(value);
}

_FetchedPairingPayload _decodeEmbeddedPairingPayload(String encodedPayload) {
  try {
    final normalized = base64Url.normalize(encodedPayload);
    final decoded = utf8.decode(base64Url.decode(normalized));
    final value = jsonDecode(decoded);
    if (value is! Map) {
      throw const FormatException('pairing payload must be an object');
    }
    final payload = Map<String, dynamic>.from(value);
    final transports = remoteTransportCandidatesFromJson(payload['transports']);
    final iroh = _irohTransport(transports);
    // Slim QR codes carry only `relayUrl` (no full `url`/ticket); fall back to it
    // so the relay server is still resolved. The host re-sends the canonical
    // transports on the pairing.confirmed reply once connected.
    final candidateUrl = iroh?.url.trim() ?? '';
    final candidateRelay = iroh?.relayUrl.trim() ?? '';
    final server = candidateUrl.isNotEmpty ? candidateUrl : candidateRelay;
    return _FetchedPairingPayload(server: server, payload: payload);
  } catch (_) {
    throw Exception(tr('remote.qrInvalid', LocaleChoices.system.id));
  }
}

RelayEnvelope pairingRequestEnvelope(PairingPayload payload, String name) {
  final pairingId = payload.pairingId?.trim();
  if (pairingId == null || pairingId.isEmpty) {
    throw Exception(tr('remote.qrMissingFields', LocaleChoices.system.id));
  }
  return RelayEnvelope(
    type: RemoteMessageType.pairingRequest,
    deviceId: payload.deviceId,
    payload: {
      'pairingId': pairingId,
      'code': payload.code,
      'secret': payload.secret,
      'deviceName': name,
      'deviceId': payload.deviceId,
    },
  );
}

Future<StoredDevice> confirmPairingOverIroh({
  required PairingPayload payload,
  required String name,
  Duration timeout = const Duration(seconds: 90),
}) async {
  RemoteTransportCandidate? transport;
  final preferred = remotePreferredTransportKind(
    payload.transports,
    pairing: true,
  );
  for (final candidate in payload.transports) {
    if (candidate.kind == preferred) {
      transport = candidate;
      break;
    }
  }
  if (transport == null) {
    throw Exception(tr('remote.qrMissingFields', LocaleChoices.system.id));
  }
  CoduxLog.info(
    '[codux-flutter-pairing] iroh confirm start relay=${payload.server} transport=${transport.kind} url=${transport.url} host=${payload.hostId ?? ''} pair=${payload.pairingId ?? ''}',
  );
  final pendingDevice = pendingPairingDevice(payload: payload, name: name);
  final pairingTransport = createRemoteTransport(pendingDevice);
  final completer = Completer<RelayEnvelope>();
  pairingTransport.onEnvelope = (envelope, _) {
    final message = RelayEnvelope.fromJson(envelope);
    if (message.type == RemoteMessageType.pairingConfirmed ||
        message.type == RemoteMessageType.pairingRejected) {
      if (!completer.isCompleted) completer.complete(message);
    }
  };
  pairingTransport.onState = (state) {
    if (RemoteTransportStateEvent.parse(state).isClosed &&
        !completer.isCompleted) {
      completer.completeError(
        Exception(tr('remote.waitTimeout', LocaleChoices.system.id)),
      );
    }
  };
  try {
    await pairingTransport.connect(pendingDevice);
    final sent = await pairingTransport.send(
      pairingRequestEnvelope(payload, name).toJson(),
    );
    if (!sent) {
      throw Exception(tr('remote.waitTimeout', LocaleChoices.system.id));
    }
    final message = await completer.future.timeout(timeout);
    if (message.type == RemoteMessageType.pairingRejected) {
      throw const PairingRejectedException();
    }
    final device = confirmedDevice(
      payload: payload,
      name: name,
      confirmed: message,
    );
    CoduxLog.info(
      '[codux-flutter-pairing] iroh confirm accepted relay=${device.server} host=${device.hostId} device=${device.deviceId} transports=${_transportLogSummary(device.transports)}',
    );
    return device;
  } on TimeoutException {
    throw Exception(tr('remote.waitTimeout', LocaleChoices.system.id));
  } finally {
    await pairingTransport.close();
  }
}

StoredDevice pendingPairingDevice({
  required PairingPayload payload,
  required String name,
}) {
  return StoredDevice(
    server: payload.server,
    hostId: payload.hostId ?? '',
    deviceId: payload.deviceId,
    token: '',
    name: name,
    hostName: payload.hostName,
    transports: payload.transports,
  );
}

StoredDevice confirmedDevice({
  required PairingPayload payload,
  required String name,
  required RelayEnvelope confirmed,
}) {
  final data = confirmed.payload;
  if (data is! Map ||
      data['hostId'] == null ||
      data['deviceId'] == null ||
      data['token'] == null) {
    throw Exception('Pairing confirmed without device credentials');
  }
  final confirmedTransports = _confirmedTransports(
    data['transports'],
    payload.server,
  );
  if (confirmedTransports.isEmpty) {
    throw Exception('Pairing confirmed without device transport');
  }
  final server = confirmedTransports
      .firstWhere(
        (candidate) => candidate.url.trim().isNotEmpty,
        orElse: () => confirmedTransports.first,
      )
      .url;
  return StoredDevice(
    server: server,
    hostId: '${data['hostId']}',
    deviceId: '${data['deviceId']}',
    token: '${data['token']}',
    name: name,
    hostName: data['hostName']?.toString() ?? payload.hostName,
    transports: confirmedTransports,
  );
}

String _newDeviceId() {
  final random = Random.secure();
  final bytes = List<int>.generate(16, (_) => random.nextInt(256));
  return bytes.map((byte) => byte.toRadixString(16).padLeft(2, '0')).join();
}

String _transportLogSummary(List<RemoteTransportCandidate> transports) {
  return transports
      .map(
        (item) =>
            '${item.kind}:${item.url.trim().isEmpty ? 'empty' : item.url}',
      )
      .join(',');
}

class PairingCancelledException implements Exception {
  const PairingCancelledException();
  @override
  String toString() => tr('pair.cancelled', LocaleChoices.system.id);
}

class PairingRejectedException implements Exception {
  const PairingRejectedException();
  @override
  String toString() => tr('pair.rejected', LocaleChoices.system.id);
}
