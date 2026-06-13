import 'package:codux_protocol_ffi/codux_protocol_ffi.dart'
    as codux_terminal_core;

enum TerminalOutputSequenceAction { accept, duplicate, baseline }

class TerminalOutputSequenceResult {
  const TerminalOutputSequenceResult({
    required this.action,
    required this.previousSeq,
    this.gap = false,
  });

  final TerminalOutputSequenceAction action;
  final int previousSeq;

  /// True when a live frame skipped ahead of the previously observed
  /// sequence, meaning output was lost and a baseline resync is required.
  final bool gap;

  bool get shouldRender =>
      action == TerminalOutputSequenceAction.accept ||
      action == TerminalOutputSequenceAction.baseline;
}

class TerminalOutputSequencer {
  final codux_terminal_core.TerminalOutputSequencerCore _core =
      codux_terminal_core.TerminalOutputSequencerCore();

  int sequenceFor(String sessionId) => _core.sequenceFor(sessionId);

  TerminalOutputSequenceResult observe({
    required String sessionId,
    required bool isBuffer,
    int? outputSeq,
    int? offset,
    bool resetsSequence = false,
  }) {
    final result = _core.observe(
      sessionId: sessionId,
      isBuffer: isBuffer,
      outputSeq: outputSeq,
      offset: offset,
      resetsSequence: resetsSequence,
    );
    return TerminalOutputSequenceResult(
      action: _actionFromCore(result.action),
      previousSeq: result.previousSeq,
      gap: result.gap,
    );
  }

  void remove(String sessionId) {
    _core.remove(sessionId);
  }

  void reset() {
    _core.reset();
  }

  void dispose() {
    _core.dispose();
  }
}

TerminalOutputSequenceAction _actionFromCore(String action) {
  switch (action) {
    case 'accept':
      return TerminalOutputSequenceAction.accept;
    case 'duplicate':
      return TerminalOutputSequenceAction.duplicate;
    case 'baseline':
      return TerminalOutputSequenceAction.baseline;
    default:
      throw FormatException('Unknown terminal sequence action: $action');
  }
}
