import '../models/remote_models.dart';
import 'remote_protocol.dart';

class RemoteTerminalProjectSubscriptionPlan {
  const RemoteTerminalProjectSubscriptionPlan({
    this.unsubscribe,
    this.unsubscribeProjectId,
    this.subscribe,
    this.subscribeProjectId,
  });

  final RelayEnvelope? unsubscribe;
  final String? unsubscribeProjectId;
  final RelayEnvelope? subscribe;
  final String? subscribeProjectId;

  bool get hasWork => unsubscribe != null || subscribe != null;
}

class RemoteTerminalSubscriptionController {
  String? _projectId;
  final Set<String> _sessionIds = <String>{};

  String? get projectId => _projectId;
  int get sessionCount => _sessionIds.length;

  void reset() {
    _projectId = null;
    _sessionIds.clear();
  }

  RemoteTerminalProjectSubscriptionPlan replaceProject(String projectId) {
    final nextProjectId = projectId.trim();
    if (nextProjectId.isEmpty || nextProjectId == _projectId) {
      return const RemoteTerminalProjectSubscriptionPlan();
    }
    final previousProjectId = _projectId;
    return RemoteTerminalProjectSubscriptionPlan(
      unsubscribe: previousProjectId == null
          ? null
          : remoteResourceUnsubscribeEnvelope(
              resource: RemoteResourceType.terminals,
              projectId: previousProjectId,
            ),
      unsubscribeProjectId: previousProjectId,
      subscribe: remoteResourceSubscribeEnvelope(
        resource: RemoteResourceType.terminals,
        projectId: nextProjectId,
        baseline: false,
      ),
      subscribeProjectId: nextProjectId,
    );
  }

  void markProjectUnsubscribed(String projectId) {
    if (_projectId == projectId.trim()) {
      _projectId = null;
    }
  }

  void markProjectSubscribed(String projectId) {
    final cleanProjectId = projectId.trim();
    if (cleanProjectId.isNotEmpty) {
      _projectId = cleanProjectId;
    }
  }

  void markSessionSubscribed(String sessionId) {
    final cleanSessionId = sessionId.trim();
    if (cleanSessionId.isNotEmpty) {
      _sessionIds.add(cleanSessionId);
    }
  }

  bool removeSession(String sessionId) {
    return _sessionIds.remove(sessionId.trim());
  }

  bool isSessionSubscribed(String sessionId) {
    return _sessionIds.contains(sessionId.trim());
  }
}
