import '../models/remote_models.dart';
import 'log_service.dart';
import 'remote_capabilities.dart';
import 'remote_protocol.dart';
import 'remote_runtime_store.dart';
import 'remote_terminal_output_controller.dart';
import 'remote_terminal_subscription_controller.dart';

typedef RemoteTerminalSend = bool Function(RelayEnvelope envelope);
typedef RemoteTerminalLookup = TerminalInfo? Function(String sessionId);
typedef RemoteTerminalRequestIdFactory = String Function(String scope);

class RemoteTerminalBindResult {
  const RemoteTerminalBindResult({
    required this.baselineRequested,
    required this.restored,
  });

  final bool baselineRequested;
  final bool restored;
}

class RemoteTerminalBindingCoordinator {
  RemoteTerminalBindingCoordinator({
    required RemoteTerminalOutputController outputController,
    required RemoteTerminalSend send,
    required RemoteTerminalLookup terminalById,
    required RemoteTerminalRequestIdFactory nextRequestId,
    int maxCharsLimit = TerminalBufferCapability.mobileMaxChars,
  }) : _outputController = outputController,
       _send = send,
       _terminalById = terminalById,
       _nextRequestId = nextRequestId,
       _maxCharsLimit = maxCharsLimit;

  final RemoteTerminalOutputController _outputController;
  final RemoteTerminalSubscriptionController _subscriptions =
      RemoteTerminalSubscriptionController();
  final RemoteTerminalSend _send;
  final RemoteTerminalLookup _terminalById;
  final RemoteTerminalRequestIdFactory _nextRequestId;
  final int _maxCharsLimit;

  void reset() {
    _subscriptions.reset();
  }

  bool replaceProjectSubscription({
    required String projectId,
    required String reason,
    required TerminalBufferCapability capability,
    required String? activeSessionId,
    bool baseline = true,
  }) {
    final maxChars = capability.maxChars.clamp(1, _maxCharsLimit);
    final requestId = _nextRequestId('project-$projectId');
    final plan = _subscriptions.replaceProject(
      projectId,
      baseline: baseline,
      maxChars: maxChars,
      chunkChars: capability.chunking ? capability.chunkChars : null,
      requestId: requestId,
    );
    if (!plan.hasWork) return false;

    final unsubscribe = plan.unsubscribe;
    if (unsubscribe != null) {
      CoduxLog.debug(
        '[codux-flutter-terminal] unsubscribe project=${plan.unsubscribeProjectId ?? ''} reason=$reason',
      );
      _send(unsubscribe);
    }

    final subscribe = plan.subscribe;
    var baselineRequested = false;
    if (subscribe != null) {
      final currentTerminal = activeSessionId == null
          ? null
          : _terminalById(activeSessionId);
      final activeBelongsToProject =
          activeSessionId != null &&
          activeSessionId.isNotEmpty &&
          currentTerminal?.projectId == projectId;
      if (baseline && activeBelongsToProject) {
        final started = _outputController.startBufferRequest(
          activeSessionId,
          requestId,
          requireBaseline: true,
          resetAssembler: true,
        );
        if (!started) return false;
      }

      CoduxLog.debug(
        '[codux-flutter-terminal] subscribe project=${plan.subscribeProjectId ?? ''} reason=$reason',
      );
      final sent = _send(subscribe);
      if (sent) {
        final commit = _subscriptions.commitFor(plan);
        _subscriptions.markProjectSubscribed(
          commit.projectId,
          baselineRequested: commit.baseline,
        );
        baselineRequested = commit.baseline;
      } else if (baseline && activeBelongsToProject) {
        _outputController.resetSessionTransient(activeSessionId);
      }
    }
    return baselineRequested;
  }

