import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_terminal_core;

typedef RemoteTerminalScreenSnapshot =
    codux_terminal_core.TerminalScreenSnapshot;
typedef RemoteTerminalScreenCell = codux_terminal_core.TerminalScreenCell;

class RemotePtySnapshot {
  const RemotePtySnapshot({
    required this.sessionId,
    required this.content,
    required this.bufferLength,
    required this.sequence,
  });

  final String sessionId;
  final String content;
  final int bufferLength;
  final int sequence;
}

class RemotePtySession<T> {
  RemotePtySession(this.sessionId, {required this.maxCachedChars})
    : _core = codux_terminal_core.TerminalCoreSession(
        sessionId: sessionId,
        maxCachedChars: maxCachedChars,
      );

  final String sessionId;
  final int maxCachedChars;
  final codux_terminal_core.TerminalCoreSession _core;
  final Map<int, T> _heldLiveByToken = {};
  int _nextHeldLiveToken = 1;

  String get content => _core.content;
  int get bufferLength => _core.bufferLength;
  int get sequence => _core.sequence;
  bool get awaitingBaseline => _core.isRestoringBaseline;
  bool get isRestoringBaseline => _core.isRestoringBaseline;

  RemotePtySnapshot snapshot() {
    final coreSnapshot = _core.snapshot();
    return RemotePtySnapshot(
      sessionId: coreSnapshot.sessionId,
      content: coreSnapshot.content,
      bufferLength: coreSnapshot.bufferLength,
      sequence: coreSnapshot.sequence,
    );
  }

  RemoteTerminalScreenSnapshot screenSnapshot() {
    return _core.screenSnapshot();
  }

  void resizeScreen({required int cols, required int rows}) {
    _core.resizeScreen(cols: cols, rows: rows);
  }

  void scrollScreenLines(int lines) {
    _core.scrollScreenLines(lines);
  }

  void scrollScreenToBottom() {
    _core.scrollScreenToBottom();
  }

  void resetTransient({bool resetSequence = false}) {
    _core.resetTransient(resetSequence: resetSequence);
    _clearHeldLive();
  }

  void requireBaseline() {
    _core.requireBaseline();
    _clearHeldLive();
  }

  void setSequence(int sequence) {
    _core.setSequence(sequence);
  }

  bool holdLive({required int? sequence, required T output}) {
    final token = _nextHeldLiveToken++;
    final held = _core.holdLiveToken(sequence: sequence, token: token);
    if (held) {
      _heldLiveByToken[token] = output;
    }
    return held;
  }

  RemotePtyBaselinePageResult acceptBaselinePage({
    required String data,
    required int offset,
    required int? bufferLength,
    required bool truncated,
  }) {
    final corePage = _core.acceptBaselinePage(
      data: data,
      offset: offset,
      bufferLength: bufferLength,
      truncated: truncated,
    );
    final duplicate =
        corePage.duplicate ||
        (!corePage.accepted &&
            offset + data.runes.length <= corePage.nextOffset);
    return RemotePtyBaselinePageResult(
      accepted: corePage.accepted,
      duplicate: duplicate,
      ready: corePage.ready,
      data: corePage.data,
      nextOffset: corePage.nextOffset,
      progress: corePage.progress,
    );
  }

  List<T> replaceFromBaseline({
    required String content,
    String? screenData,
    required int? bufferLength,
    required int? sequence,
  }) {
    final replayTokens = _core.replaceFromBaseline(
      content: content,
      screenData: screenData,
      bufferLength: bufferLength,
      sequence: sequence,
    );
    final replay = <T>[];
    for (final token in replayTokens) {
      final output = _heldLiveByToken[token];
      if (output != null) replay.add(output);
    }
    _clearHeldLive();
    return replay;
  }

  void appendLive({
    required String data,
    String? screenData,
    required int? bufferLength,
    required int? sequence,
  }) {
    _core.appendLive(
      data: data,
      screenData: screenData,
      bufferLength: bufferLength,
      sequence: sequence,
    );
  }

  void clear() {
    _core.clear();
    _clearHeldLive();
  }

  void dispose() {
    _core.dispose();
    _clearHeldLive();
  }

  void _clearHeldLive() {
    _heldLiveByToken.clear();
    _nextHeldLiveToken = 1;
  }
}

class RemotePtyBaselinePageResult {
  const RemotePtyBaselinePageResult({
    required this.accepted,
    required this.duplicate,
    required this.ready,
    required this.data,
    required this.nextOffset,
    required this.progress,
  });

  final bool accepted;
  final bool duplicate;
  final bool ready;
  final String data;
  final int nextOffset;
  final double? progress;
}

class RemotePtySessionStore<T> {
  RemotePtySessionStore({required this.maxCachedChars});

  final int maxCachedChars;
  final Map<String, RemotePtySession<T>> _sessions = {};

  RemotePtySession<T> session(String sessionId) => _sessions.putIfAbsent(
    sessionId,
    () => RemotePtySession<T>(sessionId, maxCachedChars: maxCachedChars),
  );

  RemotePtySnapshot? snapshot(String sessionId) =>
      _sessions[sessionId]?.snapshot();

  RemoteTerminalScreenSnapshot? screenSnapshot(String sessionId) =>
      _sessions[sessionId]?.screenSnapshot();

  String? content(String sessionId) {
    final content = _sessions[sessionId]?.content;
    return content == null || content.isEmpty ? null : content;
  }

  int bufferLength(String sessionId) => _sessions[sessionId]?.bufferLength ?? 0;
  int sequence(String sessionId) => _sessions[sessionId]?.sequence ?? 0;

  void resizeScreen(String sessionId, {required int cols, required int rows}) {
    _sessions[sessionId]?.resizeScreen(cols: cols, rows: rows);
  }

  void scrollScreenLines(String sessionId, int lines) {
    _sessions[sessionId]?.scrollScreenLines(lines);
  }

  void scrollScreenToBottom(String sessionId) {
    _sessions[sessionId]?.scrollScreenToBottom();
  }

  void remove(String sessionId) {
    _sessions.remove(sessionId)?.dispose();
  }

  void clear() {
    for (final session in _sessions.values) {
      session.dispose();
    }
    _sessions.clear();
  }
}
