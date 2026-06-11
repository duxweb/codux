import '../models/remote_models.dart';
import 'log_service.dart';
import 'remote_pty_session.dart';
import 'terminal_buffer_assembler.dart';
import 'terminal_output_resync.dart';
import 'terminal_output_sequencer.dart';
import 'terminal_payload_codec.dart';

enum RemoteTerminalBufferPhase { idle, requesting, receiving, rendering }

enum RemoteTerminalOutputEffectKind {
  loading,
  ack,
  markBufferReceived,
  sessionUpdated,
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

  factory RemoteTerminalOutputEffect.loading({
    required bool loading,
    RemoteTerminalBufferPhase phase = RemoteTerminalBufferPhase.requesting,
    double? progress,
  }) => RemoteTerminalOutputEffect._(
    kind: RemoteTerminalOutputEffectKind.loading,
    loading: loading,
    phase: phase,
    progress: progress,
  );

  factory RemoteTerminalOutputEffect.ack({
    required String sessionId,
    required int? outputSeq,
    required int? bufferLength,
  }) => RemoteTerminalOutputEffect._(
    kind: RemoteTerminalOutputEffectKind.ack,
    sessionId: sessionId,
    outputSeq: outputSeq,
    bufferLength: bufferLength,
  );

  factory RemoteTerminalOutputEffect.markBufferReceived(String sessionId) =>
      RemoteTerminalOutputEffect._(
        kind: RemoteTerminalOutputEffectKind.markBufferReceived,
        sessionId: sessionId,
      );

  factory RemoteTerminalOutputEffect.sessionUpdated(String sessionId) =>
      RemoteTerminalOutputEffect._(
        kind: RemoteTerminalOutputEffectKind.sessionUpdated,
        sessionId: sessionId,
      );

  final RemoteTerminalOutputEffectKind kind;
  final String? sessionId;
  final int? outputSeq;
  final int? bufferLength;
  final double? progress;
  final RemoteTerminalBufferPhase? phase;
  final bool loading;
}

class RemoteTerminalOutputController {
  RemoteTerminalOutputController({
    int maxBufferChars = 200000,
    int maxCachedChars = 2000000,
  }) : _ptySessions = RemotePtySessionStore<RelayEnvelope>(
         maxCachedChars: maxCachedChars,
       ),
       _assembler = TerminalBufferAssembler(maxChars: maxBufferChars);

  final RemotePtySessionStore<RelayEnvelope> _ptySessions;
  final TerminalBufferAssembler _assembler;
  final TerminalOutputSequencer _sequencer = TerminalOutputSequencer();
  final Map<String, String> _activeBufferRequestBySession = {};
  final Set<String> _restoreBufferRequestIds = {};

  String? cachedOutput(String sessionId) => _ptySessions.content(sessionId);

  RemoteTerminalScreenSnapshot? screenSnapshot(String sessionId) =>
      _ptySessions.screenSnapshot(sessionId);

  bool hasCachedOutput(String sessionId) =>
      _ptySessions.content(sessionId) != null;

  int bufferOffset(String sessionId) => _ptySessions.bufferLength(sessionId);

  int sequenceFor(String sessionId) => _ptySessions.sequence(sessionId);

  void resizeScreen(String sessionId, {required int cols, required int rows}) {
    _ptySessions.resizeScreen(sessionId, cols: cols, rows: rows);
  }

  void scrollScreenLines(String sessionId, int lines) {
    _ptySessions.scrollScreenLines(sessionId, lines);
  }

  void scrollScreenToBottom(String sessionId) {
    _ptySessions.scrollScreenToBottom(sessionId);
  }

  String? activeBufferRequestId(String sessionId) =>
      _activeBufferRequestBySession[sessionId];

  bool hasActiveBufferRequest(String sessionId) =>
      _activeBufferRequestBySession.containsKey(sessionId);

