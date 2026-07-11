import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';

import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_protocol_ffi;

import '../models/remote_models.dart';
import 'log_service.dart';

typedef RemoteTransportStateHandler = void Function(String state);
// [raw] is the exact wire JSON the envelope was decoded from. It is carried
// alongside the parsed map so the hot terminal-output path can hand it straight
// to the Rust router without re-serializing a (potentially 16 KB) payload that
// was just parsed off the wire.
typedef RemoteTransportEnvelopeHandler =
    void Function(Map<String, dynamic> envelope, String raw);
typedef RemoteTransportFactory = RemoteTransport Function(StoredDevice device);
typedef ControllerTransportHandleFactory =
    ControllerTransportEventHandle? Function(Map<String, dynamic> config);
typedef RemoteTransportPollTimerFactory =
    Timer Function(Duration delay, void Function() callback);

const int _remoteTransportPollBatchSize = 128;
const Duration _remoteTransportBusyPollDelay = Duration(milliseconds: 16);
const Duration _remoteTransportIdlePollDelay = Duration(milliseconds: 100);

class RemoteTransportStateEvent {
  const RemoteTransportStateEvent({
    required this.state,
    required this.detail,
    required this.path,
    required this.addr,
    required this.relayUrl,
  });

  final String state;
  final String detail;
  final String? path;
  final String? addr;
  final String? relayUrl;

  bool get isPathUpdate => state == 'path' || path != null;
  bool get isConnected => state == 'connected';
  bool get isClosed => state == 'failed' || state == 'closed';

  static RemoteTransportStateEvent parse(String rawState) {
    final state = rawState.split(':').first.trim();
    final detail = rawState.length > state.length
        ? rawState.substring(state.length + 1).trim()
        : '';
    return RemoteTransportStateEvent(
      state: state,
      detail: detail,
      path: parseTransportPath(detail),
      addr: parseTransportAddress(detail),
      relayUrl: parseRelayUrl(detail),
    );
  }
}

abstract interface class ControllerTransportEventHandle {
  bool get isClosed;
  bool send(Map<String, dynamic> envelope);
  bool sendTerminal(Map<String, dynamic> envelope);
  bool sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  });
  Map<String, dynamic>? pollEvent();
  void close();
}

abstract interface class RemoteTransport {
  String get kind;
  set onState(RemoteTransportStateHandler? handler);
  set onEnvelope(RemoteTransportEnvelopeHandler? handler);
  Future<void> connect(StoredDevice device);
  Future<bool> send(Map<String, dynamic> envelope);
  Future<bool> sendTerminal(Map<String, dynamic> envelope);
  Future<bool> sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  });
  Future<void> close();
}

String? parseTransportPath(String detail) {
  for (final part in detail.split(';')) {
    final trimmed = part.trim();
    if (!trimmed.startsWith('path=')) continue;
    final value = trimmed.substring(5).trim();
    if (value == 'direct' ||
        value == 'relay' ||
        value == 'unknown' ||
        value == 'none') {
      return value;
    }
  }
  return null;
}

String? parseTransportAddress(String detail) {
  for (final part in detail.split(';')) {
    final trimmed = part.trim();
    if (!trimmed.startsWith('addr=')) continue;
    final value = trimmed.substring(5).trim();
    return value.isEmpty ? null : value;
  }
  return null;
}

String? parseRelayUrl(String detail) {
  for (final part in detail.split(';')) {
    final trimmed = part.trim();
    if (!trimmed.startsWith('relayUrl=')) continue;
    final value = trimmed.substring(9).trim();
    return value.isEmpty ? null : value;
  }
  return null;
}

RemoteTransport createRemoteTransport(StoredDevice device) {
  return RustControllerTransport();
}

class RustControllerTransport implements RemoteTransport {
  RustControllerTransport({
    ControllerTransportHandleFactory? handleFactory,
    RemoteTransportPollTimerFactory? pollTimerFactory,
  }) : _handleFactory = handleFactory ?? _connectFfiTransport,
       _pollTimerFactory = pollTimerFactory ?? Timer.new;

  final ControllerTransportHandleFactory _handleFactory;
  final RemoteTransportPollTimerFactory _pollTimerFactory;
  ControllerTransportEventHandle? _handle;
  Timer? _pollTimer;
  RemoteTransportStateHandler? _onState;
  RemoteTransportEnvelopeHandler? _onEnvelope;
  String _kind = RemoteTransportKind.iroh;

  @override
  String get kind => _kind;

  @override
  set onState(RemoteTransportStateHandler? handler) => _onState = handler;

  @override
  set onEnvelope(RemoteTransportEnvelopeHandler? handler) =>
      _onEnvelope = handler;

