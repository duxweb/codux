import 'dart:async';

typedef TerminalBufferTimerFactory =
    Timer Function(Duration delay, void Function() callback);
typedef TerminalBufferRequestPending = bool Function(String sessionId);

class TerminalBufferRetryCoordinator {
  TerminalBufferRetryCoordinator({
    this.retryDelay = const Duration(milliseconds: 900),
    this.maxRetries = 3,
    this.stallTolerance = 1,
    this.onRetryExhausted,
    TerminalBufferTimerFactory? timerFactory,
  }) : _timerFactory = timerFactory ?? Timer.new;

  final Duration retryDelay;
  final int maxRetries;

  /// How many quiet retry ticks (no [noteProgress]) to tolerate before a still-
  /// "pending" request is treated as stalled and re-issued. A slow but live
  /// chunked transfer keeps calling [noteProgress] as chunks arrive, so it is
  /// never wiped mid-flight; only a transfer whose chunks actually stopped
  /// arriving (a dropped chunk under high latency) is re-requested. This is the
  /// auto-recovery that the old `hasPendingRequest`-only gate could never do:
  /// the host leaves the active-request flag set for an incomplete assembly, so
  /// the timer used to reschedule forever without ever re-sending.
  final int stallTolerance;
  final void Function(String sessionId)? onRetryExhausted;
  final TerminalBufferTimerFactory _timerFactory;

  Timer? _retryTimer;
  String _lastBufferedSessionId = '';
  String? _pendingSessionId;
  int _retryAttempt = 0;

  /// Monotonic counter bumped by [noteProgress] whenever a baseline chunk lands
  /// for the pending session. The retry timer snapshots it per tick to tell an
  /// advancing transfer from a stalled one.
  int _progressGen = 0;
  int _stallTicks = 0;

  String get lastBufferedSessionId => _lastBufferedSessionId;
  String? get pendingSessionId => _pendingSessionId;
  int get retryAttempt => _retryAttempt;

  /// Report that the pending baseline transfer advanced (a chunk arrived). Keeps
  /// a slow high-latency transfer from being mistaken for a stalled one.
  void noteProgress(String? sessionId) {
    if (sessionId == null || sessionId.isEmpty) return;
    if (_pendingSessionId != sessionId) return;
    _progressGen += 1;
    _stallTicks = 0;
  }

  void resetLastBuffered() {
    _lastBufferedSessionId = '';
  }

  void reset() {
    _retryTimer?.cancel();
    _retryTimer = null;
    _lastBufferedSessionId = '';
    _pendingSessionId = null;
    _retryAttempt = 0;
    _stallTicks = 0;
  }

  void resetSession(String sessionId) {
    if (_pendingSessionId == sessionId) {
      _retryTimer?.cancel();
      _retryTimer = null;
      _pendingSessionId = null;
      _retryAttempt = 0;
      _stallTicks = 0;
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
    _stallTicks = 0;
    _lastBufferedSessionId = sessionId;
    _scheduleRetry(sessionId, send);
  }

  void trackWhilePending(
    String sessionId, {
    required bool Function(String sessionId) send,
    required TerminalBufferRequestPending hasPendingRequest,
  }) {
    _retryTimer?.cancel();
    _pendingSessionId = sessionId;
    _retryAttempt = 0;
    _stallTicks = 0;
    _lastBufferedSessionId = sessionId;
    _scheduleRetry(sessionId, send, hasPendingRequest: hasPendingRequest);
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
    _stallTicks = 0;
    _lastBufferedSessionId = id;
  }

  void dispose() {
    _retryTimer?.cancel();
    _retryTimer = null;
  }

  void _scheduleRetry(
    String sessionId,
    bool Function(String sessionId) send, {
    TerminalBufferRequestPending? hasPendingRequest,
  }) {
    _retryTimer?.cancel();
    if (_retryAttempt >= maxRetries) {
      onRetryExhausted?.call(sessionId);
      return;
    }
    final progressAtSchedule = _progressGen;
    _retryTimer = _timerFactory(retryDelay, () {
      if (_pendingSessionId != sessionId) return;
      final progressed = _progressGen != progressAtSchedule;
      final stillActive = hasPendingRequest?.call(sessionId) ?? false;
      if (progressed) {
        // The transfer advanced since the last tick (a chunk landed): keep
        // waiting, burn no attempt -- even a very slow link makes progress.
        _stallTicks = 0;
        _scheduleRetry(sessionId, send, hasPendingRequest: hasPendingRequest);
        return;
      }
      if (stillActive && _stallTicks < stallTolerance) {
        // The host still has the request open but this tick was quiet. Give a
        // few grace ticks before declaring a stall so a high-latency transfer
        // with gaps between chunks is not wiped and restarted.
        _stallTicks += 1;
        _scheduleRetry(sessionId, send, hasPendingRequest: hasPendingRequest);
        return;
      }
      // Stalled (no progress past the tolerance), or the request was dropped
      // without ever delivering a baseline: re-issue a fresh one.
      _stallTicks = 0;
      _retryAttempt += 1;
      _lastBufferedSessionId = '';
      _sendAndTrack(sessionId, send, hasPendingRequest: hasPendingRequest);
    });
  }

  bool _sendAndTrack(
    String sessionId,
    bool Function(String sessionId) send, {
    TerminalBufferRequestPending? hasPendingRequest,
  }) {
    final sent = send(sessionId);
    if (!sent) return false;
    _lastBufferedSessionId = sessionId;
    _pendingSessionId = sessionId;
    _scheduleRetry(sessionId, send, hasPendingRequest: hasPendingRequest);
    return true;
  }
}
