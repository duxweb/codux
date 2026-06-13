import 'terminal_output_sequencer.dart';

class TerminalOutputResyncResult {
  const TerminalOutputResyncResult({
    required this.render,
    required this.ack,
    this.gap = false,
  });

  final bool render;
  final int? ack;

  /// True when the observed live frame left a hole in the output sequence,
  /// meaning the session needs a baseline resync to repair lost output.
  final bool gap;
}

TerminalOutputResyncResult observeTerminalOutputForResync({
  required TerminalOutputSequencer sequencer,
  required String sessionId,
  required bool isBuffer,
  required int? outputSeq,
  required int? offset,
  bool resetsSequence = false,
}) {
  final sequence = sequencer.observe(
    sessionId: sessionId,
    isBuffer: isBuffer,
    outputSeq: outputSeq,
    offset: offset,
    resetsSequence: resetsSequence,
  );
  switch (sequence.action) {
    case TerminalOutputSequenceAction.accept:
    case TerminalOutputSequenceAction.baseline:
      return TerminalOutputResyncResult(
        render: true,
        ack: outputSeq,
        gap: sequence.gap,
      );
    case TerminalOutputSequenceAction.duplicate:
      return TerminalOutputResyncResult(render: false, ack: outputSeq);
  }
}
