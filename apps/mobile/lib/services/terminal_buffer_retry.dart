import 'dart:async';

typedef TerminalBufferTimerFactory =
    Timer Function(Duration delay, void Function() callback);

class TerminalBufferRetryCoordinator {
  TerminalBufferRetryCoordinator({
    this.retryDelay = const Duration(milliseconds: 900),
    this.maxRetries = 3,
    this.onRetryExhausted,
    TerminalBufferTimerFactory? timerFactory,
  }) : _timerFactory = timerFactory ?? Timer.new;

  final Duration retryDelay;
  final int maxRetries;
  final void Function(String sessionId)? onRetryExhausted;
  final TerminalBufferTimerFactory _timerFactory;

  Timer? _retryTimer;
  String _lastBufferedSessionId = '';
  String? _pendingSessionId;
  int _retryAttempt = 0;

  String get lastBufferedSessionId => _lastBufferedSessionId;
  String? get pendingSessionId => _pendingSessionId;
  int get retryAttempt => _retryAttempt;

  void resetLastBuffered() {
    _lastBufferedSessionId = '';
  }

  void reset() {
    _retryTimer?.cancel();
    _retryTimer = null;
    _lastBufferedSessionId = '';
    _pendingSessionId = null;
    _retryAttempt = 0;
  }

  void resetSession(String sessionId) {
    if (_pendingSessionId == sessionId) {
      _retryTimer?.cancel();
      _retryTimer = null;
      _pendingSessionId = null;
      _retryAttempt = 0;
    }
    if (_lastBufferedSessionId == sessionId) {
      _lastBufferedSessionId = '';
    }
  }

  bool requestIfReady({
    required String? sessionId,
    required bool Function(String sessionId) send,
    bool force = false,
    bool replacePending = false,
  }) {
    final id = sessionId;
    if (id == null || (!force && _lastBufferedSessionId == id)) {
      return false;
    }
    if (replacePending && _pendingSessionId == id) {
      _retryTimer?.cancel();
      _retryTimer = null;
      _pendingSessionId = null;
      _retryAttempt = 0;
      _lastBufferedSessionId = '';
    }
    if (_pendingSessionId != id) {
      _retryAttempt = 0;
    }
    if (_pendingSessionId == id) {
      return false;
    }
    return _sendAndTrack(id, send);
  }

  /// Start watching an already-sent baseline request: schedules the retry
  /// timer without re-sending. If no buffer arrives within [retryDelay],
  /// `send` re-issues the request; after [maxRetries] attempts
  /// [onRetryExhausted] fires so the caller can unfreeze the session.
  void track(String sessionId, bool Function(String sessionId) send) {
    _retryTimer?.cancel();
    _pendingSessionId = sessionId;
    _retryAttempt = 0;
    _lastBufferedSessionId = sessionId;
    _scheduleRetry(sessionId, send);
  }

  void markReceived({
    required String? sessionId,
    required String? activeSessionId,
  }) {
    final id = sessionId ?? activeSessionId;
    if (id == null || _pendingSessionId != id) return;
    _retryTimer?.cancel();
    _retryTimer = null;
    _pendingSessionId = null;
    _retryAttempt = 0;
    _lastBufferedSessionId = id;
  }

  void dispose() {
    _retryTimer?.cancel();
    _retryTimer = null;
  }

  void _scheduleRetry(String sessionId, bool Function(String sessionId) send) {
    _retryTimer?.cancel();
    if (_retryAttempt >= maxRetries) {
      onRetryExhausted?.call(sessionId);
      return;
    }
    _retryTimer = _timerFactory(retryDelay, () {
      if (_pendingSessionId != sessionId) return;
      _retryAttempt += 1;
      _lastBufferedSessionId = '';
      _sendAndTrack(sessionId, send);
    });
  }

  bool _sendAndTrack(String sessionId, bool Function(String sessionId) send) {
    final sent = send(sessionId);
    if (!sent) return false;
    _lastBufferedSessionId = sessionId;
    _pendingSessionId = sessionId;
    _scheduleRetry(sessionId, send);
    return true;
  }
}
