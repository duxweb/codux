import '../models/remote_models.dart';
import 'remote_capabilities.dart';
import 'remote_protocol.dart';

typedef RemoteResourceSend = bool Function(RelayEnvelope envelope);

class RemoteResourceSubscriptionCoordinator {
  RemoteResourceSubscriptionCoordinator({required RemoteResourceSend send})
    : _send = send;

  final RemoteResourceSend _send;
  RemoteResourceSubscriptionCapability _capability =
      RemoteResourceSubscriptionCapability.fallback;
  final Map<String, String> _projectIdsByResource = <String, String>{};

  void configure(RemoteResourceSubscriptionCapability capability) {
    _capability = capability;
    _projectIdsByResource.removeWhere(
      (resource, _) => !capability.supports(resource),
    );
  }

  void reset() {
    _capability = RemoteResourceSubscriptionCapability.fallback;
    _projectIdsByResource.clear();
  }

  bool requestGlobal({
    required String resource,
    required RelayEnvelope fallback,
  }) {
    return _send(globalEnvelope(resource: resource, fallback: fallback));
  }

  RelayEnvelope globalEnvelope({
    required String resource,
    required RelayEnvelope fallback,
  }) {
    return _capability.supports(resource)
        ? remoteResourceSubscribeEnvelope(resource: resource, baseline: false)
        : fallback;
  }

  bool requestProject({
    required String resource,
    required String projectId,
    required RelayEnvelope fallback,
    Map<String, Object?> extraPayload = const {},
  }) {
    final cleanProjectId = projectId.trim();
    if (!_capability.supports(resource) || cleanProjectId.isEmpty) {
      return _send(fallback);
    }
    final previousProjectId = _projectIdsByResource[resource];
    if (previousProjectId != null && previousProjectId != cleanProjectId) {
      final sent = _send(
        remoteResourceUnsubscribeEnvelope(
          resource: resource,
          projectId: previousProjectId,
        ),
      );
      if (!sent) return false;
      _projectIdsByResource.remove(resource);
    }
    final subscribe = remoteResourceSubscribeEnvelope(
      resource: resource,
      projectId: cleanProjectId,
      baseline: false,
    );
    final payload = subscribe.payload;
    if (payload is Map) {
      for (final MapEntry(:key, :value) in extraPayload.entries) {
        if (value != null) payload[key] = value;
      }
    }
    final sent = _send(subscribe);
    if (sent) {
      _projectIdsByResource[resource] = cleanProjectId;
    }
    return sent;
  }
}
