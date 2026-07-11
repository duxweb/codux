import 'package:codux_flutter/models/remote_models.dart';
import 'package:codux_flutter/services/remote_capabilities.dart';
import 'package:codux_flutter/services/remote_protocol.dart';
import 'package:codux_flutter/services/remote_resource_subscription_coordinator.dart';
import 'package:flutter_test/flutter_test.dart';

void main() {
  test('falls back when the host does not advertise subscriptions', () {
    final sent = <RelayEnvelope>[];
    final coordinator = RemoteResourceSubscriptionCoordinator(
      send: (envelope) {
        sent.add(envelope);
        return true;
      },
    );

    coordinator.requestGlobal(
      resource: RemoteResourceType.projects,
      fallback: RelayEnvelope(type: RemoteMessageType.projectList),
    );

    expect(sent.single.type, RemoteMessageType.projectList);
  });

  test('switches project subscriptions in unsubscribe-subscribe order', () {
    final sent = <RelayEnvelope>[];
    final coordinator =
        RemoteResourceSubscriptionCoordinator(
          send: (envelope) {
            sent.add(envelope);
            return true;
          },
        )..configure(
          RemoteResourceSubscriptionCapability({RemoteResourceType.gitStatus}),
        );

    coordinator.requestProject(
      resource: RemoteResourceType.gitStatus,
      projectId: 'project-a',
      fallback: RelayEnvelope(type: RemoteMessageType.gitStatus),
      extraPayload: const {'projectPath': '/a'},
    );
    coordinator.requestProject(
      resource: RemoteResourceType.gitStatus,
      projectId: 'project-b',
      fallback: RelayEnvelope(type: RemoteMessageType.gitStatus),
      extraPayload: const {'projectPath': '/b'},
    );

    expect(sent.map((message) => message.type), [
      RemoteMessageType.resourceSubscribe,
      RemoteMessageType.resourceUnsubscribe,
      RemoteMessageType.resourceSubscribe,
    ]);
    expect(sent[1].payload, containsPair('projectId', 'project-a'));
    expect(sent[2].payload, containsPair('projectId', 'project-b'));
    expect(sent[2].payload, containsPair('projectPath', '/b'));
  });

  test('does not advance project state when unsubscribe sending fails', () {
    final sent = <RelayEnvelope>[];
    var failUnsubscribe = false;
    final coordinator =
        RemoteResourceSubscriptionCoordinator(
          send: (envelope) {
            sent.add(envelope);
            return !(failUnsubscribe &&
                envelope.type == RemoteMessageType.resourceUnsubscribe);
          },
        )..configure(
          RemoteResourceSubscriptionCapability({RemoteResourceType.worktrees}),
        );

    coordinator.requestProject(
      resource: RemoteResourceType.worktrees,
      projectId: 'project-a',
      fallback: RelayEnvelope(type: RemoteMessageType.worktreeList),
    );
    failUnsubscribe = true;
    expect(
      coordinator.requestProject(
        resource: RemoteResourceType.worktrees,
        projectId: 'project-b',
        fallback: RelayEnvelope(type: RemoteMessageType.worktreeList),
      ),
      isFalse,
    );
    failUnsubscribe = false;
    coordinator.requestProject(
      resource: RemoteResourceType.worktrees,
      projectId: 'project-b',
      fallback: RelayEnvelope(type: RemoteMessageType.worktreeList),
    );

    expect(
      sent.where(
        (message) =>
            message.type == RemoteMessageType.resourceUnsubscribe &&
            message.payload is Map &&
            (message.payload as Map)['projectId'] == 'project-a',
      ),
      hasLength(2),
    );
  });
}
