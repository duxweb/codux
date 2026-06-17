import 'dart:convert';

import 'package:codux_protocol_ffi/codux_protocol_ffi.dart';

import '../models/remote_models.dart';

enum RemoteTerminalBufferPhase { idle, requesting, receiving, rendering }

enum RemoteTerminalOutputEffectKind {
  loading,
  ack,
  markBufferReceived,
  sessionUpdated,
  requestBaselineResync,
}

class RemoteTerminalOutputEffect {
  const RemoteTerminalOutputEffect._({
    required this.kind,
    this.sessionId,
    this.outputSeq,
    this.bufferLength,
    this.progress,
    this.phase,
    this.loading = false,
  });

  factory RemoteTerminalOutputEffect.fromJson(Map<String, dynamic> json) {
    return RemoteTerminalOutputEffect._(
      kind: _kindFromName('${json['kind'] ?? ''}'),
      sessionId: json['sessionId'] as String?,
      outputSeq: _intOrNull(json['outputSeq']),
      bufferLength: _intOrNull(json['bufferLength']),
      progress: _doubleOrNull(json['progress']),
      phase: _phaseFromName(json['phase'] as String?),
      loading: json['loading'] == true,
    );
  }

  final RemoteTerminalOutputEffectKind kind;
  final String? sessionId;
  final int? outputSeq;
  final int? bufferLength;
  final double? progress;
  final RemoteTerminalBufferPhase? phase;
  final bool loading;
}

/// Consumer-side terminal output controller. The orchestration state machine
/// and the per-session remote PTY state live in the shared Rust core
/// (`RemoteTerminalOutputRouter`); this is a thin Dart facade over it so the
/// rest of the app keeps the same API.
class RemoteTerminalOutputController {
  RemoteTerminalOutputController({
    int maxBufferChars = 200000,
    // Byte safety ceiling only. The cache is primarily bounded by a trailing
    // line budget in the Rust core (matching the native emulator's ~500-line
    // scrollback); this ceiling just caps pathologically long lines and is
    // kept above the host baseline window (maxBufferChars) so a full baseline
    // is never truncated by bytes.
    int maxCachedChars = 262144,
  }) : _router = RemoteOutputRouter(
         maxBufferChars: maxBufferChars,
         maxCachedChars: maxCachedChars,
       );

  final RemoteOutputRouter _router;

  String? cachedOutput(String sessionId) => _router.content(sessionId);

  String? nativeRenderOutput(String sessionId) =>
      _router.nativeRenderContent(sessionId);

  bool hasCachedOutput(String sessionId) => _router.hasCachedOutput(sessionId);

  int bufferOffset(String sessionId) => _router.bufferOffset(sessionId);

  int sequenceFor(String sessionId) => _router.sequenceFor(sessionId);

  /// True when a live output gap was observed for [sessionId] and no baseline
  /// has repaired it yet; such a session must not skip its baseline request.
  bool hasSequenceGap(String sessionId) => _router.hasSequenceGap(sessionId);

  String? activeBufferRequestId(String sessionId) =>
      _router.activeBufferRequestId(sessionId);

  bool hasActiveBufferRequest(String sessionId) =>
      _router.hasActiveBufferRequest(sessionId);

  bool startBufferRequest(
    String sessionId,
    String requestId, {
    bool requireBaseline = false,
    bool resetAssembler = true,
    bool replaceActive = false,
  }) {
    return _router.startBufferRequest(
      sessionId,
      requestId,
      requireBaseline: requireBaseline,
      resetAssembler: resetAssembler,
      replaceActive: replaceActive,
    );
  }

  void bindSession(String sessionId, {required bool requireBaseline}) {
    _router.bindSession(sessionId, requireBaseline: requireBaseline);
  }

  void removeSession(String sessionId) {
    _router.removeSession(sessionId);
  }

  /// Bound live remote pty sessions so worker threads from previously visited
  /// projects do not accumulate. Returns the evicted session ids.
  List<String> evictInactiveSessions(
    String activeSessionId, {
    int maxSessions = 8,
  }) {
    final evicted = _router.evictInactive(
      activeSessionId,
      maxSessions: maxSessions,
    );
    return evicted;
  }

  void resetTransient() {
    _router.resetTransient();
  }

  void resetSessionTransient(String sessionId, {bool resetSequence = false}) {
    _router.resetSessionTransient(sessionId, resetSequence: resetSequence);
  }

  void resetAll() {
    _router.resetAll();
  }

  void dispose() {
    _router.dispose();
  }

  List<RemoteTerminalOutputEffect> accept(
    RelayEnvelope message, {
    required String? activeSessionId,
  }) {
    final effects = _router.accept(
      jsonEncode(message.toJson()),
      activeSessionId,
    );
    return effects
        .map(
          (effect) => RemoteTerminalOutputEffect.fromJson(
            Map<String, dynamic>.from(effect as Map),
          ),
        )
        .toList();
  }
}

RemoteTerminalOutputEffectKind _kindFromName(String name) {
  switch (name) {
    case 'loading':
      return RemoteTerminalOutputEffectKind.loading;
    case 'ack':
      return RemoteTerminalOutputEffectKind.ack;
    case 'markBufferReceived':
      return RemoteTerminalOutputEffectKind.markBufferReceived;
    case 'sessionUpdated':
      return RemoteTerminalOutputEffectKind.sessionUpdated;
    case 'requestBaselineResync':
      return RemoteTerminalOutputEffectKind.requestBaselineResync;
    default:
      return RemoteTerminalOutputEffectKind.ack;
  }
}

RemoteTerminalBufferPhase? _phaseFromName(String? name) {
  switch (name) {
    case 'idle':
      return RemoteTerminalBufferPhase.idle;
    case 'requesting':
      return RemoteTerminalBufferPhase.requesting;
    case 'receiving':
      return RemoteTerminalBufferPhase.receiving;
    case 'rendering':
      return RemoteTerminalBufferPhase.rendering;
    default:
      return null;
  }
}

int? _intOrNull(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  return int.tryParse('${value ?? ''}');
}

double? _doubleOrNull(Object? value) {
  if (value is double) return value;
  if (value is num) return value.toDouble();
  return double.tryParse('${value ?? ''}');
}