  bool startBufferRequest(
    String sessionId,
    String requestId, {
    bool requireBaseline = false,
    bool resetAssembler = true,
  }) {
    if (sessionId.trim().isEmpty || requestId.trim().isEmpty) return false;
    final activeRequestId = _activeBufferRequestBySession[sessionId];
    if (activeRequestId != null && activeRequestId != requestId) {
      CoduxLog.debug(
        '[codux-flutter-output] keep active buffer request session=$sessionId active=$activeRequestId ignored=$requestId',
      );
      return false;
    }
    _activeBufferRequestBySession[sessionId] = requestId;
    final requestKey = _bufferRequestKey(sessionId, requestId);
    if (requireBaseline) {
      _restoreBufferRequestIds.add(requestKey);
    } else {
      _restoreBufferRequestIds.remove(requestKey);
    }
    if (resetAssembler) {
      _assembler.remove(sessionId);
    }
    if (requireBaseline) {
      _ptySessions.session(sessionId).requireBaseline();
    }
    return true;
  }

  void bindSession(String sessionId, {required bool requireBaseline}) {
    if (sessionId.trim().isEmpty) return;
    _sequencer.remove(sessionId);
    _assembler.remove(sessionId);
    final session = _ptySessions.session(sessionId);
    if (requireBaseline) {
      session.requireBaseline();
    } else {
      session.resetTransient();
    }
  }

  void removeSession(String sessionId) {
    _ptySessions.remove(sessionId);
    _activeBufferRequestBySession.remove(sessionId);
    _restoreBufferRequestIds.removeWhere(
      (requestId) => requestId.startsWith('$sessionId:'),
    );
    _assembler.remove(sessionId);
    _sequencer.remove(sessionId);
  }

  void resetTransient() {
    _assembler.reset();
    _activeBufferRequestBySession.clear();
    _restoreBufferRequestIds.clear();
  }

  void resetSessionTransient(String sessionId, {bool resetSequence = false}) {
    _assembler.remove(sessionId);
    if (resetSequence) _sequencer.remove(sessionId);
    _ptySessions
        .session(sessionId)
        .resetTransient(resetSequence: resetSequence);
  }

  void resetAll() {
    _ptySessions.clear();
    _activeBufferRequestBySession.clear();
    _restoreBufferRequestIds.clear();
    _assembler.reset();
    _sequencer.reset();
  }

  void dispose() {
    _ptySessions.clear();
    _activeBufferRequestBySession.clear();
    _restoreBufferRequestIds.clear();
    _assembler.reset();
    _sequencer.dispose();
  }

  List<RemoteTerminalOutputEffect> accept(
    RelayEnvelope message, {
    required String? activeSessionId,
  }) {
    return _accept(
      message,
      activeSessionId: activeSessionId,
      replayingHeldLive: false,
    );
  }

