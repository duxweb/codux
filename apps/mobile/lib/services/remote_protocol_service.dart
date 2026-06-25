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
  // Decode the codux://pair URL / base64url token to its JSON object (stable
  // transport encoding), then VALIDATE + normalize through the shared Rust
  // parser via FFI — the same format the desktop and agent hosts emit, so the
  // client no longer re-implements the pairing format in Dart.
  final payload = _decodePairingPayloadJson(input);
  final result = remoteParsePairingPayload(payload);
  final missing = result['missingFields'];
  if (missing is List && missing.isNotEmpty) {
    throw Exception(
      '${tr('remote.qrMissingFields', LocaleChoices.system.id)} (${missing.join(', ')})',
    );
  }
  final ok = result['ok'];
  if (ok is! Map) {
    throw Exception(tr('remote.qrInvalid', LocaleChoices.system.id));
  }
  final parsed = Map<String, dynamic>.from(ok);
  final transports = remoteTransportCandidatesFromJson(parsed['transports']);
  CoduxLog.info(
    '[codux-flutter-pairing] payload ready server=${parsed['server'] ?? ''} host=${parsed['hostId'] ?? ''} pair=${parsed['pairingId'] ?? ''} transports=${_transportLogSummary(transports)}',
  );
  return PairingPayload(
    server: parsed['server']?.toString() ?? '',
    code: parsed['code']?.toString() ?? '',
    secret: parsed['secret']?.toString() ?? '',
    deviceId: _newDeviceId(),
    hostName: parsed['hostName']?.toString(),
    hostId: parsed['hostId']?.toString(),
    transports: transports,
    pairingId: parsed['pairingId']?.toString(),
  );
}

/// Decode the pairing input — a `codux://pair?payload=<base64url>` URL or a bare
/// base64url token — to its JSON object. Stable transport encoding only; the
/// format validation lives in the shared Rust parser (via FFI).
Map<String, dynamic> _decodePairingPayloadJson(String input) {
  final value = input.trim();
  if (value.isEmpty) {
    throw Exception(tr('remote.qrEmpty', LocaleChoices.system.id));
  }
  var encoded = value;
  final uri = Uri.tryParse(value);
  if (uri != null && uri.scheme == 'codux' && uri.host == 'pair') {
    final embedded = uri.queryParameters['payload']?.trim() ?? '';
    if (embedded.isNotEmpty) encoded = embedded;
  }
  try {
    final decoded = utf8.decode(base64Url.decode(base64Url.normalize(encoded)));
    final parsed = jsonDecode(decoded);
    if (parsed is! Map) {
      throw const FormatException('pairing payload must be an object');
    }
    return Map<String, dynamic>.from(parsed);
  } catch (_) {
    throw Exception(tr('remote.qrInvalid', LocaleChoices.system.id));
  }
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

  // iroh connection setup over a relay (and especially USB-tethered) paths is
  // flaky: a single QUIC handshake can stall past the 15s connect window even
  // though the next dial succeeds. Retry the CONNECT phase with fresh transports
  // so one transient timeout doesn't doom the whole pairing. This is safe
  // because the pairing.request is only sent AFTER connect succeeds — a failed
  // connect never put a request on the wire, so there is no double-request or
  // stale-active_pairing rejection risk. Once connected, send + confirm-wait run
  // exactly once (no retry) to keep that guarantee.
  const maxConnectAttempts = 3;
  Object? lastError;
  for (var attempt = 1; attempt <= maxConnectAttempts; attempt++) {
    final pairingTransport = createRemoteTransport(pendingDevice);
    final completer = Completer<RelayEnvelope>();
    var connected = false;
    pairingTransport.onEnvelope = (envelope, _) {
      final message = RelayEnvelope.fromJson(envelope);
      if (message.type == RemoteMessageType.pairingConfirmed ||
          message.type == RemoteMessageType.pairingRejected) {
        if (!completer.isCompleted) completer.complete(message);
      }
    };
    pairingTransport.onState = (state) {
      // Only treat a drop as fatal once we are past connect — a close during the
      // connect retries is expected churn, not a confirm-wait failure.
      if (connected &&
          RemoteTransportStateEvent.parse(state).isClosed &&
          !completer.isCompleted) {
        completer.completeError(
          Exception(tr('remote.waitTimeout', LocaleChoices.system.id)),
        );
      }
    };
    try {
      await pairingTransport.connect(pendingDevice);
      connected = true;
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
    } on PairingRejectedException {
      await pairingTransport.close();
      rethrow;
    } catch (error) {
      await pairingTransport.close();
      lastError = error;
      // Connected then failed later (send/confirm) is terminal — retrying would
      // re-send against an already-consumed pairing. Only retry pure connect
      // failures, and only while attempts remain.
      if (connected || attempt >= maxConnectAttempts) {
        break;
      }
      CoduxLog.info(
        '[codux-flutter-pairing] iroh connect attempt $attempt failed, retrying: $error',
      );
      await Future<void>.delayed(Duration(milliseconds: 600 * attempt));
    }
  }
  if (lastError is TimeoutException) {
    throw Exception(tr('remote.waitTimeout', LocaleChoices.system.id));
  }
  throw lastError ??
      Exception(tr('remote.waitTimeout', LocaleChoices.system.id));
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