  bool subscribeSessionBaseline({
    required String sessionId,
    required String reason,
    required TerminalBufferCapability capability,
    bool baseline = true,
    bool replaceActive = false,
  }) {
    final cleanSessionId = sessionId.trim();
    if (cleanSessionId.isEmpty) return false;
    final requestId = _nextRequestId('session-$cleanSessionId');
    final maxChars = capability.maxChars.clamp(1, _maxCharsLimit);
    final envelope = remoteResourceSubscribeEnvelope(
      resource: RemoteResourceType.terminals,
      sessionId: cleanSessionId,
      baseline: baseline,
      maxChars: maxChars,
      chunkChars: capability.chunking ? capability.chunkChars : null,
      requestId: requestId,
    );
    if (baseline) {
      final started = _outputController.startBufferRequest(
        cleanSessionId,
        requestId,
        requireBaseline: true,
        resetAssembler: true,
        replaceActive: replaceActive,
      );
      if (!started) return false;
    }
    CoduxLog.info(
      '[codux-flutter-terminal] subscribe session=$cleanSessionId reason=$reason baseline=$baseline',
    );
    final sent = _send(envelope);
    if (!sent && baseline) {
      _outputController.resetSessionTransient(cleanSessionId);
    }
    return sent && baseline;
  }

  void resubscribeVisibleTerminal({
    required bool transportConnected,
    required bool protocolReady,
    required String? activeSessionId,
    required String? selectedProjectId,
    required TerminalBufferCapability capability,
    required String reason,
    required void Function(String sessionId, bool baselineRequested)
    ensureBoundBaseline,
  }) {
    if (!transportConnected || !protocolReady) return;
    final sessionId = activeSessionId;
    if (sessionId != null && sessionId.isNotEmpty) {
      // While the cached output sequence is continuous the cache is already
      // an exact mirror, so re-subscribing with baseline:false avoids holding
      // live output for a baseline round-trip; a recorded gap means lost
      // frames that only a baseline can repair.
      final baseline =
          !_outputController.hasCachedOutput(sessionId) ||
          _outputController.hasSequenceGap(sessionId);
      final requested = subscribeSessionBaseline(
        sessionId: sessionId,
        reason: reason,
        capability: capability,
        baseline: baseline,
      );
      ensureBoundBaseline(sessionId, requested);
      return;
    }
    final projectId = selectedProjectId;
    if (projectId == null || projectId.isEmpty) return;
    _subscriptions.markProjectBaselineStale(projectId);
    replaceProjectSubscription(
      projectId: projectId,
      reason: reason,
      capability: capability,
      activeSessionId: activeSessionId,
    );
  }

  void ensureBoundTerminalHasBaseline({
    required String sessionId,
    required bool baselineRequested,
    required String reason,
    required TerminalBufferCapability capability,
  }) {
    if (baselineRequested || _outputController.hasCachedOutput(sessionId)) {
      CoduxLog.debug(
        '[codux-flutter-terminal] baseline satisfied session=$sessionId reason=$reason requested=$baselineRequested',
      );
      return;
    }
    final terminal = _terminalById(sessionId);
    if (terminal == null) return;
    final projectId = terminal.projectId;
    _subscriptions.markProjectBaselineStale(projectId);
    replaceProjectSubscription(
      projectId: projectId,
      reason: 'empty-pool-$reason',
      capability: capability,
      activeSessionId: sessionId,
    );
  }

  RemoteTerminalBindResult bindSession({
    required RemoteRuntimePlan plan,
    required String bindSessionId,
    required String reason,
    required String? selectedProjectId,
    required TerminalBufferCapability capability,
    required bool restored,
  }) {
    var baselineRequested = false;
    final hasCachedOutput =
        _outputController.hasCachedOutput(bindSessionId) &&
        !_outputController.hasSequenceGap(bindSessionId);
    final needsFullBuffer = plan.bindFullBuffer && !hasCachedOutput;
    if (selectedProjectId != null) {
      replaceProjectSubscription(
        projectId: selectedProjectId,
        reason: 'bind-$reason',
        capability: capability,
        activeSessionId: bindSessionId,
        baseline: false,
      );
    }
    if (needsFullBuffer) {
      _outputController.bindSession(bindSessionId, requireBaseline: true);
      baselineRequested = subscribeSessionBaseline(
        sessionId: bindSessionId,
        reason: 'bind-$reason',
        capability: capability,
      );
    }
    return RemoteTerminalBindResult(
      baselineRequested: baselineRequested,
      restored: restored,
    );
  }
}