  @override
  Future<void> connect(StoredDevice device) async {
    await close();
    final connected = Completer<void>();
    final config = _controllerTransportConfig(device);
    final summary = codux_protocol_ffi.controllerTransportConfigSummary(config);
    _kind = '${summary['transportKind'] ?? RemoteTransportKind.iroh}';
    _onState?.call('connecting');
    final handle = _handleFactory(config);
    if (handle == null) {
      final error = codux_protocol_ffi.lastError();
      final detail = error.isEmpty ? 'transport-connect' : error;
      _onState?.call('failed:$detail');
      throw StateError('Failed to connect remote transport: $detail');
    }
    _handle = handle;
    _drainEvents(connected: connected);
    // Must outlast the Rust side's worst-case cold start, or a slow-but-
    // progressing connect gets killed here and retried from scratch (a fresh
    // endpoint that pays the relay-registration cost all over again). The Rust
    // controller waits up to 12s for endpoint.online() THEN up to 15s for the
    // dial — ~27s — so give it 30s. A genuinely dead peer still surfaces a
    // Rust-side error well before this cap, so we don't actually wait the full
    // 30s on failure.
    await connected.future.timeout(
      const Duration(seconds: 30),
      onTimeout: () {
        throw StateError('Failed to connect remote transport: timed out');
      },
    );
  }

  @override
  Future<bool> send(Map<String, dynamic> envelope) async {
    return _handle?.send(envelope) ?? false;
  }

  @override
  Future<bool> sendTerminal(Map<String, dynamic> envelope) async {
    return _handle?.sendTerminal(envelope) ?? false;
  }

  @override
  Future<bool> sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  }) async {
    return _handle?.sendTerminalUpload(
          deviceId: deviceId,
          sessionId: sessionId,
          name: name,
          mime: mime,
          kind: kind,
          bytes: bytes,
        ) ??
        false;
  }

  @override
  Future<void> close() async {
    _pollTimer?.cancel();
    _pollTimer = null;
    final handle = _handle;
    _handle = null;
    handle?.close();
  }

  void _schedulePoll(Duration delay, {Completer<void>? connected}) {
    _pollTimer?.cancel();
    _pollTimer = _pollTimerFactory(delay, () {
      _pollTimer = null;
      _drainEvents(connected: connected);
    });
  }

  void _drainEvents({Completer<void>? connected}) {
    final handle = _handle;
    if (handle == null || handle.isClosed) return;
    var drained = 0;
    for (; drained < _remoteTransportPollBatchSize; drained += 1) {
      if (!identical(_handle, handle) || handle.isClosed) return;
      Map<String, dynamic>? event;
      try {
        event = handle.pollEvent();
      } on StateError {
        if (identical(_handle, handle)) {
          _pollTimer?.cancel();
          _pollTimer = null;
          _handle = null;
        }
        return;
      }
      if (event == null) break;
      final kind = '${event['kind'] ?? ''}';
      if (kind == 'state') {
        final state = '${event['state'] ?? ''}';
        final parsed = RemoteTransportStateEvent.parse(state);
        if (parsed.isConnected && connected?.isCompleted == false) {
          connected?.complete();
        } else if (parsed.isClosed && connected?.isCompleted == false) {
          connected?.completeError(
            StateError('Failed to connect remote transport: $state'),
          );
        }
        _onState?.call(state);
      } else if (kind == 'message') {
        final data = '${event['data'] ?? ''}';
        final decoded = jsonDecode(data);
        if (decoded is Map<String, dynamic>) {
          _onEnvelope?.call(decoded, data);
        } else if (decoded is Map) {
          _onEnvelope?.call(Map<String, dynamic>.from(decoded), data);
        }
      } else if (kind == 'log') {
        CoduxLog.info('[codux-flutter-transport] ${event['message'] ?? ''}');
      }
    }
    if (!identical(_handle, handle) || handle.isClosed) return;
    final delay = drained == _remoteTransportPollBatchSize
        ? Duration.zero
        : drained > 0
        ? _remoteTransportBusyPollDelay
        : _remoteTransportIdlePollDelay;
    _schedulePoll(delay, connected: connected);
  }
}

ControllerTransportEventHandle? _connectFfiTransport(
  Map<String, dynamic> config,
) {
  final handle = codux_protocol_ffi.ControllerTransportHandle.connect(config);
  return handle == null ? null : _FfiControllerTransportHandle(handle);
}

class _FfiControllerTransportHandle implements ControllerTransportEventHandle {
  _FfiControllerTransportHandle(this._inner);

  final codux_protocol_ffi.ControllerTransportHandle _inner;

  @override
  bool get isClosed => _inner.isClosed;

  @override
  bool send(Map<String, dynamic> envelope) => _inner.send(envelope);

  @override
  bool sendTerminal(Map<String, dynamic> envelope) =>
      _inner.sendTerminal(envelope);

  @override
  bool sendTerminalUpload({
    required String deviceId,
    required String sessionId,
    required String name,
    required String mime,
    required String kind,
    required Uint8List bytes,
  }) => _inner.sendTerminalUpload(
    deviceId: deviceId,
    sessionId: sessionId,
    name: name,
    mime: mime,
    kind: kind,
    bytes: bytes,
  );

  @override
  Map<String, dynamic>? pollEvent() => _inner.pollEvent();

  @override
  void close() => _inner.close();
}

Map<String, dynamic> _controllerTransportConfig(StoredDevice device) {
  return {
    'relayUrl': device.server,
    'hostId': device.hostId,
    'deviceId': device.deviceId,
    'deviceToken': device.token,
    'transports': device.transports.map((item) => item.toJson()).toList(),
  };
}