  List<RemoteTerminalOutputEffect> _accept(
    RelayEnvelope message, {
    required String? activeSessionId,
    required bool replayingHeldLive,
  }) {
    var payload = message.payload;
    if (payload is! Map || payload['data'] == null) return const [];
    final sessionId = message.sessionId;
    if (sessionId == null || sessionId.trim().isEmpty) {
      return const [];
    }
    final isActiveSession = sessionId == activeSessionId;
    final hadCachedOutputAtStart = hasCachedOutput(sessionId);
    if (!isActiveSession) {
      CoduxLog.debug(
        '[codux-flutter-output] cache inactive session=${message.sessionId ?? ''} active=${activeSessionId ?? ''}',
      );
    }
    final incomingRequestId = _payloadStringValue(payload['requestId']);
    final activeRequestId = _activeBufferRequestBySession[sessionId];
    if (payload['buffer'] == true &&
        activeRequestId != null &&
        (hadCachedOutputAtStart || incomingRequestId != null) &&
        incomingRequestId != activeRequestId) {
      CoduxLog.debug(
        '[codux-flutter-output] skip stale buffer request=$incomingRequestId active=$activeRequestId session=$sessionId',
      );
      return [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: _intPayloadValue(payload['outputSeq']),
          bufferLength: _intPayloadValue(payload['bufferLength']),
        ),
      ];
    }
    if (payload['buffer'] == true &&
        activeRequestId == null &&
        incomingRequestId != null &&
        incomingRequestId.isNotEmpty &&
        hasCachedOutput(sessionId)) {
      CoduxLog.debug(
        '[codux-flutter-output] skip stale untracked buffer request=$incomingRequestId session=$sessionId',
      );
      return [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: _intPayloadValue(payload['outputSeq']),
          bufferLength: _intPayloadValue(payload['bufferLength']),
        ),
      ];
    }
    if (payload['buffer'] == true &&
        activeRequestId == null &&
        incomingRequestId != null &&
        incomingRequestId.isNotEmpty) {
      _activeBufferRequestBySession[sessionId] = incomingRequestId;
      if ((_ptySessions.content(sessionId) ?? '').isEmpty) {
        _restoreBufferRequestIds.add(
          _bufferRequestKey(sessionId, incomingRequestId),
        );
      }
    }
    if (payload['buffer'] == true &&
        hadCachedOutputAtStart &&
        _activeBufferRequestBySession[sessionId] == null) {
      final payloadOffset =
          _intPayloadValue(payload['startOffset']) ??
          _intPayloadValue(payload['offset']);
      final payloadBufferLength = _intPayloadValue(payload['bufferLength']);
      if ((payloadOffset == 0 &&
              payloadBufferLength != null &&
              payloadBufferLength > bufferOffset(sessionId)) ||
          (payloadOffset != null && payloadOffset > 0)) {
        _assembler.remove(sessionId);
        CoduxLog.debug(
          '[codux-flutter-output] skip stale buffer before assembly session=$sessionId offset=${payloadOffset ?? 0} length=${payloadBufferLength ?? 0}',
        );
        return [
          RemoteTerminalOutputEffect.ack(
            sessionId: sessionId,
            outputSeq: _intPayloadValue(payload['outputSeq']),
            bufferLength: payloadBufferLength,
          ),
        ];
      }
    }

    final assembly = _assembler.accept(sessionId: sessionId, payload: payload);
    if (!assembly.ready) {
      CoduxLog.debug(
        '[codux-flutter-output] buffer chunk progress=${assembly.progress ?? 0} session=$sessionId',
      );
      if (assembly.progress == null) return const [];
      if (!isActiveSession) return const [];
      return [
        RemoteTerminalOutputEffect.loading(
          loading: true,
          phase: RemoteTerminalBufferPhase.receiving,
          progress: assembly.progress,
        ),
      ];
    }

    payload = assembly.payload ?? payload;
    final decoded = decodeTerminalOutputPayload(payload);
    final raw = decoded.data;
    final isBuffer = decoded.isBuffer;
    final outputSeq = _intPayloadValue(payload['outputSeq']);
    final activeRequestIdAfterAssembly =
        _activeBufferRequestBySession[sessionId];
    if (isBuffer &&
        hadCachedOutputAtStart &&
        activeRequestIdAfterAssembly == null &&
        decoded.offset == 0 &&
        decoded.bufferLength != null &&
        decoded.bufferLength! > bufferOffset(sessionId)) {
      _assembler.remove(sessionId);
      CoduxLog.debug(
        '[codux-flutter-output] skip duplicate baseline session=$sessionId offset=0 length=${decoded.bufferLength} local=${bufferOffset(sessionId)}',
      );
      return [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: outputSeq,
          bufferLength: decoded.bufferLength,
        ),
      ];
    }
    if (isBuffer &&
        hadCachedOutputAtStart &&
        activeRequestIdAfterAssembly == null &&
        decoded.offset != null &&
        decoded.offset! > 0) {
      _assembler.remove(sessionId);
      CoduxLog.debug(
        '[codux-flutter-output] skip stale buffer page session=$sessionId offset=${decoded.offset} length=${decoded.bufferLength ?? 0}',
      );
      return [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: outputSeq,
          bufferLength: decoded.bufferLength,
        ),
      ];
    }
    if (isBuffer) {
      CoduxLog.info(
        '[codux-flutter-output] buffer bytes=${raw.codeUnits.length} offset=${decoded.offset ?? 0} length=${decoded.bufferLength ?? 0} truncated=${decoded.truncated} seq=${outputSeq ?? 0} session=$sessionId',
      );
    }

    final ptySession = _ptySessions.session(sessionId);
    if (!replayingHeldLive &&
        !isBuffer &&
        decoded.screenData == null &&
        ptySession.holdLive(sequence: outputSeq, output: message)) {
      CoduxLog.debug(
        '[codux-flutter-output] hold live output before baseline seq=${outputSeq ?? 0} session=$sessionId',
      );
      return [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: outputSeq,
          bufferLength: decoded.bufferLength,
        ),
      ];
    }
    if (!isActiveSession && !isBuffer) {
      final resync = observeTerminalOutputForResync(
        sequencer: _sequencer,
        sessionId: sessionId,
        isBuffer: false,
        outputSeq: outputSeq,
        offset: null,
      );
      var heldLive = const <RelayEnvelope>[];
      if (resync.render && (raw.isNotEmpty || decoded.screenData != null)) {
        heldLive = _applyLiveToSession(
          sessionId,
          raw,
          decoded.screenData,
          decoded.bufferLength,
          resync.ack,
        );
        if (decoded.screenData != null) {
          _activeBufferRequestBySession.remove(sessionId);
          _removeRestoreRequest(sessionId);
          _assembler.remove(sessionId);
        }
      }
      final effects = [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: resync.ack,
          bufferLength: decoded.bufferLength,
        ),
      ];
      for (final held in heldLive) {
        effects.addAll(
          _accept(
            held,
            activeSessionId: activeSessionId,
            replayingHeldLive: true,
          ),
        );
      }
      return effects;
    }

    final resync = observeTerminalOutputForResync(
      sequencer: _sequencer,
      sessionId: sessionId,
      isBuffer: isBuffer,
      outputSeq: outputSeq,
      offset: decoded.offset,
      resetsSequence: decoded.tail,
    );
    if (!resync.render) {
      CoduxLog.debug(
        '[codux-flutter-output] drop duplicate seq=${resync.ack} session=$sessionId',
      );
      return [
        RemoteTerminalOutputEffect.ack(
          sessionId: sessionId,
          outputSeq: resync.ack,
          bufferLength: decoded.bufferLength,
        ),
      ];
    }

    CoduxLog.debug(
      '[codux-flutter-output] bytes=${raw.codeUnits.length} buffer=$isBuffer session=${message.sessionId ?? ''}',
    );

    final effects = <RemoteTerminalOutputEffect>[];
    var heldLive = const <RelayEnvelope>[];

    if (isBuffer) {
      final activeRequestId = _activeBufferRequestBySession[sessionId];
      final localCacheEmpty = (_ptySessions.content(sessionId) ?? '').isEmpty;
      final isRestoreRequest =
          activeRequestId != null &&
          _restoreBufferRequestIds.contains(
            _bufferRequestKey(sessionId, activeRequestId),
          );
      final isBaselineRestore =
          decoded.tail ||
          ptySession.awaitingBaseline ||
          isRestoreRequest ||
          localCacheEmpty;
      var renderData = raw;
      if (isBaselineRestore) {
        heldLive = _replaceSessionFromBaseline(
          sessionId,
          renderData,
          decoded.screenData,
          decoded.bufferLength,
          outputSeq,
        );
      }

      if (isBaselineRestore || _ptySessions.content(sessionId) == null) {
        if (!isBaselineRestore) {
          _replaceSessionFromBaseline(
            sessionId,
            renderData,
            decoded.screenData,
            decoded.bufferLength,
            outputSeq,
          );
        }
        _activeBufferRequestBySession.remove(sessionId);
        _removeRestoreRequest(sessionId);
        if (isActiveSession) {
          effects.add(RemoteTerminalOutputEffect.sessionUpdated(sessionId));
          effects.add(RemoteTerminalOutputEffect.markBufferReceived(sessionId));
        }
      } else {
        heldLive = _applyLiveToSession(
          sessionId,
          raw,
          decoded.screenData,
          decoded.bufferLength,
          resync.ack,
        );
        _activeBufferRequestBySession.remove(sessionId);
        _removeRestoreRequest(sessionId);
        if (isActiveSession) {
          effects.add(RemoteTerminalOutputEffect.sessionUpdated(sessionId));
          effects.add(RemoteTerminalOutputEffect.markBufferReceived(sessionId));
        }
      }
    } else if ((raw.isNotEmpty || decoded.screenData != null) &&
        isActiveSession) {
      effects.add(RemoteTerminalOutputEffect.loading(loading: false));
    }

    if (!isBuffer && (raw.isNotEmpty || decoded.screenData != null)) {
      heldLive = _applyLiveToSession(
        sessionId,
        raw,
        decoded.screenData,
        decoded.bufferLength,
        resync.ack,
      );
      if (decoded.screenData != null) {
        _activeBufferRequestBySession.remove(sessionId);
        _removeRestoreRequest(sessionId);
        _assembler.remove(sessionId);
      }
      if (isActiveSession) {
        effects.add(RemoteTerminalOutputEffect.sessionUpdated(sessionId));
      }
    }

    effects.add(
      RemoteTerminalOutputEffect.ack(
        sessionId: sessionId,
        outputSeq: resync.ack,
        bufferLength: decoded.bufferLength,
      ),
    );

    if (heldLive.isNotEmpty) {
      for (final held in heldLive) {
        effects.addAll(
          _accept(
            held,
            activeSessionId: activeSessionId,
            replayingHeldLive: true,
          ),
        );
      }
    }

    return effects;
  }

  String _bufferRequestKey(String sessionId, String requestId) =>
      '$sessionId:$requestId';

  void _removeRestoreRequest(String sessionId) {
    _restoreBufferRequestIds.removeWhere(
      (requestId) => requestId.startsWith('$sessionId:'),
    );
  }

  List<RelayEnvelope> _replaceSessionFromBaseline(
    String sessionId,
    String data,
    String? screenData,
    int? bufferLength,
    int? outputSeq,
  ) {
    return _ptySessions
        .session(sessionId)
        .replaceFromBaseline(
          content: data,
          screenData: screenData,
          bufferLength: bufferLength,
          sequence: outputSeq,
        );
  }

  void _appendLiveToSession(
    String sessionId,
    String data,
    String? screenData,
    int? bufferLength,
    int? outputSeq,
  ) {
    _ptySessions
        .session(sessionId)
        .appendLive(
          data: data,
          screenData: screenData,
          bufferLength: bufferLength,
          sequence: outputSeq,
        );
  }

  List<RelayEnvelope> _applyLiveToSession(
    String sessionId,
    String data,
    String? screenData,
    int? bufferLength,
    int? outputSeq,
  ) {
    final session = _ptySessions.session(sessionId);
    if (screenData != null && session.awaitingBaseline) {
      final existing = _ptySessions.content(sessionId) ?? '';
      return session.replaceFromBaseline(
        content: '$existing$data',
        screenData: screenData,
        bufferLength: bufferLength,
        sequence: outputSeq,
      );
    }
    _appendLiveToSession(sessionId, data, screenData, bufferLength, outputSeq);
    return const [];
  }
}

int? _intPayloadValue(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  return int.tryParse('${value ?? ''}');
}

String? _payloadStringValue(Object? value) {
  final text = value?.toString().trim();
  return text == null || text.isEmpty ? null : text;
}
