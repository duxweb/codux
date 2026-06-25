import '../models/remote_models.dart';
import 'log_service.dart';

class TerminalViewportResize {
  const TerminalViewportResize({required this.cols, required this.rows});

  final int cols;
  final int rows;
}

class _ViewportSize {
  const _ViewportSize(this.cols, this.rows);

  final int cols;
  final int rows;

  bool matches(int cols, int rows) => this.cols == cols && this.rows == rows;
}

class TerminalViewportController {
  int? _lastCols;
  int? _lastRows;
  int? _pendingCols;
  int? _pendingRows;
  final Map<String, _ViewportSize> _sentBySession = {};
  final Map<String, int> _generationBySession = {};
  String? _owner;
  int _generation = 0;

  String? get owner => _owner;
  int get generation => _generation;
  int? get pendingCols => _pendingCols;
  int? get pendingRows => _pendingRows;

  /// The host's authoritative grid size for [sessionId] as last reported in a
  /// viewport-state message, or null if none seen yet. The host keeps its own
  /// (often taller) row count for remote viewers, so this can differ from the
  /// phone's measured viewport; the local cell screen is sized to this.
  ({int cols, int rows})? reportedSize(String sessionId) {
    final size = _sentBySession[sessionId.trim()];
    if (size == null) return null;
    return (cols: size.cols, rows: size.rows);
  }

  void resetSizes() {
    _lastCols = null;
    _lastRows = null;
    _pendingCols = null;
    _pendingRows = null;
    _sentBySession.clear();
  }

  /// Record the phone's freshly measured grid even when there is no session to
  /// resize yet (the terminal pane is laid out and measured BEFORE the first
  /// `terminal.create` is issued). Caching it here lets `_createTerminal` spawn
  /// the host PTY at the phone's width up front, so the shell draws its prompt
  /// once at the final size instead of drawing at the host's default 100 cols
  /// and then redrawing on the first viewport.resize -- which left a duplicate
  /// (ghost) first prompt line on connect.
  void recordMeasured(int cols, int rows) {
    if (cols <= 0 || rows <= 0) return;
    _pendingCols = cols;
    _pendingRows = rows;
  }

  bool applyRemoteState(RelayEnvelope message) {
    final payload = message.payload;
    if (payload is! Map) return false;
    final sessionId = message.sessionId?.trim() ?? '';
    final nextGeneration = _intValue(payload['generation']) ?? 0;
    final currentGeneration = sessionId.isEmpty
        ? _generation
        : (_generationBySession[sessionId] ?? 0);
    if (nextGeneration < currentGeneration) return false;
    _generation = nextGeneration;
    if (sessionId.isNotEmpty) {
      _generationBySession[sessionId] = nextGeneration;
    }
    _owner = payload['owner']?.toString();
    final cols = _intValue(payload['cols']);
    final rows = _intValue(payload['rows']);
    if (sessionId.isNotEmpty &&
        cols != null &&
        rows != null &&
        cols > 0 &&
        rows > 0) {
      _sentBySession[sessionId] = _ViewportSize(cols, rows);
    }
    CoduxLog.debug(
      '[codux-flutter-terminal] viewport owner=${_owner ?? ''} size=${cols ?? 0}x${rows ?? 0} generation=$_generation session=${message.sessionId ?? ''}',
    );
    return true;
  }

  // resize/flushPending only PROPOSE an envelope; the dedup cache is
  // committed via markSent after the caller actually sends it. Committing
  // up front poisoned the cache when a send gate (terminal list not loaded
  // yet) dropped the envelope, permanently suppressing the resize.
  TerminalViewportResize? resize({
    required String sessionId,
    required int cols,
    required int rows,
    required bool keyboardVisible,
  }) {
    final id = sessionId.trim();
    if (id.isEmpty || cols <= 0 || rows <= 0) return null;
    _pendingCols = cols;
    _pendingRows = rows;
    final lastSessionSize = _sentBySession[id];
    final nextRows = keyboardVisible
        ? (lastSessionSize?.rows ?? _lastRows ?? rows)
        : rows;
    if (lastSessionSize?.matches(cols, nextRows) == true) return null;
    return TerminalViewportResize(cols: cols, rows: nextRows);
  }

  TerminalViewportResize? flushPending({
    required String sessionId,
    required bool force,
  }) {
    final id = sessionId.trim();
    if (id.isEmpty) return null;
    final cols = _pendingCols;
    final rows = _pendingRows;
    if (cols == null || rows == null || cols <= 0 || rows <= 0) return null;
    final lastSessionSize = _sentBySession[id];
    if (lastSessionSize?.matches(cols, rows) == true) return null;
    if (!force) {
      if (_lastCols == cols && _lastRows == rows) return null;
    }
    return TerminalViewportResize(cols: cols, rows: rows);
  }

  void markSent(String sessionId, TerminalViewportResize resize) {
    final id = sessionId.trim();
    if (id.isEmpty) return;
    _lastCols = resize.cols;
    _lastRows = resize.rows;
    _sentBySession[id] = _ViewportSize(resize.cols, resize.rows);
  }
}

int? _intValue(Object? value) {
  if (value is int) return value;
  if (value is num) return value.toInt();
  return int.tryParse('${value ?? ''}');
}
