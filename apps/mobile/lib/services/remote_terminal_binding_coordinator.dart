import '../models/remote_models.dart';
import 'log_service.dart';
import 'remote_capabilities.dart';
import 'remote_protocol.dart';
import 'remote_runtime_store.dart';
import 'remote_terminal_output_controller.dart';
import 'remote_terminal_subscription_controller.dart';

typedef RemoteTerminalSend = bool Function(RelayEnvelope envelope);
typedef RemoteTerminalRequestIdFactory = String Function(String scope);
typedef RemoteTerminalViewportSizeProvider =
    ({int cols, int rows})? Function(String sessionId);

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
    required RemoteTerminalRequestIdFactory nextRequestId,
    RemoteTerminalViewportSizeProvider? viewportSize,
    int maxCharsLimit = TerminalBufferCapability.mobileMaxChars,
  }) : _outputController = outputController,
       _send = send,
       _nextRequestId = nextRequestId,
       _viewportSize = viewportSize,
       _maxCharsLimit = maxCharsLimit;

  final RemoteTerminalOutputController _outputController;
  final RemoteTerminalSubscriptionController _subscriptions =
      RemoteTerminalSubscriptionController();
  final RemoteTerminalSend _send;
  final RemoteTerminalRequestIdFactory _nextRequestId;
  final RemoteTerminalViewportSizeProvider? _viewportSize;
  final int _maxCharsLimit;
  final Set<String> _baselineStaleSessionIds = <String>{};

  void reset() {
    _subscriptions.reset();
    _baselineStaleSessionIds.clear();
  }

  void markSessionBaselineStale(String sessionId) {
    final cleanSessionId = sessionId.trim();
    if (cleanSessionId.isNotEmpty) {
      _baselineStaleSessionIds.add(cleanSessionId);
    }
  }

  bool isSessionBaselineStale(String sessionId) {
    return _baselineStaleSessionIds.contains(sessionId.trim());
  }

  void clearSessionBaselineStale(String? sessionId) {
    final cleanSessionId = sessionId?.trim();
    if (cleanSessionId != null && cleanSessionId.isNotEmpty) {
      _baselineStaleSessionIds.remove(cleanSessionId);
    }
  }

  void replaceProjectSubscription({
    required String projectId,
    required String reason,
  }) {
    final plan = _subscriptions.replaceProject(projectId);
    if (!plan.hasWork) return;

    final unsubscribe = plan.unsubscribe;
    if (unsubscribe != null) {
      CoduxLog.debug(
        '[codux-flutter-terminal] unsubscribe project=${plan.unsubscribeProjectId ?? ''} reason=$reason',
      );
      if (!_send(unsubscribe)) {
        return;
      }
      _subscriptions.markProjectUnsubscribed(plan.unsubscribeProjectId ?? '');
    }

    final subscribe = plan.subscribe;
    if (subscribe != null) {
      CoduxLog.debug(
        '[codux-flutter-terminal] subscribe project=${plan.subscribeProjectId ?? ''} reason=$reason',
      );
      if (_send(subscribe)) {
        _subscriptions.markProjectSubscribed(plan.subscribeProjectId ?? '');
      }
    }
  }

  bool subscribeSession({
    required String sessionId,
    required String reason,
    required TerminalBufferCapability capability,
    bool baseline = true,
    bool replaceActive = false,
  }) {
    final cleanSessionId = sessionId.trim();
    if (cleanSessionId.isEmpty) return false;
    final requestId = baseline
        ? _nextRequestId('session-$cleanSessionId')
        : null;
    final maxChars = capability.maxChars.clamp(1, _maxCharsLimit);
    final viewportSize = baseline ? _viewportSize?.call(cleanSessionId) : null;
    final envelope = remoteResourceSubscribeEnvelope(
      resource: RemoteResourceType.terminals,
      sessionId: cleanSessionId,
      baseline: baseline,
      maxChars: baseline ? maxChars : null,
      chunkChars: baseline && capability.chunking
          ? capability.chunkChars
          : null,
      requestId: requestId,
      viewportCols: viewportSize?.cols,
      viewportRows: viewportSize?.rows,
    );
    if (baseline) {
      final started = _outputController.startBufferRequest(
        cleanSessionId,
        requestId!,
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
    if (sent) {
      _subscriptions.markSessionSubscribed(cleanSessionId);
    } else if (baseline) {
      _outputController.resetSessionTransient(cleanSessionId);
    }
    return sent;
  }

  void unsubscribeSession(String sessionId, {required String reason}) {
    final cleanSessionId = sessionId.trim();
    if (cleanSessionId.isEmpty ||
        !_subscriptions.isSessionSubscribed(cleanSessionId)) {
      return;
    }
    CoduxLog.debug(
      '[codux-flutter-terminal] unsubscribe session=$cleanSessionId reason=$reason',
    );
    final sent = _send(
      remoteResourceUnsubscribeEnvelope(
        resource: RemoteResourceType.terminals,
        sessionId: cleanSessionId,
      ),
    );
    if (sent) {
      _subscriptions.removeSession(cleanSessionId);
      _baselineStaleSessionIds.remove(cleanSessionId);
    }
  }

  bool resubscribeVisibleTerminal({
    required bool transportConnected,
    required bool protocolReady,
    required String? activeSessionId,
    required String? selectedProjectId,
    required TerminalBufferCapability capability,
    required String reason,
  }) {
    if (!transportConnected || !protocolReady) return false;
    final projectId = selectedProjectId?.trim();
    if (projectId != null && projectId.isNotEmpty) {
      replaceProjectSubscription(projectId: projectId, reason: reason);
    }
    final sessionId = activeSessionId?.trim();
    if (sessionId == null || sessionId.isEmpty) return false;
    return subscribeSession(
      sessionId: sessionId,
      reason: reason,
      capability: capability,
      baseline: true,
      replaceActive: true,
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
    final projectId = selectedProjectId?.trim();
    if (projectId != null && projectId.isNotEmpty) {
      replaceProjectSubscription(projectId: projectId, reason: 'bind-$reason');
    }
    final hasUsableCache =
        _outputController.hasCachedOutput(bindSessionId) &&
        !_outputController.hasSequenceGap(bindSessionId) &&
        !_baselineStaleSessionIds.contains(bindSessionId);
    final needsBaseline = !hasUsableCache || plan.bindFullBuffer;
    if (needsBaseline) {
      _outputController.bindSession(bindSessionId, requireBaseline: true);
    }
    final sent = subscribeSession(
      sessionId: bindSessionId,
      reason: 'bind-$reason',
      capability: capability,
      baseline: needsBaseline,
    );
    final baselineRequested = needsBaseline && sent;
    if (baselineRequested) {
      _baselineStaleSessionIds.remove(bindSessionId);
    }
    return RemoteTerminalBindResult(
      baselineRequested: baselineRequested,
      restored: restored,
    );
  }
}
