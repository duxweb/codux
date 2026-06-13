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

  // The decoded screen snapshot is expensive (FFI round-trip + JSON decode +
  // a fresh cell list). The UI reads it once per build, so re-decoding on
  // every frame dominated mobile terminal CPU. The screen only changes through
  // the mutating methods below, so we bump a generation there and memoize the
  // decode until it advances. Returning the same instance also lets the
  // painter's identity-based shouldRepaint skip no-op repaints.
  int _screenGeneration = 0;
  int _cachedScreenGeneration = -1;
  RemoteTerminalScreenSnapshot? _cachedScreenSnapshot;

  /// Maximum number of held live frames kept while awaiting a baseline. A
  /// baseline that never arrives (host torn down mid-request) would otherwise
  /// let this grow without bound; past the cap we drop oldest and require a
  /// fresh baseline rather than replaying stale output.
  static const int _maxHeldLive = 2048;

  String get content => _core.content;
  int get bufferLength => _core.bufferLength;
  int get sequence => _core.sequence;
  bool get awaitingBaseline => _core.isRestoringBaseline;
  bool get isRestoringBaseline => _core.isRestoringBaseline;

  void _invalidateScreen() {
    _screenGeneration++;
    _cachedScreenSnapshot = null;
  }

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
    final cached = _cachedScreenSnapshot;
    if (cached != null && _cachedScreenGeneration == _screenGeneration) {
      return cached;
    }
    final snapshot = _core.screenSnapshot();
    _cachedScreenSnapshot = snapshot;
    _cachedScreenGeneration = _screenGeneration;
    return snapshot;
  }

  void resizeScreen({required int cols, required int rows}) {
    _core.resizeScreen(cols: cols, rows: rows);
    _invalidateScreen();
  }

  void scrollScreenLines(int lines) {
    _core.scrollScreenLines(lines);
    _invalidateScreen();
  }

  void scrollScreenPixels({
    required double pixels,
    required double cellHeight,
  }) {
    _core.scrollScreenPixels(pixels: pixels, cellHeight: cellHeight);
    _invalidateScreen();
  }

  void settleScreenPixelScroll() {
    _core.settleScreenPixelScroll();
    _invalidateScreen();
  }

  void scrollScreenToBottom() {
    _core.scrollScreenToBottom();
    _invalidateScreen();
  }

  void applyHostScroll({
    required String screenData,
    required int displayOffset,
    required int totalLines,
    int marginRows = 0,
    int marginRowsBelow = 0,
  }) {
    _core.applyHostScroll(
      screenData: screenData,
      displayOffset: displayOffset,
      totalLines: totalLines,
      marginRows: marginRows,
      marginRowsBelow: marginRowsBelow,
    );
    _invalidateScreen();
  }

  void resetTransient({bool resetSequence = false}) {
    _core.resetTransient(resetSequence: resetSequence);
    _clearHeldLive();
    _invalidateScreen();
  }

  void requireBaseline() {
    _core.requireBaseline();
    _clearHeldLive();
    _invalidateScreen();
  }

  void setSequence(int sequence) {
    _core.setSequence(sequence);
  }

  bool holdLive({required int? sequence, required T output}) {
    final token = _nextHeldLiveToken++;
    final held = _core.holdLiveToken(sequence: sequence, token: token);
    if (held) {
      _heldLiveByToken[token] = output;
      // Bound the held-live buffer: a baseline that never lands must not let
      // this grow forever. Drop the oldest held frames past the cap.
      while (_heldLiveByToken.length > _maxHeldLive) {
        final oldest = _heldLiveByToken.keys.first;
        _heldLiveByToken.remove(oldest);
      }
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
    _invalidateScreen();
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
    _invalidateScreen();
  }

  void clear() {
    _core.clear();
    _clearHeldLive();
    _invalidateScreen();
  }

  void dispose() {
    _core.dispose();
    _clearHeldLive();
    _cachedScreenSnapshot = null;
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

  /// Move a session to the most-recently-used end of the order so it survives
  /// eviction. Called when a session is (re)bound / becomes active.
  void touch(String sessionId) {
    final session = _sessions.remove(sessionId);
    if (session != null) _sessions[sessionId] = session;
  }

  /// Evict the least-recently-used sessions until at most [maxSessions] remain,
  /// never evicting [keep] (the active session). Each evicted session disposes
  /// its core (freeing its headless-screen worker threads). Returns the evicted
  /// ids so the caller can drop their sequencer/assembler/gap bookkeeping.
  List<String> evictExcept(String keep, {required int maxSessions}) {
    final evicted = <String>[];
    while (_sessions.length > maxSessions) {
      String? victim;
      for (final id in _sessions.keys) {
        if (id != keep) {
          victim = id;
          break;
        }
      }
      if (victim == null) break;
      _sessions.remove(victim)?.dispose();
      evicted.add(victim);
    }
    return evicted;
  }

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

  void scrollScreenPixels(
    String sessionId, {
    required double pixels,
    required double cellHeight,
  }) {
    _sessions[sessionId]?.scrollScreenPixels(
      pixels: pixels,
      cellHeight: cellHeight,
    );
  }

  void settleScreenPixelScroll(String sessionId) {
    _sessions[sessionId]?.settleScreenPixelScroll();
  }

  void scrollScreenToBottom(String sessionId) {
    _sessions[sessionId]?.scrollScreenToBottom();
  }

  void applyHostScroll(
    String sessionId, {
    required String screenData,
    required int displayOffset,
    required int totalLines,
    int marginRows = 0,
    int marginRowsBelow = 0,
  }) {
    _sessions[sessionId]?.applyHostScroll(
      screenData: screenData,
      displayOffset: displayOffset,
      totalLines: totalLines,
      marginRows: marginRows,
      marginRowsBelow: marginRowsBelow,
    );
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
